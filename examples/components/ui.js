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
  speaker_low: "M11 5 6 9H2v6h4l5 4Z",
  speaker_high: "M11 5 6 9H2v6h4l5 4ZM15.5 8.5a5 5 0 0 1 0 7M19 5a10 10 0 0 1 0 14",
  upload: "M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4M17 8l-5-5-5 5M12 3v12",
  check: "M20 6 9 17l-5-5",
  receipt: "M4 2v20l2-1 2 1 2-1 2 1 2-1 2 1 2-1 2 1V2l-2 1-2-1-2 1-2-1-2 1-2-1-2 1ZM16 8h-6a2 2 0 1 0 0 4h4a2 2 0 1 1 0 4H8M12 17.5v-11",
  folder: "M20 20a2 2 0 0 0 2-2V8a2 2 0 0 0-2-2h-7.9a2 2 0 0 1-1.69-.9L9.6 3.9A2 2 0 0 0 7.93 3H4a2 2 0 0 0-2 2v13a2 2 0 0 0 2 2Z",
  photo:
    "M5 3h14a2 2 0 0 1 2 2v14a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2ZM21 15l-5-5L5 21M10 8.5a1.5 1.5 0 1 1-3 0 1.5 1.5 0 0 1 3 0Z",
  bell: "M6 8a6 6 0 0 1 12 0c0 7 3 9 3 9H3s3-2 3-9M10.3 21a1.9 1.9 0 0 0 3.4 0",
  user: "M19 21v-2a4 4 0 0 0-4-4H9a4 4 0 0 0-4 4v2M12 11a4 4 0 1 0 0-8 4 4 0 0 0 0 8Z",
  package: "M21 8 12 3 3 8v8l9 5 9-5ZM3 8l9 5 9-5M12 13v8",
  status: "M12 22a10 10 0 1 0 0-20 10 10 0 0 0 0 20ZM12 8v4M12 16h.01",
  chart: "M3 3v18h18M8 17v-5M13 17V7M18 17v-8",
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
 * A pushed screen's navigation bar, on the grouped background of the screens
 * that use it: the title centered, the back button a
 * chevron with the previous screen's title, which gives way when it would
 * crowd the title (as UIKit does).
 * @param {Context} cx
 * @param {{ title: string, back: { label: string, on_click: OnClick }, trailing?: Element }} opts
 */
export function navBar(cx, { title, back, trailing }) {
  const t = cx.theme();
  return v_flex()
    .pt(cx.window().safe_area.top)
    .bg(t.grouped_background)
    .border_b(0.5)
    .border_color(t.border)
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
            .child(label(cx, "headline", t.foreground, title).role("header").truncate()),
        )
        .child(
          Button.new("back")
            .test_id("back")
            .accessibility_label(`Back to ${back.label}`)
            .flex()
            .items_center()
            .gap_0p5()
            .py_2()
            .hit_slop(4)
            .active((s) => s.opacity(0.4))
            .on_click(back.on_click)
            .child(icon("chevron_left", t.primary, 26))
            .when(back.label.length + title.length <= 26, (el) => el.child(label(cx, "body", t.primary, back.label))),
        )
        .child(trailing ?? div()),
    );
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
 * The screen's one primary action: a filled 50 pt button with a 12 pt radius.
 * `tone` "tinted" is the secondary, tinted style; "destructive" commits a
 * destructive action.
 * @param {Context} cx @param {string} id @param {string} text
 * @param {{ on_click?: OnClick, disabled?: boolean, tone?: "primary" | "tinted" | "destructive", leading?: Element }} [opts]
 */
export function filledButton(cx, id, text, { on_click, disabled = false, tone = "primary", leading } = {}) {
  return Button.new(id)
    .test_id(id)
    .variant(tone)
    .size("large")
    .disabled(disabled)
    .gap_2()
    .w_full()
    .when(!disabled && Boolean(on_click), (el) => el.on_click(/** @type {OnClick} */ (on_click)))
    .when(Boolean(leading), (el) => el.child(leading))
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
 * A colored rounded square holding a white glyph, as iOS Settings marks its rows.
 * @param {Context} cx @param {keyof typeof ICONS} name @param {Color} color
 */
export function tile(cx, name, color) {
  const t = cx.theme();
  return div()
    .size_7()
    .flex_none()
    .flex()
    .items_center()
    .justify_center()
    .rounded(t.radius.sm)
    .bg(color)
    .child(icon(name, t.primary_foreground, 17));
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
 * A top-level scrolling screen with a large title that collapses: the title
 * scrolls away with the content and a compact bar with the centered title
 * takes its place, as iOS navigation bars do. The view keeps `collapsed` and
 * re-renders when `on_collapse` reports a change.
 * @param {Context} cx
 * @param {{ id: string, title: string, subtitle?: string, background: Color, collapsed: boolean, on_collapse: (collapsed: boolean, cx: Context) => void, children: (Element | null | false)[] }} opts
 */
export function scrollScreen(cx, { id, title, subtitle, background, collapsed, on_collapse, children }) {
  const t = cx.theme();
  const win = cx.window();
  return div()
    .relative()
    .size_full()
    .bg(background)
    .child(
      v_flex()
        .id(id)
        .test_id(id)
        .size_full()
        .overflow_y_scroll()
        .on_scroll((e, cx) => {
          const now = e.y > 52;
          if (now !== collapsed) on_collapse(now, cx);
        })
        .child(div().h(win.safe_area.top).flex_none())
        .child(largeTitle(cx, { title, subtitle }))
        .child(
          v_flex()
            .px_4()
            .pt_2()
            .pb(win.safe_area.bottom + 32)
            .gap_8()
            .children(/** @type {Element[]} */ (children.filter(Boolean))),
        ),
    )
    .child(
      v_flex()
        .absolute()
        .top_0()
        .left_0()
        .right_0()
        .bg(background)
        .child(div().h(win.safe_area.top))
        .when(collapsed, (el) =>
          el
            .border_b(0.5)
            .border_color(t.border)
            .child(
              h_flex()
                .min_h_11()
                .justify_center()
                .child(label(cx, "headline", t.foreground, title).role("header")),
            ),
        ),
    );
}

/**
 * A pushed page on the grouped background: its navigation bar, a sentence
 * saying what the scenario shows, the sections, and an optional bar at the
 * bottom for the page's one filled button, kept above the keyboard and the
 * home indicator.
 * @param {Context} cx
 * @param {{ title: string, intro?: string, children: (Element | null | false)[], footer?: Element }} opts
 */
export function page(cx, { title, intro, children, footer }) {
  const t = cx.theme();
  const win = cx.window();
  const bottom = Math.max(win.keyboard_height, win.safe_area.bottom);
  return v_flex()
    .size_full()
    .bg(t.grouped_background)
    .child(navBar(cx, { title, back: { label: "Components", on_click: (_e, cx) => cx.navigator.pop() } }))
    .child(
      v_flex()
        .test_id("page")
        .flex_1()
        .overflow_y_scroll()
        .child(
          v_flex()
            .px_4()
            .pt_5()
            .pb(footer ? 24 : bottom + 32)
            .gap_8()
            .when(Boolean(intro), (el) => el.child(label(cx, "subheadline", t.muted_foreground, intro ?? "").px_4()))
            .children(/** @type {Element[]} */ (children.filter(Boolean))),
        ),
    )
    .when(Boolean(footer), (el) =>
      el.child(
        div()
          .px_4()
          .pt_3()
          .pb(bottom + 12)
          .border_t(0.5)
          .border_color(t.border)
          .bg(t.grouped_background)
          .child(footer),
      ),
    );
}
