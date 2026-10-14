// Text fields, as a shipping address form: visible labels, the right keyboard
// for each field, Return moving to the next field, validation shown under the
// field it is about when the user commits, and a Save that stays disabled
// until the form can succeed.

import { View, h_flex, InputState, Textarea, TextareaState } from "native-shell";
import { button, field, page } from "../ui.js";

const POSTAL = /^\d{5}$/;

export default class FieldsPage extends View {
  init(_props, cx) {
    const make = (options) => {
      const s = InputState.new(options);
      s.on("change", (_e, cx) => {
        this.errors = {};
        cx.notify();
      });
      return s;
    };
    this.name = make({ placeholder: "Ada Lovelace", return_key: "next", auto_capitalize: "words" });
    this.street = make({ placeholder: "12 Harbour Street", return_key: "next", auto_capitalize: "words" });
    this.city = make({ placeholder: "Springfield", return_key: "next", auto_capitalize: "words" });
    this.postal = make({ placeholder: "12345", keyboard: "number", max_length: 5, return_key: "next" });
    this.phone = make({ placeholder: "+1 555 0100", keyboard: "phone", return_key: "next" });
    this.notes = TextareaState.new({ placeholder: "Gate code, a safe place to leave it…", rows: 3, auto_capitalize: "sentences" });
    this.notes.on("change", (_e, cx) => cx.notify());
    const order = [this.name, this.street, this.city, this.postal, this.phone];
    order.forEach((s, i) => s.on("submit", () => (order[i + 1] ?? this.notes).focus()));
    this.errors = {};
    this.saved = false;
  }

  release() {
    for (const s of [this.name, this.street, this.city, this.postal, this.phone, this.notes]) s.release();
  }

  get complete() {
    return [this.name, this.street, this.city, this.postal].every((s) => s.value().trim() !== "");
  }

  save(cx) {
    const errors = {};
    if (!POSTAL.test(this.postal.value().trim())) errors.postal = "Enter a 5-digit postal code.";
    const phone = this.phone.value().replace(/[\s()-]/g, "");
    if (phone !== "" && !/^\+?\d{7,15}$/.test(phone)) errors.phone = "Enter a phone number with 7 to 15 digits.";
    this.errors = errors;
    if (Object.keys(errors).length > 0) {
      cx.haptic("error");
      cx.notify();
      return;
    }
    cx.blur();
    cx.haptic("success");
    cx.push_toast({ title: "Address saved", level: "success", timeout: 2000 });
    cx.navigator.pop();
  }

  render(cx) {
    return page(cx, {
      title: "Text fields",
      intro: "A shipping address. Return moves to the next field; problems are explained under the field when you save.",
      children: [
        field(cx, { id: "name", label: "Full name", state: this.name }),
        field(cx, { id: "street", label: "Street address", state: this.street }),
        h_flex()
          .items_start()
          .gap_3()
          .child(field(cx, { id: "city", label: "City", state: this.city }).flex_1())
          .child(field(cx, { id: "postal", label: "Postal code", state: this.postal, error: this.errors.postal }).flex_none().w_32()),
        field(cx, {
          id: "phone",
          label: "Phone (optional)",
          state: this.phone,
          help: "Only used if the courier can’t find you.",
          error: this.errors.phone,
        }),
        field(cx, { id: "notes", label: "Delivery notes", input: Textarea.new(this.notes), multiline: true }),
      ],
      footer: button(cx, "save-address", "Save address", {
        variant: "primary",
        full: true,
        disabled: !this.complete,
        on_click: (_e, cx) => this.save(cx),
      }),
    });
  }
}
