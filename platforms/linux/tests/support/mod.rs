//! Shared support for the GTK integration tests.
//!
//! GTK must be initialized once per process, on the thread that then runs
//! every widget call, so each test file is a `harness = false` binary whose
//! `main` hands its cases to [`run`]:
//!
//! ```ignore
//! mod support;
//! use support::*;
//!
//! fn main() -> std::process::ExitCode {
//!     run(&[("renders a label", renders_a_label)])
//! }
//!
//! fn renders_a_label() {
//!     let f = Fixture::new(r#"
//!         import { View, div } from "native-shell";
//!         export default class App extends View {
//!           render() { return div().test_id("hello").child("Hello"); }
//!         }"#);
//!     f.expect("hello");
//! }
//! ```
//!
//! Unless `GDK_BACKEND=broadway` is already set, [`run`] starts a private
//! `gtk4-broadwayd`, so tests never open windows on the desktop and need no
//! X server in CI.
//!
//! The display comes with a minimal Broadway client ([`display`]) that
//! acknowledges every frame, so the frame clock, tick callbacks and
//! animations run at full rate, and reports a screen large enough for
//! phone-sized windows. The fixture's window has the surface's size, and the
//! fixture also lays the surface out itself ([`Fixture::layout`], part of
//! [`Fixture::settle`]) so a test never waits for a frame to see new frames.
//!
//! Screenshots under `screenshots/` are written there only with
//! `NSH_SCREENSHOTS=1`; otherwise they go to `target/screenshots/`, so a test
//! run never rewrites the committed images.

#![allow(dead_code)]

use std::cell::Cell;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::rc::Rc;
use std::time::{Duration, Instant};

pub use gtk::prelude::*;
use gtk::{gdk, glib, graphene};
use native_shell_core::protocol::{event_mask, EventType};
use native_shell_gtk::{Instance, InstanceOptions, Runtime, SurfaceView};

/// Phone-sized by default, like the other platforms' test surfaces.
pub const WIDTH: i32 = 390;
pub const HEIGHT: i32 = 844;

/// The manifest [`Fixture::new`] uses: every capability a test may need.
pub const MANIFEST: &str = r#"{
  "id": "dev.native-shell.test",
  "name": "Test",
  "version": "1.0.0",
  "entry": "main.js",
  "capabilities": { "storage": true, "clipboard": { "read": true, "write": true } }
}"#;

// MARK: Harness

pub mod display;

/// Starts GTK and libadwaita headless: on a private Broadway display unless
/// one was chosen, with temporary files kept off the small /tmp. Keep the
/// guard alive while GTK runs.
pub fn init() -> Option<display::Display> {
    if std::env::var_os("TMPDIR").is_none() {
        let dir = option_env!("CARGO_TARGET_TMPDIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("target/tmp"));
        let _ = std::fs::create_dir_all(&dir);
        std::env::set_var("TMPDIR", dir);
    }
    let display = display::start().expect("no Broadway display could start (is gtk4-broadwayd installed?)");
    gtk::init().expect("GTK cannot start");
    adw::init().expect("libadwaita cannot start");
    display
}

/// Runs the cases on the main thread. `cargo test --test x -- word` runs the cases whose name contains `word`.
pub fn run(cases: &[(&str, fn())]) -> ExitCode {
    let filters: Vec<String> = std::env::args().skip(1).filter(|a| !a.starts_with('-')).collect();
    let selected: Vec<_> = cases.iter().filter(|(name, _)| filters.is_empty() || filters.iter().any(|f| name.contains(f.as_str()))).collect();
    if std::env::args().any(|a| a == "--list") {
        for (name, _) in &selected {
            println!("{name}: test");
        }
        return ExitCode::SUCCESS;
    }
    let _display = init();

    println!("\nrunning {} tests", selected.len());
    let mut failed = Vec::new();
    for (name, case) in &selected {
        set_dark(false);
        let outcome = std::panic::catch_unwind(*case);
        pump_for(Duration::from_millis(10));
        match outcome {
            Ok(()) => println!("test {name} ... ok"),
            Err(_) => {
                println!("test {name} ... FAILED");
                failed.push(*name);
            }
        }
    }
    println!(
        "\ntest result: {}. {} passed; {} failed\n",
        if failed.is_empty() { "ok" } else { "FAILED" },
        selected.len() - failed.len(),
        failed.len()
    );
    if failed.is_empty() {
        ExitCode::SUCCESS
    } else {
        for name in failed {
            println!("    {name}");
        }
        ExitCode::FAILURE
    }
}

// MARK: Main loop

/// Runs every pending main-loop source without blocking.
pub fn pump() {
    let context = glib::MainContext::default();
    while context.iteration(false) {}
}

/// Runs the main loop for `duration` (timers, animations).
pub fn pump_for(duration: Duration) {
    let context = glib::MainContext::default();
    let deadline = Instant::now() + duration;
    while Instant::now() < deadline {
        if !context.iteration(false) {
            std::thread::sleep(Duration::from_millis(1));
        }
    }
}

/// Runs the main loop until `done` holds; false on timeout.
pub fn pump_until(timeout: Duration, mut done: impl FnMut() -> bool) -> bool {
    let context = glib::MainContext::default();
    let deadline = Instant::now() + timeout;
    loop {
        if done() {
            return true;
        }
        if Instant::now() >= deadline {
            return false;
        }
        if !context.iteration(false) {
            std::thread::sleep(Duration::from_millis(1));
        }
    }
}

/// Follows the system appearance switch the way a user's setting would.
pub fn set_dark(dark: bool) {
    let scheme = if dark { adw::ColorScheme::ForceDark } else { adw::ColorScheme::ForceLight };
    adw::StyleManager::default().set_color_scheme(scheme);
    pump();
}

/// A runtime with the example apps' accent (`primary` #2563eb light, #60a5fa
/// dark), as the iOS, Android and Linux example apps set it: screenshots of
/// example screens then compare with the other platforms'.
pub fn example_runtime() -> Runtime {
    let tokens = |hex: &str| std::collections::BTreeMap::from([("primary".to_string(), hex.to_string())]);
    let runtime = Runtime::new();
    runtime.set_theme(Some(native_shell_gtk::ThemeConfig { light: tokens("#2563eb"), dark: tokens("#60a5fa") })).expect("theme");
    runtime
}

// MARK: Fixture

/// A window with a surface rendering one program.
pub struct Fixture {
    pub window: gtk::Window,
    pub surface: SurfaceView,
    pub instance: Rc<Instance>,
    size: Cell<(i32, i32)>,
    /// The program's data directory, removed with the fixture.
    pub data_dir: tempfile::TempDir,
}

impl Fixture {
    /// Runs `main_js` as the program's `main.js` with [`MANIFEST`].
    pub fn new(main_js: &str) -> Fixture {
        Fixture::with_files(MANIFEST, &[("main.js", main_js)])
    }

    /// Runs a program given as a manifest and its files.
    pub fn with_files(manifest_json: &str, files: &[(&str, &str)]) -> Fixture {
        let files: BTreeMap<String, String> = files.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect();
        Fixture::start(|instance| instance.load_sources(manifest_json, files))
    }

    /// Runs a program directory, e.g. `../../examples/tasks` (relative to `platforms/linux`).
    pub fn open_dir(dir: impl AsRef<Path>) -> Fixture {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join(dir);
        Fixture::start(|instance| instance.load_dir(&dir))
    }

    fn start(load: impl FnOnce(&Instance) -> Result<(), String>) -> Fixture {
        Fixture::start_with(&Runtime::new(), InstanceOptions::default(), load)
    }

    /// Runs `files` with the host modules and components registered in `runtime`.
    pub fn with_runtime(runtime: &Runtime, manifest_json: &str, files: &[(&str, &str)]) -> Fixture {
        let files: BTreeMap<String, String> = files.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect();
        Fixture::start_with(runtime, InstanceOptions::default(), |instance| instance.load_sources(manifest_json, files))
    }

    /// The general constructor: a runtime, instance options (the data
    /// directory defaults to a fresh temporary one) and how to load the program.
    pub fn start_with(runtime: &Runtime, mut options: InstanceOptions, load: impl FnOnce(&Instance) -> Result<(), String>) -> Fixture {
        let data_dir = tempfile::tempdir().expect("temporary data directory");
        options.data_dir.get_or_insert_with(|| data_dir.path().to_path_buf());
        let surface = SurfaceView::new();
        let window = gtk::Window::builder().decorated(false).default_width(WIDTH).default_height(HEIGHT).child(&surface).build();
        window.present();
        pump();
        let instance = Rc::new(Instance::with_runtime("dev.native-shell.test", options, runtime).expect("instance"));
        surface.attach(instance.clone());
        let f = Fixture { window, surface, instance, size: Cell::new((WIDTH, HEIGHT)), data_dir };
        // The first environment carries the real size, so the first frame is laid out for it.
        f.layout();
        load(&f.instance).unwrap_or_else(|e| panic!("the program did not load: {e}"));
        f.settle();
        f
    }

    /// Lays the surface out at the fixture's size (see the module docs).
    pub fn layout(&self) {
        let (w, h) = self.size.get();
        self.surface.measure(gtk::Orientation::Horizontal, -1);
        self.surface.measure(gtk::Orientation::Vertical, w);
        self.surface.allocate(w, h, -1, None);
        pump();
    }

    /// Resizes the surface (the window size does not matter) and settles.
    pub fn resize(&self, width: i32, height: i32) {
        let (max_w, max_h) = display::SCREEN;
        assert!(
            width > 0 && height > 0 && width as u32 <= max_w && height as u32 <= max_h,
            "{width}x{height} does not fit the test display's {max_w}x{max_h} screen (support/display.rs)"
        );
        self.size.set((width, height));
        self.window.set_default_size(width, height);
        self.settle();
    }

    /// Waits until the JS thread has handled everything sent so far and every
    /// batch it produced is applied and laid out, repeating while applying
    /// batches makes the renderer send more (environment, measurements…).
    /// Timers due later are not waited for: use [`pump_for`] or [`Fixture::wait_for`].
    pub fn settle(&self) {
        for _ in 0..50 {
            // A new size reaches the core before the barrier below.
            self.layout();
            let before = self.surface.applied_batches();
            let Some(target) = self.instance.metrics() else { return };
            // The metrics answer comes before the tick that handles the same
            // messages; the snapshot published after that tick moves `tick_ns`.
            let ticked = pump_until(Duration::from_secs(5), || self.instance.metrics_snapshot().tick_ns > target.tick_ns);
            assert!(ticked, "the JS thread did not tick within 5 s");
            self.surface.drain();
            self.layout();
            if self.surface.applied_batches() == before {
                return;
            }
        }
        panic!("the program kept producing batches");
    }

    /// Settles, then waits up to 5 s (timers, host answers) for `test_id` to appear.
    pub fn wait_for(&self, test_id: &str) -> gtk::Widget {
        self.settle();
        let mut found = None;
        pump_until(Duration::from_secs(5), || {
            found = self.find(test_id);
            found.is_some()
        });
        assert!(found.is_some(), "no node with test id {test_id:?} appeared");
        // It may have come in a batch applied during the wait: lay it out
        // (and apply whatever followed) before handing it to a click.
        self.settle();
        self.expect(test_id)
    }

    /// The widget of the node with `test_id` (`.test_id(...)` in script), if mounted and visible in the tree.
    pub fn find(&self, test_id: &str) -> Option<gtk::Widget> {
        find_widget(self.window.upcast_ref(), &|w| w.widget_name() == test_id)
    }

    pub fn expect(&self, test_id: &str) -> gtk::Widget {
        self.find(test_id).unwrap_or_else(|| panic!("no node with test id {test_id:?}"))
    }

    /// The first widget (in tree order) that `matches`, from the window down.
    pub fn find_where(&self, matches: impl Fn(&gtk::Widget) -> bool) -> Option<gtk::Widget> {
        find_widget(self.window.upcast_ref(), &matches)
    }

    /// Bounds of `widget` in surface coordinates.
    pub fn bounds(&self, widget: &gtk::Widget) -> graphene::Rect {
        widget.compute_bounds(&self.surface).unwrap_or_else(|| panic!("{widget:?} is not inside the surface"))
    }

    /// Taps the node with `test_id` through GTK ([`click_widget`]), then settles.
    pub fn click(&self, test_id: &str) {
        // A 0×0 widget would take the press at its origin and lose the click.
        self.layout();
        let widget = self.expect(test_id);
        assert!(click_widget(&widget, self.surface.upcast_ref()), "nothing clickable at {test_id:?}");
        self.settle();
    }

    /// Long-presses the node with `test_id` through GTK ([`long_press_widget`]), then settles.
    pub fn long_press(&self, test_id: &str) {
        self.layout();
        let widget = self.expect(test_id);
        assert!(long_press_widget(&widget, self.surface.upcast_ref()), "nothing to long-press at {test_id:?}");
        self.settle();
    }

    /// Taps the node with `test_id` at the protocol level: sends `Click` to the
    /// nearest node listening for clicks, as the renderer would, then settles.
    /// Prefer [`Fixture::click`]; this one does not exercise the widget's gesture.
    pub fn tap(&self, test_id: &str) {
        self.layout();
        let node = self.node(test_id);
        let target = self.surface.listener(node, event_mask::CLICK).unwrap_or_else(|| panic!("nothing listens for clicks at {test_id:?}"));
        self.surface.send_event(target, EventType::Click, r#"{"x":1,"y":1,"count":1}"#);
        self.settle();
    }

    /// Sends an event to the node with `test_id`, then settles.
    pub fn send(&self, test_id: &str, event: EventType, payload_json: &str) {
        let node = self.node(test_id);
        self.surface.send_event(node, event, payload_json);
        self.settle();
    }

    /// The node id of `test_id`.
    pub fn node(&self, test_id: &str) -> u32 {
        let widget = self.expect(test_id);
        self.surface.node_of(&widget).unwrap_or_else(|| panic!("{test_id:?} is not a mounted node"))
    }

    /// Switches the appearance and settles (the program re-renders with the other theme).
    pub fn set_dark(&self, dark: bool) {
        set_dark(dark);
        self.settle();
    }

    /// Renders the surface offscreen into a PNG at `path` (relative paths are
    /// relative to `platforms/linux`), creating its directory.
    pub fn screenshot(&self, path: impl AsRef<Path>) -> PathBuf {
        self.before_render();
        screenshot(self.surface.upcast_ref(), path.as_ref())
    }

    /// One pixel of the surface, `0xAARRGGBB`.
    pub fn pixel(&self, x: i32, y: i32) -> u32 {
        self.before_render();
        pixel(self.surface.upcast_ref(), x, y)
    }

    /// A frame restyles widgets (appearance changes reach CSS then); the
    /// layout after it pins the surface at the fixture's size.
    fn before_render(&self) {
        next_frame(self.window.upcast_ref());
        self.layout();
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        self.window.destroy();
        pump();
    }
}

/// The widget of the node with `test_id` under `root`, for surfaces a [`Fixture`] does not own.
pub fn find_test_id(root: &gtk::Widget, test_id: &str) -> Option<gtk::Widget> {
    find_widget(root, &|w| w.widget_name() == test_id)
}

fn find_widget(root: &gtk::Widget, matches: &dyn Fn(&gtk::Widget) -> bool) -> Option<gtk::Widget> {
    if matches(root) {
        return Some(root.clone());
    }
    let mut child = root.first_child();
    while let Some(w) = child {
        if let Some(found) = find_widget(&w, matches) {
            return Some(found);
        }
        child = w.next_sibling();
    }
    None
}

/// Emits a full click where a finger on `widget`'s center would land: on a
/// `gtk::Button` or click gesture in the node's own content (not its child
/// nodes), else on the first click gesture at or above it, up to the surface.
/// False when there is none.
pub fn click_widget(widget: &gtk::Widget, surface: &gtk::Widget) -> bool {
    if let Some(target) = content_target(widget) {
        return emit_click(&target, widget);
    }
    let mut current = Some(widget.clone());
    while let Some(target) = current {
        if has_click(&target) {
            return emit_click(&target, widget);
        }
        if &target == surface {
            break;
        }
        current = target.parent();
    }
    false
}

fn has_click(widget: &gtk::Widget) -> bool {
    widget.is::<gtk::Button>() || click_gesture(widget).is_some()
}

/// The widget's click gesture for the primary button (or any button).
fn click_gesture(widget: &gtk::Widget) -> Option<gtk::GestureClick> {
    click_gesture_for(widget, |button| button == gdk::BUTTON_PRIMARY || button == 0)
}

fn click_gesture_for(widget: &gtk::Widget, button: impl Fn(u32) -> bool) -> Option<gtk::GestureClick> {
    let controllers = widget.observe_controllers();
    (0..controllers.n_items())
        .filter_map(|i| controllers.item(i).and_downcast::<gtk::GestureClick>())
        .find(|g| button(g.button()))
}

/// A long press where a finger on `widget`'s center would land: the secondary
/// click gesture (a desktop's context click) at or above it, up to the
/// surface, else holding the primary press past the long-press delay.
/// False when there is no gesture to press.
pub fn long_press_widget(widget: &gtk::Widget, surface: &gtk::Widget) -> bool {
    let center = graphene::Point::new(widget.width() as f32 / 2.0, widget.height() as f32 / 2.0);
    let at = |target: &gtk::Widget| {
        let p = widget.compute_point(target, &center).unwrap_or(center);
        (p.x() as f64, p.y() as f64)
    };
    let mut current = Some(widget.clone());
    while let Some(target) = current {
        if let Some(gesture) = click_gesture_for(&target, |b| b == gdk::BUTTON_SECONDARY) {
            let (x, y) = at(&target);
            gesture.emit_by_name::<()>("pressed", &[&1i32, &x, &y]);
            gesture.emit_by_name::<()>("released", &[&1i32, &x, &y]);
            return true;
        }
        if let Some(gesture) = click_gesture(&target) {
            let (x, y) = at(&target);
            gesture.emit_by_name::<()>("pressed", &[&1i32, &x, &y]);
            pump_for(LONG_PRESS_HOLD);
            gesture.emit_by_name::<()>("released", &[&1i32, &x, &y]);
            return true;
        }
        if &target == surface {
            break;
        }
        current = target.parent();
    }
    false
}

/// Longer than the renderer's 500 ms long-press delay.
const LONG_PRESS_HOLD: Duration = Duration::from_millis(650);

/// The first clickable widget inside `node`'s content, skipping child nodes.
fn content_target(node: &gtk::Widget) -> Option<gtk::Widget> {
    let mut child = node.first_child();
    while let Some(w) = child {
        if !w.is::<native_shell_gtk::NodeView>() {
            if has_click(&w) {
                return Some(w);
            }
            if let Some(found) = content_target(&w) {
                return Some(found);
            }
        }
        child = w.next_sibling();
    }
    None
}

fn emit_click(target: &gtk::Widget, origin: &gtk::Widget) -> bool {
    if let Some(button) = target.downcast_ref::<gtk::Button>() {
        button.emit_clicked();
        return true;
    }
    let Some(gesture) = click_gesture(target) else { return false };
    let center = graphene::Point::new(origin.width() as f32 / 2.0, origin.height() as f32 / 2.0);
    let point = origin.compute_point(target, &center).unwrap_or(center);
    let (x, y) = (point.x() as f64, point.y() as f64);
    gesture.emit_by_name::<()>("pressed", &[&1i32, &x, &y]);
    gesture.emit_by_name::<()>("released", &[&1i32, &x, &y]);
    true
}

/// Where a screenshot goes: paths under `screenshots/` (the committed images)
/// only with `NSH_SCREENSHOTS=1`, else under `target/screenshots/`. Relative
/// paths are relative to `platforms/linux`.
pub fn screenshot_path(path: &Path) -> PathBuf {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let path = path.strip_prefix(root).unwrap_or(path);
    match path.strip_prefix("screenshots") {
        Ok(rest) if std::env::var("NSH_SCREENSHOTS").as_deref() != Ok("1") => root.join("target/screenshots").join(rest),
        _ => root.join(path),
    }
}

/// Waits (up to a second) for the window of `widget` to draw one more frame.
pub fn next_frame(widget: &gtk::Widget) {
    let Some(clock) = widget.frame_clock() else { return };
    let before = clock.frame_counter();
    widget.queue_draw();
    clock.request_phase(gdk::FrameClockPhase::PAINT);
    pump_until(Duration::from_secs(1), || clock.frame_counter() > before);
}

/// Renders `widget` (inside a realized window) into a texture, as it is now.
pub fn render(widget: &gtk::Widget) -> gdk::Texture {
    let (w, h) = (widget.width(), widget.height());
    assert!(w > 0 && h > 0, "cannot render an unallocated widget");
    let snapshot = gtk::Snapshot::new();
    match widget.parent() {
        // Draws the widget at its own origin, as its parent would.
        Some(parent) => {
            let origin = widget.compute_point(&parent, &graphene::Point::zero()).unwrap_or_else(graphene::Point::zero);
            snapshot.translate(&graphene::Point::new(-origin.x(), -origin.y()));
            parent.snapshot_child(widget, &snapshot);
        }
        None => gtk::WidgetPaintable::new(Some(widget)).snapshot(&snapshot, w as f64, h as f64),
    }
    let node = snapshot.to_node().expect("the widget drew nothing");
    let renderer = widget.native().expect("the widget is not in a window").renderer().expect("no renderer");
    renderer.render_texture(&node, Some(&graphene::Rect::new(0.0, 0.0, w as f32, h as f32)))
}

/// Renders `widget` (inside a realized window) into a PNG at
/// [`screenshot_path`]`(path)`, and returns where it went.
pub fn screenshot(widget: &gtk::Widget, path: &Path) -> PathBuf {
    let path = screenshot_path(path);
    let texture = render(widget);
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).expect("screenshot directory");
    }
    texture.save_to_png(&path).unwrap_or_else(|e| panic!("cannot write {}: {e}", path.display()));
    path
}

/// Reads one pixel (`0xAARRGGBB`) of a rendered widget, for color assertions without image files.
pub fn pixel(widget: &gtk::Widget, x: i32, y: i32) -> u32 {
    let texture = render(widget);
    let mut downloader = gdk::TextureDownloader::new(&texture);
    downloader.set_format(gdk::MemoryFormat::B8g8r8a8);
    let (bytes, stride) = downloader.download_bytes();
    let i = y as usize * stride + x as usize * 4;
    let b = &bytes[i..i + 4];
    u32::from_be_bytes([b[3], b[2], b[1], b[0]])
}

/// Counts calls, for callbacks in tests.
#[derive(Default, Clone)]
pub struct Counter(Rc<Cell<usize>>);

impl Counter {
    pub fn bump(&self) {
        self.0.set(self.0.get() + 1);
    }

    pub fn get(&self) -> usize {
        self.0.get()
    }
}
