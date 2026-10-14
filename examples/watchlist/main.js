// Watchlist: live prices for the instruments a user follows.
//
// It is built on the host app: prices come from the app's `demo/quote` module
// (a request for the first quote, then pushed ticks), the signed-in user from
// `demo/user`, and each row is the app's own native cell (`demo-stock-cell`).
// The script owns the screen around them: the header, sorting, pausing the
// feed, the detail sheet, and adding or removing symbols. `host.json`
// describes what the host registers, for `native-shell check` and `types`.
// See README.md for the design notes.

import { View, div, h_flex, v_flex, list, host_component, shape, Button, Tabs, Spinner } from "native-shell";
import { quote, subscribe, unsubscribe, on } from "demo/quote";
import { current_user } from "demo/user";
import { CATALOG, SORTS, nameOf, loadSymbols, saveSymbols, sorted, formatPrice, formatPercent } from "./catalog.js";

const UNDO_MS = 5000;
const PLUS = "M12 5v14M5 12h14";
const ALERT = "M22 12a10 10 0 1 1-20 0 10 10 0 0 1 20 0ZM12 8v4M12 16h.01";
const LIST = "M8 6h13M8 12h13M8 18h13M3 6h.01M3 12h.01M3 18h.01";

/** A large title's size: 30 pt, growing with the user's text size only up to 1.6×, as native large titles do. */
const titleSize = (cx) => (30 * Math.min(cx.window().font_scale, 1.6)) / cx.window().font_scale;

const icon = (d, color, size = 20) =>
  shape(d, { view_box: [0, 0, 24, 24] })
    .stroke(color, 2)
    .w(size)
    .h(size)
    .flex_none();

export default class Watchlist extends View {
  init(_props, cx) {
    this.user = current_user();
    this.symbols = loadSymbols();
    /** @type {Map<string, import("demo/quote").Quote>} */
    this.quotes = new Map();
    this.sort = 0;
    /** "connecting" | "live" | "paused" | "error" */
    this.status = "connecting";
    this.undo = null;
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

  remove(symbol, cx) {
    const index = this.symbols.indexOf(symbol);
    this.symbols = this.symbols.filter((s) => s !== symbol);
    saveSymbols(this.symbols);
    unsubscribe([symbol]).catch(() => {});
    this.undo?.timer.cancel();
    const timer = cx.timer.after(UNDO_MS, (cx) => {
      this.undo = null;
      cx.notify();
    });
    this.undo = { symbol, index, timer };
    cx.notify();
  }

  restore(cx) {
    if (!this.undo) return;
    const { symbol, index, timer } = this.undo;
    timer.cancel();
    this.undo = null;
    this.symbols = [...this.symbols.slice(0, index), symbol, ...this.symbols.slice(index)];
    saveSymbols(this.symbols);
    subscribe([symbol]).catch(() => {});
    cx.notify();
  }

  render(cx) {
    const t = cx.theme();
    const rows = sorted(this.symbols, this.quotes, this.sort);
    return v_flex()
      .size_full()
      .bg(t.background)
      .line_height(1.375)
      .child(this.header(cx))
      .child(this.body(cx, rows))
      .when(this.undo !== null, (el) => el.child(this.undoBar(cx)))
      .child(div().h(cx.window().safe_area.bottom).flex_none());
  }

  header(cx) {
    const t = cx.theme();
    const count = this.symbols.length;
    return v_flex()
      .px_4()
      .pt(cx.window().safe_area.top + 12)
      .pb_3()
      .gap_3()
      .border_b_1()
      .border_color(t.border)
      .child(
        h_flex()
          .items_start()
          .gap_3()
          .child(
            v_flex()
              .flex_1()
              .min_w_0()
              .gap_1()
              .child(
                div().role("header").text_size(titleSize(cx)).font_bold().line_height(1.15).text_color(t.foreground).child("Watchlist"),
              )
              .child(
                div()
                  .test_id("subtitle")
                  .text_sm()
                  .text_color(t.muted_foreground)
                  .truncate()
                  .child([this.user?.name, `${count} ${count === 1 ? "symbol" : "symbols"}`].filter(Boolean).join(" · ")),
              ),
          )
          .child(
            Button.new("add")
              .test_id("add")
              .accessibility_label("Add symbols")
              .size_11()
              .flex()
              .items_center()
              .justify_center()
              .rounded_full()
              .border_1()
              .border_color(t.border)
              .active((s) => s.bg(t.muted))
              .on_click((_e, cx) => this.openAdd(cx))
              .child(icon(PLUS, t.foreground)),
          ),
      )
      .when(count > 0, (el) =>
        el.child(
          h_flex()
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
                  cx.notify();
                }),
            )
            .child(this.liveToggle(cx)),
        ),
      );
  }

  /** What the feed is doing, and the control that changes it, as one button. */
  liveToggle(cx) {
    const t = cx.theme();
    const live = this.status === "live";
    const paused = this.status === "paused";
    const label = { connecting: "Connecting", live: "Live", paused: "Paused", error: "Offline" }[this.status];
    return Button.new("live")
      .test_id("live")
      .accessibility_label(live ? "Live prices. Pause" : paused ? "Paused. Resume live prices" : label)
      .disabled(!live && !paused)
      .flex()
      .items_center()
      .gap_2()
      .px_3()
      .min_h_8()
      .rounded_full()
      .border_1()
      .border_color(t.border)
      .when(live || paused, (el) => el.active((s) => s.bg(t.muted)).on_click((_e, cx) => this.setLive(!live, cx)))
      .child(
        this.status === "connecting"
          ? Spinner.new().size_3()
          : div()
              .size_2()
              .rounded_full()
              .bg(live ? t.primary : this.status === "error" ? t.destructive : t.muted_foreground),
      )
      .child(div().test_id("status").text_sm().font_medium().text_color(t.foreground).child(label));
  }

  body(cx, rows) {
    const t = cx.theme();
    if (this.status === "error") return this.errorState(cx);
    if (rows.length === 0) {
      return v_flex()
        .test_id("empty")
        .flex_1()
        .items_center()
        .justify_center()
        .gap_2()
        .px_8()
        .pb_16()
        .child(
          div()
            .mb_2()
            .child(icon(LIST, t.muted_foreground, 40)),
        )
        .child(div().text_lg().font_semibold().text_color(t.foreground).child("No symbols yet"))
        .child(
          div().text_sm().text_center().text_color(t.muted_foreground).child("Add the instruments you follow to see their prices here."),
        )
        .child(
          Button.new("empty-add")
            .test_id("empty-add")
            .mt_4()
            .variant("outline")
            .min_h_11()
            .on_click((_e, cx) => this.openAdd(cx))
            .child("Add symbols…"),
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
      .child(div().h(1).ml_4().bg(t.border));
  }

  errorState(cx) {
    const t = cx.theme();
    return v_flex()
      .test_id("error")
      .flex_1()
      .items_center()
      .justify_center()
      .gap_2()
      .px_8()
      .pb_16()
      .child(
        div()
          .mb_2()
          .child(icon(ALERT, t.muted_foreground, 40)),
      )
      .child(div().text_lg().font_semibold().text_color(t.foreground).child("Prices aren’t available"))
      .child(div().text_sm().text_center().text_color(t.muted_foreground).child("Check your connection and try again."))
      .child(
        Button.new("retry")
          .test_id("retry")
          .mt_4()
          .variant("outline")
          .min_h_11()
          .on_click((_e, cx) => {
            this.connect(cx);
            cx.notify();
          })
          .child("Try again"),
      );
  }

  openDetail(symbol, cx) {
    cx.open_sheet(
      () => {
        const t = cx.theme();
        const q = this.quotes.get(symbol);
        const stat = (label, value) =>
          h_flex()
            .justify_between()
            .py_3()
            .border_b_1()
            .border_color(t.border)
            .child(div().text_base().text_color(t.muted_foreground).child(label))
            .child(div().text_base().text_color(t.foreground).child(value));
        return v_flex()
          .test_id("detail")
          .size_full()
          .bg(t.background)
          .px_4()
          .pt_6()
          .pb(cx.window().safe_area.bottom + 16)
          .gap_6()
          .child(
            v_flex()
              .gap_1()
              .child(div().text_sm().font_medium().text_color(t.muted_foreground).child(symbol))
              .child(div().role("header").text_xl().font_semibold().text_color(t.foreground).child(nameOf(symbol)))
              .child(
                h_flex()
                  .items_end()
                  .gap_3()
                  .pt_2()
                  .child(
                    div()
                      .test_id("detail-price")
                      .text_3xl()
                      .font_semibold()
                      .text_color(t.foreground)
                      .child(q ? formatPrice(q.last) : "—"),
                  )
                  .child(
                    div()
                      .pb_1()
                      .text_base()
                      .text_color(t.muted_foreground)
                      .child(q ? `${formatPercent(q.change_percent)} today` : ""),
                  ),
              ),
          )
          .child(
            v_flex()
              .child(stat("Previous close", q ? formatPrice(q.prev_close) : "—"))
              .child(stat("Updates", this.status === "live" ? "Live" : this.status === "paused" ? "Paused" : "Unavailable")),
          )
          .child(
            Button.new("remove")
              .test_id("remove")
              .flex()
              .items_center()
              .justify_center()
              .py_3()
              .rounded_lg()
              .border_1()
              .border_color(t.border)
              .text_base()
              .font_medium()
              .text_color(t.destructive)
              .active((s) => s.bg(t.muted))
              .on_click((_e, cx) => {
                window.close_sheet();
                this.remove(symbol, cx);
              })
              .child("Remove from watchlist"),
          );
      },
      { detents: ["medium"] },
    );
  }

  openAdd(cx) {
    cx.open_sheet(
      () => {
        const t = cx.theme();
        const available = CATALOG.filter((c) => !this.symbols.includes(c.symbol));
        return v_flex()
          .test_id("add-sheet")
          .size_full()
          .bg(t.background)
          .pt_4()
          .pb(cx.window().safe_area.bottom + 16)
          .child(
            h_flex()
              .justify_between()
              .px_4()
              .pb_2()
              .child(div().role("header").text_lg().font_semibold().text_color(t.foreground).child("Add symbols"))
              .child(
                Button.new("add-done")
                  .test_id("add-done")
                  .px_2()
                  .py_2()
                  .hit_slop(8)
                  .text_base()
                  .font_semibold()
                  .text_color(t.primary)
                  .active((s) => s.opacity(0.5))
                  .on_click(() => window.close_sheet())
                  .child("Done"),
              ),
          )
          .when(available.length === 0, (el) =>
            el.child(div().px_4().py_6().text_sm().text_color(t.muted_foreground).child("Every symbol is already on your watchlist.")),
          )
          .children(
            available.map((c) =>
              h_flex()
                .gap_3()
                .px_4()
                .py_3()
                .border_b_1()
                .border_color(t.border)
                .child(
                  v_flex()
                    .flex_1()
                    .min_w_0()
                    .child(div().text_base().font_medium().text_color(t.foreground).child(c.symbol))
                    .child(div().text_sm().text_color(t.muted_foreground).truncate().child(c.name)),
                )
                .child(
                  Button.new(`add-${c.symbol}`)
                    .test_id(`add-${c.symbol}`)
                    .accessibility_label(`Add ${c.name}`)
                    .variant("outline")
                    .h_auto()
                    .min_h_9()
                    .px_3()
                    .on_click((_e, cx) => this.add(c.symbol, cx))
                    .child("Add"),
                ),
            ),
          );
      },
      { detents: ["medium", "large"] },
    );
  }

  undoBar(cx) {
    const t = cx.theme();
    return h_flex()
      .test_id("undo-bar")
      .mx_4()
      .my_2()
      .pl_4()
      .pr_1()
      .py_1()
      .gap_3()
      .rounded_lg()
      .bg(t.foreground)
      .shadow_md()
      .child(div().role("alert").flex_1().min_w_0().text_sm().text_color(t.background).truncate().child(`Removed ${this.undo.symbol}`))
      .child(
        Button.new("undo")
          .test_id("undo")
          .px_3()
          .py_2()
          .rounded_md()
          .text_sm()
          .font_semibold()
          .text_color(t.background)
          .active((s) => s.opacity(0.6))
          .on_click((_e, cx) => this.restore(cx))
          .child("Undo"),
      );
  }
}
