# JSON-valued properties, events and commands

`crates/core/src/protocol.rs` defines the binary framing. Some values are JSON
documents in the string table (`ValueKind::Json`) or event/command payloads.
This file is their canonical schema. The core produces exactly these shapes;
renderers may be lenient when reading but must not require anything else.

General rules:

* **Colors** are JSON integers `0xAARRGGBB` (unsigned 32-bit, e.g. `4278190080`
  for opaque black). Never strings.
* **Lengths** are logical points (iOS pt / Android dp), floats.
* **Angles** are degrees.
* **Durations** are milliseconds.
* Absent keys mean "default"; `null` is never produced.
* Text offsets in events are UTF-16 code units.

## Properties

### `AttributedText` (20)

```json
{
  "runs": [
    {
      "text": "Hello",
      "size": 16,
      "weight": 400,
      "color": 4278848010,
      "family": "Menlo",
      "italic": true,
      "underline": true,
      "strike": true,
      "line_height": 22.4,
      "letter_spacing": 0.4,
      "url": "https://example.org"
    }
  ],
  "align": "left",
  "lines": 2,
  "truncate": true
}
```

* `size`, `weight` (1–1000, normally 100…900), `color`: always present.
* `family`: absent = system font.
* `italic`, `underline`, `strike`: present only when true.
* Line height: `line_height`, in points like every length; the core resolves
  multiples against the run's size before sending. Absent = the font's
  natural line height.
  Renderers implement it as a fixed line height (`minimumLineHeight ==
  maximumLineHeight` + baseline centering on iOS, `LineHeightSpan` on
  Android).
* `letter_spacing`: points, absent = 0.
* `url`: the run is a link.
* `align`: `"left" | "center" | "right" | "justify"`, absent = left (natural).
* `lines`: maximum line count, absent = unlimited.
* `truncate`: tail ellipsis on the last allowed line; absent = clip.
* `nowrap` is expressed as `lines: 1` without truncate.

### `TextStyle` (59), on `TextInput`

`{"size": 16, "weight": 400, "color": 4278848010, "family": "Menlo", "align": "left", "line_height": 22.4}`
with the same rules as a run (`size`, `weight`, `color` always present).

### `Paging` (45), on a horizontal `Scroll`

`true`: the scroll view comes to rest on whole pages, a page being the
viewport's width, as UIScrollView's `isPagingEnabled`; a fling moves at most
one page. The content's `Scroll` events report the offset as usual, so the
page is `round(x / viewport_width)`; `ScrollTo` with `animated` moves to a
page. Gpui-component's `Carousel` uses it. A renderer that does not page yet
shows a plain horizontal scroll. The scroll is horizontal only, so it never
takes vertical drags from the page around it.

### `Shadow` (6)

`{"color": 436207616, "x": 0, "y": 4, "blur": 6, "spread": 0}` — CSS box-shadow
semantics (blur radius as in CSS; iOS `shadowRadius = blur / 2`).

### `Gradient` (7)

`{"angle": 90, "stops": [[4294901760, 0.0], [4278190335, 1.0]]}` — linear, CSS
convention: `0` = towards the top, `90` = towards the right.

### `Transform` (10)

`{"translate_x": 0, "translate_y": 0, "scale": 1, "rotate": 0}` — applied in
that order around the view's center; `rotate` in degrees.

### `Animation` (11)

```json
{
  "opacity": {"kind": "timing", "duration": 250, "delay": 0, "easing": "ease_in_out"},
  "frame": {"kind": "spring", "response": 0.35, "damping": 0.825, "delay": 0},
  "transform": {…},
  "background": {…}
}
```

`kind`: `"timing" | "spring"`. Timing: `duration`, `delay` (ms), `easing`:
`"linear" | "ease_in" | "ease_out" | "ease_in_out"`. Spring: `response`
(seconds, the period of the undamped spring), `damping` (ratio, 1 = critical),
`delay` (ms). Renderers animate a property only inside a batch flagged
`ANIMATED` and only if the node has an entry for it.

### `TextValue` (50)

`{"text": "hello", "count": 3}` — see the input protocol in
`docs/agents-plan.md`.

### `Detents` (103)

`["medium", "large"]` and/or `{"height": 320}` entries.

### `AlertSpec` (105)

`{"title": "Delete?", "message": "This cannot be undone.", "actions": [{"label": "Cancel", "style": "cancel"}, {"label": "Delete", "style": "destructive"}]}`
`style`: `"default" | "cancel" | "destructive"`.

### `Items` (110)

`["Day", "Week", "Month"]`.

### `Menu` (114)

`{"title": "Sort by", "placeholder": "Choose…", "items": [{"label": "Name", "value": "name", "destructive": false, "disabled": false}]}`.
The button shows the label of the item whose `value` equals the node's
`Value`-less selection — the core sends the selected item's index in
`SelectedIndex` (`-1` = none, show `placeholder`).

### `A11yState` (134)

`{"disabled": true, "selected": true, "checked": true, "expanded": true, "busy": true}` — present keys only.

### `ThemeTokens` (152), on `ROOT_NODE`

`{"appearance": "light", "background": 4294967295, "foreground": 4278848010, "primary": …}` — every
resolved color token (defaults, host theme provider, then the program's
`set_theme`) as integers. Sent in the first batch and whenever the resolved
theme or appearance changes. Renderers use it for everything they draw
themselves: native control tints, toasts, picker fields, sheet and dialog
backgrounds.

### `HostProps` (151)

The script's props object, verbatim.

## Event payloads

| Event | Payload |
| --- | --- |
| `Click` | `{"x": 10, "y": 20, "count": 1, "modifiers"?: {"shift", "ctrl", "alt", "cmd"}}` (point in the view; `modifiers` from pointer platforms, absent = none held) |
| `DoubleClick`, `LongPress`, `PressIn`, `PressOut` | `{"x", "y"}` |
| `Change` (TextInput) | `{"text", "selection": [start, end], "composing": false, "count": 4}` |
| `Change` (Switch, Checkbox) | `{"checked": true}` |
| `Change` (Slider) | `{"value": 0.42, "final": false}` |
| `Change` (Segmented, Select) | `{"index": 1, "value": "week"}` (`value` for Select is the item value; Segmented sends the item label) |
| `Change` (DatePicker) | `{"date": "2026-10-14"}` / `"2026-10-14T09:30"` / `"09:30"` |
| `Submit` | `{"text"}` |
| `Focus`, `Blur`, `Refresh`, `Dismiss`, `BackPressed` | `{}` |
| `Scroll` | `{"x", "y", "content_width", "content_height", "viewport_width", "viewport_height"}` |
| `Pan` | `{"phase": "began"\|"changed"\|"ended"\|"cancelled", "dx", "dy", "vx", "vy"}` |
| `Pinch` | `{"phase", "scale"}` |
| `Link` | `{"url"}` |
| `Key` | `{"key", "key_char", "modifiers": {"shift", "ctrl", "alt", "cmd"}, "phase": "down"\|"up"}` |
| `Action` | `{"index": 1}` |
| `RangeChange` | `{"first": 0, "end": 30}` (half-open `[first, end)`, prefetch included, clamped to `[0, ItemCount]`) |
| `RowHeight` | `{"index", "height"}` |
| `HostEvent` | `{"name": "select", "payload": …}` |
| `Resize` (sheet node) | `{"width", "height"}`: the size the sheet presents its content at (below the grabber, above the bottom safe area for a sheet that has one), sent when presented and whenever it changes (detent change, rotation); continuous, coalesced |
| `Hover` | `{"entered": true, "x", "y"}`: the pointer entered (`true`) or left (`false`) the node; pointer platforms only, sent on nodes with the `HOVER` mask bit |
| `ToastAction` (root node) | `{"id"}`: the toast's action button was tapped; the renderer dismisses that toast |

## Command payloads

| Command | Target | Payload |
| --- | --- | --- |
| `Focus`, `Blur` | input node | `{}` |
| `ScrollTo` | scroll node | `{"x", "y", "animated"}` |
| `ScrollToIndex` | list node | `{"index", "position": "top"\|"center"\|"bottom", "animated"}` |
| `ShowToast` | root (0) | `{"id", "title", "description"?, "action"?: {"label"}, "level": "info"\|"success"\|"warning"\|"error", "duration": 3000}` (`duration` 0 = until dismissed). A toast whose `id` is still showing is replaced: the engine sends `DismissToast {"id"}` for it first, in the same batch |
| `DismissToast` | root (0) | `{"id"}` retracts that toast if still showing (else ignored); `{}` retracts every toast |
| `OpenUrl` | root | `{"url"}` |
| `SetClipboard` | root | `{"text"}` |
| `PresentPage`, `DismissPage` | page node | `{"animated": true}` |
| `Haptic` | root | `{"kind": "light"\|"medium"\|"heavy"\|"success"\|"warning"\|"error"\|"selection"}` |

## Surfaces and their lifecycle

* Every surface is a native node inserted under `ROOT_NODE` (0), in this
  order: the root content (`View`), pages in stack order, then overlays
  (`Sheet`, `Dialog`) in opening order. Renderers *present* `Page`, `Sheet`
  and `Dialog` nodes (navigation stack, sheet, modal) instead of adding them as
  plain subviews; their children are mounted inside the presented container.
* A pushed page arrives as `Create(Page)` + props + children + `Insert` +
  `Command(PresentPage, {"animated": true})` in the same batch; the first page
  pushed on top of the root content animates, a page present in the
  `INITIAL` batch does not.
* A popped page arrives as `Command(DismissPage, {"animated": true})`; its
  `Delete(page)` follows in the same or a later batch. Renderers run the exit transition and
  only then release the views (the `Delete` is applied to the node table
  immediately; the container stays on screen until the animation ends).
* A sheet's content is laid out at the width of the environment and, until
  the renderer reports one, its content height; once the renderer sends
  `Resize` with the size it presents the content at (per detent), the sheet
  surface is laid out at that size, so `size_full` / `flex_1` content fills
  the sheet. The renderer keeps the sheet's height from its detents, not from
  the frame the core sends.
* Sheets and dialogs appear when inserted and disappear on `Delete`. A user
  dismissal (swipe, back, backdrop when `Dismissable`) sends `Dismiss` on the
  surface node and hides it locally at once; the core then deletes it.
* `BackPressed` is sent on the top page node when the system back gesture
  happens while that page has `CanGoBack == false`; otherwise the renderer pops
  it locally and sends `Dismiss`.
* Alerts (`Dialog` with `AlertSpec`) send `Action {index}` and then are
  deleted by the core.

## Pages are full screen

Page surfaces are laid out at the full environment size and renderers draw no
native navigation bar or toolbar: programs draw their own headers (using
`cx.window().safe_area`). `Title` is used for accessibility (pane title) and
the system back-history menu only. `NavBarHidden` is reserved and ignored.
Back navigation stays native (edge swipe on iOS, system back on Android).

## TextInput insets

A `TextInput`'s style padding is layout-only in the tree, so the core also
sends it as `ContentInsets` (`[top, right, bottom, left]`); renderers inset the
editable text by it (`UITextField` text rect, `EditText.setPadding`).

## Units

All geometry (frames, radii, borders, font sizes, shadows) is in logical
points: iOS points, Android dp (multiply by density; `Environment.scale` is
the device scale/density). Font sizes on Android are scaled by the user's font
scale the same way in measurement and rendering (`TypedValue.applyDimension`
with `COMPLEX_UNIT_SP`), and iOS applies `UIFontMetrics` likewise; the core
never sees font-scaled sizes.

The user's text size changes two things, differently:

* **Text** scales fully. Font sizes, line heights and letter spacing in
  `AttributedText` and `TextStyle` stay unscaled; renderers apply the whole
  font scale when measuring and drawing.
* **Layout** scales through the rem. Lengths a program gives in rems (the
  spacing scale `p_4`, `gap_2`, `size_10`, `rounded_md`, `"1.5rem"` …) are laid
  out at the effective rem size `16 × clamp(font_scale, 1.0, 1.6)`, so padding,
  gaps, controls and icons grow with their labels; the clamp keeps layouts
  usable at accessibility sizes. Pixel lengths (`w(40)`, `"8px"`, `border_1`)
  and fractions (`w_1_2`) do not scale. Frames, radii and border widths the
  core sends already include this; renderers do not scale them again. Programs
  read the effective value as `window.rem_size`.

## Instance config (C ABI / JNI `create`)

See the comment in `crates/ffi/include/native_shell.h`. The theme override is
top level: `"theme": {"light": {"primary": "#2563eb"}, "dark": {…}}` (hex
strings here, since it is written by hand in host code).

## Measurement callbacks

`measure_text(attributed_json, width, width_mode)` runs on the JS thread during
layout and must be deterministic and consistent with rendering. The core
follows GPUI's text layout: text wraps only at a known or definite width;
every other probe (min-content included) lays it out unwrapped.

| `width_mode` | Meaning | Return |
| --- | --- | --- |
| `0` undefined (`width` is +∞) | lay out without wrapping except at hard line breaks | width = widest line; height of that layout |
| `1` at most `width` | wrap within `width` | width = min(`width`, widest wrapped line); height of the wrapped layout |
| `2` exactly `width` | wrap within `width` | width = `width`; height of the wrapped layout |

* Never lay out with an infinite or huge width; compute the unconstrained
  desired width instead.
* Return widths **unrounded** (the exact widest line). The core snaps them up
  to the device pixel grid, measures again at the snapped width, and sends
  that width as the text node's frame width. Renderers must lay text out at
  exactly the frame width they receive.
* Heights may be rounded up to device pixels.
* `baseline` is the first line's baseline from the top.
* `lines` / `truncate` in the JSON cap the height; an empty string measures as
  width 0 and the height of one line.

## Reserved built-in host modules

Module names starting with `nsh/` are reserved: registries may not declare
them and scripts may not import them. The core uses them to ask the platform
for built-in services through the ordinary host-call callbacks, after its own
capability check:

| Module | Method | Kind | Args | Answer |
| --- | --- | --- | --- | --- |
| `nsh/clipboard` | `read` | sync (`call_sync`) | `[]` | JSON string, or `null` when the clipboard has no text |

## Hardware key events

Renderers send each hardware key down/up exactly once, as a `Key` event on the
nearest node with the `KEY` mask bit on the focus path, or on `ROOT_NODE` (0)
when no such node has focus. Pure modifier and system keys are not sent, and
events are not consumed (text fields still type). `key` uses gpui names
(`a`–`z`, `0`–`9`, `f1`–`f12`, `enter`, `escape`, `tab`, `space`, `backspace`,
`delete`, `up`, `down`, `left`, `right`, `home`, `end`, `pageup`, `pagedown`,
punctuation characters). The engine resolves `key_context` and `bind_keys`
bindings by walking from the target node up to the root.

### Batched measurement

Before each layout the core collects every text leaf whose content changed and
measures them in one `measure_text_batch` call in max-content mode (`width_mode`
0). Renderers may shape the batch in parallel (shaping is per-thread safe with
per-thread paints / TextKit stacks). During layout, a constrained probe
(`width_mode` 1 or 2) whose width is at least the max-content width is answered
from that result without another call: the text lays out on the same lines.
Only texts that actually wrap are measured again at their width.

## Bottom-aligned lists

A `List` with `ListAlignment` 1 (GPUI's `ListAlignment::Bottom`, a chat
thread) opens scrolled to its last row, and while the user is at the end
(within a few points) the end stays in view when rows are added or the
viewport shrinks (the keyboard rising); scrolled away from the end, the
position is kept as for any list. Its first `RangeChange` covers the last
rows, and the core's initial window before that is the list's tail.

## Pointer: hover, cursor, tooltip

On platforms with a pointer (macOS, Linux, iPad with a trackpad, Android with
a mouse) renderers apply three things without a round trip, as they do for
pressed feedback: `HoverOpacity` / `HoverBackground` while the pointer is over
the node (`.hover(s => …)`; when pressed too, the pressed values win),
`Cursor` (a `CursorStyle`, GPUI's set and order: 0 arrow, 1 ibeam,
2 crosshair, 3 closed hand, 4 open hand, 5 pointing hand, 6–15 the resize
cursors, 16 vertical ibeam, 17 not allowed, 18 drag link, 19 drag copy,
20 context menu; fall back to arrow where the platform lacks one) and `Tooltip`
(plain text shown after the pointer rests on the node, with the platform's
own tooltip: `NSView.toolTip`, Android `View.tooltipText`, the toolkit's on
Linux; iOS has none and ignores it). `Hover` events go only to nodes with the
`HOVER` mask bit. Touch-only platforms ignore all four.

## Images and shapes

`ImageSource` is an `https://` URL or an absolute file path (the engine
materializes program-relative assets under the data directory). `svg(path)`
elements arrive as `Shape` nodes (path data in view-box units), drawn in one
color like gpui; `FillRule` (0 nonzero, 1 even-odd) selects the fill rule.
Renderers fit the `ViewBox` into the frame uniformly, centered. A script
shape with `fit: "fill"` (chart lines and areas) is stretched by the core
instead: after layout it sends `PathData` rewritten into frame points and a
`ViewBox` of `[0, 0, width, height]`, again whenever the frame's size
changes, so renderers draw it like any other shape.

## Shape strokes

`Shape` strokes use round caps and round joins on every platform (SVG icon
sets rely on zero-length segments such as `M12 12h.01` drawing a dot).

## Built-in control measurement

Native controls whose width depends on their content and platform chrome are
measured through `measure_component` with reserved names, when the style does
not give them a definite width:

| Name | Props JSON | Return |
| --- | --- | --- |
| `nsh/select` | `{"label": "Weekly", "placeholder": "Choose…"}` (the text the control shows) | width and height of the native select button: label at the control font plus chevron and padding |
| `nsh/date-picker` | `{"value": "2026-10-14", "mode": "date"\|"time"\|"datetime"}` | width and height of the native compact picker showing the localized value |

Sizes include the user's font scale, like text. `DatePicker` values are
`YYYY-MM-DD` (date), `HH:mm` (time, 24-hour) and `YYYY-MM-DDTHH:mm`
(datetime), both in the `Date` property and in `Change` payloads; renderers
display them localized (medium date, short time honoring the system 12/24-hour
setting).
