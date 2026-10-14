# Tasks

A personal task list: capture what needs doing, tick it off, fix it later.

```bash
native-shell check examples/tasks
```

## Design notes

**Primary task.** Add a task and complete it. Everything else (editing,
due dates, deleting) is secondary and one tap deeper.

**Object.** A task: a title, done or not, and an optional due date. Tasks live
on the device (`localStorage`); the first run shows a few sample tasks so the
screen explains itself, and any saved state, an empty list included, wins
afterwards.

**Immediately available.**

- *Add*: the field at the bottom of the screen, where the thumb rests, always
  above the keyboard. Return adds the task and keeps the keyboard up, so a list
  can be typed in one go; the arrow button does the same for touch.
- *Complete*: the circle at the start of each row (48 pt target via
  `hit_slop`), with a success haptic.
- *Edit*: a tap anywhere else on the row opens the edit sheet.

**Information for a decision.** How many tasks are open and how many are
overdue (the subtitle), each task's due date in relative words near today
("Today", "Tomorrow", "Overdue · Oct 12"). Overdue uses the destructive color
*and* an alert icon and the word "Overdue", so it never relies on color.

**Hierarchy.** The large title names the screen; the list takes the remaining
height; completed tasks collapse behind a "Completed · n" disclosure so the
open ones lead. Rows are padded, not fixed-height, and separators are inset to
the text column so the checkbox lane reads as one spine.

**Destructive actions.**

- *Delete task* (edit sheet) removes it at once and shows an Undo bar for five
  seconds. Reversible work gets undo, not a confirmation.
- *Clear…* completed opens an alert naming the count
  (`Delete 3 completed tasks?` with `Delete`), and is still undoable.

**States.**

| State | Treatment |
| --- | --- |
| Empty | Icon, "No tasks", and where to add one |
| All done | Subtitle reads "All done"; completed tasks stay behind the disclosure |
| Long content | Titles wrap; nothing truncates except the Undo label |
| Large text | Rows and the composer grow with the font scale |
| Save failure | An inline alert above the list: the change is kept in memory and will be lost on close |
| Loading | None: storage is synchronous, so there is no spinner to fake |

**Interaction path.** Field → Return (adds, stays focused) → tap a circle to
complete → tap a row → sheet: name field (Return saves), *Due date* switch,
date picker, *Delete task*. Cancel, Save, or a swipe down dismisses the sheet;
Save is disabled while the name is empty.
