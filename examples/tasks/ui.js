// The example's presentation vocabulary: icons, headers, grouped surfaces.
//
// Every color comes from `cx.theme()` by role and every length from GPUI's
// rem scale (`p_4`, `gap_3`, …), so the same code is right in light and dark
// and follows the user's text size. Rows are sized by padding, never by a
// fixed height, so they grow with large text instead of clipping it. Pixel
// values appear only for platform geometry: safe areas and the keyboard.

import { div, h_flex, v_flex, shape, Button } from "native-shell";

/** Outline icons on a 24-unit grid, drawn with a 2-unit round stroke. */
const ICONS = {
  check: "M20 6 9 17l-5-5",
  plus: "M12 5v14M5 12h14",
  arrow_up: "M12 19V5M5 12l7-7 7 7",
  chevron_down: "m6 9 6 6 6-6",
  chevron_right: "m9 18 6-6-6-6",
  calendar: "M8 2v4M16 2v4M3 10h18M5 4h14a2 2 0 0 1 2 2v14a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V6a2 2 0 0 1 2-2Z",
  trash: "M3 6h18M8 6V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2M19 6l-1 14a2 2 0 0 1-2 2H8a2 2 0 0 1-2-2L5 6M10 11v6M14 11v6",
  circle_check: "M22 12a10 10 0 1 1-20 0 10 10 0 0 1 20 0ZM8 12l3 3 5-6",
  alert: "M22 12a10 10 0 1 1-20 0 10 10 0 0 1 20 0ZM12 8v4M12 16h.01",
};

/** @param {keyof typeof ICONS} name @param {string} color */
export function icon(name, color, size = 20) {
  return shape(ICONS[name], { view_box: [0, 0, 24, 24] })
    .stroke(color, 2)
    .w(size)
    .h(size)
    .flex_none();
}

/** The root page's title block: large title, one line of status, optional trailing control. */
export function largeTitle(cx, title, subtitle, trailing) {
  const t = cx.theme();
  return h_flex()
    .items_end()
    .justify_between()
    .gap_3()
    .px_4()
    .pt(cx.window().safe_area.top + 12)
    .pb_3()
    .child(
      v_flex()
        .flex_1()
        .min_w_0()
        .gap_1()
        .child(div().role("header").text_3xl().font_bold().line_height(1.15).text_color(t.foreground).child(title))
        .when(Boolean(subtitle), (el) => el.child(div().test_id("subtitle").text_sm().text_color(t.muted_foreground).child(subtitle))),
    )
    .when(Boolean(trailing), (el) => el.child(trailing));
}

/** A one-pixel rule, inset from the leading edge to the column it separates. */
export function hairline(cx, inset = 0) {
  return div().h(1).ml(inset).bg(cx.theme().border).flex_none();
}

/** A full-width text button for sheet headers: quiet, with a 44 pt target. */
export function barButton(cx, id, label, onClick, { emphasized = false, disabled = false } = {}) {
  const t = cx.theme();
  return Button.new(id)
    .test_id(id)
    .disabled(disabled)
    .px_2()
    .py_2()
    .rounded_md()
    .hit_slop(8)
    .text_base()
    .when(emphasized, (el) => el.font_semibold())
    .text_color(disabled ? t.muted_foreground : t.primary)
    .when(!disabled, (el) => el.active((s) => s.opacity(0.5)).on_click(onClick))
    .child(label);
}

/** A centered message for a collection with nothing in it: what it is and what to do next. */
export function emptyState(cx, name, title, body) {
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
        .child(icon(name, t.muted_foreground, 40)),
    )
    .child(div().text_lg().font_semibold().text_color(t.foreground).text_center().child(title))
    .child(div().text_sm().text_color(t.muted_foreground).text_center().child(body));
}
