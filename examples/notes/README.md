# Notes

Write things down and find them again: folders of Markdown notes with
checklists, tags and one search across everything.

```bash
native-shell check examples/notes
```

## Design notes

**Primary task.** Open a note and read or change it; start a new one from any
screen with the compose button (⌘N on a hardware keyboard). Finding a note
again (folders, tags, search) comes second; organizing (moving, renaming,
deleting) third, one long press away.

**Objects.** A *folder* holds notes; *Notes* is the folder every library has
and can't be renamed or deleted. A *note* is Markdown text: its first line is
its title, `- [ ]` lines are checklist items and `#words` are tags. The
library lives in one file on the device (`fs/promises`); a first run starts
with sample notes so every screen explains itself.

**Screens.**

| Screen | What it shows |
| --- | --- |
| Folders | Every folder with its note count, *All Notes*, the tags in use, a search across every note |
| Folder | Pinned notes, then the rest grouped by when they were last edited (Today, Yesterday, Previous 7 Days, by month); the large title collapses into the bar on scroll |
| Note | The note rendered (headings, lists, quotes, links), checklist items ticked off in place; Edit shows the text, which grows as it is written |

**Interaction path.** Folders → tap a folder → tap a note (rendered) → tick an
item, or Edit → type → Done. The checklist button starts an item under the
current line. A new note opens straight into editing with the keyboard up; a
note emptied of text is discarded, as there is nothing left to keep.

**Every row action has a visible way in.**

- A long press on a note opens its menu: Pin, Move…, Share…, Delete.
- *Select* turns the rows into a selection to move or delete several at once.
- A long press on a folder (or *Edit*, then a tap) opens Rename and Delete.

Swiping a row is not offered yet: a horizontal swipe needs a renderer gesture
that leaves the list's vertical scrolling alone.

**Destructive actions.**

- *Delete* on notes removes them at once and offers Undo on a toast for five
  seconds. Reversible work gets undo, not a question.
- *Delete Folder* takes its notes with it and can't be undone, so an alert
  names the folder and says how many notes go with it
  (`Delete “Recipes”?` / `Delete`).

**Search** matches every word of the query anywhere in a note, marks the
matches in the tint, and shows the line around the first match. The folder's
search narrows to that folder; the one on Folders searches everything and says
which folder each result is in.

**Sharing** copies the note, as plain text (checklist items as ○ and ✓) or as
Markdown, with a preview of what will be copied.

**States.**

| State | Treatment |
| --- | --- |
| Loading | A spinner under the title while the library is read |
| Empty folder | Icon, "No Notes", and where to start one; Select is disabled and the search field hidden |
| No results | "No Results", repeating the query |
| Read error | "Couldn’t Open Your Notes" with Try Again; the file is never overwritten |
| Save failure | An inline alert under the bar: changes stay in memory until the app closes |
| Long content | The planning note: sections, lists and a checklist; list items wrap |
| Large text | Rows, bars, icons and the checklist grow with the text size; titles wrap to two lines in lists |

**Keyboard.** ⌘N (Ctrl+N) writes a new note, ⇧⌘N makes a folder, ⌘F moves
to the search field. The toolbar buttons name their shortcut in a tooltip
where a pointer can rest on them.
