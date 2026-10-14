// One folder's notes (or every note, or the notes with one tag).
//
// Pinned notes lead; the rest are grouped by when they were last edited, the
// way people remember them ("yesterday", "last month"). The search field
// narrows the list in place and marks what matched. A long press opens a
// note's menu; Select makes the rows a selection to move or delete together.

import { View, v_flex, InputState } from "native-shell";
import { library, ALL_NOTES } from "./store.js";
import { period } from "./dates.js";
import { barButton, emptyState, largeTitle, navBar, saveError, searchField, section, tintedButton, toolbar } from "./ui.js";
import { deleteNotes, noteRow } from "./rows.js";
import { moveSheet } from "./dialogs.js";
import { shortcuts } from "./keys.js";
import EditorPage from "./editor.js";
/** @import { Context } from "native-shell" */
/** @import { Note } from "./store.js" */

/** How far the large title scrolls before the bar takes over the title. */
const COLLAPSE_AT = 44;

export default class NotesPage extends View {
  /** @param {{ folder?: string, tag?: string }} props */
  init(props, cx) {
    this.folder = props.folder ?? null;
    this.tag = props.tag ?? null;
    this.collapsed = false;
    this.selecting = false;
    /** @type {Set<string>} */
    this.selected = new Set();
    this.search = InputState.new({ placeholder: "Search", return_key: "search", auto_capitalize: "none" });
    this.search.on("change", (_e, cx) => cx.notify());
    library.watch(cx.entity());
  }

  get title() {
    if (this.tag) return `#${this.tag}`;
    if (this.folder === ALL_NOTES) return "All Notes";
    return library.folder(this.folder)?.name ?? "Notes";
  }

  /** @returns {Note[]} */
  get notes() {
    return this.tag ? library.tagged(this.tag) : library.notesIn(this.folder);
  }

  open(note, cx) {
    this.search.blur();
    cx.navigator.push(EditorPage, { id: note.id, back: this.title }, { title: "" });
  }

  compose(cx) {
    cx.navigator.push(EditorPage, { folder: this.folder ?? ALL_NOTES, back: this.title }, { title: "New Note" });
  }

  setSelecting(on, cx) {
    this.selecting = on;
    this.selected.clear();
    cx.notify();
  }

  toggle(note, cx) {
    if (this.selected.has(note.id)) this.selected.delete(note.id);
    else this.selected.add(note.id);
    cx.haptic("selection");
    cx.notify();
  }

  onScroll(e, cx) {
    const collapsed = e.y > COLLAPSE_AT;
    if (collapsed === this.collapsed) return;
    this.collapsed = collapsed;
    cx.notify();
  }

  render(cx) {
    const notes = this.notes;
    const count = notes.length;
    const chosen = [...this.selected].filter((id) => library.note(id));
    return v_flex()
      .test_id("notes-page")
      .size_full()
      .bg(cx.theme().grouped_background)
      .key_context("Notes")
      .map(shortcuts({ new_note: (cx) => this.compose(cx), search: () => this.search.focus() }))
      .child(
        navBar(cx, {
          id: "notes",
          title: this.title,
          collapsed: this.collapsed,
          back: { label: "Folders", on_click: (_e, cx) => cx.navigator.pop() },
          trailing: [
            this.selecting
              ? barButton(cx, { id: "select", label: "Done", emphasized: true, on_click: (_e, cx) => this.setSelecting(false, cx) })
              : barButton(cx, { id: "select", label: "Select", disabled: count === 0, on_click: (_e, cx) => this.setSelecting(true, cx) }),
          ],
        }),
      )
      .when(library.saveFailed, (el) => el.child(saveError(cx)))
      .child(
        v_flex()
          .id("notes-scroll")
          .test_id("notes-scroll")
          .flex_1()
          .overflow_y_scroll()
          .on_scroll((e, cx) => this.onScroll(e, cx))
          .child(largeTitle(cx, this.title))
          // Nothing to search in an empty folder.
          .when(count > 0, (el) => el.child(searchField(cx, { id: "notes-search", state: this.search, label: `Search ${this.title}` })))
          .child(v_flex().pt_2().pb_8().gap_6().children(this.content(cx, notes))),
      )
      .child(
        this.selecting
          ? toolbar(cx, {
              id: "notes",
              leading: barButton(cx, {
                id: "move-selected",
                label: "Move",
                disabled: chosen.length === 0,
                on_click: (_e, cx) =>
                  moveSheet(cx, {
                    ids: chosen,
                    title: `Move ${chosen.length} ${chosen.length === 1 ? "Note" : "Notes"}`,
                    done: (cx) => this.setSelecting(false, cx),
                  }),
              }),
              status: chosen.length === 0 ? "Select Notes" : `${chosen.length} Selected`,
              trailing: barButton(cx, {
                id: "delete-selected",
                label: "Delete",
                destructive: true,
                disabled: chosen.length === 0,
                on_click: (_e, cx) => {
                  deleteNotes(cx, chosen);
                  this.setSelecting(false, cx);
                },
              }),
            })
          : toolbar(cx, {
              id: "notes",
              status: count === 1 ? "1 Note" : `${count} Notes`,
              trailing: barButton(cx, { id: "notes-compose", label: "New Note", icon: "compose", tooltip: "New Note (⌘N)", on_click: (_e, cx) => this.compose(cx) }),
            }),
      );
  }

  /** @param {Context} cx @param {Note[]} notes */
  content(cx, notes) {
    const query = this.search.value().trim();
    const showFolder = this.folder === ALL_NOTES || this.tag !== null;
    const row = (note) =>
      noteRow(cx, note, {
        query,
        showFolder,
        selecting: this.selecting,
        selected: this.selected.has(note.id),
        open: (cx) => this.open(note, cx),
        toggle: (cx) => this.toggle(note, cx),
      });

    if (query !== "") {
      const found = library.search(query, notes);
      if (found.length === 0) {
        return [emptyState(cx, { id: "no-results", icon: "search", title: "No Results", body: `Nothing in ${this.title} matches “${query}”.` })];
      }
      return [section(cx, { id: "results", title: found.length === 1 ? "1 Note Found" : `${found.length} Notes Found`, rows: found.map(row) })];
    }

    if (notes.length === 0) {
      return [
        emptyState(cx, {
          icon: "note",
          title: "No Notes",
          body: this.tag ? `No note carries #${this.tag} any more.` : "Notes you write in this folder appear here.",
          action: this.tag ? undefined : tintedButton(cx, { id: "empty-compose", label: "New Note", shortcut: "secondary-n", on_click: (_e, cx) => this.compose(cx) }),
        }),
      ];
    }

    /** @type {[string, Note[]][]} */
    const groups = [];
    const pinned = notes.filter((n) => n.pinned);
    if (pinned.length > 0) groups.push(["Pinned", pinned]);
    for (const note of notes.filter((n) => !n.pinned)) {
      const name = period(note.updated);
      const last = groups[groups.length - 1];
      if (last && last[0] === name) last[1].push(note);
      else groups.push([name, [note]]);
    }
    return groups.map(([name, members]) => section(cx, { id: `group-${name.toLowerCase().replace(/\s+/g, "-")}`, title: name, rows: members.map(row) }));
  }
}
