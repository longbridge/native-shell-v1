//! The instance registry behind both the C ABI and JNI.
//!
//! Platforms hold instances as plain integer ids, never pointers: a stale or
//! forged id is an error, not a use-after-free. Ids increase monotonically and
//! are never reused.
//!
//! Two threading modes:
//!
//! * dedicated: a [`ThreadedInstance`] owns its JS thread and is `Send + Sync`,
//!   so any thread may call it; the registry only hands out `Arc` clones.
//! * caller: the [`Instance`] lives on the thread that created it. The registry
//!   stores it in a [`ThreadBound`] cell and refuses every call (including
//!   destroy) from another thread, so it is never touched, or dropped, off its
//!   thread.
//!
//! In caller mode a callback runs while the instance is borrowed, and the C
//! contract allows `call_async` to answer at once (`host_respond`/`host_emit`
//! from inside the callback). Those re-entrant calls are queued and applied as
//! soon as the outer call returns, before the borrow ends.
//!
//! Every operation runs under `catch_unwind`. A panic, or a host callback that
//! panicked (see [`crate::host`]), destroys that instance only and is reported
//! through the host's `error`.

use std::collections::{BTreeMap, HashMap, VecDeque};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock, TryLockError};
use std::thread::ThreadId;

use native_shell::runner::ThreadedInstance;
use native_shell::{Instance, Metrics};
use native_shell_core::host::{Environment, Host};
use native_shell_core::manifest::Manifest;
use native_shell_core::protocol::EventType;
use native_shell_services::bundle::{self, TrustedKey};

use crate::config::{Config, Threading};
use crate::error::panic_message;
use crate::gate::Gate;

/// Wraps a value that may only be used on the thread recorded next to it.
struct ThreadBound<T>(T);
// SAFETY: every access goes through `Entry::check_thread`, which rejects other
// threads before the cell is reached, and the value is dropped by `destroy` on
// its own thread (see `CallerSlot`).
unsafe impl<T> Send for ThreadBound<T> {}
unsafe impl<T> Sync for ThreadBound<T> {}

enum Deferred {
    Respond(u64, bool, String),
    Emit(String, String, String),
}

struct CallerSlot {
    instance: Mutex<Option<ThreadBound<Instance>>>,
    deferred: Mutex<VecDeque<Deferred>>,
    /// Finished batches, moved out of the instance after every call so that
    /// `batch_ready` fires in caller mode too and `take_batch` never needs
    /// the instance (it may be called from a callback).
    batches: Mutex<VecDeque<Vec<u8>>>,
    /// `Metrics` as of the end of the last call, for `metrics` from other threads.
    metrics: Mutex<Metrics>,
    /// Destroyed from inside one of its callbacks: drop it once that call returns.
    destroy_pending: AtomicBool,
}

enum Runner {
    Threaded(Arc<ThreadedInstance>),
    Caller(Box<CallerSlot>),
}

struct Entry {
    runner: Runner,
    /// `Some` in caller mode: the only thread allowed to touch the instance.
    owner: Option<ThreadId>,
    host: Arc<dyn Host>,
    poisoned: Arc<AtomicBool>,
    gate: Arc<Gate>,
    trusted_keys: Vec<TrustedKey>,
    revoked: Vec<String>,
}

type Registry = HashMap<u64, Arc<Entry>>;

fn registry() -> MutexGuard<'static, Registry> {
    static REGISTRY: OnceLock<Mutex<Registry>> = OnceLock::new();
    // Only inserts and removes happen under this lock; poisoning cannot leave it inconsistent.
    REGISTRY.get_or_init(Default::default).lock().unwrap_or_else(|p| p.into_inner())
}

static NEXT_ID: AtomicU64 = AtomicU64::new(1);

/// A host for an instance id that is about to exist.
pub type MakeHost<'a> = dyn FnOnce(u64, Arc<AtomicBool>, Arc<Gate>) -> Result<Arc<dyn Host>, String> + 'a;

pub fn create(config_json: &[u8], make_host: Box<MakeHost<'_>>) -> Result<u64, String> {
    let config = Config::parse(config_json)?;
    let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
    let poisoned = Arc::new(AtomicBool::new(false));
    let gate = Arc::new(Gate::default());
    let host = make_host(id, poisoned.clone(), gate.clone())?;
    let created = catch_unwind(AssertUnwindSafe(|| -> Result<(Runner, Option<ThreadId>), String> {
        match config.threading {
            Threading::Dedicated => {
                let t = ThreadedInstance::spawn(config.instance, host.clone()).map_err(|e| e.to_string())?;
                Ok((Runner::Threaded(Arc::new(t)), None))
            }
            Threading::Caller => {
                let i = Instance::new(config.instance, host.clone()).map_err(|e| e.to_string())?;
                let metrics = i.metrics_fast();
                let slot = CallerSlot {
                    instance: Mutex::new(Some(ThreadBound(i))),
                    deferred: Mutex::new(VecDeque::new()),
                    batches: Mutex::new(VecDeque::new()),
                    metrics: Mutex::new(metrics),
                    destroy_pending: AtomicBool::new(false),
                };
                Ok((Runner::Caller(Box::new(slot)), Some(std::thread::current().id())))
            }
        }
    }))
    .unwrap_or_else(|p| Err(format!("panic while creating the instance: {}", panic_message(p.as_ref()))))?;
    let (runner, owner) = created;
    let entry = Entry {
        runner,
        owner,
        host,
        poisoned,
        gate,
        trusted_keys: config.trusted_keys,
        revoked: config.revoked,
    };
    registry().insert(id, Arc::new(entry));
    Ok(id)
}

/// The entry for `id`, if it exists and this thread may use it.
fn lookup(id: u64) -> Result<Arc<Entry>, String> {
    let reg = registry();
    let entry = reg.get(&id).ok_or_else(|| format!("no instance {id}"))?;
    if let Some(owner) = entry.owner {
        if owner != std::thread::current().id() {
            return Err(format!(
                "instance {id} uses NSH_THREAD_CALLER and must only be called from the thread that created it"
            ));
        }
    }
    Ok(entry.clone())
}

/// What an operation does in each mode.
enum Op<'a> {
    LoadSources(Box<Manifest>, BTreeMap<String, String>),
    LoadBundle(&'a [u8]),
    SetEnvironment(Environment),
    Dispatch(u32, EventType, &'a str, u32),
    Respond(u64, bool, &'a str),
    Emit(&'a str, &'a str, &'a str),
    Tick(f64),
    TakeBatch,
    SetActive(bool),
    TrimMemory,
}

enum Outcome {
    None,
    Tick(Option<f64>),
    Batch(Option<Vec<u8>>),
}

fn run(id: u64, op: Op<'_>) -> Result<Outcome, String> {
    let entry = lookup(id)?;
    if entry.poisoned.load(Ordering::SeqCst) {
        destroy_entry(id, false);
        return Err(format!("instance {id} was destroyed after a host callback panicked"));
    }
    let result = catch_unwind(AssertUnwindSafe(|| apply(&entry, op)));
    if let Runner::Caller(slot) = &entry.runner {
        if slot.destroy_pending.load(Ordering::SeqCst) && !is_busy(slot) {
            drop(lock(&slot.instance).take());
            return Err(format!("instance {id} was destroyed during this call"));
        }
    }
    match result {
        Ok(r) => {
            if entry.poisoned.load(Ordering::SeqCst) {
                destroy_entry(id, false);
                return Err(format!("instance {id} was destroyed after a host callback panicked"));
            }
            r
        }
        Err(payload) => {
            let message = format!("instance {id} panicked and was destroyed: {}", panic_message(payload.as_ref()));
            report(&entry, &message);
            destroy_entry(id, true);
            Err(message)
        }
    }
}

fn report(entry: &Entry, message: &str) {
    let _ = catch_unwind(AssertUnwindSafe(|| entry.host.error(message)));
}

fn apply(entry: &Entry, op: Op<'_>) -> Result<Outcome, String> {
    let op = match op {
        Op::LoadBundle(bytes) => {
            let verified = bundle::open(bytes, &entry.trusted_keys, &entry.revoked, env!("CARGO_PKG_VERSION"))
                .map_err(|e| format!("bundle rejected: {e}"))?;
            return match &entry.runner {
                Runner::Threaded(t) => {
                    let t = t.clone();
                    load_in_background(entry, move || t.load_bundle(verified));
                    Ok(Outcome::None)
                }
                Runner::Caller(slot) => {
                    let (loaded, produced) = with_caller(slot, |i| i.load_bundle(verified))?;
                    if produced {
                        entry.host.batch_ready();
                    }
                    loaded.map(|_| Outcome::None).map_err(|e| e.to_string())
                }
            };
        }
        op => op,
    };
    match &entry.runner {
        Runner::Threaded(t) => Ok(match op {
            Op::LoadSources(m, files) => {
                let t = t.clone();
                load_in_background(entry, move || t.load_sources(*m, files));
                Outcome::None
            }
            Op::SetEnvironment(env) => {
                t.set_environment(env);
                Outcome::None
            }
            Op::Dispatch(node, ty, payload, seq) => {
                t.dispatch_event(node, ty, payload, seq);
                Outcome::None
            }
            Op::Respond(rid, ok, json) => {
                t.host_respond(rid, ok, json);
                Outcome::None
            }
            Op::Emit(m, e, p) => {
                t.host_emit(m, e, p);
                Outcome::None
            }
            Op::Tick(_) => return Err("nsh_instance_tick is only for NSH_THREAD_CALLER instances".into()),
            Op::TakeBatch => Outcome::Batch(t.take_batch()),
            Op::SetActive(a) => {
                t.set_active(a);
                Outcome::None
            }
            Op::TrimMemory => {
                t.trim_memory();
                Outcome::None
            }
            Op::LoadBundle(_) => unreachable!("handled above"),
        }),
        Runner::Caller(slot) => {
            // Re-entrant answers from inside a callback are queued, not refused.
            let deferred = match &op {
                Op::Respond(rid, ok, json) => Some(Deferred::Respond(*rid, *ok, json.to_string())),
                Op::Emit(m, e, p) => Some(Deferred::Emit(m.to_string(), e.to_string(), p.to_string())),
                _ => None,
            };
            if let Some(d) = deferred {
                if is_busy(slot) {
                    lock(&slot.deferred).push_back(d);
                    return Ok(Outcome::None);
                }
            }
            if let Op::TakeBatch = op {
                return Ok(Outcome::Batch(lock(&slot.batches).pop_front()));
            }
            let (outcome, produced) = with_caller(slot, |i| -> Result<Outcome, String> {
                Ok(match op {
                    Op::LoadSources(m, files) => {
                        i.load_sources(*m, files).map_err(|e| e.to_string())?;
                        Outcome::None
                    }
                    Op::SetEnvironment(env) => {
                        i.set_environment(env);
                        Outcome::None
                    }
                    Op::Dispatch(node, ty, payload, seq) => {
                        i.dispatch_event(node, ty, payload, seq);
                        Outcome::None
                    }
                    Op::Respond(rid, ok, json) => {
                        i.host_respond(rid, ok, json);
                        Outcome::None
                    }
                    Op::Emit(m, e, p) => {
                        i.host_emit(m, e, p);
                        Outcome::None
                    }
                    Op::Tick(now) => Outcome::Tick(i.tick(now)),
                    Op::SetActive(a) => {
                        i.set_active(a);
                        Outcome::None
                    }
                    Op::TrimMemory => {
                        i.trim_memory();
                        Outcome::None
                    }
                    Op::LoadBundle(_) | Op::TakeBatch => unreachable!("handled above"),
                })
            })?;
            if produced {
                entry.host.batch_ready();
            }
            outcome
        }
    }
}

/// Dedicated mode loads asynchronously (the header's contract): the engine's
/// load waits for the first frame, which must not block the platform's main
/// thread, and a failure is reported through `error`.
fn load_in_background(entry: &Entry, load: impl FnOnce() -> Result<(), native_shell::Error> + Send + 'static) {
    let host = entry.host.clone();
    std::thread::spawn(move || {
        if let Err(e) = load() {
            let _ = catch_unwind(AssertUnwindSafe(|| host.error(&e.to_string())));
        }
    });
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|p| p.into_inner())
}

fn is_busy(slot: &CallerSlot) -> bool {
    matches!(slot.instance.try_lock(), Err(TryLockError::WouldBlock))
}

/// Borrows the caller-mode instance, then applies answers queued by callbacks
/// meanwhile and moves finished batches into the slot.
/// Returns `f`'s result and whether new batches were queued.
fn with_caller<R>(slot: &CallerSlot, f: impl FnOnce(&mut Instance) -> R) -> Result<(R, bool), String> {
    let mut guard = match slot.instance.try_lock() {
        Ok(g) => g,
        Err(TryLockError::WouldBlock) => {
            return Err("the instance is busy: only host_respond/host_emit may be called from its callbacks".into())
        }
        Err(TryLockError::Poisoned(p)) => p.into_inner(),
    };
    let instance = &mut guard.as_mut().ok_or("the instance was destroyed")?.0;
    let r = f(instance);
    loop {
        let next = lock(&slot.deferred).pop_front();
        match next {
            Some(Deferred::Respond(rid, ok, json)) => instance.host_respond(rid, ok, &json),
            Some(Deferred::Emit(m, e, p)) => instance.host_emit(&m, &e, &p),
            None => break,
        }
    }
    let mut produced = false;
    while let Some(batch) = instance.take_batch() {
        lock(&slot.batches).push_back(batch);
        produced = true;
    }
    *lock(&slot.metrics) = instance.metrics_fast();
    Ok((r, produced))
}

/// Removes an instance. `leak` keeps the VM alive forever instead of dropping
/// it: after a panic unwound through the engine its state is unknown, and a
/// leak is safer than running finalizers over it.
fn destroy_entry(id: u64, leak: bool) {
    let Some(entry) = registry().remove(&id) else { return };
    // From here on no callback of this instance runs, and none is still
    // running on another thread (one may be running on this thread: the
    // destroy call itself came from it).
    entry.gate.close();
    match &entry.runner {
        Runner::Threaded(t) => {
            let t = t.clone();
            if leak {
                std::mem::forget(t);
            } else if entry.gate.is_inside() {
                // Called from a callback on the JS thread: joining it here
                // would wait for ourselves. The closed gate already keeps the
                // host from hearing anything more.
                std::thread::spawn(move || t.shutdown());
            } else {
                // Joins the JS thread; services are shut down with the instance on it.
                t.shutdown();
            }
        }
        Runner::Caller(slot) => {
            if is_busy(slot) {
                // Inside one of its own callbacks: dropped when that call returns.
                slot.destroy_pending.store(true, Ordering::SeqCst);
                return;
            }
            let instance = lock(&slot.instance).take();
            if leak {
                std::mem::forget(instance);
            } else {
                drop(instance);
            }
        }
    }
}

/// Destroys the instance. Once this returns, no callback for it runs again.
pub fn destroy(id: u64) -> Result<(), String> {
    drop(lookup(id)?);
    let result = catch_unwind(AssertUnwindSafe(|| destroy_entry(id, false)));
    result.map_err(|p| format!("panic while destroying instance {id}: {}", panic_message(p.as_ref())))
}

/// `Metrics` as JSON, from any thread and without waiting for the instance:
/// dedicated mode reads the runner's snapshot (refreshed after every frame),
/// caller mode the one taken at the end of its owner thread's last call. The
/// snapshots come from `metrics_fast` (`memory_used_bytes` reads 0); only a
/// caller-mode request on the owner thread, outside a callback, pays for the
/// full `metrics()` and its heap walk.
pub fn metrics(id: u64) -> Result<String, String> {
    let entry = registry().get(&id).cloned().ok_or_else(|| format!("no instance {id}"))?;
    if entry.poisoned.load(Ordering::SeqCst) {
        return Err(format!("instance {id} was destroyed after a host callback panicked"));
    }
    let m = match &entry.runner {
        Runner::Threaded(t) => t.metrics_snapshot(),
        Runner::Caller(slot) => {
            let full = (entry.owner == Some(std::thread::current().id()))
                .then(|| slot.instance.try_lock().ok())
                .flatten()
                .and_then(|g| g.as_ref().map(|i| i.0.metrics()));
            full.unwrap_or_else(|| *lock(&slot.metrics))
        }
    };
    serde_json::to_string(&m).map_err(|e| e.to_string())
}

pub fn load_sources(id: u64, json: &[u8]) -> Result<(), String> {
    #[derive(serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Sources {
        manifest: serde_json::Value,
        files: BTreeMap<String, String>,
    }
    let s: Sources = serde_json::from_slice(json).map_err(|e| format!("sources JSON: {e}"))?;
    let manifest = Manifest::parse(&s.manifest.to_string()).map_err(|e| e.to_string())?;
    run(id, Op::LoadSources(Box::new(manifest), s.files)).map(|_| ())
}

pub fn load_bundle(id: u64, bytes: &[u8]) -> Result<(), String> {
    run(id, Op::LoadBundle(bytes)).map(|_| ())
}

pub fn set_environment(id: u64, json: &[u8]) -> Result<(), String> {
    let env: Environment = serde_json::from_slice(json).map_err(|e| format!("environment JSON: {e}"))?;
    run(id, Op::SetEnvironment(env)).map(|_| ())
}

pub fn dispatch_event(id: u64, node: u32, event_type: u32, payload: &str, seq: u32) -> Result<(), String> {
    let ty = EventType::from_raw(event_type).ok_or_else(|| format!("unknown event type {event_type}"))?;
    run(id, Op::Dispatch(node, ty, payload, seq)).map(|_| ())
}

pub fn host_respond(id: u64, request_id: u64, ok: bool, json: &str) -> Result<(), String> {
    run(id, Op::Respond(request_id, ok, json)).map(|_| ())
}

pub fn host_emit(id: u64, module: &str, event: &str, payload: &str) -> Result<(), String> {
    run(id, Op::Emit(module, event, payload)).map(|_| ())
}

/// Milliseconds until the next timer, `-1` when none.
pub fn tick(id: u64, now_ms: f64) -> Result<i64, String> {
    match run(id, Op::Tick(now_ms))? {
        Outcome::Tick(Some(ms)) => Ok(ms.max(0.0).ceil() as i64),
        _ => Ok(-1),
    }
}

pub fn take_batch(id: u64) -> Result<Option<Vec<u8>>, String> {
    match run(id, Op::TakeBatch)? {
        Outcome::Batch(b) => Ok(b),
        _ => Ok(None),
    }
}

pub fn set_active(id: u64, active: bool) -> Result<(), String> {
    run(id, Op::SetActive(active)).map(|_| ())
}

pub fn trim_memory(id: u64) -> Result<(), String> {
    run(id, Op::TrimMemory).map(|_| ())
}
