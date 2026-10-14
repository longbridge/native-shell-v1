// One holding: its price, what the position is worth to the investor, the
// instrument's statistics, and the transactions that built the position.
//
// The page reads the portfolio's market and transactions, so live prices and
// a transaction added from here show up at once. A transaction is deleted
// from its row's alert, with Undo afterwards.

import { View, div, h_flex, v_flex, Button } from "native-shell";
import { instrument } from "./market.js";
import { positions, totals, priceSeries, money, signedMoney, signedPercent, percent, shares, longDate } from "./portfolio.js";
import { DescriptionList } from "gpui-component";
import { rangeChart } from "./chart.js";
import { barButton, changeColor, navBar, section } from "./ui.js";

export default class HoldingPage extends View {
  init(props) {
    /** @type {string} */
    this.symbol = props.symbol;
    /** @type {import("./main.js").default} */
    this.app = props.app;
    this.range = "1D";
  }

  confirmDelete(transaction, cx) {
    const what = `${transaction.type === "buy" ? "Buy" : "Sell"} ${shares(transaction.shares)} ${transaction.symbol}`;
    cx.alert({
      title: `Delete “${what}”?`,
      message: `The ${longDate(transaction.date)} transaction will no longer count toward your returns.`,
      actions: [
        { label: "Cancel", style: "cancel" },
        {
          label: "Delete",
          style: "destructive",
          on_press: (cx) => this.app.deleteTransaction(transaction, cx),
        },
      ],
    });
  }

  render(cx) {
    const t = cx.theme();
    const market = this.app.market;
    const inst = instrument(this.symbol);
    const held = positions(this.app.transactions, market);
    const position = held.find((p) => p.symbol === this.symbol);
    const price = market.price(this.symbol);
    const prev = market.previousClose(this.symbol);
    const day = price - prev;
    const history = this.app.transactions
      .filter((tr) => tr.symbol === this.symbol)
      .sort((a, b) => (a.date === b.date ? b.id - a.id : a.date < b.date ? 1 : -1));
    return v_flex()
      .size_full()
      .bg(t.grouped_background)
      .line_height(1.375)
      .child(
        navBar(cx, {
          title: this.symbol,
          back: {
            label: "Portfolio",
            on_click: (_e, cx) => cx.navigator.pop(),
          },
          trailing: barButton(cx, "holding-add", "plus", `Add ${this.symbol} transaction`, (_e, cx) => this.app.openForm(cx, this.symbol)),
        }),
      )
      .child(
        v_flex()
          .test_id("holding")
          .flex_1()
          .overflow_y_scroll()
          .px_4()
          .pt_4()
          .pb(cx.window().safe_area.bottom + 32)
          .gap_6()
          .child(
            v_flex()
              .gap_1()
              .child(div().role("header").text_xl().font_semibold().text_color(t.foreground).child(inst.name))
              .child(div().test_id("price").text_3xl().font_bold().text_color(t.foreground).child(money(price)))
              .child(
                h_flex()
                  .gap_2()
                  .child(
                    div()
                      .test_id("price-change")
                      .text_base()
                      .font_medium()
                      .text_color(changeColor(t, day))
                      .child(`${signedMoney(day)} (${signedPercent((day / prev) * 100)})`),
                  )
                  .child(div().text_base().text_color(t.muted_foreground).child("Today")),
              ),
          )
          .child(
            rangeChart(cx, {
              id: "price",
              title: `${this.symbol} price`,
              rows: priceSeries(market, this.symbol, this.range),
              range: this.range,
              on_range: (range, cx) => {
                this.range = range;
                cx.notify();
              },
            }),
          )
          .child(position ? this.positionSection(cx, position, totals(held).value) : this.notHeld(cx))
          .child(this.statistics(cx))
          .child(this.transactions(cx, history)),
      );
  }

  positionSection(cx, p, portfolioValue) {
    const t = cx.theme();
    // Plain values are secondary text, as the list draws its own; gains and losses keep their colors.
    const colored = (id, text, color) => div().test_id(id).text_color(color).child(text);
    return titled(
      cx,
      "Your position",
      DescriptionList.new()
        .test_id("position")
        .item("Shares", colored("position-shares-value", shares(p.shares), t.muted_foreground))
        .item("Average cost", money(p.cost / p.shares))
        .item("Market value", colored("position-market-value-value", money(p.value), t.muted_foreground))
        .item(
          "Today’s return",
          colored(
            "position-today-return-value",
            `${signedMoney(p.dayChange)} (${signedPercent(p.dayPercent)})`,
            changeColor(t, p.dayChange),
          ),
        )
        .item(
          "Total return",
          colored("position-total-return-value", `${signedMoney(p.gain)} (${signedPercent(p.gainPercent)})`, changeColor(t, p.gain)),
        )
        .item("Portfolio weight", percent((p.value / portfolioValue) * 100)),
    );
  }

  notHeld(cx) {
    const t = cx.theme();
    return div()
      .test_id("not-held")
      .px_4()
      .py_3()
      .rounded_xl()
      .bg(t.muted)
      .text_sm()
      .text_color(t.muted_foreground)
      .child(`You don’t hold ${this.symbol} right now. Its transactions stay below for your records.`);
  }

  statistics(cx) {
    const market = this.app.market;
    const [dayLow, dayHigh] = market.dayRange(this.symbol);
    const [yearLow, yearHigh] = market.yearRange(this.symbol);
    return titled(
      cx,
      "Statistics",
      DescriptionList.new()
        .test_id("statistics")
        .item("Previous close", money(market.previousClose(this.symbol)))
        .item("Day’s range", `${money(dayLow)} – ${money(dayHigh)}`)
        .item("52-week range", `${money(yearLow)} – ${money(yearHigh)}`)
        .item("Sector", instrument(this.symbol).sector),
    );
  }

  transactions(cx, history) {
    const t = cx.theme();
    if (history.length === 0) {
      return section(cx, {
        title: "Transactions",
        rows: [div().px_4().py_3().text_sm().text_color(t.muted_foreground).child("No transactions.")],
      });
    }
    return section(cx, {
      id: "transactions",
      title: "Transactions",
      footer: "Tap a transaction to delete it.",
      rows: history.map((tr) => {
        const verb = tr.type === "buy" ? "Bought" : "Sold";
        return Button.new(`transaction-${tr.id}`)
          .test_id(`transaction-${tr.id}`)
          .accessibility_label(`${verb} ${shares(tr.shares)} shares at ${money(tr.price)}, ${longDate(tr.date)}`)
          .accessibility_hint("Deletes after asking")
          .flex()
          .items_center()
          .gap_3()
          .px_4()
          .py_3()
          .active((s) => s.bg(t.muted))
          .on_click((_e, cx) => this.confirmDelete(tr, cx))
          .child(
            v_flex()
              .flex_1()
              .min_w_0()
              .child(
                div()
                  .text_base()
                  .text_color(t.foreground)
                  .child(`${verb} ${shares(tr.shares)} shares`),
              )
              .child(
                div()
                  .text_sm()
                  .text_color(t.muted_foreground)
                  .child(`${longDate(tr.date)} · ${money(tr.price)} each`),
              ),
          )
          .child(
            div()
              .flex_none()
              .text_base()
              .text_color(t.foreground)
              .child(money(tr.shares * tr.price)),
          );
      }),
    });
  }
}

/** A grouped section's header over a list that draws its own card. */
function titled(cx, title, list) {
  return v_flex()
    .gap_2()
    .child(div().role("header").px_4().text_sm().font_medium().text_color(cx.theme().muted_foreground).child(title))
    .child(list);
}
