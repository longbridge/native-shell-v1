// The example's presentation vocabulary: icons, headers, grouped rows.
//
// Every color comes from `cx.theme()` by role and every length from GPUI's
// rem scale (`p_4`, `gap_3`, …), so the same code is right in light and dark
// and follows the user's text size. Rows are sized by padding, never by a
// fixed height, so they grow with large text instead of clipping it. Pixel
// values appear only for platform geometry: safe areas and the keyboard.

import { div, h_flex, v_flex, shape, Button } from "native-shell";

/** Outline icons on a 24-unit grid, drawn with a 2-unit round stroke. */
const ICONS = {
  arrow_left: "M19 12H5M12 19l-7-7 7-7",
  chevron_left: "m15 18-6-6 6-6",
  chevron_right: "m9 18 6-6-6-6",
  bell: "M6 8a6 6 0 0 1 12 0c0 7 3 9 3 9H3s3-2 3-9M10.3 21a1.94 1.94 0 0 0 3.4 0",
  clock: "M22 12a10 10 0 1 1-20 0 10 10 0 0 1 20 0ZM12 6v6l4 2",
  moon: "M12 3a6 6 0 0 0 9 9 9 9 0 1 1-9-9Z",
  chart: "M3 3v18h18M7 16v-4M12 16V8M17 16v-7",
  database: "M3 5c0-1.7 4-3 9-3s9 1.3 9 3-4 3-9 3-9-1.3-9-3ZM3 5v14c0 1.7 4 3 9 3s9-1.3 9-3V5M3 12c0 1.7 4 3 9 3s9-1.3 9-3",
  info: "M22 12a10 10 0 1 1-20 0 10 10 0 0 1 20 0ZM12 16v-4M12 8h.01",
  help: "M22 12a10 10 0 1 1-20 0 10 10 0 0 1 20 0ZM9.1 9a3 3 0 0 1 5.8 1c0 2-3 3-3 3M12 17h.01",
  external: "M15 3h6v6M10 14 21 3M18 13v6a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h6",
  user: "M19 21v-2a4 4 0 0 0-4-4H9a4 4 0 0 0-4 4v2M16 7a4 4 0 1 1-8 0 4 4 0 0 1 8 0Z",
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

/**
 * The status bar's backdrop: content scrolls beneath it rather than under the
 * system's clock and indicators.
 */
export function statusBar(cx) {
  return div().h(cx.window().safe_area.top).flex_none().bg(cx.theme().background);
}

/** The root page's title block, at the top of its scrolling content. */
export function largeTitle(cx, title) {
  const t = cx.theme();
  return div()
    .px_4()
    .pt_3()
    .pb_2()
    .child(div().role("header").text_size(titleSize(cx)).font_bold().line_height(1.15).text_color(t.foreground).child(title));
}

/**
 * A pushed page's bar, drawn the way each platform draws its own: iOS centers
 * the title and labels the back button with the previous page; Android leads
 * with an arrow and the title.
 */
export function navBar(cx, { title, back, trailing }) {
  const t = cx.theme();
  const ios = cx.window().platform === "ios";
  const backButton = Button.new("back")
    .test_id("back")
    .accessibility_label(`Back to ${back.label}`)
    .flex()
    .items_center()
    .gap_1()
    .py_2()
    .px_2()
    .rounded_md()
    .hit_slop(4)
    .active((s) => s.opacity(0.5))
    .on_click(back.on_click)
    .when(ios, (el) => el.text_color(t.primary).child(icon("chevron_left", t.primary, 24)))
    // As UIKit does, the previous page's name gives way when it would crowd the title.
    .when(ios && back.label.length + title.length <= 26, (el) => el.child(div().text_base().child(back.label)))
    .when(!ios, (el) => el.child(icon("arrow_left", t.foreground, 24)));
  const heading = div().role("header").text_lg().font_semibold().text_color(t.foreground).truncate().child(title);
  return v_flex()
    .pt(cx.window().safe_area.top)
    .bg(t.background)
    .border_b_1()
    .border_color(t.border)
    .child(
      ios
        ? h_flex()
            .relative()
            .min_h_12()
            .px_2()
            .justify_between()
            .child(div().absolute().left_0().right_0().top_0().bottom_0().flex().items_center().justify_center().px_24().child(heading))
            .child(backButton)
            .child(trailing ?? div())
        : h_flex()
            .min_h_12()
            .py_1()
            .px_2()
            .gap_2()
            .child(backButton)
            .child(div().flex_1().min_w_0().child(heading))
            .when(Boolean(trailing), (el) => el.child(trailing)),
    );
}

/** A labeled group of rows: hairline-bordered, rows divided at the text column. */
export function section(cx, { title, footer, rows, id }) {
  const t = cx.theme();
  const items = rows.filter(Boolean);
  return v_flex()
    .gap_2()
    .when(Boolean(title), (el) => el.child(div().role("header").px_4().text_sm().font_medium().text_color(t.muted_foreground).child(title)))
    .child(
      v_flex()
        .when(Boolean(id), (el) => el.test_id(id))
        .rounded_xl()
        .border_1()
        .border_color(t.border)
        .bg(t.surface)
        .overflow_hidden()
        .children(items.flatMap((row, i) => (i === 0 ? [row] : [div().h(1).ml(48).bg(t.border), row]))),
    )
    .when(Boolean(footer), (el) => el.child(div().px_4().text_sm().text_color(t.muted_foreground).child(footer)));
}

/**
 * One settings row: an icon in a fixed lane, the label, then a trailing value
 * or control. Pass `on_click` to make the whole row pressable.
 */
export function row(cx, { id, icon: name, label, detail, value, trailing, on_click, chevron, disabled }) {
  const t = cx.theme();
  const fg = disabled ? t.muted_foreground : t.foreground;
  const body = h_flex()
    .gap_3()
    .px_4()
    .py_3()
    .min_h_12()
    .child(
      div()
        .w_5()
        .flex_none()
        .flex()
        .justify_center()
        .child(name ? icon(name, t.muted_foreground) : null),
    )
    .child(
      v_flex()
        .flex_1()
        .min_w_0()
        .gap_0p5()
        .child(div().text_base().text_color(fg).child(label))
        .when(Boolean(detail), (el) => el.child(div().text_sm().text_color(t.muted_foreground).child(detail))),
    )
    .when(value !== undefined && value !== null, (el) =>
      el.child(div().flex_none().text_base().text_color(t.muted_foreground).child(value)),
    )
    .when(Boolean(trailing), (el) => el.child(trailing))
    .when(Boolean(chevron), (el) => el.child(icon("chevron_right", t.muted_foreground, 18)));
  if (!on_click) return body.when(Boolean(id), (el) => el.test_id(id));
  return Button.new(id)
    .test_id(id)
    .accessibility_label(typeof value === "string" ? `${label}, ${value}` : label)
    .disabled(!!disabled)
    .when(!disabled, (el) => el.active((s) => s.bg(t.muted)).on_click(on_click))
    .child(body);
}

/** A circle of initials, standing in for a profile photo. */
export function avatar(cx, name, size = 48) {
  const t = cx.theme();
  const initials = name
    .split(/\s+/)
    .filter(Boolean)
    .slice(0, 2)
    .map((w) => w[0].toUpperCase())
    .join("");
  return div()
    .flex_none()
    // The initials scale with the user's text size, so the circle does too.
    .size(size * Math.min(cx.window().font_scale, 2))
    .rounded_full()
    .bg(t.primary)
    .flex()
    .items_center()
    .justify_center()
    .text_color(t.primary_foreground)
    .font_semibold()
    .text_lg()
    .child(initials);
}

/**
 * A large title's size: 30 pt, growing with the user's text size only up to
 * 1.6×, as native large titles do, so a long word never breaks mid-word.
 */
function titleSize(cx) {
  const scale = cx.window().font_scale;
  return (30 * Math.min(scale, 1.6)) / scale;
}
