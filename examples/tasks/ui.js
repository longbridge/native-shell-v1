// The example's presentation vocabulary, following iOS: text styles, large
// titles and navigation bars, inset grouped lists with 44 pt rows, filled
// and plain buttons.
//
// Colors come from `cx.theme()` by role (the default theme is the iOS system
// palette), text sizes from its iOS text styles, and lengths from the rem
// scale, which grows with the user's text size. Pixel values appear only for
// platform geometry: safe areas, the keyboard and hairlines.

import { div, h_flex, v_flex, shape, Button } from "native-shell";
/** @import { ClickEvent, Color, Context, Element } from "native-shell" */

/** Outline icons on a 24-unit grid, drawn with a 2-unit round stroke. */
const ICONS = {
  check: "M20 6 9 17l-5-5",
  arrow_up: "M12 19V5M5 12l7-7 7 7",
  chevron_right: "m9 18 6-6-6-6",
  chevron_down: "m6 9 6 6 6-6",
  calendar: "M8 2v4M16 2v4M3 10h18M5 4h14a2 2 0 0 1 2 2v14a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V6a2 2 0 0 1 2-2Z",
  alert: "M22 12a10 10 0 1 1-20 0 10 10 0 0 1 20 0ZM12 8v4M12 16h.01",
  circle_check: "M22 12a10 10 0 1 1-20 0 10 10 0 0 1 20 0ZM8 12l3 3 5-6",
};

/** @typedef {"large_title" | "title1" | "title2" | "title3" | "headline" | "body" | "callout" | "subheadline" | "footnote" | "caption1" | "caption2"} TextStyle */
/** @typedef {(e: ClickEvent, cx: Context) => void} OnClick */

/**
 * Sets an iOS text style from the theme: size, weight and line height.
 * @template {Element} E
 * @param {E} el @param {Context} cx @param {TextStyle} style
 * @returns {E}
 */
export function textStyle(el, cx, style) {
  const s = cx.theme().typography[style];
  return el
    .text_size(s.size)
    .line_height(s.line_height / s.size)
    .font_weight(s.weight);
}

/** A run of text in a text style and color. @param {Context} cx @param {TextStyle} style @param {Color} color @param {string} content */
export function label(cx, style, color, content) {
  return textStyle(div(), cx, style).text_color(color).child(content);
}

/** @param {keyof typeof ICONS} name @param {Color} color */
export function icon(name, color, size = 20) {
  return shape(ICONS[name], { view_box: [0, 0, 24, 24] })
    .stroke(color, 2)
    .w(size)
    .h(size)
    .flex_none();
}

/** The status bar's backdrop, so content scrolls beneath it rather than under the clock. @param {Context} cx @param {Color} color */
export function statusBar(cx, color) {
  return div().h(cx.window().safe_area.top).flex_none().bg(color);
}

/**
 * A top-level screen's header: a bar row for buttons, then the large title
 * (Large Title, 34 pt bold) and an optional line of status.
 * @param {Context} cx
 * @param {{ title: string, subtitle?: string | null, trailing?: Element }} opts
 */
export function largeTitle(cx, { title, subtitle, trailing }) {
  const t = cx.theme();
  return v_flex()
    .px_4()
    .pb_2()
    .child(
      h_flex()
        .min_h_11()
        .justify_end()
        .when(Boolean(trailing), (el) => el.child(trailing)),
    )
    .child(label(cx, "large_title", t.foreground, title).role("header"))
    .when(Boolean(subtitle), (el) => el.child(label(cx, "subheadline", t.muted_foreground, subtitle ?? "").test_id("subtitle")));
}

/**
 * A plain bar button: tint-colored text, semibold for the screen's commit
 * action, with a 44 pt target.
 * @param {Context} cx @param {string} id @param {string} text
 * @param {{ on_click?: OnClick, emphasized?: boolean, disabled?: boolean, destructive?: boolean }} [opts]
 */
export function textButton(cx, id, text, { on_click, emphasized = false, disabled = false, destructive = false } = {}) {
  const t = cx.theme();
  return Button.new(id)
    .test_id(id)
    .link()
    .disabled(disabled)
    .px_2()
    .py_2()
    .hit_slop(6)
    .when(emphasized, (el) => el.font_semibold())
    .when(destructive, (el) => el.text_color(t.destructive))
    .when(!disabled && Boolean(on_click), (el) => el.on_click(/** @type {OnClick} */ (on_click)))
    .child(text);
}

/**
 * An inset grouped section: an optional header in secondary text, rows on a
 * rounded surface divided by hairlines inset to the text, and an optional
 * footer that explains the rows.
 * @param {Context} cx
 * @param {{ header?: string, footer?: string | null, footerColor?: Color, footerId?: string, rows: (Element | null | false)[], inset?: number, id?: string }} opts
 */
export function section(cx, { header, footer, footerColor, footerId, rows, inset = 16, id }) {
  const t = cx.theme();
  const items = /** @type {Element[]} */ (rows.filter(Boolean));
  return v_flex()
    .when(Boolean(header), (el) =>
      el.child(
        label(cx, "footnote", t.muted_foreground, (header ?? "").toUpperCase())
          .role("header")
          .px_4()
          .pb_1p5(),
      ),
    )
    .child(
      v_flex()
        .when(Boolean(id), (el) => el.test_id(id ?? ""))
        .rounded(t.radius.md)
        .bg(t.surface)
        .overflow_hidden()
        .children(items.flatMap((row, i) => (i === 0 ? [row] : [div().h(0.5).ml(inset).bg(t.border), row]))),
    )
    .when(Boolean(footer), (el) =>
      el.child(
        label(cx, "footnote", footerColor ?? t.muted_foreground, footer ?? "")
          .px_4()
          .pt_1p5()
          .when(Boolean(footerId), (el) => el.test_id(footerId ?? "").role("alert")),
      ),
    );
}

/**
 * A list row: at least 44 pt tall with 16 pt insets; an optional leading
 * element, the label (Body) and detail (Subheadline), then a trailing value
 * in secondary text, a control, or a chevron for drill-down.
 * @param {Context} cx
 * @param {{ id: string, leading?: Element, label: string, detail?: string, value?: string, trailing?: Element, on_click?: OnClick, chevron?: boolean, disabled?: boolean, destructive?: boolean }} opts
 */
export function row(cx, { id, leading, label: text, detail, value, trailing, on_click, chevron, disabled, destructive }) {
  const t = cx.theme();
  const body = h_flex()
    .gap_3()
    .px_4()
    .py_2p5()
    .min_h_11()
    .when(Boolean(leading), (el) => el.child(leading))
    .child(
      v_flex()
        .flex_1()
        .min_w_0()
        .child(label(cx, "body", disabled ? t.muted_foreground : destructive ? t.destructive : t.foreground, text).test_id(`${id}-label`))
        .when(Boolean(detail), (el) => el.child(label(cx, "subheadline", t.muted_foreground, detail ?? ""))),
    )
    .when(value != null, (el) => el.child(label(cx, "body", t.muted_foreground, value ?? "").flex_none()))
    .when(Boolean(trailing), (el) => el.child(trailing))
    .when(Boolean(chevron), (el) => el.child(icon("chevron_right", t.muted_foreground, 16)));
  if (!on_click) return body.test_id(id);
  return Button.new(id)
    .test_id(id)
    .accessibility_label(value ? `${text}, ${value}` : text)
    .disabled(Boolean(disabled))
    .when(!disabled, (el) => el.active((s) => s.bg(t.secondary)).on_click(on_click))
    .child(body);
}

/**
 * A screen or list with nothing to show yet: what it is for and what to do
 * next, centered (iOS's unavailable-content layout).
 * @param {Context} cx @param {keyof typeof ICONS} name @param {string} title @param {string} body @param {Element} [action]
 */
export function emptyState(cx, name, title, body, action) {
  const t = cx.theme();
  return v_flex()
    .test_id("empty")
    .flex_1()
    .items_center()
    .justify_center()
    .gap_2()
    .px_8()
    .pb_16()
    .child(
      div()
        .mb_2()
        .child(icon(name, t.muted_foreground, 44)),
    )
    .child(label(cx, "title2", t.foreground, title).font_bold().text_center())
    .child(label(cx, "body", t.muted_foreground, body).text_center())
    .when(Boolean(action), (el) => el.child(div().mt_2().child(action)));
}
