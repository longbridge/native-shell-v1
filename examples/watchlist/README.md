# Watchlist

Live prices for the instruments a user follows, built on the host app's own
modules and native views.

```bash
native-shell check examples/watchlist --registry examples/watchlist/host.json
```

## How the host and the script divide the work

| Piece | Owner |
| --- | --- |
| `demo/quote` — `quote(symbol)`, `subscribe(symbols)`, `unsubscribe(symbols)`, event `tick` | Host module (the app's market-data client) |
| `demo/user` — `current_user()` | Host module, synchronous |
| `demo-stock-cell` — one quote row | Host component: the app's existing native cell, measurable, emits `select` |
| Header, sort, live/paused, detail sheet, add/remove, empty and offline states | The script |

`host.json` declares the same surface for `native-shell check` and
`native-shell types`. The example apps implement it in
`platforms/ios/Example/Sources/HostDemo.swift` and
`platforms/android/example/.../DemoApplication.kt` and `StockCell.kt`. The
cell draws with the app's own palette, which also supplies the shell theme's
accent, so the native and scripted parts match in light and dark.

## Design notes

**Primary task.** Scan how the followed instruments are doing right now.

**Object.** A list of instruments; each shows symbol, name, last price and the
day's change. Prices are right-aligned with tabular digits; the change carries
its sign, so gain and loss do not depend on color.

**Immediately available.** Sort (Symbol, Change, Price) as a segmented
control, because the choice is small and should stay visible; *Live / Paused*
as one button that shows the feed's state and toggles it (pausing drops the
tick listener, so the host can stop the upstream feed); *Add symbols* as an
icon button with an accessible name.

**Detail.** A tap on a row opens a sheet with the price, the day's change, the
previous close, whether updates are live, and *Remove from watchlist*.
Removing is reversible, so it offers Undo instead of asking first.

**States.**

| State | Treatment |
| --- | --- |
| Connecting | A spinner in the live button; cells show "—" until the first quote |
| Offline | The first quotes failed: an explanation and *Try again* |
| Paused | A neutral dot and "Paused"; prices stop moving |
| Empty | "No symbols yet" and *Add symbols…* |
| Everything added | The add sheet says so instead of showing an empty list |
| Large text | The native cell measures itself with the user's font scale |
