# Settings

The preferences screen of a typical app: notifications, appearance, privacy,
storage, about, and the account behind them.

```bash
native-shell check examples/settings
```

## Design notes

**Primary task.** Find a setting and change it. Every switch and picker
applies at once; there is no Save on this screen.

**Objects.** The account (name, email), a handful of preferences, and the
cache. Preferences persist in `localStorage`, one key at a time, so a corrupt
value falls back to its default without resetting the rest.

**Hierarchy.** A large title, then the account card (the one object with
identity, so it leads and drills down), then groups ordered by how often they
are visited. Each group has a short sentence-case label above and, where a
switch needs explaining, one sentence below. Rows share one spine: an icon
lane, the label, then the value or control at the trailing edge; separators
start at the text column.

**Immediately available.** Switches for notifications, the daily summary and
usage data; a time picker for the summary; a menu for the theme (System,
Light, Dark, applied with `cx.set_theme` and handed back to the device for
System); *Help center* opens the browser (an external resource, so it carries
the external-link glyph).

**Dependent settings.** The daily summary needs notifications. When they are
off it stays visible but disabled, and the group's footer says why, instead
of disappearing.

**Drill-down.** The account card pushes the Account page (native transition
and back gesture). Its fields have visible labels, the email keyboard, Return
to the next field and to save. Save is disabled until something changed;
the email is validated on save with the message under the field. Leaving
with unsaved edits asks `Discard changes?` (`Keep editing` / `Discard
changes`), for the back button and the system back gesture alike.

**Destructive actions.** *Clear cache…* names the size and the consequence
(`Clear 184 MB of cached files?`, `Clear cache`); *Sign out…* names the
account. Both are alerts because neither can be undone from here.

**States.**

| State | Treatment |
| --- | --- |
| Measuring | The cache row reads "Measuring…" and *Clear cache…* is disabled until the size is known |
| Empty cache | "Zero KB", *Clear cache…* disabled |
| Signed out | The account card becomes *Sign in*, and the page becomes a sign-in form |
| Validation error | Red field border and the message under the email field, plus an error haptic |
| Large text | Rows grow; labels wrap; the trailing control keeps its lane |
