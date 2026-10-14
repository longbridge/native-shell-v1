# gpui-shell compatibility

native-shell runs gpui-shell programs. Names, signatures, defaults and error
messages follow gpui-shell; where a desktop concept has no phone equivalent
the API is either adapted (same call, mobile realization) or absent (importing
it fails at link time with "export not found"). `examples/gpui-todolist` is
gpui-kit's `examples/js_todolist` copied byte for byte and runs unchanged.

Modules: `"gpui-kit"`, `"gpui"`, `"gpui-base"` and `"native-shell"` resolve to
one module carrying both name sets; `"gpui-component"` exports `TitleBar`
and the gpui-component families listed under Adapted; `"gpui-shell"` exists and exports nothing, as in gpui-shell;
`"fs/promises"` is available under the `fs` capability.

## Same

| Area | API |
| --- | --- |
| Views | `class X extends View` with `init / render / update`; `cx.new(Class, props)` → Entity with `set_props`, `release() → bool` |
| Elements | `div`, `h_flex`, `v_flex`, `list(id, count, get_key, render(i, cx))`, `uniform_list(id, count, get_key, render(range, cx))`, `.child` (strings, numbers, entities; booleans ignored), `.children`, `.when`, `.map`, `.id`, `.on_item_click` |
| Styles | the full GPUI vocabulary, generated from GPUI's own reflection (3154 nullary methods) plus gpui-shell's 59 parametric methods with its argument rules and error texts; `.active(s => …)`, `.transition`, `.spring` |
| Semantics | `.role`, `.accessibility_label`, `.disabled`, `.selected`, `.pressed`, `.aria_selected`, `.track_focus(handle)` |
| Context | `cx.notify(entity?)`, `cx.spawn(body, {owner})` → Task, `cx.sleep(ms)`, `cx.timer.after/every` → Task, `cx.theme()`, `cx.open_url`, `cx.write_to_clipboard`, `cx.read_from_clipboard() → string \| undefined`, `cx.phase()`, `cx.focus_handle()`, `cx.entity()` |
| Text input | `InputState.new({placeholder, value})` with `value()`, `set_value`, `set_masked`, `set_step/min/max/loading`, `on(change \| submit \| focus \| blur)`, `release()`; `TextareaState` with `set_rows / set_auto_grow / set_soft_wrap`; `Input.new`, `Textarea.new` |
| Slider | `SliderState.new({min, max, step, value})` with `value / set_value / min_value / max_value / step_value / on(change \| release)` |
| Components | `Toggle`, `ToggleGroup`, `RadioGroup`, `Collapsible`, `Accordion` (+ Item / Header / Panel / Trigger), `Pagination` + `pagination_items`, headless `Select` / `Combobox` / `DatePicker(id, focus_handle)`, `TextView.markdown / html` with `on_link_click`, `selectable`, `scrollable` |
| Theme | `set_theme({appearance, tokens: {colors}})`, `cx.theme()` shape (tokens top level and under `colors`, spacing, radius, typography, appearance, is_dark) |
| Window | `window.open_dialog / close_dialog / close_all_dialogs / has_active_dialog`, `open_sheet / close_sheet / has_active_sheet`, `push_toast({title, description, level, timeout, id}) → id`, `remove_toast(id) → bool`, `clear_toasts() → count`, `localStorage`, `sessionStorage`, `rem_size`, `line_height`, `viewport_size`, `bounds`, `appearance`, `refresh` |
| Errors | call-scope refusals ("X is not allowed during the `render` phase; …"), state creation in render, stale `cx`, unknown style methods with "did you mean" |

## Adapted

| API | Mobile realization |
| --- | --- |
| `image`, `svg` | `image` is a native image view (program-relative assets are materialized to files); `svg` is parsed in Rust into a native shape drawn in one color (its `text_color`, else the theme foreground), like gpui |
| `Button`, `Link`, `Tab` | headless pressables as in gpui-base; gpui-component's styled forms (`.primary()`, `.secondary()`, `.outline()`, `.danger()`, `.success()`, `.warning()`, `.ghost()`, `.link()`, `.size("xsmall" \| "small" \| "medium" \| "large")`, `.label()`) draw iOS buttons: 28 / 34 / 44 / 50 pt in rems, filled variants darken when pressed, plain ones fade, small sizes keep a 44 pt touch target; `.variant(name)` is the mobile spelling of the same |
| `Checkbox`, `Switch`, `Radio`, `Progress` (+ Track / Indicator) | drawn (headless, as in gpui-base) when the element gets a child, `.headless()`, or a style that draws a box (background, border, radius, shadow, padding, text); the native control when bare or only placed by layout styles (margin, size, flex, position) |
| `Slider(state)` and its parts | always the native slider; the part components are accepted and ignored |
| `Tabs` | headless row of `Tab`s; `.items([...])` makes it a native segmented control |
| `on_click` event | `{click_count, modifiers, x, y}`; modifiers come from pointer platforms and are all false on touch |
| `on_hover`, `.hover(s => …)`, `cursor_*`, `.tooltip(text)` | applied on platforms with a pointer (macOS, Linux, iPad trackpad, Android mouse); hover styles are opacity and background; `tooltip` takes a string, as in gpui-shell; the platform's own tooltip shows it (none on iOS) |
| `NumberInput` | a native numeric field between two step buttons |
| `OtpState` / `OtpInput` | one native field with the one-time-code content type, number pad and max length |
| `Avatar` | image if set, else fallback (no load-failure fallback) |
| `v_virtual_list` / `VirtualListScrollHandle` | the native recycled list; `h_virtual_list` is a horizontally scrolling row (not virtualized) |
| `Scrollbar` | accepted; native scroll views draw their own indicators |
| `Popover`, `HoverCard` (opens on long press), `Popup` | presented as a medium bottom sheet; dismissal goes through `on_open_change(false)` |
| `Select`, `DatePicker` | `Select.new(id).items().value().on_change()` and `DatePicker.new(id)` are native pickers; gpui-base's headless shapes are selected by the call shape |
| `PathBuilder`, `Background`, `window.paint_path` | a native shape view from SVG path data; gradients and patterns paint their first color |
| `cx.bind_keys`, `.on_action`, `.key_context`, `.on_key_down / up` | hardware keyboards only; contexts match by name, chords are not supported |
| `Table` (+ Header / Body / Row / Head / Cell / Caption) | headless views with gpui-base's layout; announced with the list / list item / header roles and row and column indexes (the protocol has no table roles) |
| gpui-component conversation: `Message`, `Bubble`, `Marker`, `Attachment`, `MessageScroller` + `MessageScrollerState(item_count)` | drawn in the iOS Messages look from theme tokens (outgoing bubbles in the tint, incoming in `secondary`, 17 pt text, 18 pt radius, 80% of the row); `MessageScroller` is a bottom-aligned `list` (opens at its end, keeps the newest row in view) with a jump-to-latest button once the reader scrolls away; rows keep their identity through their `.key()`; `.scrollbar()` is accepted (native indicators). Mobile adds `Message.avatar / header / footer`, `Bubble.tail()`, `Marker.alignment`, `Attachment.media / title / description / progress / on_remove / on_retry`, and `MessageScrollerState` methods (`item_count`, `is_scrolled_up`, `is_following_tail`, `reset`, `splice(start, end, count)`, `append`, `prepend`, `scroll_to_item`, `scroll_to_end`) where gpui-shell keeps the state opaque |
| gpui-component feedback: `Alert` (+ `InfoAlert` / `SuccessAlert` / `WarningAlert` / `ErrorAlert`), `Badge`, `Tag`, `Skeleton`, `Empty` (+ `EmptyHeader` / `EmptyMedia` / `EmptyTitle` / `EmptyDescription` / `EmptyContent`), `Kbd`, `Label`, `Separator` (+ `VerticalSeparator` / `DashedSeparator` / `VerticalDashedSeparator`), `ShimmerText`, `Notification` | composed from primitives in the iOS look: alerts are 12 pt cards (or full-width banners) tinted with the variant token, announced as alerts; badges are the red count capsule (`max+` past `max`, hidden at zero) or a dot over the child's corner; tags are filled (`primary`, `secondary`) or tinted with ink pulled towards the label color so status colors stay legible in both appearances; separators are one-device-pixel hairlines (dashes clipped from a long run); `Kbd` spells keystrokes as `⇧⌘K` on iOS / macOS and `Shift+Win+K` elsewhere and reads them in words. `Skeleton` and `ShimmerText` hold still (iOS's redacted placeholders do; a renderer only tweens frames the script draws, and the engine cannot see Reduce Motion): the shimmer settings are validated and kept, the text is muted and announced busy. `Notification` is gpui-shell's button-triggered effect, shown as the platform toast: `on_click` becomes its *View* action, `on_close` runs on expiry or that action (a swipe-away is not reported). Mobile adds `Alert.icon / on_close` (from gpui-component's Rust API) and `Tag.on_close`; constructors work as `X.new(...)` and `new X(...)` |
| gpui-component forms: `Form` / `VForm` / `HForm` + `Field`, `InputGroup` (+ `InputGroupInput` / `InputGroupTextarea` / `InputGroupAddon` / `InputGroupButton` / `InputGroupText`), `Rating`, `Stepper` + `StepperItem`, `TimeField` + `TimeFieldState`, `Clipboard`, `ColorPicker` + `ColorPickerState` | drawn in the iOS look: a vertical form stacks labels (15 pt) above controls with footnote descriptions; a horizontal form is an inset grouped list (10 pt corners on `surface`, hairlines inset 16 pt) whose 44 pt rows put the label beside a trailing control, and an `InputGroup` inside a row drops its frame; `columns` / `col_span` use a grid. Input groups are 44 pt fields (10 pt corners, `input` border, `destructive` when invalid) with tinted plain buttons that keep a 44 pt touch target; `content_type` picks the keyboard, autofill type and capitalization. `Rating` stars announce themselves as buttons (a read-only rating as one image, "4.5 out of 5 stars"). `Stepper` draws done steps with a check and a lit connector. `TimeField` is the platform's time picker; the 12/24-hour clock follows the user's setting and seconds are kept but not edited. `Clipboard` turns into a check for two seconds unless `cx.window().clipboard_feedback`. `ColorPicker` is a swatch opening a sheet of suggested colors (the iOS system colors), a palette and a hex field. Mobile adds `Field.error`, half stars on a read-only `Rating`, `StepperItem.icon`, `TimeField.invalid`, `Clipboard.value_fn / on_copied`, `ColorPicker.featured_colors / icon`, and readable states (`TimeFieldState` / `ColorPickerState`: `value`, `set_value`, `on("change")`) where gpui-shell keeps them opaque |
| `open_sheet_at` | placement ignored: always a bottom sheet |
| `mouse_position`, `is_fullscreen`, `is_maximized`, `is_window_active` | constants (`{0, 0}`, `true`) |
| `cx.stop_propagation`, `cx.propagate` | no-ops: touch events do not bubble |
| `on_mouse_*`, `on_scroll_wheel`, `tab_index`… | accepted and ignored so desktop render code runs |

## Not available on mobile

Not provided: resizable panels, `Dock*`,
`CalendarState`, `gpui-fps`, `window.dispatch_action`, window management
(minimize, zoom, move, fullscreen, activate, `set_rem_size`, `focus_next/prev`),
PathBuilder dash arrays and real gradient or pattern fills, `process.*`,
`net`, and the Node compatibility modules (`path`, `buffer`, `zlib`).

## Mobile-only additions

`text`, `span`, `link`, `shape`, `host_component`, `Spinner`, native `Select`
and `DatePicker`, `cx.window()` (safe area, keyboard height, platform, font
scale), the iOS text styles as `cx.theme().typography` roles (`large_title`,
`title1`–`title3`, `headline`, `body`, `callout`, `subheadline`, `footnote`,
`caption1`, `caption2`; size, line height and weight in points, scaled by the
font scale) next to gpui-shell's keys, `cx.navigator` (push/pop pages), `cx.alert`, `cx.haptic`,
`cx.focus / blur / scroll_to / scroll_to_item`, toast actions (`push_toast({…, action: {label, on_click}})`), `cx.set_title`,
`cx.set_can_go_back`, `cx.on_back`, `.key`, `.test_id`, `.hit_slop`,
`.on_long_press`, `list(…).alignment("bottom")` (GPUI's `ListAlignment::Bottom`
for chat threads: opens at the end and keeps it in view), `.on_pan`, `.on_pinch`, `.on_press_in / out`, `.href`, and
host modules (`import … from "<host module>"`).
