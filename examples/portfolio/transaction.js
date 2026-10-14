// The transaction sheet: record a purchase or a sale.
//
// The draft lives in this closure until Add. Fields check themselves when the
// user commits, and each message sits under the field it is about and says
// what would be accepted. A sale can't exceed what was held on its date. The
// total updates as the user types, so the amount is confirmed before Add, and
// the toast afterwards offers Undo. Leaving with a draft asks first.

import { div, h_flex, v_flex, Input, InputState, Select, DatePicker, Tabs } from "native-shell";
import { INSTRUMENTS, isoDate } from "./market.js";
import { earliestDate, money, sharesOn, shares as formatShares, longDate } from "./portfolio.js";
import { hairline, textButton } from "./ui.js";

const TYPES = ["Buy", "Sell"];
const SHARES = /^\d+(\.\d{1,4})?$/;
const PRICE = /^\d+(\.\d{1,4})?$/;

/**
 * @param {import("native-shell").Context} cx
 * @param {{ market: import("./market.js").Market, transactions: () => import("./portfolio.js").Transaction[], symbol: string | null, save: (draft: Omit<import("./portfolio.js").Transaction, "id">, cx: import("native-shell").Context) => void }} opts
 */
export function openTransactionForm(cx, { market, transactions, symbol, save }) {
  const today = isoDate(new Date());
  /** @type {{ type: "buy" | "sell", symbol: string, date: string }} */
  const draft = {
    type: "buy",
    symbol: symbol ?? INSTRUMENTS[0].symbol,
    date: today,
  };
  /** Field → message, filled when the user commits. */
  let errors = {};
  // The price starts at the market's and follows the symbol until the user edits it.
  let priceEdited = false;
  const priceText = () => market.price(draft.symbol).toFixed(2);

  const sharesInput = InputState.new({
    placeholder: "0",
    keyboard: "decimal",
    allowed_chars: "0123456789.",
    return_key: "next",
  });
  const priceInput = InputState.new({
    value: priceText(),
    placeholder: "0.00",
    keyboard: "decimal",
    allowed_chars: "0123456789.",
    return_key: "done",
  });

  const number = (state) => Number(state.value().trim());
  const complete = () => sharesInput.value().trim() !== "" && priceInput.value().trim() !== "";
  const dirty = () => sharesInput.value().trim() !== "" || priceEdited || draft.date !== today || draft.type !== "buy";

  const held = () => sharesOn(transactions(), draft.symbol, draft.date);
  const sellable = () => INSTRUMENTS.filter((i) => sharesOn(transactions(), i.symbol, today) > 0);

  const validate = () => {
    const next = {};
    const s = sharesInput.value().trim();
    const p = priceInput.value().trim();
    if (!SHARES.test(s) || Number(s) <= 0) next["form-shares"] = "Enter a number of shares, like 10 or 2.5.";
    else if (draft.type === "sell" && Number(s) > held() + 1e-9) {
      next["form-shares"] =
        held() > 0
          ? `You held ${formatShares(held())} ${draft.symbol} on ${longDate(draft.date)}.`
          : `You didn’t hold ${draft.symbol} on ${longDate(draft.date)}.`;
    }
    if (!PRICE.test(p) || Number(p) <= 0) next["form-price"] = "Enter the price per share, like 142.18.";
    if (draft.date > today) next["form-date"] = "Choose today or an earlier date.";
    else if (draft.date < earliestDate()) next["form-date"] = `Choose a date after ${longDate(earliestDate())}.`;
    return next;
  };

  const release = () => {
    sharesInput.release();
    priceInput.release();
  };
  const close = () => {
    release();
    window.close_sheet();
  };
  const commit = (cx) => {
    if (!complete()) return;
    errors = validate();
    if (Object.keys(errors).length > 0) {
      cx.haptic("error");
      cx.notify();
      return;
    }
    save({ ...draft, shares: number(sharesInput), price: number(priceInput) }, cx);
    close();
  };
  const cancel = (cx) => {
    if (!dirty()) return close();
    cx.alert({
      title: "Discard this transaction?",
      actions: [
        { label: "Keep editing", style: "cancel" },
        { label: "Discard", style: "destructive", on_press: () => close() },
      ],
    });
  };
  const edited = (field) => (_e, cx) => {
    delete errors[field];
    cx.notify();
  };

  sharesInput.on("change", edited("form-shares"));
  priceInput.on("change", (e, cx) => {
    priceEdited = true;
    edited("form-price")(e, cx);
  });
  sharesInput.on("submit", () => priceInput.focus());
  priceInput.on("submit", (_e, cx) => commit(cx));

  cx.open_sheet(
    () => {
      const t = cx.theme();
      const total =
        SHARES.test(sharesInput.value().trim()) && PRICE.test(priceInput.value().trim()) ? number(sharesInput) * number(priceInput) : null;
      const symbols = draft.type === "sell" ? sellable() : INSTRUMENTS;
      // At the largest text sizes a label and its value no longer fit side by side, so they stack.
      const stacked = cx.window().font_scale >= 1.5;
      const label = (text) =>
        div()
          .when(!stacked, (el) => el.w_24().flex_none())
          .text_base()
          .text_color(t.foreground)
          .child(text);
      // A field's message sits under it, inside the group, where the eye already is.
      const fieldRow = (id, text, control) =>
        v_flex()
          .child(
            (stacked ? v_flex().gap_1().items_stretch() : h_flex().gap_3())
              .px_4()
              .py_2()
              .min_h_12()
              .child(label(text))
              .child(div().flex_1().min_w_0().child(control)),
          )
          .when(Boolean(errors[id]), (el) =>
            el.child(div().test_id(`${id}-error`).role("alert").px_4().pb_3().text_sm().text_color(t.destructive).child(errors[id])),
          );
      const input = (state, id, a11y) =>
        Input.new(state)
          .test_id(id)
          .accessibility_label(a11y)
          .w_full()
          .py_2()
          .text_base()
          .when(!stacked, (el) => el.text_right())
          .text_color(errors[id] ? t.destructive : t.foreground);
      return (
        v_flex()
          .test_id("transaction-form")
          .size_full()
          // Sheets lay out at their content's height; this fills the detent with the grouped color.
          .min_h(cx.window().height)
          .bg(t.grouped_background)
          .line_height(1.375)
          .child(
            h_flex()
              .justify_between()
              .px_2()
              .py_2()
              .child(textButton(cx, "form-cancel", "Cancel", (_e, cx) => cancel(cx)))
              .child(
                div()
                  .flex_1()
                  .min_w_0()
                  .role("header")
                  .text_center()
                  .truncate()
                  .text_base()
                  .font_semibold()
                  .text_color(t.foreground)
                  .child("New transaction"),
              )
              .child(
                textButton(cx, "form-add", "Add", (_e, cx) => commit(cx), {
                  bold: true,
                  disabled: !complete(),
                }),
              ),
          )
          .child(
            v_flex()
              .flex_1()
              .overflow_y_scroll()
              .px_4()
              .pt_2()
              .pb(Math.max(cx.window().keyboard_height, cx.window().safe_area.bottom) + 24)
              .gap_6()
              .child(
                Tabs.new("form-type")
                  .test_id("form-type")
                  .accessibility_label("Transaction type")
                  .items(TYPES)
                  .selected(draft.type === "buy" ? 0 : 1)
                  .on_change((i, cx) => {
                    draft.type = i === 0 ? "buy" : "sell";
                    // A sale names something held; keep the choice when it is.
                    const options = draft.type === "sell" ? sellable() : INSTRUMENTS;
                    if (!options.some((o) => o.symbol === draft.symbol) && options.length > 0) draft.symbol = options[0].symbol;
                    if (!priceEdited) priceInput.set_value(priceText());
                    delete errors["form-shares"];
                    cx.haptic("selection");
                    cx.notify();
                  }),
              )
              .child(
                v_flex()
                  .rounded_xl()
                  .border_1()
                  .border_color(t.border)
                  .bg(t.surface)
                  .overflow_hidden()
                  .child(
                    fieldRow(
                      "form-symbol-row",
                      "Symbol",
                      h_flex()
                        .justify_end()
                        .when(stacked, (el) => el.justify_start())
                        .child(
                          Select.new("form-symbol")
                            .test_id("form-symbol")
                            // Long names truncate inside the field rather than pushing into its label.
                            .w_full()
                            .min_w_0()
                            .accessibility_label("Symbol")
                            .items(
                              symbols.map((i) => ({
                                label: `${i.symbol} · ${i.name}`,
                                value: i.symbol,
                              })),
                            )
                            .value(draft.symbol)
                            .on_change((value, cx) => {
                              draft.symbol = value;
                              if (!priceEdited) priceInput.set_value(priceText());
                              delete errors["form-shares"];
                              cx.notify();
                            }),
                        ),
                    ),
                  )
                  .child(hairline(t).ml_4())
                  .child(fieldRow("form-shares", "Shares", input(sharesInput, "form-shares", "Shares")))
                  .child(hairline(t).ml_4())
                  .child(fieldRow("form-price", "Price", input(priceInput, "form-price", "Price per share in dollars")))
                  .child(hairline(t).ml_4())
                  .child(
                    fieldRow(
                      "form-date",
                      "Date",
                      h_flex()
                        .justify_end()
                        .when(stacked, (el) => el.justify_start())
                        .child(
                          DatePicker.new("form-date")
                            .test_id("form-date")
                            .accessibility_label(`Date, ${longDate(draft.date)}`)
                            .mode("date")
                            .value(draft.date)
                            .on_change((value, cx) => {
                              // Pickers report a date, or a full timestamp; the date is the first ten characters.
                              draft.date = value.slice(0, 10);
                              delete errors["form-date"];
                              delete errors["form-shares"];
                              cx.notify();
                            }),
                        ),
                    ),
                  ),
              )
              .child(
                h_flex()
                  .test_id("form-total-row")
                  .justify_between()
                  .px_4()
                  .child(
                    div()
                      .text_base()
                      .font_semibold()
                      .text_color(t.foreground)
                      .child(draft.type === "buy" ? "Total cost" : "Total proceeds"),
                  )
                  .child(
                    div()
                      .test_id("form-total")
                      .text_base()
                      .font_semibold()
                      .text_color(t.foreground)
                      .child(total === null ? "—" : money(total)),
                  ),
              )
              .child(
                div()
                  .px_4()
                  .text_sm()
                  .text_color(t.muted_foreground)
                  .child(
                    draft.type === "sell"
                      ? `You hold ${formatShares(held())} ${draft.symbol} on this date. Sales reduce your cost at the average price.`
                      : "The price starts at the latest simulated price; change it to what you paid.",
                  ),
              ),
          )
      );
    },
    { detents: ["large"], dismissable: false },
  );
}
