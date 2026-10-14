# Linux (GTK 4)

`platforms/linux` renders native-shell programs with GTK 4 and libadwaita on
Linux desktops. Unlike iOS, Android and macOS it does not go through the C
ABI: the crate `native-shell-gtk` links the engine crate directly and applies
the same mutation protocol, so programs need no changes. `Environment.platform`
is `"linux"`. What the framework draws follows the iOS design language;
controls the OS provides (text input and IME, the select menu, calendar and
time fields, alerts) stay GTK-native.

## Building

* System packages: GTK ≥ 4.18 and libadwaita ≥ 1.7 with their development
  files (`libgtk-4-dev libadwaita-1-dev` on Ubuntu 26.04; Fedora 42 or newer),
  and `gtk4-broadwayd` (`libgtk-4-bin`) for the tests. The crate enables the
  gtk4-rs feature `v4_18` and the libadwaita feature `v1_7`.
* `platforms/linux` is its own Cargo workspace, so the root workspace (built
  on macOS and in the Android job) never needs the GTK libraries:
  `cd platforms/linux && cargo build`.
* `cargo run --example launcher` runs the example app (the repository's
  `examples/` with the demo host modules of `examples/watchlist/host.json`);
  `cargo run --example run -- <program dir>` opens one program.

## Mapping

| Protocol | GTK |
| --- | --- |
| Nodes | One `NodeView` widget per node: it allocates its children at the core's frames (taffy does all layout) and a native widget, when the node has one, at its own bounds |
| Coordinates | Frames are fractional points; a node is allocated at the rounded-up size, placed with a fractional translation and painted at its exact size, its edges snapped to device pixels in the parent's coordinates |
| `View` decorations | Painted in `snapshot`, not CSS: background, per-corner radii, per-side borders, linear gradients, outset shadows, clipping; identical on every GTK theme |
| `Transform` | A GSK transform about the frame's center |
| Transitions | `.transition(…)` / `.spring(…)` on frame and opacity run on GTK's frame clock; moves jump under Reduce Motion (`gtk-enable-animations` off) |
| `Text` | Pango, with the very configuration the JS-thread measurer used, laid out at exactly the frame width the core settled on; selectable text is a selectable `GtkLabel` with the same text and attributes |
| `TextInput` | A controlled `GtkText` (`visibility` off when `Secure`), a `GtkTextView` when `Multiline`; input purpose from the keyboard and content types; IME preedit is never overwritten by a stale value |
| `Image` | `http(s)`, file paths and `data:` URIs fetched and decoded off the main thread, downsampled to the node's pixels and cached decoded; cover, contain, fill and center modes |
| `Shape` | GSK paths from SVG path data, view box scaled like `xMidYMid meet`, round caps and joins |
| `Scroll` | `GtkScrolledWindow` around a content node of `ContentSize`; kinetic scrolling and overlay indicators are GTK's; `ScrollTo` animates with a critically damped spring |
| `List` | A scrolled content widget as tall as `RowOffsets`; rendered rows are placed at their offsets (the core recycles row nodes as `RangeChange` moves its window), placeholders for visible rows not rendered yet |
| `Switch`, `Checkbox` | Drawn as on iOS (system green when on unless tinted; checkbox or radio by `CheckStyle`) |
| `Slider` | Drawn as on iOS; `{value, final: false}` at most every 50 ms while dragging, `final: true` at the end; arrow keys step |
| `Progress`, `Spinner` | Drawn: a thin bar (a sliding segment when indeterminate), the iOS activity indicator |
| `Segmented` | Drawn: equal segments, an accent thumb sliding to the selection; each segment a focusable tab |
| `Select` | A drawn pop-up button opening a GTK popover with the items (check on the selection, destructive in red, disabled dimmed) |
| `DatePicker` | Drawn compact pills with the localized value; a popover with `GtkCalendar` for dates, spin buttons for times |
| `Page` | Pages of an `AdwNavigationView` (slide transitions, edge swipe on touch screens and touchpads, Escape / Alt+Left / mouse back button); programs draw their own headers, `Title` is for accessibility |
| `Sheet` | A bottom sheet over a dimmed backdrop: grabber, rounded top corners, `medium` / `large` / fixed detents, dragged between them and down to dismiss |
| `Dialog` | Content centered over a dimmed backdrop, scaling in like an iOS alert |
| `Dialog` with `AlertSpec` | `AdwAlertDialog` |
| Toasts | Drawn cards at the bottom of the surface, at most three, the level shown by a symbol, an optional text action |
| `HostComponent` | The widget of a registered `HostComponentType` (see Host API) |
| `OpenUrl` | `gtk::UriLauncher` |
| `SetClipboard`, `nsh/clipboard` | The display clipboard; reads come from a main-thread copy (GTK reads asynchronously, the core asks synchronously) |
| `Haptic` | Ignored: desktops have no haptic engine to drive |
| Accessibility | Roles, labels, hints (descriptions), values and states on `GtkAccessible`; a node's click is reachable as the `node.activate` action |

Built-in control measurement (`nsh/select`, `nsh/date-picker`) uses Pango
only, so it is safe on the JS thread.

## Pointer and keyboard

* Primary button or touch: `PressIn` / `PressOut`, press feedback, and
  `Click {x, y, count, modifiers}` with the count from GTK's multi-press; the
  second press of a double click sends `DoubleClick` instead to nodes
  listening for it.
* `LongPress`: held 500 ms without moving, or at once on a secondary click.
  `Pan` after a 4 pt drag; `Pinch` from touchscreen and touchpad pinches.
* Gestures are attached only for what a node listens to, and a node claims
  the touch sequence only when it fires, so a scrolled window above still
  scrolls when a drag starts on a clickable row.
* Hardware keys are observed in the capture phase on the window and sent
  once per down/up as `Key` to the nearest node with the KEY mask above the
  focused widget, else to the root; they are never consumed.

## Environment

* `width` / `height`: the surface widget's size; re-sent on every resize.
* `scale`: the surface's (fractional) scale.
* `font_scale`: the desktop's text scaling factor, read from `gtk-xft-dpi`
  (`dpi / (96 × 1024)`; GNOME's "Large Text" and text scaling); live.
* `appearance`: libadwaita's style manager (the desktop's dark style
  preference or the app's override); live, and the core answers with new
  `ThemeTokens`.
* `locale`: the first of GLib's language names as a BCP 47 tag
  (`de_CH.UTF-8` → `de-CH`).
* `safe_area` and `keyboard_height`: 0.
* Activity: the instance is paused (`set_active(false)`: timers and frames
  stop) while its surface is unmapped (its page covered, its window hidden)
  or its window is suspended (minimized), and resumed after.

## Host API

`Runtime` (`Runtime::shared()` or one of your own) is what instances are
created from, the counterpart of iOS's `NSHRuntime`:

* Host modules implement `HostModule`: a descriptor (methods, events,
  TypeScript), `call_sync` on the JS thread (also `__listeners [event,
  count]`) and `call` for async methods, answered with a `Send` `Responder`
  from any thread; a responder dropped unanswered rejects the call.
  `CallContext::emit` sends an event to the calling instance, `Runtime::emit`
  to all.
* Host components: `HostComponentType::new(name, |events| …)` makes a
  `HostComponent` (a GTK widget and `update(props)`) on the main thread;
  `events.emit(name, payload)` reaches the script's `.on(name, …)`; an
  optional `measure` runs on the JS thread during layout.
* `set_theme` (the app's tokens per appearance), `set_logger` (default:
  GLib's log), `add_trusted_key` / `revoke_bundle` for signed bundles.

`Instance::new(app_id, InstanceOptions)` takes the data directory (default
`$XDG_DATA_HOME/native-shell/<app id>`), capability and limit overrides and
dev mode; `load_dir`, `load_sources` or `load_bundle` load a program and
`SurfaceView::attach` renders it. `Instance::connect_error` reports
unrecoverable failures; the surface shows the engine's error page or banner.

## Hot reload with `nshell dev`

`nshell dev <dir>` serves a program over HTTP and prints the command to run
it on Linux. `DevClient::connect(url, runtime)` is the client:

* `cargo run --example run -- --dev http://HOST:PORT`, or the launcher's
  `--dev` flag and its "Connect to Dev Server" row.
* It fetches `/bundle` and runs the program in dev mode; a background thread
  long-polls `/version`, and on every change the program is reloaded in
  place: a new instance on a fresh `SurfaceView` in the same container, so
  the window stays (program state starts over).
* An error the server reports (the program cannot be read), a malformed
  bundle or an unreachable server is shown as the engine's error page; the
  next good version replaces it, and an unreachable server is retried every
  second.
* Script and renderer logs go to the server (`POST /log`, printed there as
  `[device <level>] …`) as well as to stderr; this replaces the runtime's
  logger.

`tests/dev.rs` runs the client against a small in-process server with the
same endpoints: load, reload in place, error page until fixed, unreachable
server, forwarded logs.

## Testing

`cd platforms/linux && cargo test`. Every GTK test file is a
`harness = false` binary driven by `tests/support/mod.rs` (GTK is
initialized once per process): a `Fixture` runs a program, settles on a
JS-thread barrier, finds nodes by test id, clicks through the real gesture or
at the protocol level, switches appearance, reads pixels and writes
screenshots. The crate's own GTK unit tests use `src/gtk_test.rs`.

* Tests never open windows on the desktop: unless `GDK_BACKEND=broadway` is
  set, `tests/support/display.rs` starts a private `gtk4-broadwayd`.
* GDK throttles Broadway windows to one frame per second while no browser is
  attached. The display therefore connects a minimal Broadway client that
  acknowledges each frame's roundtrip and reports a 1600 × 1600 screen, so
  frame clocks, animations and phone-sized windows behave as on a desktop.
* `example_runtime()` is a runtime with the example apps' accent (`primary`
  #2563eb light, #60a5fa dark, as the iOS, Android and Linux example apps set
  it); tests that screenshot example screens use it, so their images compare
  with the other platforms'.
* Screenshots written under `screenshots/` go to `target/screenshots/` unless
  `NSH_SCREENSHOTS=1` is set, so a test run never rewrites the committed
  images. `cargo run --example screenshot` renders every example to
  `screenshots/example-<name>-{light,dark}.png`.
* CI (`.github/workflows/ci.yml`, job `linux`) runs the tests and clippy in
  an Ubuntu 26.04 container on a Broadway display.

## Differences from iOS

* No haptics: `Haptic` is ignored.
* No navigation bar: pages are full screen and programs draw their own
  headers (as they do on iOS with the bar hidden); back is Escape, Alt+Left,
  the mouse back button or the edge swipe.
* Sheets span the full window width, also on wide windows.
* Switches keep the iOS system green when on, not the GNOME accent.
* Pull to refresh has no desktop control: it maps to pulling past the top
  edge with a touchpad or touch screen, and F5 / Ctrl+R with focus inside the
  scroll view.
* Alerts are `AdwAlertDialog`s, with GNOME's button layout.
* `safe_area` and `keyboard_height` are 0; the on-screen keyboard, when a
  desktop shows one, does not resize the surface.
