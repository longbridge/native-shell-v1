// Text fields, as a shipping address form in the iOS style: each field a row
// with its label in a fixed column, the right keyboard for each, Return moving
// to the next field, problems explained under the section when the user
// saves, and a Save that stays disabled until the form can succeed.

import { View, h_flex, InputState, Textarea, TextareaState, Input } from "native-shell";
import { filledButton, label, page, section, textStyle } from "../ui.js";
/** @import { Context } from "native-shell" */

const POSTAL = /^\d{5}$/;

export default class FieldsPage extends View {
  init() {
    /** @param {object} options */
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
    /** @type {{ postal?: string, phone?: string }} */
    this.errors = {};
  }

  release() {
    for (const s of [this.name, this.street, this.city, this.postal, this.phone, this.notes]) s.release();
  }

  get complete() {
    return [this.name, this.street, this.city, this.postal].every((s) => s.value().trim() !== "");
  }

  /** @param {Context} cx */
  save(cx) {
    /** @type {{ postal?: string, phone?: string }} */
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

  /**
   * A form row: the label in a fixed column, the field filling the rest.
   * @param {Context} cx @param {string} id @param {string} text @param {InputState} state @param {boolean} invalid
   */
  field(cx, id, text, state, invalid) {
    const t = cx.theme();
    return h_flex()
      .gap_3()
      .px_4()
      .min_h_11()
      .child(
        label(cx, "body", invalid ? t.destructive : t.foreground, text)
          .w_24()
          .flex_none(),
      )
      .child(textStyle(Input.new(state), cx, "body").test_id(id).accessibility_label(text).flex_1().min_h_11().text_color(t.foreground));
  }

  render(cx) {
    const t = cx.theme();
    const e = this.errors;
    return page(cx, {
      title: "Text Fields",
      intro: "A shipping address. Return moves to the next field; problems are explained under the section when you save.",
      children: [
        section(cx, {
          header: "Address",
          footer: e.postal,
          footerColor: t.destructive,
          footerId: "postal-error",
          rows: [
            this.field(cx, "name", "Name", this.name, false),
            this.field(cx, "street", "Street", this.street, false),
            this.field(cx, "city", "City", this.city, false),
            this.field(cx, "postal", "Postal Code", this.postal, Boolean(e.postal)),
          ],
        }),
        section(cx, {
          header: "Contact",
          footer: e.phone ?? "Optional. Only used if the courier can’t find you.",
          footerColor: e.phone ? t.destructive : undefined,
          footerId: e.phone ? "phone-error" : undefined,
          rows: [this.field(cx, "phone", "Phone", this.phone, Boolean(e.phone))],
        }),
        section(cx, {
          header: "Delivery Notes",
          rows: [
            textStyle(Textarea.new(this.notes), cx, "body")
              .test_id("notes")
              .accessibility_label("Delivery notes")
              .min_h_24()
              .px_4()
              .py_2p5()
              .text_color(t.foreground),
          ],
        }),
      ],
      footer: filledButton(cx, "save-address", "Save Address", { disabled: !this.complete, on_click: (_e, cx) => this.save(cx) }),
    });
  }
}
