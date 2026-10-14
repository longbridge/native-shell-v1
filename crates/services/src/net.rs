//! Background network work: `fetch` requests and WebSocket connections.
//!
//! Each request and each socket runs on its own worker thread with blocking
//! I/O; results go into [`crate::Shared`]'s queue and the JS thread picks them
//! up in `Services::poll`. Nothing here touches a JS value.

use std::io::Read as _;
use std::net::{TcpStream, ToSocketAddrs as _};
use std::sync::mpsc::{Receiver, TryRecvError};
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use tungstenite::protocol::frame::coding::CloseCode;
use tungstenite::protocol::CloseFrame;
use tungstenite::{Message, WebSocket};
use url::Url;

use crate::policy::{check_network, same_origin};
use crate::{Completion, Shared, WsEvent};

/// One TLS configuration with the `ring` provider and the webpki roots, shared
/// by HTTP and WebSocket. Built explicitly rather than through rustls's process
/// default so the provider never depends on which features a host app links.
pub fn tls_config() -> Arc<rustls::ClientConfig> {
    static CONFIG: OnceLock<Arc<rustls::ClientConfig>> = OnceLock::new();
    CONFIG
        .get_or_init(|| {
            let roots = rustls::RootCertStore { roots: webpki_roots::TLS_SERVER_ROOTS.to_vec() };
            let config = rustls::ClientConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider()))
                .with_safe_default_protocol_versions()
                .map(|b| b.with_root_certificates(roots).with_no_client_auth())
                .unwrap_or_else(|_| unreachable!("ring supports the default protocol versions"));
            Arc::new(config)
        })
        .clone()
}

pub struct FetchRequest {
    pub url: Url,
    pub method: String,
    pub headers: Vec<(String, String)>,
    pub body: Option<Vec<u8>>,
}

pub struct FetchResponse {
    pub status: u16,
    pub status_text: String,
    pub url: String,
    pub redirected: bool,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

pub fn spawn_fetch(shared: Arc<Shared>, id: u64, req: FetchRequest) {
    std::thread::spawn(move || {
        let result = fetch(&shared, req);
        shared.complete(Completion::Fetch { id, result });
    });
}

fn fetch(shared: &Shared, req: FetchRequest) -> Result<FetchResponse, String> {
    let limits = &shared.config.limits;
    let agent = ureq::AgentBuilder::new()
        .redirects(0)
        .timeout(limits.fetch_timeout)
        .tls_config(tls_config())
        .build();
    let FetchRequest { mut url, mut method, mut headers, mut body } = req;
    for hop in 0..=limits.fetch_redirects {
        let mut request = agent.request_url(&method, &url);
        for (name, value) in &headers {
            request = request.set(name, value);
        }
        let sent = match &body {
            Some(body) => request.send_bytes(body),
            None => request.call(),
        };
        let response = match sent {
            Ok(r) | Err(ureq::Error::Status(_, r)) => r,
            Err(ureq::Error::Transport(e)) => return Err(format!("fetch {url} failed: {e}")),
        };
        let status = response.status();
        if matches!(status, 301 | 302 | 303 | 307 | 308) {
            if let Some(location) = response.header("location") {
                if hop == limits.fetch_redirects {
                    break;
                }
                let next = url.join(location).map_err(|e| format!("fetch {url}: bad redirect `{location}`: {e}"))?;
                if (matches!(status, 301 | 302) && method == "POST") || (status == 303 && method != "HEAD") {
                    method = "GET".into();
                    body = None;
                    headers.retain(|(n, _)| !n.eq_ignore_ascii_case("content-type"));
                }
                check_network(&shared.config.capabilities, "fetch redirect to", &method, &next)?;
                if url.scheme() == "https" && next.scheme() != "https" {
                    return Err(format!("fetch {url}: redirect to {next} refused (HTTPS downgrade)"));
                }
                if !same_origin(&url, &next) {
                    if body.is_some() {
                        return Err(format!("fetch {url}: redirect to {next} refused (would replay a {method} body across origins)"));
                    }
                    // Credentials belong to the origin that was asked for.
                    headers.retain(|(n, _)| !n.eq_ignore_ascii_case("authorization") && !n.eq_ignore_ascii_case("cookie"));
                }
                url = next;
                continue;
            }
        }
        let mut out = Vec::new();
        let mut names = response.headers_names();
        names.sort_unstable_by_key(|n| n.to_ascii_lowercase());
        names.dedup_by(|a, b| a.eq_ignore_ascii_case(b));
        for name in names {
            for value in response.all(&name) {
                out.push((name.clone(), value.to_string()));
            }
        }
        let status_text = response.status_text().to_string();
        let final_url = response.get_url().to_string();
        let mut bytes = Vec::new();
        response
            .into_reader()
            .take(limits.fetch_body_bytes as u64 + 1)
            .read_to_end(&mut bytes)
            .map_err(|e| format!("fetch {url}: reading the body failed: {e}"))?;
        if bytes.len() > limits.fetch_body_bytes {
            return Err(format!("fetch {url}: response body over the {}-byte limit", limits.fetch_body_bytes));
        }
        return Ok(FetchResponse { status, status_text, url: final_url, redirected: hop > 0, headers: out, body: bytes });
    }
    Err(format!("fetch {url}: more than {} redirects", limits.fetch_redirects))
}

pub enum WsCommand {
    Send(Message),
    Close(u16, String),
}

/// How long a socket worker blocks in `read` before it checks for commands.
const POLL_INTERVAL: Duration = Duration::from_millis(15);
/// How long a peer has to answer our close frame.
const CLOSE_GRACE: Duration = Duration::from_secs(5);

pub fn spawn_socket(shared: Arc<Shared>, id: u64, url: Url, protocols: Vec<String>, commands: Receiver<WsCommand>) {
    std::thread::spawn(move || {
        let emit = |event| shared.complete(Completion::Ws { id, event });
        match connect(&shared, &url, &protocols) {
            Ok((socket, protocol)) => {
                emit(WsEvent::Open(protocol));
                run_socket(socket, commands, &emit);
            }
            Err(message) => {
                emit(WsEvent::Error(message));
                emit(WsEvent::Close { code: 1006, reason: String::new(), clean: false });
            }
        }
    });
}

type Socket = WebSocket<tungstenite::stream::MaybeTlsStream<TcpStream>>;

fn connect(shared: &Shared, url: &Url, protocols: &[String]) -> Result<(Socket, String), String> {
    use tungstenite::client::IntoClientRequest as _;
    let timeout = shared.config.limits.fetch_timeout;
    let host = url.host_str().ok_or_else(|| format!("WebSocket {url}: no host"))?;
    let port = url.port_or_known_default().unwrap_or(80);
    let addr = (host, port)
        .to_socket_addrs()
        .map_err(|e| format!("WebSocket {url}: {e}"))?
        .next()
        .ok_or_else(|| format!("WebSocket {url}: {host} has no address"))?;
    let stream = TcpStream::connect_timeout(&addr, timeout).map_err(|e| format!("WebSocket {url}: {e}"))?;
    let _ = stream.set_nodelay(true);
    stream.set_read_timeout(Some(timeout)).map_err(|e| format!("WebSocket {url}: {e}"))?;
    let control = stream.try_clone().map_err(|e| format!("WebSocket {url}: {e}"))?;
    let mut request = url.as_str().into_client_request().map_err(|e| format!("WebSocket {url}: {e}"))?;
    if !protocols.is_empty() {
        let value = protocols.join(", ").parse().map_err(|_| format!("WebSocket {url}: invalid protocol list"))?;
        request.headers_mut().insert("Sec-WebSocket-Protocol", value);
    }
    let config = tungstenite::protocol::WebSocketConfig::default()
        .max_message_size(Some(shared.config.limits.fetch_body_bytes))
        .max_frame_size(Some(shared.config.limits.fetch_body_bytes));
    let (socket, response) = tungstenite::client_tls_with_config(
        request,
        stream,
        Some(config),
        Some(tungstenite::Connector::Rustls(tls_config())),
    )
    .map_err(|e| format!("WebSocket {url}: {e}"))?;
    // From here on reads time out quickly so the worker can serve `send`/`close`.
    control.set_read_timeout(Some(POLL_INTERVAL)).map_err(|e| format!("WebSocket {url}: {e}"))?;
    let protocol = response
        .headers()
        .get("sec-websocket-protocol")
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default()
        .to_string();
    Ok((socket, protocol))
}

fn run_socket(mut socket: Socket, commands: Receiver<WsCommand>, emit: &dyn Fn(WsEvent)) {
    // When we sent a close frame; a peer that never answers is given up on.
    let mut closing: Option<std::time::Instant> = None;
    let mut close_frame: Option<(u16, String)> = None;
    loop {
        loop {
            let command = match commands.try_recv() {
                Ok(c) => c,
                Err(TryRecvError::Empty) => break,
                // The `Services` is gone: leave politely.
                Err(TryRecvError::Disconnected) if closing.is_none() => WsCommand::Close(1001, String::new()),
                Err(TryRecvError::Disconnected) => break,
            };
            let result = match command {
                WsCommand::Send(message) => socket.send(message),
                WsCommand::Close(code, reason) => {
                    if closing.is_some() {
                        continue;
                    }
                    closing = Some(std::time::Instant::now());
                    socket.close(Some(CloseFrame { code: CloseCode::from(code), reason: reason.into() }))
                }
            };
            if let Err(e) = result {
                if !would_block(&e) {
                    return fail(emit, e);
                }
            }
        }
        match socket.read() {
            Ok(Message::Text(text)) => emit(WsEvent::Text(text.to_string())),
            Ok(Message::Binary(bytes)) => emit(WsEvent::Binary(bytes.to_vec())),
            Ok(Message::Close(frame)) => {
                close_frame = Some(frame.map_or((1005, String::new()), |f| (u16::from(f.code), f.reason.to_string())));
            }
            Ok(_) => {}
            Err(tungstenite::Error::ConnectionClosed) | Err(tungstenite::Error::AlreadyClosed) => {
                let (code, reason) = close_frame.unwrap_or((1005, String::new()));
                return emit(WsEvent::Close { code, reason, clean: true });
            }
            Err(e) if would_block(&e) => {
                if closing.is_some_and(|t| t.elapsed() > CLOSE_GRACE) {
                    let (code, reason) = close_frame.unwrap_or((1006, String::new()));
                    return emit(WsEvent::Close { code, reason, clean: false });
                }
                // Pongs to pings are queued by tungstenite and go out on flush.
                if let Err(e) = socket.flush() {
                    if !would_block(&e) {
                        return fail(emit, e);
                    }
                }
            }
            Err(e) => return fail(emit, e),
        }
    }
}

fn would_block(e: &tungstenite::Error) -> bool {
    matches!(e, tungstenite::Error::Io(io) if matches!(io.kind(), std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut))
}

fn fail(emit: &dyn Fn(WsEvent), e: tungstenite::Error) {
    emit(WsEvent::Error(e.to_string()));
    emit(WsEvent::Close { code: 1006, reason: String::new(), clean: false });
}
