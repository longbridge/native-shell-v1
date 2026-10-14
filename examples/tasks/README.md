# Tasks

A personal task list: capture what needs doing, tick it off, fix it later.

```bash
nshell check examples/tasks
```

## Design notes

The screen follows iOS: a plain list under a left-aligned large title, an
edit sheet with Cancel and Save in its bar, and an alert only where a choice
cannot be undone in place.

**Primary task.** Add a task and complete it. Everything else (editing,
due dates, deleting) is secondary and one tap deeper.

**Object.** A task: a title, done or not, and an optional due date. Tasks live
on the device (`localStorage`); the first run shows a few sample tasks so the
screen explains itself, and any saved state, an empty list included, wins
afterwards.

**Immediately available.**

- *Add*: the field at the bottom of the screen, where the thumb rests, always
  above the keyboard. Return adds the task and keeps the keyboard up, so a list
  can be typed in one go; the arrow button does the same for touch and is
  disabled while the field is empty.
- *Complete*: the circle at the start of each row (a 44 pt target through
  `hit_slop`), with a success haptic.
- *Edit*: a tap anywhere else on the row opens the edit sheet.

**Information for a decision.** How many tasks are open and how many are
overdue (under the title), each task's due date in relative words near today
("Today", "Tomorrow", "Overdue · Oct 12"). Overdue uses the destructive color
*and* an alert icon and the word "Overdue", so it never relies on color.

**Hierarchy.** The large title names the screen; the list takes the remaining
height; completed tasks collapse behind a "Completed" disclosure so the open
ones lead. Rows are padded, not fixed-height, and separators are inset to the
text so the circles read as one column.

**Destructive actions.**

- *Delete Task* (edit sheet) removes it at once and shows a toast with Undo
  for five seconds. Reversible work gets undo, not a confirmation.
- *Clear…* completed asks first, naming the count
  (`Delete 3 completed tasks?` with Cancel and a red Delete), and the toast
  still offers Undo.

**States.**

| State | Treatment |
| --- | --- |
| Empty | Icon, "No tasks", and where to add one |
| All done | The status reads "All done"; completed tasks stay behind the disclosure |
| Long content | Titles wrap; nothing truncates |
| Large text | Rows, the title and the composer grow with Dynamic Type |
| Dark mode | Theme tokens only; designed for both appearances |
| Save failure | An inline alert above the list: the change is kept in memory and will be lost on close |
| Loading | None: storage is synchronous, so there is no spinner to fake |

**Interaction path.** Field → Return (adds, stays focused) → tap a circle to
complete → tap a row → sheet: name field (Return saves), *Due Date* switch,
date picker, *Delete Task*. Cancel, Save or a swipe down dismisses the sheet;
Save is disabled while the name is empty.
