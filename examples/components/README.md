# Components

Every control the shell renders, each shown in a small, believable task with
its states, rather than in a dump of widgets.

```bash
nshell check examples/components
```

## Pages

| Page | Scenario | What it shows |
| --- | --- | --- |
| Buttons | A post composer | One filled primary commit (*Publish*), a gray alternative (*Save Draft*), a plain text exit (*Discard…* with confirmation), icon buttons with accessible names, a destructive commit set apart; rest, pressed, disabled and loading |
| Text Fields | A shipping address | An inset grouped form with visible labels, the right keyboard per field, Return to the next field, help text in the footer, validation in the footer on save, a Save that stays disabled until the form can succeed |
| Selection | Checkout preferences | Switches for settings that apply at once, checkmarks for independent choices and for one of a small set, an unavailable option kept visible with its reason; the whole row is the control |
| Sliders and Progress | Slideshow settings and an upload | The value written next to each slider; a spinner while the wait is unknown, then determinate progress with Cancel Upload; the finished state |
| Pickers | A table booking | A segmented control for two modes, menus for longer lists, the native date picker, and a footer sentence that reads the booking back |
| Charts | A health and money dashboard | `BarChart` for steps per day (week and month, labels thinned with `tick_margin`), an `AreaChart` of a portfolio pinned to its range with a loading state, a donut `PieChart` of spending with a tappable legend, a two-series `RadarChart` of language skills, and an empty chart; a tap reads a point's value, a second tap puts it away |
| Overlays | A shared document | The smallest surface for each decision: a sheet for share targets, an alert-style dialog for a one-field rename, a toast for a result you can’t see (making it available offline), an alert for a deletion that can’t be undone |

## Design notes

**Primary task.** Find how a control looks and behaves in context, then read
the page's code.

**Hierarchy.** The index is an inset grouped list (Actions, Input, Feedback)
under a large title that collapses as it scrolls; each row has a colored icon
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
Pickers shows the requested state; Overlays shows the deleted state. All of
them use theme tokens only and grow with Dynamic Type.
