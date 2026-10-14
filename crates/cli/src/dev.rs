//! `native-shell dev`: serves a program to devices on the LAN, with hot reload
//! and remote logs.
//!
//! Plain HTTP/1.1 over `std::net`, one thread per connection: a dev server
//! talks to a handful of devices, and every endpoint is tiny.
//!
//! * `GET /manifest`: the manifest JSON.
//! * `GET /bundle`: unsigned sources in the `nsh_instance_load_sources` format.
//! * `GET /version[?since=<v>]`: `{"version": v, "error": …}`. With `since`
//!   equal to the current version the request is held (long poll) until the
//!   program changes or 25 s pass, so a device reloads within one round trip.
//! * `POST /log`: a device log line (`{"level", "message"}` or plain text),
//!   printed here.
//!
//! The directory is polled every 250 ms (no platform watcher dependency) and a
//! change is published once the files have been quiet for 200 ms, so an editor
//! that saves several files at once triggers one reload.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream, UdpSocket};
use std::path::Path;
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant, SystemTime};

use anyhow::{Context, Result};

use crate::program::{list_files, Program};

const POLL: Duration = Duration::from_millis(250);
const DEBOUNCE: Duration = Duration::from_millis(200);
const LONG_POLL: Duration = Duration::from_secs(25);

/// What devices are served: the last successfully read program, or why reading failed.
struct Snapshot {
    version: String,
    manifest: String,
    bundle: String,
    error: Option<String>,
}

struct Shared {
    snapshot: Mutex<Snapshot>,
    changed: Condvar,
}

impl Snapshot {
    fn read(dir: &Path) -> Snapshot {
        match Program::read(dir) {
            Ok(p) => Snapshot { version: p.version(), manifest: p.manifest_json.clone(), bundle: p.sources_json(), error: None },
            Err(e) => {
                let error = format!("{e:#}");
                let version = format!("error-{:x}", fnv(error.as_bytes()));
                Snapshot { version, manifest: String::new(), bundle: String::new(), error: Some(error) }
            }
        }
    }
}

pub struct DevServer {
    pub addr: SocketAddr,
}

impl DevServer {
    /// Binds `0.0.0.0:port` (0 picks a free port) and starts serving in background threads.
    pub fn start(dir: &Path, port: u16, on_reload: impl Fn(&str, Option<&str>) + Send + 'static) -> Result<DevServer> {
        let listener = TcpListener::bind(("0.0.0.0", port)).with_context(|| format!("cannot listen on port {port}"))?;
        let addr = listener.local_addr()?;
        let first = Snapshot::read(dir);
        on_reload(&first.version, first.error.as_deref());
        let shared = Arc::new(Shared { snapshot: Mutex::new(first), changed: Condvar::new() });

        let watch_shared = shared.clone();
        let watch_dir = dir.to_path_buf();
        std::thread::spawn(move || watch(&watch_dir, &watch_shared, on_reload));

        std::thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                let shared = shared.clone();
                std::thread::spawn(move || {
                    // A broken connection only affects that device.
                    let _ = serve(stream, &shared);
                });
            }
        });
        Ok(DevServer { addr })
    }
}

/// The address other devices on the LAN reach this machine at.
pub fn lan_ip() -> String {
    // Connecting a UDP socket sends nothing; it only selects the outgoing interface.
    UdpSocket::bind("0.0.0.0:0")
        .and_then(|s| s.connect("10.255.255.255:1").map(|_| s))
        .and_then(|s| s.local_addr())
        .map(|a| a.ip().to_string())
        .unwrap_or_else(|_| "127.0.0.1".into())
}

fn fingerprint(dir: &Path) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for (rel, path) in list_files(dir).unwrap_or_default() {
        let meta = std::fs::metadata(&path).ok();
        let mtime = meta
            .as_ref()
            .and_then(|m| m.modified().ok())
            .and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok())
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let len = meta.map(|m| m.len()).unwrap_or(0);
        h = fnv_extend(h, rel.as_bytes());
        h = fnv_extend(h, &mtime.to_le_bytes());
        h = fnv_extend(h, &len.to_le_bytes());
    }
    h
}

fn watch(dir: &Path, shared: &Shared, on_reload: impl Fn(&str, Option<&str>)) {
    let mut seen = fingerprint(dir);
    let mut pending: Option<Instant> = None;
    loop {
        std::thread::sleep(POLL);
        let now = fingerprint(dir);
        if now != seen {
            seen = now;
            pending = Some(Instant::now());
            continue;
        }
        if pending.is_some_and(|t| t.elapsed() >= DEBOUNCE) {
            pending = None;
            let next = Snapshot::read(dir);
            let mut snap = shared.snapshot.lock().unwrap_or_else(|p| p.into_inner());
            if next.version != snap.version {
                on_reload(&next.version, next.error.as_deref());
                *snap = next;
                shared.changed.notify_all();
            }
        }
    }
}

struct Request {
    method: String,
    path: String,
    query: Vec<(String, String)>,
    body: Vec<u8>,
}

fn read_request(stream: &mut TcpStream) -> Result<Request> {
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut line = String::new();
    reader.read_line(&mut line)?;
    let mut parts = line.split_whitespace();
    let method = parts.next().unwrap_or("").to_string();
    let target = parts.next().unwrap_or("/").to_string();
    let mut content_length = 0usize;
    loop {
        let mut h = String::new();
        if reader.read_line(&mut h)? == 0 || h.trim().is_empty() {
            break;
        }
        if let Some((k, v)) = h.split_once(':') {
            if k.trim().eq_ignore_ascii_case("content-length") {
                content_length = v.trim().parse().unwrap_or(0);
            }
        }
    }
    let mut body = vec![0; content_length.min(1 << 20)];
    reader.read_exact(&mut body)?;
    let (path, q) = target.split_once('?').unwrap_or((&target, ""));
    let query = q
        .split('&')
        .filter(|p| !p.is_empty())
        .map(|p| {
            let (k, v) = p.split_once('=').unwrap_or((p, ""));
            (k.to_string(), v.to_string())
        })
        .collect();
    Ok(Request { method, path: path.to_string(), query, body })
}

fn respond(stream: &mut TcpStream, status: u16, content_type: &str, body: &[u8]) -> std::io::Result<()> {
    let reason = match status {
        200 => "OK",
        404 => "Not Found",
        405 => "Method Not Allowed",
        _ => "Internal Server Error",
    };
    write!(
        stream,
        "HTTP/1.1 {status} {reason}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n",
        body.len()
    )?;
    stream.write_all(body)?;
    stream.flush()
}

fn serve(mut stream: TcpStream, shared: &Shared) -> Result<()> {
    let req = read_request(&mut stream)?;
    const JSON: &str = "application/json; charset=utf-8";
    match (req.method.as_str(), req.path.as_str()) {
        ("GET", "/manifest") | ("GET", "/bundle") => {
            let snap = shared.snapshot.lock().unwrap_or_else(|p| p.into_inner());
            match &snap.error {
                Some(e) => respond(&mut stream, 500, "text/plain; charset=utf-8", e.as_bytes())?,
                None => {
                    let body = if req.path == "/manifest" { &snap.manifest } else { &snap.bundle };
                    respond(&mut stream, 200, JSON, body.as_bytes())?
                }
            }
        }
        ("GET", "/version") => {
            let since = req.query.iter().find(|(k, _)| k == "since").map(|(_, v)| v.clone());
            let deadline = Instant::now() + LONG_POLL;
            let mut snap = shared.snapshot.lock().unwrap_or_else(|p| p.into_inner());
            while since.as_deref() == Some(snap.version.as_str()) {
                let left = deadline.saturating_duration_since(Instant::now());
                if left.is_zero() {
                    break;
                }
                snap = shared.changed.wait_timeout(snap, left).unwrap_or_else(|p| p.into_inner()).0;
            }
            let body = serde_json::json!({ "version": snap.version, "error": snap.error }).to_string();
            drop(snap);
            respond(&mut stream, 200, JSON, body.as_bytes())?
        }
        ("POST", "/log") => {
            let text = String::from_utf8_lossy(&req.body);
            match serde_json::from_str::<serde_json::Value>(&text) {
                Ok(v) if v.get("message").is_some() => {
                    let level = v.get("level").and_then(|l| l.as_str()).unwrap_or("info");
                    let message = v.get("message").and_then(|m| m.as_str()).map(str::to_string).unwrap_or_else(|| v["message"].to_string());
                    println!("[device {level}] {message}");
                }
                _ => println!("[device] {}", text.trim_end()),
            }
            respond(&mut stream, 200, JSON, b"{}")?
        }
        (_, "/manifest" | "/bundle" | "/version" | "/log") => respond(&mut stream, 405, "text/plain", b"method not allowed")?,
        _ => respond(&mut stream, 404, "text/plain", b"not found")?,
    }
    Ok(())
}

fn fnv(bytes: &[u8]) -> u64 {
    fnv_extend(0xcbf2_9ce4_8422_2325, bytes)
}

fn fnv_extend(mut h: u64, bytes: &[u8]) -> u64 {
    for b in bytes {
        h ^= u64::from(*b);
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    h
}
