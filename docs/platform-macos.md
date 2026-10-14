# macOS (AppKit)

`platforms/macos` renders native-shell programs with AppKit views in a Mac
app. It consumes the same C ABI (`crates/ffi/include/native_shell.h`) and the
same mutation protocol as iOS; programs need no changes. `Environment.platform`
is `"macos"`.

## Building

`scripts/build-macos.sh` builds the core for `aarch64-apple-darwin` and
`x86_64-apple-darwin` (into `target/macos`), lipos them into one universal
static library and wraps it as `platforms/macos/Frameworks/NativeShellFFI.xcframework`
with the Clang module `NativeShellFFI`, the module the iOS package imports.
The Swift package `NativeShellMac` picks it up, or a stub of the C ABI when it
has not been built.

## Mapping

| Protocol | AppKit |
| --- | --- |
| Coordinates | Every renderer view is flipped and layer-backed: the core's top-left frames are view frames unchanged |
| `View` decorations | Layer background, continuous corners (exact for circles and capsules), uniform borders on the layer, per-side borders and per-corner radii as shape layers, `CAGradientLayer`, layer shadow |
| `Transform` | Layer affine transform about the view's center (AppKit anchors layer-backed views at their origin, so the center is folded into the matrix) |
| `Text` | TextKit 1 (`NSLayoutManager`) drawing in the view, the same stack measurement uses; selectable text is a TextKit 1 `NSTextView` |
| `TextInput` | `NSTextField` (`NSSecureTextField` when `Secure`), a scrolling TextKit 1 `NSTextView` when `Multiline`; the system focus ring follows the program-drawn field's bounds and corners |
| `Image` | Layer contents with a contents gravity per content mode; ImageIO downsampling |
| `Shape` | `CAShapeLayer`, round caps and joins |
| `Scroll` | `NSScrollView` with a flipped document view sized by `ContentSize` |
| `List` | `NSScrollView` whose document is as tall as `RowOffsets`; rendered rows are placed at their offsets (the core already recycles row nodes as `RangeChange` moves its window); `ListAlignment` bottom opens at the end and keeps it in view while the user is there |
| `Switch` | `NSSwitch` |
| `Checkbox` | `NSButton` checkbox or radio style |
| `Slider` | `NSSlider` (continuous; changes throttled to 50 ms while dragging, final value on mouse up) |
| `Progress`, `Spinner` | `NSProgressIndicator` bar / spinning |
| `Segmented` | `NSSegmentedControl`, the accent as the selected bezel |
| `Select` | `NSPopUpButton`; a hidden item shows the placeholder before a choice |
| `DatePicker` | `NSDatePicker`, text field and stepper, system short formats |
| `Page` | A page stack in `NSHViewController`: pages slide in from the trailing edge (instantly with Reduce Motion) |
| `Sheet`, `Dialog` | Window sheets: sized to the largest detent (reported with `Resize`), else to their content |
| `Dialog` with `AlertSpec` | `NSAlert` as a sheet |
| Toasts | An in-window overlay at the bottom of the surface |
| `Haptic` | `NSHapticFeedbackManager` (Force Touch trackpads only) |
| `SetClipboard`, `nsh/clipboard` | `NSPasteboard.general` |
| `OpenUrl` | `NSWorkspace.open` |

Built-in control measurement (`nsh/select`, `nsh/date-picker`) uses the
system control font and chrome widths measured once on the main thread, so it
stays safe on the JS thread.

## Environment

* `width` / `height`: the surface view's size; resizing the window re-sends it.
* `scale`: the window's backing scale factor.
* `safe_area`: the controller view's `safeAreaInsets` (zero in a standard
  window; the title bar height in a full-size content view, so program headers
  sit below the traffic lights).
* `keyboard_height`: always 0.
* `font_scale`: 1. macOS has no system text size apps can read.
* `appearance`: from the view's effective appearance; switching between light
  and dark re-sends the environment, and the core answers with new
  `ThemeTokens`.
* `clipboard_feedback`: false.

## Pointer and keyboard

* A left click sends `Click` with `count` from the event (a second click
  within the double-click interval sends `DoubleClick` to nodes listening for
  it) and the real modifiers in `modifiers` (`{shift, ctrl, alt, cmd}`, the
  shape of `Key`'s). Clicks go to the innermost listening node; views that do
  not listen pass the mouse to their parent, as AppKit's responder chain does.
* Pressed feedback (`ActiveOpacity`, `ActiveBackground`, `PressIn` /
  `PressOut`) follows the pointer while the button is down: dragging out of the
  view shows the resting state again, as AppKit buttons do.
* `LongPress`: holding the button 500 ms, a secondary click, or a
  Control-click (the Mac's context gesture).
* `Pan`: dragging with the button down past 4 pt; `Pinch`: trackpad
  magnification.
* Hardware keys are observed with a local event monitor and sent once per
  down/up as `Key` to the nearest node with the KEY mask on the first
  responder's view chain (a text field's editor included), else to the root.
  Events are never consumed, so text fields type and menus still get their key
  equivalents. Key repeats are sent as further downs.
* Back: Command-[ (the view, or a Go menu item with the `goBack:` action) and
  the mouse's back button. A page whose `CanGoBack` is false gets
  `BackPressed` instead. Escape dismisses a dismissable sheet or dialog.
* Hover: `HoverOpacity` / `HoverBackground` apply while the pointer is over
  the node (pressed values win), tracked with an `NSTrackingArea` only on
  nodes that need it; nodes with the `HOVER` mask bit get `Hover
  {entered, x, y}`. `Cursor` maps GPUI's cursor styles to `NSCursor` (the
  diagonal resize cursors need macOS 15, the arrow stands in before);
  `Tooltip` is the view's `toolTip`.
* `HitSlop` and `Focusable` are accepted and ignored; `RefreshEnabled` too
  (there is no pull to refresh with a pointer).

## Differences from iOS

* Window sheets cannot be dragged between detents: a sheet with `Detents` is
  as tall as its largest one (within the window) and reports that size with
  `Resize`, so its content fills it; a sheet without detents fits its content.
* Alert buttons follow macOS order: the primary action trailing (the default
  button, `hasDestructiveAction` when destructive), Cancel next to it with
  Escape.
* `keyboard_height` is 0 and `font_scale` is 1 (see Environment).
* Haptics play only on Force Touch trackpads, while a finger rests on one.

## Not yet in the protocol

* Mouse button and scroll wheel events for `on_mouse_down` / `on_mouse_up` /
  `on_scroll_wheel` with the button number (deferred).
