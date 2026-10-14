//! What the host app registers to extend scripts, process-wide: host modules
//! (JS-importable modules answered by Rust code), host components (GTK widgets
//! scripts place with `host_component(name, props)`), the theme override,
//! trusted bundle keys and the log sink. Register before creating instances:
//! each instance receives the registry as it was at its creation.

use std::collections::BTreeMap;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock, Weak};

use native_shell::runner::ThreadedInstance;
use native_shell::ThemeConfig;
use native_shell_core::host::{HostComponentDescriptor, HostModuleDescriptor, HostRegistry, LogLevel, WidthMode};
use native_shell_services::bundle::TrustedKey;
use serde_json::Value;

/// Names under this prefix (`nsh/clipboard`, `nsh/select`) are the platform's built-ins.
pub(crate) const RESERVED_PREFIX: &str = "nsh/";

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// A JS-importable module answered by host code, e.g. `import { quote } from "demo/quote"`.
///
/// Both kinds of call arrive on the instance's JS thread. Sync methods must
/// answer quickly; async methods answer through the [`Responder`], from any
/// thread (hop to the main loop with `glib::MainContext::default().invoke`
/// for GTK work). When a script's listener count for an event changes, the
/// sync method `__listeners` is called with `[event, count]` (errors are ignored).
pub trait HostModule: Send + Sync + 'static {
    /// Name, methods, events and TypeScript declarations.
    fn descriptor(&self) -> HostModuleDescriptor;

    fn call_sync(&self, method: &str, _args: &[Value], cx: &CallContext) -> Result<Value, String> {
        Err(format!("host module `{}` has no sync method `{method}`", cx.module))
    }

    fn call(&self, method: &str, _args: Vec<Value>, cx: CallContext, respond: Responder) {
        respond.reject(format!("host module `{}` has no async method `{method}`", cx.module));
    }
}

/// The calling instance, as seen by a host module.
#[derive(Clone)]
pub struct CallContext {
    pub app_id: String,
    /// The module being called.
    pub module: String,
    pub(crate) instance: Weak<ThreadedInstance>,
}

impl CallContext {
    /// Pushes an event of this module to the calling instance only ([`Runtime::emit`] broadcasts).
    pub fn emit(&self, event: &str, payload: &Value) {
        if let Some(instance) = self.instance.upgrade() {
            instance.host_emit(&self.module, event, &payload.to_string());
        }
    }
}

/// Answers one async host-module call; `Send`, and only the first answer counts.
/// Dropping it unanswered rejects the call, so a script never waits forever.
pub struct Responder {
    request: u64,
    module: String,
    instance: Weak<ThreadedInstance>,
    answered: AtomicBool,
}

impl Responder {
    pub(crate) fn new(request: u64, module: &str, instance: Weak<ThreadedInstance>) -> Responder {
        Responder { request, module: module.to_string(), instance, answered: AtomicBool::new(false) }
    }

    /// Resolves the script's promise with `value`.
    pub fn resolve(&self, value: &Value) {
        self.answer(true, &value.to_string());
    }

    /// Rejects the script's promise with an error carrying `message`.
    pub fn reject(&self, message: impl Into<String>) {
        self.answer(false, &message.into());
    }

    pub fn is_answered(&self) -> bool {
        self.answered.load(Ordering::Acquire)
    }

    fn answer(&self, ok: bool, json: &str) {
        if self.answered.swap(true, Ordering::AcqRel) {
            return;
        }
        // A destroyed instance took its VM along: the promise is gone too.
        if let Some(instance) = self.instance.upgrade() {
            instance.host_respond(self.request, ok, json);
        }
    }
}

impl Drop for Responder {
    fn drop(&mut self) {
        if !self.is_answered() {
            let message = format!("host module `{}` dropped the call without answering", self.module);
            self.answer(false, &message);
        }
    }
}

/// A host component's widget and how it takes new props; lives on the main thread.
pub trait HostComponent: 'static {
    /// The widget filling the node; created once, at mount.
    fn widget(&self) -> gtk::Widget;

    /// The full props on every change.
    fn update(&mut self, props: &Value);
}

/// Sends a host component's events to the script (`.on(name, fn)`); main thread.
#[derive(Clone)]
pub struct EventSink(pub(crate) Rc<EmitFn>);

type EmitFn = dyn Fn(&str, Value);

impl EventSink {
    pub fn emit(&self, event: &str, payload: Value) {
        (self.0)(event, payload);
    }
}

type CreateFn = dyn Fn(EventSink) -> Box<dyn HostComponent> + Send + Sync;
type MeasureFn = dyn Fn(&Value, WidthMode) -> (f32, f32) + Send + Sync;

/// A native view the host registers by name; scripts place it with `host_component(name, props)`.
///
/// `create` runs on the main thread for every mounted node. `measure`, when
/// given, runs on the JS thread during layout (no GTK calls there; Pango is
/// fine) and makes the component measurable: unsized, it takes that size.
#[derive(Clone)]
pub struct HostComponentType {
    descriptor: HostComponentDescriptor,
    create: Arc<CreateFn>,
    measure: Option<Arc<MeasureFn>>,
}

impl HostComponentType {
    pub fn new(name: &str, create: impl Fn(EventSink) -> Box<dyn HostComponent> + Send + Sync + 'static) -> HostComponentType {
        HostComponentType {
            descriptor: HostComponentDescriptor {
                name: name.to_string(),
                events: Vec::new(),
                measurable: false,
                requires_permission: true,
                props_ts: None,
            },
            create: Arc::new(create),
            measure: None,
        }
    }

    /// Event names the component emits.
    pub fn events<S: Into<String>>(mut self, events: impl IntoIterator<Item = S>) -> Self {
        self.descriptor.events = events.into_iter().map(Into::into).collect();
        self
    }

    /// Default true: the program must list the component in `capabilities.host_components`.
    pub fn requires_permission(mut self, required: bool) -> Self {
        self.descriptor.requires_permission = required;
        self
    }

    /// TypeScript type of the props, e.g. `{ symbol: string; compact?: boolean }`.
    pub fn props_ts(mut self, ts: &str) -> Self {
        self.descriptor.props_ts = Some(ts.to_string());
        self
    }

    /// Intrinsic size for the props within the width constraint.
    pub fn measure(mut self, measure: impl Fn(&Value, WidthMode) -> (f32, f32) + Send + Sync + 'static) -> Self {
        self.descriptor.measurable = true;
        self.measure = Some(Arc::new(measure));
        self
    }

    pub fn name(&self) -> &str {
        &self.descriptor.name
    }

    pub(crate) fn create(&self, events: EventSink) -> Box<dyn HostComponent> {
        (self.create)(events)
    }
}

type Logger = dyn Fn(LogLevel, &str) + Send + Sync;

#[derive(Default)]
struct Inner {
    modules: Mutex<BTreeMap<String, Arc<dyn HostModule>>>,
    components: Mutex<BTreeMap<String, HostComponentType>>,
    instances: Mutex<Vec<Weak<ThreadedInstance>>>,
    theme: Mutex<Option<ThemeConfig>>,
    logger: Mutex<Option<Arc<Logger>>>,
    trusted_keys: Mutex<Vec<TrustedKey>>,
    revoked: Mutex<Vec<String>>,
}

/// The registry instances are created from. Cheap to clone; `Send + Sync`.
#[derive(Clone, Default)]
pub struct Runtime {
    inner: Arc<Inner>,
}

impl Runtime {
    pub fn new() -> Runtime {
        Runtime::default()
    }

    /// The process-wide runtime [`crate::Instance::new`] uses.
    pub fn shared() -> &'static Runtime {
        static SHARED: OnceLock<Runtime> = OnceLock::new();
        SHARED.get_or_init(Runtime::new)
    }

    /// Registers (or replaces) a host module under its descriptor's name.
    pub fn register_module(&self, module: impl HostModule) -> Result<(), String> {
        let name = module.descriptor().name;
        if name.starts_with(RESERVED_PREFIX) {
            return Err(format!("host module `{name}`: names starting with `{RESERVED_PREFIX}` are reserved for built-ins"));
        }
        lock(&self.inner.modules).insert(name, Arc::new(module));
        Ok(())
    }

    /// Registers (or replaces) a host component type.
    pub fn register_component(&self, component: HostComponentType) -> Result<(), String> {
        let name = component.name().to_string();
        if name.starts_with(RESERVED_PREFIX) {
            return Err(format!("host component `{name}`: names starting with `{RESERVED_PREFIX}` are reserved for built-ins"));
        }
        lock(&self.inner.components).insert(name, component);
        Ok(())
    }

    pub fn unregister_module(&self, name: &str) {
        lock(&self.inner.modules).remove(name);
    }

    pub fn unregister_component(&self, name: &str) {
        lock(&self.inner.components).remove(name);
    }

    /// Pushes a host-module event to every live instance (scripts without listeners ignore it).
    pub fn emit(&self, module: &str, event: &str, payload: &Value) {
        let payload = payload.to_string();
        let mut instances = lock(&self.inner.instances);
        instances.retain(|weak| match weak.upgrade() {
            Some(instance) => {
                instance.host_emit(module, event, &payload);
                true
            }
            None => false,
        });
    }

    /// The app's design tokens mapped onto the shell's color tokens per appearance
    /// (`{"light": {"primary": "#2563eb"}, "dark": {…}}`); checked here, not at first render.
    pub fn set_theme(&self, theme: Option<ThemeConfig>) -> Result<(), String> {
        if let Some(theme) = &theme {
            theme.validate().map_err(|e| e.to_string())?;
        }
        *lock(&self.inner.theme) = theme;
        Ok(())
    }

    /// Receives renderer and script logs, from any thread. Default: GLib's log.
    pub fn set_logger(&self, logger: impl Fn(LogLevel, &str) + Send + Sync + 'static) {
        *lock(&self.inner.logger) = Some(Arc::new(logger));
    }

    /// Trusts an Ed25519 public key (base64) to sign `.nspkg` bundles.
    pub fn add_trusted_key(&self, id: &str, public_key_b64: &str) -> Result<(), String> {
        let key = TrustedKey::from_base64(id, public_key_b64).map_err(|e| format!("trusted key `{id}`: {e}"))?;
        lock(&self.inner.trusted_keys).push(key);
        Ok(())
    }

    /// Refuses the bundle with this SHA-256 (hex) even when its signature is valid.
    pub fn revoke_bundle(&self, sha256_hex: &str) {
        lock(&self.inner.revoked).push(sha256_hex.to_string());
    }

    /// The descriptors of everything registered, as instances receive them.
    pub fn registry(&self) -> HostRegistry {
        HostRegistry {
            modules: lock(&self.inner.modules).values().map(|m| m.descriptor()).collect(),
            components: lock(&self.inner.components).values().map(|c| c.descriptor.clone()).collect(),
        }
    }

    pub(crate) fn module(&self, name: &str) -> Option<Arc<dyn HostModule>> {
        lock(&self.inner.modules).get(name).cloned()
    }

    pub(crate) fn component(&self, name: &str) -> Option<HostComponentType> {
        lock(&self.inner.components).get(name).cloned()
    }

    pub(crate) fn measure_component(&self, name: &str, props: &Value, width: WidthMode) -> Option<(f32, f32)> {
        let measure = lock(&self.inner.components).get(name)?.measure.clone()?;
        Some(measure(props, width))
    }

    pub(crate) fn theme(&self) -> Option<ThemeConfig> {
        lock(&self.inner.theme).clone()
    }

    pub(crate) fn trusted_keys(&self) -> Vec<TrustedKey> {
        lock(&self.inner.trusted_keys).clone()
    }

    pub(crate) fn revoked(&self) -> Vec<String> {
        lock(&self.inner.revoked).clone()
    }

    pub(crate) fn track(&self, instance: &Arc<ThreadedInstance>) {
        let mut instances = lock(&self.inner.instances);
        instances.retain(|weak| weak.strong_count() > 0);
        instances.push(Arc::downgrade(instance));
    }

    pub(crate) fn log(&self, level: LogLevel, message: &str) {
        let logger = lock(&self.inner.logger).clone();
        match logger {
            Some(logger) => logger(level, message),
            None => {
                use gtk::glib;
                match level {
                    LogLevel::Debug => glib::g_debug!("native-shell", "{}", message),
                    LogLevel::Info => glib::g_info!("native-shell", "{}", message),
                    // Script errors are the program's, not GTK criticals.
                    LogLevel::Warn => glib::g_message!("native-shell", "{}", message),
                    LogLevel::Error => glib::g_warning!("native-shell", "{}", message),
                }
            }
        }
    }
}
