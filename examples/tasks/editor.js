// The edit sheet: rename a task, set or clear its due date, or delete it.
//
// The sheet is a secondary task over the list, so it keeps the list's context
// behind it. Its draft lives in this closure; the list changes only on Save,
// and Cancel or a swipe down leaves the task as it was.

import { div, h_flex, v_flex, Button, Input, InputState, Switch, DatePicker } from "native-shell";
import { barButton, hairline, icon } from "./ui.js";
import { isoDate } from "./store.js";

/**
 * @param {import("native-shell").Context} cx
 * @param {import("./store.js").Task} task
 * @param {{ save: (task: import("./store.js").Task, cx: any) => void, remove: (task: import("./store.js").Task, cx: any) => void }} actions
 */
export function openEditor(cx, task, actions) {
  const title = InputState.new({ value: task.title, placeholder: "Task name", return_key: "done", auto_capitalize: "sentences" });
  let due = task.due;

  const close = () => {
    title.release();
    window.close_sheet();
  };
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
      const canSave = title.value().trim() !== "";
      return v_flex()
        .test_id("editor")
        .size_full()
        .bg(t.background)
        .pb(cx.window().safe_area.bottom + 16)
        .child(
          h_flex()
            .justify_between()
            .px_2()
            .py_2()
            .child(barButton(cx, "editor-cancel", "Cancel", () => close()))
            .child(div().role("header").text_base().font_semibold().text_color(t.foreground).child("Edit task"))
            .child(barButton(cx, "editor-save", "Save", (_e, cx) => commit(cx), { emphasized: true, disabled: !canSave })),
        )
        .child(
          v_flex()
            .px_4()
            .pt_2()
            .gap_6()
            .child(
              Input.new(title)
                .test_id("editor-title")
                .accessibility_label("Task name")
                .px_4()
                .py_3()
                .rounded_lg()
                .bg(t.muted)
                .text_base()
                .text_color(t.foreground),
            )
            .child(
              v_flex()
                .rounded_lg()
                .border_1()
                .border_color(t.border)
                .overflow_hidden()
                .child(
                  h_flex()
                    .gap_3()
                    .px_4()
                    .py_3()
                    .child(icon("calendar", t.muted_foreground))
                    .child(div().flex_1().text_base().text_color(t.foreground).child("Due date"))
                    .child(
                      Switch.new("editor-due")
                        .test_id("editor-due")
                        .accessibility_label("Due date")
                        .checked(due !== null)
                        .on_change((on, cx) => {
                          due = on ? (task.due ?? isoDate(0)) : null;
                          cx.notify();
                        }),
                    ),
                )
                .when(due !== null, (el) =>
                  el.child(hairline(cx, 48)).child(
                    h_flex()
                      .gap_3()
                      .pl(48)
                      .pr_4()
                      .py_2()
                      .child(div().flex_1().text_base().text_color(t.foreground).child("Date"))
                      .child(
                        DatePicker.new("editor-date")
                          .test_id("editor-date")
                          .min_w_40()
                          .value(due ?? isoDate(0))
                          .on_change((value, cx) => {
                            due = value;
                            cx.notify();
                          }),
                      ),
                  ),
                ),
            )
            .child(
              Button.new("editor-delete")
                .test_id("editor-delete")
                .flex()
                .items_center()
                .justify_center()
                .gap_2()
                .py_3()
                .rounded_lg()
                .border_1()
                .border_color(t.border)
                .text_base()
                .font_medium()
                .text_color(t.destructive)
                .active((s) => s.bg(t.muted))
                .child(icon("trash", t.destructive, 18))
                .child("Delete task")
                .on_click((_e, cx) => {
                  actions.remove(task, cx);
                  close();
                }),
            ),
        );
    },
    { detents: ["medium", "large"], on_dismiss: () => title.release() },
  );
}
