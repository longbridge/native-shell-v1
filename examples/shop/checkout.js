// Checkout: where it goes, how fast, what it costs, then Place Order.
//
// The address form labels each field above it (placeholders would read as
// values already filled in) and moves field to field with Return, finishing
// on the last one; problems are explained under the field when the order is
// placed, and the keyboard moves to the first of them. Placing the order shows its
// progress on the button and locks the page (back included) until the store
// answers; a failure says the card wasn't charged and offers to try again,
// keeping everything typed. Success replaces the form with the confirmation.

import { View, div, h_flex, v_flex, Button, Input, InputState, Radio, Spinner } from "native-shell";
import { ErrorAlert, Form, Field } from "gpui-component";
import { productById, photo } from "./catalog.js";
import { store, money, arrivalDate, deliveryPrice, DELIVERY, ACCOUNT } from "./store.js";
import { icon, navBar, photoFrame, section, text, valueRow } from "./ui.js";

const POSTAL = /^\d{5}$/;
const PHONE = /^\+?\d{7,15}$/;

/**
 * @typedef {{
 *   id: keyof import("./store.js").Address,
 *   label: string,
 *   auto_capitalize?: import("native-shell").InputOptions["auto_capitalize"],
 *   keyboard?: import("native-shell").InputOptions["keyboard"],
 *   max_length?: number,
 *   required?: string,
 *   help?: string,
 * }} FieldSpec
 */

/** @type {FieldSpec[]} */
const FIELDS = [
  { id: "name", label: "Full name", auto_capitalize: "words", required: "Enter the name to deliver to." },
  { id: "street", label: "Street address", auto_capitalize: "words", required: "Enter a street address." },
  { id: "apartment", label: "Apartment, suite", auto_capitalize: "words" },
  { id: "city", label: "City", auto_capitalize: "words", required: "Enter a city." },
  { id: "postal", label: "Postal code", keyboard: "number", max_length: 5, required: "Enter a postal code." },
  { id: "phone", label: "Phone", keyboard: "phone", help: "Only used if the courier can’t find you." },
];

export default class CheckoutPage extends View {
  init(props, cx) {
    this.selectTab = props.selectTab;
    /** "form" | "placing" | "failed" | "placed" */
    this.state = "form";
    this.failure = null;
    this.order = null;
    this.delivery = "standard";
    this.errors = {};
    this.inputs = Object.fromEntries(
      FIELDS.map((f, i) => {
        const last = i === FIELDS.length - 1;
        const state = InputState.new({
          value: f.id === "name" ? ACCOUNT.name : "",
          keyboard: f.keyboard ?? "default",
          max_length: f.max_length,
          auto_capitalize: f.auto_capitalize ?? "none",
          return_key: last ? "done" : "next",
        });
        state.on("change", (_e, cx) => {
          if (this.errors[f.id]) this.errors = { ...this.errors, [f.id]: undefined };
          cx.notify();
        });
        return [f.id, state];
      }),
    );
    FIELDS.forEach((f, i) => {
      const next = FIELDS[i + 1];
      this.inputs[f.id].on("submit", (_e, cx) => (next ? this.inputs[next.id].focus() : cx.blur()));
    });
    cx.on_back((cx) => {
      if (this.state !== "placing") cx.navigator.pop();
    });
  }

  release() {
    for (const s of Object.values(this.inputs)) s.release();
  }

  value(id) {
    return this.inputs[id].value().trim();
  }

  validate() {
    const errors = {};
    for (const f of FIELDS) if (f.required && this.value(f.id) === "") errors[f.id] = f.required;
    if (!errors.postal && !POSTAL.test(this.value("postal"))) errors.postal = "Enter a 5-digit postal code.";
    const phone = this.value("phone").replace(/[\s()-]/g, "");
    if (phone !== "" && !PHONE.test(phone)) errors.phone = "Enter a phone number with 7 to 15 digits.";
    return errors;
  }

  place(cx) {
    this.errors = this.validate();
    const first = FIELDS.find((f) => this.errors[f.id]);
    if (first) {
      cx.haptic("error");
      cx.focus(`field-${first.id}`);
      cx.notify();
      return;
    }
    cx.blur();
    this.state = "placing";
    this.failure = null;
    cx.set_can_go_back(false);
    cx.notify();
    const address = /** @type {import("./store.js").Address} */ (Object.fromEntries(FIELDS.map((f) => [f.id, this.value(f.id)])));
    cx.spawn(async (cx) => {
      try {
        this.order = await store.placeOrder(cx, { address, delivery: this.delivery });
        this.state = "placed";
        cx.haptic("success");
      } catch (e) {
        this.state = "failed";
        this.failure = e.message;
        cx.haptic("error");
      }
      cx.set_can_go_back(true);
      cx.notify();
    });
  }

  finish(tab, cx) {
    cx.navigator.pop();
    this.selectTab(tab, cx);
  }

  render(cx) {
    const t = cx.theme();
    if (this.state === "placed") return this.confirmation(cx);
    const placing = this.state === "placing";
    const win = cx.window();
    return v_flex()
      .size_full()
      .bg(t.grouped_background)
      .line_height(1.3)
      .child(navBar(cx, { title: "Checkout", back: { label: "Cart", disabled: placing, on_click: (_e, cx) => cx.navigator.pop() } }))
      .child(
        v_flex()
          .test_id("checkout-page")
          .flex_1()
          .overflow_y_scroll()
          .child(
            v_flex()
              .px_4()
              .pt_4()
              .pb(Math.max(win.keyboard_height - 96, 0) + 24)
              .gap_6()
              .child(this.items(cx))
              .child(this.address(cx, placing))
              .child(this.deliveryOptions(cx, placing))
              .child(this.payment(cx))
              .child(this.summary(cx)),
          ),
      )
      .child(this.placeBar(cx, placing));
  }

  items(cx) {
    const t = cx.theme();
    const count = store.count;
    return h_flex()
      .test_id("checkout-items")
      .gap_2()
      .accessibility_label(`${count} ${count === 1 ? "item" : "items"}`)
      .children(
        store.cart.slice(0, 4).map((l) => {
          const p = productById(l.product);
          return p ? photoFrame(cx, photo(p, l.color), 56, { radius: 10 }) : null;
        }),
      )
      .when(store.cart.length > 4, (el) =>
        el.child(
          div()
            .size("3.5rem")
            .flex()
            .items_center()
            .justify_center()
            .rounded(10)
            .bg(t.muted)
            .child(text(cx, "subheadline", `+${store.cart.length - 4}`, t.muted_foreground)),
        ),
      )
      .child(text(cx, "subheadline", `${count} ${count === 1 ? "item" : "items"}`, t.muted_foreground).flex_1().text_right());
  }

  address(cx, placing) {
    const t = cx.theme();
    return v_flex()
      .gap_2()
      .child(text(cx, "footnote", "SHIPPING ADDRESS", t.muted_foreground).role("header").px_4())
      .child(
        div()
          .p_4()
          .rounded_xl()
          .bg(t.surface)
          .child(
            // Two columns so city and postal code share a row; the rest span both.
            Form.new()
              .columns(2)
              .children(FIELDS.map((f) => this.field(cx, f, placing).col_span(f.id === "city" || f.id === "postal" ? 1 : 2))),
          ),
      );
  }

  /** @param {import("native-shell").Context} cx @param {FieldSpec} f @param {boolean} placing */
  field(cx, f, placing) {
    const t = cx.theme();
    const error = this.errors[f.id];
    return Field.new()
      .test_id(`address-${f.id}`)
      .label(f.label)
      .required(Boolean(f.required))
      .when(Boolean(f.help), (el) => el.description(f.help))
      .error(error ?? null)
      .child(
        Input.new(this.inputs[f.id])
          .id(`field-${f.id}`)
          .test_id(`field-${f.id}`)
          .disabled(placing)
          .min_h_11()
          .px_3()
          .rounded_lg()
          .border_1()
          .border_color(error ? t.destructive : t.input)
          .bg(t.background)
          .text_size(t.typography.body.size)
          .text_color(t.foreground),
      );
  }

  deliveryOptions(cx, placing) {
    const t = cx.theme();
    const amount = store.subtotal() - store.discount();
    return section(cx, {
      title: "Delivery",
      rows: DELIVERY.map((option) => {
        const selected = this.delivery === option.id;
        const cost = deliveryPrice(option, amount);
        return Radio.new(`delivery-${option.id}`)
          .test_id(`delivery-${option.id}`)
          .accessibility_label(`${option.label}, ${cost === 0 ? "free" : money(cost)}, arrives ${arrivalDate(option.days)}`)
          .checked(selected)
          .disabled(placing)
          .flex()
          .items_center()
          .gap_3()
          .px_4()
          .py_3()
          .min_h_11()
          .when(!placing, (el) =>
            el.active((s) => s.bg(t.muted)).on_change((_on, cx) => {
              this.delivery = option.id;
              cx.haptic("selection");
              cx.notify();
            }),
          )
          .child(
            v_flex()
              .flex_1()
              .min_w_0()
              .gap_0p5()
              .child(
                h_flex()
                  .gap_2()
                  .child(text(cx, "body", option.label))
                  .child(text(cx, "body", cost === 0 ? "Free" : money(cost), cost === 0 ? t.success : t.muted_foreground)),
              )
              .child(text(cx, "footnote", `Arrives ${arrivalDate(option.days)} · ${option.detail}`, t.muted_foreground)),
          )
          // iOS marks the one chosen row of a list with a checkmark.
          .child(div().w_6().flex_none().when(selected, (el) => el.child(icon("check_circle", t.primary, 22))));
      }),
    });
  }

  payment(cx) {
    const t = cx.theme();
    return section(cx, {
      title: "Payment",
      footer: "You’re charged when the order ships.",
      rows: [
        h_flex()
          .gap_3()
          .px_4()
          .py_3()
          .min_h_11()
          .child(icon("card", t.muted_foreground, 22))
          .child(
            v_flex()
              .flex_1()
              .child(text(cx, "body", "Card ending 4242"))
              .child(text(cx, "footnote", `Billing address: ${ACCOUNT.name}`, t.muted_foreground)),
          ),
      ],
    });
  }

  summary(cx) {
    const t = cx.theme();
    const totals = store.totals(this.delivery);
    return section(cx, {
      title: "Summary",
      rows: [
        valueRow(cx, "Subtotal", money(totals.subtotal)),
        totals.discount > 0 && valueRow(cx, `Discount (${store.promo?.code})`, money(-totals.discount), { color: t.success }),
        valueRow(cx, "Delivery", totals.delivery === 0 ? "Free" : money(totals.delivery), { id: "summary-delivery" }),
        valueRow(cx, "Tax", money(totals.tax)),
        valueRow(cx, "Total", money(totals.total), { id: "total", strong: true }),
      ],
    });
  }

  placeBar(cx, placing) {
    const t = cx.theme();
    const failed = this.state === "failed";
    const total = store.totals(this.delivery).total;
    return v_flex()
      .flex_none()
      .gap_3()
      .px_4()
      .pt_3()
      .pb(Math.max(cx.window().keyboard_height, cx.window().safe_area.bottom, 12))
      .border_t_1()
      .border_color(t.border)
      .bg(t.background)
      .when(failed, (el) =>
        el.child(
          ErrorAlert.new("order-failed", `${this.failure} Your card wasn’t charged; your cart and address are kept.`)
            .title("We couldn’t place your order")
            .test_id("order-failed"),
        ),
      )
      .child(
        Button.new("place-order")
          .test_id("place-order")
          .primary()
          .size("large")
          .disabled(placing)
          .accessibility_label(placing ? "Placing order" : failed ? "Try again" : `Place order, ${money(total)}`)
          .when(!placing, (el) => el.on_click((_e, cx) => this.place(cx)))
          .when(placing, (el) => el.child(Spinner.new().size_5().text_color(t.primary_foreground)))
          .child(placing ? "Placing Order…" : failed ? "Try Again" : `Place Order · ${money(total)}`),
      );
  }

  confirmation(cx) {
    const t = cx.theme();
    const o = this.order;
    const win = cx.window();
    return v_flex()
      .test_id("order-placed")
      .size_full()
      .bg(t.grouped_background)
      .line_height(1.3)
      .pt(win.safe_area.top)
      .child(
        v_flex()
          .flex_1()
          .overflow_y_scroll()
          .child(
            v_flex()
              .items_center()
              .gap_3()
              .px_8()
              .pt_16()
              .pb_8()
              .child(icon("check_circle", t.success, 64))
              .child(text(cx, "title1", "Order Placed").font_bold().role("header").text_center())
              .child(
                text(cx, "body", `Arrives ${o.arrives}. We’ll email ${ACCOUNT.email} when it ships.`, t.muted_foreground)
                  .text_center()
                  .test_id("arrives"),
              ),
          )
          .child(
            v_flex()
              .px_4()
              .child(
                section(cx, {
                  rows: [
                    valueRow(cx, "Order", o.id, { id: "order-id" }),
                    valueRow(cx, "Delivery", o.delivery),
                    valueRow(cx, "Ship to", `${o.address.street}, ${o.address.city}`),
                    valueRow(cx, "Total", money(o.totals.total), { strong: true }),
                  ],
                }),
              ),
          ),
      )
      .child(
        v_flex()
          .flex_none()
          .gap_2()
          .px_4()
          .pt_3()
          .pb(Math.max(win.safe_area.bottom, 12))
          .child(
            Button.new("continue-shopping")
              .test_id("continue-shopping")
              .primary()
              .size("large")
              .on_click((_e, cx) => this.finish("shop", cx))
              .child("Continue Shopping"),
          )
          .child(
            Button.new("view-orders")
              .test_id("view-orders")
              .ghost()
              .size("large")
              .on_click((_e, cx) => this.finish("account", cx))
              .child("View Orders"),
          ),
      );
  }
}
