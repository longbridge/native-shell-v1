// The note: read it rendered, or edit its Markdown.
//
// A note opens rendered, where checklist items are tapped off in place; Edit
// (or the checklist button) switches to its text, which grows with what is
// written. The first line is the title. There is no Save: every change is
// kept as it is typed, and a note emptied of text is discarded, as nothing
// worth keeping is left.

import { View, div, h_flex, v_flex, Button, Checkbox, Textarea, TextareaState, TextView } from "native-shell";
import { library, titleOf, toggleItem, plain, CHECK_ITEM } from "./store.js";
import { longDate } from "./dates.js";
import { actionSheet, barButton, font, icon, navBar, saveError, toolbar } from "./ui.js";
import { moveSheet, shareSheet } from "./dialogs.js";
import { deleteNotes } from "./rows.js";
import { shortcuts } from "./keys.js";

export default class EditorPage extends View {
  /** @param {{ id?: string, folder?: string, back: string }} props */
  init(props, cx) {
    this.back = props.back;
    this.note = (props.id && library.note(props.id)) || library.draft(props.folder ?? "notes");
    const fresh = this.note.body === "";
    this.editing = fresh;
    this.focused = false;
    this.body = TextareaState.new({ value: this.note.body, placeholder: "Start writing. The first line is the title.", auto_capitalize: "sentences" });
    this.body.set_auto_grow(12, 10000);
    this.body.on("change", (_e, cx) => this.edit(this.body.value(), cx));
    this.body.on("focus", (_e, cx) => {
      this.focused = true;
      cx.notify();
    });
    this.body.on("blur", (_e, cx) => {
      this.focused = false;
      cx.notify();
    });
    library.watch(cx.entity());
    if (fresh) cx.timer.after(0, () => this.body.focus());
  }

  /** Keeps the text as it is: added on the first character, discarded when emptied. */
  edit(body, cx) {
    const stored = library.note(this.note.id) !== null;
    if (body.trim() === "") {
      this.note = { ...this.note, body };
      if (stored) library.remove([this.note.id], cx);
      cx.notify();
      return;
    }
    this.note = library.put({ ...(library.note(this.note.id) ?? this.note), body }, cx);
  }

  toggleItem(line, cx) {
    const body = toggleItem(this.note.body, line);
    this.body.set_value(body);
    cx.haptic("selection");
    this.edit(body, cx);
  }

  /** Starts a checklist item on the line after the cursor's (or on it, when it's empty). */
  addItem(cx) {
    const value = this.body.value();
    const at = this.editing ? this.body.selection()[1] : value.length;
    const start = value.lastIndexOf("\n", at - 1) + 1;
    const end = value.indexOf("\n", at) === -1 ? value.length : value.indexOf("\n", at);
    const line = value.slice(start, end);
    const next =
      line.trim() === ""
        ? `${value.slice(0, start)}- [ ] ${value.slice(end)}`
        : `${value.slice(0, end)}\n- [ ] ${value.slice(end)}`;
    this.editing = true;
    this.body.set_value(next);
    this.edit(next, cx);
    this.body.focus();
  }

  setEditing(editing, cx) {
    this.editing = editing;
    if (editing) this.body.focus();
    else this.body.blur();
    cx.notify();
  }

  compose(cx) {
    cx.navigator.push(EditorPage, { folder: this.note.folder, back: this.back }, { title: "New Note" });
  }

  menu(cx) {
    const note = this.note;
    const title = titleOf(note.body);
    actionSheet(cx, {
      id: "editor-menu",
      title,
      actions: [
        { id: "menu-pin", label: note.pinned ? "Unpin Note" : "Pin Note", icon: "pin", run: (cx) => library.patch([note.id], { pinned: !note.pinned }, cx) },
        { id: "menu-move", label: "Move…", icon: "move", run: (cx) => moveSheet(cx, { ids: [note.id], title: `Move “${title}”` }) },
        {
          id: "menu-delete",
          label: "Delete",
          icon: "trash",
          destructive: true,
          run: (cx) => {
            cx.navigator.pop();
            // The toast's Undo outlives this page, so the delete runs owned by none.
            cx.timer.after(0, (cx) => deleteNotes(cx, [note.id]), { owner: null });
          },
        },
      ],
    });
  }

  render(cx) {
    const t = cx.theme();
    // The library may have changed under the page (pinned or moved from a menu).
    this.note = library.note(this.note.id) ?? this.note;
    const stored = library.note(this.note.id) !== null;
    const empty = this.note.body.trim() === "";
    return v_flex()
      .test_id("editor")
      .size_full()
      .bg(t.background)
      .key_context("Editor")
      .map(shortcuts({ new_note: (cx) => this.compose(cx) }))
      .child(
        navBar(cx, {
          id: "editor",
          plain: true,
          back: { label: this.back, on_click: (_e, cx) => cx.navigator.pop() },
          trailing: [
            barButton(cx, { id: "share", label: "Share", icon: "share", disabled: empty, on_click: (_e, cx) => shareSheet(cx, this.note) }),
            barButton(cx, { id: "more", label: "More", icon: "more", disabled: !stored, on_click: (_e, cx) => this.menu(cx) }),
            this.focused ? barButton(cx, { id: "done", label: "Done", emphasized: true, on_click: (_e, cx) => this.setEditing(false, cx) }) : null,
          ],
        }),
      )
      .when(library.saveFailed, (el) => el.child(saveError(cx)))
      .child(
        v_flex()
          .id("note-scroll")
          .test_id("note-scroll")
          .flex_1()
          .overflow_y_scroll()
          .child(
            h_flex()
              .justify_center()
              .gap_1()
              .pt_1()
              .pb_2()
              .when(this.note.pinned, (el) => el.child(icon("pin", t.muted_foreground, 12)))
              .child(div().test_id("edited").map(font(cx, "caption1")).text_color(t.muted_foreground).child(longDate(this.note.updated))),
          )
          .child(this.editing || empty ? this.textArea(cx) : this.rendered(cx)),
      )
      .child(
        toolbar(cx, {
          id: "editor",
          leading: barButton(cx, { id: "add-item", label: "Add checklist item", icon: "checklist", on_click: (_e, cx) => this.addItem(cx) }),
          center: barButton(cx, {
            id: "toggle-preview",
            label: this.editing ? "Preview" : "Edit",
            icon: this.editing ? "eye" : "pencil",
            disabled: empty,
            on_click: (_e, cx) => this.setEditing(!this.editing, cx),
          }),
          trailing: barButton(cx, { id: "editor-compose", label: "New Note", icon: "compose", tooltip: "New Note (⌘N)", on_click: (_e, cx) => this.compose(cx) }),
        }),
      );
  }

  textArea(cx) {
    const t = cx.theme();
    return Textarea.new(this.body)
      .test_id("body")
      .accessibility_label("Note")
      .mx_4()
      .mb_8()
      .map(font(cx, "body"))
      .line_height(1.35)
      .text_color(t.foreground);
  }

  /**
   * The note as Markdown, with its title as a heading and each checklist run
   * as rows that can be ticked off.
   */
  rendered(cx) {
    const t = cx.theme();
    const blocks = [];
    for (const segment of segments(this.note.body)) {
      if (segment.items) {
        blocks.push(
          v_flex()
            .test_id(`checklist-${segment.start}`)
            .children(segment.items.map((item) => this.checklistItem(cx, item))),
        );
      } else {
        blocks.push(
          TextView.markdown(`text-${segment.start}`, segment.markdown)
            .test_id(`text-${segment.start}`)
            .map(font(cx, "body"))
            .text_color(t.foreground),
        );
      }
    }
    return Button.new("rendered")
      .test_id("rendered")
      .accessibility_hint("Double-tap to edit")
      .flex()
      .flex_col()
      .gap_3()
      .px_4()
      .pb_8()
      .on_click((_e, cx) => this.setEditing(true, cx))
      .children(blocks);
  }

  checklistItem(cx, { line, text, done }) {
    const t = cx.theme();
    // A Checkbox with children is drawn by the script; the whole row is its target.
    return Checkbox.new(`item-${line}`)
      .test_id(`item-${line}`)
      .accessibility_label(text)
      .checked(done)
      .flex()
      .items_start()
      .gap_3()
      .py_1p5()
      .active((s) => s.opacity(0.6))
      .on_change((_done, cx) => this.toggleItem(line, cx))
      .child(
        div()
          .size_6()
          .mt_px()
          .flex_none()
          .flex()
          .items_center()
          .justify_center()
          .rounded_full()
          .border_2()
          .border_color(done ? t.primary : t.muted_foreground)
          .when(done, (el) => el.bg(t.primary))
          .child(done ? icon("check", t.primary_foreground, 14) : div()),
      )
      .child(
        div()
          .flex_1()
          .map(font(cx, "body"))
          .text_color(done ? t.muted_foreground : t.foreground)
          .when(done, (el) => el.line_through())
          .child(text),
      );
  }
}

/**
 * The body cut into Markdown runs and checklist runs, each knowing the line
 * it starts on. The first line reads as the title even without a `#`.
 * @param {string} body
 * @returns {({ start: number, markdown: string, items?: undefined } | { start: number, markdown?: undefined, items: { line: number, text: string, done: boolean }[] })[]}
 */
function segments(body) {
  const lines = body.split("\n");
  const out = [];
  let i = 0;
  while (i < lines.length) {
    const start = i;
    if (CHECK_ITEM.test(lines[i])) {
      const items = [];
      for (; i < lines.length && CHECK_ITEM.test(lines[i]); i++) items.push({ line: i, text: plain(lines[i]), done: /\[[xX]\]/.test(lines[i]) });
      out.push({ start, items });
    } else {
      const run = [];
      for (; i < lines.length && !CHECK_ITEM.test(lines[i]); i++) run.push(lines[i]);
      if (start === 0 && run[0].trim() !== "" && !/^\s*#/.test(run[0])) run[0] = `# ${run[0]}`;
      const markdown = run.join("\n").trim();
      if (markdown !== "") out.push({ start, markdown });
    }
  }
  return out;
}
