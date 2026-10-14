// Events: what's on in the city this week, and tickets for it.
//
// Three tabs for the three questions people bring: what's on (Discover), what
// am I doing when (Calendar), and where are my tickets (Tickets). An event
// opens as a pushed page with everything needed to decide, and booking is a
// short pushed flow that ends with the ticket. See README.md for the design
// notes.

import { View, v_flex } from "native-shell";
import { Discover } from "./discover.js";
import { CalendarTab } from "./calendar.js";
import { TicketsTab } from "./tickets.js";
import EventPage from "./event.js";
import { fetchProgram } from "./service.js";
import { tabBar } from "./ui.js";
/** @import { Context } from "native-shell" */
/** @import { Event } from "./catalog.js" */

const TABS = [
  { id: "discover", label: "Discover", icon: /** @type {const} */ ("compass") },
  { id: "calendar", label: "Calendar", icon: /** @type {const} */ ("calendar") },
  { id: "tickets", label: "Tickets", icon: /** @type {const} */ ("ticket") },
];

export default class Events extends View {
  init(_props, cx) {
    this.tab = "discover";
    /**
     * The program as last fetched. `events` is null until the first answer;
     * `offline` is set when a refresh failed and the list is the older copy.
     * @type {{ events: Event[] | null, at: number, refreshing: boolean, offline: boolean }}
     */
    this.program = { events: null, at: 0, refreshing: false, offline: false };
    const app = {
      program: () => this.program,
      refresh: (cx) => this.refresh(cx),
      open: (event, cx) => this.open(event, cx),
      show: (tab, cx) => this.show(tab, cx),
    };
    this.discover = new Discover(app);
    this.calendar = new CalendarTab(app);
    this.tickets = new TicketsTab(app);
    this.load(cx);
  }

  load(cx) {
    cx.spawn(async (cx) => {
      const { events, at } = await fetchProgram(cx);
      this.program = { events, at, refreshing: false, offline: false };
      cx.notify();
    });
  }

  /** Pull to refresh, and the offline banner's Try Again. Keeps the list it has when the network fails. */
  refresh(cx) {
    if (this.program.refreshing) return;
    this.program = { ...this.program, refreshing: true };
    cx.notify();
    cx.spawn(async (cx) => {
      try {
        const { events, at } = await fetchProgram(cx, { refresh: true });
        this.program = { events, at, refreshing: false, offline: false };
      } catch {
        this.program = { ...this.program, refreshing: false, offline: true };
      }
      cx.notify();
    });
  }

  /** @param {Event} event @param {Context} cx */
  open(event, cx) {
    cx.navigator.push(EventPage, { event, back: TABS.find((t) => t.id === this.tab)?.label ?? "Back" }, { title: event.title });
  }

  show(tab, cx) {
    if (tab === this.tab) return;
    this.tab = tab;
    cx.haptic("selection");
    cx.notify();
  }

  render(cx) {
    const t = cx.theme();
    const page = { discover: this.discover, calendar: this.calendar, tickets: this.tickets }[this.tab];
    return v_flex()
      .size_full()
      .bg(t.background)
      .line_height(1.375)
      .child(page.render(cx))
      .child(tabBar(cx, TABS, this.tab, (id, cx) => this.show(id, cx)));
  }
}
