//! One script instance: a handle on the core running on its own JS thread,
//! plus the host callbacks the core calls there (text and component
//! measurement, host-module calls, batch notifications, logs).

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::{Arc, OnceLock, Weak};

use gtk::glib;
use native_shell::runner::ThreadedInstance;
use native_shell::{InstanceConfig, Limits, Metrics};
use native_shell_core::host::{Environment, Host, LogLevel, Measurer, TextMetrics, WidthMode};
use native_shell_core::manifest::{Capabilities, Manifest};
use native_shell_core::protocol::EventType;
use native_shell_services::bundle;
use serde_json::Value;

use crate::runtime::{CallContext, Responder, Runtime, RESERVED_PREFIX};

/// What the platform side answers on the JS thread.
struct GtkHost {
    /// Wakes the surface's main-thread drain.
    ready: async_channel::Sender<()>,
    /// Unrecoverable errors, reported on the main thread.
    errors: async_channel::Sender<String>,
    runtime: Runtime,
    app_id: String,
    /// Set right after the JS thread starts; host calls only come after a load.
    instance: OnceLock<Weak<ThreadedInstance>>,
}

impl GtkHost {
    fn instance(&self) -> Weak<ThreadedInstance> {
        self.instance.get().cloned().unwrap_or_default()
    }

    fn context(&self, module: &str) -> CallContext {
        CallContext { app_id: self.app_id.clone(), module: module.to_string(), instance: self.instance() }
    }
}

fn parse_args(args_json: &str) -> Vec<Value> {
    match serde_json::from_str(args_json) {
        Ok(Value::Array(args)) => args,
        _ => Vec::new(),
    }
}

impl Measurer for GtkHost {
    fn measure_text(&self, attributed_json: &str, width: WidthMode) -> TextMetrics {
        crate::text::measure(attributed_json, width)
    }

    fn measure_component(&self, name: &str, props_json: &str, width: WidthMode) -> Option<(f32, f32)> {
        if name.starts_with(RESERVED_PREFIX) {
            let (w, h) = crate::components::measure_builtin(name, props_json)?;
            return Some(match width {
                WidthMode::Undefined => (w, h),
                WidthMode::AtMost(max) | WidthMode::Exactly(max) => (w.min(max), h),
            });
        }
        let props: Value = serde_json::from_str(props_json).unwrap_or(Value::Null);
        self.runtime.measure_component(name, &props, width)
    }
}

impl Host for GtkHost {
    fn batch_ready(&self) {
        let _ = self.ready.try_send(());
    }

    fn call_sync(&self, module: &str, method: &str, args_json: &str) -> Result<String, String> {
        if module == "nsh/clipboard" {
            return match method {
                "read" => Ok(crate::clipboard::text().map_or(Value::Null, Value::String).to_string()),
                _ => Err(format!("nsh/clipboard has no method `{method}`")),
            };
        }
        let host_module = self.runtime.module(module).ok_or_else(|| format!("host module `{module}` is not registered"))?;
        host_module.call_sync(method, &parse_args(args_json), &self.context(module)).map(|v| v.to_string())
    }

    fn call_async(&self, request_id: u64, module: &str, method: &str, args_json: &str) {
        let respond = Responder::new(request_id, module, self.instance());
        match self.runtime.module(module) {
            Some(host_module) => host_module.call(method, parse_args(args_json), self.context(module), respond),
            None => respond.reject(format!("host module `{module}` is not registered")),
        }
    }

    fn log(&self, level: LogLevel, message: &str) {
        self.runtime.log(level, message);
    }

    fn error(&self, message: &str) {
        self.runtime.log(LogLevel::Error, message);
        let _ = self.errors.try_send(message.to_string());
    }
}

#[derive(Debug, Clone, Default)]
pub struct InstanceOptions {
    /// Where `localStorage` and `fs/promises` keep the program's data.
    /// Default: `$XDG_DATA_HOME/native-shell/<app id>`.
    pub data_dir: Option<PathBuf>,
    /// Relaxes eval and frozen builtins for debugging; never relaxes capabilities.
    pub dev: bool,
    /// Capabilities that replace the manifest's (host grants).
    pub capabilities: Option<Capabilities>,
    /// Script limits; `None` keeps the core's defaults.
    pub limits: Option<Limits>,
}

type ErrorHandler = Box<dyn Fn(&str)>;

/// A program instance. Create it on the main thread, render it with
/// [`crate::SurfaceView::attach`]; dropping it stops its JS thread.
pub struct Instance {
    pub(crate) threaded: Arc<ThreadedInstance>,
    /// Signalled whenever a batch is ready; the surface drains on it.
    pub(crate) ready: async_channel::Receiver<()>,
    app_id: String,
    runtime: Runtime,
    error_handlers: Rc<RefCell<Vec<ErrorHandler>>>,
}

impl Instance {
    /// An instance with the process-wide [`Runtime::shared`] registrations.
    pub fn new(app_id: &str, options: InstanceOptions) -> Result<Instance, String> {
        Instance::with_runtime(app_id, options, Runtime::shared())
    }

    pub fn with_runtime(app_id: &str, options: InstanceOptions, runtime: &Runtime) -> Result<Instance, String> {
        crate::clipboard::watch();
        let (ready_tx, ready_rx) = async_channel::bounded(1);
        let (errors_tx, errors_rx) = async_channel::unbounded();
        let host = Arc::new(GtkHost {
            ready: ready_tx,
            errors: errors_tx,
            runtime: runtime.clone(),
            app_id: app_id.to_string(),
            instance: OnceLock::new(),
        });

        let data_dir = options.data_dir.unwrap_or_else(|| default_data_dir(app_id));
        std::fs::create_dir_all(&data_dir).map_err(|e| format!("cannot create {}: {e}", data_dir.display()))?;
        let mut config = InstanceConfig::new(app_id);
        config.data_dir = Some(data_dir);
        config.dev = options.dev;
        config.capabilities = options.capabilities;
        config.limits = options.limits.unwrap_or_default();
        config.registry = runtime.registry();
        config.theme = runtime.theme();
        config.environment = crate::surface::initial_environment();

        let threaded = Arc::new(ThreadedInstance::spawn(config, host.clone()).map_err(|e| e.to_string())?);
        let _ = host.instance.set(Arc::downgrade(&threaded));
        runtime.track(&threaded);

        let error_handlers: Rc<RefCell<Vec<ErrorHandler>>> = Rc::default();
        let handlers = Rc::downgrade(&error_handlers);
        glib::spawn_future_local(async move {
            while let Ok(message) = errors_rx.recv().await {
                let Some(handlers) = handlers.upgrade() else { break };
                for handler in handlers.borrow().iter() {
                    handler(&message);
                }
            }
        });
        Ok(Instance { threaded, ready: ready_rx, app_id: app_id.to_string(), runtime: runtime.clone(), error_handlers })
    }

    pub fn runtime(&self) -> &Runtime {
        &self.runtime
    }

    pub fn app_id(&self) -> &str {
        &self.app_id
    }

    /// Calls `handler` on the main thread when the instance fails in a way the
    /// script cannot recover from; the surface keeps its last good frame.
    pub fn connect_error(&self, handler: impl Fn(&str) + 'static) {
        self.error_handlers.borrow_mut().push(Box::new(handler));
    }

    /// Loads a program directory: `native-shell.json` and every source file below it.
    pub fn load_dir(&self, dir: &Path) -> Result<(), String> {
        let manifest_json = std::fs::read_to_string(dir.join("native-shell.json"))
            .map_err(|e| format!("cannot read {}/native-shell.json: {e}", dir.display()))?;
        let mut files = BTreeMap::new();
        collect_sources(dir, dir, &mut files)?;
        self.load_sources(&manifest_json, files)
    }

    /// Loads a program from memory: the manifest JSON and its files by path (`main.js`…).
    pub fn load_sources(&self, manifest_json: &str, files: BTreeMap<String, String>) -> Result<(), String> {
        let manifest = Manifest::parse(manifest_json).map_err(|e| e.to_string())?;
        self.threaded.load_sources(manifest, files).map_err(|e| e.to_string())
    }

    /// Loads a signed `.nspkg` bundle, checked against the runtime's trusted keys.
    pub fn load_bundle(&self, bytes: &[u8]) -> Result<(), String> {
        let (trusted, revoked) = (self.runtime.trusted_keys(), self.runtime.revoked());
        let verified = bundle::open(bytes, &trusted, &revoked, core_version()).map_err(|e| e.to_string())?;
        self.threaded.load_bundle(verified).map_err(|e| e.to_string())
    }

    /// Pushes a host-module event to this instance only.
    pub fn emit(&self, module: &str, event: &str, payload: &Value) {
        self.threaded.host_emit(module, event, &payload.to_string());
    }

    /// Pauses timers (window hidden, app in background) or resumes them.
    pub fn set_active(&self, active: bool) {
        self.threaded.set_active(active);
    }

    pub fn trim_memory(&self) {
        self.threaded.trim_memory();
    }

    /// Runtime counters, asked of the JS thread: every message sent before is handled when it answers.
    pub fn metrics(&self) -> Option<Metrics> {
        self.threaded.metrics()
    }

    /// The counters as of the JS thread's last tick; never blocks.
    pub fn metrics_snapshot(&self) -> Metrics {
        self.threaded.metrics_snapshot()
    }

    pub fn set_environment(&self, env: Environment) {
        self.threaded.set_environment(env);
    }

    pub fn dispatch_event(&self, node: u32, event: EventType, payload_json: &str, seq: u32) {
        self.threaded.dispatch_event(node, event, payload_json, seq);
    }

    pub fn take_batch(&self) -> Option<Vec<u8>> {
        self.threaded.take_batch()
    }
}

impl Drop for Instance {
    fn drop(&mut self) {
        // Joins the JS thread here: a host module still holding the handle
        // must never be the one to drop it on the JS thread itself.
        self.threaded.shutdown();
    }
}

/// `$XDG_DATA_HOME/native-shell/<app id>`, with the app id made a safe file name.
pub fn default_data_dir(app_id: &str) -> PathBuf {
    let safe: String = app_id.chars().map(|c| if c.is_alphanumeric() || matches!(c, '.' | '-' | '_') { c } else { '_' }).collect();
    let safe = if safe.is_empty() || safe.chars().all(|c| c == '.') { "default".to_string() } else { safe };
    glib::user_data_dir().join("native-shell").join(safe)
}

/// The core's version, which bundles name as their minimum runtime.
fn core_version() -> &'static str {
    let v = native_shell_core::ENGINE_VERSION;
    let v = v.split_once('/').map_or(v, |(_, rest)| rest);
    v.split_once('+').map_or(v, |(version, _)| version)
}

fn collect_sources(root: &Path, dir: &Path, out: &mut BTreeMap<String, String>) -> Result<(), String> {
    let entries = std::fs::read_dir(dir).map_err(|e| format!("cannot list {}: {e}", dir.display()))?;
    for entry in entries.flatten() {
        let path = entry.path();
        if entry.file_name().to_string_lossy().starts_with('.') {
            continue;
        }
        if path.is_dir() {
            collect_sources(root, &path, out)?;
        } else {
            let Ok(rel) = path.strip_prefix(root) else { continue };
            let bytes = std::fs::read(&path).map_err(|e| format!("cannot read {}: {e}", path.display()))?;
            // Every file is a source (`svg(path)` icons, images); binary ones
            // travel as data URLs, as the CLI sends them.
            let text = String::from_utf8(bytes).unwrap_or_else(|e| {
                format!("data:application/octet-stream;base64,{}", gtk::glib::base64_encode(e.as_bytes()))
            });
            out.insert(rel.to_string_lossy().replace('\\', "/"), text);
        }
    }
    Ok(())
}
