//! What the platform provides to the core, and the descriptors of what the host
//! app registers for scripts.
//!
//! Two kinds of extension come from the host app (typically data push, its
//! HTTP client, storage, user info, business components):
//!
//! * **Host modules**: named JS modules (`import { subscribe } from "demo/quote"`)
//!   whose functions are answered by host code. Calls carry JSON. A method is
//!   `sync` (answered on the JS thread before the call returns: cheap lookups
//!   such as the current user), or `async` (returns a promise; the host answers
//!   later through `respond`). Modules may also push events to scripts
//!   (`emit`), which `gpui-shell` cannot do.
//! * **Host components**: native views the host registers by name; scripts place
//!   them with `host_component(name, props)` and lay them out like any element.
//!
//! The core only knows descriptors; implementations live on the platform side
//! and are reached through [`Host`].

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct TextMetrics {
    pub width: f32,
    pub height: f32,
    /// Distance from the top to the first line's baseline.
    pub baseline: f32,
}

/// Width constraint of a measurement.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum WidthMode {
    /// Lay out unconstrained (min-content/max-content probes).
    Undefined,
    /// At most this wide.
    AtMost(f32),
    /// Exactly this wide.
    Exactly(f32),
}

impl WidthMode {
    pub fn to_raw(self) -> (f32, i32) {
        match self {
            WidthMode::Undefined => (f32::INFINITY, 0),
            WidthMode::AtMost(w) => (w, 1),
            WidthMode::Exactly(w) => (w, 2),
        }
    }
}

/// Measurement callbacks, invoked synchronously on the JS thread during layout.
///
/// `attributed_json` is exactly the `Prop::AttributedText` value the renderer
/// will receive, so a renderer can cache the measured layout under that string
/// and draw it without measuring again (PrecomputedText / TextKit).
pub trait Measurer {
    fn measure_text(&self, attributed_json: &str, width: WidthMode) -> TextMetrics;

    /// Measures several texts at once. Renderers that can shape in parallel override it;
    /// results must equal calling [`Measurer::measure_text`] on each request.
    fn measure_text_batch(&self, requests: &[(&str, WidthMode)]) -> Vec<TextMetrics> {
        requests.iter().map(|(json, width)| self.measure_text(json, *width)).collect()
    }

    /// Intrinsic size of a host component, or `None` when it has no intrinsic
    /// size (the style must then give one).
    fn measure_component(&self, _name: &str, _props_json: &str, _width: WidthMode) -> Option<(f32, f32)> {
        None
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum MethodKind {
    Sync,
    #[default]
    Async,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HostMethod {
    pub name: String,
    #[serde(default)]
    pub kind: MethodKind,
    /// TypeScript signature for the generated `.d.ts`, e.g. `(symbol: string) => Promise<Quote>`.
    #[serde(default)]
    pub ts: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HostModuleDescriptor {
    /// Import specifier, e.g. `"demo/quote"`.
    pub name: String,
    #[serde(default)]
    pub methods: Vec<HostMethod>,
    /// Event names the module may `emit`.
    #[serde(default)]
    pub events: Vec<String>,
    /// When true, a bundle must list the module under `permissions.host_modules`.
    #[serde(default = "default_true")]
    pub requires_permission: bool,
    /// Extra TypeScript declarations (types the signatures mention).
    #[serde(default)]
    pub dts: Option<String>,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HostComponentDescriptor {
    /// Element name scripts use, e.g. `"demo-stock-cell"`.
    pub name: String,
    /// Event names the component may emit (`.on("select", …)` in script).
    #[serde(default)]
    pub events: Vec<String>,
    /// The host can measure it (`Measurer::measure_component`); otherwise the style must size it.
    #[serde(default)]
    pub measurable: bool,
    #[serde(default = "default_true")]
    pub requires_permission: bool,
    /// TypeScript type of its props, e.g. `{ symbol: string; compact?: boolean }`.
    #[serde(default)]
    pub props_ts: Option<String>,
}

/// What the host app registered, handed to every instance at creation.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct HostRegistry {
    #[serde(default)]
    pub modules: Vec<HostModuleDescriptor>,
    #[serde(default)]
    pub components: Vec<HostComponentDescriptor>,
}

impl HostRegistry {
    pub fn module(&self, name: &str) -> Option<&HostModuleDescriptor> {
        self.modules.iter().find(|m| m.name == name)
    }

    pub fn component(&self, name: &str) -> Option<&HostComponentDescriptor> {
        self.components.iter().find(|c| c.name == name)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
    Debug,
    Info,
    Warn,
    Error,
}

/// The platform side of an instance. In threaded mode every method is called
/// on the instance's JS thread, so implementations must be `Send + Sync`.
pub trait Host: Measurer + Send + Sync {
    /// A batch is ready: schedule `take_batch` on the main thread (next frame).
    fn batch_ready(&self);

    /// A `sync` host-module method. Must not block for long: it runs on the JS thread.
    fn call_sync(&self, module: &str, method: &str, args_json: &str) -> Result<String, String>;

    /// An `async` host-module method; answer later with `respond(request_id, …)`.
    fn call_async(&self, request_id: u64, module: &str, method: &str, args_json: &str);

    fn log(&self, level: LogLevel, message: &str);

    /// The instance failed in a way the script cannot recover from (uncaught render error on first render…).
    fn error(&self, message: &str) {
        self.log(LogLevel::Error, message);
    }
}

/// The screen the instance renders into. Sent by the renderer on attach and on every change.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Environment {
    pub width: f32,
    pub height: f32,
    #[serde(default = "one")]
    pub scale: f32,
    #[serde(default = "one")]
    pub font_scale: f32,
    /// `[top, right, bottom, left]`.
    #[serde(default)]
    pub safe_area: [f32; 4],
    #[serde(default)]
    pub keyboard_height: f32,
    #[serde(default)]
    pub appearance: crate::theme::Appearance,
    /// `"ios"`, `"android"` or `"macos"` (`"test"` in headless runs).
    #[serde(default = "test_platform")]
    pub platform: String,
    #[serde(default = "en")]
    pub locale: String,
    /// The platform shows its own confirmation when an app writes to the
    /// clipboard (Android 13+), so programs should not show another one.
    #[serde(default)]
    pub clipboard_feedback: bool,
}

fn one() -> f32 {
    1.0
}
fn test_platform() -> String {
    "test".into()
}
fn en() -> String {
    "en".into()
}

impl Default for Environment {
    fn default() -> Self {
        Environment {
            width: 390.0,
            height: 844.0,
            scale: 3.0,
            font_scale: 1.0,
            safe_area: [0.0; 4],
            keyboard_height: 0.0,
            appearance: Default::default(),
            platform: test_platform(),
            locale: en(),
            clipboard_feedback: false,
        }
    }
}
