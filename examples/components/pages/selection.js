// Selection controls, as checkout and notification preferences: switches for
// settings that apply at once, checkboxes for independent choices, radios for
// one choice from a small visible set. A whole row is the control, so the
// target is the row, and an unavailable option stays visible with its reason.

import { View, div, v_flex, shape, Checkbox, Radio, Switch } from "native-shell";
import { card, heading, icon, page, row } from "../ui.js";

const RING = "M22 12a10 10 0 1 1-20 0 10 10 0 0 1 20 0Z";
const DOT = "M17 12a5 5 0 1 1-10 0 5 5 0 0 1 10 0Z";

const SPEEDS = [
  { id: "standard", label: "Standard", detail: "Free · 3–5 business days" },
  { id: "express", label: "Express", detail: "$9.90 · 1–2 business days" },
  { id: "same-day", label: "Same day", detail: "Not available for this address", disabled: true },
];

const EMAILS = [
  { id: "receipts", label: "Receipts", detail: "A copy of every order" },
  { id: "digest", label: "Weekly digest", detail: "New arrivals in the categories you follow" },
  { id: "news", label: "Product news", detail: "A few times a year" },
];

export default class SelectionPage extends View {
  init() {
    this.push = { orders: true, prices: false };
    this.email = { receipts: true, digest: false, news: false };
    this.speed = "standard";
  }

  render(cx) {
    const t = cx.theme();
    const divided = (rows) => rows.flatMap((r, i) => (i === 0 ? [r] : [div().h(1).ml(52).bg(t.border), r]));
    return page(cx, {
      title: "Selection controls",
      intro: "Preferences from a shop’s checkout. Each control applies as soon as it changes.",
      children: [
        v_flex()
          .gap_3()
          .child(heading(cx, "Push notifications"))
          .child(
            card(cx)
              .child(
                row(cx, {
                  id: "push-orders",
                  label: "Order updates",
                  detail: "Shipped, out for delivery, delivered",
                  trailing: Switch.new("orders")
                    .test_id("switch-orders")
                    .accessibility_label("Order updates")
                    .checked(this.push.orders)
                    .on_change((on, cx) => this.setPush("orders", on, cx)),
                }),
              )
              .child(div().h(1).ml_4().bg(t.border))
              .child(
                row(cx, {
                  id: "push-prices",
                  label: "Price drops",
                  detail: "On items you saved",
                  trailing: Switch.new("prices")
                    .test_id("switch-prices")
                    .accessibility_label("Price drops")
                    .checked(this.push.prices)
                    .on_change((on, cx) => this.setPush("prices", on, cx)),
                }),
              ),
          ),
        v_flex()
          .gap_3()
          .child(heading(cx, "Email me about"))
          .child(card(cx).children(divided(EMAILS.map((e) => this.checkRow(cx, e))))),
        v_flex()
          .gap_3()
          .child(heading(cx, "Delivery speed"))
          .child(
            card(cx)
              .role("list")
              .children(divided(SPEEDS.map((s) => this.radioRow(cx, s)))),
          ),
      ],
    });
  }

  setPush(key, on, cx) {
    this.push = { ...this.push, [key]: on };
    cx.notify();
  }

  checkRow(cx, { id, label, detail }) {
    const t = cx.theme();
    const checked = this.email[id];
    return Checkbox.new(`email-${id}`)
      .test_id(`email-${id}`)
      .accessibility_label(label)
      .checked(checked)
      .flex()
      .items_start()
      .gap_3()
      .px_4()
      .py_3()
      .active((s) => s.bg(t.muted))
      .on_change((on, cx) => {
        this.email = { ...this.email, [id]: on };
        cx.notify();
      })
      .child(
        div()
          .size_6()
          .flex_none()
          .flex()
          .items_center()
          .justify_center()
          .rounded_md()
          .border_2()
          .border_color(checked ? t.primary : t.muted_foreground)
          .when(checked, (el) => el.bg(t.primary).child(icon("check", t.primary_foreground, 16))),
      )
      .child(text(cx, label, detail, false));
  }

  radioRow(cx, { id, label, detail, disabled }) {
    const t = cx.theme();
    const selected = this.speed === id;
    const color = disabled ? t.border : selected ? t.primary : t.muted_foreground;
    return Radio.new(`speed-${id}`)
      .test_id(`speed-${id}`)
      .accessibility_label(`${label}, ${detail}`)
      .checked(selected)
      .disabled(!!disabled)
      .flex()
      .items_start()
      .gap_3()
      .px_4()
      .py_3()
      .when(!disabled, (el) =>
        el
          .active((s) => s.bg(t.muted))
          .on_change((_on, cx) => {
            this.speed = id;
            cx.notify();
          }),
      )
      .child(
        div()
          .relative()
          .size_6()
          .flex_none()
          .child(
            shape(RING, { view_box: [0, 0, 24, 24] })
              .stroke(color, 2)
              .absolute()
              .top_0()
              .left_0()
              .size_6(),
          )
          .when(selected, (el) =>
            el.child(
              shape(DOT, { view_box: [0, 0, 24, 24] })
                .fill(color)
                .absolute()
                .top_0()
                .left_0()
                .size_6(),
            ),
          ),
      )
      .child(text(cx, label, detail, disabled));
  }
}

function text(cx, label, detail, disabled) {
  const t = cx.theme();
  return v_flex()
    .flex_1()
    .min_w_0()
    .gap_0p5()
    .child(
      div()
        .text_base()
        .text_color(disabled ? t.muted_foreground : t.foreground)
        .child(label),
    )
    .child(div().text_sm().text_color(t.muted_foreground).child(detail));
}
