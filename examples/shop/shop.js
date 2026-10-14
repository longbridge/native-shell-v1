// The Shop tab: a featured product, category chips and a two-column grid.
//
// The catalog arrives from the (simulated) network, so the tab has a loading
// state drawn as the grid it will become, an error state with a retry, and
// pull to refresh. Choosing a category filters the grid in place; a card
// opens the product page.

import { View, div, h_flex, v_flex, Button, image } from "native-shell";
import { Skeleton, Empty, EmptyHeader, EmptyMedia, EmptyTitle, EmptyDescription, EmptyContent } from "gpui-component";
import { BRAND, CATEGORIES, photo, productById } from "./catalog.js";
import ProductPage from "./product.js";
import { store } from "./store.js";
import { icon, largeTitle, photoFrame, price, productTag, stars, statusBar, text } from "./ui.js";

const FEATURED = "throw";

export default class ShopTab extends View {
  init(_props, cx) {
    this.category = "all";
    /** @type {import("./catalog.js").Product[] | null} */
    this.products = null;
    this.error = null;
    this.refreshing = false;
    this.load(cx);
  }

  load(cx) {
    this.error = null;
    cx.spawn(async (cx) => {
      try {
        this.products = await store.fetchCatalog(cx);
        this.error = null;
      } catch (e) {
        this.error = e.message;
        // Products already on screen stay; only the refresh failed.
        if (this.products !== null) cx.push_toast({ title: "Couldn’t refresh the shop", description: e.message, level: "error" });
      }
      this.refreshing = false;
      cx.notify();
    });
  }

  retry(cx) {
    this.products = null;
    this.load(cx);
    cx.notify();
  }

  open(product, cx) {
    cx.navigator.push(ProductPage, { productId: product.id, back: "Shop" }, { title: product.name });
  }

  render(cx) {
    const t = cx.theme();
    const width = cx.window().width;
    return v_flex()
      .size_full()
      .bg(t.background)
      .child(statusBar(cx))
      .child(
        v_flex()
          .test_id("catalog")
          .id("catalog")
          .flex_1()
          .overflow_y_scroll()
          .refreshing(this.refreshing)
          .on_refresh((_e, cx) => {
            this.refreshing = true;
            this.load(cx);
            cx.notify();
          })
          .child(largeTitle(cx, "Shop", `${BRAND} · Free delivery over $50`))
          .child(this.body(cx, width)),
      );
  }

  body(cx, width) {
    if (this.error !== null && this.products === null) return this.errorState(cx);
    if (this.products === null) return this.loading(cx, width);
    const shown = this.products.filter((p) => this.category === "all" || p.category === this.category);
    return v_flex()
      .pb_8()
      .gap_4()
      .when(this.category === "all", (el) => el.child(this.featured(cx, width)))
      .child(this.chips(cx))
      .child(this.grid(cx, shown, width));
  }

  /** The season's pick: one product, large, with why it is worth a look. */
  featured(cx, width) {
    const t = cx.theme();
    const product = productById(FEATURED);
    if (!product) return null;
    // At large text sizes the photo moves above the words, which then get the full width.
    const stacked = cx.window().font_scale >= 1.5;
    return Button.new("featured")
      .test_id("featured")
      .accessibility_label(`Featured: ${product.name}, on sale`)
      .mx_4()
      .rounded_xl()
      .overflow_hidden()
      .bg(t.muted)
      .flex()
      .when(stacked, (el) => el.flex_col_reverse())
      .items_stretch()
      .active((s) => s.opacity(0.7))
      .on_click((_e, cx) => this.open(product, cx))
      .child(
        v_flex()
          .flex_1()
          .min_w_0()
          .gap_1()
          .p_4()
          .child(text(cx, "footnote", "AUTUMN EDIT", t.muted_foreground).font_semibold())
          .child(text(cx, "title3", "20% off wool throws").font_semibold())
          .child(text(cx, "subheadline", "Lambswool, woven in herringbone. Until Sunday.", t.muted_foreground)),
      )
      .child(
        image(photo(product, "rust"))
          .when(stacked, (el) => el.w_full().h(140))
          .when(!stacked, (el) => el.w(Math.min(128, width * 0.32)))
          .flex_none()
          .object_fit("cover")
          .accessibility_hidden(),
      );
  }

  chips(cx) {
    const t = cx.theme();
    return h_flex()
      .test_id("categories")
      .role("tab_list")
      .id("categories")
      .overflow_x_scroll()
      .gap_2()
      .px_4()
      .children(
        CATEGORIES.map((c) => {
          const selected = c.id === this.category;
          return Button.new(`category-${c.id}`)
            .test_id(`category-${c.id}`)
            .role("tab")
            .aria_selected(selected)
            .flex_none()
            .flex()
            .items_center()
            .px_4()
            .min_h_9()
            .hit_slop([4, 0, 4, 0])
            .rounded_full()
            .bg(selected ? t.foreground : t.muted)
            .active((s) => s.opacity(0.6))
            .on_click((_e, cx) => {
              this.category = c.id;
              cx.haptic("selection");
              cx.notify();
            })
            .child(text(cx, "subheadline", c.label, selected ? t.background : t.foreground).font_medium());
        }),
      );
  }

  grid(cx, products, width) {
    const side = cardSide(width);
    const rows = [];
    for (let i = 0; i < products.length; i += 2) rows.push(products.slice(i, i + 2));
    return v_flex()
      .test_id("grid")
      .px_4()
      .gap_6()
      .children(
        rows.map((pair) =>
          h_flex()
            .items_start()
            .gap_3()
            .children(pair.map((p) => this.card(cx, p, side)))
            .when(pair.length === 1, (el) => el.child(div().w(side))),
        ),
      );
  }

  card(cx, product, side) {
    const t = cx.theme();
    const tag = productTag(product);
    return Button.new(`product-${product.id}`)
      .test_id(`product-${product.id}`)
      .accessibility_label(`${product.name}, ${product.was ? "on sale, " : ""}${(product.price / 100).toFixed(2)} dollars`)
      .w(side)
      .flex()
      .flex_col()
      .gap_2()
      .active((s) => s.opacity(0.6))
      .on_click((_e, cx) => this.open(product, cx))
      .child(
        div()
          .relative()
          .child(photoFrame(cx, photo(product), side))
          .when(tag !== null, (el) => el.child(div().absolute().top_2().left_2().child(tag))),
      )
      .child(
        v_flex()
          .gap_0p5()
          .child(text(cx, "subheadline", product.name).font_medium())
          .child(price(cx, product))
          .child(stars(cx, `card-${product.id}`, product.rating, product.reviews))
          .when(product.colors.length > 1, (el) =>
            el.child(text(cx, "caption1", `${product.colors.length} colors`, t.muted_foreground)),
          ),
      );
  }

  /** The grid's shape in placeholders, so the page doesn't jump when products arrive. */
  loading(cx, width) {
    const side = cardSide(width);
    const card = () =>
      v_flex()
        .w(side)
        .gap_2()
        .child(Skeleton.new().w(side).h(side).rounded_xl())
        .child(Skeleton.new().w(side * 0.8).h_4())
        .child(Skeleton.new().w(side * 0.4).h_4());
    return v_flex()
      .test_id("catalog-loading")
      .accessibility_label("Loading products")
      .px_4()
      .pt_2()
      .gap_6()
      .child(Skeleton.new().h_24().rounded_xl())
      .children([0, 1].map(() => h_flex().gap_3().child(card()).child(card())));
  }

  errorState(cx) {
    const t = cx.theme();
    return Empty.new()
      .test_id("catalog-error")
      .pt_16()
      .header(
        EmptyHeader.new()
          .media(EmptyMedia.new().variant("icon").child(icon("wifi_off", t.muted_foreground, 32)))
          .title(EmptyTitle.new().child("Can’t load the shop"))
          .description(EmptyDescription.new().child("Check your connection and try again. Your cart is saved on this device.")),
      )
      .content(
        EmptyContent.new().child(
          Button.new("catalog-retry")
            .test_id("catalog-retry")
            .outline()
            .on_click((_e, cx) => this.retry(cx))
            .child("Try again"),
        ),
      );
  }
}

/** Two cards per row between 16 pt margins and a 12 pt gutter, at most 240 pt wide (large phones in landscape). */
function cardSide(width) {
  return Math.min(240, Math.floor((width - 32 - 12) / 2));
}
