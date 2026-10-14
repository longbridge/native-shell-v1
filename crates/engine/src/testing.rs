//! A headless instance for tests and `nshell check`.
//!
//! [`Harness`] wires an [`Instance`] to a recording [`FakeHost`] (deterministic
//! text metrics from `native_shell_tree::testing::FakeMeasurer`, scripted
//! host-module answers, every log line and call recorded) and a manual clock,
//! and applies every batch to an in-memory native tree through the reference
//! decoder, the way a renderer would. A batch a renderer could not apply (an
//! op on a missing node, an index out of range, a node left without a frame…)
//! fails the test right there.
//!
//! Interaction helpers address nodes by `test_id` and send the same events a
//! renderer sends, with the same payloads and native event counts, so tests
//! exercise the real routing (`tree.handler`, coalescing, input echo
//! suppression) rather than calling script handlers directly.

use std::collections::{BTreeMap, HashMap};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

use base64::Engine as _;
use native_shell_core::host::{Environment, Host, HostRegistry, LogLevel, Measurer, TextMetrics, WidthMode};
use native_shell_core::manifest::{Capabilities, Manifest, MANIFEST_FILE};
use native_shell_core::protocol::{
    self, batch_flags, Command, EventType, Frame, Mutation, Prop, PropValue, ViewType, ROOT_NODE,
};
use native_shell_tree::testing::{FakeMeasurer, Replayer};

use crate::{Clock, Error, Instance, InstanceConfig, Limits, ThemeConfig};

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|p| p.into_inner())
}

/// Answers a host module's methods: `(method, args_json) → result JSON or error message`.
pub type SyncResponder = Box<dyn Fn(&str, &str) -> Result<String, String> + Send + Sync>;
/// Answers async methods; `None` leaves the request pending for [`Harness::respond`].
pub type AsyncResponder = Box<dyn Fn(&str, &str) -> Option<Result<String, String>> + Send + Sync>;

/// One recorded host-module call.
#[derive(Debug, Clone, PartialEq)]
pub struct HostCall {
    /// The request id of an async call.
    pub request: Option<u64>,
    pub module: String,
    pub method: String,
    pub args: String,
}

/// A recording host with scripted host modules.
#[derive(Default)]
pub struct FakeHost {
    sync: Mutex<HashMap<String, SyncResponder>>,
    asynchronous: Mutex<HashMap<String, AsyncResponder>>,
    calls: Mutex<Vec<HostCall>>,
    pending: Mutex<Vec<HostCall>>,
    logs: Mutex<Vec<(LogLevel, String)>>,
    errors: Mutex<Vec<String>>,
    batch_ready: AtomicUsize,
}

impl FakeHost {
    pub fn new() -> FakeHost {
        FakeHost::default()
    }

    pub fn on_sync(&self, module: &str, f: impl Fn(&str, &str) -> Result<String, String> + Send + Sync + 'static) {
        lock(&self.sync).insert(module.to_string(), Box::new(f));
    }

    pub fn on_async(
        &self,
        module: &str,
        f: impl Fn(&str, &str) -> Option<Result<String, String>> + Send + Sync + 'static,
    ) {
        lock(&self.asynchronous).insert(module.to_string(), Box::new(f));
    }

    pub fn calls(&self) -> Vec<HostCall> {
        lock(&self.calls).clone()
    }

    pub fn logs(&self) -> Vec<(LogLevel, String)> {
        lock(&self.logs).clone()
    }

    pub fn errors(&self) -> Vec<String> {
        lock(&self.errors).clone()
    }

    pub fn batch_ready_count(&self) -> usize {
        self.batch_ready.load(Ordering::SeqCst)
    }

    /// Async calls not yet answered (and answers the scripted responders produced).
    fn take_pending(&self) -> Vec<(HostCall, Option<Result<String, String>>)> {
        let pending: Vec<HostCall> = std::mem::take(&mut *lock(&self.pending));
        let responders = lock(&self.asynchronous);
        pending
            .into_iter()
            .map(|c| {
                let answer = responders.get(&c.module).and_then(|f| f(&c.method, &c.args));
                (c, answer)
            })
            .collect()
    }
}

impl Measurer for FakeHost {
    fn measure_text(&self, attributed_json: &str, width: WidthMode) -> TextMetrics {
        FakeMeasurer.measure_text(attributed_json, width)
    }

    fn measure_component(&self, name: &str, props_json: &str, width: WidthMode) -> Option<(f32, f32)> {
        FakeMeasurer.measure_component(name, props_json, width)
    }
}

impl Host for FakeHost {
    fn batch_ready(&self) {
        self.batch_ready.fetch_add(1, Ordering::SeqCst);
    }

    fn call_sync(&self, module: &str, method: &str, args_json: &str) -> Result<String, String> {
        lock(&self.calls).push(HostCall {
            request: None,
            module: module.into(),
            method: method.into(),
            args: args_json.into(),
        });
        match lock(&self.sync).get(module) {
            Some(f) => f(method, args_json),
            None => Err(format!("no fake for {module}.{method}")),
        }
    }

    fn call_async(&self, request_id: u64, module: &str, method: &str, args_json: &str) {
        let call = HostCall {
            request: Some(request_id),
            module: module.into(),
            method: method.into(),
            args: args_json.into(),
        };
        lock(&self.calls).push(call.clone());
        lock(&self.pending).push(call);
    }

    fn log(&self, level: LogLevel, message: &str) {
        lock(&self.logs).push((level, message.to_string()));
    }

    fn error(&self, message: &str) {
        lock(&self.errors).push(message.to_string());
        self.log(LogLevel::Error, message);
    }
}

/// Configures a [`Harness`] before it loads the program.
pub struct HarnessBuilder {
    files: BTreeMap<String, String>,
    manifest: Option<String>,
    registry: HostRegistry,
    capabilities: Option<Capabilities>,
    environment: Environment,
    limits: Limits,
    data_dir: Option<std::path::PathBuf>,
    dev: bool,
    theme: Option<ThemeConfig>,
    emulate_ranges: bool,
    host: Arc<FakeHost>,
}

impl HarnessBuilder {
    /// The manifest JSON (otherwise `native-shell.json` from the files, or a default one).
    pub fn manifest(mut self, json: &str) -> Self {
        self.manifest = Some(json.to_string());
        self
    }

    pub fn registry(mut self, registry: HostRegistry) -> Self {
        self.registry = registry;
        self
    }

    /// The host's capability override.
    pub fn capabilities(mut self, caps: Capabilities) -> Self {
        self.capabilities = Some(caps);
        self
    }

    pub fn environment(mut self, env: Environment) -> Self {
        self.environment = env;
        self
    }

    pub fn limits(mut self, limits: Limits) -> Self {
        self.limits = limits;
        self
    }

    pub fn data_dir(mut self, dir: impl Into<std::path::PathBuf>) -> Self {
        self.data_dir = Some(dir.into());
        self
    }

    pub fn dev(mut self, dev: bool) -> Self {
        self.dev = dev;
        self
    }

    /// Whether the harness reports list ranges on its own when an item count changes,
    /// as a renderer does (on by default). Off, only [`Harness::scroll_list`] reports them.
    pub fn emulate_list_ranges(mut self, on: bool) -> Self {
        self.emulate_ranges = on;
        self
    }

    /// The host's token overrides per appearance.
    pub fn theme(mut self, theme: ThemeConfig) -> Self {
        self.theme = Some(theme);
        self
    }

    pub fn sync_module(
        self,
        module: &str,
        f: impl Fn(&str, &str) -> Result<String, String> + Send + Sync + 'static,
    ) -> Self {
        self.host.on_sync(module, f);
        self
    }

    pub fn async_module(
        self,
        module: &str,
        f: impl Fn(&str, &str) -> Option<Result<String, String>> + Send + Sync + 'static,
    ) -> Self {
        self.host.on_async(module, f);
        self
    }

    /// Creates the instance, loads the program, and applies the first batches.
    /// A load failure is kept in [`Harness::load_error`] so tests can inspect the error page.
    pub fn start(self) -> Harness {
        let manifest_json = self.manifest.clone().or_else(|| self.files.get(MANIFEST_FILE).cloned());
        let (manifest, capabilities) = match &manifest_json {
            Some(json) => (Manifest::parse(json), self.capabilities.clone()),
            None => (
                Manifest::parse(r#"{"id":"test.harness","name":"Harness"}"#),
                Some(self.capabilities.clone().unwrap_or_else(Capabilities::all)),
            ),
        };
        let app_id = manifest.as_ref().map(|m| m.id.clone()).unwrap_or_else(|_| "test.harness".into());
        let now = Arc::new(AtomicU64::new(0f64.to_bits()));
        let read = now.clone();
        let config = InstanceConfig {
            app_id,
            data_dir: self.data_dir,
            limits: self.limits,
            capabilities,
            registry: self.registry,
            environment: self.environment,
            dev: self.dev,
            theme: self.theme,
        };
        let host: Arc<dyn Host> = self.host.clone();
        let instance = Instance::new(config, host);
        let mut h = Harness {
            instance: None,
            host: self.host,
            native: Replayer::new(),
            now,
            last_seq: 0,
            batches: 0,
            theme_tokens: Vec::new(),
            load_error: None,
            commands: Vec::new(),
            inputs: HashMap::new(),
            mutations: Vec::new(),
            lists: HashMap::new(),
            emulate_ranges: self.emulate_ranges,
        };
        let mut instance = match instance {
            Ok(mut i) => {
                i.set_clock(Clock::new(move || f64::from_bits(read.load(Ordering::SeqCst))));
                i
            }
            Err(e) => {
                h.load_error = Some(e);
                return h;
            }
        };
        let files: BTreeMap<String, String> = self.files.into_iter().filter(|(k, _)| k != MANIFEST_FILE).collect();
        let loaded = match manifest {
            Ok(m) => instance.load_sources(m, files),
            Err(e) => Err(Error::Manifest(e)),
        };
        h.instance = Some(instance);
        if let Err(e) = loaded {
            h.load_error = Some(e);
        }
        h.settle();
        h
    }
}

/// What the harness emulates of a native text field.
#[derive(Debug, Clone, Default)]
pub struct NativeInput {
    pub text: String,
    pub count: u32,
    pub composing: bool,
}

/// A headless instance with a manual clock and an in-memory native tree.
pub struct Harness {
    instance: Option<Instance>,
    host: Arc<FakeHost>,
    native: Replayer,
    /// The manual clock the instance reads; moves only when a test says so.
    /// (f64 bits; atomic so the instance can read it from inside any harness call.)
    now: Arc<AtomicU64>,
    last_seq: u32,
    batches: usize,
    /// Every `ThemeTokens` the renderer received, oldest first.
    theme_tokens: Vec<serde_json::Value>,
    load_error: Option<Error>,
    commands: Vec<(u32, Command, String)>,
    inputs: HashMap<u32, NativeInput>,
    mutations: Vec<Mutation>,
    lists: HashMap<u32, NativeList>,
    emulate_ranges: bool,
}

/// What the harness emulates of a native list: its scroll offset and the range it last reported.
#[derive(Debug, Clone, Copy, Default)]
struct NativeList {
    scroll_y: f32,
    reported: Option<(u32, u32)>,
    count_changed: bool,
}

/// A native text change the renderer has made but the core has not received yet.
#[derive(Debug, Clone)]
pub struct InFlight {
    node: u32,
    payload: String,
}

fn plain_text(json: &str) -> String {
    let v: serde_json::Value = serde_json::from_str(json).unwrap_or_default();
    v.get("runs")
        .and_then(|r| r.as_array())
        .map(|runs| runs.iter().filter_map(|r| r.get("text").and_then(|t| t.as_str())).collect())
        .unwrap_or_default()
}

impl Harness {
    /// `files` are `(path, source)`; a `native-shell.json` among them is the manifest.
    /// Without one, every capability is granted.
    pub fn new(files: &[(&str, &str)]) -> Harness {
        Harness::build(files).start()
    }

    pub fn build(files: &[(&str, &str)]) -> HarnessBuilder {
        HarnessBuilder {
            files: files.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect(),
            manifest: None,
            registry: HostRegistry::default(),
            capabilities: None,
            environment: Environment::default(),
            limits: Limits::default(),
            data_dir: None,
            dev: false,
            theme: None,
            emulate_ranges: true,
            host: Arc::new(FakeHost::new()),
        }
    }

    pub fn with_manifest(files: &[(&str, &str)], manifest_json: &str) -> Harness {
        Harness::build(files).manifest(manifest_json).start()
    }

    pub fn with_registry(files: &[(&str, &str)], registry: HostRegistry) -> Harness {
        Harness::build(files).registry(registry).start()
    }

    /// Loads every file of a program directory (`examples/tasks`); binary files as `data:` URLs.
    pub fn from_dir(dir: &std::path::Path) -> std::io::Result<HarnessBuilder> {
        let mut files = Vec::new();
        collect(dir, dir, &mut files)?;
        let refs: Vec<(&str, &str)> = files.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
        Ok(Harness::build(&refs))
    }

    pub fn load_error(&self) -> Option<&Error> {
        self.load_error.as_ref()
    }

    pub fn host(&self) -> &FakeHost {
        &self.host
    }

    pub fn instance(&self) -> &Instance {
        self.instance.as_ref().expect("the instance could not be created; see load_error()")
    }

    pub fn instance_mut(&mut self) -> &mut Instance {
        self.instance.as_mut().expect("the instance could not be created; see load_error()")
    }

    pub fn native(&self) -> &Replayer {
        &self.native
    }

    /// The current clock in ms.
    pub fn now(&self) -> f64 {
        f64::from_bits(self.now.load(Ordering::SeqCst))
    }

    /// Batches applied so far.
    pub fn batch_count(&self) -> usize {
        self.batches
    }

    /// Commands received so far: `(node, command, payload)`.
    pub fn commands(&self) -> &[(u32, Command, String)] {
        &self.commands
    }

    pub fn clear_commands(&mut self) {
        self.commands.clear();
    }

    /// Every mutation applied so far, batch markers included.
    pub fn mutations(&self) -> &[Mutation] {
        &self.mutations
    }

    /// Every `ThemeTokens` sent on the root, oldest first (the current one is
    /// also in `native().root_props()`).
    pub fn theme_tokens(&self) -> &[serde_json::Value] {
        &self.theme_tokens
    }

    pub fn clear_mutations(&mut self) {
        self.mutations.clear();
    }

    pub fn logs(&self) -> Vec<String> {
        self.host.logs().into_iter().map(|(_, m)| m).collect()
    }

    pub fn error_logs(&self) -> Vec<String> {
        self.host.logs().into_iter().filter(|(l, _)| *l == LogLevel::Error).map(|(_, m)| m).collect()
    }

    pub fn calls(&self) -> Vec<HostCall> {
        self.host.calls()
    }

    /// Advances the clock and runs everything due.
    pub fn advance(&mut self, ms: f64) {
        self.advance_clock(ms);
        self.settle();
    }

    /// Advances the clock without ticking, as time passes on a device while an
    /// instance idles: scripts see it, timers due by then wait for the next tick.
    pub fn advance_clock(&mut self, ms: f64) {
        self.now.store((self.now() + ms).to_bits(), Ordering::SeqCst);
    }

    /// Ticks at the current time until nothing changes: scripted host answers, timers due, frames.
    /// Async calls without a scripted answer stay pending for [`respond`](Self::respond).
    pub fn settle(&mut self) {
        for _ in 0..100 {
            let Some(instance) = self.instance.as_mut() else { return };
            let mut progressed = false;
            for (call, answer) in self.host.take_pending() {
                match (call.request, answer) {
                    (Some(id), Some(answer)) => {
                        let (ok, json) = match answer {
                            Ok(j) => (true, j),
                            Err(e) => (false, e),
                        };
                        instance.host_respond(id, ok, &json);
                        progressed = true;
                    }
                    _ => lock(&self.host.pending).push(call),
                }
            }
            instance.tick(f64::from_bits(self.now.load(Ordering::SeqCst)));
            let batches: Vec<Vec<u8>> = std::iter::from_fn(|| instance.take_batch()).collect();
            progressed |= !batches.is_empty();
            for b in batches {
                self.apply(&b);
            }
            for (node, payload) in self.list_ranges() {
                let seq = self.last_seq;
                self.instance_mut().dispatch_event(node, EventType::RangeChange, &payload, seq);
                progressed = true;
            }
            if !progressed {
                break;
            }
        }
    }

    fn apply(&mut self, bytes: &[u8]) {
        let batch = protocol::decode(bytes).unwrap_or_else(|e| panic!("undecodable batch: {e}"));
        match (batch.first(), batch.last()) {
            (Some(Mutation::BeginBatch { seq, flags }), Some(Mutation::EndBatch)) => {
                assert_eq!(*seq, self.last_seq + 1, "batch seq must increase by one");
                assert_eq!(
                    flags & batch_flags::INITIAL != 0,
                    self.last_seq == 0,
                    "INITIAL is set on the first batch only"
                );
                self.last_seq = *seq;
            }
            _ => panic!("a batch must start with BeginBatch and end with EndBatch: {batch:?}"),
        }
        for m in &batch {
            if let Mutation::SetProp { node: ROOT_NODE, prop: Prop::ThemeTokens, value: PropValue::Json(json) } = m {
                self.theme_tokens.push(serde_json::from_str(json).expect("ThemeTokens is JSON"));
            }
        }
        if let Err(e) = self.native.apply(&batch) {
            panic!("the renderer could not apply batch {}: {e}\n{batch:#?}", self.last_seq);
        }
        self.batches += 1;
        self.mutations.extend(batch.iter().cloned());
        for m in &batch {
            match m {
                Mutation::Command { node, command, payload } => self.commands.push((*node, *command, payload.clone())),
                Mutation::SetProp { node, prop: Prop::TextValue, value: PropValue::Json(json) } => {
                    let v: serde_json::Value = serde_json::from_str(json).unwrap_or_default();
                    let text = v.get("text").and_then(|t| t.as_str()).unwrap_or("").to_string();
                    let count = v.get("count").and_then(|c| c.as_u64()).unwrap_or(0) as u32;
                    let input = self.inputs.entry(*node).or_default();
                    // The renderer's rule: apply a script write only if it saw every native change.
                    if count >= input.count && !input.composing && text != input.text {
                        input.text = text;
                    }
                }
                Mutation::Delete { node } => {
                    self.inputs.remove(node);
                    self.lists.remove(node);
                }
                Mutation::SetProp { node, prop: Prop::ItemCount, .. } => {
                    self.lists.entry(*node).or_default().count_changed = true;
                }
                _ => {}
            }
        }
    }

    /// Like a renderer: when a list's item count changes, report the rows visible at its
    /// scroll offset (from `RowOffsets`), plus `Prefetch` rows on each side.
    fn list_ranges(&mut self) -> Vec<(u32, String)> {
        let mut out = Vec::new();
        if !self.emulate_ranges {
            return out;
        }
        for (node, list) in self.lists.iter_mut() {
            if !std::mem::take(&mut list.count_changed) {
                continue;
            }
            let Some(n) = self.native.nodes.get(node) else { continue };
            let count = match n.props.get(&Prop::ItemCount) {
                Some(PropValue::I32(c)) => (*c).max(0) as u32,
                _ => 0,
            };
            let offsets = match n.props.get(&Prop::RowOffsets) {
                Some(PropValue::F32Array(o)) => o.clone(),
                _ => Vec::new(),
            };
            let prefetch = match n.props.get(&Prop::Prefetch) {
                Some(PropValue::F32(p)) => *p as u32,
                _ => 10,
            };
            let height = n.frame.map_or(0.0, |f| f.height);
            let top = list.scroll_y;
            let first = (0..count).rfind(|&i| offsets.get(i as usize).is_some_and(|o| *o <= top)).unwrap_or(0);
            let last =
                (0..=count).find(|&i| offsets.get(i as usize).is_some_and(|o| *o >= top + height)).unwrap_or(count);
            let range = (first.saturating_sub(prefetch), (last + prefetch).min(count));
            if list.reported != Some(range) {
                list.reported = Some(range);
                out.push((*node, serde_json::json!({ "first": range.0, "end": range.1 }).to_string()));
            }
        }
        out.sort();
        out
    }

    // -------------------------------------------------------------------
    // Inspection
    // -------------------------------------------------------------------

    /// The native node carrying `test_id`.
    pub fn find(&self, test_id: &str) -> Option<u32> {
        let mut found: Vec<u32> = self
            .native
            .nodes
            .iter()
            .filter(|(_, n)| matches!(n.props.get(&Prop::TestId), Some(PropValue::Str(s)) if s == test_id))
            .map(|(id, _)| *id)
            .collect();
        found.sort_unstable();
        found.first().copied()
    }

    fn expect(&self, test_id: &str) -> u32 {
        self.find(test_id).unwrap_or_else(|| panic!("no node with test_id {test_id:?}\n{}", self.dump()))
    }

    pub fn prop(&self, test_id: &str, prop: Prop) -> Option<PropValue> {
        self.native.prop(self.expect(test_id), prop).cloned()
    }

    /// The frame of the node with `test_id`, relative to its native parent.
    pub fn frame(&self, test_id: &str) -> Frame {
        self.native.nodes[&self.expect(test_id)].frame.unwrap_or_else(|| panic!("{test_id} has no frame"))
    }

    /// Every prop of the node with `test_id` (background, radii, borders, text…), for style assertions.
    pub fn props(&self, test_id: &str) -> HashMap<Prop, PropValue> {
        self.native.nodes[&self.expect(test_id)].props.clone()
    }

    pub fn view_type(&self, test_id: &str) -> ViewType {
        self.native.nodes[&self.expect(test_id)].view_type
    }

    /// The visible text inside the node with `test_id` (text inputs: their native text).
    pub fn text_of(&self, test_id: &str) -> String {
        let node = self.expect(test_id);
        let mut out = Vec::new();
        self.collect_text(node, &mut out, true);
        out.join(" ")
    }

    /// All visible text, in tree order.
    pub fn texts(&self) -> Vec<String> {
        let mut out = Vec::new();
        for c in self.native.children(ROOT_NODE) {
            self.collect_text(*c, &mut out, false);
        }
        out
    }

    fn collect_text(&self, node: u32, out: &mut Vec<String>, inputs: bool) {
        let Some(n) = self.native.nodes.get(&node) else { return };
        if matches!(n.props.get(&Prop::Hidden), Some(PropValue::Bool(true))) {
            return;
        }
        if let Some(PropValue::Json(j)) = n.props.get(&Prop::AttributedText) {
            out.push(plain_text(j));
        }
        if inputs && n.view_type == ViewType::TextInput {
            out.push(self.inputs.get(&node).map(|i| i.text.clone()).unwrap_or_default());
        }
        for c in &n.children {
            self.collect_text(*c, out, inputs);
        }
    }

    /// The emulated native state of a text field.
    pub fn native_input(&self, test_id: &str) -> NativeInput {
        self.native_input_node(self.expect(test_id))
    }

    pub fn native_input_node(&self, node: u32) -> NativeInput {
        self.inputs.get(&node).cloned().unwrap_or_default()
    }

    /// Surfaces under the root, in order: `(node, view type)`.
    pub fn surfaces(&self) -> Vec<(u32, ViewType)> {
        self.native.children(ROOT_NODE).iter().map(|n| (*n, self.native.nodes[n].view_type)).collect()
    }

    pub fn dump(&self) -> String {
        self.instance.as_ref().map(|i| i.tree().dump()).unwrap_or_default()
    }

    // -------------------------------------------------------------------
    // Interaction
    // -------------------------------------------------------------------

    /// Sends a raw event as the renderer would, then settles.
    pub fn dispatch(&mut self, node: u32, event: EventType, payload: &str) {
        let seq = self.last_seq;
        self.instance_mut().dispatch_event(node, event, payload, seq);
        self.settle();
    }

    /// Dispatches without settling (to observe coalescing).
    pub fn dispatch_raw(&mut self, node: u32, event: EventType, payload: &str) {
        let seq = self.last_seq;
        self.instance_mut().dispatch_event(node, event, payload, seq);
    }

    /// The nearest node at or above `node` that listens for `mask`, as a gesture recognizer would find it.
    fn listener(&self, mut node: u32, mask: u32) -> u32 {
        loop {
            let Some(n) = self.native.nodes.get(&node) else { return node };
            if n.mask & mask != 0 {
                return node;
            }
            match n.parent {
                Some(p) if p != ROOT_NODE => node = p,
                _ => return node,
            }
        }
    }

    pub fn tap(&mut self, test_id: &str) {
        let node = self.expect(test_id);
        self.tap_node(node);
    }

    /// Taps the text leaf showing exactly `text` (for programs without test ids).
    pub fn tap_text(&mut self, text: &str) {
        let node = self.find_text(text).unwrap_or_else(|| panic!("no text {text:?} on screen\n{}", self.dump()));
        self.tap_node(node);
    }

    /// Taps a node, delivered to the nearest node at or above it that listens for clicks.
    pub fn tap_node(&mut self, node: u32) {
        let node = self.listener(node, protocol::event_mask::CLICK);
        self.dispatch(node, EventType::Click, r#"{"x":1,"y":1,"count":1}"#);
    }

    /// The first text leaf (in tree order) whose text is exactly `text`.
    pub fn find_text(&self, text: &str) -> Option<u32> {
        let mut stack: Vec<u32> = self.native.children(ROOT_NODE).iter().rev().copied().collect();
        while let Some(node) = stack.pop() {
            let n = self.native.nodes.get(&node)?;
            if let Some(PropValue::Json(j)) = n.props.get(&Prop::AttributedText) {
                if plain_text(j) == text {
                    return Some(node);
                }
            }
            stack.extend(n.children.iter().rev());
        }
        None
    }

    /// Every native node of a type, in tree order.
    pub fn nodes_of(&self, view_type: ViewType) -> Vec<u32> {
        let mut out = Vec::new();
        let mut stack: Vec<u32> = self.native.children(ROOT_NODE).iter().rev().copied().collect();
        while let Some(node) = stack.pop() {
            let Some(n) = self.native.nodes.get(&node) else { continue };
            if n.view_type == view_type {
                out.push(node);
            }
            stack.extend(n.children.iter().rev());
        }
        out
    }

    pub fn long_press(&mut self, test_id: &str) {
        let node = self.listener(self.expect(test_id), protocol::event_mask::LONG_PRESS);
        self.dispatch(node, EventType::LongPress, r#"{"x":1,"y":1}"#);
    }

    fn input_change(&mut self, node: u32, text: &str, composing: bool) {
        let change = self.native_edit(node, text, composing);
        self.deliver(change);
    }

    fn native_edit(&mut self, node: u32, text: &str, composing: bool) -> InFlight {
        let count = {
            let input = self.inputs.entry(node).or_default();
            input.count += 1;
            input.text = text.to_string();
            input.composing = composing;
            input.count
        };
        let len = text.encode_utf16().count();
        let payload =
            serde_json::json!({ "text": text, "selection": [len, len], "composing": composing, "count": count });
        InFlight { node, payload: payload.to_string() }
    }

    /// The user edits the field natively; the change event is not delivered yet.
    pub fn type_in_flight(&mut self, test_id: &str, text: &str) -> InFlight {
        let node = self.expect(test_id);
        self.native_edit(node, text, false)
    }

    /// Delivers a change made with [`type_in_flight`](Self::type_in_flight).
    pub fn deliver(&mut self, change: InFlight) {
        self.dispatch(change.node, EventType::Change, &change.payload);
    }

    /// Replaces a field's text as one native edit (one change event).
    pub fn type_text(&mut self, test_id: &str, text: &str) {
        let node = self.expect(test_id);
        self.input_change(node, text, false);
    }

    /// [`type_text`](Self::type_text) by node.
    pub fn type_text_node(&mut self, node: u32, text: &str) {
        self.input_change(node, text, false);
    }

    pub fn submit_node(&mut self, node: u32) {
        self.dispatch(node, EventType::Submit, "{}");
    }

    /// An IME composition: each step a composing change, then the committed text.
    pub fn compose(&mut self, test_id: &str, steps: &[&str], committed: &str) {
        let node = self.expect(test_id);
        for s in steps {
            self.input_change(node, s, true);
        }
        self.input_change(node, committed, false);
    }

    pub fn submit(&mut self, test_id: &str) {
        let node = self.expect(test_id);
        self.dispatch(node, EventType::Submit, "{}");
    }

    pub fn focus(&mut self, test_id: &str) {
        let node = self.expect(test_id);
        self.dispatch(node, EventType::Focus, "{}");
    }

    /// Flips a switch, checkbox or radio as a tap on it would.
    pub fn toggle(&mut self, test_id: &str) {
        let node = self.expect(test_id);
        let checked = matches!(self.native.prop(node, Prop::Checked), Some(PropValue::Bool(true)));
        self.dispatch(node, EventType::Change, &serde_json::json!({ "checked": !checked }).to_string());
    }

    /// A slider drag: intermediate values (coalesced), then the final one.
    pub fn slide(&mut self, test_id: &str, values: &[f64]) {
        let node = self.expect(test_id);
        for (i, v) in values.iter().enumerate() {
            let last = i + 1 == values.len();
            let payload = serde_json::json!({ "value": v, "final": last }).to_string();
            if last {
                self.dispatch(node, EventType::Change, &payload);
            } else {
                self.dispatch_raw(node, EventType::Change, &payload);
            }
        }
    }

    /// Picks an item of tabs (segmented) or a select.
    pub fn select(&mut self, test_id: &str, index: u32) {
        let node = self.expect(test_id);
        let mut payload = serde_json::json!({ "index": index });
        if let Some(PropValue::Json(menu)) = self.native.prop(node, Prop::Menu) {
            let menu: serde_json::Value = serde_json::from_str(menu).unwrap_or_default();
            if let Some(v) = menu.get("items").and_then(|i| i.get(index as usize)).and_then(|i| i.get("value")) {
                payload["value"] = v.clone();
            }
        }
        self.dispatch(node, EventType::Change, &payload.to_string());
    }

    pub fn pick_date(&mut self, test_id: &str, date: &str) {
        let node = self.expect(test_id);
        self.dispatch(node, EventType::Change, &serde_json::json!({ "date": date }).to_string());
    }

    /// The renderer reports a list's visible range `[first, end)` (prefetch included).
    pub fn scroll_list(&mut self, list_test_id: &str, first: u32, end: u32) {
        let node = self.expect(list_test_id);
        let top = match self.native.prop(node, Prop::RowOffsets) {
            Some(PropValue::F32Array(o)) => o.get(first as usize).copied().unwrap_or(0.0),
            _ => 0.0,
        };
        let list = self.lists.entry(node).or_default();
        list.scroll_y = top;
        list.reported = Some((first, end));
        self.dispatch(node, EventType::RangeChange, &serde_json::json!({ "first": first, "end": end }).to_string());
    }

    pub fn scroll(&mut self, test_id: &str, y: f32) {
        let node = self.expect(test_id);
        self.dispatch(node, EventType::Scroll, &serde_json::json!({ "x": 0, "y": y }).to_string());
    }

    /// A host component emits `name` with `payload_json`.
    pub fn host_event(&mut self, test_id: &str, name: &str, payload_json: &str) {
        let node = self.expect(test_id);
        let payload: serde_json::Value = serde_json::from_str(payload_json).unwrap_or_default();
        self.dispatch(node, EventType::HostEvent, &serde_json::json!({ "name": name, "payload": payload }).to_string());
    }

    fn top_surface(&self) -> u32 {
        *self.native.children(ROOT_NODE).last().expect("no surface")
    }

    /// System back on the top surface.
    pub fn back(&mut self) {
        let node = self.top_surface();
        self.dispatch(node, EventType::BackPressed, "{}");
    }

    /// The user dismisses the top surface natively (swipe down, backdrop, back gesture).
    pub fn dismiss(&mut self) {
        let node = self.top_surface();
        self.dispatch(node, EventType::Dismiss, "{}");
    }

    /// The user taps the action button of toast `id` (the renderer has dismissed it).
    pub fn toast_action(&mut self, id: &str) {
        self.dispatch(ROOT_NODE, EventType::ToastAction, &serde_json::json!({ "id": id }).to_string());
    }

    /// The user picks an action of the alert on top.
    pub fn alert_action(&mut self, index: u32) {
        let node = self.top_surface();
        self.dispatch(node, EventType::Action, &serde_json::json!({ "index": index }).to_string());
    }

    /// The top surface's alert spec, if it is an alert.
    pub fn alert(&self) -> Option<serde_json::Value> {
        match self.native.prop(self.top_surface(), Prop::AlertSpec) {
            Some(PropValue::Json(j)) => serde_json::from_str(j).ok(),
            _ => None,
        }
    }

    pub fn emit(&mut self, module: &str, event: &str, payload_json: &str) {
        self.instance_mut().host_emit(module, event, payload_json);
        self.settle();
    }

    pub fn respond(&mut self, request_id: u64, ok: bool, json: &str) {
        self.instance_mut().host_respond(request_id, ok, json);
        self.settle();
    }

    pub fn set_environment(&mut self, env: Environment) {
        self.instance_mut().set_environment(env);
        self.settle();
    }
}

fn collect(root: &std::path::Path, dir: &std::path::Path, out: &mut Vec<(String, String)>) -> std::io::Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let path = entry?.path();
        if path.is_dir() {
            collect(root, &path, out)?;
        } else {
            let rel = path.strip_prefix(root).unwrap_or(&path).to_string_lossy().replace('\\', "/");
            // Binary files (images) travel in sources as data URLs, as the CLI sends them.
            let text = match String::from_utf8(std::fs::read(&path)?) {
                Ok(text) => text,
                Err(e) => format!(
                    "data:application/octet-stream;base64,{}",
                    base64::engine::general_purpose::STANDARD.encode(e.into_bytes())
                ),
            };
            out.push((rel, text));
        }
    }
    Ok(())
}
