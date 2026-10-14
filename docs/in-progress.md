# In progress

Who is working on what in shared areas (`crates/engine`, `crates/core`,
`examples/`, `docs/`), so two teams don't build the same thing. Add a row
before you start shared work and remove it when the work has landed. Check
this file (and `git log origin/main`) before starting anything shared.

| Area | What | Who | Since |
| --- | --- | --- | --- |
| `examples/{tasks,settings,watchlist,components,chat,notes}`, example apps' launchers | iOS-direction redesign (chat and notes keep their gpui-component parts): typography roles, inset grouped lists, iOS Button presets, `grouped_background`, removing stopgaps; final light/dark screenshots on iOS and Android | macOS team (agent `design`) | 2026-10-14 |
| `crates/core` (Paging prop), renderers, `crates/engine/js/gpui_component/carousel.js` | `Paging` (45) on horizontal Scroll; Carousel on a paged scroll instead of a pan. Engine, Carousel and Linux renderer done; still to implement in the iOS, Android and macOS renderers (until then a paged scroll is a plain horizontal scroll) | open for the macOS team | 2026-10-14 |
