// Watchlist: live prices for the instruments a user follows.
//
// It is built on the host app: prices come from the app's `demo/quote` module
// (a request for the first quote, then pushed ticks), the signed-in user from
// `demo/user`, and each row is the app's own native cell (`demo-stock-cell`).
// The script owns the screen around them, laid out as an iOS plain list under
// a large title: sorting, pausing the feed, the detail sheet, and adding or
// removing symbols. `host.json` describes what the host registers, for
// `nshell check` and `nshell types`. See README.md for the design notes.

import { View, div, h_flex, v_flex, list, host_component, Button, Tabs, Spinner } from "native-shell";
import { quote, subscribe, unsubscribe, on } from "demo/quote";
import { current_user } from "demo/user";
import { CATALOG, SORTS, nameOf, loadSymbols, saveSymbols, sorted, formatPrice, formatPercent } from "./catalog.js";
import { emptyState, icon, label, largeTitle, row, section, statusBar, textButton, textStyle } from "./ui.js";
/** @import { Context } from "native-shell" */

export default class Watchlist extends View {
  init(_props, cx) {
    this.user = current_user();
    this.symbols = loadSymbols();
    /** @type {Map<string, import("demo/quote").Quote>} */
    this.quotes = new Map();
    this.sort = 0;
    /** "connecting" | "live" | "paused" | "error" */
    this.status = "connecting";
    this.subscription = null;
    this.connect(cx);
  }

  /** First quotes by request, then pushed ticks for the same symbols. */
  connect(cx) {
    this.status = "connecting";
    cx.spawn(async (cx) => {
      try {
        const first = await Promise.all(this.symbols.map((s) => quote(s)));
        for (const q of first) this.quotes.set(q.symbol, q);
        await subscribe(this.symbols);
        this.listen();
        this.status = "live";
      } catch {
        this.status = "error";
      }
      cx.notify();
    });
  }

  // The host counts listeners, so dropping the last one lets it stop the feed.
  listen() {
    this.subscription ??= on("tick", (tick, cx) => {
      this.quotes.set(tick.symbol, tick);
      cx.notify();
    });
  }

  release() {
    this.subscription?.cancel();
  }

  setLive(live, cx) {
    if (live) {
      this.listen();
    } else {
      this.subscription?.cancel();
      this.subscription = null;
    }
    this.status = live ? "live" : "paused";
    cx.notify();
  }

  add(symbol, cx) {
    this.symbols = [...this.symbols, symbol];
    saveSymbols(this.symbols);
    cx.notify();
    cx.spawn(async (cx) => {
      try {
        this.quotes.set(symbol, await quote(symbol));
        await subscribe([symbol]);
      } catch {
        // The row keeps its placeholder; the next tick or a retry fills it.
      }
      cx.notify();
    });
  }

  /** Removing is reversible for a few seconds: the toast's Undo puts the symbol back where it was. @param {string} symbol @param {Context} cx */
  remove(symbol, cx) {
    const before = this.symbols;
    this.symbols = this.symbols.filter((s) => s !== symbol);
    saveSymbols(this.symbols);
    unsubscribe([symbol]).catch(() => {});
    cx.notify();
    cx.push_toast({
      id: "undo",
      title: `Removed ${symbol}`,
      timeout: 5000,
      action: {
        label: "Undo",
        on_click: (cx) => {
          this.symbols = before;
          saveSymbols(this.symbols);
          subscribe([symbol]).catch(() => {});
          cx.notify();
        },
      },
    });
  }

  render(cx) {
    const t = cx.theme();
    const rows = sorted(this.symbols, this.quotes, this.sort);
    return v_flex()
      .size_full()
      .bg(t.background)
      .child(statusBar(cx, t.background))
      .child(this.header(cx))
      .child(this.body(cx, rows))
      .child(div().h(cx.window().safe_area.bottom).flex_none());
  }

  /** @param {Context} cx */
  header(cx) {
    const t = cx.theme();
    const count = this.symbols.length;
    return v_flex()
      .pb_3()
      .border_b(0.5)
      .border_color(t.border)
      .child(
        largeTitle(cx, {
          title: "Watchlist",
          subtitle: [this.user?.name, `${count} ${count === 1 ? "symbol" : "symbols"}`].filter(Boolean).join(" · "),
          trailing: Button.new("add")
            .test_id("add")
            .accessibility_label("Add symbols")
            .size_11()
            .flex()
            .items_center()
            .justify_center()
            .active((s) => s.opacity(0.4))
            .on_click((_e, cx) => this.openAdd(cx))
            .child(icon("plus", t.primary, 24)),
        }),
      )
      .when(count > 0, (el) =>
        el.child(
          h_flex()
            .px_4()
            .gap_3()
            .child(
              Tabs.new("sort")
                .test_id("sort")
                .accessibility_label("Sort by")
                .flex_1()
                .items(SORTS)
                .selected(this.sort)
                .on_change((i, cx) => {
                  this.sort = i;
                  cx.haptic("selection");
                  cx.notify();
                }),
            )
            .child(this.liveToggle(cx)),
        ),
      );
  }

  /** What the feed is doing, and the control that changes it, as one tinted capsule. @param {Context} cx */
  liveToggle(cx) {
    const t = cx.theme();
    const live = this.status === "live";
    const paused = this.status === "paused";
    const text = { connecting: "Connecting", live: "Live", paused: "Paused", error: "Offline" }[this.status];
    return textStyle(Button.new("live"), cx, "subheadline")
      .test_id("live")
      .accessibility_label(live ? "Live prices. Pause" : paused ? "Paused. Resume live prices" : text)
      .disabled(!live && !paused)
      .flex()
      .items_center()
      .gap_1p5()
      .px_3()
      .min_h_8()
      .hit_slop(6)
      .rounded_full()
      .bg(t.accent)
      .text_color(t.accent_foreground)
      .font_semibold()
      .when(live || paused, (el) => el.active((s) => s.opacity(0.5)).on_click((_e, cx) => this.setLive(!live, cx)))
      .child(
        this.status === "connecting"
          ? Spinner.new().size_3()
          : div()
              .size_2()
              .rounded_full()
              .bg(live ? t.success : this.status === "error" ? t.destructive : t.muted_foreground),
      )
      .child(div().test_id("status").child(text));
  }

  /** @param {Context} cx @param {string[]} rows */
  body(cx, rows) {
    const t = cx.theme();
    if (this.status === "error") {
      return v_flex()
        .test_id("error")
        .flex_1()
        .child(
          emptyState(
            cx,
            "alert",
            "Prices Aren’t Available",
            "Check your connection and try again.",
            textButton(cx, "retry", "Try Again", {
              emphasized: true,
              on_click: (_e, cx) => {
                this.connect(cx);
                cx.notify();
              },
            }),
          ),
        );
    }
    if (rows.length === 0) {
      return emptyState(
        cx,
        "chart",
        "No Symbols",
        "Add the instruments you follow to see their prices here.",
        textButton(cx, "empty-add", "Add Symbols…", { emphasized: true, on_click: (_e, cx) => this.openAdd(cx) }),
      );
    }
    return list(
      "watchlist",
      rows.length,
      (i) => rows[i],
      (i, cx) => this.row(rows[i], cx),
    )
      .test_id("watchlist")
      .flex_1()
      .estimated_row_height(64);
  }

  /** @param {string} symbol @param {Context} cx */
  row(symbol, cx) {
    const t = cx.theme();
    const q = this.quotes.get(symbol);
    return v_flex()
      .child(
        host_component("demo-stock-cell", {
          symbol,
          name: nameOf(symbol),
          last: q?.last ?? null,
          change_percent: q?.change_percent ?? null,
        })
          .test_id(`cell-${symbol}`)
          .w_full()
          .on("select", (_payload, cx) => this.openDetail(symbol, cx)),
      )
      .child(div().h(0.5).ml_4().bg(t.border));
  }

  /** @param {string} symbol @param {Context} cx */
  openDetail(symbol, cx) {
    cx.open_sheet(
      (cx) => {
        const t = cx.theme();
        const q = this.quotes.get(symbol);
        return v_flex()
          .test_id("detail")
          .size_full()
          .bg(t.grouped_background)
          .px_4()
          .pt_6()
          .pb(cx.window().safe_area.bottom + 16)
          .gap_6()
          .child(
            v_flex()
              .px_4()
              .child(label(cx, "subheadline", t.muted_foreground, symbol))
              .child(label(cx, "title2", t.foreground, nameOf(symbol)).font_bold().role("header"))
              .child(
                h_flex()
                  .items_end()
                  .gap_2()
                  .pt_2()
                  .child(label(cx, "large_title", t.foreground, q ? formatPrice(q.last) : "—").test_id("detail-price"))
                  .child(label(cx, "body", t.muted_foreground, q ? `${formatPercent(q.change_percent)} today` : "").pb_1()),
              ),
          )
          .child(
            section(cx, {
              rows: [
                row(cx, { id: "prev-close", label: "Previous Close", value: q ? formatPrice(q.prev_close) : "—" }),
                row(cx, {
                  id: "updates",
                  label: "Updates",
                  value: this.status === "live" ? "Live" : this.status === "paused" ? "Paused" : "Unavailable",
                }),
              ],
            }),
          )
          .child(
            section(cx, {
              rows: [
                row(cx, {
                  id: "remove",
                  label: "Remove from Watchlist",
                  destructive: true,
                  on_click: (_e, cx) => {
                    window.close_sheet();
                    this.remove(symbol, cx);
                  },
                }),
              ],
            }),
          );
      },
      { detents: ["medium"] },
    );
  }

  /** @param {Context} cx */
  openAdd(cx) {
    cx.open_sheet(
      (cx) => {
        const t = cx.theme();
        const available = CATALOG.filter((c) => !this.symbols.includes(c.symbol));
        return v_flex()
          .test_id("add-sheet")
          .size_full()
          .bg(t.grouped_background)
          .pb(cx.window().safe_area.bottom + 16)
          .child(
            h_flex()
              .relative()
              .justify_center()
              .px_2()
              .pt_3()
              .pb_2()
              .child(label(cx, "headline", t.foreground, "Add Symbols").role("header"))
              .child(
                div()
                  .absolute()
                  .right_2()
                  .top_2()
                  .child(textButton(cx, "add-done", "Done", { emphasized: true, on_click: () => window.close_sheet() })),
              ),
          )
          .child(
            div()
              .px_4()
              .pt_3()
              .child(
                section(cx, {
                  footer:
                    available.length === 0
                      ? "Every symbol is already on your watchlist."
                      : "Prices start updating as soon as a symbol is added.",
                  rows: available.map((c) =>
                    row(cx, {
                      id: `row-${c.symbol}`,
                      label: c.symbol,
                      detail: c.name,
                      trailing: textStyle(Button.new(`add-${c.symbol}`), cx, "subheadline")
                        .test_id(`add-${c.symbol}`)
                        .accessibility_label(`Add ${c.name}`)
                        .px_3()
                        .min_h_8()
                        .hit_slop(6)
                        .flex()
                        .items_center()
                        .rounded_full()
                        .bg(t.accent)
                        .text_color(t.accent_foreground)
                        .font_semibold()
                        .active((s) => s.opacity(0.5))
                        .on_click((_e, cx) => {
                          cx.haptic("success");
                          this.add(c.symbol, cx);
                        })
                        .child("Add"),
                    }),
                  ),
                }),
              ),
          );
      },
      { detents: ["medium", "large"] },
    );
  }
}
