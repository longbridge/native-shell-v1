// Pickers, as a table booking: a segmented control for a choice between two
// modes, pop-up menus for a value from a longer list, and the native date
// picker. The footer reads back the whole booking before it is sent.

import { View, Tabs, Select, DatePicker } from "native-shell";
import { filledButton, icon, page, row, section } from "../ui.js";
/** @import { Context } from "native-shell" */

const SITTINGS = ["Lunch", "Dinner"];
const TIMES = [
  ["12:00", "12:30", "13:00", "13:30"],
  ["18:00", "18:30", "19:00", "19:30", "20:00", "20:30"],
];
const MONTHS = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
const WEEKDAYS = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];

/** @param {number} offset */
function isoDate(offset) {
  const d = new Date();
  d.setDate(d.getDate() + offset);
  return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(d.getDate()).padStart(2, "0")}`;
}

/** @param {string} iso */
function readDate(iso) {
  const [y, m, d] = iso.split("-").map(Number);
  const date = new Date(y, m - 1, d);
  return `${WEEKDAYS[date.getDay()]}, ${MONTHS[m - 1]} ${d}`;
}

/** @param {string} hhmm */
function readTime(hhmm) {
  const [h, m] = hhmm.split(":").map(Number);
  return `${((h + 11) % 12) + 1}:${String(m).padStart(2, "0")} ${h < 12 ? "AM" : "PM"}`;
}

export default class PickersPage extends View {
  init() {
    this.sitting = 1;
    this.guests = "2";
    this.date = isoDate(3);
    this.time = "19:30";
    this.requested = false;
  }

  /** @param {{ sitting?: number, guests?: string, date?: string, time?: string }} patch @param {Context} cx */
  set(patch, cx) {
    Object.assign(this, patch, { requested: false });
    cx.notify();
  }

  render(cx) {
    const t = cx.theme();
    const times = TIMES[this.sitting];
    const guests = Number(this.guests);
    return page(cx, {
      title: "Pickers",
      intro: "A table booking at The Lantern. Every choice is visible at once, and the sentence below reads it back.",
      children: [
        Tabs.new("sitting")
          .test_id("sitting")
          .accessibility_label("Sitting")
          .items(SITTINGS)
          .selected(this.sitting)
          .on_change((i, cx) => this.set({ sitting: i, time: TIMES[i][Math.min(2, TIMES[i].length - 1)] }, cx)),
        section(cx, {
          footer: this.requested
            ? "Requested. The Lantern will confirm by text message within the hour."
            : `Table for ${guests} on ${readDate(this.date)} at ${readTime(this.time)}.`,
          footerId: "summary",
          rows: [
            row(cx, {
              id: "guests-row",
              label: "Guests",
              trailing: Select.new("guests")
                .test_id("guests")
                .accessibility_label("Guests")
                .items(Array.from({ length: 8 }, (_, i) => ({ label: `${i + 1} ${i === 0 ? "guest" : "guests"}`, value: String(i + 1) })))
                .value(this.guests)
                .on_change((v, cx) => this.set({ guests: v }, cx)),
            }),
            row(cx, {
              id: "date-row",
              label: "Date",
              trailing: DatePicker.new("date")
                .test_id("date")
                .accessibility_label("Date")
                .value(this.date)
                .on_change((v, cx) => this.set({ date: v }, cx)),
            }),
            row(cx, {
              id: "time-row",
              label: "Time",
              trailing: Select.new("time")
                .test_id("time")
                .accessibility_label("Time")
                .items(times.map((v) => ({ label: readTime(v), value: v })))
                .value(this.time)
                .on_change((v, cx) => this.set({ time: v }, cx)),
            }),
          ],
        }),
      ],
      footer: filledButton(cx, "request", this.requested ? "Requested" : "Request Booking", {
        disabled: this.requested,
        leading: this.requested ? icon("check", t.primary_foreground, 18) : undefined,
        on_click: (_e, cx) => {
          this.requested = true;
          cx.haptic("success");
          cx.notify();
        },
      }),
    });
  }
}
