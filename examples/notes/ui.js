// The example's presentation vocabulary: bars, grouped lists, fields, menus.
//
// Colors come from `cx.theme()` by role and text from its iOS text styles, so
// the same code is right in light and dark and follows the user's text size.
// Rows are sized by padding and minimum heights, never fixed heights, so large
// text grows them instead of clipping it. Pixel values appear only for
// platform geometry: safe areas and the keyboard.

import { div, h_flex, v_flex, shape, text, span, Button, Input } from "native-shell";
import { Empty, EmptyContent, EmptyDescription, EmptyHeader, EmptyMedia, EmptyTitle, Kbd, Separator } from "gpui-component";
/** @import { ClickEvent, Color, Context, Element, InputState } from "native-shell" */

/** Outline icons on a 24-unit grid, drawn with a 2-unit round stroke. */
const ICONS = {
  chevron_left: "m15 18-6-6 6-6",
  chevron_right: "m9 18 6-6-6-6",
  folder: "M4 20h16a2 2 0 0 0 2-2V8a2 2 0 0 0-2-2h-7.9a2 2 0 0 1-1.7-.9l-.8-1.2A2 2 0 0 0 7.9 3H4a2 2 0 0 0-2 2v13a2 2 0 0 0 2 2Z",
  folder_plus: "M12 10v6M9 13h6M4 20h16a2 2 0 0 0 2-2V8a2 2 0 0 0-2-2h-7.9a2 2 0 0 1-1.7-.9l-.8-1.2A2 2 0 0 0 7.9 3H4a2 2 0 0 0-2 2v13a2 2 0 0 0 2 2Z",
  stack: "m12 2 10 5-10 5L2 7ZM2 17l10 5 10-5M2 12l10 5 10-5",
  compose: "M12 3H5a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h14a2 2 0 0 0 2-2v-7M18.4 2.6a2.1 2.1 0 1 1 3 3L12 15l-4 1 1-4Z",
  search: "M11 19a8 8 0 1 0 0-16 8 8 0 0 0 0 16ZM21 21l-4.3-4.3",
  clear: "M22 12a10 10 0 1 1-20 0 10 10 0 0 1 20 0ZM15 9l-6 6M9 9l6 6",
  pin: "M12 17v5M9 10.8a2 2 0 0 1-1.1 1.8l-1.8.9A2 2 0 0 0 5 15.2V17h14v-1.8a2 2 0 0 0-1.1-1.7l-1.8-.9a2 2 0 0 1-1.1-1.8V7a1 1 0 0 1 1-1 2 2 0 0 0 0-4H8a2 2 0 0 0 0 4 1 1 0 0 1 1 1Z",
  trash: "M3 6h18M8 6V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2M19 6l-1 14a2 2 0 0 1-2 2H8a2 2 0 0 1-2-2L5 6M10 11v6M14 11v6",
  share: "M4 12v8a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2v-8M16 6l-4-4-4 4M12 2v13",
  more: "M22 12a10 10 0 1 1-20 0 10 10 0 0 1 20 0ZM8 12h.01M12 12h.01M16 12h.01",
  checklist: "M3 5l1.5 1.5L7 4M3 12l1.5 1.5L7 11M3 19l1.5 1.5L7 18M11 6h10M11 13h10M11 20h10",
  eye: "M2 12s3.5-7 10-7 10 7 10 7-3.5 7-10 7S2 12 2 12ZM12 15a3 3 0 1 0 0-6 3 3 0 0 0 0 6Z",
  pencil: "M17 3a2.8 2.8 0 1 1 4 4L7.5 20.5 2 22l1.5-5.5Z",
  move: "M4 20h16a2 2 0 0 0 2-2V8a2 2 0 0 0-2-2h-7.9a2 2 0 0 1-1.7-.9l-.8-1.2A2 2 0 0 0 7.9 3H4a2 2 0 0 0-2 2v13a2 2 0 0 0 2 2ZM9 13h6M13 10.5l2.5 2.5-2.5 2.5",
  copy: "M8 8h12a2 2 0 0 1 2 2v10a2 2 0 0 1-2 2H10a2 2 0 0 1-2-2ZM16 8V5a2 2 0 0 0-2-2H5a2 2 0 0 0-2 2v9a2 2 0 0 0 2 2h3",
  check: "M20 6 9 17l-5-5",
  hash: "M4 9h16M4 15h16M10 3 8 21M16 3l-2 18",
  alert: "M22 12a10 10 0 1 1-20 0 10 10 0 0 1 20 0ZM12 8v4M12 16h.01",
  note: "M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8ZM14 2v6h6M8 13h8M8 17h5",
};

/** @typedef {keyof typeof ICONS} IconName */

/**
 * An icon `size` points across at the default text size, in rems so it grows
 * with the user's text size as the labels beside it do.
 * @param {IconName} name @param {Color} color
 */
export function icon(name, color, size = 22) {
  const rem = /** @type {`${number}rem`} */ (`${size / 16}rem`);
  return shape(ICONS[name], { view_box: [0, 0, 24, 24] })
    .stroke(color, 2)
    .w(rem)
    .h(rem)
    .flex_none();
}

/**
 * One of the iOS text styles (`"headline"`, `"body"`, `"footnote"`…), for `.map(...)`.
 * @param {Context} cx
 * @param {"title1" | "title2" | "title3" | "headline" | "body" | "callout" | "subheadline" | "footnote" | "caption1"} role
 */
export function font(cx, role) {
  const style = cx.theme().typography[role];
  return (/** @type {Element} */ el) => el.text_size(style.size).font_weight(style.weight);
}

/**
 * The bar at the top of every page. The title appears in it once the page's
 * large title has scrolled away, and a hairline then separates the two. A
 * `plain` page (the editor) has no grouped background to match.
 * @param {Context} cx
 * @param {{ id: string, title?: string, collapsed?: boolean, plain?: boolean, back?: { label: string, on_click: (e: ClickEvent, cx: Context) => void }, trailing?: (Element | null)[] }} opts
 */
export function navBar(cx, { id, title = "", collapsed = false, plain = false, back, trailing = [] }) {
  const t = cx.theme();
  return v_flex()
    .flex_none()
    .pt(cx.window().safe_area.top)
    .bg(collapsed || plain ? t.background : t.grouped_background)
    .when(collapsed, (el) => el.border_b_1().border_color(t.border))
    .child(
      h_flex()
        .relative()
        .min_h_11()
        .px_2()
        .justify_between()
        .child(
          div()
            .absolute()
            .left_0()
            .right_0()
            .top_0()
            .bottom_0()
            .flex()
            .items_center()
            .justify_center()
            .px_24()
            .child(
              div()
                .test_id(`${id}-bar-title`)
                .role("header")
                .map(font(cx, "headline"))
                .text_color(t.foreground)
                .truncate()
                .opacity(collapsed ? 1 : 0)
                .child(title),
            ),
        )
        .child(
          back
            ? Button.new(`${id}-back`)
                .test_id(`${id}-back`)
                .accessibility_label(`Back to ${back.label}`)
                .flex()
                .items_center()
                .min_h_11()
                .pr_2()
                .text_color(t.primary)
                .active((s) => s.opacity(0.5))
                .on_click(back.on_click)
                .child(icon("chevron_left", t.primary, 26))
                .child(div().map(font(cx, "body")).max_w_32().truncate().child(back.label))
            : div(),
        )
        .child(h_flex().gap_1().children(trailing.filter(Boolean))),
    );
}

/**
 * A text or icon button for a bar: tinted, quiet, at least 44 × 44.
 * @param {Context} cx
 * @param {{ id: string, label: string, icon?: IconName, on_click: (e: ClickEvent, cx: Context) => void, emphasized?: boolean, disabled?: boolean, destructive?: boolean, tooltip?: string }} opts
 */
export function barButton(cx, { id, label, icon: name, on_click, emphasized = false, disabled = false, destructive = false, tooltip }) {
  const t = cx.theme();
  const color = disabled ? t.muted_foreground : destructive ? t.destructive : t.primary;
  return Button.new(id)
    .test_id(id)
    .accessibility_label(label)
    .disabled(disabled)
    .min_w_11()
    .min_h_11()
    .px_2()
    .flex()
    .items_center()
    .justify_center()
    .rounded_md()
    .text_color(color)
    .map(font(cx, emphasized ? "headline" : "body"))
    .when(Boolean(tooltip), (el) => el.tooltip(/** @type {string} */ (tooltip)))
    .when(!disabled, (el) => el.active((s) => s.opacity(0.5)).on_click(on_click))
    .child(name ? icon(name, color) : label);
}

/**
 * A tinted button for the one action a screen offers on its own (an empty
 * folder's New Note, Try Again), with its shortcut beside it on a desktop.
 * @param {Context} cx
 * @param {{ id: string, label: string, on_click: (e: ClickEvent, cx: Context) => void, shortcut?: string }} opts
 */
export function tintedButton(cx, { id, label, on_click, shortcut }) {
  const t = cx.theme();
  const desktop = ["macos", "linux"].includes(cx.window().platform);
  return h_flex()
    .gap_3()
    .child(
      Button.new(id)
        .test_id(id)
        .flex()
        .items_center()
        .px_5()
        .min_h_11()
        .rounded_full()
        .bg(t.accent)
        .map(font(cx, "headline"))
        .text_color(t.accent_foreground)
        .active((s) => s.opacity(0.6))
        .on_click(on_click)
        .child(label),
    )
    .when(desktop && Boolean(shortcut), (el) => el.child(Kbd.new(/** @type {string} */ (shortcut)).test_id(`${id}-shortcut`)));
}

/** A top-level page's title, at the top of its scrolling content. */
export function largeTitle(cx, title) {
  const t = cx.theme();
  return div()
    .px_4()
    .pt_1()
    .pb_2()
    .child(div().test_id("large-title").role("header").text_size(titleSize(cx)).font_bold().line_height(1.15).text_color(t.foreground).child(title));
}

/**
 * The search field under a large title: a magnifier, the field, and a clear
 * button while it holds text.
 * @param {Context} cx
 * @param {{ id: string, state: InputState, label: string }} opts
 */
export function searchField(cx, { id, state, label }) {
  const t = cx.theme();
  const filled = state.value() !== "";
  return h_flex()
    .role("search")
    .mx_4()
    .mb_2()
    .pl_2()
    .gap_1()
    .rounded_lg()
    .bg(t.secondary)
    .child(icon("search", t.muted_foreground, 18))
    .child(
      Input.new(state)
        .test_id(id)
        .accessibility_label(label)
        .flex_1()
        .min_w_0()
        .min_h_10()
        .px_1()
        .map(font(cx, "body"))
        .text_color(t.foreground),
    )
    .child(
      filled
        ? Button.new(`${id}-clear`)
            .test_id(`${id}-clear`)
            .accessibility_label("Clear search")
            .size_10()
            .flex()
            .items_center()
            .justify_center()
            .active((s) => s.opacity(0.5))
            .on_click((_e, cx) => {
              state.set_value("");
              cx.notify();
            })
            .child(icon("clear", t.muted_foreground, 18))
        : div(),
    );
}

/**
 * An inset group: a header in secondary text, rows on one rounded surface
 * divided at the text column, and an optional footer.
 * @param {Context} cx
 * On a sheet, whose own background is plain, the group takes the grouped color instead (`on_sheet`).
 * @param {{ id?: string, title?: string, footer?: string, rows: (Element | null)[], inset?: number, on_sheet?: boolean }} opts
 */
export function section(cx, { id, title, footer, rows, inset = 16, on_sheet = false }) {
  const t = cx.theme();
  const items = rows.filter(Boolean);
  return v_flex()
    .px_4()
    .gap_1p5()
    .when(Boolean(title), (el) =>
      el.child(div().role("header").px_4().map(font(cx, "footnote")).text_color(t.muted_foreground).child(title?.toUpperCase())),
    )
    .child(
      v_flex()
        .when(Boolean(id), (el) => el.test_id(/** @type {string} */ (id)))
        .rounded_xl()
        .bg(on_sheet ? t.muted : t.surface)
        .overflow_hidden()
        .children(items.flatMap((row, i) => (i === 0 ? [row] : [Separator.new().ml(inset), row]))),
    )
    .when(Boolean(footer), (el) => el.child(div().px_4().map(font(cx, "footnote")).text_color(t.muted_foreground).child(footer)));
}

/**
 * The bar at the bottom of a page: leading and trailing actions around a
 * status line (or a centered action), above the home indicator and the keyboard.
 * @param {Context} cx
 * @param {{ id: string, leading?: Element | null, status?: string, center?: Element | null, trailing?: Element | null }} opts
 */
export function toolbar(cx, { id, leading, status, center, trailing }) {
  const t = cx.theme();
  const win = cx.window();
  return h_flex()
    .flex_none()
    .px_2()
    .pb(Math.max(win.keyboard_height, win.safe_area.bottom))
    .min_h_12()
    .bg(t.background)
    .border_t_1()
    .border_color(t.border)
    .child(div().flex_1().flex().child(leading ?? div()))
    .child(center ?? div().test_id(`${id}-status`).map(font(cx, "caption1")).text_color(t.muted_foreground).child(status ?? ""))
    .child(div().flex_1().flex().justify_end().child(trailing ?? div()));
}

/**
 * gpui-component's Empty for a collection with nothing in it: what it is and
 * what to do next, with an optional action under it.
 * @param {Context} cx
 * @param {{ id?: string, icon: IconName, title: string, body: string, action?: Element }} opts
 */
export function emptyState(cx, { id = "empty", icon: name, title, body, action }) {
  const t = cx.theme();
  return Empty.new()
    .test_id(id)
    .py_12()
    .header(
      EmptyHeader.new()
        .media(EmptyMedia.new().child(icon(name, t.muted_foreground, 44)))
        .title(EmptyTitle.new().text_size(cx.theme().typography.title3.size).child(title))
        .description(EmptyDescription.new().map(font(cx, "subheadline")).child(body)),
    )
    .when(Boolean(action), (el) => el.content(EmptyContent.new().child(/** @type {Element} */ (action))));
}

/** The line under the bar when the library couldn't be written. */
export function saveError(cx) {
  const t = cx.theme();
  return h_flex()
    .test_id("save-error")
    .role("alert")
    .gap_2()
    .mx_4()
    .mb_2()
    .px_3()
    .py_2()
    .rounded_lg()
    .bg(t.surface)
    .border_1()
    .border_color(t.destructive)
    .child(icon("alert", t.destructive, 18))
    .child(
      div()
        .flex_1()
        .map(font(cx, "footnote"))
        .text_color(t.foreground)
        .child("Couldn’t save your notes. Changes are kept until the app closes."),
    );
}

/**
 * A menu of actions on one object, as a sheet: the object's name, then its
 * actions, destructive ones in red. The long-press alternative to swiping.
 * @param {Context} cx
 * @param {{ id: string, title: string, message?: string, actions: ({ id: string, label: string, icon: IconName, destructive?: boolean, run: (cx: Context) => void } | null)[] }} spec
 */
export function actionSheet(cx, { id, title, message, actions }) {
  cx.haptic("medium");
  cx.open_sheet(
    () => {
      const t = cx.theme();
      const rows = actions.filter(Boolean).map((a) => {
        const color = a.destructive ? t.destructive : t.foreground;
        return Button.new(a.id)
          .test_id(a.id)
          .flex()
          .items_center()
          .gap_3()
          .px_4()
          .py_3()
          .min_h_12()
          .active((s) => s.bg(t.secondary))
          .on_click((_e, cx) => {
            window.close_sheet();
            a.run(cx);
          })
          .child(div().flex_1().map(font(cx, "body")).text_color(color).child(a.label))
          .child(icon(a.icon, a.destructive ? t.destructive : t.primary, 20));
      });
      return v_flex()
        .test_id(id)
        .size_full()
        .bg(t.background)
        .pt_5()
        .pb(cx.window().safe_area.bottom + 16)
        .gap_4()
        .child(
          v_flex()
            .px_8()
            .gap_0p5()
            .child(div().role("header").map(font(cx, "headline")).text_color(t.foreground).text_center().truncate().child(title))
            .when(Boolean(message), (el) =>
              el.child(div().map(font(cx, "footnote")).text_color(t.muted_foreground).text_center().child(message)),
            ),
        )
        .child(section(cx, { rows, on_sheet: true }));
    },
    { detents: ["medium"] },
  );
}

/**
 * `body` with every occurrence of the search words drawn in the tint, bold.
 * @param {string} body @param {string[]} words @param {Color} color
 */
export function highlighted(body, words, color) {
  const lower = body.toLocaleLowerCase();
  /** @type {[number, number][]} */
  const ranges = [];
  for (const w of words) {
    for (let i = lower.indexOf(w); i !== -1 && w !== ""; i = lower.indexOf(w, i + w.length)) ranges.push([i, i + w.length]);
  }
  ranges.sort((a, b) => a[0] - b[0]);
  const runs = [];
  let at = 0;
  for (const [start, end] of ranges) {
    if (end <= at) continue;
    const from = Math.max(start, at);
    if (from > at) runs.push(body.slice(at, from));
    runs.push(span(body.slice(from, end)).font_weight(700).text_color(color));
    at = end;
  }
  if (at < body.length) runs.push(body.slice(at));
  return text(runs.length > 0 ? runs : [""]);
}

/**
 * A large title's size: 34 pt, growing with the user's text size only up to
 * 1.6×, as native large titles do, so a long word never breaks mid-word.
 */
function titleSize(cx) {
  const scale = cx.window().font_scale;
  return (cx.theme().typography.large_title.size * Math.min(scale, 1.6)) / scale;
}
