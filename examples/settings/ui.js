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
  bell: "M6 8a6 6 0 0 1 12 0c0 7 3 9 3 9H3s3-2 3-9M10.3 21a1.94 1.94 0 0 0 3.4 0",
  clock: "M22 12a10 10 0 1 1-20 0 10 10 0 0 1 20 0ZM12 6v6l4 2",
  moon: "M12 3a6 6 0 0 0 9 9 9 9 0 1 1-9-9Z",
  hand: "M18 11V6a2 2 0 0 0-4 0M14 10V4a2 2 0 0 0-4 0v2M10 10.5V6a2 2 0 0 0-4 0v8M18 8a2 2 0 1 1 4 0v6a8 8 0 0 1-8 8h-2c-2.8 0-4.5-.86-5.99-2.34l-3.6-3.6a2 2 0 0 1 2.83-2.82L7 15",
  database: "M3 5c0-1.7 4-3 9-3s9 1.3 9 3-4 3-9 3-9-1.3-9-3ZM3 5v14c0 1.7 4 3 9 3s9-1.3 9-3V5M3 12c0 1.7 4 3 9 3s9-1.3 9-3",
  info: "M22 12a10 10 0 1 1-20 0 10 10 0 0 1 20 0ZM12 16v-4M12 8h.01",
  help: "M22 12a10 10 0 1 1-20 0 10 10 0 0 1 20 0ZM9.1 9a3 3 0 0 1 5.8 1c0 2-3 3-3 3M12 17h.01",
  external: "M7 17 17 7M7 7h10v10",
  user: "M19 21v-2a4 4 0 0 0-4-4H9a4 4 0 0 0-4 4v2M16 7a4 4 0 1 1-8 0 4 4 0 0 1 8 0Z",
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
