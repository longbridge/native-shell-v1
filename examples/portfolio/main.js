// Portfolio: how an investor's holdings are doing, and the record behind them.
//
// One scrolling page answers the first question—what is it worth, and how did
// today go—before anything else: the total, the day's change, the value over
// a chosen range, how it is spread, then the holdings themselves. A holding
// opens its own page; a transaction is added in a sheet; alerts live on a
// settings page. Prices are simulated (market.js) and update live while the
// app is in front, which the runtime takes care of: timers pause while the
// app is in the background. See README.md for the design notes.

import { View, div, h_flex, v_flex, Button, Spinner } from "native-shell";
import { DropdownMenu, PieChart, Skeleton, WarningAlert } from "gpui-component";
import { Market, instrument } from "./market.js";
import {
  loadTransactions,
  saveTransactions,
  loadSettings,
  saveSettings,
  positions,
  totals,
  series,
  money,
  signedMoney,
  signedPercent,
  percent,
  shares,
} from "./portfolio.js";
import { rangeChart } from "./chart.js";
import { openTransactionForm } from "./transaction.js";
import HoldingPage from "./holding.js";
import AlertsPage from "./alerts.js";
import { barButton, changeColor, emptyState, icon, section, statusBar, titleSize } from "./ui.js";

/** How often live prices move. */
const TICK_MS = 2000;
/** The simulated network's round trip. */
const LOAD_MS = 400;
const REFRESH_MS = 1200;
const UNDO_MS = 5000;

const SORTS = [
  { label: "Value", value: "value" },
  { label: "Today’s change", value: "day" },
  { label: "Total return", value: "gain" },
  { label: "Symbol", value: "symbol" },
];

/** At most this many slices; the rest are drawn as one. */
const SLICES = 5;

export default class Portfolio extends View {
  init(_props, cx) {
    this.market = new Market();
    this.transactions = loadTransactions(this.market);
    this.nextId = this.transactions.reduce((max, t) => Math.max(max, t.id), 0) + 1;
    this.settings = loadSettings();
    this.range = "1D";
    this.sort = "value";
    /** "loading" | "live" | "offline" */
    this.status = "loading";
    /** Whether any prices have arrived; until then offline is a full-screen error. */
    this.loaded = false;
    this.refreshing = false;
    this.retrying = false;
    /** Symbols already alerted today, so a move is announced once. */
    this.alerted = new Set();
    this.saveFailed = false;
    this.load(cx);
    cx.timer.every(TICK_MS, (cx) => this.tick(cx));
  }

  /** The first prices, after a simulated round trip. */
  load(cx) {
    cx.spawn(async (cx) => {
      await cx.sleep(LOAD_MS);
      this.connected();
    });
  }

  connected() {
    this.status = this.settings.offline ? "offline" : "live";
    if (this.status === "live") this.loaded = true;
    window.refresh();
  }

  tick(cx) {
    if (this.status !== "live") return;
    const before = new Map(positions(this.transactions, this.market).map((p) => [p.symbol, p.dayPercent]));
    this.market.tick();
    this.announceMoves(before, cx);
    // Open pages read the same market, so every view redraws, not just this one.
    window.refresh();
  }

  /** A holding crossing the alert threshold is announced once, with the move in words. */
  announceMoves(before, cx) {
    const s = this.settings;
    if (!s.moveAlerts) return;
    for (const p of positions(this.transactions, this.market)) {
      const was = Math.abs(before.get(p.symbol) ?? 0);
      if (s.muted.includes(p.symbol) || this.alerted.has(p.symbol)) continue;
      if (was < s.threshold && Math.abs(p.dayPercent) >= s.threshold) {
        this.alerted.add(p.symbol);
        cx.push_toast({
          id: `move-${p.symbol}`,
          title: `${p.symbol} is ${p.dayPercent < 0 ? "down" : "up"} ${signedPercent(p.dayPercent).slice(1)} today`,
          description: `${instrument(p.symbol).name} · ${money(p.price)}`,
          level: "info",
        });
      }
    }
  }

  /** Pull to refresh: a fresh round trip, which fails while the network is off. */
  refresh(cx) {
    this.refreshing = true;
    cx.notify();
    cx.spawn(async (cx) => {
      await cx.sleep(REFRESH_MS);
      this.refreshing = false;
      this.status = this.settings.offline ? "offline" : "live";
      if (this.status === "live") {
        this.loaded = true;
        this.market.tick();
      }
      window.refresh();
    });
  }

  retry(cx) {
    this.retrying = true;
    cx.notify();
    // Owned by the portfolio, so a retry started from a page outlives the page.
    cx.spawn(
      async (cx) => {
        await cx.sleep(LOAD_MS);
        this.retrying = false;
        this.connected();
      },
      { owner: this },
    );
  }

  // Transactions ------------------------------------------------------------

  commit(cx) {
    this.saveFailed = !saveTransactions(this.transactions);
    window.refresh();
    cx.notify();
  }

  addTransaction(draft, cx) {
    const transaction = { ...draft, id: this.nextId++ };
    this.transactions = [...this.transactions, transaction];
    this.commit(cx);
    cx.haptic("success");
    const verb = draft.type === "buy" ? "Bought" : "Sold";
    cx.push_toast({
      id: "transaction",
      title: `${verb} ${shares(draft.shares)} ${draft.symbol}`,
      description: `${money(draft.shares * draft.price)} at ${money(draft.price)} a share`,
      level: "success",
      duration: UNDO_MS,
      action: {
        label: "Undo",
        on_click: (cx) => this.removeTransaction(transaction.id, cx),
      },
    });
  }

  removeTransaction(id, cx) {
    this.transactions = this.transactions.filter((t) => t.id !== id);
    this.commit(cx);
  }

  deleteTransaction(transaction, cx) {
    const removed = transaction;
    this.removeTransaction(transaction.id, cx);
    cx.push_toast({
      id: "transaction",
      title: "Transaction deleted",
      level: "info",
      duration: UNDO_MS,
      action: {
        label: "Undo",
        on_click: (cx) => {
          this.transactions = [...this.transactions, removed];
          this.commit(cx);
        },
      },
    });
  }

  erase(cx) {
    this.transactions = [];
    this.commit(cx);
  }

  setSettings(patch, cx) {
    const wasOffline = this.settings.offline;
    this.settings = { ...this.settings, ...patch };
    saveSettings(this.settings);
    if (patch.offline !== undefined && patch.offline !== wasOffline) {
      // Turning the network off is noticed at once; turning it on, at the next round trip.
      if (patch.offline) this.status = "offline";
      else this.retry(cx);
    }
    window.refresh();
    cx.notify();
  }

  openForm(cx, symbol = null) {
    openTransactionForm(cx, {
      market: this.market,
      transactions: () => this.transactions,
      symbol,
      save: (draft, cx) => this.addTransaction(draft, cx),
    });
  }

  openHolding(symbol, cx) {
    cx.navigator.push(HoldingPage, { symbol, app: this }, { title: symbol });
  }

  openAlerts(cx) {
    cx.navigator.push(AlertsPage, { app: this }, { title: "Alerts" });
  }

  // Rendering ---------------------------------------------------------------

  render(cx) {
    const t = cx.theme();
    const held = positions(this.transactions, this.market);
    return v_flex()
      .size_full()
      .bg(t.grouped_background)
      .line_height(1.375)
      .child(statusBar(cx))
      .child(
        v_flex()
          .test_id("portfolio")
          .id("portfolio")
          .flex_1()
          .overflow_y_scroll()
          .when(this.loaded, (el) => el.on_refresh((_e, cx) => this.refresh(cx)).refreshing(this.refreshing))
          .child(this.header(cx))
          .child(this.body(cx, held)),
      );
  }

  /** iOS's large-title layout: the bar's buttons above, the title on its own line beneath. */
  header(cx) {
    const t = cx.theme();
    return v_flex()
      .px_4()
      .pb_2()
      .child(
        h_flex()
          .justify_end()
          .gap_1()
          .mr_neg_2()
          .child(barButton(cx, "alerts", "bell", "Alerts", (_e, cx) => this.openAlerts(cx)))
          .child(barButton(cx, "add", "plus", "Add transaction", (_e, cx) => this.openForm(cx))),
      )
      .child(div().role("header").text_size(titleSize(cx)).font_bold().line_height(1.15).text_color(t.foreground).child("Portfolio"));
  }

  body(cx, held) {
    const t = cx.theme();
    const content = v_flex()
      .px_4()
      .pt_2()
      .pb(cx.window().safe_area.bottom + 32)
      .gap_6();
    if (this.status === "loading" || (!this.loaded && this.retrying)) return content.child(this.placeholder(cx));
    if (!this.loaded) {
      return content.child(
        emptyState(cx, {
          id: "error",
          icon: "wifi_off",
          title: "Prices aren’t available",
          message: "Your portfolio needs a connection to load prices. Check your connection and try again.",
          action: Button.new("retry")
            .test_id("retry")
            .outline()
            .on_click((_e, cx) => this.retry(cx))
            .child("Try again"),
        }),
      );
    }
    if (held.length === 0) {
      return content.child(
        emptyState(cx, {
          id: "empty",
          icon: "chart",
          title: "No holdings yet",
          message: "Add a purchase to start tracking what your investments are worth.",
          action: Button.new("empty-add")
            .test_id("empty-add")
            .primary()
            .size("large")
            .px_6()
            .on_click((_e, cx) => this.openForm(cx))
            .child("Add transaction"),
        }),
      );
    }
    return content
      .when(this.status === "offline", (el) => el.child(this.offlineBanner(cx)))
      .when(this.saveFailed, (el) =>
        el.child(
          div()
            .role("alert")
            .text_sm()
            .text_color(t.destructive)
            .child("Couldn’t save on this device. Your changes last until the app closes."),
        ),
      )
      .child(this.summary(cx, held))
      .child(
        rangeChart(cx, {
          id: "value",
          title: "Portfolio value",
          rows: series(this.transactions, this.market, this.range),
          range: this.range,
          on_range: (range, cx) => {
            this.range = range;
            cx.notify();
          },
        }),
      )
      .child(this.allocation(cx, held))
      .child(this.holdings(cx, held))
      .child(
        div()
          .test_id("updated")
          .text_xs()
          .text_center()
          .text_color(t.muted_foreground)
          .child(`Simulated prices · ${this.status === "live" ? "Updating live" : "Not updating while offline"}`),
      );
  }

  /** The page's shape while the first prices load, as iOS draws redacted content. */
  placeholder(cx) {
    const t = cx.theme();
    const line = (w, h = 16) => Skeleton.new().w(w).h(h);
    return v_flex()
      .test_id("loading")
      .accessibility_label("Loading your portfolio")
      .gap_6()
      .child(v_flex().gap_2().child(line(88, 14)).child(line(200, 36)).child(line(150)))
      .child(Skeleton.new().h(32).rounded_lg())
      .child(Skeleton.new().h(184).rounded_xl())
      .child(
        v_flex()
          .rounded_xl()
          .bg(t.surface)
          .p_4()
          .gap_4()
          .children(
            [0, 1, 2, 3].map(() =>
              h_flex()
                .gap_4()
                .child(v_flex().flex_1().gap_2().child(line(64)).child(line(160, 12)))
                .child(line(72)),
            ),
          ),
      );
  }

  summary(cx, held) {
    const t = cx.theme();
    const sum = totals(held);
    return v_flex()
      .gap_1()
      .child(div().text_sm().text_color(t.muted_foreground).child("Total value"))
      .child(div().test_id("total").text_3xl().font_bold().text_color(t.foreground).child(money(sum.value)))
      .child(
        h_flex()
          .gap_2()
          .flex_wrap()
          .child(
            div()
              .test_id("day-change")
              .text_base()
              .font_medium()
              .text_color(changeColor(t, sum.dayChange))
              .child(`${signedMoney(sum.dayChange)} (${signedPercent(sum.dayPercent)})`),
          )
          .child(div().text_base().text_color(t.muted_foreground).child("Today")),
      )
      .child(
        h_flex()
          .gap_2()
          .flex_wrap()
          .child(
            div()
              .test_id("total-gain")
              .text_sm()
              .text_color(changeColor(t, sum.gain))
              .child(`${signedMoney(sum.gain)} (${signedPercent(sum.gainPercent)})`),
          )
          .child(div().text_sm().text_color(t.muted_foreground).child("All time")),
      );
  }

  offlineBanner(cx) {
    const t = cx.theme();
    return WarningAlert.new("offline", "Prices may be out of date.")
      .test_id("offline")
      .title("You’re offline")
      .icon(icon("wifi_off", t.warning))
      .child(
        Button.new("offline-retry")
          .test_id("offline-retry")
          .ghost()
          .size("small")
          .disabled(this.retrying)
          .on_click((_e, cx) => this.retry(cx))
          .when(this.retrying, (el) => el.child(Spinner.new().size_3()))
          .child(this.retrying ? "Retrying" : "Retry"),
      );
  }

  allocation(cx, held) {
    const t = cx.theme();
    const sum = totals(held).value;
    const byValue = [...held].sort((a, b) => b.value - a.value);
    const top = byValue.slice(0, byValue.length > SLICES ? SLICES - 1 : SLICES);
    const rest = byValue.slice(top.length);
    const rows = top.map((p) => ({ label: p.symbol, value: p.value }));
    if (rest.length > 0)
      rows.push({
        label: "Other",
        value: rest.reduce((s, p) => s + p.value, 0),
      });
    const colors = sliceColors(t);
    return section(cx, {
      title: "Allocation",
      rows: [
        h_flex()
          .test_id("allocation")
          .gap_4()
          .p_4()
          .child(
            div()
              .size(112)
              .flex_none()
              .child(
                PieChart.new(() => rows)
                  .test_id("allocation-chart")
                  .accessibility_label(`Allocation: ${rows.map((r) => `${r.label} ${percent((r.value / sum) * 100)}`).join(", ")}`)
                  .inner_radius(58)
                  .value_format(money)
                  .pad_angle(0.02)
                  .size_full(),
              ),
          )
          .child(
            v_flex()
              .flex_1()
              .min_w_0()
              .gap_1p5()
              .accessibility_hidden(true)
              .children(
                rows.map((r, i) =>
                  h_flex()
                    .gap_2()
                    .child(
                      div()
                        .size_2p5()
                        .flex_none()
                        .rounded_full()
                        .bg(colors[i % colors.length]),
                    )
                    .child(div().flex_1().min_w_0().truncate().text_sm().text_color(t.foreground).child(r.label))
                    .child(
                      div()
                        .flex_none()
                        .text_sm()
                        .text_color(t.muted_foreground)
                        .child(percent((r.value / sum) * 100)),
                    ),
                ),
              ),
          ),
      ],
    });
  }

  holdings(cx, held) {
    const t = cx.theme();
    const sorted = [...held].sort(
      {
        value: (a, b) => b.value - a.value,
        day: (a, b) => b.dayPercent - a.dayPercent,
        gain: (a, b) => b.gainPercent - a.gainPercent,
        symbol: (a, b) => a.symbol.localeCompare(b.symbol),
      }[this.sort],
    );
    return section(cx, {
      id: "holdings",
      title: `Holdings · ${held.length}`,
      trailing: SORTS.reduce(
        (menu, sort) =>
          menu.item(sort.label, (cx) => {
            this.sort = sort.value;
            cx.haptic("selection");
            cx.notify();
          }),
        DropdownMenu.new("sort", `Sort: ${SORTS.find((s) => s.value === this.sort).label}`)
          .test_id("sort")
          .accessibility_label("Sort holdings"),
      ),
      rows: sorted.map((p) => this.holdingRow(cx, p)),
    });
  }

  holdingRow(cx, p) {
    const t = cx.theme();
    const trailing = this.sort === "gain" ? signedPercent(p.gainPercent) : signedPercent(p.dayPercent);
    const trailingValue = this.sort === "gain" ? p.gain : p.dayChange;
    return Button.new(`holding-${p.symbol}`)
      .test_id(`holding-${p.symbol}`)
      .accessibility_label(`${p.symbol}, ${p.name}, ${money(p.value)}, ${trailing} ${this.sort === "gain" ? "total" : "today"}`)
      .flex()
      .items_center()
      .gap_3()
      .px_4()
      .py_3()
      .min_h_12()
      .active((s) => s.bg(t.muted))
      .on_click((_e, cx) => this.openHolding(p.symbol, cx))
      .child(
        v_flex()
          .flex_1()
          .min_w_0()
          .child(div().text_base().font_semibold().text_color(t.foreground).child(p.symbol))
          .child(
            div()
              .text_sm()
              .text_color(t.muted_foreground)
              .truncate()
              .child(`${shares(p.shares)} shares · ${p.name}`),
          ),
      )
      .child(
        v_flex()
          .items_end()
          .flex_none()
          .child(div().test_id(`value-${p.symbol}`).text_base().text_color(t.foreground).child(money(p.value)))
          .child(div().test_id(`change-${p.symbol}`).text_sm().font_medium().text_color(changeColor(t, trailingValue)).child(trailing)),
      )
      .child(icon("chevron_right", t.muted_foreground, 16));
  }
}

/** The legend's swatches: the chart tokens, in the order the pie gives them to its slices. */
function sliceColors(t) {
  return [t.chart_1, t.chart_2, t.chart_3, t.chart_4, t.chart_5];
}
