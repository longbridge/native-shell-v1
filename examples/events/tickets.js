// Tickets: everything booked on this device, soonest first, one tap from the
// code the door staff scan.

import { div, h_flex, v_flex, Button } from "native-shell";
import { ErrorAlert } from "gpui-component";
import { category, eventById } from "./catalog.js";
import { formatTime, relativeDay } from "./dates.js";
import { allTickets, failedToSave } from "./store.js";
import TicketPage from "./ticket.js";
import { categoryIcon, emptyState, icon, largeTitle, statusBar } from "./ui.js";
/** @import { Context } from "native-shell" */
/** @import { Ticket } from "./store.js" */

export class TicketsTab {
  constructor(app) {
    this.app = app;
  }

  /** @param {Context} cx */
  render(cx) {
    const tickets = allTickets();
    const count = tickets.reduce((n, t) => n + t.quantity, 0);
    return v_flex()
      .flex_1()
      .min_h_0()
      .child(statusBar(cx))
      .child(
        v_flex()
          .id("tickets")
          .test_id("tickets")
          .flex_1()
          .overflow_y_scroll()
          .child(largeTitle(cx, "Tickets", tickets.length === 0 ? null : `${count} ${count === 1 ? "ticket" : "tickets"} for ${tickets.length} ${tickets.length === 1 ? "event" : "events"}`))
          .when(failedToSave(), (el) =>
            el.child(
              div()
                .px_4()
                .pb_3()
                .child(
                  ErrorAlert.new("save-error", "They’re here until the app closes. Free some space on the device.")
                    .test_id("save-error")
                    .title("Couldn’t save your tickets"),
                ),
            ),
          )
          .child(
            tickets.length === 0
              ? emptyState(cx, {
                  id: "no-tickets",
                  icon: "ticket",
                  title: "No tickets yet",
                  body: "Events you book appear here, ready to show at the door.",
                  action: { id: "browse", label: "Browse Events", on_click: (cx) => this.app.show("discover", cx) },
                })
              : v_flex()
                  .gap_3()
                  .px_4()
                  .pt_1()
                  .pb_8()
                  .children(tickets.map((ticket) => ticketCard(cx, ticket))),
          ),
      );
  }
}

/** @param {Context} cx @param {Ticket} ticket */
function ticketCard(cx, ticket) {
  const t = cx.theme();
  const event = eventById(ticket.eventId);
  const tier = event.tiers.find((x) => x.id === ticket.tierId);
  const when = `${relativeDay(event.day)} · ${formatTime(event.time)}`;
  return Button.new(`ticket-${ticket.id}`)
    .test_id(`ticket-${ticket.id}`)
    .accessibility_label(`${event.title}, ${when}, ${ticket.quantity} × ${tier?.name ?? "ticket"}`)
    .flex()
    .items_center()
    .gap_3()
    .p_3()
    .rounded_xl()
    .bg(t.surface)
    .active((s) => s.bg(t.muted))
    .on_click((_e, cx) => cx.navigator.push(TicketPage, { ticketId: ticket.id }, { title: event.title }))
    .child(
      div()
        .size_12()
        .flex_none()
        .rounded_lg()
        .bg(category(event.category).colors[0])
        .flex()
        .items_center()
        .justify_center()
        .child(categoryIcon(event.category, "white", 26)),
    )
    .child(
      v_flex()
        .flex_1()
        .min_w_0()
        .gap_0p5()
        .child(div().text_base().font_semibold().text_color(t.foreground).child(event.title))
        .child(div().text_sm().text_color(t.muted_foreground).child(when))
        .child(
          h_flex()
            .gap_1()
            .child(icon("ticket", t.muted_foreground, 14))
            .child(div().text_sm().text_color(t.muted_foreground).child(`${ticket.quantity} × ${tier?.name ?? "Ticket"}`)),
        ),
    )
    .child(icon("chevron_right", t.muted_foreground, 18));
}
