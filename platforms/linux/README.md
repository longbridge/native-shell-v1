# native-shell for Linux (GTK 4)

`native-shell-gtk` renders native-shell programs with GTK 4 and libadwaita on
Linux desktops. It is its own Cargo workspace (it needs the GTK development
libraries, which the root workspace must not require) and links the engine
crate directly, without the C ABI.

The core runs each program on its own JS thread and hands over binary mutation
batches; the crate applies them on the GTK main thread to a retained widget
tree. Every node is a `NodeView` that places its children at the frames taffy
computed and paints its own box decorations, so radii, borders and shadows
match the other platforms. Text is measured and drawn with Pango. What the
framework draws follows the iOS design language (see `AGENTS.md`); controls
the OS provides (text input and IME, date pickers, menus, dialogs) stay
GTK-native.

Requirements: GTK ≥ 4.18 and libadwaita ≥ 1.7 (`libgtk-4-dev`,
`libadwaita-1-dev`; Ubuntu 26.04, Fedora 42 or newer). The tests also need
`gtk4-broadwayd` (`libgtk-4-bin`).

## Embedding

```rust
use std::rc::Rc;
use native_shell_gtk::{Instance, InstanceOptions, Runtime, SurfaceView};

// Once, before creating instances: host modules, host components, theme.
Runtime::shared().register_module(MyQuotes::new())?;

let surface = SurfaceView::new();             // a gtk::Widget; put it in a window
let instance = Rc::new(Instance::new("org.example.app", InstanceOptions::default())?);
surface.attach(instance.clone());
instance.load_dir(Path::new("path/to/program"))?;   // or load_sources / load_bundle
```

* `Runtime` is the registry instances are created from (`Runtime::shared()`
  for the process, or your own). It mirrors the iOS `NSHRuntime`:
  * **Host modules** implement `HostModule`: a descriptor (methods, events,
    TypeScript), `call_sync` (on the JS thread; also receives `__listeners
    [event, count]`) and `call` for async methods, answered through a `Send`
    `Responder` from any thread. A responder dropped unanswered rejects the
    call. `CallContext::emit` pushes an event to the calling instance,
    `Runtime::emit` to all of them.
  * **Host components** are `HostComponentType::new(name, |events| …)`: the
    factory makes a `HostComponent` (a GTK widget plus `update(props)`) on the
    main thread; `events.emit(name, payload)` reaches the script's
    `.on(name, …)`. `.measure(|props, width| …)` runs on the JS thread during
    layout (Pango only, no GTK widgets) and makes the component measurable.
  * `set_theme` maps the app's design tokens onto the shell's, `set_logger`
    receives logs (default: GLib's log), `add_trusted_key` / `revoke_bundle`
    govern signed bundles.
* `InstanceOptions`: data directory (default `$XDG_DATA_HOME/native-shell/<app
  id>`), capability and limit overrides, dev mode. `Instance::connect_error`
  reports unrecoverable failures; the surface keeps showing the engine's
  error page or banner.
* The surface keeps the core's `Environment` current: size, the display's
  (fractional) scale, appearance from libadwaita's style manager, the
  desktop's text scaling factor (`gtk-xft-dpi`) and the locale, all live.
  Safe areas and keyboard height are zero on the desktop.
* `nsh/clipboard` reads come from a main-thread copy of the clipboard text,
  since GTK reads the clipboard asynchronously and the core asks
  synchronously. The root's `SetClipboard`, `OpenUrl` (`gtk::UriLauncher`)
  and `Haptic` (nothing to drive on desktops) are in `src/root_commands.rs`.

## Commands

```sh
cargo test                                   # unit + GTK integration tests
cargo clippy --all-targets -- -D warnings
cargo run --example launcher                 # the example app
cargo run --example launcher -- --example tasks --reset
cargo run --example run -- ../../examples/chat
cargo run --example screenshot [-- tasks …]  # screenshots/example-<name>-{light,dark}.png
cargo run --example launcher -- --screenshot screenshots
```

The example app lists the repository's `examples/` and opens each on its own
page. It registers what `examples/watchlist/host.json` declares
(`examples/common/host_demo.rs`): `demo/quote`, ticking only while a script
listens, `demo/user`, and the `demo-stock-cell` GTK widget, plus the example
apps' blue accent.

## Tests and Broadway

GTK runs on one thread per process, so every GTK test file is a
`harness = false` binary driven by `tests/support/mod.rs`: `run(&[("name",
case)])` starts GTK once and runs the cases (`cargo test --test host --
module` filters by name). `Fixture` loads a program (inline, from files or a
directory), settles on a JS-thread barrier, finds nodes by test id, clicks
through the GTK gesture or the protocol, switches appearance, reads pixels and
writes screenshots.

Tests never open windows on the desktop: unless `GDK_BACKEND=broadway` is set,
the harness starts a private `gtk4-broadwayd` (`tests/support/display.rs`,
shared with the crate's own GTK unit tests). GDK throttles Broadway windows to
one frame per second while no browser is attached, so the display comes with a
minimal client that acknowledges each frame and reports a large screen: frame
clocks, tick callbacks and animations run at full rate, and windows can be
phone-sized. The fixture still lays the surface out itself after each batch,
so tests never wait for a frame to see a change. CI runs the same tests in an
Ubuntu 26.04 container on a Broadway display (`.github/workflows/ci.yml`, job
`linux`).

Screenshots that tests write under `screenshots/` land in
`target/screenshots/` unless `NSH_SCREENSHOTS=1` is set, so a test run never
rewrites the committed images; set it to refresh them
(`NSH_SCREENSHOTS=1 cargo test`). `examples/screenshot` and the launcher's
`--screenshot` always write to `screenshots/`.
