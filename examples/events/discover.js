// Discover: what's on. Featured events first, then everything upcoming by day,
// narrowed by category.
//
// The first load shows the screen's shape in placeholders, not a spinner, so
// nothing jumps when the program arrives. A failed refresh keeps the program
// on screen and says how old it is.

import { div, h_flex, v_flex, Button } from "native-shell";
import { Marker, Skeleton, Tag, WarningAlert } from "gpui-component";
import { CATEGORIES, CITY, FEW_SEATS, category } from "./catalog.js";
import { formatPrice, formatTime, relativeDay } from "./dates.js";
import { fromPrice, seatsLeft } from "./store.js";
import { categoryIcon, icon, largeTitle, poster, statusBar } from "./ui.js";
/** @import { Context } from "native-shell" */
/** @import { Event } from "./catalog.js" */

export class Discover {
  constructor(app) {
    this.app = app;
    /** The category filter, or null for everything. */
    this.filter = null;
  }

  /** @param {Context} cx */
  render(cx) {
    const t = cx.theme();
    const program = this.app.program();
    const events = program.events;
    return v_flex()
      .flex_1()
      .min_h_0()
      .child(statusBar(cx))
      .child(
        v_flex()
          .id("discover")
          .test_id("discover")
          .flex_1()
          .overflow_y_scroll()
          .when(events !== null, (el) => el.on_refresh((_e, cx) => this.app.refresh(cx)).refreshing(program.refreshing))
          .child(largeTitle(cx, "Discover", events === null ? `What’s on in ${CITY}` : `${events.length} events in ${CITY}`))
          .when(program.offline, (el) => el.child(this.offlineBanner(cx, program)))
          .child(events === null ? this.placeholder(cx) : this.content(cx, events))
          .child(div().h_8()),
      );
  }

  /** @param {Context} cx @param {Event[]} events */
  content(cx, events) {
    const t = cx.theme();
    const shown = this.filter === null ? events : events.filter((e) => e.category === this.filter);
    const days = [...new Set(shown.map((e) => e.day))].sort((a, b) => a - b);
    return v_flex()
      .gap_5()
      .child(this.featured(cx, events.filter((e) => e.featured)))
      .child(this.categories(cx))
      .child(
        v_flex()
          .test_id("upcoming")
          .gap_4()
          .children(
            days.map((day) =>
              v_flex()
                .gap_1()
                .child(new Marker(`day-${day}`).test_id(`day-${day}`).role("header").px_4().child(relativeDay(day)))
                .child(
                  v_flex()
                    .children(
                      shown
                        .filter((e) => e.day === day)
                        .flatMap((e, i) => [i > 0 ? div().h(1).ml(76).bg(t.border) : null, eventRow(cx, e, (cx) => this.app.open(e, cx))]),
                    ),
                ),
            ),
          ),
      );
  }

  /** @param {Context} cx @param {Event[]} events */
  featured(cx, events) {
    const t = cx.theme();
    const width = cx.window().width - 48;
    return v_flex()
      .gap_2()
      .child(div().role("header").px_4().text_xl().font_bold().text_color(t.foreground).child("Featured"))
      .child(
        h_flex()
          .id("featured")
          .test_id("featured")
          .overflow_x_scroll()
          .gap_3()
          .px_4()
          .children(
            events.map((e) =>
              Button.new(`featured-${e.id}`)
                .test_id(`featured-${e.id}`)
                .accessibility_label(`${e.title}, ${relativeDay(e.day)} at ${formatTime(e.time)}, ${e.venue.name}`)
                .w(width)
                .flex_none()
                .flex()
                .flex_col()
                .rounded_xl()
                .bg(t.surface)
                .overflow_hidden()
                .active((s) => s.opacity(0.7))
                .on_click((_e, cx) => this.app.open(e, cx))
                .child(poster(cx, e, { height: 150, rounded: false }))
                .child(
                  v_flex()
                    .gap_0p5()
                    .px_4()
                    .py_3()
                    .child(div().text_xs().font_semibold().text_color(t.primary).child(category(e.category).label.toUpperCase()))
                    .child(div().text_lg().font_semibold().text_color(t.foreground).truncate().child(e.title))
                    .child(
                      div()
                        .text_sm()
                        .text_color(t.muted_foreground)
                        .truncate()
                        .child(`${relativeDay(e.day)} · ${formatTime(e.time)} · ${e.venue.name}`),
                    ),
                ),
            ),
          ),
      );
  }

  /** @param {Context} cx */
  categories(cx) {
    const choices = [{ id: null, label: "All" }, ...CATEGORIES];
    return h_flex()
      .id("categories")
      .test_id("categories")
      .role("tab_list")
      .overflow_x_scroll()
      .gap_2()
      .px_4()
      .children(
        choices.map((c) =>
          Button.new(`category-${c.id ?? "all"}`)
            .test_id(`category-${c.id ?? "all"}`)
            .role("tab")
            .aria_selected(this.filter === c.id)
            .accessibility_label(c.label)
            .flex_none()
            .hit_slop(6)
            .active((s) => s.opacity(0.6))
            .on_click((_e, cx) => {
              this.filter = c.id;
              cx.haptic("selection");
              cx.notify();
            })
            .child(
              Tag.new()
                .variant(this.filter === c.id ? "primary" : "secondary")
                .size("large")
                .rounded_full()
                .px_4()
                .py_1p5()
                .child(c.label),
            ),
        ),
      );
  }

  /** @param {Context} cx */
  offlineBanner(cx, program) {
    const t = cx.theme();
    const at = new Date(program.at);
    const time = formatTime(`${at.getHours()}:${at.getMinutes()}`);
    return div()
      .px_4()
      .pb_3()
      .child(
        WarningAlert.new("offline", `Showing the program from ${time}. Bookings need a connection.`)
          .test_id("offline")
          .title("You’re offline")
          .icon(icon("wifi_off", t.warning, 20))
          .child(
            Button.new("retry-program")
              .test_id("retry-program")
              .flex_none()
              .self_center()
              .px_2()
              .py_2()
              .text_base()
              .font_semibold()
              .text_color(t.primary)
              .active((s) => s.opacity(0.5))
              .on_click((_e, cx) => this.app.refresh(cx))
              .child("Try Again"),
          ),
      );
  }

  /** The screen's shape while the first answer is on its way. @param {Context} cx */
  placeholder(cx) {
    return v_flex()
      .test_id("loading")
      .accessibility_label("Loading events")
      .gap_5()
      .child(
        v_flex()
          .gap_2()
          .px_4()
          .child(Skeleton.new().w(110).h(22))
          .child(Skeleton.new().h(230).rounded_xl()),
      )
      .child(
        h_flex()
          .gap_2()
          .px_4()
          .children([56, 72, 96, 64].map((w) => Skeleton.new().w(w).h(32).flex_none().rounded_full())),
      )
      .child(
        v_flex()
          .gap_3()
          .px_4()
          .child(Skeleton.new().w(80).h(16))
          .children(
            [0, 1, 2].map(() =>
              h_flex()
                .gap_3()
                .child(Skeleton.new().size(48).flex_none().rounded_lg())
                .child(
                  v_flex()
                    .flex_1()
                    .gap_2()
                    .child(Skeleton.new().h(16))
                    .child(Skeleton.new().w(160).h(12)),
                ),
            ),
          ),
      );
  }
}

/**
 * One event in a list: its category glyph, title, time and place, and price.
 * @param {Context} cx
 * @param {Event} e
 * @param {(cx: Context) => void} onOpen
 */
export function eventRow(cx, e, onOpen, { showDay = false, note = null } = {}) {
  const t = cx.theme();
  const c = category(e.category);
  const left = seatsLeft(e);
  const price = fromPrice(e);
  const when = showDay ? `${relativeDay(e.day)} · ${formatTime(e.time)}` : formatTime(e.time);
  return Button.new(`event-${e.id}`)
    .test_id(`event-${e.id}`)
    .accessibility_label(`${e.title}, ${when}, ${e.venue.name}${left === 0 ? ", sold out" : ""}`)
    .flex()
    .items_center()
    .gap_3()
    .px_4()
    .py_3()
    .active((s) => s.bg(t.muted))
    .on_click((_e, cx) => onOpen(cx))
    .child(
      div()
        .size(48)
        .flex_none()
        .rounded_lg()
        .bg(c.colors[0])
        .flex()
        .items_center()
        .justify_center()
        .child(categoryIcon(e.category, "white", 26)),
    )
    .child(
      v_flex()
        .flex_1()
        .min_w_0()
        .gap_0p5()
        .child(div().text_base().font_semibold().text_color(t.foreground).child(e.title))
        .child(div().text_sm().text_color(t.muted_foreground).child(`${when} · ${e.venue.name}`))
        .child(
          h_flex()
            .gap_2()
            .flex_wrap()
            .child(div().text_sm().font_medium().text_color(t.foreground).child(price === null ? "Sold out" : price === 0 ? "Free" : `From ${formatPrice(price)}`))
            .when(left > 0 && left <= FEW_SEATS, (el) => el.child(Tag.new().variant("warning").size("small").rounded_full().child(`${left} left`)))
            .when(Boolean(note), (el) => el.child(Tag.new().variant(note === "Going" ? "success" : "info").size("small").rounded_full().child(note))),
        ),
    )
    .child(icon("chevron_right", t.muted_foreground, 18));
}
