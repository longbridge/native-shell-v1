// One past order: its status, what was in it, where it went and what it cost.

import { View, h_flex, v_flex } from "native-shell";
import { Tag } from "gpui-component";
import { productById, photo } from "./catalog.js";
import { store, money, shortDate } from "./store.js";
import { STATUS_VARIANT } from "./account.js";
import { navBar, photoFrame, section, text, valueRow } from "./ui.js";

export default class OrderPage extends View {
  init(props) {
    this.order = store.orders.find((o) => o.id === props.orderId);
  }

  render(cx) {
    const t = cx.theme();
    const o = this.order;
    const a = o.address;
    return v_flex()
      .size_full()
      .bg(t.grouped_background)
      .line_height(1.3)
      .child(navBar(cx, { title: `Order ${o.id}`, back: { label: "Account", on_click: (_e, cx) => cx.navigator.pop() } }))
      .child(
        v_flex()
          .test_id("order")
          .flex_1()
          .overflow_y_scroll()
          .child(
            v_flex()
              .px_4()
              .pt_4()
              .pb(cx.window().safe_area.bottom + 24)
              .gap_6()
              .child(
                v_flex()
                  .gap_2()
                  .px_4()
                  .child(h_flex().child(Tag.new().variant(STATUS_VARIANT[o.status] ?? "secondary").child(o.status)))
                  .child(text(cx, "title2", o.status === "Delivered" ? `Delivered ${o.arrives}` : `Arrives ${o.arrives}`).font_bold())
                  .child(text(cx, "subheadline", `Placed ${shortDate(o.placed)} · ${o.delivery} delivery`, t.muted_foreground)),
              )
              .child(
                section(cx, {
                  title: "Items",
                  inset: 88,
                  rows: o.lines.map((l) => {
                    const p = productById(l.product);
                    if (!p) return null;
                    const color = p.colors.find((c) => c.id === l.color)?.label ?? "";
                    const size = p.sizes?.find((s) => s.id === l.size)?.label;
                    return h_flex()
                      .gap_3()
                      .px_4()
                      .py_3()
                      .child(photoFrame(cx, photo(p, l.color), 56, { radius: 10 }))
                      .child(
                        v_flex()
                          .flex_1()
                          .min_w_0()
                          .child(text(cx, "body", p.name))
                          .child(text(cx, "footnote", `${size ? `${color} · ${size}` : color} · Qty ${l.quantity}`, t.muted_foreground)),
                      )
                      .child(text(cx, "body", money(p.price * l.quantity)));
                  }),
                }),
              )
              .child(
                section(cx, {
                  title: "Shipped to",
                  rows: [
                    v_flex()
                      .px_4()
                      .py_3()
                      .gap_0p5()
                      .child(text(cx, "body", a.name))
                      .child(text(cx, "body", a.apartment ? `${a.street}, ${a.apartment}` : a.street, t.muted_foreground))
                      .child(text(cx, "body", `${a.city} ${a.postal}`, t.muted_foreground)),
                  ],
                }),
              )
              .child(
                section(cx, {
                  title: "Payment",
                  rows: [
                    valueRow(cx, "Subtotal", money(o.totals.subtotal)),
                    o.totals.discount > 0 && valueRow(cx, "Discount", money(-o.totals.discount), { color: t.success }),
                    valueRow(cx, "Delivery", o.totals.delivery === 0 ? "Free" : money(o.totals.delivery)),
                    valueRow(cx, "Tax", money(o.totals.tax)),
                    valueRow(cx, "Total", money(o.totals.total), { strong: true }),
                  ],
                  footer: "Charged to the card ending 4242.",
                }),
              ),
          ),
      );
  }
}
