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
  arrow_up: "M12 19V5M5 12l7-7 7 7",
  copy: "M10 8h10a2 2 0 0 1 2 2v10a2 2 0 0 1-2 2H10a2 2 0 0 1-2-2V10a2 2 0 0 1 2-2ZM4 16a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h10a2 2 0 0 1 2 2",
  trash: "M3 6h18M8 6V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2M19 6l-1 14a2 2 0 0 1-2 2H8a2 2 0 0 1-2-2L5 6M10 11v6M14 11v6",
  refresh: "M21 12a9 9 0 1 1-3-6.7L21 8M21 3v5h-5",
  alert: "M22 12a10 10 0 1 1-20 0 10 10 0 0 1 20 0ZM12 8v4M12 16h.01",
  history: "M3 12a9 9 0 1 0 3-6.7L3 8M3 3v5h5M12 7v5l4 2",
  chevron_right: "m9 18 6-6-6-6",
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
 * A circle of initials standing in for a profile photo. `points` is its size
 * at the default text size; it grows with the user's text size like the
 * initials inside it.
 * @param {Context} cx @param {string} name @param {number} points
 */
export function avatar(cx, name, points) {
  const t = cx.theme();
  const initials = name
    .split(/\s+/)
    .filter(Boolean)
    .slice(0, 2)
    .map((w) => w[0].toUpperCase())
    .join("");
  return textStyle(div(), cx, points >= 56 ? "title2" : "headline")
    .flex_none()
    .size(`${points / 16}rem`)
    .rounded_full()
    .bg(t.muted_foreground)
    .flex()
    .items_center()
    .justify_center()
    .text_color(t.primary_foreground)
    .font_semibold()
    .child(initials);
}
