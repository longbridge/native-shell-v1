# In progress

Who is working on what in shared areas (`crates/engine`, `crates/core`,
`examples/`, `docs/`), so two teams don't build the same thing. Add a row
before you start shared work and remove it when the work has landed. Check
this file (and `git log origin/main`) before starting anything shared.

| Area | What | Who | Since |
| --- | --- | --- | --- |
| `crates/engine/js` (new module `gpui-component` exports) | Feedback components: `Alert` (+ Info/Success/Warning/Error), `Badge`, `Tag`, `Skeleton`, `Empty` (+ parts), `Kbd`, `Label`, `Separator` (+ dashed/vertical), `ShimmerText`, `Notification` | Linux team (agent `comp-feedback`) | 2026-10-14 |
| `crates/engine/js` | Form components: `Form` / `HForm` / `VForm` / `Field`, `InputGroup` (+ parts), `Rating`, `Stepper` (+ Item), `TimeField`, `Clipboard`, `ColorPicker` | Linux team (agent `comp-forms`) | 2026-10-14 |
| `crates/engine/js` | Data and navigation components: `DescriptionList`, `Tree`, `List`, `TabBar`, `Breadcrumb`, `Settings` (+ Page / Group / Item), `GroupBox`, `Calendar`, `Carousel` (+ parts), `Menu` / `DropdownButton`, `Toolbar`, `Icon` | Linux team (agent `comp-data`) | 2026-10-14 |
| `crates/engine/js` | Charts: `BarChart`, `LineChart`, `AreaChart`, `PieChart`, `RadarChart` | Linux team (agent `comp-charts`) | 2026-10-14 |
| `crates/engine/js` | Conversation components: `Message`, `Bubble`, `Attachment`, `Marker`, `MessageScroller` | Linux team (agent `comp-chat`) | 2026-10-14 |
| `examples/chat` (+ its harness test in `crates/engine/tests/examples.rs`) | Move the chat example onto `Message` / `Bubble` / `Attachment` / `Marker` / `MessageScroller` and `list(...).alignment("bottom")`; attachments, unread marker, longer thread | Linux team (agent `comp-chat`) | 2026-10-14 |
| `examples/` | New example programs: `shop` (catalog, product, cart, checkout), `notes` (folders, markdown, search), `portfolio` (charts, holdings, transactions), `events` (calendar, booking form) | Linux team (agents `ex-*`) | 2026-10-14 |
