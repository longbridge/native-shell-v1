// The Account tab: who is signed in, their orders, and the demo's switch.
//
// Orders are listed newest first with their status as a tag, and open their
// details. The demo section makes the simulated network fail, so the shop's
// error and retry states can be seen without turning the network off.

import { View, div, h_flex, v_flex, Button, Switch } from "native-shell";
import { Tag, Empty, EmptyHeader, EmptyMedia, EmptyTitle, EmptyDescription } from "gpui-component";
import { productById, photo } from "./catalog.js";
import OrderPage from "./order.js";
import { store, money, shortDate, ACCOUNT } from "./store.js";
import { icon, largeTitle, photoFrame, section, statusBar, text } from "./ui.js";

export const STATUS_VARIANT = { Processing: "info", Shipped: "warning", Delivered: "success" };

export default class AccountTab extends View {
  render(cx) {
    const t = cx.theme();
    return v_flex()
      .size_full()
      .bg(t.grouped_background)
      .child(statusBar(cx).bg(t.grouped_background))
      .child(
        v_flex()
          .test_id("account")
          .flex_1()
          .overflow_y_scroll()
          .child(largeTitle(cx, "Account"))
          .child(
            v_flex()
              .px_4()
              .pt_2()
              .pb_8()
              .gap_6()
              .child(this.profile(cx))
              .child(this.orders(cx))
              .child(this.demo(cx)),
          ),
      );
  }

  profile(cx) {
    const t = cx.theme();
    const initials = ACCOUNT.name
      .split(/\s+/)
      .map((w) => w[0])
      .join("");
    return h_flex()
      .gap_3()
      .p_4()
      .rounded_xl()
      .bg(t.surface)
      .child(
        div()
          .size("3.5rem")
          .flex_none()
          .rounded_full()
          .bg(t.primary)
          .flex()
          .items_center()
          .justify_center()
          .accessibility_hidden()
          .child(text(cx, "title3", initials, t.primary_foreground).font_semibold()),
      )
      .child(
        v_flex()
          .flex_1()
          .min_w_0()
          .gap_0p5()
          .child(text(cx, "headline", ACCOUNT.name))
          .child(text(cx, "subheadline", ACCOUNT.email, t.muted_foreground).truncate()),
      );
  }

  orders(cx) {
    const t = cx.theme();
    if (store.orders.length === 0) {
      return section(cx, {
        title: "Orders",
        rows: [
          Empty.new()
            .test_id("orders-empty")
            .py_6()
            .header(
              EmptyHeader.new()
                .media(EmptyMedia.new().child(icon("package", t.muted_foreground, 32)))
                .title(EmptyTitle.new().text_size(t.typography.headline.size).child("No orders yet"))
                .description(EmptyDescription.new().child("Orders you place appear here, with their delivery status.")),
            ),
        ],
      });
    }
    return section(cx, {
      title: "Orders",
      id: "orders",
      inset: 88,
      rows: store.orders.map((o) => this.orderRow(cx, o)),
    });
  }

  orderRow(cx, order) {
    const t = cx.theme();
    const first = order.lines[0];
    const product = first && productById(first.product);
    const items = order.lines.reduce((n, l) => n + l.quantity, 0);
    return Button.new(`order-${order.id}`)
      .test_id(`order-${order.id}`)
      .accessibility_label(`Order ${order.id}, ${order.status}, ${items} ${items === 1 ? "item" : "items"}, ${money(order.totals.total)}`)
      .flex()
      .items_center()
      .gap_3()
      .px_4()
      .py_3()
      .active((s) => s.bg(t.muted))
      .on_click((_e, cx) => cx.navigator.push(OrderPage, { orderId: order.id }, { title: `Order ${order.id}` }))
      .child(product ? photoFrame(cx, photo(product, first.color), 56, { radius: 10 }) : div().size("3.5rem"))
      .child(
        v_flex()
          .flex_1()
          .min_w_0()
          .gap_0p5()
          .child(
            h_flex()
              .gap_2()
              .child(text(cx, "body", shortDate(order.placed)).flex_1().min_w_0())
              .child(Tag.new().variant(STATUS_VARIANT[order.status] ?? "secondary").size("small").child(order.status)),
          )
          .child(text(cx, "footnote", `${order.id} · ${items} ${items === 1 ? "item" : "items"} · ${money(order.totals.total)}`, t.muted_foreground)),
      )
      .child(icon("chevron_right", t.muted_foreground, 16));
  }

  demo(cx) {
    const t = cx.theme();
    return section(cx, {
      title: "Demo",
      footer: "While this is on, loading the shop and placing orders fail, to show how the app handles a lost connection.",
      rows: [
        h_flex()
          .gap_3()
          .px_4()
          .py_2()
          .min_h_11()
          .child(icon("flask", t.muted_foreground, 20))
          .child(text(cx, "body", "Simulate network errors").flex_1())
          .child(
            Switch.new("fail-network")
              .test_id("fail-network")
              .accessibility_label("Simulate network errors")
              .checked(store.failNetwork)
              .on_change((on) => {
                store.failNetwork = on;
                store.changed();
              }),
          ),
      ],
    });
  }
}
