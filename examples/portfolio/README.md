# Portfolio

A personal investment portfolio: what it's worth, how today went, what it's
made of, and the transactions behind it.

```bash
native-shell check examples/portfolio
```

Instruments and prices are fictional and simulated (`market.js`): each
instrument's history is a random walk seeded by its symbol and counted in
trading days back from today, so every device and every test run sees the
same totals and charts. Nothing is fetched; the "network" is a short delay.

| File | What |
| --- | --- |
| `main.js` | The overview: total, day change, value chart, allocation, holdings |
| `holding.js` | One holding: price chart, position, statistics, transactions |
| `transaction.js` | The sheet that records a purchase or a sale |
| `alerts.js` | Alert settings, the simulated connection, erasing the data |
| `chart.js` | The range picker and area chart both pages use |
| `market.js` | Fictional instruments and their simulated prices |
| `portfolio.js` | Transactions on the device, positions and returns, formatting |
| `ui.js` | Icons, bars, grouped rows and states |

## Design notes

**Primary task.** See what the portfolio is worth and how today went. The
total and the day's change are the first thing on the screen; everything
else is below or one tap deeper.

**Object.** Transactions (buy or sell, symbol, shares, price, date) are the
only record, kept on the device. Positions, cost (average-cost method),
returns, the allocation and every chart are derived from them, so adding or
undoing one changes the whole screen consistently. A first run starts with a
believable year of sample transactions.

**Immediately available.**

- *Range*: 1D / 1W / 1M / 1Y as a segmented control above the chart; the
  caption under the chart says what the line means in words (the previous
  close on 1D, the range's gain or loss otherwise).
- *Sort holdings*: a menu in the section header (value, today's change,
  total return, symbol). The trailing number of each row follows the sort:
  total return when sorting by it, today's change otherwise.
- *Add transaction*: the + in the header (and on a holding's page, with its
  symbol chosen).
- *Alerts*: the bell.
- *Refresh*: pull down.

**Gain and loss.** Gains use the success color and losses the destructive
one, and every change carries its sign (`+$412.80`, `−$96.10`, with a true
minus), so meaning never depends on color.

**Live prices.** Prices move every two seconds while the app is in front.
The runtime pauses timers while the app is in the background, so the feed
stops there with no extra code. Offline, prices stop moving and say so.

**Price alerts.** A holding that crosses the chosen daily move (2, 3, 5 or
10%) is announced once with a toast. Holdings can be muted one by one; with
alerts off, the dependent settings stay visible but disabled.

**The transaction form.** Type (Buy / Sell), symbol, shares and price
(decimal keyboards), and date (the native date picker). The price starts at
the latest price and follows the symbol until the user edits it. Shares
Return moves to price; price Return adds. Each field checks itself when the
user commits and says what it accepts, under the field: a number of shares,
a price, a date no later than today and within the simulated history, and a
sale no larger than what was held on its date. The total updates as the user
types (*Total cost* / *Total proceeds*), so the amount is confirmed before
*Add*. The sheet closes with *Cancel* or *Add* only, so a draft is never lost
to a swipe; *Cancel* with a draft asks *Discard this transaction?*.

**Destructive actions.**

- *Add* is confirmed by a toast with *Undo*.
- Deleting a transaction (tap it on the holding's page) asks with the
  transaction named (`Delete “Sell 5 NOVA”?`), then offers *Undo*.
- *Erase portfolio…* names the count (`Erase 9 transactions?`) and can't be
  undone, so it says so.

**States.**

| State | Treatment |
| --- | --- |
| Loading | A spinner and "Getting the latest prices…" for the first round trip |
| Offline, first load | Full screen: "Prices aren't available", *Try again* |
| Offline, later | A banner above the total ("You're offline · Prices may be out of date") with *Retry*; the last prices stay |
| Refreshing | The system pull-to-refresh indicator |
| Empty | "No holdings yet" and *Add transaction* |
| Closed position | A holding sold to zero leaves the list; its page says so and keeps the transactions |
| Save failure | An inline message: changes last until the app closes |
| Large text | Rows grow with the text; names truncate before numbers do |

The connection can be turned off on the alerts page (*Simulate no
connection*), to see the offline states on a device.
