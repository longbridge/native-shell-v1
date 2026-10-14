// The Search tab: a field at the top, suggestions before anything is typed,
// results as they are typed, and a way out when nothing matches.
//
// Results filter on every keystroke (the catalog is small and local), so
// Return only puts the keyboard away. Tapping a suggestion fills the field.

import { View, h_flex, v_flex, list, Button, Input, InputState } from "native-shell";
import { Empty, EmptyHeader, EmptyMedia, EmptyTitle, EmptyDescription, EmptyContent } from "gpui-component";
import { search, photo, CATEGORIES } from "./catalog.js";
import ProductPage from "./product.js";
import { icon, largeTitle, photoFrame, price, statusBar, text } from "./ui.js";

const SUGGESTIONS = ["Mug", "Linen", "Wool", "Brass", "Bag", "Terracotta"];

export default class SearchTab extends View {
  init() {
    this.query = InputState.new({
      placeholder: "Products, colors and more",
      return_key: "search",
      auto_capitalize: "none",
    });
    this.query.on("change", (_e, cx) => cx.notify());
    this.query.on("submit", (_e, cx) => cx.blur());
  }

  release() {
    this.query.release();
  }

  setQuery(value, cx) {
    this.query.set_value(value);
    cx.notify();
  }

  render(cx) {
    const t = cx.theme();
    const q = this.query.value().trim();
    const results = search(q);
    return v_flex()
      .size_full()
      .bg(t.background)
      .child(statusBar(cx))
      .child(largeTitle(cx, "Search"))
      .child(this.field(cx, q))
      .child(q === "" ? this.suggestions(cx) : results.length === 0 ? this.noResults(cx, q) : this.results(cx, results));
  }

  field(cx, q) {
    const t = cx.theme();
    return h_flex()
      .mx_4()
      .mb_2()
      .pl_3()
      .gap_2()
      .rounded_lg()
      .bg(t.muted)
      .child(icon("search", t.muted_foreground, 18))
      .child(
        Input.new(this.query)
          .test_id("search")
          .accessibility_label("Search products")
          .flex_1()
          .min_h_10()
          .py_2()
          .text_size(t.typography.body.size)
          .text_color(t.foreground),
      )
      .when(q !== "", (el) =>
        el.child(
          Button.new("search-clear")
            .test_id("search-clear")
            .accessibility_label("Clear search")
            .size_10()
            .flex()
            .items_center()
            .justify_center()
            .active((s) => s.opacity(0.5))
            .on_click((_e, cx) => this.setQuery("", cx))
            .child(icon("close", t.muted_foreground, 16)),
        ),
      );
  }

  suggestions(cx) {
    const t = cx.theme();
    return v_flex()
      .test_id("suggestions")
      .px_4()
      .pt_4()
      .gap_3()
      .child(text(cx, "headline", "Popular searches").role("header"))
      .child(
        h_flex()
          .flex_wrap()
          .gap_2()
          .children(
            SUGGESTIONS.map((s) =>
              Button.new(`suggest-${s.toLowerCase()}`)
                .test_id(`suggest-${s.toLowerCase()}`)
                .flex()
                .items_center()
                .gap_1p5()
                .px_3()
                .min_h_9()
                .rounded_full()
                .border_1()
                .border_color(t.border)
                .active((s) => s.bg(t.muted))
                .on_click((_e, cx) => this.setQuery(s, cx))
                .child(icon("search", t.muted_foreground, 14))
                .child(text(cx, "subheadline", s)),
            ),
          ),
      );
  }

  results(cx, results) {
    const t = cx.theme();
    return v_flex()
      .flex_1()
      .min_h_0()
      .child(
        text(cx, "footnote", `${results.length} ${results.length === 1 ? "result" : "results"}`, t.muted_foreground)
          .test_id("result-count")
          .px_4()
          .py_2(),
      )
      .child(
        list(
          "results",
          results.length,
          (i) => results[i].id,
          (i, cx) => this.row(cx, results[i]),
        )
          .test_id("results")
          .flex_1()
          .estimated_row_height(88),
      );
  }

  row(cx, product) {
    const t = cx.theme();
    const category = CATEGORIES.find((c) => c.id === product.category)?.label ?? "";
    return Button.new(`result-${product.id}`)
      .test_id(`result-${product.id}`)
      .accessibility_label(product.name)
      .flex()
      .items_center()
      .gap_3()
      .pl_4()
      .active((s) => s.bg(t.muted))
      .on_click((_e, cx) => {
        cx.blur();
        cx.navigator.push(ProductPage, { productId: product.id, back: "Search" }, { title: product.name });
      })
      .child(photoFrame(cx, photo(product), 64, { radius: 10 }))
      .child(
        h_flex()
          .flex_1()
          .min_w_0()
          .gap_2()
          .py_3()
          .pr_4()
          .border_b_1()
          .border_color(t.border)
          .child(
            v_flex()
              .flex_1()
              .min_w_0()
              .gap_0p5()
              .child(text(cx, "body", product.name))
              .child(
                text(
                  cx,
                  "footnote",
                  product.colors.length > 1 ? `${category} · ${product.colors.length} colors` : category,
                  t.muted_foreground,
                ),
              )
              .child(price(cx, product)),
          )
          .child(icon("chevron_right", t.muted_foreground, 16)),
      );
  }

  noResults(cx, q) {
    const t = cx.theme();
    return Empty.new()
      .test_id("no-results")
      .pt_12()
      .header(
        EmptyHeader.new()
          .media(EmptyMedia.new().variant("icon").child(icon("search", t.muted_foreground, 32)))
          .title(EmptyTitle.new().child(`No results for “${q}”`))
          .description(EmptyDescription.new().child("Check the spelling, or try a more general word like “mug” or “bag”.")),
      )
      .content(
        EmptyContent.new().child(
          Button.new("no-results-clear")
            .test_id("no-results-clear")
            .outline()
            .on_click((_e, cx) => this.setQuery("", cx))
            .child("Clear search"),
        ),
      );
  }
}
