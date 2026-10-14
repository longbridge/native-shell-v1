// Description lists, as an order's details: the status first with the one
// thing to do about it (track the package, or pick another delivery option
// from the split button's menu), then the order, its items and the delivery
// read like a receipt. Less frequent actions wait in the More menu. The
// order loads first, as it would from a server.

import { View, h_flex, v_flex, Spinner } from "native-shell";
import { DescriptionList, DescriptionItem, DropdownButton, GroupBox, Icon, Menu, MenuItem } from "gpui-component";
import { label, page } from "../ui.js";

const ORDER = {
  number: "W4471-2289",
  placed: "Oct 12, 2026",
  payment: "Visa •••• 4242",
  shipTo: "Ada Lovelace, 12 Harbour Street, Bristol BS1 4XX",
  items: [
    { name: "Linen apron", quantity: 1, price: 38 },
    { name: "Herb scissors", quantity: 2, price: 12 },
    { name: "Seed tray set", quantity: 1, price: 16.5 },
  ],
  shipping: 4.95,
  taxRate: 0.08,
  carrier: "Royal Mail",
  tracking: "RM 22 334 556 GB",
  instructions: "Leave with the neighbour at number 14 if no one answers.",
};

const money = (n) => `$${n.toFixed(2)}`;

export default class OrderPage extends View {
  init(_props, cx) {
    this.loading = true;
    /** The delivery option chosen from the split button's menu, if any. */
    this.option = null;
    cx.spawn(async (cx) => {
      await cx.sleep(500);
      this.loading = false;
      cx.notify();
    });
  }

  choose(option, cx) {
    this.option = option;
    cx.haptic("selection");
    cx.notify();
  }

  render(cx) {
    const t = cx.theme();
    const subtotal = ORDER.items.reduce((sum, it) => sum + it.quantity * it.price, 0);
    const tax = Math.round(subtotal * ORDER.taxRate * 100) / 100;
    const total = subtotal + ORDER.shipping + tax;
    const intro = "An order from Harbour Supply Co. Each section reads like a receipt: a label, and its value on the right.";
    if (this.loading) {
      return page(cx, {
        title: "Order",
        intro,
        children: [
          h_flex()
            .test_id("order-loading")
            .justify_center()
            .gap_2()
            .py_8()
            .child(Spinner.new().size_5())
            .child(label(cx, "subheadline", t.muted_foreground, "Loading order…")),
        ],
      });
    }
    return page(cx, {
      title: "Order",
      intro,
      children: [
        new GroupBox()
          .title("Status")
          .variant("fill")
          .test_id("status")
          .child(
            h_flex()
              .gap_3()
              .items_start()
              .child(new Icon("icons/circle-check.svg").size("large").color(t.success).accessibility_label("On its way"))
              .child(
                v_flex()
                  .flex_1()
                  .min_w_0()
                  .gap_0p5()
                  .child(label(cx, "headline", t.foreground, "Arriving today by 6 PM"))
                  .child(
                    label(
                      cx,
                      "subheadline",
                      t.muted_foreground,
                      this.option ?? "Out for delivery in Bristol. Updated 9:41 AM.",
                    ).test_id("delivery-note"),
                  ),
              ),
          )
          .child(
            new DropdownButton("track", "Track Package")
              .variant("primary")
              .test_id("track")
              .on_click((_e, cx) => cx.push_toast({ title: "Tracking opens in Royal Mail", level: "info", timeout: 2000 }))
              .menu_item("Deliver to a Neighbour", (cx) => this.choose("A neighbour can take it: number 14 is on the list.", cx))
              .menu_item("Leave in a Safe Place", (cx) => this.choose("It will be left in the porch, out of sight.", cx))
              .menu_item("Collect from a Post Office", (cx) => this.choose("Held at Bristol Central from tomorrow, 9 AM.", cx)),
          ),
        new GroupBox()
          .title("Order")
          .child(
            new DescriptionList()
              .test_id("order-details")
              .child(new DescriptionItem("Order number").value(ORDER.number))
              .child(new DescriptionItem("Placed").value(ORDER.placed))
              .child(new DescriptionItem("Payment").value(ORDER.payment))
              .child(new DescriptionItem("Ship to").value(ORDER.shipTo)),
          ),
        new GroupBox().title("Items").child(
          new DescriptionList()
            .test_id("items")
            .children(
              ORDER.items.map((it) =>
                new DescriptionItem(it.quantity > 1 ? `${it.name} × ${it.quantity}` : it.name).value(money(it.quantity * it.price)),
              ),
            )
            .item("Shipping", money(ORDER.shipping))
            .item("Tax", money(tax))
            .item("Total", label(cx, "headline", t.foreground, money(total)).test_id("total")),
        ),
        new GroupBox().title("Delivery").child(
          new DescriptionList()
            .vertical()
            .columns(2)
            .test_id("delivery")
            .child(new DescriptionItem("Carrier").value(ORDER.carrier))
            .child(new DescriptionItem("Tracking").value(ORDER.tracking))
            .child(new DescriptionItem("Instructions").value(ORDER.instructions).span(2)),
        ),
        h_flex()
          .justify_between()
          .px_4()
          .on_action("invoice", (_e, cx) => cx.push_toast({ title: "Invoice saved to Files", level: "success", timeout: 2000 }))
          .on_action("problem", (_e, cx) =>
            cx.alert({
              title: "Report a problem with this order?",
              message: "Harbour Supply Co. will reply by email within a day.",
              actions: [{ label: "Cancel", style: "cancel" }, { label: "Report" }],
            }),
          )
          .child(label(cx, "footnote", t.muted_foreground, "Returns open once it arrives."))
          .child(
            new Menu("More")
              .test_id("more")
              .child(new MenuItem("Download Invoice", "invoice"))
              .child(new MenuItem("Report a Problem", "problem"))
              .child(new MenuItem("Return Items", "return").disabled(true)),
          ),
      ],
    });
  }
}
