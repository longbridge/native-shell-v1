// Shop: a small store for everyday goods, from browsing to a placed order.
//
// Four tabs, as a store app on iOS has them: Shop (categories and a product
// grid), Search, Cart (with a badge counting what's in it) and Account (the
// order history). Each tab keeps its own state while another is showing, so
// a search or a scrolled grid is still there on the way back. Products and
// checkout open as pushed pages over the tabs. See README.md.

import { View, div, h_flex, v_flex, Button } from "native-shell";
import { Badge } from "gpui-component";
import ShopTab from "./shop.js";
import SearchTab from "./search.js";
import CartTab from "./cart.js";
import AccountTab from "./account.js";
import { store } from "./store.js";
import { icon, text } from "./ui.js";

/** @type {{ id: string, label: string, icon: Parameters<typeof icon>[0], view: import("native-shell").ViewClass }[]} */
const TABS = [
  { id: "shop", label: "Shop", icon: "shop", view: ShopTab },
  { id: "search", label: "Search", icon: "search", view: SearchTab },
  { id: "cart", label: "Cart", icon: "cart", view: CartTab },
  { id: "account", label: "Account", icon: "user", view: AccountTab },
];

export default class Shop extends View {
  init(_props, cx) {
    this.tab = "shop";
    const selectTab = (id, cx) => this.select(id, cx);
    this.views = Object.fromEntries(TABS.map((t) => [t.id, cx.new(t.view, { selectTab })]));
  }

  select(id, cx) {
    if (id === this.tab) return;
    this.tab = id;
    cx.haptic("selection");
    window.refresh();
  }

  render(cx) {
    const t = cx.theme();
    return v_flex()
      .size_full()
      .bg(t.background)
      .line_height(1.3)
      .child(div().flex_1().min_h_0().child(this.views[this.tab]))
      .child(this.tabBar(cx));
  }

  tabBar(cx) {
    const t = cx.theme();
    const count = store.count;
    return h_flex()
      .test_id("tab-bar")
      .role("tab_list")
      .flex_none()
      .items_start()
      .pt_1()
      .pb(Math.max(cx.window().safe_area.bottom, 8))
      .border_t_1()
      .border_color(t.border)
      .bg(t.surface)
      .children(
        TABS.map((tab) => {
          const selected = tab.id === this.tab;
          const color = selected ? t.primary : t.muted_foreground;
          const glyph = icon(tab.icon, color, 24);
          return Button.new(`tab-${tab.id}`)
            .test_id(`tab-${tab.id}`)
            .role("tab")
            .aria_selected(selected)
            .accessibility_label(tab.id === "cart" && count > 0 ? `Cart, ${count} ${count === 1 ? "item" : "items"}` : tab.label)
            .flex_1()
            .flex()
            .flex_col()
            .items_center()
            .gap_0p5()
            .min_h_11()
            .pt_1()
            .active((s) => s.opacity(0.5))
            .on_click((_e, cx) => this.select(tab.id, cx))
            .child(tab.id === "cart" ? Badge.new().count(count).size("small").child(glyph) : glyph)
            .child(text(cx, "caption2", tab.label, color).font_medium());
        }),
      );
  }
}
