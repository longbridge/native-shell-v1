// The overlays: naming a folder, moving notes, sharing a note.
//
// Each uses the smallest surface that fits: a dialog for a one-field form,
// sheets for choices that keep the list in view. What they close over is
// their state, and the library changes only when the user confirms.

import { div, h_flex, v_flex, Button, Input, InputState } from "native-shell";
import { library, plain, titleOf } from "./store.js";
import { font, icon, section } from "./ui.js";
/** @import { Context } from "native-shell" */

/**
 * Asks for a folder's name: a new folder, or a new name for one. Save waits
 * for a name no other folder has.
 * @param {Context} cx
 * @param {{ title: string, message: string, folder?: import("./store.js").Folder, save: (name: string, cx: Context) => void }} opts
 */
export function folderDialog(cx, { title, message, folder, save }) {
  const state = InputState.new({ value: folder?.name ?? "", placeholder: "Name", return_key: "done", auto_capitalize: "words" });
  const problem = () => {
    const name = state.value().trim();
    if (name === "") return "empty";
    if (library.folderNamed(name, folder?.id ?? null)) return `A folder named “${name}” already exists.`;
    return null;
  };
  const close = () => {
    state.release();
    window.close_dialog();
  };
  const commit = (cx) => {
    if (problem() !== null) return;
    const name = state.value().trim();
    close();
    save(name, cx);
  };
  state.on("change", (_e, cx) => cx.notify());
  state.on("submit", (_e, cx) => commit(cx));
  window.open_dialog(
    () => {
      const t = cx.theme();
      const issue = problem();
      const button = (id, label, on_click, { emphasized = false, disabled = false } = {}) =>
        Button.new(id)
          .test_id(id)
          .disabled(disabled)
          .flex_1()
          .flex()
          .items_center()
          .justify_center()
          .min_h_11()
          .py_2()
          .map(font(cx, emphasized ? "headline" : "body"))
          .text_color(disabled ? t.muted_foreground : t.primary)
          .when(!disabled, (el) => el.active((s) => s.bg(t.secondary)).on_click(on_click))
          .child(label);
      return v_flex()
        .test_id("folder-dialog")
        .w(Math.min(300, cx.window().width - 48))
        .rounded_xl()
        .bg(t.surface)
        .overflow_hidden()
        .child(
          v_flex()
            .px_4()
            .pt_5()
            .pb_4()
            .gap_1()
            .child(div().role("header").map(font(cx, "headline")).text_color(t.foreground).text_center().child(title))
            .child(div().map(font(cx, "footnote")).text_color(t.foreground).text_center().child(message))
            .child(
              Input.new(state)
                .test_id("folder-name")
                .accessibility_label("Folder name")
                .mt_3()
                .min_h_10()
                .px_2()
                .rounded_md()
                .border_1()
                .border_color(issue !== null && issue !== "empty" ? t.destructive : t.input)
                .bg(t.background)
                .map(font(cx, "body"))
                .text_color(t.foreground),
            )
            .when(issue !== null && issue !== "empty", (el) =>
              el.child(div().test_id("folder-name-error").role("alert").map(font(cx, "footnote")).text_color(t.destructive).child(issue)),
            ),
        )
        .child(div().h(1).bg(t.border))
        .child(
          h_flex()
            .items_stretch()
            .child(button("folder-cancel", "Cancel", () => close()))
            .child(div().w(1).bg(t.border))
            .child(button("folder-save", "Save", (_e, cx) => commit(cx), { emphasized: true, disabled: issue !== null })),
        );
    },
    { on_dismiss: () => state.release() },
  );
}

/**
 * Moves notes to another folder: every folder, the one they are in marked
 * and not selectable.
 * @param {Context} cx
 * @param {{ ids: string[], title: string, done?: (cx: Context) => void }} opts
 */
export function moveSheet(cx, { ids, title, done }) {
  const from = new Set(ids.map((id) => library.note(id)?.folder));
  cx.open_sheet(
    () => {
      const t = cx.theme();
      const rows = library.folders.map((f) => {
        const here = from.size === 1 && from.has(f.id);
        return Button.new(`move-to-${f.id}`)
          .test_id(`move-to-${f.id}`)
          .accessibility_label(here ? `${f.name}, current folder` : f.name)
          .disabled(here)
          .flex()
          .items_center()
          .gap_3()
          .px_4()
          .py_3()
          .min_h_11()
          .when(!here, (el) =>
            el.active((s) => s.bg(t.secondary)).on_click((_e, cx) => {
              library.patch(ids, { folder: f.id }, cx);
              cx.haptic("success");
              window.close_sheet();
              done?.(cx);
            }),
          )
          .child(icon("folder", t.primary))
          .child(div().flex_1().map(font(cx, "body")).text_color(here ? t.muted_foreground : t.foreground).child(f.name))
          .child(here ? icon("check", t.muted_foreground, 18) : div().map(font(cx, "body")).text_color(t.muted_foreground).child(String(library.count(f.id))));
      });
      return v_flex()
        .test_id("move-sheet")
        .size_full()
        .bg(t.background)
        .pt_5()
        .gap_4()
        .child(div().role("header").px_8().map(font(cx, "headline")).text_color(t.foreground).text_center().child(title))
        .child(v_flex().flex_1().overflow_y_scroll().pb(cx.window().safe_area.bottom + 16).child(section(cx, { rows, inset: 50, on_sheet: true })));
    },
    { detents: ["medium", "large"] },
  );
}

/**
 * Shares a note by copying it, as Markdown or as plain text, with a preview
 * of what will be copied.
 * @param {Context} cx
 * @param {import("./store.js").Note} note
 */
export function shareSheet(cx, note) {
  const copy = (cx, text, what) => {
    cx.write_to_clipboard(text);
    window.close_sheet();
    cx.haptic("success");
    // Some systems confirm a copy themselves; a second confirmation would be noise.
    if (!cx.window().clipboard_feedback) cx.push_toast({ id: "copied", title: `Copied ${what}`, level: "success", duration: 2000 });
  };
  const asText = note.body
    .split("\n")
    .map((l) => (/^\s*[-*+]\s+\[[xX]\]/.test(l) ? `✓ ${plain(l)}` : /^\s*[-*+]\s+\[ \]/.test(l) ? `○ ${plain(l)}` : plain(l)))
    .join("\n");
  cx.open_sheet(
    () => {
      const t = cx.theme();
      const action = (id, name, label, run) =>
        Button.new(id)
          .test_id(id)
          .flex()
          .items_center()
          .gap_3()
          .px_4()
          .py_3()
          .min_h_12()
          .active((s) => s.bg(t.secondary))
          .on_click((_e, cx) => run(cx))
          .child(div().flex_1().map(font(cx, "body")).text_color(t.foreground).child(label))
          .child(icon(name, t.primary, 20));
      return v_flex()
        .test_id("share-sheet")
        .size_full()
        .bg(t.background)
        .pt_5()
        .pb(cx.window().safe_area.bottom + 16)
        .gap_4()
        .child(div().role("header").px_8().map(font(cx, "headline")).text_color(t.foreground).text_center().truncate().child(`Share “${titleOf(note.body)}”`))
        .child(
          div()
            .mx_4()
            .p_4()
            .rounded_xl()
            .bg(t.muted)
            .child(div().test_id("share-preview").map(font(cx, "subheadline")).text_color(t.muted_foreground).line_clamp(5).child(asText)),
        )
        .child(
          section(cx, {
            on_sheet: true,
            rows: [
              action("copy-text", "copy", "Copy Text", (cx) => copy(cx, asText, "text")),
              action("copy-markdown", "note", "Copy as Markdown", (cx) => copy(cx, note.body, "as Markdown")),
            ],
          }),
        );
    },
    { detents: ["medium"] },
  );
}

