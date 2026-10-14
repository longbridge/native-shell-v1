// A calendar, as a cabin booking: past and booked nights are dimmed, a stay
// is a range from check-in to check-out (a day visit is one date), and the
// summary under the calendar says what is still missing, what is wrong, or
// what the stay costs. The request button waits until the dates can be booked.

import { View, v_flex } from "native-shell";
import { Calendar, CalendarState, DescriptionList, DescriptionItem, GroupBox, Tab, TabBar } from "gpui-component";
import { filledButton, label, page } from "../ui.js";

const NIGHTLY = 145;
const CLEANING = 40;
const DAY_VISIT = 60;
const MONTHS = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
const WEEKDAYS = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];

/** @param {Date} d */
const iso = (d) => `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(d.getDate()).padStart(2, "0")}`;

/** @param {string} date */
function parse(date) {
  const [y, m, d] = date.split("-").map(Number);
  return new Date(y, m - 1, d);
}

/** @param {number} offset days from today */
function isoDate(offset) {
  const d = new Date();
  d.setDate(d.getDate() + offset);
  return iso(d);
}

/** @param {string} date @param {number} days */
function addDays(date, days) {
  const d = parse(date);
  d.setDate(d.getDate() + days);
  return iso(d);
}

/** @param {string} date */
const readDate = (date) => `${WEEKDAYS[parse(date).getDay()]}, ${MONTHS[parse(date).getMonth()]} ${parse(date).getDate()}`;

/** @param {string} from @param {string} to */
const nightsBetween = (from, to) => Math.round((parse(to).getTime() - parse(from).getTime()) / 86_400_000);

/** @param {number} n */
const money = (n) => `$${n.toFixed(2)}`;

export default class BookingPage extends View {
  init() {
    this.today = isoDate(0);
    // Two stays someone else has booked.
    this.booked = new Set([3, 4, 5, 12, 13].map(isoDate));
    this.calendar = CalendarState();
    this.calendar.set_date([null, null]);
    this.calendar.set_disabled_matcher((d) => d < this.today || this.booked.has(d));
    this.calendar.on("change", (_date, cx) => {
      this.requested = false;
      cx.notify();
    });
    this.mode = 0;
    this.requested = false;
  }

  /** @param {number} mode @param {import("native-shell").Context} cx */
  setMode(mode, cx) {
    this.mode = mode;
    this.requested = false;
    this.calendar.set_date(mode === 0 ? [null, null] : null);
    cx.notify();
  }

  /** What the dates come to: a prompt, a problem, or the stay and its price. */
  summary() {
    const date = this.calendar.date();
    if (!Array.isArray(date)) {
      return date === null ? { prompt: "Choose the day of your visit." } : { rows: [["Date", readDate(date)]], total: DAY_VISIT };
    }
    const [from, to] = date;
    if (from === null) return { prompt: "Choose your check-in date." };
    if (to === null) return { prompt: `Check-in ${readDate(from)}. Now choose your check-out date.` };
    const nights = nightsBetween(from, to);
    for (let i = 1; i < nights; i++) {
      const night = addDays(from, i);
      if (this.booked.has(night)) return { error: `${readDate(night)} is already booked. Choose dates on one side of it.` };
    }
    return {
      rows: [
        ["Check-in", readDate(from)],
        ["Check-out", readDate(to)],
        ["Nights", `${nights} × ${money(NIGHTLY)}`],
        ["Cleaning", money(CLEANING)],
      ],
      total: nights * NIGHTLY + CLEANING,
    };
  }

  render(cx) {
    const t = cx.theme();
    const s = this.summary();
    return page(cx, {
      title: "Booking",
      intro: "Fernhill Cabin, sleeps four. Dimmed dates are past or already booked.",
      children: [
        new TabBar("stay")
          .variant("segmented")
          .test_id("stay")
          .selected_index(this.mode)
          .on_change((i, cx) => this.setMode(i, cx))
          .child(new Tab().label("Overnight"))
          .child(new Tab().label("Day Visit")),
        v_flex()
          .rounded(t.radius.md)
          .bg(t.surface)
          .px_2()
          .py_3()
          .child(new Calendar(this.calendar).test_id("calendar")),
        s.rows
          ? new GroupBox().title(this.mode === 0 ? "Your stay" : "Your visit").child(
              new DescriptionList()
                .test_id("summary")
                .children(s.rows.map(([k, v]) => new DescriptionItem(k).value(v)))
                .item("Total", label(cx, "headline", t.foreground, money(s.total ?? 0)).test_id("total")),
            )
          : label(cx, "subheadline", s.error ? t.destructive : t.muted_foreground, s.error ?? s.prompt ?? "")
              .test_id(s.error ? "summary-error" : "summary-prompt")
              .when(Boolean(s.error), (el) => el.role("alert"))
              .px_4(),
        this.requested
          ? label(cx, "subheadline", t.muted_foreground, "Requested. Fernhill replies within a day; nothing is charged until then.")
              .test_id("requested")
              .role("alert")
              .px_4()
          : null,
      ],
      footer: filledButton(cx, "request", this.requested ? "Requested" : "Request Booking", {
        disabled: !s.rows || this.requested,
        on_click: (_e, cx) => {
          this.requested = true;
          cx.haptic("success");
          cx.notify();
        },
      }),
    });
  }
}
