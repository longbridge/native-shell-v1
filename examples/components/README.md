# Components

Every control the shell renders, each shown in a small, believable task with
its states, rather than in a dump of widgets.

```bash
native-shell check examples/components
```

## Pages

| Page | Scenario | What it shows |
| --- | --- | --- |
| Buttons | A post composer | One primary commit (*Publish*), a quieter alternative (*Save draft*), a low-emphasis exit (*Discard…* with confirmation), icon buttons with accessible names, a destructive commit set apart; rest, pressed, disabled and loading |
| Text fields | A shipping address | Visible labels, the right keyboard per field, Return to the next field, help text, validation under the field on save, a Save that stays disabled until the form can succeed |
| Selection controls | Checkout preferences | Switches for settings that apply at once, checkboxes for independent choices, radios for one of a small set, an unavailable option kept visible with its reason; the whole row is the control |
| Sliders and progress | Slideshow settings and an upload | The value written next to each slider; a spinner while the wait is unknown, then determinate progress with Cancel; the finished state |
| Pickers | A table booking | A segmented control for two modes, menus for longer lists, the native date picker, and a sentence that reads the booking back |
| Overlays | A shared document | The smallest surface for each decision: a sheet for share targets, a dialog for a one-field rename, a toast for a result you can’t see (making it available offline), an alert for a deletion that can’t be undone |

## Design notes

**Primary task.** Find how a control looks and behaves in context, then read
the page's code.

**Hierarchy.** The index is a grouped list (Actions, Input, Feedback); each
row names the component family and its scenario and pushes a page with the
native transition and back gesture. Each page opens with one sentence saying
what the scenario demonstrates, then the scenario itself.

**Conventions shared by every page.** Buttons are at least 44 pt tall with
body text; primary is used once per page, for the default commit; labels name
the result (`Publish`, `Save address`, `Request booking`) and take an ellipsis
when more input or a confirmation follows (`Discard…`, `Rename…`). Commit
buttons for forms sit in a footer above the keyboard and the home indicator.
