//! One program on one thread: the QuickJS runtime, the prelude, the tree, and
//! the batches waiting for the renderer.
//!
//! # A frame
//!
//! `tick` fires due timers, runs promise jobs, delivers background completions
//! (`fetch`, sockets) and coalesced events, then builds a frame: every entity
//! the script invalidated renders once (`render` budget), its output converts
//! into a [`Desc`] kept as `Rc` per entity, the surface list (root, pages,
//! overlays) is assembled, and [`Tree::commit`] turns the difference into
//! mutations. Commands queued by the script (focus, toast, …) follow the
//! mutations, so every node they target already exists. The whole is one
//! encoded batch; `seq` increases per batch.
//!
//! # Failure
//!
//! A render that throws (or whose output does not convert) keeps the entity's
//! last good description and discards the handlers it registered; a red banner
//! (`test_id` `nsh-error-banner`) shows the message on the root surface until
//! the entity renders again. When the entry view itself never rendered, the
//! root shows an error page (`nsh-error-page`) and `load_*` returns
//! [`Error::FirstRender`].
//!
//! # Events
//!
//! Continuous events (scroll, pan, pinch, list ranges, a slider while
//! dragging) are coalesced to the latest per `(node, type)` and delivered on
//! the next tick; discrete ones are delivered at once, after any coalesced
//! event of the same node, so order per node is preserved. An event for a node
//! the tree no longer has (it was deleted by a later batch than the one the
//! renderer saw; tree ids are never reused) is dropped.

use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use native_shell_core::desc::{Desc, EntityId};
use native_shell_core::host::{Environment, Host, HostRegistry, LogLevel};
use native_shell_core::manifest::{Capabilities, Manifest};
use native_shell_core::protocol::{EventType, ROOT_NODE};
use native_shell_core::style::{Resolved, StyleCache};
use native_shell_services::bundle::VerifiedBundle;
use native_shell_services::{Services, ServicesConfig, Waker};
use native_shell_tree::Tree;
use rquickjs::{CatchResultExt, Context, Ctx, Function, Module, Object, Persistent, Runtime, Value};

mod frame;

use crate::assets::Assets;
use crate::error::{script_error, Error, ScriptError};
use crate::loader::{self, Program, ProgramRef, ShellLoader, ShellResolver};
use crate::natives::{self, Clock, Shared, SharedRef, ThemeConfig};
use crate::sandbox::{self, Budget, Phase};

/// Resource ceilings of one instance.
#[derive(Debug, Clone, PartialEq)]
pub struct Limits {
    /// QuickJS heap. A leak surfaces as a catchable exception on the offending allocation.
    pub memory_bytes: usize,
    /// Interpreter stack: deep recursion becomes a `RangeError`, not a native overflow.
    pub stack_bytes: usize,
    /// One `render(cx)` (and one overlay content function).
    pub render_budget: Duration,
    /// One event handler, timer callback, host event or promise-job drain.
    pub event_budget: Duration,
    /// Evaluating the program's modules at load.
    pub module_budget: Duration,
}

impl Default for Limits {
    fn default() -> Limits {
        Limits {
            memory_bytes: 64 << 20,
            stack_bytes: 1 << 20,
            render_budget: Duration::from_millis(100),
            event_budget: Duration::from_millis(500),
            module_budget: Duration::from_secs(5),
        }
    }
}

pub struct InstanceConfig {
    pub app_id: String,
    /// The program's own directory (`localStorage`, `fs/promises`); `None` keeps storage in memory.
    pub data_dir: Option<PathBuf>,
    pub limits: Limits,
    /// The host's override of the manifest's capabilities (e.g. grant `demo/trade` to first-party programs only).
    pub capabilities: Option<Capabilities>,
    pub registry: HostRegistry,
    pub environment: Environment,
    /// Development mode: `eval`, the `Function` constructor and writable builtins. Never relaxes capabilities.
    pub dev: bool,
    /// The host's design tokens per appearance, on top of the defaults; `cx.set_theme` applies over them.
    pub theme: Option<ThemeConfig>,
    /// Pins `Date` for tests and screenshots: `Date.now()` and `new Date()`
    /// start at this Unix time (ms) and advance with the engine's time, so
    /// timers and dates agree. `None` reads the system clock.
    pub wall_clock: Option<f64>,
}

impl InstanceConfig {
    pub fn new(app_id: impl Into<String>) -> InstanceConfig {
        InstanceConfig {
            app_id: app_id.into(),
            data_dir: None,
            limits: Limits::default(),
            capabilities: None,
            registry: HostRegistry::default(),
            environment: Environment::default(),
            dev: false,
            theme: None,
            wall_clock: None,
        }
    }
}

/// Counters for tests, `check`, and host diagnostics. Serializes as snake_case JSON.
#[derive(Debug, Clone, Copy, PartialEq, Default, serde::Serialize)]
pub struct Metrics {
    pub renders: u64,
    pub render_errors: u64,
    /// Description nodes built by conversion (a clean entity costs none).
    pub converted_nodes: u64,
    pub batches: u64,
    pub events: u64,
    pub events_dropped: u64,
    pub events_coalesced: u64,
    pub timers_fired: u64,
    pub style_cache_hits: u64,
    pub style_cache_misses: u64,
    pub memory_used_bytes: i64,
    pub native_nodes: usize,
    pub entities: usize,
    /// Cumulative wall time, in nanoseconds, of the frame phases: script `render`
    /// calls, conversion to descriptions, tree commit (diff and layout), encoding.
    pub render_ns: u64,
    pub convert_ns: u64,
    pub commit_ns: u64,
    pub encode_ns: u64,
    /// Host measurement callbacks made during layout, and their cumulative time.
    /// Texts measured (each item of a batch counts), and `measure_text_batch` calls.
    pub measure_text_calls: u64,
    pub measure_text_batches: u64,
    pub measure_text_ns: u64,
    pub measure_component_calls: u64,
    pub measure_component_ns: u64,
    /// Cumulative time inside `tick` (timers, jobs, completions, events, frame).
    pub tick_ns: u64,
    /// Size of the last encoded batch.
    pub batch_bytes: u64,
    /// Whether every prelude module loaded from the bytecode embedded at build
    /// time (false when any fell back to parsing source).
    pub prelude_bytecode: bool,
    /// From the start of `Instance::new` to the first batch queued for the renderer.
    pub boot_ns: u64,
    /// Cumulative time in `dispatch_event` (handlers and their promise jobs).
    pub dispatch_ns: u64,
    /// Threaded runner: time messages waited in the channel before the JS thread took them.
    pub queue_wait_ns: u64,
    /// Threaded runner: time from a batch being finished to `Host::batch_ready`.
    pub handoff_ns: u64,
}

/// A surface as the prelude listed it.
enum SurfaceRecord {
    Page { key: String, entity: EntityId, title: String, can_go_back: bool, nav_bar_hidden: bool },
    Overlay { key: String, sheet: Option<Vec<String>>, dismissable: bool },
    Alert { key: String, spec: String },
}

impl SurfaceRecord {
    fn key(&self) -> &str {
        match self {
            SurfaceRecord::Page { key, .. } | SurfaceRecord::Overlay { key, .. } | SurfaceRecord::Alert { key, .. } => {
                key
            }
        }
    }
}

/// One frame's answer from the prelude's `surfaces()`.
struct SurfaceUpdate {
    records: Vec<SurfaceRecord>,
    /// Overlay contents evaluated this frame, converted (or the error).
    converted: Vec<(String, Result<Desc, String>)>,
    /// Lists found in overlay contents, with the entity that opened the overlay.
    owners: Vec<(String, EntityId)>,
}

pub struct Instance {
    // Field order is drop order: every JS handle before the context, the
    // context before the runtime (see also `Drop`).
    engine: Option<Persistent<Object<'static>>>,
    services: Option<Services>,
    context: Context,
    runtime: Runtime,
    shared: SharedRef,
    program: ProgramRef,
    assets: Assets,
    budget: Budget,
    host: Arc<dyn Host>,
    tree: Tree,
    styles: StyleCache,
    base_style: Rc<Resolved>,
    app_id: String,
    data_dir: Option<PathBuf>,
    capabilities_override: Option<Capabilities>,
    dev: bool,
    waker: Waker,
    loaded: bool,
    active: bool,

    root: Option<EntityId>,
    entities: HashMap<EntityId, Rc<Desc>>,
    list_owner: HashMap<String, EntityId>,
    /// Dev mode: per view or overlay, its keyed dual-form controls and whether each was drawn.
    dual_forms: HashMap<String, HashMap<String, bool>>,
    /// Dev mode: drawn controls already warned about for showing no state.
    warned_empty: HashSet<String>,
    surfaces: Vec<SurfaceRecord>,
    overlay_content: HashMap<String, Desc>,
    /// Render failures by owner (`e<id>` or an overlay key), newest last.
    errors: BTreeMap<String, (u64, String)>,
    error_serial: u64,
    /// The entry view never rendered: the root shows this instead.
    fatal: Option<ScriptError>,
    /// Page surfaces on screen, and those the user dismissed natively since the last frame.
    presented: HashSet<String>,
    native_dismissed: HashSet<String>,
    force_commit: bool,
    /// The `ThemeTokens` JSON last sent on the root, so an unchanged theme is not sent again.
    sent_theme_tokens: Option<String>,

    continuous: Vec<(u32, EventType, String)>,
    animated: HashSet<u32>,
    batches: VecDeque<Vec<u8>>,
    seq: u32,
    metrics: Metrics,
    /// When `Instance::new` started, for `Metrics::boot_ns`.
    created: std::time::Instant,
}

fn setup<E: std::fmt::Display>(e: E) -> Error {
    Error::Setup(e.to_string())
}

/// `{"a":1}` → JS value; invalid JSON becomes `undefined` and a log line.
fn parse_payload<'js>(ctx: &Ctx<'js>, host: &dyn Host, json: &str) -> Value<'js> {
    if json.is_empty() {
        return Value::new_undefined(ctx.clone());
    }
    match ctx.json_parse(json) {
        Ok(v) => v,
        Err(_) => {
            let _ = ctx.catch();
            host.log(LogLevel::Warn, &format!("native-shell: ignoring a payload that is not JSON: {json}"));
            Value::new_undefined(ctx.clone())
        }
    }
}

impl Instance {
    pub fn new(config: InstanceConfig, host: Arc<dyn Host>) -> Result<Instance, Error> {
        let created = std::time::Instant::now();
        let runtime = Runtime::new().map_err(setup)?;
        runtime.set_memory_limit(config.limits.memory_bytes);
        runtime.set_max_stack_size(config.limits.stack_bytes);
        let budget = Budget::new(config.limits.clone());
        runtime.set_interrupt_handler(Some(budget.interrupt_handler()));
        let program: ProgramRef =
            Rc::new(std::cell::RefCell::new(Program { registry: config.registry.clone(), ..Program::default() }));
        runtime.set_loader(ShellResolver(program.clone()), ShellLoader(program.clone()));
        let context = Context::full(&runtime).map_err(setup)?;

        let mut tree = Tree::new();
        tree.set_environment(&config.environment);
        let host_theme = config.theme.unwrap_or_default();
        host_theme.validate().map_err(Error::Setup)?;
        if let Some(m) = config.registry.modules.iter().find(|m| m.name.starts_with(loader::RESERVED_PREFIX)) {
            return Err(Error::Setup(format!(
                "host module `{}`: the `{}` prefix is reserved for built-in modules",
                m.name,
                loader::RESERVED_PREFIX
            )));
        }
        let shared = Rc::new(std::cell::RefCell::new(Shared::new(
            host.clone(),
            config.registry,
            config.environment,
            host_theme,
            Clock::real(),
        )));
        shared.borrow_mut().wall_clock = config.wall_clock.filter(|t| t.is_finite());
        if let Some(fg) = shared.borrow().theme.color("foreground") {
            tree.set_default_text_color(fg);
        }
        Ok(Instance {
            engine: None,
            services: None,
            context,
            runtime,
            shared,
            assets: Assets::new(program.clone(), config.data_dir.as_deref()),
            program,
            budget,
            host,
            tree,
            styles: StyleCache::default(),
            base_style: Rc::new(Resolved::base()),
            app_id: config.app_id,
            data_dir: config.data_dir,
            capabilities_override: config.capabilities,
            dev: config.dev,
            waker: Arc::new(|| {}),
            loaded: false,
            active: true,
            root: None,
            entities: HashMap::new(),
            list_owner: HashMap::new(),
            dual_forms: HashMap::new(),
            warned_empty: HashSet::new(),
            surfaces: Vec::new(),
            overlay_content: HashMap::new(),
            errors: BTreeMap::new(),
            error_serial: 0,
            fatal: None,
            presented: HashSet::new(),
            native_dismissed: HashSet::new(),
            force_commit: true,
            sent_theme_tokens: None,
            continuous: Vec::new(),
            animated: HashSet::new(),
            batches: VecDeque::new(),
            seq: 0,
            metrics: Metrics::default(),
            created,
        })
    }

    /// Called from background threads when `fetch` or a socket completes; set before loading.
    pub fn set_waker(&mut self, waker: Waker) {
        self.waker = waker;
    }

    /// Runs `files` (path → source) as the program `manifest` describes: evaluates the
    /// entry module, instantiates its default `View`, and queues the first batch.
    pub fn load_sources(&mut self, manifest: Manifest, files: BTreeMap<String, String>) -> Result<(), Error> {
        self.timed_load(manifest, files, None)
    }

    /// Like [`load_sources`](Self::load_sources) for a verified `.nspkg`; modules with bytecode load from it.
    pub fn load_bundle(&mut self, bundle: VerifiedBundle) -> Result<(), Error> {
        let mut sources = BTreeMap::new();
        for (path, bytes) in &bundle.files {
            if path.ends_with(".js") || path.ends_with(".mjs") {
                let source =
                    String::from_utf8(bytes.clone()).map_err(|_| Error::Bundle(format!("`{path}` is not UTF-8")))?;
                sources.insert(path.clone(), source);
            }
        }
        let manifest = bundle.manifest.clone();
        self.timed_load(manifest, sources, Some(Rc::new(bundle)))
    }

    /// Loads, then records whether the prelude came from bytecode.
    fn timed_load(
        &mut self,
        manifest: Manifest,
        files: BTreeMap<String, String>,
        bundle: Option<Rc<VerifiedBundle>>,
    ) -> Result<(), Error> {
        let r = self.load(manifest, files, bundle);
        let p = self.program.borrow();
        self.metrics.prelude_bytecode = p.prelude_bytecode > 0 && p.prelude_source == 0;
        r
    }

    fn load(
        &mut self,
        manifest: Manifest,
        files: BTreeMap<String, String>,
        bundle: Option<Rc<VerifiedBundle>>,
    ) -> Result<(), Error> {
        if self.loaded {
            return Err(Error::AlreadyLoaded);
        }
        self.loaded = true;
        let capabilities = self.capabilities_override.clone().unwrap_or_else(|| manifest.capabilities.clone());
        let entry = loader::normalize(&manifest.entry).ok_or_else(|| {
            Error::Manifest(native_shell_core::manifest::ManifestError::Entry(manifest.entry.clone()))
        })?;
        {
            let mut p = self.program.borrow_mut();
            p.sources = normalize_keys(files);
            p.bundle = bundle;
            p.capabilities = capabilities.clone();
            if !p.has(&entry) {
                return Err(Error::Module(ScriptError {
                    file: Some(entry.clone()),
                    line: None,
                    message: "the entry module is missing from the program".into(),
                }));
            }
        }
        self.shared.borrow_mut().capabilities = capabilities.clone();

        let mut config = ServicesConfig::new(self.app_id.clone(), self.data_dir.clone(), capabilities);
        config.waker = self.waker.clone();
        let services = Services::new(config);

        let shared = self.shared.clone();
        let dev = self.dev;
        let budget = self.budget.clone();
        let boot = self.context.with(|ctx| -> Result<(), String> {
            services.install(&ctx).catch(&ctx).map_err(|e| format!("services: {e}"))?;
            natives::install(&ctx, &shared).catch(&ctx).map_err(|e| format!("natives: {e}"))?;
            budget.start(Phase::Module);
            let result = import(&ctx, loader::BOOT).map(|_| ());
            budget.stop();
            result.map_err(|e| format!("prelude: {e}"))?;
            sandbox::install(&ctx, dev).catch(&ctx).map_err(|e| format!("sandbox: {e}"))
        });
        self.services = Some(services);
        boot.map_err(Error::Setup)?;
        self.engine = self.shared.borrow_mut().engine.take();
        if self.engine.is_none() {
            return Err(Error::Setup("the prelude did not register the engine".into()));
        }

        // The program.
        let budget = self.budget.clone();
        let program = self.program.clone();
        program.borrow_mut().refusal = None;
        let entry_class = self.context.with(|ctx| -> Result<Persistent<Value<'static>>, Error> {
            budget.start(Phase::Module);
            let ns = import(&ctx, &entry);
            budget.stop();
            let ns = ns.map_err(|mut e| {
                if let Some(trip) = budget.take_trip() {
                    e.message = trip;
                }
                match program.borrow_mut().refusal.take() {
                    Some(refusal) => Error::Capability(refusal),
                    None => Error::Module(e),
                }
            })?;
            let default: Value<'_> = ns.get("default").map_err(|e| {
                Error::Module(ScriptError { file: Some(entry.clone()), line: None, message: e.to_string() })
            })?;
            Ok(Persistent::save(&ctx, default))
        })?;
        self.drain_jobs();

        let mounted = self.call(Phase::Event, "mount", |ctx, f| {
            let cls = entry_class.clone().restore(ctx)?;
            f.call::<_, f64>((cls,))
        });
        match mounted {
            Ok(id) => self.root = Some(id as EntityId),
            Err(e) => {
                self.fatal = Some(e.clone());
                self.host.error(&format!("native-shell: {e}"));
            }
        }
        self.drain_jobs();
        self.frame();
        match &self.fatal {
            Some(e) => Err(Error::FirstRender(e.clone())),
            None => Ok(()),
        }
    }

    /// Screen size, safe area, appearance, font scale… Relayout and re-render on the next tick.
    pub fn set_environment(&mut self, env: Environment) {
        let font_changed;
        {
            let mut s = self.shared.borrow_mut();
            if s.env == env {
                return;
            }
            font_changed = s.env.font_scale != env.font_scale;
            s.env = env.clone();
            if let Ok(theme) = s.theme_spec.theme(&env, &s.host_theme) {
                if theme != s.theme {
                    s.theme = theme;
                    s.theme_changed = true;
                }
            }
        }
        self.tree.set_environment(&env);
        if font_changed {
            self.tree.clear_measure_cache();
        }
        self.invalidate_all();
    }

    /// An event from the renderer. `seq` is the last batch the renderer had applied.
    pub fn dispatch_event(&mut self, node: u32, event: EventType, payload_json: &str, seq: u32) {
        let started = std::time::Instant::now();
        self.dispatch(node, event, payload_json, seq);
        self.metrics.dispatch_ns += started.elapsed().as_nanos() as u64;
    }

    fn dispatch(&mut self, node: u32, event: EventType, payload_json: &str, seq: u32) {
        self.metrics.events += 1;
        let parsed: serde_json::Value = serde_json::from_str(payload_json).unwrap_or(serde_json::Value::Null);
        let dragging = event == EventType::Change && parsed.get("final") == Some(&serde_json::Value::Bool(false));
        if event.is_continuous() || dragging {
            if let Some(slot) = self.continuous.iter_mut().find(|(n, e, _)| *n == node && *e == event) {
                slot.2 = payload_json.to_string();
                self.metrics.events_coalesced += 1;
            } else {
                self.continuous.push((node, event, payload_json.to_string()));
            }
            return;
        }
        self.flush_continuous(Some(node));
        self.deliver(node, event, payload_json, &parsed, seq);
        self.drain_jobs();
    }

    /// The answer to an async host-module call (`Host::call_async`).
    pub fn host_respond(&mut self, request_id: u64, ok: bool, json: &str) {
        let r = self.call(Phase::Task, "respond", |_, f| f.call::<_, ()>((request_id as f64, ok, json)));
        self.log_failure("host response", r);
        self.drain_jobs();
    }

    /// An event a host module pushes to its `on(event, fn)` listeners.
    pub fn host_emit(&mut self, module: &str, event: &str, payload_json: &str) {
        let host = self.host.clone();
        let r = self.call(Phase::Task, "emit", |ctx, f| {
            let payload = parse_payload(ctx, &*host, payload_json);
            f.call::<_, ()>((module, event, payload))
        });
        self.log_failure("host event", r);
        self.drain_jobs();
    }

    /// Replaces the clock scripts read between ticks ([`Clock::real`] by
    /// default), e.g. with a manual one in tests. Time stays where it was.
    pub fn set_clock(&mut self, clock: Clock) {
        self.shared.borrow_mut().time.set_clock(clock);
    }

    /// Advances the clock to `now_ms` (the host's time base; never backwards) and
    /// does everything due; returns the delay until the next timer.
    pub fn tick(&mut self, now_ms: f64) -> Option<f64> {
        let started = std::time::Instant::now();
        let next = self.tick_inner(now_ms);
        self.metrics.tick_ns += started.elapsed().as_nanos() as u64;
        next
    }

    fn tick_inner(&mut self, now_ms: f64) -> Option<f64> {
        // Timer callbacks never see a time before one already observed.
        let floor = self.shared.borrow().time.last();
        let now_ms = self.shared.borrow_mut().time.tick(now_ms);
        if !self.loaded {
            return None;
        }
        if self.active {
            // Bounded so a zero-interval timer cannot starve the frame. Each
            // callback runs with the clock at its own due time (never going
            // back), so a timer it starts is due relative to that moment and is
            // picked up by this same loop when that is still `now_ms` or earlier:
            // a tick that arrives late, or a test jumping ahead, sees the same
            // timeline as one ticking at every due time.
            for _ in 0..10_000 {
                let due = self.shared.borrow_mut().timers.take_due(now_ms);
                let Some((id, at)) = due else { break };
                self.shared.borrow_mut().time.pin(Some(at.clamp(floor.min(now_ms), now_ms)));
                self.metrics.timers_fired += 1;
                let r = self.call(Phase::Task, "fire_timer", |_, f| f.call::<_, ()>((id,)));
                self.log_failure("timer", r);
                self.drain_jobs();
            }
            self.shared.borrow_mut().time.pin(None);
        }
        if let Some(services) = &self.services {
            let delivered = self.context.with(|ctx| services.poll(&ctx));
            if delivered > 0 {
                self.drain_jobs();
            }
        }
        self.flush_continuous(None);
        self.drain_jobs();
        if !self.active {
            return None;
        }
        self.frame();
        self.shared.borrow().timers.next_due().map(|due| (due - now_ms).max(0.0))
    }

    /// The oldest queued batch, encoded for the renderer.
    pub fn take_batch(&mut self) -> Option<Vec<u8>> {
        self.batches.pop_front()
    }

    /// Whether something asked for a frame (`cx.notify`, a queued command…) since the last one.
    pub fn needs_frame(&self) -> bool {
        let s = self.shared.borrow();
        s.frame_requested || s.theme_changed || !self.continuous.is_empty()
    }

    /// Backgrounded (`false`): timers and frames pause, storage is flushed. Foreground resumes them.
    pub fn set_active(&mut self, active: bool) {
        if self.active == active {
            return;
        }
        self.active = active;
        if active {
            self.force_commit = true;
            self.shared.borrow_mut().frame_requested = true;
        } else if let Some(s) = &self.services {
            s.flush();
        }
    }

    /// Memory pressure: collect garbage and drop caches that rebuild on demand.
    pub fn trim_memory(&mut self) {
        self.styles.clear();
        self.tree.clear_measure_cache();
        self.runtime.run_gc();
    }

    pub fn tree(&self) -> &Tree {
        &self.tree
    }

    /// Every counter, including `memory_used_bytes`, which walks the whole JS
    /// heap; keep it off hot paths.
    pub fn metrics(&self) -> Metrics {
        Metrics { memory_used_bytes: self.runtime.memory_usage().memory_used_size, ..self.metrics_fast() }
    }

    /// The counters that cost nothing to read: all but `memory_used_bytes`
    /// (0 here). What the runner publishes after every tick.
    pub fn metrics_fast(&self) -> Metrics {
        Metrics {
            style_cache_hits: self.styles.hits,
            style_cache_misses: self.styles.misses,
            entities: self.entities.len(),
            ..self.metrics
        }
    }

    /// Runner timings: how long a message waited in the channel, and how long a
    /// finished batch waited before `Host::batch_ready`.
    pub(crate) fn note_runner(&mut self, queue_wait_ns: u64, handoff_ns: u64) {
        self.metrics.queue_wait_ns += queue_wait_ns;
        self.metrics.handoff_ns += handoff_ns;
    }

    // -----------------------------------------------------------------------
    // JS calls
    // -----------------------------------------------------------------------

    /// Calls `engine[method]` under `phase`'s budget; a thrown error comes back located.
    fn call<R>(
        &self,
        phase: Phase,
        method: &str,
        f: impl for<'js> FnOnce(&Ctx<'js>, Function<'js>) -> rquickjs::Result<R>,
    ) -> Result<R, ScriptError> {
        let Some(engine) = self.engine.clone() else {
            return Err(ScriptError { file: None, line: None, message: "no program is loaded".into() });
        };
        let budget = &self.budget;
        self.context.with(|ctx| {
            let run = || -> rquickjs::Result<R> {
                let engine = engine.restore(&ctx)?;
                let func: Function<'_> = engine.get(method)?;
                f(&ctx, func)
            };
            budget.start(phase);
            let r = run();
            budget.stop();
            r.map_err(|e| match e {
                rquickjs::Error::Exception => script_error(&ctx.catch(), budget.take_trip()),
                other => {
                    ScriptError { file: None, line: None, message: budget.take_trip().unwrap_or(other.to_string()) }
                }
            })
        })
    }

    fn log_failure<R>(&self, what: &str, r: Result<R, ScriptError>) {
        if let Err(e) = r {
            self.host.log(LogLevel::Error, &format!("{what} failed: {e}"));
        }
    }

    /// Runs promise jobs until none is left, under the task budget.
    fn drain_jobs(&self) {
        self.budget.start(Phase::Task);
        loop {
            match self.runtime.execute_pending_job() {
                Ok(true) => {}
                Ok(false) => break,
                Err(e) => {
                    let trip = self.budget.take_trip();
                    let err = e.0.with(|ctx| script_error(&ctx.catch(), trip));
                    self.host.log(LogLevel::Error, &format!("promise job failed: {err}"));
                }
            }
        }
        self.budget.stop();
    }

    // -----------------------------------------------------------------------
    // Events
    // -----------------------------------------------------------------------

    fn flush_continuous(&mut self, node: Option<u32>) {
        let (now, later): (Vec<_>, Vec<_>) =
            std::mem::take(&mut self.continuous).into_iter().partition(|(n, _, _)| node.is_none_or(|x| x == *n));
        self.continuous = later;
        for (n, e, payload) in now {
            let parsed: serde_json::Value = serde_json::from_str(&payload).unwrap_or(serde_json::Value::Null);
            self.deliver(n, e, &payload, &parsed, self.seq);
        }
    }

    fn deliver(&mut self, node: u32, event: EventType, payload: &str, parsed: &serde_json::Value, seq: u32) {
        let surface = if node == ROOT_NODE {
            Some("root".to_string())
        } else {
            self.tree.surface_for_node(node).map(str::to_string)
        };
        let r = match (event, surface) {
            (EventType::RangeChange, _) => {
                self.range_change(node, parsed);
                return;
            }
            // Nothing focused listens: the key goes to the top surface's root view.
            (EventType::Key, _) if node == ROOT_NODE || self.tree.handler(node, EventType::Key).is_none() => {
                let host = self.host.clone();
                self.call(Phase::Event, "key_root", |ctx, f| {
                    let payload = parse_payload(ctx, &*host, payload);
                    f.call::<_, bool>((payload,)).map(|_| ())
                })
            }
            (EventType::BackPressed, surface) => {
                let key = surface.unwrap_or_else(|| "root".into());
                self.call(Phase::Event, "back", |_, f| f.call::<_, ()>((key,)))
            }
            // The size a sheet is presented at: lay its content out at it.
            (EventType::Resize, _) => {
                let dim = |k: &str| parsed.get(k).and_then(|v| v.as_f64()).map(|v| v as f32);
                if let (Some(w), Some(h)) = (dim("width"), dim("height")) {
                    if self.tree.set_surface_size(node, w, h) {
                        self.force_commit = true;
                        self.shared.borrow_mut().frame_requested = true;
                    }
                } else {
                    self.metrics.events_dropped += 1;
                }
                return;
            }
            (EventType::ToastAction, _) => {
                let id = parsed.get("id").and_then(|i| i.as_str()).unwrap_or_default().to_string();
                let r = self.call(Phase::Event, "toast_action", |_, f| f.call::<_, bool>((id.clone(),)));
                if matches!(r, Ok(false)) {
                    self.metrics.events_dropped += 1;
                    self.host.log(
                        LogLevel::Debug,
                        &format!("native-shell: dropped toast_action for toast {id:?}: no longer showing"),
                    );
                }
                r.map(|_| ())
            }
            (EventType::Dismiss, Some(key)) => {
                self.native_dismissed.insert(key.clone());
                self.call(Phase::Event, "dismissed", |_, f| f.call::<_, ()>((key,)))
            }
            (EventType::Action, Some(key)) => {
                let index = parsed.get("index").and_then(|i| i.as_u64()).unwrap_or(0) as u32;
                self.call(Phase::Event, "alert_action", |_, f| f.call::<_, ()>((key, index)))
            }
            _ => {
                let Some(hid) = self.tree.handler(node, event) else {
                    self.metrics.events_dropped += 1;
                    self.host.log(
                        LogLevel::Debug,
                        &format!("native-shell: dropped {} for node {node} (renderer at batch {seq}): no such node or handler", event.name()),
                    );
                    return;
                };
                if event == EventType::Change {
                    if let (Some(text), Some(count)) =
                        (parsed.get("text").and_then(|t| t.as_str()), parsed.get("count").and_then(|c| c.as_u64()))
                    {
                        self.tree.record_native_text(node, text, count as u32);
                    }
                }
                let host = self.host.clone();
                self.call(Phase::Event, "dispatch", |ctx, f| {
                    let payload = parse_payload(ctx, &*host, payload);
                    f.call::<_, bool>((hid, payload, event.script_name())).map(|_| ())
                })
            }
        };
        self.log_failure(&format!("{} handler", event.script_name()), r);
    }

    fn range_change(&mut self, node: u32, parsed: &serde_json::Value) {
        let Some(id) = self.tree.list_id(node).map(str::to_string) else {
            self.metrics.events_dropped += 1;
            return;
        };
        let first = parsed.get("first").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
        let end = parsed.get("end").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
        if self.tree.set_list_window(&id, first, end) {
            let window = self.tree.list_window(&id);
            self.shared.borrow_mut().list_windows.insert(id.clone(), window);
            if let Some(owner) = self.list_owner.get(&id).copied() {
                let r = self.call(Phase::Event, "invalidate", |_, f| f.call::<_, ()>((owner,)));
                self.log_failure("list range", r);
            }
        }
    }

    fn invalidate_all(&mut self) {
        self.force_commit = true;
        if self.engine.is_some() {
            let r = self.call(Phase::Task, "invalidate_all", |_, f| f.call::<_, ()>(()));
            self.log_failure("invalidate", r);
        }
        self.shared.borrow_mut().frame_requested = true;
    }
}

fn normalize_keys<T>(m: BTreeMap<String, T>) -> BTreeMap<String, T> {
    m.into_iter().filter_map(|(k, v)| loader::normalize(&k).map(|k| (k, v))).collect()
}

/// Imports a module by specifier and runs jobs until its evaluation settles; returns its namespace.
fn import<'js>(ctx: &Ctx<'js>, specifier: &str) -> Result<Object<'js>, ScriptError> {
    let located = |ctx: &Ctx<'js>| script_error(&ctx.catch(), None);
    let promise = Module::import(ctx, specifier).map_err(|e| match e {
        rquickjs::Error::Exception => located(ctx),
        other => ScriptError { file: Some(specifier.to_string()), line: None, message: other.to_string() },
    })?;
    loop {
        if let Some(result) = promise.result::<Object<'js>>() {
            return result.map_err(|e| match e {
                rquickjs::Error::Exception => located(ctx),
                other => ScriptError { file: Some(specifier.to_string()), line: None, message: other.to_string() },
            });
        }
        if !ctx.execute_pending_job() {
            return Err(ScriptError {
                file: Some(specifier.to_string()),
                line: None,
                message: "module evaluation did not finish (top-level await on something that never settles)".into(),
            });
        }
    }
}

impl Drop for Instance {
    fn drop(&mut self) {
        if let Some(s) = &self.services {
            s.shutdown();
        }
        self.engine = None;
        // Pending promise jobs hold JS values; run nothing more, just let them go
        // with the runtime. Services keeps its handle in runtime userdata.
        self.services = None;
    }
}
