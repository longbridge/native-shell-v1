// Calendar: the month at a glance, with a dot under each day that has events
// (in the tint when the user is going or has added one), and the selected
// day's agenda beneath.

import { div, h_flex, v_flex, Button } from "native-shell";
import { Marker } from "gpui-component";
import { dayDate, isoDay, longDate, monthTitle, offsetOf } from "./dates.js";
import { eventRow } from "./discover.js";
import { isSaved, ticketsFor } from "./store.js";
import { emptyState, icon, largeTitle, statusBar } from "./ui.js";
/** @import { Context } from "native-shell" */
/** @import { Event } from "./catalog.js" */

const WEEKDAYS = ["S", "M", "T", "W", "T", "F", "S"];

export class CalendarTab {
  constructor(app) {
    this.app = app;
    /** The selected day, as days from today. */
    this.day = 0;
    /** The month shown: its first day. */
    this.month = firstOfMonth(dayDate(0));
  }

  select(offset, cx) {
    this.day = offset;
    this.month = firstOfMonth(dayDate(offset));
    cx.haptic("selection");
    cx.notify();
  }

  /** @param {Context} cx */
  render(cx) {
    const events = this.app.program().events ?? [];
    const today = events.filter((e) => e.day === this.day).sort((a, b) => a.time.localeCompare(b.time));
    return v_flex()
      .flex_1()
      .min_h_0()
      .child(statusBar(cx))
      .child(
        v_flex()
          .id("calendar")
          .test_id("calendar")
          .flex_1()
          .overflow_y_scroll()
          .child(largeTitle(cx, "Calendar"))
          .child(this.monthGrid(cx, events))
          .child(this.agenda(cx, events, today))
          .child(div().h_8()),
      );
  }

  /** @param {Context} cx @param {Event[]} events */
  monthGrid(cx, events) {
    const t = cx.theme();
    const first = this.month;
    const lead = first.getDay();
    const days = new Date(first.getFullYear(), first.getMonth() + 1, 0).getDate();
    const cells = [...Array(lead).fill(null), ...Array.from({ length: days }, (_, i) => new Date(first.getFullYear(), first.getMonth(), i + 1))];
    while (cells.length % 7 !== 0) cells.push(null);
    const weeks = [];
    for (let i = 0; i < cells.length; i += 7) weeks.push(cells.slice(i, i + 7));
    const shift = (n) => (_e, cx) => {
      this.month = new Date(first.getFullYear(), first.getMonth() + n, 1);
      cx.notify();
    };
    return v_flex()
      .px_3()
      .gap_1()
      .child(
        h_flex()
          .justify_between()
          .child(div().test_id("month").pl_1().text_base().font_semibold().text_color(t.foreground).child(monthTitle(first)))
          .child(
            h_flex()
              .child(navButton(cx, "previous-month", "chevron_left", "Previous month", shift(-1)))
              .child(navButton(cx, "next-month", "chevron_right", "Next month", shift(1))),
          ),
      )
      .child(h_flex().children(WEEKDAYS.map((d) => div().flex_1().text_center().text_xs().font_semibold().text_color(t.muted_foreground).child(d))))
      .children(
        weeks.map((week) =>
          h_flex().children(
            week.map((date) => {
              if (date === null) return div().flex_1();
              const offset = offsetOf(date);
              const on = events.filter((e) => e.day === offset);
              const mine = on.some((e) => ticketsFor(e).length > 0 || isSaved(e));
              const selected = offset === this.day;
              const isToday = offset === 0;
              return Button.new(`day-${isoDay(date)}`)
                .test_id(`day-${isoDay(date)}`)
                .accessibility_label(`${longDate(date)}${on.length > 0 ? `, ${on.length} ${on.length === 1 ? "event" : "events"}` : ""}`)
                .aria_selected(selected)
                .flex_1()
                .flex()
                .flex_col()
                .items_center()
                .py_1()
                .active((s) => s.opacity(0.5))
                .on_click((_e, cx) => this.select(offset, cx))
                .child(
                  div()
                    .size_9()
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded_full()
                    .when(selected, (el) => el.bg(isToday ? t.destructive : t.foreground))
                    .text_base()
                    .when(selected || isToday, (el) => el.font_semibold())
                    .text_color(selected ? t.background : isToday ? t.destructive : offset < 0 ? t.muted_foreground : t.foreground)
                    .child(String(date.getDate())),
                )
                .child(div().size_1p5().rounded_full().when(on.length > 0, (el) => el.bg(mine ? t.primary : t.muted_foreground)));
            }),
          ),
        ),
      );
  }

  /** @param {Context} cx @param {Event[]} events @param {Event[]} today */
  agenda(cx, events, today) {
    const t = cx.theme();
    const date = dayDate(this.day);
    const next = events.filter((e) => e.day > this.day).sort((a, b) => a.day - b.day)[0];
    return v_flex()
      .pt_5()
      .gap_2()
      .child(new Marker("agenda-title").test_id("agenda-title").role("header").px_4().child(longDate(date)))
      .child(
        today.length === 0
          ? emptyState(cx, {
              id: "empty-day",
              icon: "calendar",
              title: "Nothing on this day",
              body: next ? `The next event is ${next.title} on ${longDate(dayDate(next.day))}.` : "Nothing else is scheduled yet.",
              action: next ? { id: "next-event-day", label: "Go to Next Event", on_click: (cx) => this.select(next.day, cx) } : undefined,
            })
          : v_flex()
              .test_id("agenda")
              .children(
                today.flatMap((e, i) => [
                  i > 0 ? div().h(1).ml(76).bg(t.border) : null,
                  eventRow(cx, e, (cx) => this.app.open(e, cx), { note: ticketsFor(e).length > 0 ? "Going" : isSaved(e) ? "In Calendar" : null }),
                ]),
              ),
      );
  }
}

function firstOfMonth(date) {
  return new Date(date.getFullYear(), date.getMonth(), 1);
}

function navButton(cx, id, name, label, onClick) {
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
    .child(icon(name, t.primary, 20));
}
