// The example's presentation vocabulary: icons, text styles, bars, grouped
// sections, product thumbnails, prices and the quantity stepper.
//
// Every color comes from `cx.theme()` by role and every text size from the
// iOS text styles (`cx.theme().typography`), so the same code is right in
// light and dark and follows the user's text size. Rows are sized by padding,
// never by a fixed height, so they grow with large text instead of clipping
// it. Pixel values appear only for platform geometry: safe areas, the
// keyboard and the photos' square frames.

import { div, h_flex, v_flex, shape, image, Button } from "native-shell";
import { Rating, Tag } from "gpui-component";
import { money } from "./store.js";
/** @import { ClickEvent, Color, Context, Element } from "native-shell" */

/** Outline icons on a 24-unit grid, drawn with a 2-unit round stroke. */
const ICONS = {
  shop: "M3 9h18l-1.5 11a2 2 0 0 1-2 1.7h-11a2 2 0 0 1-2-1.7ZM8 9V7a4 4 0 0 1 8 0v2",
  search: "M11 19a8 8 0 1 0 0-16 8 8 0 0 0 0 16ZM21 21l-4.3-4.3",
  cart: "M2 3h3l2.7 12.4a2 2 0 0 0 2 1.6h8.6a2 2 0 0 0 2-1.5L22 7H6M10 21h.01M18 21h.01",
  user: "M19 21v-2a4 4 0 0 0-4-4H9a4 4 0 0 0-4 4v2M16 7a4 4 0 1 1-8 0 4 4 0 0 1 8 0Z",
  chevron_left: "m15 18-6-6 6-6",
  chevron_right: "m9 18 6-6-6-6",
  arrow_left: "M19 12H5M12 19l-7-7 7-7",
  plus: "M12 5v14M5 12h14",
  minus: "M5 12h14",
  close: "M18 6 6 18M6 6l12 12",
  trash: "M3 6h18M8 6V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2M19 6l-1 14a2 2 0 0 1-2 2H8a2 2 0 0 1-2-2L5 6M10 11v6M14 11v6",
  truck: "M1 4h14v12H1ZM15 9h4l3 3v4h-7M5.5 21a2 2 0 1 0 0-4 2 2 0 0 0 0 4ZM18.5 21a2 2 0 1 0 0-4 2 2 0 0 0 0 4Z",
  return: "M9 14 4 9l5-5M4 9h11a5 5 0 0 1 0 10h-3",
  check_circle: "M22 12a10 10 0 1 1-20 0 10 10 0 0 1 20 0ZM8 12l3 3 5-6",
  package: "m16.5 9.4-9-5.2M21 16V8a2 2 0 0 0-1-1.7l-7-4a2 2 0 0 0-2 0l-7 4A2 2 0 0 0 3 8v8a2 2 0 0 0 1 1.7l7 4a2 2 0 0 0 2 0l7-4a2 2 0 0 0 1-1.7ZM3.3 7 12 12l8.7-5M12 22V12",
  wifi_off: "M2 2l20 20M8.5 16.4a5 5 0 0 1 7 0M5 12.9a10 10 0 0 1 5.2-2.8M19 12.9a10 10 0 0 0-2.1-1.6M2 8.8a15 15 0 0 1 4.2-2.6M22 8.8A15 15 0 0 0 11 4.7M12 20h.01",
  tag: "M12.6 2.6A2 2 0 0 0 11.2 2H4a2 2 0 0 0-2 2v7.2a2 2 0 0 0 .6 1.4l8.7 8.7a2.4 2.4 0 0 0 3.4 0l6.6-6.6a2.4 2.4 0 0 0 0-3.4ZM7.5 8a.5.5 0 1 0 0-1 .5.5 0 0 0 0 1Z",
  card: "M2 5h20v14H2ZM2 10h20",
  pin: "M20 10c0 6-8 12-8 12s-8-6-8-12a8 8 0 0 1 16 0ZM12 13a3 3 0 1 0 0-6 3 3 0 0 0 0 6Z",
  flask: "M9 3h6M10 3v6l-5 9a2 2 0 0 0 1.7 3h10.6a2 2 0 0 0 1.7-3l-5-9V3",
};

/** @typedef {"large_title" | "title1" | "title2" | "title3" | "headline" | "body" | "callout" | "subheadline" | "footnote" | "caption1" | "caption2"} TextRole */

/** @param {keyof typeof ICONS} name @param {Color} color */
export function icon(name, color, size = 20) {
  return shape(ICONS[name], { view_box: [0, 0, 24, 24] })
    .stroke(color, 2)
    .w(size)
    .h(size)
    .flex_none()
    .accessibility_hidden();
}

/**
 * Text in one of the iOS text styles (`headline`, `body`, `footnote`, …),
 * in the label color unless told otherwise.
 * @param {Context} cx
 * @param {TextRole} role
 * @param {string} content
 * @param {Color} [color]
 */
export function text(cx, role, content, color) {
  const t = cx.theme();
  const style = t.typography[role];
  const el = div().text_size(style.size).text_color(color ?? t.foreground);
  if (style.weight >= 700) el.font_bold();
  else if (style.weight >= 600) el.font_semibold();
  return el.child(content);
}

/** The status bar's backdrop: content scrolls beneath it rather than under the system's clock. */
export function statusBar(cx) {
  return div().h(cx.window().safe_area.top).flex_none().bg(cx.theme().background);
}

/** A tab's large title, at the top of its scrolling content, with an optional line under it. */
export function largeTitle(cx, title, subtitle) {
  const t = cx.theme();
  return v_flex()
    .gap_1()
    .px_4()
    .pt_2()
    .pb_2()
    .child(text(cx, "large_title", title).text_size(titleSize(cx)).role("header").line_height(1.15))
    .when(Boolean(subtitle), (el) => el.child(text(cx, "subheadline", subtitle, t.muted_foreground).test_id("subtitle")));
}

/**
 * A pushed page's bar, drawn the way each platform draws its own: iOS centers
 * the title and labels the back button with the previous page; Android leads
 * with an arrow and the title.
 * @param {Context} cx
 * @param {{ title: string, back: { label: string, on_click: (e: ClickEvent, cx: Context) => void, disabled?: boolean } }} opts
 */
export function navBar(cx, { title, back }) {
  const t = cx.theme();
  const ios = cx.window().platform !== "android";
  const backButton = Button.new("back")
    .test_id("back")
    .accessibility_label(`Back to ${back.label}`)
    .disabled(!!back.disabled)
    .flex()
    .items_center()
    .gap_1()
    .py_2()
    .px_2()
    .rounded_md()
    .hit_slop(4)
    .when(!back.disabled, (el) => el.active((s) => s.opacity(0.5)).on_click(back.on_click))
    .when(Boolean(back.disabled), (el) => el.opacity(0.4))
    .when(ios, (el) => el.text_color(t.primary).child(icon("chevron_left", t.primary, 24)))
    // As UIKit does, the previous page's name gives way when it would crowd the title.
    .when(ios && back.label.length + title.length <= 26, (el) => el.child(text(cx, "body", back.label, t.primary)))
    .when(!ios, (el) => el.child(icon("arrow_left", t.foreground, 24)));
  const heading = text(cx, "headline", title).role("header").truncate();
  return v_flex()
    .pt(cx.window().safe_area.top)
    .bg(t.background)
    .border_b_1()
    .border_color(t.border)
    .child(
      ios
        ? h_flex()
            .relative()
            .min_h_11()
            .px_2()
            .child(div().absolute().left_0().right_0().top_0().bottom_0().flex().items_center().justify_center().px_24().child(heading))
            .child(backButton)
        : h_flex().min_h_12().py_1().px_2().gap_2().child(backButton).child(div().flex_1().min_w_0().child(heading)),
    );
}

/**
 * An inset grouped section: a header in secondary text, rows on the surface
 * color divided by hairlines inset to the text, and an optional footer.
 * @param {Context} cx
 * @param {{ title?: string, footer?: string | Element, rows: (Element | null | false)[], id?: string, inset?: number }} opts
 */
export function section(cx, { title, footer, rows, id, inset = 16 }) {
  const t = cx.theme();
  const items = rows.filter(Boolean);
  return v_flex()
    .gap_2()
    .when(Boolean(title), (el) => el.child(text(cx, "footnote", title.toUpperCase(), t.muted_foreground).role("header").px_4()))
    .child(
      v_flex()
        .when(Boolean(id), (el) => el.test_id(id))
        .rounded_xl()
        .bg(t.surface)
        .overflow_hidden()
        .children(items.flatMap((row, i) => (i === 0 ? [row] : [div().h(1).ml(inset).bg(t.border), row]))),
    )
    .when(Boolean(footer), (el) =>
      el.child(typeof footer === "string" ? text(cx, "footnote", footer, t.muted_foreground).px_4() : footer),
    );
}

/**
 * A label and a trailing value on one row of a section, at least 44 pt high.
 * @param {Context} cx
 * @param {string} label
 * @param {string} value
 * @param {{ id?: string, strong?: boolean, color?: Color }} [opts]
 */
export function valueRow(cx, label, value, { id, strong = false, color } = {}) {
  const t = cx.theme();
  return h_flex()
    .gap_3()
    .px_4()
    .py_3()
    .min_h_11()
    .child(text(cx, strong ? "headline" : "body", label).flex_1().min_w_0())
    .child(
      text(cx, strong ? "headline" : "body", value, color ?? (strong ? t.foreground : t.muted_foreground))
        .flex_none()
        .when(Boolean(id), (el) => el.test_id(id)),
    );
}

/**
 * A product photo in a rounded square frame on the muted fill, so it reads before the image loads.
 * @param {Context} cx
 * @param {string} src
 * @param {number} size
 * @param {{ radius?: number, label?: string }} [opts]
 */
export function photoFrame(cx, src, size, { radius = 12, label } = {}) {
  return div()
    .size(size)
    .flex_none()
    .rounded(radius)
    .overflow_hidden()
    .bg(cx.theme().muted)
    .child(
      image(src)
        .size_full()
        .object_fit("cover")
        .when(Boolean(label), (el) => el.accessibility_label(label))
        .when(!label, (el) => el.accessibility_hidden()),
    );
}

/**
 * A price, with the earlier price struck through beside it when on sale.
 * @param {Context} cx
 * @param {{ price: number, was?: number }} product
 */
export function price(cx, product, /** @type {TextRole} */ role = "subheadline") {
  const t = cx.theme();
  const sale = product.was !== undefined && product.was > product.price;
  return h_flex()
    .gap_1p5()
    .flex_wrap()
    .accessibility_label(sale ? `${money(product.price)}, was ${money(product.was)}` : money(product.price))
    .child(text(cx, role, money(product.price), sale ? t.destructive : t.foreground).font_semibold())
    .when(sale, (el) => el.child(text(cx, role, money(product.was), t.muted_foreground).line_through()));
}

/** "New" or "Sale −20%", for a product card or page. */
export function productTag(product) {
  if (product.was) {
    return Tag.new()
      .variant("danger")
      .size("small")
      .child(`Sale −${Math.round((1 - product.price / product.was) * 100)}%`);
  }
  if (product.isNew) return Tag.new().variant("info").size("small").child("New");
  return null;
}

/**
 * A read-only rating: stars to the nearest half, then the score and how many
 * reviews it averages. Announced as one phrase.
 * @param {Context} cx
 * @param {string} id
 * @param {number} rating
 * @param {number} reviews
 */
export function stars(cx, id, rating, reviews) {
  const t = cx.theme();
  const count = String(reviews).replace(/\B(?=(\d{3})+(?!\d))/g, ",");
  return h_flex()
    .gap_1()
    .accessibility_label(`Rated ${rating} out of 5, ${count} reviews`)
    .child(Rating.new(`${id}-rating`).value(rating).size("xsmall").disabled().accessibility_hidden())
    .child(text(cx, "footnote", `${rating.toFixed(1)} · ${count}`, t.muted_foreground));
}

/**
 * The quantity stepper: minus, the count, plus, each a 44 pt target. The
 * ends disable at the limits rather than doing nothing.
 * @param {Context} cx
 * @param {{ id: string, value: number, min?: number, max: number, label: string, on_change: (value: number, cx: Context) => void, disabled?: boolean }} opts
 */
export function quantityStepper(cx, { id, value, min = 1, max, label, on_change, disabled = false }) {
  const t = cx.theme();
  const step = (dir, name, can) =>
    Button.new(`${id}-${dir < 0 ? "minus" : "plus"}`)
      .test_id(`${id}-${dir < 0 ? "minus" : "plus"}`)
      .accessibility_label(`${dir < 0 ? "Decrease" : "Increase"} ${label}`)
      .disabled(!can)
      .size_11()
      .flex()
      .items_center()
      .justify_center()
      .rounded_full()
      .when(can, (el) => el.active((s) => s.bg(t.border)).on_click((_e, cx) => {
        cx.haptic("selection");
        on_change(value + dir, cx);
      }))
      .child(icon(name, can ? t.foreground : t.border, 16));
  return h_flex()
    .flex_none()
    .rounded_full()
    .bg(t.secondary)
    .accessibility_label(label)
    .accessibility_value(String(value))
    .child(step(-1, "minus", !disabled && value > min))
    .child(text(cx, "headline", String(value)).test_id(`${id}-value`).min_w_6().text_center())
    .child(step(1, "plus", !disabled && value < max));
}

/** A large title's size: 34 pt, growing with the user's text size only up to 1.6×, as native large titles do. */
function titleSize(cx) {
  const scale = cx.window().font_scale;
  return (cx.theme().typography.large_title.size * Math.min(scale, 1.6)) / scale;
}
