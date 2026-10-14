// Selection controls, as checkout and notification preferences in the iOS
// style: switches for settings that apply at once, checkmarks for independent
// choices (Checkbox) and for one choice from a small set (Radio). The whole
// row is the control, and an unavailable option stays visible with its reason.

import { View, div, Checkbox, Radio, Switch } from "native-shell";
import { icon, page, row, section } from "../ui.js";
/** @import { Context } from "native-shell" */

/** @type {{ id: string, label: string, detail: string, disabled?: boolean }[]} */
const SPEEDS = [
  { id: "standard", label: "Standard", detail: "Free · 3–5 business days" },
  { id: "express", label: "Express", detail: "$9.90 · 1–2 business days" },
  { id: "same-day", label: "Same Day", detail: "Not available for this address", disabled: true },
];

/** @type {{ id: "receipts" | "digest" | "news", label: string, detail: string }[]} */
const EMAILS = [
  { id: "receipts", label: "Receipts", detail: "A copy of every order" },
  { id: "digest", label: "Weekly Digest", detail: "New arrivals in the categories you follow" },
  { id: "news", label: "Product News", detail: "A few times a year" },
];

export default class SelectionPage extends View {
  init() {
    this.push = { orders: true, prices: false };
    this.email = { receipts: true, digest: false, news: false };
    this.speed = "standard";
  }

  /** @param {"orders" | "prices"} key @param {boolean} on @param {Context} cx */
  setPush(key, on, cx) {
    this.push = { ...this.push, [key]: on };
    cx.notify();
  }

  render(cx) {
    return page(cx, {
      title: "Selection",
      intro: "Preferences from a shop’s checkout. Each control applies as soon as it changes.",
      children: [
        section(cx, {
          header: "Push Notifications",
          rows: [
            row(cx, {
              id: "push-orders",
              label: "Order Updates",
              detail: "Shipped, out for delivery, delivered",
              trailing: Switch.new("orders")
                .test_id("switch-orders")
                .accessibility_label("Order updates")
                .checked(this.push.orders)
                .on_change((on, cx) => this.setPush("orders", on, cx)),
            }),
            row(cx, {
              id: "push-prices",
              label: "Price Drops",
              detail: "On items you saved",
              trailing: Switch.new("prices")
                .test_id("switch-prices")
                .accessibility_label("Price drops")
                .checked(this.push.prices)
                .on_change((on, cx) => this.setPush("prices", on, cx)),
            }),
          ],
        }),
        section(cx, {
          header: "Email Me About",
          footer: "Choose any number.",
          rows: EMAILS.map((e) =>
            this.choice(cx, Checkbox.new(`email-${e.id}`), e.label, e.detail, this.email[e.id], false, (on, cx) => {
              this.email = { ...this.email, [e.id]: on };
              cx.haptic("selection");
              cx.notify();
            }).test_id(`email-${e.id}`),
          ),
        }),
        section(cx, {
          header: "Delivery Speed",
          rows: SPEEDS.map((s) =>
            this.choice(cx, Radio.new(`speed-${s.id}`), s.label, s.detail, this.speed === s.id, Boolean(s.disabled), (_on, cx) => {
              this.speed = s.id;
              cx.haptic("selection");
              cx.notify();
            }).test_id(`speed-${s.id}`),
          ),
        }),
      ],
    });
  }

  /**
   * A row that is the control: label and detail, a trailing checkmark when chosen.
   * @param {Context} cx
   * @param {import("native-shell").ToggleElement} control a headless Checkbox or Radio
   * @param {string} text @param {string} detail @param {boolean} checked @param {boolean} disabled
   * @param {(checked: boolean, cx: Context) => void} onChange
   */
  choice(cx, control, text, detail, checked, disabled, onChange) {
    const t = cx.theme();
    return control
      .accessibility_label(`${text}, ${detail}`)
      .checked(checked)
      .disabled(disabled)
      .when(!disabled, (el) => el.active((s) => s.bg(t.secondary)).on_change(onChange))
      .child(
        row(cx, {
          id: `${text}-row`,
          label: text,
          detail,
          disabled,
          trailing: div()
            .size_5()
            .flex_none()
            .child(checked ? icon("check", t.primary, 20) : div()),
        }),
      );
  }
}
