// The product page: photos, the price and rating, the choices a buyer has to
// make (color, size, quantity), then what the product is.
//
// The one primary action, Add to Cart, sits in a bar at the bottom where the
// thumb rests, and states what it will add and for how much. Adding confirms
// with a haptic and a toast that can undo it; the tab bar's badge counts it.

import { View, div, h_flex, v_flex, Button, Tabs, Select, image } from "native-shell";
import { productById, photos, CATEGORIES } from "./catalog.js";
import { store, money, MAX_QUANTITY, FREE_DELIVERY } from "./store.js";
import { icon, navBar, price, productTag, quantityStepper, section, stars, text } from "./ui.js";

const SWIPE = 40;

export default class ProductPage extends View {
  init(props) {
    this.product = productById(props.productId);
    this.back = props.back;
    this.color = this.product.colors[0].id;
    this.size = this.product.sizes?.[0].id ?? null;
    this.quantity = 1;
    this.slide = 0;
    this.dragX = 0;
  }

  get slides() {
    return photos(this.product);
  }

  pickColor(index, cx) {
    this.color = this.product.colors[index].id;
    // The gallery follows: each color's photo is its slide.
    this.slide = index;
    cx.haptic("selection");
    cx.notify();
  }

  showSlide(index, cx) {
    this.slide = Math.max(0, Math.min(this.slides.length - 1, index));
    this.dragX = 0;
    cx.notify();
  }

  add(cx) {
    const p = this.product;
    const line = { product: p.id, color: this.color, size: this.size, quantity: this.quantity };
    const undo = store.add(line);
    cx.haptic("success");
    cx.push_toast({
      id: "added",
      title: "Added to cart",
      description: `${this.quantity} × ${p.name}, ${this.variantLabel()}`,
      level: "success",
      timeout: 4000,
      action: { label: "Undo", on_click: () => undo() },
    });
    this.quantity = 1;
    cx.notify();
  }

  variantLabel() {
    const p = this.product;
    const color = p.colors.find((c) => c.id === this.color)?.label ?? "";
    const size = p.sizes?.find((s) => s.id === this.size)?.label;
    return size ? `${color}, ${size}` : color;
  }

  render(cx) {
    const t = cx.theme();
    const p = this.product;
    const win = cx.window();
    return v_flex()
      .size_full()
      .bg(t.background)
      .line_height(1.3)
      .child(navBar(cx, { title: p.name, back: { label: this.back, on_click: (_e, cx) => cx.navigator.pop() } }))
      .child(
        v_flex()
          .test_id("product")
          .flex_1()
          .overflow_y_scroll()
          .child(this.gallery(cx, win.width))
          .child(
            v_flex()
              .px_4()
              .pt_4()
              .pb_8()
              .gap_6()
              .child(this.heading(cx))
              .child(this.options(cx))
              .child(this.about(cx)),
          ),
      )
      .child(this.buyBar(cx));
  }

  /** A swipeable photo gallery with page dots; each dot is also a button. */
  gallery(cx, width) {
    const t = cx.theme();
    const slides = this.slides;
    const current = slides[this.slide];
    return v_flex()
      .test_id("gallery")
      .role("image")
      .accessibility_label(current.label)
      .accessibility_value(`Photo ${this.slide + 1} of ${slides.length}`)
      .accessibility_hint("Swipe left or right for more photos")
      .child(
        div()
          .w(width)
          .h(width)
          .overflow_hidden()
          .bg(t.muted)
          .on_pan((e, cx) => {
            if (e.phase === "changed") {
              this.dragX = e.dx;
              cx.notify();
            } else if (e.phase === "ended") {
              this.showSlide(this.slide + (e.dx < -SWIPE ? 1 : e.dx > SWIPE ? -1 : 0), cx);
            } else {
              this.showSlide(this.slide, cx);
            }
          })
          .child(
            h_flex()
              .relative()
              .left(-this.slide * width + this.dragX)
              .w(width * slides.length)
              .h(width)
              .children(slides.map((s, i) => image(s.src).test_id(`slide-${i}`).w(width).h(width).flex_none().object_fit("cover"))),
          ),
      )
      .child(
        h_flex()
          .justify_center()
          .gap_0()
          .pt_1()
          .children(
            slides.map((s, i) =>
              Button.new(`dot-${i}`)
                .test_id(`dot-${i}`)
                .accessibility_label(s.label)
                .aria_selected(i === this.slide)
                .p_1p5()
                .hit_slop([8, 4, 8, 4])
                .on_click((_e, cx) => this.showSlide(i, cx))
                .child(div().size_2().rounded_full().bg(i === this.slide ? t.foreground : t.border)),
            ),
          ),
      );
  }

  heading(cx) {
    const t = cx.theme();
    const p = this.product;
    const category = CATEGORIES.find((c) => c.id === p.category)?.label ?? "";
    const tag = productTag(p);
    return v_flex()
      .gap_2()
      .child(
        h_flex()
          .gap_2()
          .child(text(cx, "footnote", category.toUpperCase(), t.muted_foreground).font_semibold())
          .when(tag !== null, (el) => el.child(tag)),
      )
      .child(text(cx, "title2", p.name).font_bold().role("header"))
      .child(stars(cx, p.rating, p.reviews, 16))
      .child(price(cx, p, "title3").test_id("price"));
  }

  options(cx) {
    const t = cx.theme();
    const p = this.product;
    const color = p.colors.find((c) => c.id === this.color);
    const inCart = store.quantityOf(p.id, this.color, this.size);
    return v_flex()
      .gap_5()
      .when(p.colors.length > 1, (el) =>
        el.child(
          v_flex()
            .gap_2()
            .child(text(cx, "subheadline", `Color: ${color?.label ?? ""}`, t.muted_foreground).test_id("color-label"))
            .child(
              Tabs.new("color")
                .test_id("color")
                .accessibility_label("Color")
                .items(p.colors.map((c) => c.label))
                .selected(p.colors.indexOf(color))
                .on_change((i, cx) => this.pickColor(i, cx)),
            ),
        ),
      )
      .when(Boolean(p.sizes), (el) =>
        el.child(
          h_flex()
            .gap_3()
            .child(text(cx, "body", "Size").flex_1())
            .child(
              Select.new("size")
                .test_id("size")
                .accessibility_label("Size")
                .min_w_32()
                .items(p.sizes.map((s) => ({ label: s.label, value: s.id })))
                .value(this.size)
                .on_change((value, cx) => {
                  this.size = value;
                  cx.notify();
                }),
            ),
        ),
      )
      .child(
        h_flex()
          .gap_3()
          .child(
            v_flex()
              .flex_1()
              .child(text(cx, "body", "Quantity"))
              .when(inCart > 0, (el) => el.child(text(cx, "footnote", `${inCart} already in your cart`, t.muted_foreground).test_id("in-cart"))),
          )
          .child(
            quantityStepper(cx, {
              id: "quantity",
              value: this.quantity,
              max: Math.max(1, MAX_QUANTITY - inCart),
              label: "Quantity",
              on_change: (value, cx) => {
                this.quantity = value;
                cx.notify();
              },
            }),
          ),
      );
  }

  about(cx) {
    const t = cx.theme();
    const p = this.product;
    return v_flex()
      .gap_4()
      .child(text(cx, "body", p.summary).line_height(1.4))
      .child(
        section(cx, {
          title: "Details",
          rows: p.details.map((d) => h_flex().px_4().py_3().min_h_11().child(text(cx, "body", d))),
        }),
      )
      .child(
        v_flex()
          .gap_2()
          .child(perk(cx, "truck", `Free standard delivery on orders over ${money(FREE_DELIVERY)}`))
          .child(perk(cx, "return", "Free returns within 30 days")),
      );
  }

  buyBar(cx) {
    const t = cx.theme();
    const atLimit = store.quantityOf(this.product.id, this.color, this.size) >= MAX_QUANTITY;
    return h_flex()
      .flex_none()
      .gap_3()
      .px_4()
      .pt_3()
      .pb(Math.max(cx.window().safe_area.bottom, 12))
      .border_t_1()
      .border_color(t.border)
      .bg(t.background)
      .child(
        Button.new("add-to-cart")
          .test_id("add-to-cart")
          .primary()
          .size("large")
          .flex_1()
          .disabled(atLimit)
          .on_click((_e, cx) => this.add(cx))
          .child(atLimit ? `Limit of ${MAX_QUANTITY} reached` : `Add to Cart · ${money(this.product.price * this.quantity)}`),
      );
  }
}

function perk(cx, name, label) {
  const t = cx.theme();
  return h_flex().gap_3().child(icon(name, t.muted_foreground, 20)).child(text(cx, "subheadline", label, t.muted_foreground).flex_1());
}
