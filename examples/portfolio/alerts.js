// Alerts: when the app speaks up about the portfolio, and the data behind it.
//
// Every change applies at once, as on any iOS settings page. Settings that
// depend on another stay visible but disabled, with the reason beneath them.
// Erasing the portfolio is the one destructive action, so it names what goes
// and asks first.

import { View, v_flex, Switch, Select, DatePicker } from "native-shell";
import { positions } from "./portfolio.js";
import { navBar, row, section } from "./ui.js";

const THRESHOLDS = [2, 3, 5, 10].map((n) => ({ label: `${n}%`, value: n }));

/** `17:00` → `5:00 PM`. */
function formatTime(hhmm) {
  const [h, m] = hhmm.split(":").map(Number);
  return `${((h + 11) % 12) + 1}:${String(m).padStart(2, "0")} ${h < 12 ? "AM" : "PM"}`;
}

export default class AlertsPage extends View {
  init(props) {
    /** @type {import("./main.js").default} */
    this.app = props.app;
  }

  set(patch, cx) {
    this.app.setSettings(patch, cx);
    cx.notify();
  }

  confirmErase(cx) {
    const count = this.app.transactions.length;
    cx.alert({
      title: `Erase ${count} ${count === 1 ? "transaction" : "transactions"}?`,
      message: "Your holdings and their history will be removed from this device. This can’t be undone.",
      actions: [
        { label: "Cancel", style: "cancel" },
        {
          label: "Erase",
          style: "destructive",
          on_press: (cx) => {
            this.app.erase(cx);
            cx.notify();
          },
        },
      ],
    });
  }

  render(cx) {
    const t = cx.theme();
    const s = this.app.settings;
    const held = positions(this.app.transactions, this.app.market).sort((a, b) => a.symbol.localeCompare(b.symbol));
    return v_flex()
      .size_full()
      .bg(t.grouped_background)
      .line_height(1.375)
      .child(
        navBar(cx, {
          title: "Alerts",
          back: {
            label: "Portfolio",
            on_click: (_e, cx) => cx.navigator.pop(),
          },
        }),
      )
      .child(
        v_flex()
          .test_id("alerts-page")
          .flex_1()
          .overflow_y_scroll()
          .px_4()
          .pt_6()
          .pb(cx.window().safe_area.bottom + 32)
          .gap_6()
          .children([
            section(cx, {
              title: "Price moves",
              footer: s.moveAlerts
                ? `You’ll get an alert the first time a holding moves ${s.threshold}% or more in a day.`
                : "Turn on to hear about big daily moves in what you hold.",
              rows: [
                row(cx, {
                  id: "move-alerts-row",
                  label: "Big daily moves",
                  trailing: Switch.new("move-alerts")
                    .test_id("move-alerts")
                    .accessibility_label("Big daily moves")
                    .checked(s.moveAlerts)
                    .on_change((on, cx) => this.set({ moveAlerts: on }, cx)),
                }),
                row(cx, {
                  id: "threshold-row",
                  label: "Move of at least",
                  disabled: !s.moveAlerts,
                  trailing: Select.new("threshold")
                    .test_id("threshold")
                    .accessibility_label("Move of at least")
                    .disabled(!s.moveAlerts)
                    .items(THRESHOLDS)
                    .value(s.threshold)
                    .on_change((value, cx) => this.set({ threshold: value }, cx)),
                }),
              ],
            }),
            held.length > 0
              ? section(cx, {
                  id: "alert-holdings",
                  title: "Holdings",
                  footer: s.moveAlerts ? "Turn off a holding to stop its alerts." : "Holdings follow the switch above.",
                  rows: held.map((p) =>
                    row(cx, {
                      id: `alert-${p.symbol}-row`,
                      label: p.symbol,
                      detail: p.name,
                      disabled: !s.moveAlerts,
                      trailing: Switch.new(`alert-${p.symbol}`)
                        .test_id(`alert-${p.symbol}`)
                        .accessibility_label(`Alerts for ${p.symbol}`)
                        .disabled(!s.moveAlerts)
                        .checked(s.moveAlerts && !s.muted.includes(p.symbol))
                        .on_change((on, cx) =>
                          this.set(
                            {
                              muted: on ? s.muted.filter((m) => m !== p.symbol) : [...s.muted, p.symbol],
                            },
                            cx,
                          ),
                        ),
                    }),
                  ),
                })
              : null,
            section(cx, {
              title: "Daily summary",
              footer: "A note after the close with the day’s change and your biggest movers.",
              rows: [
                row(cx, {
                  id: "summary-row",
                  label: "Daily summary",
                  trailing: Switch.new("summary")
                    .test_id("summary")
                    .accessibility_label("Daily summary")
                    .checked(s.summary)
                    .on_change((on, cx) => this.set({ summary: on }, cx)),
                }),
                s.summary
                  ? row(cx, {
                      id: "summary-time-row",
                      label: "Delivered at",
                      trailing: DatePicker.new("summary-time")
                        .test_id("summary-time")
                        .accessibility_label(`Delivered at ${formatTime(s.summaryTime)}`)
                        .min_w_32()
                        .mode("time")
                        .value(s.summaryTime)
                        // Pickers report `HH:mm`, or a full timestamp; the time is the last five characters either way.
                        .on_change((value, cx) => this.set({ summaryTime: value.slice(-5) }, cx)),
                    })
                  : null,
              ],
            }),
            section(cx, {
              title: "Data",
              footer: "Prices in this app are simulated. Turning the network off shows what happens without a connection.",
              rows: [
                row(cx, {
                  id: "offline-row",
                  label: "Simulate no connection",
                  trailing: Switch.new("simulate-offline")
                    .test_id("simulate-offline")
                    .accessibility_label("Simulate no connection")
                    .checked(s.offline)
                    .on_change((on, cx) => this.set({ offline: on }, cx)),
                }),
                row(cx, {
                  id: "erase",
                  label: "Erase portfolio…",
                  destructive: this.app.transactions.length > 0,
                  disabled: this.app.transactions.length === 0,
                  on_click: (_e, cx) => this.confirmErase(cx),
                }),
              ],
            }),
          ]),
      );
  }
}
