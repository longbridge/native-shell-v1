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
//! Broadway freezes a window's frame clock after its first frame until a
//! browser acknowledges it, and no browser ever connects here, so GTK never
//! lays the window out again by itself (and tick callbacks never run). The
//! fixture lays the surface out itself at a fixed size ([`Fixture::layout`],
//! part of [`Fixture::settle`]), which also frees tests from Broadway's
//! 1024×768 screen: the surface can be phone-sized whatever the window is.

#![allow(dead_code)]

use std::cell::Cell;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitCode};
use std::rc::Rc;
use std::time::{Duration, Instant};

pub use gtk::prelude::*;
use gtk::{gdk, glib, graphene};
use native_shell_core::protocol::{event_mask, EventType};
use native_shell_gtk::{Instance, InstanceOptions, SurfaceView};

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

struct Broadway(Child);

impl Drop for Broadway {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

/// Starts a private Broadway display unless the caller chose one.
fn start_display() -> Option<Broadway> {
    if std::env::var("GDK_BACKEND").as_deref() == Ok("broadway") {
        return None;
    }
    // Displays 100+ stay clear of the ones agents and developers pick by hand.
    let display = 100 + std::process::id() % 800;
    let runtime_dir = std::env::var("XDG_RUNTIME_DIR").unwrap_or_else(|_| std::env::temp_dir().display().to_string());
    let socket = PathBuf::from(runtime_dir).join(format!("broadway{}.socket", display + 1));
    let child = Command::new("gtk4-broadwayd")
        .arg(format!(":{display}"))
        .arg("--port")
        .arg((20000 + display).to_string())
        .arg("--address")
        .arg("127.0.0.1")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .expect("gtk4-broadwayd is needed to run the GTK tests (package libgtk-4-bin)");
    let guard = Broadway(child);
    let deadline = Instant::now() + Duration::from_secs(10);
    while !socket.exists() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(20));
    }
    std::env::set_var("GDK_BACKEND", "broadway");
    std::env::set_var("BROADWAY_DISPLAY", format!(":{display}"));
    std::env::remove_var("WAYLAND_DISPLAY");
    std::env::remove_var("DISPLAY");
    Some(guard)
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
    let _display = start_display();
    gtk::init().expect("GTK cannot start");
    adw::init().expect("libadwaita cannot start");

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

// MARK: Fixture

/// A window with a surface rendering one program.
pub struct Fixture {
    pub window: gtk::Window,
    pub surface: SurfaceView,
    pub instance: Rc<Instance>,
    size: Cell<(i32, i32)>,
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
        Fixture::start_with(InstanceOptions::default(), load)
    }

    /// Like the constructors above, with explicit instance options.
    pub fn start_with(options: InstanceOptions, load: impl FnOnce(&Instance) -> Result<(), String>) -> Fixture {
        let surface = SurfaceView::new();
        let window = gtk::Window::builder().decorated(false).child(&surface).build();
        window.present();
        pump();
        let instance = Rc::new(Instance::new("dev.native-shell.test", options).expect("instance"));
        surface.attach(instance.clone());
        let f = Fixture { window, surface, instance, size: Cell::new((WIDTH, HEIGHT)) };
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
        self.size.set((width, height));
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
        found.unwrap_or_else(|| panic!("no node with test id {test_id:?} appeared"))
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

    /// Taps the node with `test_id` through GTK: emits the press and release of
    /// the first click gesture at or above its widget (or `clicked` on a
    /// `gtk::Button`), at the widget's center, then settles.
    pub fn click(&self, test_id: &str) {
        let widget = self.expect(test_id);
        assert!(click_widget(&widget), "no click gesture at or above {test_id:?}");
        self.settle();
    }

    /// Taps the node with `test_id` at the protocol level: sends `Click` to the
    /// nearest node listening for clicks, as the renderer would, then settles.
    /// Prefer [`Fixture::click`]; this one does not exercise the widget's gesture.
    pub fn tap(&self, test_id: &str) {
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
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(path);
        screenshot(self.surface.upcast_ref(), &path);
        path
    }

    /// One pixel of the surface, `0xAARRGGBB`.
    pub fn pixel(&self, x: i32, y: i32) -> u32 {
        pixel(self.surface.upcast_ref(), x, y)
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        self.window.destroy();
        pump();
    }
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

/// Emits a full click on the first click gesture at or above `widget`; false when there is none.
pub fn click_widget(widget: &gtk::Widget) -> bool {
    let (w, h) = (widget.width() as f64, widget.height() as f64);
    let mut current = Some(widget.clone());
    while let Some(target) = current {
        if let Some(button) = target.downcast_ref::<gtk::Button>() {
            button.emit_clicked();
            return true;
        }
        let point = widget
            .compute_point(&target, &graphene::Point::new((w / 2.0) as f32, (h / 2.0) as f32))
            .map_or((w / 2.0, h / 2.0), |p| (p.x() as f64, p.y() as f64));
        let controllers = target.observe_controllers();
        for i in 0..controllers.n_items() {
            let Some(gesture) = controllers.item(i).and_downcast::<gtk::GestureClick>() else { continue };
            gesture.emit_by_name::<()>("pressed", &[&1i32, &point.0, &point.1]);
            gesture.emit_by_name::<()>("released", &[&1i32, &point.0, &point.1]);
            return true;
        }
        current = target.parent();
    }
    false
}

/// Renders `widget` (inside a realized window) into a texture.
pub fn render(widget: &gtk::Widget) -> gdk::Texture {
    pump();
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

/// Renders `widget` (inside a realized window) into a PNG.
pub fn screenshot(widget: &gtk::Widget, path: &Path) {
    let texture = render(widget);
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).expect("screenshot directory");
    }
    texture.save_to_png(path).unwrap_or_else(|e| panic!("cannot write {}: {e}", path.display()));
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
