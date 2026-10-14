//! An instance of a program, running on its own JS thread.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use native_shell::runner::ThreadedInstance;
use native_shell::{InstanceConfig, Metrics};
use native_shell_core::host::{Environment, Host, LogLevel, Measurer, TextMetrics, WidthMode};
use native_shell_core::manifest::Manifest;
use native_shell_core::protocol::EventType;

/// What the platform side answers on the JS thread.
struct GtkHost {
    /// Wakes the surface's main-thread drain.
    ready: async_channel::Sender<()>,
}

impl Measurer for GtkHost {
    fn measure_text(&self, attributed_json: &str, width: WidthMode) -> TextMetrics {
        crate::text::measure(attributed_json, width)
    }
}

impl Host for GtkHost {
    fn batch_ready(&self) {
        let _ = self.ready.try_send(());
    }

    fn call_sync(&self, module: &str, _method: &str, _args_json: &str) -> Result<String, String> {
        Err(format!("host module `{module}` is not registered"))
    }

    fn call_async(&self, _request_id: u64, _module: &str, _method: &str, _args_json: &str) {}

    fn log(&self, level: LogLevel, message: &str) {
        eprintln!("[native-shell {level:?}] {message}");
    }
}

#[derive(Debug, Clone, Default)]
pub struct InstanceOptions {
    /// Where `localStorage` and `fs/promises` keep the program's data; `None` keeps it in memory.
    pub data_dir: Option<PathBuf>,
    /// Development mode (see `InstanceConfig::dev`).
    pub dev: bool,
}

pub struct Instance {
    pub(crate) threaded: ThreadedInstance,
    /// Signalled whenever a batch is ready; the surface drains on it.
    pub(crate) ready: async_channel::Receiver<()>,
}

impl Instance {
    pub fn new(app_id: &str, options: InstanceOptions) -> Result<Instance, String> {
        let (tx, rx) = async_channel::bounded(1);
        let host = Arc::new(GtkHost { ready: tx });
        let mut config = InstanceConfig::new(app_id);
        config.data_dir = options.data_dir;
        config.dev = options.dev;
        config.environment = Environment { platform: "linux".into(), ..Environment::default() };
        let threaded = ThreadedInstance::spawn(config, host).map_err(|e| e.to_string())?;
        Ok(Instance { threaded, ready: rx })
    }

    /// Loads a program directory: `native-shell.json` and every source file below it.
    pub fn load_dir(&self, dir: &Path) -> Result<(), String> {
        let manifest_json = std::fs::read_to_string(dir.join("native-shell.json"))
            .map_err(|e| format!("cannot read {}/native-shell.json: {e}", dir.display()))?;
        let manifest = Manifest::parse(&manifest_json).map_err(|e| e.to_string())?;
        let mut files = BTreeMap::new();
        collect_sources(dir, dir, &mut files)?;
        self.threaded.load_sources(manifest, files).map_err(|e| e.to_string())
    }

    /// Loads a program from memory: the manifest JSON and its files by path (`main.js`…).
    pub fn load_sources(&self, manifest_json: &str, files: BTreeMap<String, String>) -> Result<(), String> {
        let manifest = Manifest::parse(manifest_json).map_err(|e| e.to_string())?;
        self.threaded.load_sources(manifest, files).map_err(|e| e.to_string())
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

fn collect_sources(root: &Path, dir: &Path, out: &mut BTreeMap<String, String>) -> Result<(), String> {
    let entries = std::fs::read_dir(dir).map_err(|e| format!("cannot list {}: {e}", dir.display()))?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_sources(root, &path, out)?;
        } else if matches!(path.extension().and_then(|e| e.to_str()), Some("js" | "mjs" | "json")) {
            let Ok(rel) = path.strip_prefix(root) else { continue };
            let text = std::fs::read_to_string(&path).map_err(|e| format!("cannot read {}: {e}", path.display()))?;
            out.insert(rel.to_string_lossy().replace('\\', "/"), text);
        }
    }
    Ok(())
}
