//! Running an instance on its own thread.
//!
//! QuickJS values cannot cross threads, so the [`Instance`] is created on the
//! JS thread and never leaves it. [`ThreadedInstance`] is the `Send + Sync`
//! handle the platform bindings hold: every operation is a message, the thread
//! sleeps until a message, the next timer, or a background completion
//! (`fetch`, sockets) wakes it, ticks, and moves finished batches into a queue
//! any thread can drain with [`ThreadedInstance::take_batch`] — the renderer's
//! main thread, after [`Host::batch_ready`].

use std::collections::{BTreeMap, VecDeque};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use native_shell_core::host::{Environment, Host};
use native_shell_core::manifest::Manifest;
use native_shell_core::protocol::EventType;
use native_shell_services::bundle::VerifiedBundle;

use crate::{Error, Instance, InstanceConfig, Metrics};

/// Where an instance's JS runs.
///
/// * `Dedicated`: on a thread of its own ([`ThreadedInstance`]); the host only
///   posts events and drains batches. The default for apps.
/// * `Caller`: on whichever thread drives the [`Instance`] directly, calling
///   `tick` from its own run loop (tests, the CLI, hosts that already have a
///   script thread).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ThreadingMode {
    #[default]
    Dedicated,
    Caller,
}

enum Message {
    LoadSources(Box<(Manifest, BTreeMap<String, String>)>, Sender<Result<(), Error>>),
    LoadBundle(Box<VerifiedBundle>, Sender<Result<(), Error>>),
    SetEnvironment(Environment),
    Event {
        node: u32,
        event: EventType,
        payload: String,
        seq: u32,
        sent: Instant,
    },
    Respond {
        id: u64,
        ok: bool,
        json: String,
    },
    Emit {
        module: String,
        event: String,
        payload: String,
    },
    SetActive(bool),
    TrimMemory,
    Metrics(Sender<Metrics>),
    /// A background completion is waiting for `tick`.
    Wake,
    Shutdown,
}

type BatchQueue = Arc<Mutex<VecDeque<Vec<u8>>>>;

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// A `Send + Sync` handle to an instance running on its own thread.
pub struct ThreadedInstance {
    tx: Sender<Message>,
    batches: BatchQueue,
    snapshot: Arc<Mutex<Metrics>>,
    thread: Mutex<Option<JoinHandle<()>>>,
}

impl ThreadedInstance {
    /// Starts the JS thread and creates the instance on it.
    pub fn spawn(config: InstanceConfig, host: Arc<dyn Host>) -> Result<ThreadedInstance, Error> {
        let (tx, rx) = mpsc::channel();
        let (ready_tx, ready_rx) = mpsc::channel();
        let batches: BatchQueue = Arc::default();
        let queue = batches.clone();
        let snapshot: Arc<Mutex<Metrics>> = Arc::default();
        let published = snapshot.clone();
        let wake = tx.clone();
        let thread = std::thread::Builder::new()
            .name(format!("native-shell:{}", config.app_id))
            .spawn(move || {
                let mut instance = match Instance::new(config, host.clone()) {
                    Ok(i) => i,
                    Err(e) => {
                        let _ = ready_tx.send(Err(e));
                        return;
                    }
                };
                instance.set_waker(Arc::new(move || {
                    let _ = wake.send(Message::Wake);
                }));
                let _ = ready_tx.send(Ok(()));
                run(instance, rx, host, queue, published);
            })
            .map_err(|e| Error::Setup(format!("cannot start the JS thread: {e}")))?;
        match ready_rx.recv() {
            Ok(Ok(())) => Ok(ThreadedInstance { tx, batches, snapshot, thread: Mutex::new(Some(thread)) }),
            Ok(Err(e)) => Err(e),
            Err(_) => Err(Error::Setup("the JS thread exited during setup".into())),
        }
    }

    fn send(&self, m: Message) {
        let _ = self.tx.send(m);
    }

    fn request<T>(&self, make: impl FnOnce(Sender<T>) -> Message) -> Option<T> {
        let (tx, rx) = mpsc::channel();
        self.send(make(tx));
        rx.recv().ok()
    }

    /// Loads a program and waits for its first frame (or failure).
    pub fn load_sources(&self, manifest: Manifest, files: BTreeMap<String, String>) -> Result<(), Error> {
        self.request(|tx| Message::LoadSources(Box::new((manifest, files)), tx))
            .unwrap_or_else(|| Err(Error::Setup("the instance has shut down".into())))
    }

    pub fn load_bundle(&self, bundle: VerifiedBundle) -> Result<(), Error> {
        self.request(|tx| Message::LoadBundle(Box::new(bundle), tx))
            .unwrap_or_else(|| Err(Error::Setup("the instance has shut down".into())))
    }

    pub fn set_environment(&self, env: Environment) {
        self.send(Message::SetEnvironment(env));
    }

    pub fn dispatch_event(&self, node: u32, event: EventType, payload_json: &str, seq: u32) {
        self.send(Message::Event { node, event, payload: payload_json.to_string(), seq, sent: Instant::now() });
    }

    pub fn host_respond(&self, request_id: u64, ok: bool, json: &str) {
        self.send(Message::Respond { id: request_id, ok, json: json.to_string() });
    }

    pub fn host_emit(&self, module: &str, event: &str, payload_json: &str) {
        self.send(Message::Emit {
            module: module.to_string(),
            event: event.to_string(),
            payload: payload_json.to_string(),
        });
    }

    pub fn set_active(&self, active: bool) {
        self.send(Message::SetActive(active));
    }

    pub fn trim_memory(&self) {
        self.send(Message::TrimMemory);
    }

    /// The oldest finished batch; callable from any thread.
    pub fn take_batch(&self) -> Option<Vec<u8>> {
        lock(&self.batches).pop_front()
    }

    /// Current metrics, asked of the JS thread (blocks until it answers).
    pub fn metrics(&self) -> Option<Metrics> {
        self.request(Message::Metrics)
    }

    /// The metrics as of the JS thread's last tick; never blocks on it.
    pub fn metrics_snapshot(&self) -> Metrics {
        *lock(&self.snapshot)
    }

    /// Stops the JS thread and waits for it; the instance is dropped there.
    pub fn shutdown(&self) {
        self.send(Message::Shutdown);
        if let Some(t) = lock(&self.thread).take() {
            let _ = t.join();
        }
    }
}

impl Drop for ThreadedInstance {
    fn drop(&mut self) {
        self.shutdown();
    }
}

fn run(
    mut instance: Instance,
    rx: Receiver<Message>,
    host: Arc<dyn Host>,
    batches: BatchQueue,
    snapshot: Arc<Mutex<Metrics>>,
) {
    let start = Instant::now();
    let now = || start.elapsed().as_secs_f64() * 1000.0;
    let mut next: Option<f64> = instance.tick(now());
    loop {
        let wait = next.map_or(Duration::from_secs(3600), |ms| Duration::from_secs_f64(ms.max(0.0) / 1000.0));
        let first = match rx.recv_timeout(wait) {
            Ok(m) => Some(m),
            Err(RecvTimeoutError::Timeout) => None,
            Err(RecvTimeoutError::Disconnected) => return,
        };
        // Everything already queued is handled before one tick builds one frame.
        for m in first.into_iter().chain(std::iter::from_fn(|| rx.try_recv().ok())) {
            match m {
                Message::LoadSources(program, reply) => {
                    let (manifest, files) = *program;
                    let _ = reply.send(instance.load_sources(manifest, files));
                }
                Message::LoadBundle(bundle, reply) => {
                    let _ = reply.send(instance.load_bundle(*bundle));
                }
                Message::SetEnvironment(env) => instance.set_environment(env),
                Message::Event { node, event, payload, seq, sent } => {
                    instance.note_runner(sent.elapsed().as_nanos() as u64, 0);
                    instance.dispatch_event(node, event, &payload, seq);
                }
                Message::Respond { id, ok, json } => instance.host_respond(id, ok, &json),
                Message::Emit { module, event, payload } => instance.host_emit(&module, &event, &payload),
                Message::SetActive(active) => instance.set_active(active),
                Message::TrimMemory => instance.trim_memory(),
                Message::Metrics(reply) => {
                    let _ = reply.send(instance.metrics());
                }
                Message::Wake => {}
                Message::Shutdown => return,
            }
        }
        next = instance.tick(now());
        let finished = Instant::now();
        let mut moved = false;
        while let Some(b) = instance.take_batch() {
            lock(&batches).push_back(b);
            moved = true;
        }
        if moved {
            host.batch_ready();
            instance.note_runner(0, finished.elapsed().as_nanos() as u64);
        }
        // After the handoff, and without the heap walk: this runs on every tick.
        *lock(&snapshot) = instance.metrics_fast();
    }
}
