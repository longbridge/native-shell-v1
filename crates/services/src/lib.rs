//! Standard library of native-shell: storage, fs, fetch, WebSocket, crypto,
//! encoding, URL, and signed bundles.
//!
//! # Threads
//!
//! JS values live on the instance's JS thread and never leave it. Work that
//! blocks (HTTP requests, socket I/O, storage writes) runs on worker threads
//! that only see plain Rust data; when one finishes it queues a
//! [`Completion`] and calls the engine's `waker`. The engine then calls
//! [`Services::poll`] on the JS thread, which hands each completion to a
//! function defined by the script-side prelude (`prelude.js`). That function
//! keeps the pending promises and open sockets in a closure on the JS heap, so
//! the garbage collector sees them, and Rust holds only that one function, in
//! the runtime's userdata, which the runtime drops before it frees itself.
//!
//! # Capabilities
//!
//! Every entry point checks [`Capabilities`] before it does anything, and a
//! refusal names the manifest change that would grant it (see [`policy`]).
//! `fetch` re-checks every redirect hop.

pub mod bundle;
mod fs;
mod js;
mod net;
pub mod policy;
mod storage;

use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use native_shell_core::manifest::Capabilities;

use crate::storage::{lock, Persisted, Store};

/// Resource limits; the defaults are the documented ones, tests shrink them.
#[derive(Debug, Clone, PartialEq)]
pub struct Limits {
    /// Request and response bodies of `fetch`, and WebSocket messages.
    pub fetch_body_bytes: usize,
    pub fetch_timeout: Duration,
    pub fetch_redirects: usize,
    pub storage_total_bytes: usize,
    pub storage_keys: usize,
    pub storage_value_bytes: usize,
    pub fs_read_bytes: usize,
    pub fs_write_bytes: usize,
    pub fs_readdir_entries: usize,
}

impl Default for Limits {
    fn default() -> Limits {
        Limits {
            fetch_body_bytes: 8 << 20,
            fetch_timeout: Duration::from_secs(30),
            fetch_redirects: 10,
            storage_total_bytes: 8 << 20,
            storage_keys: 4096,
            storage_value_bytes: 1 << 20,
            fs_read_bytes: 64 << 20,
            fs_write_bytes: 8 << 20,
            fs_readdir_entries: 10_000,
        }
    }
}

pub type Waker = Arc<dyn Fn() + Send + Sync>;

#[derive(Clone)]
pub struct ServicesConfig {
    pub app_id: String,
    /// Where `localStorage.json` and `files/` live. `None`: storage is memory
    /// only and `fs` is unavailable.
    pub data_dir: Option<PathBuf>,
    pub capabilities: Capabilities,
    /// Called from worker threads whenever a completion is queued.
    pub waker: Waker,
    pub limits: Limits,
}

impl ServicesConfig {
    pub fn new(app_id: impl Into<String>, data_dir: Option<PathBuf>, capabilities: Capabilities) -> ServicesConfig {
        ServicesConfig {
            app_id: app_id.into(),
            data_dir,
            capabilities,
            waker: Arc::new(|| {}),
            limits: Limits::default(),
        }
    }
}

/// The standard library of one program instance.
pub struct Services {
    shared: Arc<Shared>,
    id: u64,
}

pub(crate) struct Shared {
    pub config: ServicesConfig,
    queue: Mutex<VecDeque<Completion>>,
    /// Fetches started and not yet delivered.
    in_flight: AtomicUsize,
    sockets: Mutex<HashMap<u64, Sender<net::WsCommand>>>,
    local: Arc<Persisted>,
    session: Mutex<Store>,
    fs: fs::Fs,
    shut_down: AtomicBool,
}

pub(crate) enum Completion {
    Fetch { id: u64, result: Result<net::FetchResponse, String> },
    Ws { id: u64, event: WsEvent },
}

pub(crate) enum WsEvent {
    Open(String),
    Text(String),
    Binary(Vec<u8>),
    Error(String),
    Close { code: u16, reason: String, clean: bool },
}

static NEXT_ID: AtomicU64 = AtomicU64::new(1);

pub(crate) fn next_id() -> u64 {
    NEXT_ID.fetch_add(1, Ordering::Relaxed)
}

impl Shared {
    pub fn complete(&self, completion: Completion) {
        if self.shut_down.load(Ordering::Acquire) {
            return;
        }
        lock(&self.queue).push_back(completion);
        (self.config.waker)();
    }
}

impl Services {
    pub fn new(config: ServicesConfig) -> Services {
        let local_path = config.data_dir.as_ref().map(|d| d.join(storage::LOCAL_FILE));
        let fs = fs::Fs::new(config.data_dir.as_deref());
        Services {
            shared: Arc::new(Shared {
                config,
                queue: Mutex::new(VecDeque::new()),
                in_flight: AtomicUsize::new(0),
                sockets: Mutex::new(HashMap::new()),
                local: Arc::new(Persisted::new(Store::new(local_path))),
                session: Mutex::new(Store::in_memory()),
                fs,
                shut_down: AtomicBool::new(false),
            }),
            id: next_id(),
        }
    }

    pub fn config(&self) -> &ServicesConfig {
        &self.shared.config
    }

    /// Installs `fetch`, `Headers`, `Response`, `WebSocket`, `localStorage`,
    /// `sessionStorage`, `Storage`, `crypto`, `TextEncoder`, `TextDecoder`,
    /// `URL`, `URLSearchParams`, `atob`, `btoa`, and the hidden
    /// `__nsh_services.fs` the engine's `fs/promises` module re-exports.
    pub fn install<'js>(&self, ctx: &rquickjs::Ctx<'js>) -> rquickjs::Result<()> {
        js::install(ctx, &self.shared, self.id)
    }

    /// Delivers completed background work on the JS thread; returns how many
    /// completions were delivered. Promise reactions run on the engine's next
    /// `execute_pending_job`.
    pub fn poll<'js>(&self, ctx: &rquickjs::Ctx<'js>) -> usize {
        let delivered = js::poll(ctx, &self.shared, self.id);
        self.shared.local.persist_in_background();
        delivered
    }

    /// Whether anything is still expected from the background: a fetch in
    /// flight, an open socket, or completions waiting for `poll`.
    pub fn has_pending(&self) -> bool {
        self.shared.in_flight.load(Ordering::Acquire) > 0
            || !lock(&self.shared.sockets).is_empty()
            || !lock(&self.shared.queue).is_empty()
    }

    /// Writes `localStorage` to disk now.
    pub fn flush(&self) {
        let _ = self.shared.local.flush();
    }

    /// Closes sockets, abandons requests in flight (their results are
    /// dropped), and flushes storage. Pending promises never settle.
    pub fn shutdown(&self) {
        self.shared.shut_down.store(true, Ordering::Release);
        for (_, socket) in lock(&self.shared.sockets).drain() {
            let _ = socket.send(net::WsCommand::Close(1001, String::new()));
        }
        lock(&self.shared.queue).clear();
        self.shared.in_flight.store(0, Ordering::Release);
        self.flush();
    }
}

impl Drop for Services {
    fn drop(&mut self) {
        // Dropping the senders lets each socket worker close and exit.
        lock(&self.shared.sockets).clear();
        self.flush();
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn services_can_move_to_the_js_thread() {
        fn send_sync<T: Send + Sync>() {}
        send_sync::<super::Services>();
    }
}
