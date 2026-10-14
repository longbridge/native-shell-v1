// A note as a list row, and what can be done to one from a list.
//
// Every row action is reachable two ways: a long press opens the note's
// menu, and Select (in the bar) turns the rows into a selection for moving
// and deleting several at once.

import { div, h_flex, v_flex, Button } from "native-shell";
import { library, plain, previewOf, titleOf, terms } from "./store.js";
import { rowDate } from "./dates.js";
import { actionSheet, font, highlighted, icon } from "./ui.js";
import { moveSheet, shareSheet } from "./dialogs.js";
/** @import { Context } from "native-shell" */
/** @import { Note } from "./store.js" */

const UNDO_MS = 5000;

/**
 * @param {Context} cx
 * @param {Note} note
 * @param {{ query?: string, showFolder?: boolean, selecting?: boolean, selected?: boolean, open: (cx: Context) => void, toggle?: (cx: Context) => void }} opts
 */
export function noteRow(cx, note, { query = "", showFolder = false, selecting = false, selected = false, open, toggle }) {
  const t = cx.theme();
  const words = terms(query);
  const title = titleOf(note.body);
  const detail = words.length > 0 ? snippet(note.body, words) : previewOf(note.body);
  const folder = showFolder ? library.folder(note.folder)?.name : null;
  return Button.new(`note-${note.id}`)
    .test_id(`note-${note.id}`)
    .accessibility_label(`${title}, ${rowDate(note.updated)}, ${detail}${note.pinned ? ", pinned" : ""}`)
    .accessibility_hint(selecting ? "Selects the note" : "Opens the note. Long press for actions.")
    .when(selecting, (el) => el.aria_selected(selected))
    .flex()
    .items_center()
    .gap_3()
    .pl_4()
    .pr_4()
    .py_2p5()
    .min_h_11()
    .active((s) => s.bg(t.muted))
    .on_click((_e, cx) => (selecting ? toggle?.(cx) : open(cx)))
    .when(!selecting, (el) => el.on_long_press((_e, cx) => noteMenu(cx, note)))
    .when(selecting, (el) => el.child(selectionMark(cx, `select-${note.id}`, selected)))
    .child(
      v_flex()
        .flex_1()
        .min_w_0()
        .gap_0p5()
        .child(
          div()
            .test_id(`title-${note.id}`)
            .map(font(cx, "headline"))
            .text_color(t.foreground)
            .line_clamp(2)
            .child(words.length > 0 ? highlighted(title, words, t.primary) : title),
        )
        .child(
          // At accessibility text sizes the date takes its own line, leaving the preview room to read.
          (cx.window().font_scale >= 1.5 ? v_flex().gap_0p5() : h_flex().items_start().gap_2())
            .map(font(cx, "subheadline"))
            .child(div().flex_none().text_color(t.foreground).child(rowDate(note.updated)))
            .child(
              div()
                .test_id(`detail-${note.id}`)
                .flex_1()
                .min_w_0()
                .text_color(t.muted_foreground)
                .line_clamp(words.length > 0 ? 2 : 1)
                .child(words.length > 0 ? highlighted(detail, words, t.primary) : detail),
            ),
        )
        .when(Boolean(folder), (el) =>
          el.child(
            h_flex()
              .gap_1()
              .child(icon("folder", t.muted_foreground, 14))
              .child(div().map(font(cx, "footnote")).text_color(t.muted_foreground).child(folder ?? "")),
          ),
        ),
    );
}

/** The round selection mark of a row in Select mode. */
function selectionMark(cx, id, selected) {
  const t = cx.theme();
  // Drawn, not a control: the row is what's pressed, and it announces the selection.
  return div()
    .test_id(id)
    .accessibility_hidden(true)
    .size_6()
    .flex_none()
    .flex()
    .items_center()
    .justify_center()
    .rounded_full()
    .border_2()
    .border_color(selected ? t.primary : t.muted_foreground)
    .when(selected, (el) => el.bg(t.primary))
    .child(selected ? icon("check", t.primary_foreground, 14) : div());
}

/** The note's actions, on a long press. */
export function noteMenu(cx, note) {
  const title = titleOf(note.body);
  actionSheet(cx, {
    id: "note-menu",
    title,
    message: library.folder(note.folder)?.name,
    actions: [
      {
        id: "menu-pin",
        label: note.pinned ? "Unpin Note" : "Pin Note",
        icon: "pin",
        run: (cx) => library.patch([note.id], { pinned: !note.pinned }, cx),
      },
      { id: "menu-move", label: "Move…", icon: "move", run: (cx) => moveSheet(cx, { ids: [note.id], title: `Move “${title}”` }) },
      { id: "menu-share", label: "Share…", icon: "share", run: (cx) => shareSheet(cx, note) },
      { id: "menu-delete", label: "Delete", icon: "trash", destructive: true, run: (cx) => deleteNotes(cx, [note.id]) },
    ],
  });
}

/** Deletes at once, with Undo on a toast: reversible work gets undo, not a question. */
export function deleteNotes(cx, ids) {
  const first = library.note(ids[0]);
  const restore = library.remove(ids, cx);
  cx.haptic("success");
  cx.push_toast({
    id: "deleted",
    title: ids.length === 1 && first ? `Deleted “${titleOf(first.body)}”` : `Deleted ${ids.length} notes`,
    duration: UNDO_MS,
    action: { label: "Undo", on_click: (cx) => restore(cx) },
  });
}

/** The line of a note around its first match, cut to stay readable in a row. */
function snippet(body, words) {
  const lines = body.split("\n").map(plain).filter(Boolean);
  const hit = lines.slice(1).find((l) => words.some((w) => l.toLocaleLowerCase().includes(w))) ?? lines[1] ?? "";
  const at = Math.min(...words.map((w) => hit.toLocaleLowerCase().indexOf(w)).filter((i) => i >= 0));
  // Keep a little context before a match deep in a long line.
  return Number.isFinite(at) && at > 40 ? `…${hit.slice(hit.lastIndexOf(" ", at - 20) + 1)}` : hit;
}
