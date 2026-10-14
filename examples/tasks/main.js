// Tasks: capture what needs doing, tick it off, fix it later.
//
// A plain iOS list under a large title. The list is the product, so it takes
// the screen; the one way in, the field at the bottom where the thumb rests,
// is always visible above the keyboard. Completing is a tap on the circle,
// editing a tap on the row, and deleting lives in the edit sheet with an Undo
// toast afterwards, so no action depends on a hidden swipe. See README.md.

import { View, div, h_flex, v_flex, list, Button, Checkbox, Input, InputState } from "native-shell";
import { load, save, describeDue } from "./store.js";
import { openEditor } from "./editor.js";
import { emptyState, icon, label, largeTitle, statusBar, textButton, textStyle } from "./ui.js";
/** @import { Context } from "native-shell" */
/** @import { Task } from "./store.js" */

export default class Tasks extends View {
  init(_props, cx) {
    this.items = load();
    this.nextId = this.items.reduce((max, i) => Math.max(max, i.id), 0) + 1;
    this.showCompleted = false;
    this.saveFailed = false;
    this.draft = InputState.new({ placeholder: "New task", return_key: "done", auto_capitalize: "sentences" });
    this.draft.on("change", (_e, cx) => cx.notify());
    // Return adds and keeps the keyboard up, so a list can be typed in one go.
    this.draft.on("submit", (_e, cx) => this.add(cx));
  }

  get open() {
    return this.items.filter((i) => !i.done);
  }

  get completed() {
    return this.items.filter((i) => i.done);
  }

  /** @param {Context} cx */
  commit(cx) {
    this.saveFailed = !save(this.items);
    cx.notify();
  }

  /** @param {Context} cx */
  add(cx) {
    const title = this.draft.value().trim();
    if (title === "") return;
    this.items = [...this.items, { id: this.nextId++, title, done: false, due: null }];
    this.draft.set_value("");
    this.commit(cx);
    cx.scroll_to_item("tasks", this.open.length - 1, "bottom");
  }

  /** @param {Task} task @param {boolean} done @param {Context} cx */
  setDone(task, done, cx) {
    this.items = this.items.map((i) => (i.id === task.id ? { ...i, done } : i));
    cx.haptic(done ? "success" : "selection");
    this.commit(cx);
  }

  /** @param {Task} task @param {Context} cx */
  replaceTask(task, cx) {
    this.items = this.items.map((i) => (i.id === task.id ? task : i));
    this.commit(cx);
  }

  /** @param {Task} task @param {Context} cx */
  remove(task, cx) {
    const before = this.items;
    this.items = this.items.filter((i) => i.id !== task.id);
    this.commit(cx);
    this.offerUndo(`Deleted “${task.title}”`, before, cx);
  }

  /** @param {Context} cx */
  clearCompleted(cx) {
    const count = this.completed.length;
    cx.alert({
      title: `Delete ${count} completed ${count === 1 ? "task" : "tasks"}?`,
      actions: [
        { label: "Cancel", style: "cancel" },
        {
          label: "Delete",
          style: "destructive",
          on_press: (cx) => {
            const before = this.items;
            this.items = this.items.filter((i) => !i.done);
            this.commit(cx);
            this.offerUndo(`Deleted ${count} completed ${count === 1 ? "task" : "tasks"}`, before, cx);
          },
        },
      ],
    });
  }

  /** Deletion is reversible for a few seconds: the toast's Undo puts the list back. @param {string} title @param {Task[]} before @param {Context} cx */
  offerUndo(title, before, cx) {
    cx.push_toast({
      id: "undo",
      title,
      timeout: 5000,
      action: {
        label: "Undo",
        on_click: (cx) => {
          this.items = before;
          this.commit(cx);
        },
      },
    });
  }

  /** The list's rows: open tasks, then a header for the completed ones and, when shown, the tasks themselves. */
  rows() {
    /** @type {{ key: string, task?: Task, header?: number }[]} */
    const rows = this.open.map((task) => ({ key: `task-${task.id}`, task }));
    const done = this.completed;
    if (done.length > 0) {
      rows.push({ key: "completed", header: done.length });
      if (this.showCompleted) rows.push(...done.map((task) => ({ key: `task-${task.id}`, task })));
    }
    return rows;
  }

  render(cx) {
    const t = cx.theme();
    const open = this.open.length;
    const overdue = this.open.filter((i) => i.due && describeDue(i.due).overdue).length;
    const status = open === 0 ? "All done" : `${open} to do${overdue > 0 ? ` · ${overdue} overdue` : ""}`;
    const rows = this.rows();

    return v_flex()
      .size_full()
      .bg(t.background)
      .child(statusBar(cx, t.background))
      .child(largeTitle(cx, { title: "Tasks", subtitle: this.items.length > 0 ? status : null }))
      .when(this.saveFailed, (el) => el.child(this.saveError(cx)))
      .child(
        this.items.length === 0
          ? emptyState(cx, "circle_check", "No tasks", "Add one below. Tasks stay on this device until you complete them.")
          : list(
              "tasks",
              rows.length,
              (i) => rows[i].key,
              (i, cx) => {
                const row = rows[i];
                return row.task ? this.taskRow(row.task, cx) : this.completedHeader(row.header ?? 0, cx);
              },
            )
              .test_id("tasks")
              .flex_1()
              .estimated_row_height(56),
      )
      .child(this.composer(cx));
  }

  /** @param {Task} task @param {Context} cx */
  taskRow(task, cx) {
    const t = cx.theme();
    const due = task.due && !task.done ? describeDue(task.due) : null;
    return v_flex().child(
      h_flex()
        .items_start()
        .gap_3()
        .pl_4()
        .child(
          Checkbox.new(`check-${task.id}`)
            .test_id(`check-${task.id}`)
            .accessibility_label(task.title)
            .checked(task.done)
            .hit_slop(11)
            .mt_2p5()
            .size("1.375rem")
            .flex_none()
            .flex()
            .items_center()
            .justify_center()
            .rounded_full()
            .border(1.5)
            .border_color(task.done ? t.primary : t.input)
            .when(task.done, (el) => el.bg(t.primary))
            // Always drawn: a Checkbox without children would be the bare native control.
            .child(task.done ? icon("check", t.primary_foreground, 14) : div())
            .active((s) => s.opacity(0.4))
            .on_change((done, cx) => this.setDone(task, done, cx)),
        )
        .child(
          Button.new(`task-${task.id}`)
            .test_id(`task-${task.id}`)
            .accessibility_label(`Edit “${task.title}”`)
            .flex_1()
            .min_w_0()
            .flex()
            .flex_col()
            .gap_0p5()
            .py_2p5()
            .pr_4()
            .min_h_11()
            .justify_center()
            .border_b(0.5)
            .border_color(t.border)
            .active((s) => s.opacity(0.5))
            .on_click((_e, cx) =>
              openEditor(cx, task, {
                save: (next, cx) => this.replaceTask(next, cx),
                remove: (gone, cx) => this.remove(gone, cx),
              }),
            )
            .child(
              label(cx, "body", task.done ? t.muted_foreground : t.foreground, task.title)
                .test_id(`title-${task.id}`)
                .when(task.done, (el) => el.line_through()),
            )
            .when(due !== null, (el) =>
              el.child(
                h_flex()
                  .gap_1()
                  .when(Boolean(due?.overdue), (el) => el.child(icon("alert", t.destructive, 14)))
                  .child(
                    label(cx, "subheadline", due?.overdue ? t.destructive : t.muted_foreground, due?.text ?? "").test_id(`due-${task.id}`),
                  ),
              ),
            ),
        ),
    );
  }

  /** @param {number} count @param {Context} cx */
  completedHeader(count, cx) {
    const t = cx.theme();
    return h_flex()
      .pt_6()
      .pb_1()
      .pl_4()
      .pr_2()
      .gap_1()
      .child(
        Button.new("toggle-completed")
          .test_id("toggle-completed")
          .accessibility_label(`Completed, ${count}`)
          .aria_selected(this.showCompleted)
          .flex_1()
          .flex()
          .items_center()
          .gap_1p5()
          .py_2()
          .active((s) => s.opacity(0.4))
          .on_click((_e, cx) => {
            this.showCompleted = !this.showCompleted;
            cx.haptic("selection");
            cx.notify();
          })
          .child(label(cx, "headline", t.foreground, "Completed"))
          .child(label(cx, "body", t.muted_foreground, String(count)))
          .child(icon(this.showCompleted ? "chevron_down" : "chevron_right", t.muted_foreground, 16)),
      )
      .child(textButton(cx, "clear-completed", "Clear…", { on_click: (_e, cx) => this.clearCompleted(cx) }));
  }

  /** @param {Context} cx */
  composer(cx) {
    const t = cx.theme();
    const win = cx.window();
    const empty = this.draft.value().trim() === "";
    return h_flex()
      .gap_2()
      .px_4()
      .pt_2()
      .pb(Math.max(win.keyboard_height, win.safe_area.bottom) + 8)
      .border_t(0.5)
      .border_color(t.border)
      .bg(t.background)
      .child(
        textStyle(Input.new(this.draft), cx, "body")
          .test_id("draft")
          .accessibility_label("New task")
          .flex_1()
          .min_h_9()
          .px_4()
          .py_1p5()
          .rounded_full()
          .bg(t.muted)
          .text_color(t.foreground),
      )
      .child(
        Button.new("add")
          .test_id("add")
          .accessibility_label("Add task")
          .disabled(empty)
          .hit_slop(6)
          .size_8()
          .flex_none()
          .flex()
          .items_center()
          .justify_center()
          .rounded_full()
          .bg(empty ? t.secondary : t.primary)
          .when(!empty, (el) => el.active((s) => s.opacity(0.6)))
          .on_click((_e, cx) => this.add(cx))
          .child(icon("arrow_up", empty ? t.muted_foreground : t.primary_foreground, 18)),
      );
  }

  /** @param {Context} cx */
  saveError(cx) {
    const t = cx.theme();
    return h_flex()
      .test_id("save-error")
      .role("alert")
      .items_start()
      .gap_2()
      .mx_4()
      .mb_2()
      .p_3()
      .rounded(t.radius.md)
      .bg(t.muted)
      .child(icon("alert", t.warning, 18))
      .child(label(cx, "subheadline", t.foreground, "Couldn’t save changes. They’ll be lost when the app closes.").flex_1());
  }
}
