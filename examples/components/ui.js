// The example's presentation vocabulary: icons, page scaffolding, grouped
// rows, form fields and buttons sized for touch.
//
// Every color comes from `cx.theme()` by role and every length from GPUI's
// rem scale (`p_4`, `gap_3`, …), so the same code is right in light and dark
// and follows the user's text size. Rows and controls are sized by padding
// and minimum heights, never fixed heights, so large text grows them instead
// of clipping. Pixel values appear only for platform geometry: safe areas and
// the keyboard.

import { div, h_flex, v_flex, shape, Button, Input, Spinner } from "native-shell";
/** @import { ClickEvent, Color, Context, Element } from "native-shell" */

/** Outline icons on a 24-unit grid, drawn with a 2-unit round stroke. */
const ICONS = {
  arrow_left: "M19 12H5M12 19l-7-7 7-7",
  chevron_left: "m15 18-6-6 6-6",
  chevron_right: "m9 18 6-6-6-6",
  pointer: "M9 9l5 12 1.8-5.2L21 14ZM7.2 2.2 8 5.1M5.1 8 2.2 7.2M14 4.1 12 6.2M6.2 12l-2.1 2",
  text: "M17 6.1H3M21 12.1H3M15.1 18H3",
  toggle: "M7 6h10a6 6 0 0 1 0 12H7A6 6 0 0 1 7 6ZM9 12a2 2 0 1 1-4 0 2 2 0 0 1 4 0Z",
  sliders: "M4 21v-7M4 10V3M12 21v-9M12 8V3M20 21v-5M20 12V3M1 14h6M9 8h6M17 16h6",
  calendar: "M8 2v4M16 2v4M3 10h18M5 4h14a2 2 0 0 1 2 2v14a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V6a2 2 0 0 1 2-2Z",
  layers: "m12 2 10 5-10 5L2 7ZM2 17l10 5 10-5M2 12l10 5 10-5",
  share: "M4 12v8a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2v-8M16 6l-4-4-4 4M12 2v13",
  download: "M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4M7 10l5 5 5-5M12 15V3",
  link: "M10 13a5 5 0 0 0 7.5.5l3-3a5 5 0 0 0-7-7l-1.7 1.7M14 11a5 5 0 0 0-7.5-.5l-3 3a5 5 0 0 0 7 7l1.7-1.7",
  pencil: "M17 3a2.8 2.8 0 1 1 4 4L7.5 20.5 2 22l1.5-5.5Z",
  trash: "M3 6h18M8 6V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2M19 6l-1 14a2 2 0 0 1-2 2H8a2 2 0 0 1-2-2L5 6M10 11v6M14 11v6",
  file: "M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8ZM14 2v6h6M8 13h8M8 17h5",
  mail: "M4 4h16a2 2 0 0 1 2 2v12a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V6a2 2 0 0 1 2-2ZM22 6l-10 7L2 6",
  message: "M21 15a2 2 0 0 1-2 2H7l-4 4V5a2 2 0 0 1 2-2h14a2 2 0 0 1 2 2Z",
  volume: "M11 5 6 9H2v6h4l5 4ZM15.5 8.5a5 5 0 0 1 0 7M19 5a10 10 0 0 1 0 14",
  upload: "M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4M17 8l-5-5-5 5M12 3v12",
  check: "M20 6 9 17l-5-5",
  chart: "M3 3v18h18M8 17v-5M13 17V7M18 17v-8",
};

/** @param {keyof typeof ICONS} name @param {Color} color */
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
export function largeTitle(cx, title, subtitle) {
  const t = cx.theme();
  return v_flex()
    .gap_1()
    .px_4()
    .pt_3()
    .pb_2()
    .child(div().role("header").text_size(titleSize(cx)).font_bold().line_height(1.15).text_color(t.foreground).child(title))
    .when(Boolean(subtitle), (el) => el.child(div().text_sm().text_color(t.muted_foreground).child(subtitle)));
}

/**
 * A pushed page's bar, drawn the way each platform draws its own: iOS centers
 * the title and labels the back button with the previous page; Android leads
 * with an arrow and the title.
 * @param {Context} cx
 * @param {{ title: string, back: { label: string, on_click: (e: ClickEvent, cx: Context) => void }, trailing?: Element }} opts
 */
function navBar(cx, { title, back, trailing }) {
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

/**
 * A pushed page: its bar, then content that scrolls under it and clears the
 * home indicator and the keyboard. `intro` says what the scenario is.
 * @param {Context} cx
 * @param {{ title: string, intro?: string, children: (Element | null)[], footer?: Element }} opts
 */
export function page(cx, { title, intro, children, footer }) {
  const t = cx.theme();
  const win = cx.window();
  return v_flex()
    .size_full()
    .bg(t.background)
    .line_height(1.375)
    .child(navBar(cx, { title, back: { label: "Components", on_click: (_e, cx) => cx.navigator.pop() } }))
    .child(
      v_flex()
        .test_id("page")
        .flex_1()
        .overflow_y_scroll()
        .child(
          v_flex()
            .px_4()
            .pt_4()
            .pb(footer ? 24 : Math.max(win.keyboard_height, win.safe_area.bottom) + 32)
            .gap_6()
            .when(Boolean(intro), (el) => el.child(div().text_sm().text_color(t.muted_foreground).child(intro)))
            .children(children),
        ),
    )
    .when(Boolean(footer), (el) =>
      el.child(
        div()
          .px_4()
          .pt_3()
          .pb(Math.max(win.keyboard_height, win.safe_area.bottom) + 12)
          .border_t_1()
          .border_color(t.border)
          .bg(t.background)
          .child(footer),
      ),
    );
}

/** A section heading inside a page: what the next group is for. */
export function heading(cx, title, description) {
  const t = cx.theme();
  return v_flex()
    .gap_1()
    .child(div().role("header").text_base().font_semibold().text_color(t.foreground).child(title))
    .when(Boolean(description), (el) => el.child(div().text_sm().text_color(t.muted_foreground).child(description)));
}

/** A hairline-bordered surface for one object. */
export function card(cx) {
  const t = cx.theme();
  return v_flex().rounded_xl().border_1().border_color(t.border).bg(t.surface).overflow_hidden();
}

/**
 * A labeled group of rows: hairline-bordered, rows divided at the text column.
 * @param {Context} cx
 * @param {{ title?: string, footer?: string, rows: (Element | null)[], id?: string }} opts
 */
export function section(cx, { title, footer, rows, id }) {
  const t = cx.theme();
  const items = rows.filter(Boolean);
  return v_flex()
    .gap_2()
    .when(Boolean(title), (el) => el.child(div().role("header").px_4().text_sm().font_medium().text_color(t.muted_foreground).child(title)))
    .child(
      card(cx)
        .when(Boolean(id), (el) => el.test_id(id))
        .children(items.flatMap((row, i) => (i === 0 ? [row] : [div().h(1).ml(48).bg(t.border), row]))),
    )
    .when(Boolean(footer), (el) => el.child(div().px_4().text_sm().text_color(t.muted_foreground).child(footer)));
}

/**
 * One row: an optional icon in a fixed lane, the label and detail, then a trailing value or control.
 * @param {Context} cx
 * @param {{ id: string, icon?: keyof typeof ICONS, label: string, detail?: string, trailing?: Element, on_click?: (e: ClickEvent, cx: Context) => void, chevron?: boolean, disabled?: boolean }} opts
 */
export function row(cx, { id, icon: name, label, detail, trailing, on_click, chevron, disabled }) {
  const t = cx.theme();
  const body = h_flex()
    .gap_3()
    .px_4()
    .py_3()
    .min_h_12()
    .when(Boolean(name), (el) => el.child(div().w_5().flex_none().flex().justify_center().child(icon(name, t.muted_foreground))))
    .child(
      v_flex()
        .flex_1()
        .min_w_0()
        .gap_0p5()
        .child(
          div()
            .text_base()
            .text_color(disabled ? t.muted_foreground : t.foreground)
            .child(label),
        )
        .when(Boolean(detail), (el) => el.child(div().text_sm().text_color(t.muted_foreground).child(detail))),
    )
    .when(Boolean(trailing), (el) => el.child(trailing))
    .when(Boolean(chevron), (el) => el.child(icon("chevron_right", t.muted_foreground, 18)));
  if (!on_click) return body.when(Boolean(id), (el) => el.test_id(id));
  return Button.new(id)
    .test_id(id)
    .accessibility_label(detail ? `${label}, ${detail}` : label)
    .disabled(!!disabled)
    .when(!disabled, (el) => el.active((s) => s.bg(t.muted)).on_click(on_click))
    .child(body);
}

/**
 * A button at touch size. `variant` keeps the component's meaning (primary
 * for the default commit, outline and ghost for quieter actions, destructive
 * for a destructive commit); this only sets the 44 pt height and body text.
 * `danger` colors a quieter button's label for an action that destroys data
 * but still asks first.
 * @param {Context} cx
 * @param {string} id
 * @param {string} label
 * @param {{ variant?: "primary" | "secondary" | "outline" | "ghost" | "destructive", danger?: boolean, disabled?: boolean, loading?: boolean, icon?: keyof typeof ICONS, on_click?: (e: ClickEvent, cx: Context) => void, full?: boolean }} [opts]
 */
export function button(
  cx,
  id,
  label,
  { variant = "outline", danger = false, disabled = false, loading = false, icon: name, on_click, full = false } = {},
) {
  const t = cx.theme();
  const fg = danger
    ? t.destructive
    : {
        primary: t.primary_foreground,
        secondary: t.secondary_foreground,
        outline: t.foreground,
        ghost: t.foreground,
        destructive: t.destructive_foreground,
      }[variant];
  return Button.new(id)
    .test_id(id)
    .variant(variant)
    .disabled(disabled || loading)
    .h_auto()
    .min_h_11()
    .py_2()
    .px_4()
    .rounded_lg()
    .text_base()
    .when(danger, (el) => el.text_color(t.destructive))
    .when(full, (el) => el.w_full())
    .when(loading, (el) => el.child(Spinner.new().size_4()))
    .when(Boolean(name) && !loading, (el) => el.child(icon(name, fg, 18)))
    .child(label)
    .when(!disabled && !loading && Boolean(on_click), (el) => el.on_click(on_click));
}

/**
 * A labeled text field with optional help and an error that replaces it.
 * @param {Context} cx
 * @param {{ id: string, label: string, state?: import("native-shell").InputState, help?: string, error?: string | null, multiline?: boolean, input?: Element }} opts
 */
export function field(cx, { id, label, state, help, error, multiline = false, input }) {
  const t = cx.theme();
  const control = (input ?? Input.new(state))
    .test_id(id)
    .accessibility_label(label)
    .min_h_11()
    .px_3()
    .py_2()
    .rounded_lg()
    .border_1()
    .border_color(error ? t.destructive : t.input)
    .bg(t.background)
    .text_base()
    .text_color(t.foreground)
    .when(multiline, (el) => el.min_h_24());
  return v_flex()
    .gap_1p5()
    .min_w_0()
    .child(div().text_sm().font_medium().text_color(t.foreground).child(label))
    .child(control)
    .when(Boolean(error), (el) => el.child(div().test_id(`${id}-error`).role("alert").text_sm().text_color(t.destructive).child(error)))
    .when(!error && Boolean(help), (el) => el.child(div().text_sm().text_color(t.muted_foreground).child(help)));
}

/**
 * A large title's size: 30 pt, growing with the user's text size only up to
 * 1.6×, as native large titles do, so a long word never breaks mid-word.
 */
function titleSize(cx) {
  const scale = cx.window().font_scale;
  return (30 * Math.min(scale, 1.6)) / scale;
}
