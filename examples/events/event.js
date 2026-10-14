// An event: everything needed to decide—when, where, how much, how full—then
// one primary action, Get Tickets, pinned above the home indicator.

import { View, div, h_flex, v_flex, Button } from "native-shell";
import { SuccessAlert, Tag } from "gpui-component";
import { FEW_SEATS, category } from "./catalog.js";
import { dayDate, formatPrice, longDate, timeRange } from "./dates.js";
import BookingPage from "./booking.js";
import TicketPage from "./ticket.js";
import { fromPrice, isSaved, seatsLeft, setSaved, ticketsFor } from "./store.js";
import { appToast, barIcon, icon, navBar, poster, primaryButton, streetPlan, tintedButton } from "./ui.js";
import { descriptionList } from "./kit.js";
/** @import { Context } from "native-shell" */
/** @import { Event } from "./catalog.js" */

export default class EventPage extends View {
  /** @param {{ event: Event, back: string }} props */
  init(props) {
    this.event = props.event;
    this.back = props.back;
  }

  share(cx) {
    cx.write_to_clipboard(`https://events.example.org/e/${this.event.id}`);
    // Android 13+ confirms clipboard writes itself.
    if (!cx.window().clipboard_feedback) cx.push_toast({ id: "link-copied", title: "Link copied", level: "success" });
  }

  toggleCalendar(cx) {
    const on = !isSaved(this.event);
    setSaved(this.event, on);
    cx.haptic(on ? "success" : "light");
    appToast(cx, {
      id: "calendar",
      title: on ? "Added to Calendar" : "Removed from Calendar",
      level: on ? "success" : "info",
      action: { label: "Undo", on_click: () => setSaved(this.event, !on) },
    });
  }

  render(cx) {
    const t = cx.theme();
    const e = this.event;
    const left = seatsLeft(e);
    const price = fromPrice(e);
    const mine = ticketsFor(e);
    const saved = isSaved(e);
    const win = cx.window();
    return v_flex()
      .size_full()
      .bg(t.grouped_background)
      .line_height(1.375)
      .child(
        navBar(cx, {
          title: e.title,
          back: { label: this.back, on_click: (_e, cx) => cx.navigator.pop() },
          trailing: barIcon(cx, "share", "share", "Share", (_e, cx) => this.share(cx)),
        }),
      )
      .child(
        v_flex()
          .id("event")
          .test_id("event")
          .flex_1()
          .overflow_y_scroll()
          .child(poster(cx, e, { height: 200, rounded: false }))
          .child(
            v_flex()
              .px_4()
              .pt_4()
              .pb_8()
              .gap_5()
              .child(
                v_flex()
                  .gap_1()
                  .child(div().text_xs().font_semibold().text_color(t.primary).child(category(e.category).label.toUpperCase()))
                  .child(div().test_id("event-title").role("header").text_2xl().font_bold().line_height(1.2).text_color(t.foreground).child(e.title))
                  .child(div().text_base().text_color(t.muted_foreground).child(e.summary))
                  .when(left === 0 || left <= FEW_SEATS, (el) =>
                    el.child(
                      h_flex()
                        .pt_1()
                        .child(
                          Tag.new()
                            .test_id("availability")
                            .variant(left === 0 ? "danger" : "warning")
                            .rounded_full()
                            .child(left === 0 ? "Sold out" : `Few seats left · ${left}`),
                        ),
                    ),
                  ),
              )
              .when(mine.length > 0, (el) =>
                el.child(
                  SuccessAlert.new("going", `${mine.reduce((n, x) => n + x.quantity, 0)} tickets, ready at the door.`)
                    .test_id("going")
                    .title("You’re going")
                    .child(
                      Button.new("view-tickets")
                        .test_id("view-tickets")
                        .flex_none()
                        .self_center()
                        .px_2()
                        .py_2()
                        .text_base()
                        .font_semibold()
                        .text_color(t.primary)
                        .active((s) => s.opacity(0.5))
                        .on_click((_e, cx) => cx.navigator.push(TicketPage, { ticketId: mine[0].id }, { title: e.title }))
                        .child("Show"),
                    ),
                ),
              )
              .child(
                descriptionList(cx, "details", [
                  { label: "Date", value: longDate(dayDate(e.day)) },
                  { label: "Time", value: timeRange(e.time, e.minutes) },
                  { label: "Venue", value: `${e.venue.name}, ${e.venue.district}` },
                  { label: "Price", value: priceLine(e) },
                ]),
              )
              .child(
                h_flex()
                  .gap_3()
                  .child(
                    tintedButton(cx, {
                      id: "add-to-calendar",
                      label: saved ? "In Calendar" : "Add to Calendar",
                      icon: saved ? "calendar_check" : "calendar_plus",
                      on_click: (_e, cx) => this.toggleCalendar(cx),
                    }),
                  ),
              )
              .child(
                v_flex()
                  .gap_2()
                  .child(div().role("header").text_lg().font_semibold().text_color(t.foreground).child("About"))
                  .child(div().text_base().text_color(t.foreground).child(e.about)),
              )
              .child(
                v_flex()
                  .gap_2()
                  .child(div().role("header").text_lg().font_semibold().text_color(t.foreground).child("Location"))
                  .child(
                    v_flex()
                      .rounded_xl()
                      .bg(t.surface)
                      .overflow_hidden()
                      .child(streetPlan(cx, e.venue.id))
                      .child(
                        h_flex()
                          .gap_3()
                          .px_4()
                          .py_3()
                          .child(icon("pin", t.muted_foreground, 20))
                          .child(
                            v_flex()
                              .flex_1()
                              .min_w_0()
                              .child(div().text_base().font_medium().text_color(t.foreground).child(e.venue.name))
                              .child(div().text_sm().text_color(t.muted_foreground).child(`${e.venue.address}, ${e.venue.district}`)),
                          ),
                      ),
                  ),
              ),
          ),
      )
      .child(
        h_flex()
          .gap_4()
          .px_4()
          .pt_3()
          .pb(Math.max(win.safe_area.bottom, 12))
          .border_t_1()
          .border_color(t.border)
          .bg(t.background)
          .child(
            v_flex()
              .flex_none()
              .child(div().text_xs().text_color(t.muted_foreground).child(price === null ? "Tickets" : "From"))
              .child(div().test_id("from-price").text_lg().font_bold().text_color(t.foreground).child(price === null ? "Sold out" : formatPrice(price))),
          )
          .child(
            div()
              .flex_1()
              .child(
                primaryButton(cx, {
                  id: "get-tickets",
                  label: price === null ? "Sold Out" : price === 0 ? "Reserve a Spot" : "Get Tickets",
                  disabled: price === null,
                  on_click: (_e, cx) => cx.navigator.push(BookingPage, { event: e }, { title: "Tickets" }),
                }),
              ),
          ),
      );
  }
}

/** Every tier's price, cheapest first: `$28 – $120`, or one price. @param {Event} e */
function priceLine(e) {
  const prices = e.tiers.map((t) => t.price).sort((a, b) => a - b);
  const low = prices[0];
  const high = prices[prices.length - 1];
  return low === high ? formatPrice(low) : `${formatPrice(low)} – ${formatPrice(high)}`;
}
