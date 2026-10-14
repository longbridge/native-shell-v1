// The example's presentation vocabulary: icons, bars, grouped rows, states.
//
// Every color comes from `cx.theme()` by role and every length from GPUI's
// rem scale, so the same code is right in light and dark and follows the
// user's text size. Pixel values appear only for platform geometry: safe
// areas and the keyboard.

import { div, h_flex, v_flex, shape, Button } from "native-shell";
import { Empty, EmptyHeader, EmptyMedia, EmptyTitle, EmptyDescription, EmptyContent } from "gpui-component";
/** @import { ClickEvent, Color, Context, Element, Theme } from "native-shell" */

/** Outline icons on a 24-unit grid, drawn with a 2-unit round stroke. */
const ICONS = {
  arrow_left: "M19 12H5M12 19l-7-7 7-7",
  chevron_left: "m15 18-6-6 6-6",
  chevron_right: "m9 18 6-6-6-6",
  plus: "M12 5v14M5 12h14",
  bell: "M6 8a6 6 0 0 1 12 0c0 7 3 9 3 9H3s3-2 3-9M10.3 21a1.94 1.94 0 0 0 3.4 0",
  sort: "M3 6h18M6 12h12M10 18h4",
  wifi_off:
    "M2 2l20 20M8.5 16.5a5 5 0 0 1 7 0M5 13a10 10 0 0 1 5.17-2.69M19 13a10 10 0 0 0-2.17-1.5M2 8.82a15 15 0 0 1 4.17-2.65M22 8.82a15 15 0 0 0-11.29-3.76M12 20h.01",
  chart: "M3 3v18h18M7 14l4-4 4 4 5-6",
  clock: "M22 12a10 10 0 1 1-20 0 10 10 0 0 1 20 0ZM12 6v6l4 2",
  trend: "M22 7 13.5 15.5 8.5 10.5 2 17M16 7h6v6",
  trash: "M3 6h18M19 6l-1 14a2 2 0 0 1-2 2H8a2 2 0 0 1-2-2L5 6M10 11v6M14 11v6M8 6V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2",
};

/** @param {keyof typeof ICONS} name @param {Color} color */
export function icon(name, color, size = 20) {
  return shape(ICONS[name], { view_box: [0, 0, 24, 24] })
    .stroke(color, 2)
    .w(size)
    .h(size)
    .flex_none();
}

/** Gains in the success color, losses in the destructive one; the sign carries the meaning too. */
export function changeColor(t, n) {
  return n < -0.004 ? t.destructive : t.success;
}

/** The status bar's backdrop: content scrolls beneath it rather than under the clock. */
export function statusBar(cx) {
  return div().h(cx.window().safe_area.top).flex_none().bg(cx.theme().grouped_background);
}

/**
 * A large title's size: 30 pt, growing with the user's text size only up to
 * 1.6×, as native large titles do, so a long word never breaks mid-word.
 */
export function titleSize(cx) {
  const scale = cx.window().font_scale;
  return (30 * Math.min(scale, 1.6)) / scale;
}

/** A round icon button in a bar, with an accessible name. */
export function barButton(cx, id, name, label, on_click) {
  const t = cx.theme();
  return Button.new(id)
    .test_id(id)
    .accessibility_label(label)
    .size_11()
    .flex()
    .items_center()
    .justify_center()
    .rounded_full()
    .active((s) => s.bg(t.muted))
    .on_click(on_click)
    .child(icon(name, t.primary, 22));
}

/**
 * A pushed page's bar, drawn the way each platform draws its own: iOS centers
 * the title and labels the back button with the previous page; Android leads
 * with an arrow and the title.
 * @param {Context} cx
 * @param {{ title: string, back: { label: string, on_click: (e: ClickEvent, cx: Context) => void }, trailing?: Element }} opts
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
    .bg(t.grouped_background)
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

/**
 * A labeled group of rows: hairline-bordered, rows divided at the text column.
 * @param {Context} cx
 * @param {{ title?: string, footer?: string, rows: (Element | null)[], id?: string, trailing?: Element, inset?: number }} opts
 */
export function section(cx, { title, footer, rows, id, trailing, inset = 16 }) {
  const t = cx.theme();
  const items = rows.filter(Boolean);
  return v_flex()
    .gap_2()
    .when(Boolean(title) || Boolean(trailing), (el) =>
      el.child(
        h_flex()
          .px_4()
          .gap_2()
          .justify_between()
          .child(
            div()
              .flex_1()
              .min_w_0()
              .role("header")
              .text_sm()
              .font_medium()
              .text_color(t.muted_foreground)
              .child(title ?? ""),
          )
          .when(Boolean(trailing), (el) => el.child(div().flex_none().child(trailing))),
      ),
    )
    .child(
      v_flex()
        .when(Boolean(id), (el) => el.test_id(id))
        .rounded_xl()
        .border_1()
        .border_color(t.border)
        .bg(t.surface)
        .overflow_hidden()
        .children(items.flatMap((row, i) => (i === 0 ? [row] : [div().h(1).ml(inset).bg(t.border), row]))),
    )
    .when(Boolean(footer), (el) => el.child(div().px_4().text_sm().text_color(t.muted_foreground).child(footer)));
}

/**
 * One grouped row: the label, then a trailing value or control. Pass
 * `on_click` to make the whole row pressable.
 * @param {Context} cx
 * @param {{ id: string, label: string, detail?: string, value?: string, trailing?: Element | null, on_click?: (e: ClickEvent, cx: Context) => void, chevron?: boolean, disabled?: boolean, destructive?: boolean }} opts
 */
export function row(cx, { id, label, detail, value, trailing, on_click, chevron, disabled, destructive }) {
  const t = cx.theme();
  const fg = disabled ? t.muted_foreground : destructive ? t.destructive : t.foreground;
  const body = h_flex()
    .gap_3()
    .px_4()
    .py_3()
    .min_h_12()
    .child(
      v_flex()
        .flex_1()
        .min_w_0()
        .gap_0p5()
        .child(div().text_base().text_color(fg).child(label))
        .when(Boolean(detail), (el) => el.child(div().text_sm().text_color(t.muted_foreground).child(detail))),
    )
    .when(value !== undefined && value !== null, (el) =>
      el.child(div().test_id(`${id}-value`).flex_none().text_base().text_color(t.muted_foreground).child(value)),
    )
    .when(Boolean(trailing), (el) => el.child(trailing))
    .when(Boolean(chevron), (el) => el.child(icon("chevron_right", t.muted_foreground, 18)));
  if (!on_click) return body.test_id(id);
  return Button.new(id)
    .test_id(id)
    .accessibility_label(typeof value === "string" ? `${label}, ${value}` : label)
    .disabled(!!disabled)
    .when(!disabled, (el) => el.active((s) => s.bg(t.muted)).on_click(on_click))
    .child(body);
}

/**
 * A centered message for a screen or section with nothing to show.
 * @param {Context} cx
 * @param {{ id: string, icon: keyof typeof ICONS, title: string, message: string, action?: Element }} opts
 */
export function emptyState(cx, { id, icon: name, title, message, action }) {
  const t = cx.theme();
  return Empty.new()
    .test_id(id)
    .py_12()
    .header(
      EmptyHeader.new()
        .media(EmptyMedia.new().child(icon(name, t.muted_foreground, 40)))
        .title(EmptyTitle.new().child(title))
        .description(EmptyDescription.new().child(message)),
    )
    .when(Boolean(action), (el) => el.content(EmptyContent.new().child(action)));
}

/** A text button in the tint color, for bars and section headers. */
export function textButton(cx, id, label, on_click, { bold = false, disabled = false } = {}) {
  const t = cx.theme();
  return Button.new(id)
    .test_id(id)
    .disabled(disabled)
    .flex_none()
    .px_2()
    .py_2()
    .hit_slop(6)
    .text_base()
    .when(bold, (el) => el.font_semibold())
    .text_color(disabled ? t.muted_foreground : t.primary)
    .when(!disabled, (el) => el.active((s) => s.opacity(0.5)).on_click(on_click))
    .child(label);
}

/** @param {Theme} t */
export function hairline(t) {
  return div().h(1).bg(t.border);
}
