// Booking: tickets, then who's coming, then a review that says exactly what
// will be charged. Each step keeps what was entered when going back; the
// payment can fail, and says so with a way to try again that charges nothing
// twice. It ends on the ticket.

import { View, div, h_flex, v_flex, shape, Button, Input, InputState, NumberInput, Radio } from "native-shell";
import { ErrorAlert } from "gpui-component";
import { category } from "./catalog.js";
import { dayDate, formatPrice, formatTime, longDate, relativeDay } from "./dates.js";
import { pay } from "./service.js";
import { addTicket, left } from "./store.js";
import TicketPage from "./ticket.js";
import { categoryIcon, icon, navBar, primaryButton, section } from "./ui.js";
import { descriptionList, field, steps } from "./kit.js";
/** @import { Context } from "native-shell" */
/** @import { Event, Tier } from "./catalog.js" */

const STEPS = ["Tickets", "Details", "Review"];
const MAX_PER_ORDER = 6;
const EMAIL = /^[^\s@]+@[^\s@]+\.[^\s@]+$/;
const PHONE = /^[+\d][\d\s()-]{6,}$/;
const RING = "M22 12a10 10 0 1 1-20 0 10 10 0 0 1 20 0Z";
const DOT = "M17 12a5 5 0 1 1-10 0 5 5 0 0 1 10 0Z";

export default class BookingPage extends View {
  /** @param {{ event: Event }} props */
  init(props, cx) {
    this.event = props.event;
    this.step = 0;
    /** "idle" | "paying" | "failed" */
    this.payment = "idle";
    this.failure = "";
    const open = this.event.tiers.find((tier) => left(this.event, tier) > 0);
    this.tierId = open?.id ?? this.event.tiers[0].id;
    this.quantity = InputState.new({ value: "2", keyboard: "number", return_key: "done" });
    this.quantity.set_step(1);
    this.quantity.set_min(1);
    this.clampQuantity();
    this.quantity.on("change", (_e, cx) => cx.notify());
    this.quantity.on("blur", (_e, cx) => {
      this.clampQuantity();
      cx.notify();
    });

    this.name = InputState.new({ placeholder: "Full name", return_key: "next", auto_capitalize: "words", content_type: "username" });
    this.email = InputState.new({
      placeholder: "name@example.org",
      keyboard: "email",
      return_key: "next",
      auto_capitalize: "none",
      content_type: "email",
    });
    this.phone = InputState.new({ placeholder: "Optional", keyboard: "phone", return_key: "done" });
    /** Errors by field, shown once the user tries to continue. */
    this.errors = {};
    /** @type {["name" | "email" | "phone", InputState][]} */
    const fields = [["name", this.name], ["email", this.email], ["phone", this.phone]];
    for (const [key, state] of fields) {
      state.on("change", (_e, cx) => {
        if (this.errors[key]) this.errors = { ...this.errors, [key]: this.validate()[key] };
        cx.notify();
      });
    }
    this.name.on("submit", () => this.email.focus());
    this.email.on("submit", () => this.phone.focus());
    this.phone.on("submit", (_e, cx) => this.next(cx));
    cx.on_back((cx) => this.back(cx));
  }

  release() {
    for (const state of [this.quantity, this.name, this.email, this.phone]) state.release();
  }

  get tier() {
    return /** @type {Tier} */ (this.event.tiers.find((t) => t.id === this.tierId));
  }

  /** Seats this order can take: the tier's left, at most six. */
  get maxQuantity() {
    return Math.max(1, Math.min(MAX_PER_ORDER, left(this.event, this.tier)));
  }

  get count() {
    const n = Number.parseInt(this.quantity.value(), 10);
    return Number.isFinite(n) ? Math.min(Math.max(n, 1), this.maxQuantity) : 1;
  }

  get total() {
    return this.tier.price * this.count;
  }

  clampQuantity() {
    this.quantity.set_max(this.maxQuantity);
    if (String(this.count) !== this.quantity.value()) this.quantity.set_value(String(this.count));
  }

  pickTier(id, cx) {
    this.tierId = id;
    this.clampQuantity();
    cx.haptic("selection");
    cx.notify();
  }

  validate() {
    const errors = {};
    if (this.name.value().trim() === "") errors.name = "Enter the name on the booking.";
    const email = this.email.value().trim();
    if (email === "") errors.email = "Enter an email address for the tickets.";
    else if (!EMAIL.test(email)) errors.email = "Enter an email address like name@example.org.";
    const phone = this.phone.value().trim();
    if (phone !== "" && !PHONE.test(phone)) errors.phone = "Enter a phone number with its area code, or leave it empty.";
    return errors;
  }

  next(cx) {
    if (this.step === 0) {
      this.clampQuantity();
    } else if (this.step === 1) {
      this.errors = this.validate();
      if (Object.keys(this.errors).length > 0) {
        cx.haptic("error");
        cx.notify();
        return;
      }
      cx.blur();
    }
    this.step += 1;
    cx.notify();
  }

  back(cx) {
    if (this.payment === "paying") return;
    if (this.step === 0) {
      cx.navigator.pop();
      return;
    }
    this.step -= 1;
    this.payment = "idle";
    cx.notify();
  }

  confirm(cx) {
    this.payment = "paying";
    // Leaving mid-payment would leave the user unsure whether they were charged.
    cx.set_can_go_back(false);
    cx.notify();
    const order = { event: this.event, tierId: this.tierId, quantity: this.count };
    cx.spawn(async (cx) => {
      try {
        const { total } = await pay(cx, order);
        const ticket = addTicket({
          eventId: this.event.id,
          tierId: this.tierId,
          quantity: this.count,
          total,
          attendee: { name: this.name.value().trim(), email: this.email.value().trim(), phone: this.phone.value().trim() },
        });
        cx.haptic("success");
        cx.set_can_go_back(true);
        cx.navigator.pop();
        cx.navigator.push(TicketPage, { ticketId: ticket.id, booked: true }, { title: this.event.title });
      } catch (err) {
        this.payment = "failed";
        this.failure = err instanceof Error ? err.message : String(err);
        cx.haptic("error");
        cx.set_can_go_back(true);
        cx.notify();
      }
    });
  }

  render(cx) {
    const t = cx.theme();
    const win = cx.window();
    return v_flex()
      .size_full()
      .bg(t.grouped_background)
      .line_height(1.375)
      .child(
        navBar(cx, {
          title: STEPS[this.step],
          back: { label: this.step === 0 ? "Event" : STEPS[this.step - 1], on_click: (_e, cx) => this.back(cx) },
        }),
      )
      .child(
        v_flex()
          .id("booking")
          .test_id("booking")
          .flex_1()
          .overflow_y_scroll()
          .child(
            v_flex()
              .px_4()
              .pt_4()
              .pb_8()
              .gap_6()
              .child(steps(cx, "steps", STEPS, this.step))
              .child(this.eventSummary(cx))
              .children(this.step === 0 ? this.ticketsStep(cx) : this.step === 1 ? this.detailsStep(cx) : this.reviewStep(cx)),
          ),
      )
      .child(
        v_flex()
          .gap_2()
          .px_4()
          .pt_3()
          .pb(Math.max(win.keyboard_height, win.safe_area.bottom, 12))
          .border_t_1()
          .border_color(t.border)
          .bg(t.background)
          .child(
            h_flex()
              .justify_between()
              .child(div().text_base().text_color(t.muted_foreground).child(`${this.count} × ${this.tier.name}`))
              .child(div().test_id("total").text_base().font_semibold().text_color(t.foreground).child(formatPrice(this.total))),
          )
          .child(this.footerButton(cx)),
      );
  }

  footerButton(cx) {
    if (this.step < 2) return primaryButton(cx, { id: "continue", label: "Continue", on_click: (_e, cx) => this.next(cx) });
    const paying = this.payment === "paying";
    const label =
      this.payment === "failed" ? "Try Again" : paying ? "Paying…" : this.total === 0 ? "Confirm Reservation" : `Pay ${formatPrice(this.total)}`;
    return primaryButton(cx, { id: "pay", label, loading: paying, on_click: (_e, cx) => this.confirm(cx) });
  }

  eventSummary(cx) {
    const t = cx.theme();
    const e = this.event;
    return h_flex()
      .gap_3()
      .child(
        div()
          .size_12()
          .flex_none()
          .rounded_lg()
          .bg(category(e.category).colors[0])
          .flex()
          .items_center()
          .justify_center()
          .child(categoryIcon(e.category, "white", 22)),
      )
      .child(
        v_flex()
          .flex_1()
          .min_w_0()
          .child(div().text_base().font_semibold().text_color(t.foreground).child(e.title))
          .child(div().text_sm().text_color(t.muted_foreground).child(`${relativeDay(e.day)} · ${formatTime(e.time)} · ${e.venue.name}`)),
      );
  }

  ticketsStep(cx) {
    const t = cx.theme();
    return [
      section(cx, {
        title: "Seat type",
        id: "tiers",
        inset: 52,
        rows: this.event.tiers.map((tier) => this.tierRow(cx, tier)),
      }),
      section(cx, {
        title: "Quantity",
        footer: `Up to ${this.maxQuantity} per order.`,
        rows: [
          h_flex()
            .gap_3()
            .px_4()
            .py_2()
            .min_h_12()
            .child(div().flex_1().text_base().text_color(t.foreground).child("Tickets"))
            .child(
              NumberInput.new(this.quantity)
                .test_id("quantity")
                .accessibility_label("Number of tickets")
                .gap_1()
                .rounded_lg()
                .bg(t.muted)
                .input(
                  Input.new(this.quantity)
                    .test_id("quantity-field")
                    .accessibility_label("Number of tickets")
                    .w_10()
                    .py_1()
                    .text_center()
                    .text_base()
                    .font_semibold()
                    .text_color(t.foreground),
                )
                .decrement_button(stepButton(cx, "quantity-less", "minus", "Fewer tickets", this.count <= 1))
                .increment_button(stepButton(cx, "quantity-more", "plus", "More tickets", this.count >= this.maxQuantity))
                .on_step((_a, cx) => {
                  cx.haptic("selection");
                  cx.notify();
                }),
            ),
        ],
      }),
    ];
  }

  /** @param {Context} cx @param {Tier} tier */
  tierRow(cx, tier) {
    const t = cx.theme();
    const remaining = left(this.event, tier);
    const disabled = remaining === 0;
    const selected = this.tierId === tier.id;
    const color = disabled ? t.border : selected ? t.primary : t.muted_foreground;
    const availability = disabled ? "Sold out" : remaining <= 10 ? `${remaining} left` : null;
    return Radio.new(`tier-${tier.id}`)
      .test_id(`tier-${tier.id}`)
      .accessibility_label(`${tier.name}, ${formatPrice(tier.price)}${availability ? `, ${availability}` : ""}`)
      .checked(selected)
      .disabled(disabled)
      .flex()
      .items_start()
      .gap_3()
      .px_4()
      .py_3()
      .when(!disabled, (el) => el.active((s) => s.bg(t.muted)).on_change((_on, cx) => this.pickTier(tier.id, cx)))
      .child(
        div()
          .relative()
          .size_6()
          .flex_none()
          .child(shape(RING, { view_box: [0, 0, 24, 24] }).stroke(color, 2).absolute().top_0().left_0().size_6())
          .when(selected, (el) => el.child(shape(DOT, { view_box: [0, 0, 24, 24] }).fill(color).absolute().top_0().left_0().size_6())),
      )
      .child(
        v_flex()
          .flex_1()
          .min_w_0()
          .gap_0p5()
          .child(div().text_base().text_color(disabled ? t.muted_foreground : t.foreground).child(tier.name))
          .child(div().text_sm().text_color(t.muted_foreground).child(tier.detail))
          .when(availability !== null, (el) =>
            el.child(div().text_sm().font_medium().text_color(disabled ? t.muted_foreground : t.warning).child(availability)),
          ),
      )
      .child(div().flex_none().text_base().font_medium().text_color(disabled ? t.muted_foreground : t.foreground).child(formatPrice(tier.price)));
  }

  detailsStep(cx) {
    return [
      v_flex()
        .test_id("attendee-form")
        .gap_4()
        .child(field(cx, { id: "name", label: "Name", state: this.name, error: this.errors.name }))
        .child(field(cx, { id: "email", label: "Email", state: this.email, error: this.errors.email, help: "Tickets and changes go here." }))
        .child(field(cx, { id: "phone", label: "Phone", state: this.phone, error: this.errors.phone, help: "Only used if the event changes at short notice." })),
    ];
  }

  reviewStep(cx) {
    const e = this.event;
    return [
      this.payment === "failed"
        ? ErrorAlert.new("payment-failed", `${this.failure} You haven’t been charged. Try again, or come back later.`)
            .test_id("payment-failed")
            .title("Payment didn’t go through")
        : null,
      descriptionList(cx, "review", [
        { label: "Event", value: e.title },
        { label: "Date", value: `${longDate(dayDate(e.day))}, ${formatTime(e.time)}` },
        { label: "Venue", value: e.venue.name },
        { label: "Tickets", value: `${this.count} × ${this.tier.name}` },
        { label: "Name", value: this.name.value().trim() },
        { label: "Email", value: this.email.value().trim() },
      ]),
      this.total === 0
        ? null
        : section(cx, {
            title: "Payment",
            footer: "You’re charged once the box office confirms your seats.",
            rows: [
              h_flex()
                .gap_3()
                .px_4()
                .py_3()
                .min_h_12()
                .child(icon("card", cx.theme().muted_foreground, 20))
                .child(div().flex_1().text_base().text_color(cx.theme().foreground).child("Visa ending in 4242")),
            ],
          }),
    ].filter(Boolean);
  }
}

function stepButton(cx, id, name, label, disabled) {
  const t = cx.theme();
  return Button.new(id)
    .test_id(id)
    .accessibility_label(label)
    .disabled(disabled)
    .size_11()
    .flex()
    .items_center()
    .justify_center()
    .when(!disabled, (el) => el.active((s) => s.opacity(0.5)))
    .child(icon(name, disabled ? t.border : t.primary, 20));
}
