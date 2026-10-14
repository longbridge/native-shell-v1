// The example's presentation vocabulary: icons, bars, grouped rows, posters
// and the drawings that stand in for photos, maps and ticket codes.
//
// Every color comes from `cx.theme()` by role and every length from GPUI's
// rem scale (`p_4`, `gap_3`, …), so the same code is right in light and dark
// and follows the user's text size. Rows and controls are sized by padding
// and minimum heights, never fixed heights, so large text grows them instead
// of clipping. Pixel values appear only for platform geometry (safe areas,
// the keyboard) and for drawings that have a fixed aspect.

import { div, h_flex, v_flex, shape, Button, Spinner } from "native-shell";
import { Badge, DescriptionList, Empty, EmptyContent, EmptyDescription, EmptyHeader, EmptyMedia, EmptyTitle } from "gpui-component";
import { category } from "./catalog.js";
import { dayDate } from "./dates.js";
/** @import { ClickEvent, Color, Context, Element } from "native-shell" */
/** @import { Event } from "./catalog.js" */

/** Outline icons on a 24-unit grid, drawn with a 2-unit round stroke. */
const ICONS = {
  arrow_left: "M19 12H5M12 19l-7-7 7-7",
  chevron_left: "m15 18-6-6 6-6",
  chevron_right: "m9 18 6-6-6-6",
  compass: "M22 12a10 10 0 1 1-20 0 10 10 0 0 1 20 0ZM16.2 7.8l-2.1 6.3-6.3 2.1 2.1-6.3Z",
  calendar: "M8 2v4M16 2v4M3 10h18M5 4h14a2 2 0 0 1 2 2v14a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V6a2 2 0 0 1 2-2Z",
  calendar_plus: "M8 2v4M16 2v4M3 10h18M5 4h14a2 2 0 0 1 2 2v14a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V6a2 2 0 0 1 2-2ZM12 13v5M9.5 15.5h5",
  calendar_check: "M8 2v4M16 2v4M3 10h18M5 4h14a2 2 0 0 1 2 2v14a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V6a2 2 0 0 1 2-2ZM9 16l2 2 4-4",
  ticket: "M2 9a3 3 0 0 0 0 6v2a2 2 0 0 0 2 2h16a2 2 0 0 0 2-2v-2a3 3 0 0 0 0-6V7a2 2 0 0 0-2-2H4a2 2 0 0 0-2 2ZM13 5v2M13 17v2M13 11v2",
  share: "M4 12v8a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2v-8M16 6l-4-4-4 4M12 2v13",
  pin: "M20 10c0 6-8 12-8 12s-8-6-8-12a8 8 0 0 1 16 0ZM15 10a3 3 0 1 1-6 0 3 3 0 0 1 6 0Z",
  clock: "M22 12a10 10 0 1 1-20 0 10 10 0 0 1 20 0ZM12 6v6l4 2",
  wifi_off: "M12 20h.01M8.5 16.4a5 5 0 0 1 7 0M5 12.9a10 10 0 0 1 5.2-2.8M19 12.9a10 10 0 0 0-2-1.4M2 8.8a15 15 0 0 1 4.2-2.8M10.7 5.1A15 15 0 0 1 22 8.8M2 2l20 20",
  alert: "M22 12a10 10 0 1 1-20 0 10 10 0 0 1 20 0ZM12 8v4M12 16h.01",
  check: "M20 6 9 17l-5-5",
  check_circle: "M22 12a10 10 0 1 1-20 0 10 10 0 0 1 20 0ZM8 12l3 3 5-6",
  minus: "M5 12h14",
  plus: "M12 5v14M5 12h14",
  users: "M16 21v-2a4 4 0 0 0-4-4H6a4 4 0 0 0-4 4v2M13 7a4 4 0 1 1-8 0 4 4 0 0 1 8 0ZM22 21v-2a4 4 0 0 0-3-3.9M16 3.1a4 4 0 0 1 0 7.8",
  card: "M2 7a2 2 0 0 1 2-2h16a2 2 0 0 1 2 2v10a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2ZM2 10h20M6 15h4",
  music: "M9 18V5l12-2v13M9 18a3 3 0 1 1-6 0 3 3 0 0 1 6 0ZM21 16a3 3 0 1 1-6 0 3 3 0 0 1 6 0Z",
  mask: "M4 3h16v8a8 8 0 0 1-16 0ZM8 9h.01M16 9h.01M8.5 14a4 4 0 0 0 7 0",
  cup: "M17 8h1a4 4 0 1 1 0 8h-1M3 8h14v9a4 4 0 0 1-4 4H7a4 4 0 0 1-4-4ZM6 2v2M10 2v2M14 2v2",
  palette:
    "M12 22a10 10 0 1 1 10-10c0 2.8-2.2 4-4.5 4H15a2 2 0 0 0-1.5 3.3A1.6 1.6 0 0 1 12 22ZM7.5 10.5h.01M10.5 7.5h.01M15.5 8.5h.01",
  mic: "M12 2a3 3 0 0 0-3 3v7a3 3 0 0 0 6 0V5a3 3 0 0 0-3-3ZM19 10v2a7 7 0 0 1-14 0v-2M12 19v3",
  sun: "M16 12a4 4 0 1 1-8 0 4 4 0 0 1 8 0ZM12 2v2M12 20v2M4.9 4.9l1.4 1.4M17.7 17.7l1.4 1.4M2 12h2M20 12h2M6.3 17.7l-1.4 1.4M19.1 4.9l-1.4 1.4",
};

/** @typedef {keyof typeof ICONS} IconName */

/** @param {IconName} name @param {Color} color */
export function icon(name, color, size = 20) {
  return shape(ICONS[name], { view_box: [0, 0, 24, 24] })
    .stroke(color, 2)
    .w(size)
    .h(size)
    .flex_none()
    .accessibility_hidden();
}

/**
 * The status bar's backdrop: content scrolls beneath it rather than under the
 * system's clock and indicators.
 */
export function statusBar(cx, color) {
  return div()
    .h(cx.window().safe_area.top)
    .flex_none()
    .bg(color ?? cx.theme().background);
}

/** A top-level screen's title block, at the top of its scrolling content. */
export function largeTitle(cx, title, subtitle) {
  const t = cx.theme();
  return v_flex()
    .gap_0p5()
    .px_4()
    .pt_3()
    .pb_2()
    .child(div().role("header").text_size(titleSize(cx)).font_bold().line_height(1.15).text_color(t.foreground).child(title))
    .when(Boolean(subtitle), (el) => el.child(div().test_id("subtitle").text_sm().text_color(t.muted_foreground).child(subtitle)));
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

/** An icon button for a navigation bar: 44 pt target, tinted. */
export function barIcon(cx, id, name, label, onClick) {
  const t = cx.theme();
  return Button.new(id)
    .test_id(id)
    .accessibility_label(label)
    .size_11()
    .flex()
    .items_center()
    .justify_center()
    .rounded_md()
    .active((s) => s.opacity(0.5))
    .on_click(onClick)
    .child(icon(name, t.primary, 22));
}

/**
 * The tab bar: one item per top-level section, the current one in the tint.
 * @param {Context} cx
 * @param {{ id: string, label: string, icon: IconName, badge?: number }[]} items
 * @param {string} current
 * @param {(id: string, cx: Context) => void} onSelect
 */
export function tabBar(cx, items, current, onSelect) {
  const t = cx.theme();
  return h_flex()
    .test_id("tab-bar")
    .role("tab_list")
    .flex_none()
    .border_t_1()
    .border_color(t.border)
    .bg(t.background)
    .pt_1()
    .pb(Math.max(cx.window().safe_area.bottom, 8))
    .children(
      items.map((item) => {
        const selected = item.id === current;
        const color = selected ? t.primary : t.muted_foreground;
        return Button.new(`tab-${item.id}`)
          .test_id(`tab-${item.id}`)
          .role("tab")
          .aria_selected(selected)
          .accessibility_label(item.badge ? `${item.label}, ${item.badge} today` : item.label)
          .flex_1()
          .flex()
          .flex_col()
          .items_center()
          .gap_0p5()
          .py_1()
          .min_h_11()
          .active((s) => s.opacity(0.5))
          .on_click((_e, cx) => onSelect(item.id, cx))
          .child(Badge.new().count(item.badge ?? 0).size("small").child(icon(item.icon, color, 24)))
          .child(div().text_xs().font_medium().text_color(color).child(item.label));
      }),
    );
}

/**
 * A labeled group of rows, inset and rounded: rows divided at the text column.
 * @param {Context} cx
 * @param {{ title?: string, footer?: string, rows: (Element | null | false)[], id?: string, inset?: number }} opts
 */
export function section(cx, { title, footer, rows, id, inset = 16 }) {
  const t = cx.theme();
  const items = rows.filter(Boolean);
  return v_flex()
    .gap_2()
    .when(Boolean(title), (el) =>
      el.child(div().role("header").px_4().text_sm().font_medium().text_color(t.muted_foreground).child(title)),
    )
    .child(
      v_flex()
        .when(Boolean(id), (el) => el.test_id(id))
        .rounded_xl()
        .bg(t.surface)
        .overflow_hidden()
        .children(items.flatMap((row, i) => (i === 0 ? [row] : [div().h(1).ml(inset).bg(t.border), row]))),
    )
    .when(Boolean(footer), (el) => el.child(div().px_4().text_sm().text_color(t.muted_foreground).child(footer)));
}

/**
 * The page's one primary action: filled, 50 pt high, 12 pt corners.
 * @param {Context} cx
 * @param {{ id: string, label: string, on_click?: (e: ClickEvent, cx: Context) => void, disabled?: boolean, loading?: boolean, destructive?: boolean }} opts
 */
export function primaryButton(cx, { id, label, on_click, disabled = false, loading = false, destructive = false }) {
  const t = cx.theme();
  const off = disabled || loading;
  const bg = destructive ? t.destructive : t.primary;
  const fg = destructive ? t.destructive_foreground : t.primary_foreground;
  return Button.new(id)
    .test_id(id)
    .disabled(off)
    .accessibility_label(label)
    .w_full()
    .min_h(50)
    .py_3()
    .px_4()
    .flex()
    .items_center()
    .justify_center()
    .gap_2()
    .rounded(12)
    .bg(disabled ? t.secondary : bg)
    .text_base()
    .font_semibold()
    .text_color(disabled ? t.muted_foreground : fg)
    .when(loading, (el) => el.opacity(0.8).child(Spinner.new().size_4().text_color(fg)))
    .when(!off && Boolean(on_click), (el) => el.active((s) => s.opacity(0.7)).on_click(on_click))
    .child(label);
}

/**
 * A secondary action: tinted fill, same height as the primary one.
 * @param {Context} cx
 * @param {{ id: string, label: string, icon?: IconName, on_click: (e: ClickEvent, cx: Context) => void, destructive?: boolean }} opts
 */
export function tintedButton(cx, { id, label, icon: name, on_click, destructive = false }) {
  const t = cx.theme();
  const fg = destructive ? t.destructive : t.primary;
  return Button.new(id)
    .test_id(id)
    .accessibility_label(label)
    .flex_1()
    .min_h(50)
    .py_3()
    .px_4()
    .flex()
    .items_center()
    .justify_center()
    .gap_2()
    .rounded(12)
    .bg(destructive ? t.muted : t.accent)
    .text_base()
    .font_semibold()
    .text_color(fg)
    .active((s) => s.opacity(0.6))
    .on_click(on_click)
    .when(Boolean(name), (el) => el.child(icon(name, fg, 20)))
    .child(label);
}

/**
 * An event's poster: its category's colors, a large glyph and a calendar
 * chip with the date. Drawn rather than photographed, so it has no text that
 * could clash with the colors and needs no download.
 * @param {Context} cx
 * @param {Event} event
 * @param {{ height: number, rounded?: boolean, chip?: boolean }} opts
 */
export function poster(cx, event, { height, rounded = true, chip = true }) {
  const t = cx.theme();
  const c = category(event.category);
  const glyph = Math.round(height * 0.42);
  const date = dayDate(event.day);
  return div()
    .relative()
    .w_full()
    .h(height)
    .flex_none()
    .overflow_hidden()
    .when(rounded, (el) => el.rounded_xl())
    .bg(c.colors[0])
    .accessibility_hidden()
    // The second color as a disc over the corner: two tones without a gradient, which not every renderer paints.
    .child(
      div()
        .absolute()
        .left(-height * 0.35)
        .top(height * 0.45)
        .size(height * 1.3)
        .rounded_full()
        .bg(c.colors[1])
        .opacity(0.85),
    )
    // Rings around the glyph, like ripples, so posters of one category still read as one family.
    .child(
      shape("M100 10a90 90 0 1 1 0 180 90 90 0 0 1 0-180ZM100 40a60 60 0 1 1 0 120 60 60 0 0 1 0-120Z", { view_box: [0, 0, 200, 200] })
        .stroke("white", 3)
        .opacity(0.25)
        .absolute()
        .right(-height * 0.25)
        .top(-height * 0.1)
        .w(height * 1.2)
        .h(height * 1.2),
    )
    .child(
      div()
        .absolute()
        .right(height * 0.18)
        .top(height * 0.29)
        .child(shape(ICONS[glyphFor(c.glyph)], { view_box: [0, 0, 24, 24] }).stroke("white", 1.5).w(glyph).h(glyph)),
    )
    .when(chip, (el) =>
      el.child(
        v_flex()
          .absolute()
          .left_3()
          .top_3()
          .items_center()
          .px_2()
          .py_1()
          .rounded_lg()
          .bg(t.surface)
          .child(div().text_xs().font_semibold().text_color(t.destructive).child(MONTHS[date.getMonth()]))
          .child(div().text_lg().font_bold().line_height(1.1).text_color(t.foreground).child(String(date.getDate()))),
      ),
    );
}

const MONTHS = ["JAN", "FEB", "MAR", "APR", "MAY", "JUN", "JUL", "AUG", "SEP", "OCT", "NOV", "DEC"];

/** @returns {IconName} */
function glyphFor(name) {
  return /** @type {IconName} */ (name in ICONS ? name : "music");
}

/** A category's glyph at icon size. @param {string} id @param {Color} color */
export function categoryIcon(id, color, size = 20) {
  return icon(glyphFor(category(id).glyph), color, size);
}

/**
 * A street plan around the venue, drawn rather than loaded: a river, a park,
 * the street grid and a pin. It places the venue in its district; directions
 * open in the user's map app.
 * @param {Context} cx
 */
export function streetPlan(cx, seed) {
  const t = cx.theme();
  // Each venue gets its own plan: the river and the main road move with the seed.
  const n = [...seed].reduce((h, ch) => (h * 31 + ch.charCodeAt(0)) % 997, 7);
  const riverY = 40 + (n % 5) * 12;
  const roadX = 70 + (n % 7) * 22;
  return div()
    .test_id("map")
    .relative()
    .w_full()
    .h(150)
    .overflow_hidden()
    .bg(t.muted)
    .accessibility_hidden()
    .child(
      shape(`M0 ${riverY}C80 ${riverY - 25} 160 ${riverY + 30} 240 ${riverY}S360 ${riverY - 20} 400 ${riverY + 5}`, { view_box: [0, 0, 400, 150] })
        .stroke(t.info, 14)
        .opacity(0.35)
        .absolute()
        .left_0()
        .top_0()
        .size_full(),
    )
    .child(
      shape(`M${roadX + 40} ${riverY + 30}h70v45h-70Z`, { view_box: [0, 0, 400, 150] })
        .fill(t.success)
        .opacity(0.3)
        .absolute()
        .left_0()
        .top_0()
        .size_full(),
    )
    .child(
      shape(
        `M0 ${riverY + 22}H400M0 120H400M${roadX} 0V150M${roadX + 130} 0V150M${roadX - 60} ${riverY + 22}V150M${roadX + 220} ${riverY + 22}V150`,
        { view_box: [0, 0, 400, 150] },
      )
        .stroke(t.background, 6)
        .absolute()
        .left_0()
        .top_0()
        .size_full(),
    )
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
        .child(
          div()
            .size_10()
            .rounded_full()
            .bg(t.destructive)
            .border_2()
            .border_color(t.surface)
            .flex()
            .items_center()
            .justify_center()
            .shadow_md()
            .child(shape(ICONS.pin, { view_box: [0, 0, 24, 24] }).stroke(t.destructive_foreground, 2).w(20).h(20)),
        ),
    );
}

/**
 * A ticket's scannable code: a 25-module grid with the three finder squares,
 * the rest drawn from the ticket code. Dark on white in both appearances,
 * because door scanners read it that way.
 * @param {string} code
 */
export function ticketCode(code, size = 200) {
  const N = 25;
  let h = 2166136261;
  const bits = [];
  for (let i = 0; i < N * N; i++) {
    h = Math.imul(h ^ code.charCodeAt(i % code.length) ^ i, 16777619) >>> 0;
    bits.push((h >>> 13) & 1);
  }
  const finder = (x, y) => {
    for (const [fx, fy] of [[0, 0], [N - 7, 0], [0, N - 7]]) {
      const dx = x - fx;
      const dy = y - fy;
      if (dx >= -1 && dx <= 7 && dy >= -1 && dy <= 7) {
        if (dx < 0 || dy < 0 || dx > 6 || dy > 6) return 0;
        const ring = Math.min(dx, dy, 6 - dx, 6 - dy);
        return ring === 1 ? 0 : 1;
      }
    }
    return null;
  };
  let path = "";
  for (let y = 0; y < N; y++) {
    for (let x = 0; x < N; x++) {
      const f = finder(x, y);
      if (f ?? bits[y * N + x]) path += `M${x} ${y}h1v1h-1Z`;
    }
  }
  return div()
    .test_id("ticket-code")
    .role("image")
    .accessibility_label(`Ticket code ${code}`)
    .p_3()
    .rounded_lg()
    .bg("white")
    .child(shape(path, { view_box: [0, 0, N, N] }).fill("black").w(size).h(size));
}

/**
 * A collection with nothing in it: what it is, and what to do next.
 * @param {Context} cx
 * @param {{ id: string, icon: IconName, title: string, body: string, action?: { id: string, label: string, on_click: (cx: Context) => void } }} opts
 */
export function emptyState(cx, { id, icon: name, title, body, action }) {
  const t = cx.theme();
  return Empty.new()
    .test_id(id)
    .header(
      EmptyHeader.new()
        .media(EmptyMedia.new().variant("icon").child(icon(name, t.muted_foreground, 32)))
        .title(EmptyTitle.new().child(title))
        .description(EmptyDescription.new().child(body)),
    )
    .when(Boolean(action), (el) =>
      el.content(
        EmptyContent.new().child(
          Button.new(action.id)
            .test_id(action.id)
            .min_h_11()
            .px_5()
            .flex()
            .items_center()
            .rounded_full()
            .bg(t.accent)
            .text_base()
            .font_semibold()
            .text_color(t.primary)
            .active((s) => s.opacity(0.6))
            .on_click((_e, cx) => action.on_click(cx))
            .child(action.label),
        ),
      ),
    );
}

/**
 * Facts about one object as label–value rows. At large text sizes a label
 * beside its value leaves room for neither, so each label sits above its value.
 * @param {Context} cx
 * @param {string} id
 * @param {[label: string, value: string][]} items
 */
export function details(cx, id, items) {
  const list = DescriptionList.new().test_id(id);
  if (cx.window().font_scale > 1.35) list.vertical().columns(1);
  for (const [label, value] of items) list.item(label, value);
  return list;
}

/**
 * A large title's size: 30 pt, growing with the user's text size only up to
 * 1.6×, as native large titles do, so a long word never breaks mid-word.
 */
function titleSize(cx) {
  const scale = cx.window().font_scale;
  return (30 * Math.min(scale, 1.6)) / scale;
}
