# In progress

Who is working on what in shared areas (`crates/engine`, `crates/core`,
`examples/`, `docs/`), so two teams don't build the same thing. Add a row
before you start shared work and remove it when the work has landed. Check
this file (and `git log origin/main`) before starting anything shared.

| Area | What | Who | Since |
| --- | --- | --- | --- |
| `examples/{tasks,settings,watchlist,components,chat,notes}`, example apps' launchers | iOS-direction redesign landed for tasks, settings, watchlist, components and chat; paused (not started: notes review, Android launcher as an inset-grouped list, final reviewed light/dark screenshot pass on iOS and Android) | macOS team, paused | 2026-10-14 |
| `crates/core` (Paging prop), renderers, `crates/engine/js/gpui_component/carousel.js` | `Paging` (45) on horizontal Scroll; Carousel on a paged scroll instead of a pan. Engine, Carousel and Linux renderer done; still to implement in the iOS, Android and macOS renderers (until then a paged scroll is a plain horizontal scroll) | open for the macOS team | 2026-10-14 |
| `crates/core` (Resize event), `crates/engine`, `crates/tree` | Sheet content fills the sheet (finding 27): contract, tree, engine, macOS and iOS renderers landed (iOS suite not rerun); Android and Linux renderers still to send `Resize` | macOS team, paused | 2026-10-15 |
