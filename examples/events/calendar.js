// Calendar: the month at a glance, with a dot under each day that has events
// (in the tint when the user is going or has added one), and the selected
// day's agenda beneath.

import { div, v_flex } from "native-shell";
import { Calendar, CalendarState, Marker } from "gpui-component";
import { dayDate, isoDay, longDate, offsetOf } from "./dates.js";
import { eventRow } from "./discover.js";
import { isSaved, ticketsFor } from "./store.js";
import { emptyState, largeTitle, statusBar } from "./ui.js";
/** @import { Context } from "native-shell" */
/** @import { Event } from "./catalog.js" */

export class CalendarTab {
  constructor(app) {
    this.app = app;
    /** The selected day, as days from today. */
    this.day = 0;
    this.state = CalendarState();
    this.state.set_date(isoDay(dayDate(0)));
    // The program starts today, so earlier days have nothing to show.
    this.state.set_disabled_matcher((date) => date < isoDay(dayDate(0)));
    this.state.set_marker_matcher((date) => {
      const on = (this.app.program().events ?? []).filter((e) => isoDay(dayDate(e.day)) === date);
      if (on.length === 0) return null;
      return on.some((e) => ticketsFor(e).length > 0 || isSaved(e)) ? "tinted" : "default";
    });
    this.state.on("change", (date, cx) => {
      const [y, m, d] = String(date).split("-").map(Number);
      this.day = offsetOf(new Date(y, m - 1, d));
      cx.haptic("selection");
      cx.notify();
    });
  }

  select(offset, cx) {
    this.day = offset;
    this.state.set_date(isoDay(dayDate(offset)));
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
          .child(div().px_3().child(new Calendar(this.state).test_id("cal")))
          .child(this.agenda(cx, events, today))
          .child(div().h_8()),
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
      .child(new Marker("agenda-title").test_id("agenda-title").role("header").px_4().text_size(13).font_weight(600).child(longDate(date)))
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
