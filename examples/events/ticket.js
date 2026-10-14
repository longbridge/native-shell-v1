// A ticket, as shown at the door: the code large on white, the event, the
// seats and the name on the booking. Cancelling asks first, naming the event
// and the refund, and can still be undone for a few seconds afterwards.

import { View, div, h_flex, v_flex, Button } from "native-shell";
import { SuccessAlert } from "gpui-component";
import { eventById } from "./catalog.js";
import { dayDate, formatPrice, longDate, timeRange } from "./dates.js";
import { cancelTicket, ticketById } from "./store.js";
import { appToast, emptyState, navBar, poster, ticketCode } from "./ui.js";
/** @import { Context } from "native-shell" */

const UNDO_MS = 6000;

export default class TicketPage extends View {
  /** @param {{ ticketId: number, booked?: boolean }} props */
  init(props) {
    this.ticketId = props.ticketId;
    this.booked = !!props.booked;
  }

  /** @param {Context} cx */
  confirmCancel(cx) {
    const ticket = ticketById(this.ticketId);
    if (!ticket) return;
    const event = eventById(ticket.eventId);
    const refund = ticket.total === 0 ? "Your spot goes to someone on the waiting list." : `${formatPrice(ticket.total)} goes back to Visa ending in 4242.`;
    cx.alert({
      title: `Cancel tickets for “${event.title}”?`,
      message: refund,
      actions: [
        { label: "Keep Tickets", style: "cancel" },
        {
          label: "Cancel Booking",
          style: "destructive",
          on_press: (cx) => {
            const restore = cancelTicket(ticket);
            cx.haptic("warning");
            cx.navigator.pop();
            appToast(cx, {
              id: "booking-cancelled",
              title: "Booking cancelled",
              description: event.title,
              duration: UNDO_MS,
              action: { label: "Undo", on_click: () => restore() },
            });
          },
        },
      ],
    });
  }

  render(cx) {
    const t = cx.theme();
    const ticket = ticketById(this.ticketId);
    const event = ticket ? eventById(ticket.eventId) : null;
    const page = v_flex().size_full().bg(t.grouped_background).line_height(1.375);
    const bar = navBar(cx, { title: "Ticket", back: { label: this.booked ? "Event" : "Tickets", on_click: (_e, cx) => cx.navigator.pop() } });
    if (!ticket || !event) {
      return page.child(bar).child(
        emptyState(cx, { id: "ticket-gone", icon: "ticket", title: "Booking cancelled", body: "This ticket is no longer valid." }),
      );
    }
    const tier = event.tiers.find((x) => x.id === ticket.tierId);
    return page.child(bar).child(
      v_flex()
        .id("ticket")
        .test_id("ticket")
        .flex_1()
        .overflow_y_scroll()
        .child(
          v_flex()
            .px_4()
            .pt_4()
            .pb(Math.max(cx.window().safe_area.bottom, 16) + 16)
            .gap_5()
            .when(this.booked, (el) =>
              el.child(
                SuccessAlert.new("booked", `We sent the tickets to ${ticket.attendee.email}. They’re also here, under Tickets.`)
                  .test_id("booked")
                  .title("You’re going!"),
              ),
            )
            .child(
              v_flex()
                .rounded_xl()
                .bg(t.surface)
                .overflow_hidden()
                .child(poster(cx, event, { height: 96, rounded: false }))
                .child(
                  v_flex()
                    .gap_1()
                    .px_4()
                    .pt_4()
                    .child(div().test_id("ticket-event").role("header").text_xl().font_bold().text_color(t.foreground).child(event.title))
                    .child(div().text_base().text_color(t.foreground).child(longDate(dayDate(event.day))))
                    .child(div().text_sm().text_color(t.muted_foreground).child(`${timeRange(event.time, event.minutes)} · ${event.venue.name}`)),
                )
                .child(
                  // The tear line between the stub and the code.
                  div().mx_4().my_4().h(1).border_t_1().border_dashed().border_color(t.border),
                )
                .child(
                  v_flex()
                    .items_center()
                    .gap_2()
                    .pb_5()
                    .child(ticketCode(ticket.code, 184))
                    .child(div().test_id("ticket-code-text").text_lg().font_semibold().font_family(t.typography.mono).text_color(t.foreground).child(ticket.code))
                    .child(
                      div()
                        .test_id("ticket-seats")
                        .text_sm()
                        .text_color(t.muted_foreground)
                        .child(`${ticket.quantity} × ${tier?.name ?? "Ticket"} · Admits ${ticket.quantity}`),
                    ),
                )
                .child(
                  h_flex()
                    .justify_between()
                    .px_4()
                    .py_3()
                    .border_t_1()
                    .border_color(t.border)
                    .child(div().text_sm().text_color(t.muted_foreground).child("Name"))
                    .child(div().text_sm().font_medium().text_color(t.foreground).child(ticket.attendee.name)),
                ),
            )
            .child(div().px_4().text_sm().text_color(t.muted_foreground).text_center().child("Turn the screen brightness up at the door. The code works offline."))
            .child(
              Button.new("cancel-booking")
                .test_id("cancel-booking")
                .flex()
                .items_center()
                .justify_center()
                .min_h_12()
                .py_3()
                .rounded_xl()
                .bg(t.surface)
                .text_base()
                .text_color(t.destructive)
                .active((s) => s.bg(t.muted))
                .on_click((_e, cx) => this.confirmCancel(cx))
                .child("Cancel Booking…"),
            ),
        ),
    );
  }
}
