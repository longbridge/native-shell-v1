//! Hot reload: runs the program a dev server serves (`nshell dev`), or a
//! local program directory, and reloads it in place whenever its files
//! change.
//!
//! The server (crates/cli/src/dev.rs) answers plain HTTP/1.1: `GET /bundle`
//! (`{manifest, files}`), `GET /version?since=<v>` (held until the program
//! changes, `{version, error}`) and `POST /log`. A background thread long-polls
//! the version and fetches each new bundle; the main thread then starts a new
//! instance in dev mode on a fresh [`SurfaceView`] inside the same container,
//! so the window stays and the old program is dropped. A server-side error,
//! or a server that cannot be reached, is shown as the engine's error page
//! until the next good version arrives. Script and renderer logs are
//! forwarded to the server, which prints them.
//!
//! A local directory is watched the way `nshell dev` watches it: polled
//! every 250 ms, and a change is published once the files have been quiet
//! for 200 ms, so an editor saving several files reloads once.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime};

use gtk::glib;
use adw::prelude::*;
use serde_json::Value;

use crate::instance::{Instance, InstanceOptions};
use crate::runtime::Runtime;
use crate::surface::SurfaceView;

/// Longer than the server's 25 s long poll.
const READ_TIMEOUT: Duration = Duration::from_secs(35);
const RETRY: Duration = Duration::from_secs(1);
/// As `nshell dev` watches a directory.
const WATCH_POLL: Duration = Duration::from_millis(250);
const WATCH_DEBOUNCE: Duration = Duration::from_millis(200);

/// What the poll thread hands to the main thread.
enum Update {
    Program { version: String, bundle: String },
    Error { version: String, message: String },
}

type ReloadHandler = Box<dyn Fn(&str)>;

struct Inner {
    container: adw::Bin,
    runtime: Runtime,
    surface: RefCell<Option<SurfaceView>>,
    version: RefCell<Option<String>>,
    reloaded: RefCell<Vec<ReloadHandler>>,
    stop: Arc<AtomicBool>,
}

/// A connection to a dev server; its [`DevClient::widget`] shows the program.
/// Dropping it stops polling.
pub struct DevClient {
    inner: Rc<Inner>,
}

impl Drop for DevClient {
    fn drop(&mut self) {
        self.inner.stop.store(true, Ordering::Relaxed);
    }
}

impl DevClient {
    /// Connects to `url` (`http://host:port`, as `nshell dev` prints it) and
    /// starts polling. Host modules and components come from `runtime`, whose
    /// logger is replaced by one that also forwards to the server.
    pub fn connect(url: &str, runtime: &Runtime) -> Result<DevClient, String> {
        let server = Server::parse(url)?;
        forward_logs(&server, runtime);
        DevClient::start(runtime, "nshell-dev-poll", move |stop, tx| poll(&server, &stop, &tx))
    }

    /// Runs the program in `dir` and reloads it whenever its files change.
    pub fn watch(dir: &Path, runtime: &Runtime) -> Result<DevClient, String> {
        if !dir.join("native-shell.json").is_file() {
            return Err(format!("{} is not a program: it has no native-shell.json", dir.display()));
        }
        let dir = dir.to_path_buf();
        DevClient::start(runtime, "native-shell-watch", move |stop, tx| watch(&dir, &stop, &tx))
    }

    /// The container and the main-thread side; `source` runs on a thread of its own.
    fn start(
        runtime: &Runtime,
        thread: &str,
        source: impl FnOnce(Arc<AtomicBool>, async_channel::Sender<Update>) + Send + 'static,
    ) -> Result<DevClient, String> {
        let stop = Arc::new(AtomicBool::new(false));
        let (tx, rx) = async_channel::unbounded();
        let thread_stop = stop.clone();
        std::thread::Builder::new().name(thread.into()).spawn(move || source(thread_stop, tx)).map_err(|e| e.to_string())?;

        let container = adw::Bin::builder().hexpand(true).vexpand(true).build();
        let inner = Rc::new(Inner {
            container,
            runtime: runtime.clone(),
            surface: RefCell::default(),
            version: RefCell::default(),
            reloaded: RefCell::default(),
            stop,
        });
        let weak = Rc::downgrade(&inner);
        glib::spawn_future_local(async move {
            while let Ok(update) = rx.recv().await {
                let Some(inner) = weak.upgrade() else { break };
                inner.apply(update);
            }
        });
        Ok(DevClient { inner })
    }

    /// The container showing the current program's surface.
    pub fn widget(&self) -> gtk::Widget {
        self.inner.container.clone().upcast()
    }

    /// The surface of the program shown now (a new one after every reload).
    pub fn surface(&self) -> Option<SurfaceView> {
        self.inner.surface.borrow().clone()
    }

    /// The server version shown now.
    pub fn version(&self) -> Option<String> {
        self.inner.version.borrow().clone()
    }

    /// Calls `f` with the version after every (re)load, error pages included.
    pub fn connect_reloaded(&self, f: impl Fn(&str) + 'static) {
        self.inner.reloaded.borrow_mut().push(Box::new(f));
    }
}

impl Inner {
    fn apply(&self, update: Update) {
        let (version, loaded) = match update {
            Update::Program { version, bundle } => {
                let loaded = parse_bundle(&bundle).and_then(|(app_id, manifest, files)| self.show(&app_id, |i| i.load_sources(&manifest, files)));
                (version, loaded)
            }
            Update::Error { version, message } => (version, Err(message)),
        };
        if let Err(message) = loaded {
            // Shown the way the engine shows a program that cannot start.
            let files = [(ERROR_FILE.to_string(), error_program(&message))].into();
            let _ = self.show("dev.native-shell.error", |i| i.load_sources(ERROR_MANIFEST, files));
        }
        self.version.replace(Some(version.clone()));
        for f in self.reloaded.borrow().iter() {
            f(&version);
        }
    }

    /// Runs a program on a new surface in place of the current one.
    fn show(&self, app_id: &str, load: impl FnOnce(&Instance) -> Result<(), String>) -> Result<(), String> {
        let options = InstanceOptions { dev: true, ..InstanceOptions::default() };
        let instance = Rc::new(Instance::with_runtime(app_id, options, &self.runtime)?);
        let surface = SurfaceView::new();
        self.container.set_child(Some(&surface));
        surface.attach(instance.clone());
        self.surface.replace(Some(surface));
        // A first render that fails still leaves the engine's error page on the surface.
        load(&instance)
    }
}

/// The error program's only file; the engine names it in the message.
const ERROR_FILE: &str = "nshell-dev.js";
const ERROR_MANIFEST: &str = r#"{"id":"dev.native-shell.error","name":"Dev server","version":"1.0.0","entry":"nshell-dev.js"}"#;

/// A program whose first render fails with `message`, so the engine shows its error page.
fn error_program(message: &str) -> String {
    let message = Value::String(message.to_string());
    format!("import {{ View }} from \"native-shell\";\nexport default class DevError extends View {{ render() {{ throw new Error({message}); }} }}\n")
}

/// `(app id, manifest JSON, files)` of a `/bundle` answer.
fn parse_bundle(bundle: &str) -> Result<(String, String, BTreeMap<String, String>), String> {
    let value: Value = serde_json::from_str(bundle).map_err(|e| format!("the dev server sent a malformed bundle: {e}"))?;
    let manifest = value.get("manifest").filter(|m| m.is_object()).ok_or("the dev server's bundle has no manifest")?;
    let app_id = manifest.get("id").and_then(Value::as_str).unwrap_or("dev.native-shell.program").to_string();
    let files = value
        .get("files")
        .and_then(Value::as_object)
        .map(|f| f.iter().filter_map(|(k, v)| Some((k.clone(), v.as_str()?.to_string()))).collect())
        .unwrap_or_default();
    Ok((app_id, manifest.to_string(), files))
}

fn poll(server: &Server, stop: &AtomicBool, tx: &async_channel::Sender<Update>) {
    let mut since: Option<String> = None;
    let mut unreachable = false;
    while !stop.load(Ordering::Relaxed) {
        let path = match &since {
            Some(v) => format!("/version?since={v}"),
            None => "/version".to_string(),
        };
        let answer = server.request("GET", &path, "").and_then(|(_, body)| {
            let v: Value = serde_json::from_str(&body).map_err(|e| e.to_string())?;
            let version = v.get("version").and_then(Value::as_str).ok_or("no version in the answer")?.to_string();
            Ok((version, v.get("error").and_then(Value::as_str).map(str::to_string)))
        });
        let update = match answer {
            Err(e) => {
                if !unreachable {
                    unreachable = true;
                    // A new pseudo-version, so the program reloads once the server is back.
                    since = Some("unreachable".into());
                    let message = format!("Cannot reach the dev server at {}: {e}", server.url);
                    if tx.send_blocking(Update::Error { version: "unreachable".into(), message }).is_err() {
                        return;
                    }
                }
                std::thread::sleep(RETRY);
                continue;
            }
            Ok((version, _)) if since.as_deref() == Some(version.as_str()) => continue,
            Ok((version, Some(message))) => Update::Error { version, message },
            Ok((version, None)) => match server.request("GET", "/bundle", "") {
                Ok((200, bundle)) => Update::Program { version, bundle },
                Ok((_, message)) | Err(message) => Update::Error { version, message },
            },
        };
        unreachable = false;
        since = Some(match &update {
            Update::Program { version, .. } | Update::Error { version, .. } => version.clone(),
        });
        if tx.send_blocking(update).is_err() {
            return;
        }
    }
}

/// Size and modification time of every file below `dir`, in path order.
fn scan(dir: &Path, out: &mut Vec<(PathBuf, u64, Option<SystemTime>)>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    let mut entries: Vec<_> = entries.flatten().collect();
    entries.sort_by_key(|e| e.path());
    for entry in entries {
        let path = entry.path();
        match entry.metadata() {
            Ok(m) if m.is_dir() => scan(&path, out),
            Ok(m) => out.push((path, m.len(), m.modified().ok())),
            Err(_) => {}
        }
    }
}

/// The program in `dir` as an update: its sources, or why they cannot be read.
fn read_program(dir: &Path) -> Update {
    let read = || -> Result<String, String> {
        let manifest_json = std::fs::read_to_string(dir.join("native-shell.json"))
            .map_err(|e| format!("cannot read {}/native-shell.json: {e}", dir.display()))?;
        let manifest: Value = serde_json::from_str(&manifest_json).map_err(|e| format!("native-shell.json: {e}"))?;
        let mut files = BTreeMap::new();
        crate::instance::collect_sources(dir, dir, &mut files)?;
        files.remove("native-shell.json");
        Ok(serde_json::json!({ "manifest": manifest, "files": files }).to_string())
    };
    match read() {
        Ok(bundle) => Update::Program { version: format!("{:016x}", fnv(bundle.as_bytes())), bundle },
        Err(message) => Update::Error { version: format!("error-{:016x}", fnv(message.as_bytes())), message },
    }
}

fn fnv(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |h, b| (h ^ u64::from(*b)).wrapping_mul(0x0100_0000_01b3))
}

fn watch(dir: &Path, stop: &AtomicBool, tx: &async_channel::Sender<Update>) {
    let snapshot = || {
        let mut files = Vec::new();
        scan(dir, &mut files);
        files
    };
    let mut seen = snapshot();
    if tx.send_blocking(read_program(dir)).is_err() {
        return;
    }
    let mut changed_at: Option<Instant> = None;
    while !stop.load(Ordering::Relaxed) {
        std::thread::sleep(WATCH_POLL);
        let now = snapshot();
        if now != seen {
            seen = now;
            changed_at = Some(Instant::now());
        }
        if changed_at.is_some_and(|t| t.elapsed() >= WATCH_DEBOUNCE) {
            changed_at = None;
            if tx.send_blocking(read_program(dir)).is_err() {
                return;
            }
        }
    }
}

/// Sends every log line to the server too, from a thread of its own.
fn forward_logs(server: &Server, runtime: &Runtime) {
    let (tx, rx) = std::sync::mpsc::channel::<String>();
    let server = server.clone();
    let _ = std::thread::Builder::new().name("nshell-dev-log".into()).spawn(move || {
        for body in rx {
            let _ = server.request("POST", "/log", &body);
        }
    });
    let tx = std::sync::Mutex::new(tx);
    runtime.set_logger(move |level, message| {
        eprintln!("[native-shell {level:?}] {message}");
        let level = serde_json::to_value(level).unwrap_or(Value::Null);
        let body = serde_json::json!({ "level": level, "message": message }).to_string();
        let _ = tx.lock().unwrap_or_else(|p| p.into_inner()).send(body);
    });
}

/// A dev server address and the few HTTP/1.1 requests this client makes.
#[derive(Clone)]
struct Server {
    url: String,
    host: String,
    port: u16,
}

impl Server {
    fn parse(url: &str) -> Result<Server, String> {
        let url = url.trim().trim_end_matches('/');
        let rest = url.strip_prefix("http://").ok_or_else(|| format!("`{url}` is not an http:// URL"))?;
        let (host, port) = rest.rsplit_once(':').ok_or_else(|| format!("`{url}` has no port"))?;
        let port = port.parse().map_err(|_| format!("`{url}` has no valid port"))?;
        if host.is_empty() {
            return Err(format!("`{url}` has no host"));
        }
        Ok(Server { url: url.to_string(), host: host.to_string(), port })
    }

    /// One request on a fresh connection; `(status, body)`.
    fn request(&self, method: &str, path: &str, body: &str) -> Result<(u16, String), String> {
        let addr = (self.host.as_str(), self.port)
            .to_socket_addrs()
            .map_err(|e| e.to_string())?
            .next()
            .ok_or("the host has no address")?;
        let mut stream = TcpStream::connect_timeout(&addr, Duration::from_secs(5)).map_err(|e| e.to_string())?;
        stream.set_read_timeout(Some(READ_TIMEOUT)).map_err(|e| e.to_string())?;
        let request = format!(
            "{method} {path} HTTP/1.1\r\nHost: {}:{}\r\nConnection: close\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
            self.host,
            self.port,
            body.len()
        );
        stream.write_all(request.as_bytes()).map_err(|e| e.to_string())?;
        let mut response = Vec::new();
        stream.read_to_end(&mut response).map_err(|e| e.to_string())?;
        let text = String::from_utf8_lossy(&response);
        let (head, body) = text.split_once("\r\n\r\n").ok_or("a malformed HTTP answer")?;
        let status = head.split_whitespace().nth(1).and_then(|s| s.parse().ok()).ok_or("a malformed HTTP status")?;
        Ok((status, body.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn urls() {
        let s = Server::parse("http://192.168.1.20:8080/").unwrap();
        assert_eq!((s.host.as_str(), s.port, s.url.as_str()), ("192.168.1.20", 8080, "http://192.168.1.20:8080"));
        assert!(Server::parse("https://example.org:1").is_err());
        assert!(Server::parse("http://example.org").is_err());
        assert!(Server::parse("http://:80").is_err());
    }

    #[test]
    fn bundles() {
        let (id, manifest, files) = parse_bundle(r#"{"manifest":{"id":"org.example.t","entry":"main.js"},"files":{"main.js":"x"}}"#).unwrap();
        assert_eq!(id, "org.example.t");
        assert!(manifest.contains("\"entry\""));
        assert_eq!(files.get("main.js").map(String::as_str), Some("x"));
        assert!(parse_bundle("{}").is_err());
        assert!(parse_bundle("nope").is_err());
    }
}
