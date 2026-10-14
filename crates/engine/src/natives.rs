//! The native functions the prelude calls (`__nsh_native`, captured and deleted
//! from the global object by `js/core.js` before the program runs), and the
//! state they share with the instance.
//!
//! Every function borrows [`Shared`] for as long as it needs and never calls
//! back into JS while holding it; host calls happen after the borrow ends, so a
//! host that logs or measures cannot observe a held borrow.

use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap};
use std::rc::Rc;
use std::sync::Arc;

use native_shell_core::host::{Environment, Host, HostRegistry, LogLevel};
use native_shell_core::manifest::Capabilities;
use native_shell_core::protocol::Command;
use native_shell_core::style;
use native_shell_core::theme::{Appearance, Theme};
use native_shell_tree::DEFAULT_WINDOW;
use rquickjs::function::Opt;
use rquickjs::{Ctx, Exception, Function, Object, Persistent};
use serde::{Deserialize, Serialize};

/// Reserved host module whose sync `read` answers `cx.read_from_clipboard()`
/// (the protocol has no clipboard-read command; the platform SDK implements it).
pub(crate) const CLIPBOARD_MODULE: &str = "nsh/clipboard";

/// Where an imperative command goes, resolved to a node after the commit that may create it.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Target {
    Root,
    Ref(String),
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct PendingCommand {
    pub command: Command,
    pub target: Target,
    pub payload: String,
}

#[derive(Debug, Default)]
pub(crate) struct Timers {
    next_id: u32,
    entries: HashMap<u32, (f64, Option<f64>)>,
}

impl Timers {
    pub fn start(&mut self, now: f64, delay: f64, repeat: bool) -> u32 {
        self.next_id += 1;
        self.entries.insert(self.next_id, (now + delay, repeat.then_some(delay.max(1.0))));
        self.next_id
    }

    pub fn cancel(&mut self, id: u32) {
        self.entries.remove(&id);
    }

    pub fn next_due(&self) -> Option<f64> {
        self.entries.values().map(|(due, _)| *due).min_by(f64::total_cmp)
    }

    /// The earliest timer due at `now` (ties in creation order) and when it was
    /// due; repeating timers re-arm.
    pub fn take_due(&mut self, now: f64) -> Option<(u32, f64)> {
        let (&id, &(due, interval)) = self
            .entries
            .iter()
            .filter(|(_, (due, _))| *due <= now)
            .min_by(|a, b| a.1 .0.total_cmp(&b.1 .0).then(a.0.cmp(b.0)))?;
        match interval {
            Some(every) => {
                self.entries.insert(id, (due + every, Some(every)));
            }
            None => {
                self.entries.remove(&id);
            }
        }
        Some((id, due))
    }
}

/// A monotonic clock in milliseconds: what the instance reads whenever a script
/// asks for the time or starts a timer. [`Clock::real`] by default; tests pass
/// a manual one.
#[derive(Clone)]
pub struct Clock(Arc<dyn Fn() -> f64 + Send + Sync>);

impl Clock {
    pub fn new(read: impl Fn() -> f64 + Send + Sync + 'static) -> Clock {
        Clock(Arc::new(read))
    }

    /// Milliseconds since this clock was made, from `Instant`.
    pub fn real() -> Clock {
        let start = std::time::Instant::now();
        Clock::new(move || start.elapsed().as_secs_f64() * 1000.0)
    }

    pub fn read(&self) -> f64 {
        (self.0)()
    }
}

impl std::fmt::Debug for Clock {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Clock")
    }
}

/// The engine's time, in the host's time base (the `now_ms` it passes to
/// `tick`). Between ticks it runs on with the clock, so a timer started from a
/// tap after the instance idled is due `delay` after the tap, not after the
/// last tick. It never goes back: a host time behind it is ignored.
#[derive(Debug)]
pub(crate) struct EngineTime {
    clock: Clock,
    /// Host time and clock reading at the last tick.
    anchor: (f64, f64),
    last: f64,
    /// While a timer callback runs: its due time (see `Instance::tick`).
    pinned: Option<f64>,
}

impl EngineTime {
    pub fn new(clock: Clock) -> EngineTime {
        let at = clock.read();
        EngineTime { clock, anchor: (0.0, at), last: 0.0, pinned: None }
    }

    pub fn now(&mut self) -> f64 {
        if let Some(t) = self.pinned {
            return t;
        }
        let t = self.anchor.0 + (self.clock.read() - self.anchor.1);
        self.last = self.last.max(t);
        self.last
    }

    /// The host's tick at `host_now`; answers the engine time it stands for.
    pub fn tick(&mut self, host_now: f64) -> f64 {
        let t = if host_now.is_finite() { host_now.max(self.now()) } else { self.now() };
        self.anchor = (t, self.clock.read());
        self.last = t;
        t
    }

    pub fn set_clock(&mut self, clock: Clock) {
        // From the latest observed time: nothing has seen the old clock run on since.
        self.anchor = (self.last, clock.read());
        self.clock = clock;
    }

    /// The latest time anything observed (a script read, a timer start, a tick).
    pub fn last(&self) -> f64 {
        self.last
    }

    pub fn pin(&mut self, at: Option<f64>) {
        self.pinned = at;
    }
}

/// `cx.set_theme({appearance, tokens})` as the script last asked.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ThemeSpec {
    #[serde(default)]
    pub appearance: Option<Appearance>,
    #[serde(default)]
    pub tokens: BTreeMap<String, String>,
}

impl ThemeSpec {
    /// Defaults for the appearance, then the host's tokens for it, then the script's.
    pub fn theme(&self, env: &Environment, host: &ThemeConfig) -> Result<Theme, String> {
        let appearance = self.appearance.unwrap_or(env.appearance);
        let mut tokens = host.tokens(appearance).clone();
        tokens.extend(self.tokens.iter().map(|(k, v)| (k.clone(), v.clone())));
        Theme::with_overrides(appearance, &tokens)
    }
}

/// The host app's design tokens mapped onto the shell's, per appearance (hex colors).
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ThemeConfig {
    #[serde(default)]
    pub light: BTreeMap<String, String>,
    #[serde(default)]
    pub dark: BTreeMap<String, String>,
}

impl ThemeConfig {
    pub fn tokens(&self, appearance: Appearance) -> &BTreeMap<String, String> {
        match appearance {
            Appearance::Light => &self.light,
            Appearance::Dark => &self.dark,
        }
    }

    /// Checks every token name and color of both appearances.
    pub fn validate(&self) -> Result<(), String> {
        for appearance in [Appearance::Light, Appearance::Dark] {
            Theme::with_overrides(appearance, self.tokens(appearance)).map_err(|e| format!("theme: {e}"))?;
        }
        Ok(())
    }
}

pub(crate) struct Shared {
    pub host: Arc<dyn Host>,
    pub capabilities: Capabilities,
    pub registry: HostRegistry,
    pub env: Environment,
    pub theme: Theme,
    pub theme_spec: ThemeSpec,
    pub host_theme: ThemeConfig,
    pub theme_changed: bool,
    pub commands: Vec<PendingCommand>,
    pub timers: Timers,
    pub time: EngineTime,
    pub frame_requested: bool,
    pub next_request: u64,
    /// Each list's window as of the last commit or range change; read while rendering.
    pub list_windows: HashMap<String, (u32, u32)>,
    /// The prelude's engine object, until the instance takes it.
    pub engine: Option<Persistent<Object<'static>>>,
}

impl Shared {
    pub fn new(
        host: Arc<dyn Host>,
        registry: HostRegistry,
        env: Environment,
        host_theme: ThemeConfig,
        clock: Clock,
    ) -> Shared {
        let theme =
            ThemeSpec::default().theme(&env, &host_theme).unwrap_or_else(|_| Theme::default_for(env.appearance));
        Shared {
            host,
            capabilities: Capabilities::default(),
            registry,
            env,
            theme,
            theme_spec: ThemeSpec::default(),
            host_theme,
            theme_changed: false,
            commands: Vec::new(),
            timers: Timers::default(),
            time: EngineTime::new(clock),
            frame_requested: false,
            next_request: 0,
            list_windows: HashMap::new(),
            engine: None,
        }
    }

    pub fn request_id(&mut self) -> u64 {
        self.next_request += 1;
        self.next_request
    }

    /// `cx.window()`: the environment as scripts see it.
    pub fn window_json(&self) -> String {
        let e = &self.env;
        serde_json::json!({
            "width": e.width,
            "height": e.height,
            "scale": e.scale,
            "font_scale": e.font_scale,
            "safe_area": { "top": e.safe_area[0], "right": e.safe_area[1], "bottom": e.safe_area[2], "left": e.safe_area[3] },
            "keyboard_height": e.keyboard_height,
            "appearance": e.appearance,
            "platform": e.platform,
            "locale": e.locale,
        })
        .to_string()
    }
}

pub(crate) type SharedRef = Rc<RefCell<Shared>>;

fn borrow<'a, 'js>(ctx: &Ctx<'js>, shared: &'a SharedRef) -> rquickjs::Result<std::cell::RefMut<'a, Shared>> {
    shared.try_borrow_mut().map_err(|_| Exception::throw_internal(ctx, "native-shell: re-entrant native call"))
}

/// 0 not a style method, 1 nullary, 2 takes arguments, 3 both (`border`, `rounded`…).
fn style_kind(name: &str) -> u32 {
    static NULLARY: std::sync::OnceLock<std::collections::HashSet<String>> = std::sync::OnceLock::new();
    let nullary = NULLARY.get_or_init(|| style::nullary_names().into_iter().collect());
    u32::from(nullary.contains(name)) | (u32::from(style::PARAM.contains(&name)) << 1)
}

fn level(name: &str) -> LogLevel {
    match name {
        "debug" => LogLevel::Debug,
        "warn" => LogLevel::Warn,
        "error" => LogLevel::Error,
        _ => LogLevel::Info,
    }
}

/// Splits `https://host:port/path?q` into scheme, host and path.
fn split_url(url: &str) -> Option<(&str, &str, &str)> {
    let (scheme, rest) = url.split_once("://")?;
    let end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let authority = &rest[..end];
    let host = authority.rsplit_once('@').map_or(authority, |(_, h)| h);
    let host = host.split(':').next().unwrap_or(host);
    let path = &rest[end..];
    Some((scheme, host, if path.is_empty() { "/" } else { path }))
}

fn command(
    ctx: &Ctx<'_>,
    shared: &SharedRef,
    kind: String,
    target: Option<String>,
    payload: String,
) -> rquickjs::Result<()> {
    let mut s = borrow(ctx, shared)?;
    let (command, target) = match (kind.as_str(), target) {
        ("focus", Some(r)) => (Command::Focus, Target::Ref(r)),
        ("blur", Some(r)) => (Command::Blur, Target::Ref(r)),
        ("blur", None) => (Command::Blur, Target::Root),
        ("scroll_to", Some(r)) => (Command::ScrollTo, Target::Ref(r)),
        ("scroll_to_index", Some(r)) => (Command::ScrollToIndex, Target::Ref(r)),
        ("toast", None) => (Command::ShowToast, Target::Root),
        ("toast_dismiss", None) => (Command::DismissToast, Target::Root),
        ("haptic", None) => (Command::Haptic, Target::Root),
        ("clipboard", None) => {
            if !s.capabilities.clipboard.write {
                return Err(Exception::throw_type(
                    ctx,
                    "cx.write_to_clipboard is not granted; set capabilities.clipboard.write to true in native-shell.json",
                ));
            }
            (Command::SetClipboard, Target::Root)
        }
        ("open_url", None) => {
            if !s.capabilities.open_url {
                return Err(Exception::throw_type(
                    ctx,
                    "cx.open_url is not granted; set capabilities.open_url to true in native-shell.json",
                ));
            }
            (Command::OpenUrl, Target::Root)
        }
        (other, _) => return Err(Exception::throw_internal(ctx, &format!("native-shell: bad command `{other}`"))),
    };
    s.commands.push(PendingCommand { command, target, payload });
    s.frame_requested = true;
    Ok(())
}

pub(crate) fn install<'js>(ctx: &Ctx<'js>, shared: &SharedRef) -> rquickjs::Result<()> {
    let n = Object::new(ctx.clone())?;

    let s = shared.clone();
    n.set(
        "register",
        Function::new(ctx.clone(), move |ctx: Ctx<'js>, engine: Object<'js>| -> rquickjs::Result<()> {
            borrow(&ctx, &s)?.engine = Some(Persistent::save(&ctx, engine));
            Ok(())
        })?,
    )?;

    n.set("style_kind", Function::new(ctx.clone(), |name: String| style_kind(&name))?)?;

    n.set("style_suggest", Function::new(ctx.clone(), |name: String| style::suggest(&name))?)?;

    let s = shared.clone();
    n.set(
        "request_frame",
        Function::new(ctx.clone(), move |ctx: Ctx<'_>| -> rquickjs::Result<()> {
            borrow(&ctx, &s)?.frame_requested = true;
            Ok(())
        })?,
    )?;

    let s = shared.clone();
    n.set(
        "list_window",
        Function::new(ctx.clone(), move |ctx: Ctx<'_>, id: String| -> rquickjs::Result<Vec<u32>> {
            let (first, last) = borrow(&ctx, &s)?.list_windows.get(&id).copied().unwrap_or((0, DEFAULT_WINDOW));
            Ok(vec![first, last])
        })?,
    )?;

    let s = shared.clone();
    n.set(
        "timer_start",
        Function::new(ctx.clone(), move |ctx: Ctx<'_>, delay: f64, repeat: bool| -> rquickjs::Result<u32> {
            let mut sh = borrow(&ctx, &s)?;
            let now = sh.time.now();
            let delay = if delay.is_finite() { delay.max(0.0) } else { 0.0 };
            Ok(sh.timers.start(now, delay, repeat))
        })?,
    )?;

    let s = shared.clone();
    n.set(
        "now",
        Function::new(ctx.clone(), move |ctx: Ctx<'_>| -> rquickjs::Result<f64> { Ok(borrow(&ctx, &s)?.time.now()) })?,
    )?;

    let s = shared.clone();
    n.set(
        "timer_cancel",
        Function::new(ctx.clone(), move |ctx: Ctx<'_>, id: u32| -> rquickjs::Result<()> {
            borrow(&ctx, &s)?.timers.cancel(id);
            Ok(())
        })?,
    )?;

    let s = shared.clone();
    n.set(
        "command",
        Function::new(
            ctx.clone(),
            move |ctx: Ctx<'_>, kind: String, target: Option<String>, payload: String| -> rquickjs::Result<()> {
                command(&ctx, &s, kind, target, payload)
            },
        )?,
    )?;

    let s = shared.clone();
    n.set(
        "theme",
        Function::new(ctx.clone(), move |ctx: Ctx<'_>| -> rquickjs::Result<String> {
            Ok(borrow(&ctx, &s)?.theme.to_json().to_string())
        })?,
    )?;

    let s = shared.clone();
    n.set(
        "set_theme",
        Function::new(ctx.clone(), move |ctx: Ctx<'_>, json: String| -> rquickjs::Result<()> {
            let spec: ThemeSpec = serde_json::from_str(&json)
                .map_err(|e| Exception::throw_type(&ctx, &format!("cx.set_theme({{appearance, tokens}}): {e}")))?;
            let mut sh = borrow(&ctx, &s)?;
            let theme = spec
                .theme(&sh.env, &sh.host_theme)
                .map_err(|e| Exception::throw_type(&ctx, &format!("cx.set_theme: {e}")))?;
            sh.theme_spec = spec;
            if theme != sh.theme {
                sh.theme = theme;
                sh.theme_changed = true;
                sh.frame_requested = true;
            }
            Ok(())
        })?,
    )?;

    let s = shared.clone();
    n.set(
        "window",
        Function::new(ctx.clone(), move |ctx: Ctx<'_>| -> rquickjs::Result<String> {
            Ok(borrow(&ctx, &s)?.window_json())
        })?,
    )?;

    let s = shared.clone();
    n.set(
        "host_module",
        Function::new(ctx.clone(), move |ctx: Ctx<'_>, name: String| -> rquickjs::Result<String> {
            let sh = borrow(&ctx, &s)?;
            let m = sh
                .registry
                .module(&name)
                .ok_or_else(|| Exception::throw_reference(&ctx, &format!("unknown host module `{name}`")))?;
            Ok(serde_json::json!({ "methods": m.methods, "events": m.events }).to_string())
        })?,
    )?;

    let s = shared.clone();
    n.set(
        "host_component",
        Function::new(ctx.clone(), move |ctx: Ctx<'_>, name: String| -> rquickjs::Result<String> {
            let sh = borrow(&ctx, &s)?;
            let Some(c) = sh.registry.component(&name) else {
                let known: Vec<&str> = sh.registry.components.iter().map(|c| c.name.as_str()).collect();
                return Err(Exception::throw_reference(
                    &ctx,
                    &format!("unknown host component `{name}`; the host registered: {}", known.join(", ")),
                ));
            };
            if c.requires_permission && !sh.capabilities.allows_host_component(&name) {
                return Err(Exception::throw_type(
                    &ctx,
                    &format!("host component `{name}` is not granted; add it to capabilities.host_components in native-shell.json"),
                ));
            }
            Ok(serde_json::json!({ "events": c.events, "measurable": c.measurable }).to_string())
        })?,
    )?;

    let s = shared.clone();
    n.set(
        "host_call_sync",
        Function::new(
            ctx.clone(),
            move |ctx: Ctx<'_>, module: String, method: String, args: String| -> rquickjs::Result<String> {
                let host = borrow(&ctx, &s)?.host.clone();
                host.call_sync(&module, &method, &args)
                    .map_err(|e| Exception::throw_message(&ctx, &format!("{module}.{method}: {e}")))
            },
        )?,
    )?;

    let s = shared.clone();
    n.set(
        "host_call_async",
        Function::new(
            ctx.clone(),
            move |ctx: Ctx<'_>, module: String, method: String, args: String| -> rquickjs::Result<f64> {
                let (host, id) = {
                    let mut sh = borrow(&ctx, &s)?;
                    (sh.host.clone(), sh.request_id())
                };
                host.call_async(id, &module, &method, &args);
                Ok(id as f64)
            },
        )?,
    )?;

    let s = shared.clone();
    n.set(
        "host_listeners",
        Function::new(
            ctx.clone(),
            move |ctx: Ctx<'_>, module: String, event: String, count: u32| -> rquickjs::Result<()> {
                let host = borrow(&ctx, &s)?.host.clone();
                // Optional for the module to handle: it lets a quote module subscribe
                // upstream on the first listener and unsubscribe after the last.
                let _ = host.call_sync(&module, "__listeners", &serde_json::json!([event, count]).to_string());
                Ok(())
            },
        )?,
    )?;

    let s = shared.clone();
    n.set(
        "clipboard_read",
        Function::new(ctx.clone(), move |ctx: Ctx<'_>| -> rquickjs::Result<String> {
            let host = {
                let sh = borrow(&ctx, &s)?;
                if !sh.capabilities.clipboard.read {
                    return Err(Exception::throw_type(
                        &ctx,
                        "cx.read_from_clipboard is not granted; set capabilities.clipboard.read to true in native-shell.json",
                    ));
                }
                sh.host.clone()
            };
            // Synchronous, as gpui-shell's: the host answers with a JSON string or null.
            let json = host
                .call_sync(CLIPBOARD_MODULE, "read", "[]")
                .map_err(|e| Exception::throw_message(&ctx, &format!("cx.read_from_clipboard: {e}")))?;
            Ok(if json.is_empty() { "null".into() } else { json })
        })?,
    )?;

    let s = shared.clone();
    n.set(
        "check_image",
        Function::new(ctx.clone(), move |ctx: Ctx<'_>, src: String| -> rquickjs::Result<String> {
            let sh = borrow(&ctx, &s)?;
            if let Some((scheme, host, path)) = split_url(&src) {
                if !sh.capabilities.allows_http("GET", scheme, host, path) {
                    return Err(Exception::throw_type(
                        &ctx,
                        &format!("image(\"{src}\"): `{host}` is not granted; add it to capabilities.network.hosts in native-shell.json"),
                    ));
                }
                return Ok(src);
            }
            crate::loader::normalize(&src).filter(|p| !p.is_empty()).ok_or_else(|| {
                Exception::throw_type(&ctx, &format!("image(\"{src}\"): use an https:// URL or a path inside the program"))
            })
        })?,
    )?;

    let s = shared.clone();
    n.set(
        "log",
        Function::new(ctx.clone(), move |ctx: Ctx<'_>, lvl: String, message: Opt<String>| -> rquickjs::Result<()> {
            let host = borrow(&ctx, &s)?.host.clone();
            host.log(level(&lvl), message.0.as_deref().unwrap_or(""));
            Ok(())
        })?,
    )?;

    ctx.globals().set("__nsh_native", n)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timers_fire_in_due_order_and_repeat() {
        let mut t = Timers::default();
        let a = t.start(0.0, 100.0, false);
        let b = t.start(0.0, 50.0, true);
        assert_eq!(t.take_due(40.0), None);
        assert_eq!(t.take_due(100.0), Some((b, 50.0)));
        // Both due at 100: creation order breaks the tie.
        assert_eq!(t.take_due(100.0), Some((a, 100.0)));
        assert_eq!(t.take_due(100.0), Some((b, 100.0)));
        assert_eq!(t.take_due(100.0), None);
        assert_eq!(t.next_due(), Some(150.0));
        t.cancel(b);
        assert_eq!(t.next_due(), None);
    }

    #[test]
    fn urls_split_into_scheme_host_path() {
        assert_eq!(
            split_url("https://img.example.com:443/a.png?x=1"),
            Some(("https", "img.example.com", "/a.png?x=1"))
        );
        assert_eq!(split_url("https://example.com"), Some(("https", "example.com", "/")));
        assert_eq!(split_url("assets/logo.png"), None);
    }
}
