# native-shell: multi-agent implementation plan

This plan splits the build of native-shell into parallel work streams, one per
agent. Each stream owns a directory; nobody edits another stream's files. The
shared contracts below are fixed before the streams start, so streams can
proceed without waiting on each other, and are only changed by the lead.

Background: `docs/architecture.md` (architecture, choices, risks) and the
gpui-shell programming model this project mirrors on mobile
(gpui-kit's `crates/shell`, available locally at `~/work/gpui-kit/crates/shell`).

## Goals

1. **The runtime** (M0–M2 of the plan): JS in QuickJS (interpreter only) →
   description → keyed diff + flattening → taffy layout → binary mutation batch
   → UIKit / Android View. Events back with `seq`. Controlled inputs that keep
   IME composition intact. Virtualized lists. Pages, sheets, dialogs, toasts.
   Theme tokens, transitions, accessibility. Capabilities, signed bundles,
   several isolated instances per host.
2. **Host extension**: the app embedding native-shell registers
   * **host modules**: JS-importable modules answered by native code, such as
     data push over the app's own WebSocket, the app's HTTP client (with its
     auth and signing), storage, the current user, routing, analytics. Sync methods, async methods, and events
     pushed from host to script.
   * **host components**: the app's existing native views (a quote cell, a
     chart…) placed and laid out by scripts like any element, with props in and
     events out.
   Both are gated by the program's manifest (`capabilities.host_modules`,
   `capabilities.host_components`), which is how App Store 4.7.2 is honored:
   scripts reach host-defined, mediated services, never raw platform APIs.
3. **Developer experience** (M3): CLI `new / check / types / bundle / keygen /
   dev`, `.d.ts` generation including host modules and components, hot reload
   over LAN, remote logs.
4. **Automated verification at every layer**: Rust unit and integration tests
   (headless instance with a deterministic measurer), XCTest on the iOS
   simulator, JVM + instrumented tests on the Android emulator.

## Shared contracts (lead-owned, do not edit)

| Contract | Where |
| --- | --- |
| Mutation protocol: ops, view types, props, value kinds, events, commands, encoder and reference decoder, ABI hash | `crates/core/src/protocol.rs` |
| Style method vocabulary and resolution (`Resolved`: taffy style + visual + text) | `crates/core/src/style.rs` |
| Theme tokens | `crates/core/src/theme.rs` |
| Description tree (`Desc`, `ElementKind`, `Surface`, `CommitInput`) | `crates/core/src/desc.rs` |
| Host interfaces (`Measurer`, `Host`, `Environment`, `HostRegistry` and descriptors) | `crates/core/src/host.rs` |
| Manifest and capabilities | `crates/core/src/manifest.rs` |
| C ABI | `crates/ffi/include/native_shell.h` |
| JNI surface | "JNI" section below |
| Script API | "Script API" section below |

A stream that needs a contract change asks the lead (SendMessage to `main`)
rather than editing it.

## Streams

| # | Agent | Owns | Delivers | Depends on |
| --- | --- | --- | --- | --- |
| 1 | `tree` | `crates/tree/` | Reconciler, flattening, taffy layout, text and component measurement with cache, list windowing, native-child diffing, frame emission, `testing::FakeMeasurer` | core |
| 2 | `engine` | `crates/engine/` (package `native-shell`), `examples/` | QuickJS instance, sandbox, JS prelude (View, cx, builders, components), JS→`Desc` conversion, scheduler and timers, events and coalescing, input protocol, navigation and overlays, host modules and components on the script side, threaded runner, headless test harness, example programs | core, tree (API), services (API) |
| 3 | `services` | `crates/services/` | `localStorage` / `sessionStorage`, `fs/promises` in the data dir, `fetch`, `WebSocket`, `crypto`, `TextEncoder/Decoder`, `URL`, policy checks against `Capabilities`, signed `.nspkg` bundles (Ed25519, key rotation, revocation, min version, ABI hash), bytecode compile/load | core |
| 4 | `ffi-cli` | `crates/ffi/`, `crates/cli/`, `scripts/build-*.sh` | C ABI + JNI implementation with `catch_unwind`, xcframework and `.so` build scripts (16 KB pages), CLI `new / check / types / bundle / keygen / dev` | engine (API), services |
| 5 | `ios` | `platforms/ios/` | Swift Package `NativeShell`: decoder, mounting, all view types, TextKit measurement, controlled input, collection-view list, navigation, sheets, alerts, toasts, host module and component registration API, XCTest suite on the simulator, example app | protocol, C ABI |
| 6 | `android` | `platforms/android/` | Gradle project: `native-shell` AAR (decoder, `ShellViewGroup` mounting, all view types, `PrecomputedText` measurement, controlled `EditText`, `RecyclerView` list, page stack, bottom sheet, dialogs, Snackbar, host registration API), `native-shell-compose` artifact, JVM + instrumented tests, example app | protocol, JNI |

The lead (main session) integrates, reviews, runs the full test matrix, and
keeps the design record in the planning document's "native-shell 实现记录" page.

### Rules for every stream

* Stay inside your directory. Read anything; write only what you own.
* Build with your own target dir to avoid lock contention:
  `CARGO_TARGET_DIR=target/<agent> cargo …`.
* Keep your crate compiling at every step other streams might observe; land
  work in small compiling increments.
* Every feature comes with automated tests in the same change. "Done" means the
  stream's test command passes; report the command and its output.
* Match the code style of `crates/core`: module docs that explain *why*,
  comments only where the code cannot say it, no dead code, no `unwrap` on
  data that came from scripts or the platform.
* Do not commit; the lead commits.

## Script API

**Compatibility rule: gpui-shell is the reference.** Names, argument
conventions, handler signatures, defaults and error messages follow
gpui-shell; where this section and gpui-shell disagree, gpui-shell wins. The
style vocabulary and semantics are generated from GPUI itself
(`tools/style-gen` → `crates/core/src/style_table.rs`): `div()` is
`display: block` by default, `h_flex()` is `flex().flex_row().items_center()`,
`v_flex()` is `flex().flex_col()`, text defaults to 16 px with GPUI's φ
(≈1.618) line height, the numeric suffix scale is GPUI's (`p_4` = 1 rem =
16 px). Mobile-only additions (gestures, pages, sheets, host extension) are
additive and never change the meaning of a gpui-shell name.

Module `"native-shell"`, also importable as `"gpui-kit"`, `"gpui"`,
`"gpui-base"` for source compatibility with gpui-shell programs.

```js
import { View, div, h_flex, v_flex, text, span, link, image, shape, list, uniform_list,
  host_component, Button, Input, InputState, Textarea, TextareaState, Switch, Checkbox,
  Radio, Slider, Progress, Spinner, Tabs, Select, DatePicker } from "native-shell";

export default class Todo extends View {
  init(props, cx) {
    this.draft = InputState.new({ placeholder: "What needs doing?" });
    this.draft.on("submit", (_e, cx) => this.add(cx));
    this.items = JSON.parse(localStorage.getItem("items") ?? "[]");
  }
  render(cx) {
    const t = cx.theme();
    return v_flex().size_full().bg(t.colors.background).p_4().gap_2()
      .child(div().text_xl().font_bold().child("Todo"))
      .child(Input.new(this.draft).h_10().px_3().rounded_md().border_1().border_color("border"))
      .child(list("items", this.items.length, (i) => this.items[i].id,
        (i, cx) => this.row(this.items[i], cx)).flex_1());
  }
}
```

### Programming model (as gpui-shell)

* `class X extends View`; `init(props, cx)` once, `render(cx)` on
  invalidation, optional `update(props)`. Entry module `export default`s a
  View subclass.
* `cx.notify()` invalidates; repeated calls in one handler coalesce; calling it
  during render throws. Render output is a snapshot; nothing re-enters JS on
  frames.
* `cx.new(Class, props)` → Entity (`set_props`, `release`), its own
  invalidation boundary.
* Retained state objects (`InputState`, `TextareaState`) are created in
  `init` or handlers, never in render, and have `.on(event, fn)` / `release()`.
* `cx.spawn(async cx => …)`, `cx.sleep(ms)`, `cx.timer.after(ms, fn)` /
  `cx.timer.every(ms, fn)` returning `{cancel, is_done}`; also `setTimeout`,
  `setInterval`, `clearTimeout`, `clearInterval`, `queueMicrotask`.
* Call scopes `render / event / task / layout`: no notify, overlays or state
  creation during render; violations throw with a message naming the rule.
* Errors: a failing render keeps the last good frame and shows an error banner
  surface; failing on first render shows an error page.

### Elements

* `div()`, `h_flex()`, `v_flex()`; strings and numbers are text children;
  consecutive text children merge into one text leaf styled by inherited text
  style. `text(str | [span…])`, `span(str)` (style methods apply to the run),
  `link(url, label)`.
* Style methods: the vocabulary in `crates/core/src/style.rs` (nullary names
  from `nullary_names()`, parametric from `PARAM`). Unknown names throw with a
  "did you mean" hint. `.active(s => s.opacity(0.6))` is applied natively on
  press; `.hover`/`.focus` are accepted and ignored.
* Composition: `.child`, `.children`, `.when(cond, el => el)`,
  `.map(el => el)`, `.key(k)`, `.id(name)`.
* Events: `.on_click((e, cx) => …)` (`e.count`), `.on_double_click`,
  `.on_long_press`, `.on_press_in`, `.on_press_out`, `.on_pan`, `.on_pinch`,
  `.on_scroll`, `.on_key_down`, `.on(name, fn)` for host component events.
* Accessibility and testing: `.role(name)`, `.accessibility_label(s)`,
  `.accessibility_hint(s)`, `.accessibility_value(s)`, `.test_id(s)`,
  `.hit_slop(px)`.
* Animation: `.transition(prop, {duration, delay, easing})`,
  `.spring(prop, {response, damping})`; run natively.
* Scrolling: `overflow_y_scroll()` etc. make a native scroll view;
  `.on_refresh(fn)` + `.refreshing(bool)` add pull to refresh.
* `image(src)`: `https://…` (network capability) or a bundle-relative path;
  `.object_fit("cover"|"contain"|"fill"|"center")`, `.tint(color)`.
* `shape(path_d, {view_box})` with `.fill(color)`, `.stroke(color, width)`.
* `list(id, count, key_fn, (index, cx) => el)` and
  `uniform_list(id, count, (range, cx) => [el…])`; `.estimated_row_height(px)`;
  only the window the renderer reports (plus prefetch) is rendered.
  `cx.scroll_to_item(list_id, index, "top"|"center"|"bottom")`.
* `host_component(name, props)`: a native view registered by the host. Size it
  with style methods, or let the host measure it if it declared `measurable`.
  `.on(event, (payload, cx) => …)`.

### Components (behavior from gpui-base, native look)

| Constructor | Builder | Events |
| --- | --- | --- |
| `Button.new(id)` | `.label(s)` or children, `.disabled(b)`, `.variant("primary"\|"secondary"\|"outline"\|"ghost"\|"destructive")` | `on_click` |
| `Input.new(state)` / `Textarea.new(state)` | style methods for font and box; state: `placeholder, value, secure, keyboard ("default"\|"number"\|"decimal"\|"email"\|"phone"\|"url"), max_length, allowed_chars, return_key, content_type ("one_time_code"…), auto_capitalize, editable` | state `.on("change" \| "submit" \| "focus" \| "blur")`; `state.value`, `state.set_value(s)`, `state.focus()`, `state.blur()` |
| `Switch.new(id)`, `Checkbox.new(id)`, `Radio.new(id)` | `.checked(b)`, `.disabled(b)`, `.label(s)` | `on_change((checked, cx))` |
| `Slider.new(id)` | `.value(v)`, `.min`, `.max`, `.step` | `on_change((value, cx))` throttled while dragging, final value always delivered |
| `Progress.new()` / `Spinner.new()` | `.value(0..1)` / `.indeterminate()` | — |
| `Tabs.new(id)` | `.items([...])`, `.selected(i)` | `on_change((index, cx))` |
| `Select.new(id)` | `.items([{label, value}])`, `.value(v)`, `.placeholder(s)` (native menu) | `on_change((value, cx))` |
| `DatePicker.new(id)` | `.value("YYYY-MM-DD")`, `.mode("date"\|"time"\|"datetime")` | `on_change((value, cx))` |

### cx (context)

`notify, new, spawn, sleep, timer, theme(), set_theme({appearance, tokens}),
window() → {width, height, safe_area, keyboard_height, appearance, platform,
font_scale, locale}, focus(id), blur(), scroll_to(id, {x, y, animated}),
scroll_to_item(list, index, position), open_url(url), write_to_clipboard(s),
read_from_clipboard() → Promise<string>, haptic(kind), push_toast({title,
description, level, duration}), open_sheet(() => el, {detents, dismissable,
on_dismiss}), close_sheet(), open_dialog(() => el, {dismissable}),
close_dialog(), alert({title, message, actions: [{label, style:
"default"|"cancel"|"destructive", on_press}]}), navigator.push(Class, props,
{title, nav_bar_hidden}), navigator.pop(), navigator.can_go_back,
set_title(s), set_can_go_back(bool) / on_back(fn)`.

Overlay content functions are re-evaluated whenever their opener re-renders.

### Host modules (script side)

```js
import { quote, subscribe, on } from "demo/quote";   // a host-registered module
const q = await quote("700.HK");                   // async method → Promise
import { current_user } from "demo/user";
const u = current_user();                          // sync method → value
const sub = on("tick", (payload, cx) => { this.last = payload; cx.notify(); });
sub.cancel();
```

Every host module exports its methods plus `on(event, fn)`. Arguments are
passed as a JSON array; results and payloads are JSON. Importing a module the
manifest does not grant (when it `requires_permission`) fails at link time
with a message naming `capabilities.host_modules`.

### Standard library

`fetch`, `WebSocket`, `localStorage`, `sessionStorage`, `console`,
`TextEncoder`, `TextDecoder`, `URL`, `URLSearchParams`, `crypto.randomUUID`,
`crypto.getRandomValues`, `crypto.subtle.digest("SHA-256", …)`, timers, and
`import { readFile, writeFile, readdir, exists, unlink, mkdir, stat, rename }
from "fs/promises"` confined to the data directory. No `eval`, no `Function`
constructor (outside dev mode), frozen builtin prototypes.

## Engine internals the streams share

### `native-shell-tree` API (stream 1 provides, stream 2 consumes)

```rust
pub struct Tree { .. }
impl Tree {
    pub fn new() -> Tree;
    pub fn set_environment(&mut self, env: &Environment);   // relayout on next commit
    /// Diff, flatten, lay out every surface; returns the batch body (no Begin/EndBatch).
    pub fn commit(&mut self, input: &CommitInput, measurer: &dyn Measurer) -> Vec<Mutation>;
    pub fn handler(&self, node: u32, event: EventType) -> Option<HandlerId>;
    pub fn node_for_ref(&self, ref_id: &str) -> Option<u32>;
    /// The list a node belongs to and its id, for RangeChange events.
    pub fn list_id(&self, node: u32) -> Option<&str>;
    /// Current window of a list (default `[0, min(count, 20))`).
    pub fn list_window(&self, list_id: &str) -> (u32, u32);
    pub fn set_list_window(&mut self, list_id: &str, first: u32, last: u32) -> bool; // true if changed
    /// Native text changed: record it so the next diff does not echo it back.
    pub fn record_native_text(&mut self, node: u32, text: &str, count: u32);
    pub fn native_text_count(&self, node: u32) -> u32;
    pub fn clear_measure_cache(&mut self);                  // font scale / theme change
    // Inspection for tests and `check --print-tree`:
    pub fn find_by_test_id(&self, id: &str) -> Option<u32>;
    pub fn frame(&self, node: u32) -> Option<Frame>;        // absolute, in surface coordinates
    pub fn dump(&self) -> String;                           // indented native tree with types, frames, key props
    pub fn native_count(&self) -> usize;
    pub fn stats(&self) -> TreeStats;                       // nodes, natives, flattened, measure hits/misses
}
pub mod testing { pub struct FakeMeasurer; /* deterministic: 0.5em Latin, 1em CJK/emoji, line height 1.2em, word wrap */ }
```

Surfaces map to native nodes the tree creates under `ROOT_NODE` (0):
`Root` → a `View` sized to the environment; `Page` → `Page` node with
`Title`/`CanGoBack`/`NavBarHidden`; `Sheet` → `Sheet` with `Detents`/
`Dismissable`; `Dialog` → `Dialog`; `Alert` → `Dialog` with `AlertSpec` and no
children. Surface content is laid out at the environment size (sheets: width
only; height from content).

Flattening: a `View` with no painting (`Visual::paints()` false, no border,
no handlers, no a11y/test props, no `ref_id`, not hidden, not a scroll
container, not a surface root) is layout-only: no native view; its children
attach to the nearest native ancestor at the right index, frames offset by
the flattened ancestors. A node whose flatten status changes is recreated
together with its subtree (no reparenting).

Text leaves become `Text` nodes with `AttributedText` JSON
(`{runs:[{text,size,weight,color,family,italic,underline,strike,line_height,letter_spacing,url}],align,lines,truncate}`,
values resolved through inheritance, defaults size 16, weight 400, color
`foreground`). Measurement results are cached by `(json, width mode, width)`.

Input value protocol: `TextValue` JSON `{text, count}` where `count` is the
latest native event count the core had seen; renderers apply it only if
`count >=` their own counter and they are not composing, and skip it when the
text is equal.

Lists: the `List` node gets `ItemCount`, `RowOffsets` (estimated heights for
rows never rendered, measured heights otherwise, last element is the total)
and `Prefetch`; each rendered row is a `ListRow` child with `RowIndex`, laid
out as its own root at the list's width; its frame is `(0, 0, width,
height)` in the cell.

### Services API (stream 3 provides, stream 2 consumes)

```rust
pub struct Services { .. }
impl Services {
    pub fn new(cfg: ServicesConfig) -> Services; // app id, data dir, Capabilities, waker: Arc<dyn Fn() + Send + Sync>
    /// Installs globals (fetch, WebSocket, localStorage, sessionStorage, crypto, TextEncoder/Decoder, URL)
    /// and the hidden `__nsh_services` object the engine's `fs/promises` module re-exports.
    pub fn install<'js>(&self, ctx: &rquickjs::Ctx<'js>) -> rquickjs::Result<()>;
    /// Delivers completed background operations (fetch, WebSocket frames) on the JS thread.
    pub fn poll<'js>(&self, ctx: &rquickjs::Ctx<'js>) -> usize;
    pub fn has_pending(&self) -> bool;
    pub fn flush(&self);    // persist storage
    pub fn shutdown(&self); // close sockets, cancel requests
}
pub mod bundle {
    pub fn build(files, manifest, signing_key, key_id, opts) -> Result<Vec<u8>>;  // .nspkg
    pub fn open(bytes, trusted: &[TrustedKey], revoked: &[String], runtime_version) -> Result<VerifiedBundle>;
    pub struct VerifiedBundle { pub manifest: Manifest, pub files: BTreeMap<String, Vec<u8>>, pub bytecode: BTreeMap<String, Vec<u8>>, pub abi_hash: String, pub sha256: String }
}
```

### JNI

Kotlin `dev.nativeshell.NativeBridge` (object), implemented in
`crates/ffi/src/jni.rs` with the same semantics as the C ABI:

```kotlin
internal object NativeBridge {
  external fun create(callbacks: HostCallbacks, configJson: String): Long   // 0 on failure → lastError()
  external fun loadSources(handle: Long, json: String): Int
  external fun loadBundle(handle: Long, bytes: ByteArray): Int
  external fun setEnvironment(handle: Long, json: String)
  external fun dispatchEvent(handle: Long, node: Int, eventType: Int, payloadJson: String, seq: Int)
  external fun hostRespond(handle: Long, requestId: Long, ok: Boolean, json: String)
  external fun hostEmit(handle: Long, module: String, event: String, payloadJson: String)
  external fun tick(handle: Long, nowMs: Double): Long
  external fun takeBatch(handle: Long): Long                 // batch handle, 0 = none
  external fun batchBuffer(batch: Long): java.nio.ByteBuffer // direct, zero-copy, little-endian
  external fun freeBatch(batch: Long)
  external fun setActive(handle: Long, active: Boolean)
  external fun trimMemory(handle: Long)
  external fun metrics(handle: Long): String              // JSON, same as nsh_instance_metrics
  external fun destroy(handle: Long)                       // after it returns, no HostCallbacks call for this instance
  external fun lastError(): String
  external fun abiHash(): String
  external fun protocolVersion(): Int
}

interface HostCallbacks {                       // invoked on the JS thread
  fun measureText(attributedJson: String, width: Float, widthMode: Int): FloatArray      // [w, h, baseline]
  fun measureTextBatch(attributedJson: Array<String>, widths: FloatArray, widthModes: IntArray): FloatArray // 3 floats per item; parallel shaping allowed
  fun measureComponent(name: String, propsJson: String, width: Float, widthMode: Int): FloatArray? // [w, h] or null
  fun batchReady()
  fun callSync(module: String, method: String, argsJson: String): String  // throw to reject
  fun callAsync(requestId: Long, module: String, method: String, argsJson: String)
  fun log(level: Int, message: String)
  fun error(message: String)
}
```

Library name `native_shell` (`System.loadLibrary("native_shell")`).

## Host integration design

Apps that embed a script runtime usually already have a JS bridge for their
web views: a namespaced object registered with the bridge, methods that are
either synchronous (return a string) or asynchronous (take a completion
callback), and a way to push data back into the page. native-shell's host
extension deliberately mirrors that shape, so an existing bridge API can be
re-exposed with a thin adapter and teams keep their mental model.

### Registration API (platform side)

Registration happens once per process on a shared runtime object, before
instances start (instances see what was registered before their creation).
Every instance gets the registry as `HostRegistry` JSON in its config.

**iOS** (Swift; every public type is `@objc` so Objective-C apps can use it;
deployment target iOS 14):

```swift
@objc public protocol NSHHostModule: NSObjectProtocol {
  var name: String { get }                       // "demo/quote"
  var methods: [NSHHostMethod] { get }            // name + .sync/.async + optional TS signature
  @objc optional var events: [String] { get }
  @objc optional var requiresPermission: Bool { get }   // default true
  @objc optional var typeScriptDeclarations: String { get }
  @objc optional func callSync(_ method: String, args: [Any], context: NSHCallContext) throws -> Any?
  @objc optional func call(_ method: String, args: [Any], context: NSHCallContext, respond: NSHResponder)
}
@objc public final class NSHResponder: NSObject { func resolve(_ value: Any?); func reject(_ message: String) }   // exactly once, any thread
@objc public final class NSHCallContext: NSObject { let appId: String; func emit(_ event: String, _ payload: Any?) }
@objc public protocol NSHHostComponent: NSObjectProtocol {
  static var componentName: String { get }       // "demo-stock-cell"
  init()
  var view: UIView { get }
  func update(props: [String: Any])               // called with the full props on every change
  @objc optional func prepareForReuse()           // reset contract for pooling
  @objc optional static var events: [String] { get }
  @objc optional static func measure(props: [String: Any], maxWidth: CGFloat) -> CGSize   // presence ⇒ measurable; JS thread
  @objc optional var eventSink: NSHEventSink? { get set }   // set by the renderer; component calls sink.emit("select", payload)
}
NSHRuntime.shared.register(module:), .register(component:), .emit(module:event:payload:) // broadcast to instances listening
NSHRuntime.shared.themeProvider: (NSHAppearance) -> [String: UIColor]   // maps the app's design tokens to the shell color tokens
NSHRuntime.shared.imageLoader / urlOpener / logger hooks (defaults: URLSession + NSCache, UIApplication.open, os_log)
```

**Android** (Kotlin, Java-friendly: `@JvmStatic`, functional interfaces;
package `dev.nativeshell`):

```kotlin
interface HostModule {
  val name: String
  val methods: List<HostMethod>                  // HostMethod(name, MethodKind.SYNC|ASYNC, ts = null)
  val events: List<String> get() = emptyList()
  val requiresPermission: Boolean get() = true
  val typeScriptDeclarations: String? get() = null
  fun callSync(method: String, args: JSONArray, context: CallContext): Any? = throw UnsupportedOperationException()
  fun call(method: String, args: JSONArray, context: CallContext, responder: Responder) {}   // answer on any thread
}
interface HostComponent {                         // one instance per native view
  fun createView(context: Context): View
  fun update(view: View, props: JSONObject)
  fun reset(view: View) {}
  fun setEventSink(view: View, sink: EventSink) {}
}
class HostComponentSpec(val name: String, val events: List<String> = emptyList(),
  val measure: ((props: JSONObject, maxWidthPx: Float) -> SizeF)? = null, val requiresPermission: Boolean = true,
  val factory: () -> HostComponent)
NativeShell.registerModule(module), registerComponent(spec), emit(module, event, payload)
NativeShell.themeProvider = ThemeProvider { dark -> mapOf("primary" to color, …) }
NativeShell.imageLoader = ImageLoader { url, imageView -> … }   // apps plug Glide/Coil; default: HttpURLConnection + LruCache
```

### Semantics

* **Arguments and results** cross as JSON (`[Any]` / `JSONArray` in, JSON-able
  values out). Errors reject the script promise (async) or throw (sync) with
  the host's message.
* **Sync** methods run on the instance's JS thread and must be fast (reads of
  in-memory state such as the current user). Anything that touches the network
  or disk is async.
* **Async** responders may be answered on any thread, exactly once; responders
  still pending when an instance is destroyed are dropped and their promises
  never settle (the VM is gone).
* **Events** (`emit`) go to every live instance that imported the module and
  has listeners; payloads are JSON. Whenever a script's listener count for an
  event changes, the SDK calls the module's sync method `__listeners` with
  `[event, count]` (optional to implement; errors ignored), so a module that
  subscribes upstream (a push channel) can subscribe on the first listener and
  unsubscribe after the last, without polling.
* **Permissions**: modules and components that `requiresPermission` must be
  listed in the program's `capabilities.host_modules` /
  `capabilities.host_components`; the app may also pass an override
  `capabilities` per instance (e.g. grant a sensitive module only to
  first-party programs). This is how App Store guideline 4.7.2 is respected:
  programs reach app-defined, mediated services, never raw platform APIs.
* **Components** are pooled per name; `prepareForReuse` / `reset` must
  restore defaults. Props arrive whole on every change. Size: style first; if
  unsized and `measure` exists, the core asks for it during layout (JS thread)
  with the available width.
* **Theme**: apps map their design system to the shell color tokens via the theme
  provider (sent as `theme: {light, dark}` in the instance config);
  appearance changes re-render instances. Host components re-skin themselves.

### Distribution

* iOS: `NativeShell.xcframework` (Rust static lib, device + simulator slices)
  plus Swift sources, shipped as a Swift Package **and** a CocoaPods podspec
  (`NativeShell.podspec`, `vendored_frameworks`, `ios.deployment_target =
  '14.0'`) since many apps are CocoaPods-only.
* Android: AAR `dev.nativeshell:native-shell` (minSdk 24) with `arm64-v8a`,
  `armeabi-v7a`, `x86_64` `.so`s, 16 KB page aligned; optional
  `dev.nativeshell:native-shell-compose`. Built with AGP 8.6+, Kotlin 2.x,
  Java 17.

## Test matrix (the lead runs it before calling a stream done)

| Layer | Command |
| --- | --- |
| Rust (all crates) | `cargo test --workspace` |
| Cross builds | `scripts/build-ios.sh`, `scripts/build-android.sh` |
| iOS | `xcodebuild test -scheme NativeShell -destination 'platform=iOS Simulator,name=…'` in `platforms/ios/` |
| Android JVM | `./gradlew :native-shell:testDebugUnitTest` in `platforms/android/` |
| Android device | `./gradlew :native-shell:connectedDebugAndroidTest` on the emulator |
| Examples | `nshell check examples/*` |

## Rename to native-shell (done)

Decided by the user: the project became **native-shell** (desktop targets may
follow, so the name must not say "mobile"). The rename was done in one pass
after every stream had finished; nothing is kept under the old name (the
project was unreleased). The table records the old names for reference.

| What | From | To |
| --- | --- | --- |
| Crates | `mobile-shell`, `mobile-shell-{core,tree,services,ffi,cli}` | `native-shell`, `native-shell-{core,tree,services,ffi,cli}` (Rust paths `native_shell*`) |
| CLI binary, typings | `mobile-shell`, `mobile-shell.d.ts` | `native-shell`, `native-shell.d.ts` |
| Script import specifier | `"mobile-shell"` | `"native-shell"` (the `"gpui-kit"`, `"gpui"`, `"gpui-base"` aliases stay) |
| Manifest | `mobile-shell.json` | `native-shell.json` |
| Signed bundle | `.msp`, magic `MSPK` | `.nspkg`, magic `NSPK` |
| Batch buffer magic | `MSB1` | `NSB1` |
| C ABI | `mobile_shell.h`, `libmobile_shell`, `ms_*`, `MS*` types, `MS_*` macros | `native_shell.h`, `libnative_shell`, `nsh_*`, `NSH*`, `NSH_*` (`NS`/`ns_` are Apple's prefix) |
| Clang module | `MobileShellFFI` | `NativeShellFFI` |
| Swift package / ObjC types | `MobileShell`, `MS*` (`MSRuntime`, `MSHostModule`…) | `NativeShell`, `NSH*` (`NSHRuntime`, `NSHHostModule`…) |
| Android | package `dev.mobileshell`, AAR `dev.mobileshell:mobile-shell[-compose]`, `System.loadLibrary("mobile_shell")` | `dev.nativeshell`, `dev.nativeshell:native-shell[-compose]`, `"native_shell"` |
| Engine identity | `ENGINE_VERSION = "mobile-shell/…"` | `"native-shell/…"` (changes the ABI hash; fine before release) |
| Reserved module prefix | `ms/` | `nsh/` |
| Engine internals | prelude modules `ms:*`, globals `__ms_native` / `__ms_services`, test ids `ms-error-banner` / `ms-error-page` | `nsh:*`, `__nsh_native` / `__nsh_services`, `nsh-error-banner` / `nsh-error-page` |
| Prose | "mobile-shell", "mobile" as the platform class | "native-shell", "native" where it means "any native platform" |

Renaming the repository directory itself is left to the user.
