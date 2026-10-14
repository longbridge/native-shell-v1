// Forms, as a flower delivery checkout in three steps. A stepper shows where
// the buyer is and takes them back; the recipient step is a stacked form with
// framed fields, a time picker and a ribbon color; payment is an inset grouped
// list with a promo code that can be refused; the last step confirms the order
// with a copyable number and asks for a rating. Problems are explained under
// the field they are about when the buyer continues.

import { View, div, h_flex, v_flex, InputState, TextareaState, Switch, Spinner } from "native-shell";
import {
  VForm,
  HForm,
  Field,
  InputGroup,
  InputGroupInput,
  InputGroupTextarea,
  InputGroupAddon,
  InputGroupButton,
  InputGroupText,
  Rating,
  Stepper,
  StepperItem,
  TimeField,
  TimeFieldState,
  Clipboard,
  ColorPicker,
  ColorPickerState,
} from "gpui-component";
import { filledButton, label, page, row, section } from "../ui.js";
/** @import { Context } from "native-shell" */

const STEPS = ["Recipient", "Payment", "Done"];
const BOUQUET = { name: "Spring tulips, 15 stems", price: 45 };
const DELIVERY = 9.99;
const PROMOS = { SPRING10: 0.1 };
const MESSAGE_MAX = 150;
const OPENING = ["09:00", "20:00"];
/** Satin ribbon stock; the colors are the product, not the interface. */
const RIBBONS = ["#C2185B", "#E57373", "#F6BF26", "#7CB342", "#5C6BC0", "#8D6E63", "#F5F5F5", "#212121"];
const ORDER = "FL-4521";

const money = (n) => `$${n.toFixed(2)}`;

/** A card number passes the Luhn check. */
function luhn(digits) {
  let sum = 0;
  for (let i = 0; i < digits.length; i++) {
    let d = Number(digits[digits.length - 1 - i]);
    if (i % 2 === 1) d = d * 2 > 9 ? d * 2 - 9 : d * 2;
    sum += d;
  }
  return sum % 10 === 0;
}

function expired(mmyy) {
  const [m, y] = mmyy.split("/").map(Number);
  const now = new Date();
  return 2000 + y < now.getFullYear() || (2000 + y === now.getFullYear() && m < now.getMonth() + 1);
}

export default class FormsPage extends View {
  init(_props, cx) {
    /** @template {InputState} S @param {string} key @param {S} state @returns {S} */
    const field = (key, state) => {
      state.on("change", (_e, cx) => this.clear(key, cx));
      return state;
    };
    this.name = field("name", InputState.new({ placeholder: "Jordan Lee", return_key: "next" }));
    this.phone = field("phone", InputState.new({ placeholder: "555 010 0199", return_key: "done", max_length: 14 }));
    this.message = field("message", TextareaState.new({ placeholder: "Happy birthday! Love, Sam", rows: 3, auto_capitalize: "sentences" }));
    this.card = field("card", InputState.new({ placeholder: "4242 4242 4242 4242", return_key: "next", max_length: 23 }));
    this.expiry = field("expiry", InputState.new({ placeholder: "MM/YY", return_key: "next", max_length: 5 }));
    this.cvc = field("cvc", InputState.new({ placeholder: "123", return_key: "done", max_length: 4 }));
    this.promo = InputState.new({ placeholder: "Gift or promo code", auto_capitalize: "characters", return_key: "done" });
    this.name.on("submit", () => this.phone.focus());
    this.card.on("submit", () => this.expiry.focus());
    this.expiry.on("submit", () => this.cvc.focus());
    this.promo.on("submit", (_e, cx) => this.applyPromo(cx));
    this.time = TimeFieldState.new({ value: "14:00" });
    this.time.on("change", (_t, cx) => this.clear("time", cx));
    this.ribbon = ColorPickerState.new({ value: RIBBONS[0] });
    this.ribbon.on("change", (_hex, cx) => cx.notify());

    this.step = 0;
    this.errors = {};
    this.saveCard = true;
    this.code = "";
    this.discount = 0;
    this.promoError = null;
    this.checking = false;
    this.placing = false;
    this.rating = 0;
  }

  release() {
    for (const s of [this.name, this.phone, this.message, this.card, this.expiry, this.cvc, this.promo, this.time, this.ribbon]) s.release();
  }

  clear(key, cx) {
    if (key in this.errors) delete this.errors[key];
    cx.notify();
  }

  get total() {
    return BOUQUET.price * (1 - this.discount) + DELIVERY;
  }

  validateRecipient() {
    const errors = {};
    if (this.name.value().trim() === "") errors.name = "Enter who the flowers are for.";
    if (this.phone.value().replace(/\D/g, "").length !== 10) errors.phone = "Enter a 10-digit phone number.";
    const t = this.time.value();
    if (t < OPENING[0] || t > OPENING[1]) errors.time = "Choose a time between 9:00 and 20:00.";
    if (this.message.value().length > MESSAGE_MAX) errors.message = `Shorten the message to ${MESSAGE_MAX} characters.`;
    return errors;
  }

  validatePayment() {
    const errors = {};
    const digits = this.card.value().replace(/\s/g, "");
    if (!/^\d{13,19}$/.test(digits) || !luhn(digits)) errors.card = "Check the card number.";
    const expiry = this.expiry.value().trim();
    if (!/^(0[1-9]|1[0-2])\/\d{2}$/.test(expiry)) errors.expiry = "Enter the expiry as MM/YY.";
    else if (expired(expiry)) errors.expiry = "This card has expired.";
    if (!/^\d{3,4}$/.test(this.cvc.value())) errors.cvc = "Enter the 3 or 4 digits on the back.";
    return errors;
  }

  next(cx) {
    this.errors = this.step === 0 ? this.validateRecipient() : this.validatePayment();
    if (Object.keys(this.errors).length > 0) {
      cx.haptic("error");
      cx.notify();
      return;
    }
    cx.blur();
    if (this.step === 0) {
      this.step = 1;
      cx.notify();
      return;
    }
    this.placing = true;
    cx.notify();
    cx.spawn(async (cx) => {
      await cx.sleep(1200);
      this.placing = false;
      this.step = 2;
      cx.haptic("success");
      cx.notify();
    });
  }

  applyPromo(cx) {
    const code = this.promo.value().trim().toUpperCase();
    if (code === "" || this.checking) return;
    this.checking = true;
    cx.notify();
    cx.spawn(async (cx) => {
      await cx.sleep(600);
      this.checking = false;
      if (code in PROMOS) {
        this.code = code;
        this.discount = PROMOS[code];
        this.promoError = null;
        this.promo.set_value("");
        cx.haptic("success");
      } else {
        this.promoError = `“${code}” isn’t a valid code, or it has expired.`;
        cx.haptic("error");
      }
      cx.notify();
    });
  }

  stepper() {
    return Stepper.new("checkout")
      .test_id("stepper")
      .selected_index(this.step)
      .text_center(true)
      .on_change((i, cx) => {
        this.step = i;
        this.errors = {};
        cx.notify();
      })
      // Earlier steps can be revisited until the order is placed; later ones are reached by continuing.
      .children(STEPS.map((title, i) => StepperItem.new().disabled(i > this.step || this.step === 2 || this.placing).child(title)));
  }

  recipient() {
    const length = this.message.value().length;
    return VForm.new()
      .test_id("recipient")
      .child(
        Field.new()
          .test_id("name")
          .label("Recipient")
          .required(true)
          .error(this.errors.name)
          .child(InputGroup.new("name").invalid(!!this.errors.name).input(InputGroupInput.new(this.name).test_id("name-input").content_type("name"))),
      )
      .child(
        Field.new()
          .test_id("phone")
          .label("Recipient’s phone")
          .required(true)
          .description("The courier calls if no one answers the door.")
          .error(this.errors.phone)
          .child(
            InputGroup.new("phone")
              .invalid(!!this.errors.phone)
              .addon(InputGroupAddon.new("country").child(InputGroupText.new().child("+1")))
              .input(InputGroupInput.new(this.phone).test_id("phone-input").content_type("telephone_number")),
          ),
      )
      .child(
        Field.new()
          .test_id("time")
          .label("Delivery time")
          .description("Today, between 9:00 and 20:00.")
          .error(this.errors.time)
          .align("start")
          .child(TimeField.new(this.time).test_id("time-picker").invalid(!!this.errors.time)),
      )
      .child(
        Field.new()
          .test_id("message")
          .label("Card message")
          .error(this.errors.message)
          .child(
            InputGroup.new("message")
              .invalid(length > MESSAGE_MAX)
              .input(InputGroupTextarea.new(this.message).test_id("message-input").rows(3))
              .addon(
                InputGroupAddon.new("count")
                  .align("block-end")
                  .child(InputGroupText.new().test_id("message-count").text_sm().child(`${length}/${MESSAGE_MAX}`)),
              ),
          ),
      )
      .child(
        Field.new()
          .test_id("ribbon")
          .label("Ribbon")
          .description("Satin, tied around the stems.")
          .align("start")
          .child(ColorPicker.new(this.ribbon).test_id("ribbon-picker").accessibility_label("Ribbon color").featured_colors(RIBBONS)),
      );
  }

  /** @param {Context} cx */
  payment(cx) {
    const t = cx.theme();
    return v_flex()
      .gap_8()
      .child(
        v_flex()
          .child(label(cx, "footnote", t.muted_foreground, "CARD").role("header").px_4().pb_1p5())
          .child(
            HForm.new()
              .test_id("payment")
              .child(
                Field.new()
                  .test_id("card")
                  .label("Number")
                  .error(this.errors.card)
                  .child(InputGroup.new("card").input(InputGroupInput.new(this.card).test_id("card-input").content_type("credit_card_number"))),
              )
              .child(
                Field.new()
                  .test_id("expiry")
                  .label("Expires")
                  .error(this.errors.expiry)
                  .child(InputGroup.new("expiry").input(InputGroupInput.new(this.expiry).test_id("expiry-input").content_type("credit_card_expiration"))),
              )
              .child(
                Field.new()
                  .test_id("cvc")
                  .label("CVC")
                  .error(this.errors.cvc)
                  .child(InputGroup.new("cvc").input(InputGroupInput.new(this.cvc).test_id("cvc-input").content_type("credit_card_security_code").masked(true))),
              )
              .child(
                Field.new()
                  .label("Save Card")
                  .child(
                    Switch.new("save-card")
                      .test_id("save-card")
                      .checked(this.saveCard)
                      .on_change((on, cx) => {
                        this.saveCard = on;
                        cx.notify();
                      }),
                  ),
              ),
          ),
      )
      .child(
        VForm.new().child(
          Field.new()
            .test_id("promo")
            .label("Promo code")
            .error(this.promoError)
            .description(this.code ? `${this.code} applied: ${money(BOUQUET.price * this.discount)} off the flowers.` : "Try SPRING10.")
            .child(
              InputGroup.new("promo")
                .invalid(this.promoError !== null)
                .input(
                  InputGroupInput.new(this.promo)
                    .test_id("promo-input")
                    .on_change((_v, cx) => {
                      this.promoError = null;
                      cx.notify();
                    }),
                )
                .addon(
                  InputGroupAddon.new("apply")
                    .align("inline-end")
                    .child(
                      InputGroupButton.new("apply")
                        .test_id("apply")
                        .label("Apply")
                        .loading(this.checking)
                        .disabled(this.promo.value().trim() === "")
                        .on_click((_e, cx) => this.applyPromo(cx)),
                    ),
                ),
            ),
        ),
      )
      .child(
        section(cx, {
          header: "Order",
          rows: [
            row(cx, { id: "line-flowers", label: BOUQUET.name, value: money(BOUQUET.price * (1 - this.discount)) }),
            row(cx, { id: "line-delivery", label: "Delivery", value: money(DELIVERY) }),
            row(cx, {
              id: "line-total",
              label: "Total",
              trailing: label(cx, "headline", t.foreground, money(this.total)).test_id("total"),
            }),
          ],
        }),
      );
  }

  /** @param {Context} cx */
  done(cx) {
    const t = cx.theme();
    return v_flex()
      .gap_8()
      .child(
        v_flex()
          .test_id("confirmation")
          .p_4()
          .gap_2()
          .rounded(t.radius.md)
          .bg(t.surface)
          .child(label(cx, "title3", t.foreground, "Order placed").font_semibold().role("header"))
          .child(
            label(
              cx,
              "body",
              t.muted_foreground,
              `${BOUQUET.name} for ${this.name.value().trim()}, today at ${this.time.value()}. We’ll text you when they’re on the way.`,
            ),
          )
          .child(
            h_flex()
              .gap_2()
              .child(label(cx, "body", t.foreground, `Order ${ORDER}`))
              .child(Clipboard.new("order").test_id("copy-order").value(ORDER).accessibility_label("Copy order number")),
          ),
      )
      .child(
        v_flex()
          .gap_2()
          .px_4()
          .child(label(cx, "headline", t.foreground, "How was checkout?").role("header"))
          .child(label(cx, "subheadline", t.muted_foreground, "Your rating helps us keep it quick."))
          .child(
            Rating.new("checkout-rating")
              .test_id("rating")
              .value(this.rating)
              .on_change((v, cx) => {
                this.rating = v;
                cx.notify();
              }),
          )
          .when(this.rating > 0, (el) =>
            el.child(
              label(cx, "subheadline", t.muted_foreground, this.rating >= 4 ? "Thank you!" : "Thanks. We’ll look at what slowed you down.")
                .test_id("thanks")
                .role("alert"),
            ),
          ),
      )
      .child(
        h_flex()
          .gap_2()
          .px_4()
          .child(Rating.new("florist").test_id("florist-rating").size("small").value(4.5).disabled(true))
          .child(label(cx, "footnote", t.muted_foreground, "Bloom & Stem · 4.5 from 1,284 reviews")),
      );
  }

  /** @param {Context} cx */
  footer(cx) {
    if (this.step === 2) return filledButton(cx, "finish", "Done", { on_click: (_e, cx) => cx.navigator.pop() });
    const t = cx.theme();
    return filledButton(cx, "continue", this.step === 0 ? "Continue" : `Place Order · ${money(this.total)}`, {
      disabled: this.placing,
      leading: this.placing ? Spinner.new().size_5().text_color(t.primary_foreground) : undefined,
      on_click: (_e, cx) => this.next(cx),
    });
  }

  /** @param {Context} cx */
  render(cx) {
    const body = this.step === 0 ? this.recipient() : this.step === 1 ? this.payment(cx) : this.done(cx);
    return page(cx, {
      title: "Forms",
      intro: "Sending flowers. Problems are explained under the field when you continue.",
      children: [this.stepper(), body],
      footer: this.footer(cx),
    });
  }
}
