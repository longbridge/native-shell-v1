# Settings

The preferences screen of a typical app: notifications, appearance, privacy,
storage, about, and the account behind them.

```bash
nshell check examples/settings
```

## Design notes

Laid out as iOS Settings: a large title that collapses into a centered
navigation bar title as the list scrolls, inset grouped sections on the
grouped background, and a colored icon tile leading each row.

**Primary task.** Find a setting and change it. Every switch and picker
applies at once; there is no Save on this screen.

**Objects.** The account (name, email), a handful of preferences, and the
cache. Preferences persist in `localStorage`, one key at a time, so a corrupt
value falls back to its default without resetting the rest.

**Hierarchy.** The large title, then the account row (the one object with
identity, so it leads, with an avatar and a chevron), then sections ordered by
how often they are visited. Where a switch needs explaining, one sentence sits
in the section footer. Rows share one spine: the icon tile, the label, then the
value (secondary text) or control at the trailing edge; separators start at
the text.

**Immediately available.** Switches for Notifications, Daily Summary and Share
Usage Data; a time picker for the summary; a menu for Appearance (System,
Light, Dark, applied with `cx.set_theme` and handed back to the device for
System); *Help Center* opens the browser, so it carries the external-link
glyph instead of a chevron.

**Dependent settings.** The daily summary needs notifications. When they are
off it stays visible but disabled, and the section footer says why, instead
of disappearing.

**Drill-down.** The account row pushes the Account page: a navigation bar with
a centered title, "Settings" as the back label, and the edge swipe. Its fields
are an inset grouped form with the email keyboard, Return to the next field and
to save. Save is disabled until something changed; the email is validated on
save with the message in the section footer, in the destructive color. Leaving
with unsaved edits asks `Discard changes?` (Keep Editing / Discard Changes),
for the back button and the back gesture alike.

**Destructive actions.** *Clear Cache…* names the size and the consequence
(`Clear 184 MB of cached files?`, Clear Cache in red); *Sign Out…* names the
account. Both are alerts because neither can be undone from here.

**States.**

| State | Treatment |
| --- | --- |
| Measuring | Cached Files reads "Measuring…" and *Clear Cache…* is disabled until the size is known |
| Empty cache | "Zero KB", *Clear Cache…* disabled |
| Signed out | The account row becomes *Sign In*, and the page a sign-in form |
| Validation error | The message under the field in the destructive color, plus an error haptic |
| Large text | Rows grow; labels wrap; the trailing control keeps its place |
| Dark mode | Grouped background and cell colors from the theme, designed for both appearances |

**Interaction path.** Scroll (the title collapses) → toggle Notifications
(Daily Summary disables, footer explains) → Appearance menu → Clear Cache… →
alert → Account row → edit email → Return → Save, or back → `Discard
changes?`.
