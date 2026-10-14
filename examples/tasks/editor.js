// The edit sheet: rename a task, set or clear its due date, or delete it.
//
// An iOS sheet for a secondary task: Cancel and Save in its bar, the fields as
// inset grouped rows, the destructive row last and red. The draft lives in
// this closure; the list changes only on Save, and Cancel or a swipe down
// leaves the task as it was.

import { h_flex, v_flex, Input, InputState, Switch, DatePicker } from "native-shell";
import { label, row, section, textButton, textStyle } from "./ui.js";
import { isoDate } from "./store.js";
/** @import { Context } from "native-shell" */
/** @import { Task } from "./store.js" */

/**
 * @param {Context} cx
 * @param {Task} task
 * @param {{ save: (task: Task, cx: Context) => void, remove: (task: Task, cx: Context) => void }} actions
 */
export function openEditor(cx, task, actions) {
  const title = InputState.new({ value: task.title, placeholder: "Task name", return_key: "done", auto_capitalize: "sentences" });
  /** @type {string | null} */
  let due = task.due;

  const close = () => {
    title.release();
    window.close_sheet();
  };
  /** @param {Context} cx */
  const commit = (cx) => {
    const name = title.value().trim();
    if (name === "") return;
    actions.save({ ...task, title: name, due }, cx);
    close();
  };
  title.on("change", (_e, cx) => cx.notify());
  title.on("submit", (_e, cx) => commit(cx));

  cx.open_sheet(
    () => {
      const t = cx.theme();
      return v_flex()
        .test_id("editor")
        .size_full()
        .bg(t.grouped_background)
        .pb(cx.window().safe_area.bottom + 16)
        .child(
          h_flex()
            .relative()
            .justify_between()
            .px_2()
            .pt_3()
            .pb_2()
            .child(textButton(cx, "editor-cancel", "Cancel", { on_click: () => close() }))
            .child(label(cx, "headline", t.foreground, "Edit task").role("header"))
            .child(
              textButton(cx, "editor-save", "Save", {
                on_click: (_e, cx) => commit(cx),
                emphasized: true,
                disabled: title.value().trim() === "",
              }),
            ),
        )
        .child(
          v_flex()
            .px_4()
            .pt_3()
            .gap_8()
            .child(
              section(cx, {
                rows: [
                  textStyle(Input.new(title), cx, "body")
                    .test_id("editor-title")
                    .accessibility_label("Task name")
                    .min_h_11()
                    .px_4()
                    .text_color(t.foreground),
                ],
              }),
            )
            .child(
              section(cx, {
                rows: [
                  row(cx, {
                    id: "editor-due-row",
                    label: "Due date",
                    trailing: Switch.new("editor-due")
                      .test_id("editor-due")
                      .accessibility_label("Due date")
                      .checked(due !== null)
                      .on_change((on, cx) => {
                        due = on ? (task.due ?? isoDate(0)) : null;
                        cx.notify();
                      }),
                  }),
                  due !== null &&
                    row(cx, {
                      id: "editor-date-row",
                      label: "Date",
                      trailing: DatePicker.new("editor-date")
                        .test_id("editor-date")
                        .value(due)
                        .on_change((value, cx) => {
                          due = value;
                          cx.notify();
                        }),
                    }),
                ],
              }),
            )
            .child(
              section(cx, {
                rows: [
                  row(cx, {
                    id: "editor-delete",
                    label: "Delete task",
                    destructive: true,
                    on_click: (_e, cx) => {
                      actions.remove(task, cx);
                      close();
                    },
                  }),
                ],
              }),
            ),
        );
    },
    { detents: ["medium", "large"], on_dismiss: () => title.release() },
  );
}
