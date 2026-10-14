# Components

Every control the shell renders, each shown in a small, believable task with
its states, rather than in a dump of widgets.

```bash
native-shell check examples/components
```

## Pages

| Page | Scenario | What it shows |
| --- | --- | --- |
| Buttons | A post composer | One filled primary commit (*Publish*), a gray alternative (*Save Draft*), a plain text exit (*Discard…* with confirmation), icon buttons with accessible names, a destructive commit set apart; rest, pressed, disabled and loading |
| Text Fields | A shipping address | An inset grouped form with visible labels, the right keyboard per field, Return to the next field, help text in the footer, validation in the footer on save, a Save that stays disabled until the form can succeed |
| Selection | Checkout preferences | Switches for settings that apply at once, checkmarks for independent choices and for one of a small set, an unavailable option kept visible with its reason; the whole row is the control |
| Sliders and Progress | Slideshow settings and an upload | The value written next to each slider; a spinner while the wait is unknown, then determinate progress with Cancel Upload; the finished state |
| Pickers | A table booking | A segmented control for two modes, menus for longer lists, the native date picker, and a footer sentence that reads the booking back |
| Forms | A flower delivery checkout | A stepper that shows the three steps and takes the buyer back; a stacked form of framed fields (required markers, descriptions, a phone prefix, a message counter that turns red past 150), the platform time picker, a ribbon color sheet; an inset grouped payment form whose errors appear under each row, a promo code that is checked (loading) and can be refused; the confirmation with a copyable order number, a rating that answers, and a read-only average |
| Charts | A health and money dashboard | `BarChart` for steps per day (week and month, labels thinned with `tick_margin`), an `AreaChart` of a portfolio pinned to its range with a loading state, a donut `PieChart` of spending with a tappable legend, a two-series `RadarChart` of language skills, and an empty chart; a tap reads a point's value, a second tap puts it away |
| Description List | An order's details | The status with its one action (a split button: *Track Package*, or another delivery option from its menu), then the order, items and delivery as label/value rows and a two-column grid; less frequent actions in a *More* menu; a loading state while the order arrives |
| Tree | A project's files | Folders that open in place with a turning disclosure chevron, the selected file's details, a path above that goes back to any folder, a toolbar that sorts (a pull-down menu), counts and adds a folder |
| Calendar | A cabin booking | Past and booked nights dimmed, a check-in to check-out range (one date for a day visit), and a summary that prompts for what is missing, explains a stay across a booked night, or prices the stay; *Request Booking* waits for valid dates |
| Carousel | A trip's photo album | Photos paged by swipe, arrows and dots, a caption under each, a star to keep it, and Favorites with its empty state |
| Settings | Notification preferences | Two pages on a segmented control, inset grouped sections with footers, and rows that stay visible but disabled, with the reason, while notifications are off |
| Overlays | A shared document | The smallest surface for each decision: a sheet for share targets, an alert-style dialog for a one-field rename, a toast for a result you can’t see (making it available offline), an alert for a deletion that can’t be undone |
| Status | A shop’s order screen | `gpui-component`’s feedback set in context: a status `Tag` and a banner `Alert` per delivery state (success, a dismissible warning, a declined payment with its fix), `Label` rows with muted quantities split by inset `Separator` hairlines, a `Notification` button confirmed by a toast, `Badge` counts on a tab bar (capped at 99+) and presence dots on avatars, an inbox emptied into an `Empty` state with Undo, `Skeleton` rows and `ShimmerText` while recommendations load, `Kbd` shortcuts for a hardware keyboard |

## Design notes

**Primary task.** Find how a control looks and behaves in context, then read
the page's code.

**Hierarchy.** The index is an inset grouped list (Actions, Input, Data,
Feedback) under a large title that collapses as it scrolls; each row has a colored icon
tile, names the component family and its scenario, and pushes a page with the
native transition, a navigation bar with "Components" as the back label, and
the edge swipe. Each page opens with one sentence saying what the scenario
demonstrates, then the scenario itself.

**Conventions shared by every page.** Touch targets are at least 44 pt; the
filled button (50 pt high, 12 pt radius) is used once per page, for the
default commit; labels are title case and name the result (`Publish`,
`Save Address`, `Request Booking`) and take an ellipsis when more input or a
confirmation follows (`Discard…`, `Rename…`). Commit buttons for forms sit in a
footer above the keyboard and the home indicator. Alerts name the object and
the verb, with Cancel first and the destructive action in red.

**States.** Every page shows its rest, pressed and disabled states; Buttons
and Sliders and Progress show loading; Text Fields shows validation errors;
Pickers shows the requested state; Overlays shows the deleted state;
Description List loads; Calendar prompts, explains a clash and requests;
Carousel has an empty Favorites; Settings disables what depends on a switch. All of
them use theme tokens only (the album's photos are illustrations in
`assets/photos`) and grow with Dynamic Type.
