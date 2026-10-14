// The Cart tab: what's in the cart, how much it comes to, and the way to
// checkout.
//
// Each line has a quantity stepper and a visible Remove button; swiping a
// line left reveals the same Remove, as iOS lists do. Removing is immediate
// and undoable from the toast. A promo code is checked when applied and the
// reason it doesn't apply sits under the field. The free-delivery bar says
// how far the cart is from free shipping.

import { View, div, h_flex, v_flex, Button, Input, InputState, Progress } from "native-shell";
import { Empty, EmptyHeader, EmptyMedia, EmptyTitle, EmptyDescription, EmptyContent, ErrorAlert } from "gpui-component";
import { productById, photo } from "./catalog.js";
import CheckoutPage from "./checkout.js";
import ProductPage from "./product.js";
import { store, money, MAX_QUANTITY, FREE_DELIVERY } from "./store.js";
import { icon, largeTitle, photoFrame, quantityStepper, section, text, valueRow } from "./ui.js";

/** How far a line slides to reveal Remove, and how far a swipe must travel to leave it open. */
const REVEAL = 96;
const SWIPE = 48;

export default class CartTab extends View {
  init(props) {
    this.selectTab = props.selectTab;
    this.code = InputState.new({ placeholder: "Promo code", return_key: "done", auto_capitalize: "characters" });
    this.code.on("change", (_e, cx) => {
      this.promoError = null;
      cx.notify();
    });
    this.code.on("submit", (_e, cx) => this.applyPromo(cx));
    this.promoError = null;
    /** The line swiped open, and the drag in progress. */
    this.open = null;
    this.drag = null;
  }

  release() {
    this.code.release();
  }

  applyPromo(cx) {
    if (this.code.value().trim() === "") return;
    this.promoError = store.applyPromo(this.code.value());
    if (this.promoError === null) {
      this.code.set_value("");
      cx.blur();
      cx.haptic("success");
    } else {
      cx.haptic("error");
    }
    cx.notify();
  }

  remove(index, cx) {
    const product = productById(store.cart[index].product);
    const undo = store.remove(index);
    this.open = null;
    cx.push_toast({
      id: "removed",
      title: `Removed ${product?.name ?? "item"}`,
      timeout: 5000,
      action: { label: "Undo", on_click: () => undo() },
    });
  }

  checkout(cx) {
    cx.blur();
    cx.navigator.push(CheckoutPage, { selectTab: this.selectTab }, { title: "Checkout" });
  }

  render(cx) {
    const t = cx.theme();
    const count = store.count;
    return v_flex()
      .size_full()
      .bg(t.grouped_background)
      .child(div().h(cx.window().safe_area.top).flex_none())
      .child(
        store.cart.length === 0
          ? v_flex().flex_1().child(largeTitle(cx, "Cart")).child(this.empty(cx))
          : v_flex()
              .flex_1()
              .min_h_0()
              .child(
                v_flex()
                  .test_id("cart")
                  .id("cart")
                  .flex_1()
                  .overflow_y_scroll()
                  .child(largeTitle(cx, "Cart", `${count} ${count === 1 ? "item" : "items"}`))
                  .child(
                    v_flex()
                      .px_4()
                      .pt_2()
                      .pb_8()
                      .gap_6()
                      .when(store.saveFailed, (el) =>
                        el.child(
                          ErrorAlert.new("save-error", "Your cart couldn’t be saved on this device. It will be lost when the app closes.")
                            .test_id("save-error"),
                        ),
                      )
                      .child(this.lines(cx))
                      .child(this.promo(cx))
                      .child(this.summary(cx)),
                  ),
              )
              .child(this.checkoutBar(cx)),
      );
  }

  lines(cx) {
    return section(cx, {
      id: "lines",
      inset: 104,
      rows: store.cart.map((line, i) => this.line(cx, line, i)),
      footer: this.freeDelivery(cx),
    });
  }

  line(cx, line, index) {
    const t = cx.theme();
    const p = productById(line.product);
    if (!p) return null;
    const key = `${line.product}-${line.color}-${line.size ?? ""}`;
    const color = p.colors.find((c) => c.id === line.color)?.label ?? "";
    const size = p.sizes?.find((s) => s.id === line.size)?.label;
    const offset = this.drag?.key === key ? Math.min(0, this.drag.base + this.drag.dx) : this.open === key ? -REVEAL : 0;
    // At large text sizes the line total moves under the name, which keeps the width to wrap in.
    const large = cx.window().font_scale >= 1.5;
    const total = text(cx, "body", money(p.price * line.quantity)).font_semibold().test_id(`line-total-${index}`);
    // Remove sits behind the line, drawn only while the line is moved aside.
    return div()
      .key(key)
      .relative()
      .overflow_hidden()
      .when(offset < 0, (el) => el.bg(t.destructive))
      .when(offset < 0, (el) => el.child(
        Button.new(`swipe-remove-${index}`)
          .test_id(`swipe-remove-${index}`)
          .accessibility_hidden()
          .absolute()
          .top_0()
          .bottom_0()
          .right_0()
          .w(REVEAL)
          .flex()
          .flex_col()
          .items_center()
          .justify_center()
          .gap_1()
          .active((s) => s.opacity(0.7))
          .on_click((_e, cx) => this.remove(index, cx))
          .child(icon("trash", t.destructive_foreground, 20))
          .child(text(cx, "footnote", "Remove", t.destructive_foreground).font_semibold()),
      ))
      .child(
        h_flex()
          .test_id(`line-${index}`)
          .relative()
          .left(offset)
          .items_start()
          .gap_3()
          .px_4()
          .py_3()
          .bg(t.surface)
          .on_pan((e, cx) => this.swipe(key, e, cx))
          .child(
            Button.new(`line-photo-${index}`)
              .accessibility_label(`Open ${p.name}`)
              .active((s) => s.opacity(0.6))
              .on_click((_e, cx) => cx.navigator.push(ProductPage, { productId: p.id, back: "Cart" }, { title: p.name }))
              .child(photoFrame(cx, photo(p, line.color), 76, { radius: 10 })),
          )
          .child(
            v_flex()
              .flex_1()
              .min_w_0()
              .gap_1()
              .child(
                h_flex()
                  .items_start()
                  .gap_2()
                  .child(text(cx, "body", p.name).flex_1().min_w_0())
                  .when(!large, (el) => el.child(total)),
              )
              .child(text(cx, "footnote", size ? `${color} · ${size}` : color, t.muted_foreground))
              .when(large, (el) => el.child(total))
              .child(
                h_flex()
                  .flex_wrap()
                  .gap_2()
                  .pt_1()
                  .justify_between()
                  .child(
                    quantityStepper(cx, {
                      id: `line-${index}-quantity`,
                      value: line.quantity,
                      max: MAX_QUANTITY,
                      label: `Quantity of ${p.name}`,
                      on_change: (value) => store.setQuantity(index, value),
                    }),
                  )
                  .child(
                    Button.new(`remove-${index}`)
                      .test_id(`remove-${index}`)
                      .accessibility_label(`Remove ${p.name}`)
                      .ghost()
                      .size("small")
                      .text_color(t.destructive)
                      .on_click((_e, cx) => this.remove(index, cx))
                      .child("Remove"),
                  ),
              ),
          ),
      );
  }

  /** A horizontal drag on a line: follows the finger, then settles open or closed. */
  swipe(key, e, cx) {
    if (e.phase === "began") {
      if (this.open !== null && this.open !== key) this.open = null;
      this.drag = { key, base: this.open === key ? -REVEAL : 0, dx: 0 };
    } else if (e.phase === "changed" && this.drag?.key === key) {
      this.drag.dx = e.dx;
    } else if (this.drag?.key === key) {
      const end = this.drag.base + this.drag.dx;
      this.open = e.phase === "ended" && end < -SWIPE ? key : null;
      this.drag = null;
    }
    cx.notify();
  }

  freeDelivery(cx) {
    const t = cx.theme();
    const amount = store.subtotal() - store.discount();
    const left = FREE_DELIVERY - amount;
    return v_flex()
      .test_id("free-delivery")
      .px_4()
      .pt_1()
      .gap_2()
      .child(
        h_flex()
          .gap_2()
          .child(icon("truck", left > 0 ? t.muted_foreground : t.success, 16))
          .child(
            text(
              cx,
              "footnote",
              left > 0 ? `Add ${money(left)} more for free standard delivery.` : "Your order ships free with standard delivery.",
              t.muted_foreground,
            )
              .flex_1()
              .min_w_0(),
          ),
      )
      .when(left > 0, (el) => el.child(Progress.new("free-delivery-progress").value(Math.round((amount / FREE_DELIVERY) * 100)).h_1()));
  }

  promo(cx) {
    const t = cx.theme();
    const applied = store.promo;
    if (applied) {
      return section(cx, {
        title: "Promo code",
        rows: [
          h_flex()
            .gap_3()
            .px_4()
            .py_3()
            .min_h_11()
            .child(icon("tag", t.success, 18))
            .child(
              v_flex()
                .flex_1()
                .min_w_0()
                .child(text(cx, "body", applied.code).test_id("promo-applied"))
                .child(text(cx, "footnote", applied.label, t.muted_foreground)),
            )
            .child(
              Button.new("promo-remove")
                .test_id("promo-remove")
                .accessibility_label(`Remove code ${applied.code}`)
                .ghost()
                .size("small")
                .on_click(() => store.removePromo())
                .child("Remove"),
            ),
        ],
        footer: store.discount() === 0 ? "This code no longer applies to your cart’s total." : undefined,
      });
    }
    const empty = this.code.value().trim() === "";
    return section(cx, {
      title: "Promo code",
      rows: [
        h_flex()
          .gap_2()
          .pl_4()
          .pr_2()
          .py_1()
          .child(
            Input.new(this.code)
              .test_id("promo")
              .accessibility_label("Promo code")
              .flex_1()
              .min_h_11()
              .text_size(t.typography.body.size)
              .text_color(t.foreground),
          )
          .child(
            Button.new("promo-apply")
              .test_id("promo-apply")
              .ghost()
              .size("small")
              .disabled(empty)
              .on_click((_e, cx) => this.applyPromo(cx))
              .child("Apply"),
          ),
      ],
      footer: this.promoError
        ? h_flex()
            .px_4()
            .gap_1p5()
            .items_start()
            .child(icon("tag", t.destructive, 14).mt_0p5())
            .child(text(cx, "footnote", this.promoError, t.destructive).test_id("promo-error").role("alert").flex_1())
        : "Codes are applied before tax.",
    });
  }

  summary(cx) {
    const t = cx.theme();
    const totals = store.totals();
    return section(cx, {
      title: "Summary",
      rows: [
        valueRow(cx, "Subtotal", money(totals.subtotal), { id: "subtotal" }),
        totals.discount > 0 && valueRow(cx, "Discount", money(-totals.discount), { id: "discount", color: t.success }),
        valueRow(cx, "Delivery", "At checkout"),
        valueRow(cx, "Estimated tax", money(totals.tax)),
        valueRow(cx, "Estimated total", money(totals.subtotal - totals.discount + totals.tax), { id: "estimated-total", strong: true }),
      ],
    });
  }

  checkoutBar(cx) {
    const t = cx.theme();
    return v_flex()
      .flex_none()
      .px_4()
      .py_3()
      .border_t_1()
      .border_color(t.border)
      .bg(t.background)
      .child(
        Button.new("checkout")
          .test_id("checkout")
          .primary()
          .size("large")
          .on_click((_e, cx) => this.checkout(cx))
          .child("Checkout"),
      );
  }

  empty(cx) {
    const t = cx.theme();
    return Empty.new()
      .test_id("cart-empty")
      .flex_1()
      .pb_16()
      .header(
        EmptyHeader.new()
          .media(EmptyMedia.new().variant("icon").child(icon("cart", t.muted_foreground, 32)))
          .title(EmptyTitle.new().child("Your cart is empty"))
          .description(EmptyDescription.new().child("Things you add from the shop wait here until you check out.")),
      )
      .content(
        EmptyContent.new().child(
          Button.new("browse")
            .test_id("browse")
            .outline()
            .on_click((_e, cx) => this.selectTab("shop", cx))
            .child("Browse the Shop"),
        ),
      );
  }
}
