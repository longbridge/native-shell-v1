//! The native half of the script API: the private object `n` the prelude is
//! evaluated with, and delivery of completions into the prelude.

use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::atomic::Ordering;
use std::sync::mpsc;
use std::sync::Arc;

use base64::Engine as _;
use rquickjs::{ArrayBuffer, Ctx, Exception, Function, JsLifetime, Object, TypedArray, Value};
use sha2::Digest as _;
use url::Url;

use crate::net::{self, FetchRequest, WsCommand};
use crate::policy::{self, Grant};
use crate::storage::{lock, Store};
use crate::{next_id, Completion, Shared, WsEvent};

const PRELUDE: &str = include_str!("prelude.js");

/// Each installed `Services`' `deliver` function, kept in the runtime's
/// userdata so the runtime drops it before it frees itself.
struct Deliverers<'js>(RefCell<HashMap<u64, Function<'js>>>);

// SAFETY: the only lifetime is the `'js` of the functions inside.
unsafe impl<'js> JsLifetime<'js> for Deliverers<'js> {
    type Changed<'to> = Deliverers<'to>;
}

type R<T> = rquickjs::Result<T>;

pub fn install<'js>(ctx: &Ctx<'js>, shared: &Arc<Shared>, services_id: u64) -> R<()> {
    let n = Object::new(ctx.clone())?;
    encoding(ctx, &n)?;
    url_natives(ctx, &n)?;
    crypto(ctx, &n)?;
    storage(ctx, &n, shared)?;
    fs(ctx, &n, shared)?;
    fetch(ctx, &n, shared)?;
    websocket(ctx, &n, shared)?;

    let prelude: Function = ctx.eval(PRELUDE)?;
    let deliver: Function = prelude.call((n, ctx.globals()))?;
    if let Some(existing) = ctx.userdata::<Deliverers>() {
        existing.0.borrow_mut().insert(services_id, deliver);
        return Ok(());
    }
    let map = HashMap::from([(services_id, deliver)]);
    ctx.store_userdata(Deliverers(RefCell::new(map)))
        .map_err(|_| Exception::throw_internal(ctx, "native-shell-services: runtime userdata is busy"))?;
    Ok(())
}

pub fn poll<'js>(ctx: &Ctx<'js>, shared: &Shared, services_id: u64) -> usize {
    let batch: Vec<Completion> = lock(&shared.queue).drain(..).collect();
    if batch.is_empty() {
        return 0;
    }
    let deliver = ctx.userdata::<Deliverers>().and_then(|d| d.0.borrow().get(&services_id).cloned());
    let Some(deliver) = deliver else { return 0 };
    let mut delivered = 0;
    for completion in batch {
        let result = match completion {
            Completion::Fetch { id, result } => {
                shared.in_flight.fetch_sub(1, Ordering::AcqRel);
                fetch_args(ctx, result).and_then(|(ok, value)| deliver.call::<_, ()>(("fetch", id as f64, ok, value)))
            }
            Completion::Ws { id, event } => {
                if matches!(event, WsEvent::Close { .. }) {
                    lock(&shared.sockets).remove(&id);
                }
                ws_args(ctx, event).and_then(|(kind, value)| deliver.call::<_, ()>(("ws", id as f64, kind, value)))
            }
        };
        // The prelude catches handler errors itself; anything left is a bug
        // there, and must not stay pending on the context.
        if let Err(rquickjs::Error::Exception) = result {
            let _ = ctx.catch();
        }
        delivered += 1;
    }
    delivered
}

fn fetch_args<'js>(ctx: &Ctx<'js>, result: Result<net::FetchResponse, String>) -> R<(bool, Value<'js>)> {
    match result {
        Ok(r) => {
            let o = Object::new(ctx.clone())?;
            o.set("status", r.status)?;
            o.set("statusText", r.status_text)?;
            o.set("url", r.url)?;
            o.set("redirected", r.redirected)?;
            let headers: Vec<Vec<String>> = r.headers.into_iter().map(|(k, v)| vec![k, v]).collect();
            o.set("headers", headers)?;
            o.set("body", TypedArray::<u8>::new(ctx.clone(), r.body)?)?;
            Ok((true, o.into_value()))
        }
        Err(message) => Ok((false, rquickjs::String::from_str(ctx.clone(), &message)?.into_value())),
    }
}

fn ws_args<'js>(ctx: &Ctx<'js>, event: WsEvent) -> R<(&'static str, Value<'js>)> {
    let string = |s: &str| rquickjs::String::from_str(ctx.clone(), s).map(|s| s.into_value());
    Ok(match event {
        WsEvent::Open(protocol) => ("open", string(&protocol)?),
        WsEvent::Text(text) => ("message", string(&text)?),
        WsEvent::Binary(bytes) => ("message", ArrayBuffer::new(ctx.clone(), bytes)?.into_value()),
        WsEvent::Error(message) => ("error", string(&message)?),
        WsEvent::Close { code, reason, clean } => {
            let o = Object::new(ctx.clone())?;
            o.set("code", code)?;
            o.set("reason", reason)?;
            o.set("clean", clean)?;
            ("close", o.into_value())
        }
    })
}

fn bytes<'js>(ctx: &Ctx<'js>, value: Value<'js>, what: &str) -> R<Vec<u8>> {
    let array = TypedArray::<u8>::from_value(value).map_err(|_| Exception::throw_type(ctx, &format!("{what}: expected bytes")))?;
    let bytes = array.as_bytes().ok_or_else(|| Exception::throw_type(ctx, &format!("{what}: detached buffer")))?;
    Ok(bytes.to_vec())
}

macro_rules! native {
    ($ctx:expr, $obj:expr, $name:literal, $f:expr) => {
        $obj.set($name, Function::new($ctx.clone(), $f)?.with_name($name)?)?
    };
}

fn encoding<'js>(ctx: &Ctx<'js>, n: &Object<'js>) -> R<()> {
    native!(ctx, n, "utf8_encode", |ctx: Ctx<'js>, s: String| TypedArray::<u8>::new(ctx, s.into_bytes()));
    native!(ctx, n, "utf8_decode", |ctx: Ctx<'js>, data: Value<'js>, fatal: bool, ignore_bom: bool| -> R<Option<String>> {
        let data = bytes(&ctx, data, "TextDecoder.decode")?;
        let mut data = data.as_slice();
        if !ignore_bom {
            data = data.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(data);
        }
        Ok(if fatal {
            std::str::from_utf8(data).ok().map(str::to_owned)
        } else {
            Some(String::from_utf8_lossy(data).into_owned())
        })
    });
    native!(ctx, n, "btoa", |s: String| -> Option<String> {
        let latin1: Option<Vec<u8>> = s.chars().map(|c| u8::try_from(u32::from(c)).ok()).collect();
        latin1.map(|b| base64::engine::general_purpose::STANDARD.encode(b))
    });
    native!(ctx, n, "atob", |s: String| -> Option<String> { atob(&s).map(|b| b.into_iter().map(char::from).collect()) });
    Ok(())
}

/// WHATWG "forgiving-base64 decode".
fn atob(s: &str) -> Option<Vec<u8>> {
    use base64::engine::{general_purpose::GeneralPurposeConfig, DecodePaddingMode, GeneralPurpose};
    let mut data: String = s.chars().filter(|c| !matches!(c, ' ' | '\t' | '\n' | '\x0C' | '\r')).collect();
    if data.len().is_multiple_of(4) {
        let trimmed = data.trim_end_matches('=').len();
        if data.len() - trimmed > 2 {
            return None;
        }
        data.truncate(trimmed);
    }
    if data.len() % 4 == 1 || !data.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'+' || b == b'/') {
        return None;
    }
    let engine = GeneralPurpose::new(
        &base64::alphabet::STANDARD,
        GeneralPurposeConfig::new()
            .with_decode_padding_mode(DecodePaddingMode::RequireNone)
            .with_decode_allow_trailing_bits(true),
    );
    engine.decode(data).ok()
}

fn url_object<'js>(ctx: &Ctx<'js>, url: &Url) -> R<Object<'js>> {
    use url::quirks as q;
    let o = Object::new(ctx.clone())?;
    o.set("href", q::href(url))?;
    o.set("origin", q::origin(url))?;
    o.set("protocol", q::protocol(url))?;
    o.set("username", q::username(url))?;
    o.set("password", q::password(url))?;
    o.set("host", q::host(url))?;
    o.set("hostname", q::hostname(url))?;
    o.set("port", q::port(url))?;
    o.set("pathname", q::pathname(url))?;
    o.set("search", q::search(url))?;
    o.set("hash", q::hash(url))?;
    Ok(o)
}

fn url_natives<'js>(ctx: &Ctx<'js>, n: &Object<'js>) -> R<()> {
    native!(ctx, n, "url_parse", |ctx: Ctx<'js>, input: String, base: Option<String>| -> R<Option<Object<'js>>> {
        let parsed = match base {
            Some(base) => Url::parse(&base).and_then(|b| b.join(&input)),
            None => Url::parse(&input),
        };
        parsed.ok().map(|u| url_object(&ctx, &u)).transpose()
    });
    native!(ctx, n, "url_set", |ctx: Ctx<'js>, href: String, prop: String, value: String| -> R<Object<'js>> {
        use url::quirks as q;
        let mut url = Url::parse(&href).map_err(|e| Exception::throw_type(&ctx, &format!("Invalid URL: {e}")))?;
        // Setters that cannot apply leave the URL as it was, as in browsers.
        match prop.as_str() {
            "protocol" => drop(q::set_protocol(&mut url, &value)),
            "username" => drop(q::set_username(&mut url, &value)),
            "password" => drop(q::set_password(&mut url, &value)),
            "host" => drop(q::set_host(&mut url, &value)),
            "hostname" => drop(q::set_hostname(&mut url, &value)),
            "port" => drop(q::set_port(&mut url, &value)),
            "pathname" => q::set_pathname(&mut url, &value),
            "search" => q::set_search(&mut url, &value),
            "hash" => q::set_hash(&mut url, &value),
            _ => {}
        }
        url_object(&ctx, &url)
    });
    native!(ctx, n, "form_parse", |s: String| -> Vec<Vec<String>> {
        url::form_urlencoded::parse(s.as_bytes()).map(|(k, v)| vec![k.into_owned(), v.into_owned()]).collect()
    });
    native!(ctx, n, "form_serialize", |pairs: Vec<Vec<String>>| -> String {
        let mut out = url::form_urlencoded::Serializer::new(String::new());
        for pair in &pairs {
            if let [k, v] = pair.as_slice() {
                out.append_pair(k, v);
            }
        }
        out.finish()
    });
    Ok(())
}

fn random_bytes(len: usize) -> Vec<u8> {
    let mut buf = vec![0u8; len];
    // getrandom only fails when the OS has no entropy source, which neither
    // iOS nor Android lacks; a zeroed buffer would be silently insecure.
    getrandom::getrandom(&mut buf).unwrap_or_else(|e| panic!("getrandom failed: {e}"));
    buf
}

fn crypto<'js>(ctx: &Ctx<'js>, n: &Object<'js>) -> R<()> {
    native!(ctx, n, "random", |ctx: Ctx<'js>, len: usize| TypedArray::<u8>::new(ctx, random_bytes(len.min(65536))));
    native!(ctx, n, "uuid", || -> String {
        let mut b = random_bytes(16);
        b[6] = (b[6] & 0x0f) | 0x40;
        b[8] = (b[8] & 0x3f) | 0x80;
        let h: String = b.iter().map(|x| format!("{x:02x}")).collect();
        format!("{}-{}-{}-{}-{}", &h[..8], &h[8..12], &h[12..16], &h[16..20], &h[20..])
    });
    native!(ctx, n, "digest", |ctx: Ctx<'js>, alg: String, data: Value<'js>| -> R<ArrayBuffer<'js>> {
        let data = bytes(&ctx, data, "crypto.subtle.digest")?;
        let out = match alg.as_str() {
            "SHA-256" => sha2::Sha256::digest(&data).to_vec(),
            "SHA-512" => sha2::Sha512::digest(&data).to_vec(),
            _ => {
                return Err(Exception::throw_message(
                    &ctx,
                    &format!("NotSupportedError: crypto.subtle.digest: `{alg}` is not supported (SHA-256, SHA-512)"),
                ))
            }
        };
        ArrayBuffer::new(ctx, out)
    });
    Ok(())
}

fn storage<'js>(ctx: &Ctx<'js>, n: &Object<'js>, shared: &Arc<Shared>) -> R<()> {
    fn with<'js, T>(ctx: &Ctx<'js>, shared: &Shared, kind: u32, f: impl FnOnce(&mut Store) -> T) -> R<T> {
        if !shared.config.capabilities.storage {
            let name = if kind == 0 { "localStorage" } else { "sessionStorage" };
            return Err(policy::deny(ctx, name, Grant::Flag("storage")));
        }
        Ok(if kind == 0 { f(&mut lock(&shared.local.store)) } else { f(&mut lock(&shared.session)) })
    }
    let s = shared.clone();
    native!(ctx, n, "st_check", move |ctx: Ctx<'js>, name: String| -> R<()> {
        if s.config.capabilities.storage {
            Ok(())
        } else {
            Err(policy::deny(&ctx, &name, Grant::Flag("storage")))
        }
    });
    let s = shared.clone();
    native!(ctx, n, "st_get", move |ctx: Ctx<'js>, kind: u32, key: String| with(&ctx, &s, kind, |st| st.get(&key)));
    let s = shared.clone();
    native!(ctx, n, "st_set", move |ctx: Ctx<'js>, kind: u32, key: String, value: String| {
        let limits = &s.config.limits;
        with(&ctx, &s, kind, |st| st.set(&key, &value, limits).err().map(|q| q.0))
    });
    let s = shared.clone();
    native!(ctx, n, "st_remove", move |ctx: Ctx<'js>, kind: u32, key: String| with(&ctx, &s, kind, |st| st.remove(&key)));
    let s = shared.clone();
    native!(ctx, n, "st_clear", move |ctx: Ctx<'js>, kind: u32| with(&ctx, &s, kind, |st| st.clear()));
    let s = shared.clone();
    native!(ctx, n, "st_key", move |ctx: Ctx<'js>, kind: u32, i: u32| with(&ctx, &s, kind, |st| st.key(i as usize)));
    let s = shared.clone();
    native!(ctx, n, "st_len", move |ctx: Ctx<'js>, kind: u32| with(&ctx, &s, kind, |st| st.len() as u32));
    let s = shared.clone();
    native!(ctx, n, "st_flush", move |ctx: Ctx<'js>, kind: u32| -> R<()> {
        with(&ctx, &s, kind, |_| ())?;
        if kind == 0 {
            s.local.flush().map_err(|e| Exception::throw_message(&ctx, &format!("localStorage.flush: {e}")))?;
        }
        Ok(())
    });
    Ok(())
}

fn fs<'js>(ctx: &Ctx<'js>, n: &Object<'js>, shared: &Arc<Shared>) -> R<()> {
    fn run<'js, T>(ctx: &Ctx<'js>, shared: &Shared, f: impl FnOnce(&crate::fs::Fs) -> Result<T, String>) -> R<T> {
        if !shared.config.capabilities.fs {
            return Err(policy::deny(ctx, "fs/promises", Grant::Flag("fs")));
        }
        f(&shared.fs).map_err(|e| Exception::throw_message(ctx, &e))
    }
    let s = shared.clone();
    native!(ctx, n, "fs_read", move |ctx: Ctx<'js>, path: String, utf8: bool| -> R<Value<'js>> {
        let data = run(&ctx, &s, |fs| fs.read(&path, &s.config.limits))?;
        if utf8 {
            Ok(rquickjs::String::from_str(ctx.clone(), &String::from_utf8_lossy(&data))?.into_value())
        } else {
            Ok(TypedArray::<u8>::new(ctx.clone(), data)?.into_value())
        }
    });
    let s = shared.clone();
    native!(ctx, n, "fs_write", move |ctx: Ctx<'js>, path: String, data: Value<'js>| -> R<()> {
        let data = bytes(&ctx, data, "writeFile")?;
        run(&ctx, &s, |fs| fs.write(&path, &data, &s.config.limits))
    });
    let s = shared.clone();
    native!(ctx, n, "fs_readdir", move |ctx: Ctx<'js>, path: String| run(&ctx, &s, |fs| fs.readdir(&path, &s.config.limits)));
    let s = shared.clone();
    native!(ctx, n, "fs_exists", move |ctx: Ctx<'js>, path: String| run(&ctx, &s, |fs| fs.exists(&path)));
    let s = shared.clone();
    native!(ctx, n, "fs_unlink", move |ctx: Ctx<'js>, path: String| run(&ctx, &s, |fs| fs.unlink(&path)));
    let s = shared.clone();
    native!(ctx, n, "fs_mkdir", move |ctx: Ctx<'js>, path: String, recursive: bool| run(&ctx, &s, |fs| fs.mkdir(&path, recursive)));
    let s = shared.clone();
    native!(ctx, n, "fs_rmdir", move |ctx: Ctx<'js>, path: String| run(&ctx, &s, |fs| fs.rmdir(&path)));
    let s = shared.clone();
    native!(ctx, n, "fs_stat", move |ctx: Ctx<'js>, path: String| -> R<Object<'js>> {
        let st = run(&ctx, &s, |fs| fs.stat(&path))?;
        let o = Object::new(ctx.clone())?;
        o.set("size", st.size as f64)?;
        o.set("is_file", st.is_file)?;
        o.set("is_dir", st.is_dir)?;
        o.set("mtime_ms", st.mtime_ms)?;
        Ok(o)
    });
    let s = shared.clone();
    native!(ctx, n, "fs_rename", move |ctx: Ctx<'js>, from: String, to: String| run(&ctx, &s, |fs| fs.rename(&from, &to)));
    Ok(())
}

/// Headers a script may not set: they belong to the connection, not the request.
fn forbidden_header(name: &str) -> bool {
    matches!(
        name.to_ascii_lowercase().as_str(),
        "host" | "content-length" | "connection" | "expect" | "keep-alive" | "proxy-authenticate"
            | "proxy-authorization" | "te" | "trailer" | "transfer-encoding" | "upgrade"
    )
}

fn token(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(|b| b.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(&b))
}

fn fetch<'js>(ctx: &Ctx<'js>, n: &Object<'js>, shared: &Arc<Shared>) -> R<()> {
    let s = shared.clone();
    native!(ctx, n, "fetch", move |ctx: Ctx<'js>, url: String, method: String, headers: Vec<Vec<String>>, body: Value<'js>| -> R<f64> {
        let parsed = Url::parse(&url).map_err(|e| Exception::throw_type(&ctx, &format!("fetch: invalid URL `{url}`: {e}")))?;
        if !matches!(parsed.scheme(), "http" | "https") {
            return Err(Exception::throw_type(&ctx, &format!("fetch {url}: only http and https URLs are supported")));
        }
        if !token(&method) {
            return Err(Exception::throw_type(&ctx, &format!("fetch {url}: `{method}` is not an HTTP method")));
        }
        policy::check_network(&s.config.capabilities, "fetch", &method, &parsed).map_err(|m| Exception::throw_type(&ctx, &m))?;
        let mut list = Vec::with_capacity(headers.len());
        for pair in headers {
            let [name, value]: [String; 2] =
                pair.try_into().map_err(|_| Exception::throw_type(&ctx, "fetch: malformed headers"))?;
            if !token(&name) || value.contains(['\r', '\n', '\0']) {
                return Err(Exception::throw_type(&ctx, &format!("fetch {url}: invalid header `{name}`")));
            }
            if forbidden_header(&name) {
                return Err(Exception::throw_type(&ctx, &format!("fetch {url}: header `{name}` may not be set by scripts")));
            }
            list.push((name, value));
        }
        let body = if body.is_undefined() || body.is_null() {
            None
        } else {
            if matches!(method.as_str(), "GET" | "HEAD") {
                return Err(Exception::throw_type(&ctx, &format!("fetch {url}: a {method} request cannot have a body")));
            }
            let body = bytes(&ctx, body, "fetch body")?;
            if body.len() > s.config.limits.fetch_body_bytes {
                return Err(Exception::throw_range(
                    &ctx,
                    &format!("fetch {url}: request body over the {}-byte limit", s.config.limits.fetch_body_bytes),
                ));
            }
            Some(body)
        };
        let id = next_id();
        s.in_flight.fetch_add(1, Ordering::AcqRel);
        net::spawn_fetch(s.clone(), id, FetchRequest { url: parsed, method, headers: list, body });
        Ok(id as f64)
    });
    Ok(())
}

fn websocket<'js>(ctx: &Ctx<'js>, n: &Object<'js>, shared: &Arc<Shared>) -> R<()> {
    let s = shared.clone();
    native!(ctx, n, "ws_open", move |ctx: Ctx<'js>, url: String, protocols: Vec<String>| -> R<f64> {
        let mut parsed = Url::parse(&url).map_err(|e| Exception::throw_syntax(&ctx, &format!("WebSocket: invalid URL `{url}`: {e}")))?;
        let scheme = match parsed.scheme() {
            "ws" | "http" => "ws",
            "wss" | "https" => "wss",
            other => return Err(Exception::throw_syntax(&ctx, &format!("WebSocket {url}: scheme `{other}` is not ws or wss"))),
        };
        let _ = parsed.set_scheme(scheme);
        if parsed.fragment().is_some() {
            return Err(Exception::throw_syntax(&ctx, &format!("WebSocket {url}: URLs cannot have a fragment")));
        }
        if protocols.iter().any(|p| !token(p)) {
            return Err(Exception::throw_syntax(&ctx, &format!("WebSocket {url}: invalid subprotocol")));
        }
        policy::check_network(&s.config.capabilities, "WebSocket", "GET", &parsed).map_err(|m| Exception::throw_type(&ctx, &m))?;
        let id = next_id();
        let (tx, rx) = mpsc::channel();
        lock(&s.sockets).insert(id, tx);
        net::spawn_socket(s.clone(), id, parsed, protocols, rx);
        Ok(id as f64)
    });
    let s = shared.clone();
    native!(ctx, n, "ws_send", move |ctx: Ctx<'js>, id: f64, data: Value<'js>| -> R<()> {
        let message = if let Some(text) = data.as_string() {
            tungstenite::Message::text(text.to_string()?)
        } else {
            tungstenite::Message::binary(bytes(&ctx, data, "WebSocket.send")?)
        };
        if message.len() > s.config.limits.fetch_body_bytes {
            return Err(Exception::throw_range(&ctx, "WebSocket.send: message over the size limit"));
        }
        if let Some(socket) = lock(&s.sockets).get(&(id as u64)) {
            let _ = socket.send(WsCommand::Send(message));
        }
        Ok(())
    });
    let s = shared.clone();
    native!(ctx, n, "ws_close", move |id: f64, code: u16, reason: String| {
        if let Some(socket) = lock(&s.sockets).get(&(id as u64)) {
            let _ = socket.send(WsCommand::Close(code, reason));
        }
    });
    Ok(())
}
