# Watchlist

Live prices for the instruments a user follows, built on the host app's own
modules and native views.

```bash
nshell check examples/watchlist --registry examples/watchlist/host.json
```

## How the host and the script divide the work

| Piece | Owner |
| --- | --- |
| `demo/quote` — `quote(symbol)`, `subscribe(symbols)`, `unsubscribe(symbols)`, event `tick` | Host module (the app's market-data client) |
| `demo/user` — `current_user()` | Host module, synchronous |
| `demo-stock-cell` — one quote row | Host component: the app's existing native cell, measurable, emits `select` |
| Header, sort, live/paused, detail sheet, add/remove, empty and offline states | The script |

`host.json` declares the same surface for `nshell check` and
`nshell types`. The example apps implement it in
`platforms/ios/Example/Sources/HostDemo.swift` and
`platforms/android/example/.../DemoApplication.kt` and `StockCell.kt`. The
cell draws with the app's own palette, which also supplies the shell theme's
accent, so the native and scripted parts match in light and dark.

## Design notes

An iOS plain list under a large title, with the add button in the title's
trailing position and secondary tasks in sheets with a grabber.

**Primary task.** Scan how the followed instruments are doing right now.

**Object.** A list of instruments; each shows symbol, name, last price and the
day's change. Prices are right-aligned with tabular digits; the change carries
its sign, so gain and loss do not depend on color.

**Immediately available.** Sort (Symbol, Change, Price) as a segmented
control, because the choice is small and should stay visible; *Live / Paused*
as one tinted capsule that shows the feed's state and toggles it (pausing drops
the tick listener, so the host can stop the upstream feed); *Add Symbols* as
the plus button beside the title, with an accessible name.

**Detail.** A tap on a row opens a medium sheet with the price as a large
number, the day's change, an inset grouped section with the previous close and
whether updates are live, and *Remove from Watchlist*. Removing is reversible,
so it shows a toast with Undo instead of asking first.

**States.**

| State | Treatment |
| --- | --- |
| Connecting | A spinner in the live capsule; cells show "—" until the first quote |
| Offline | The first quotes failed: "Prices Aren’t Available" and *Try Again* |
| Paused | A neutral dot and "Paused"; prices stop moving |
| Empty | "No Symbols" and *Add Symbols…* |
| Everything added | The add sheet says so instead of showing an empty list |
| Large text | The native cell measures itself with the user's font scale |
| Dark mode | The host cell's palette and the shell theme share one accent in both appearances |

**Interaction path.** Scan → Sort: Change → tap the capsule to pause and
resume → tap a row → sheet → Remove from Watchlist → toast Undo → plus button
→ Add Symbols sheet → tap a symbol.
