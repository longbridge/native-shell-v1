// Tasks: capture what needs doing, tick it off, fix it later.
//
// The list is the product, so it takes the screen; the one way in—the field
// at the bottom, where the thumb rests—is always visible above the keyboard.
// Completing is a tap on the circle, editing a tap on the row, and deleting
// lives in the edit sheet with an Undo bar afterwards, so no action depends
// on a hidden swipe. See README.md for the design notes.

import { View, div, h_flex, v_flex, list, Button, Checkbox, Input, InputState } from "native-shell";
import { load, save, describeDue } from "./store.js";
import { openEditor } from "./editor.js";
import { emptyState, hairline, icon, largeTitle } from "./ui.js";

const UNDO_MS = 5000;

export default class Tasks extends View {
  init(_props, cx) {
    this.items = load();
    this.nextId = this.items.reduce((max, i) => Math.max(max, i.id), 0) + 1;
    this.showCompleted = false;
    this.saveFailed = false;
    /** The last deletion, while it can still be undone. */
    this.undo = null;
    this.draft = InputState.new({ placeholder: "Add a task", return_key: "done", auto_capitalize: "sentences" });
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

  commit(cx) {
    this.saveFailed = !save(this.items);
    cx.notify();
  }

  add(cx) {
    const title = this.draft.value().trim();
    if (title === "") return;
    this.items = [...this.items, { id: this.nextId++, title, done: false, due: null }];
    this.draft.set_value("");
    this.commit(cx);
    cx.scroll_to_item("tasks", this.open.length - 1, "bottom");
  }

  setDone(task, done, cx) {
    this.items = this.items.map((i) => (i.id === task.id ? { ...i, done } : i));
    if (done) cx.haptic("success");
    this.commit(cx);
  }

  update(task, cx) {
    this.items = this.items.map((i) => (i.id === task.id ? task : i));
    this.commit(cx);
  }

  remove(task, cx) {
    const index = this.items.findIndex((i) => i.id === task.id);
    this.items = this.items.filter((i) => i.id !== task.id);
    this.offerUndo({ label: `Deleted “${task.title}”`, restore: (items) => [...items.slice(0, index), task, ...items.slice(index)] }, cx);
    this.commit(cx);
  }

  clearCompleted(cx) {
    const done = this.completed;
    cx.alert({
      title: `Delete ${done.length} completed ${done.length === 1 ? "task" : "tasks"}?`,
      actions: [
        { label: "Cancel", style: "cancel" },
        {
          label: "Delete",
          style: "destructive",
          on_press: (cx) => {
            const before = this.items;
            this.items = this.items.filter((i) => !i.done);
            this.offerUndo({ label: `Deleted ${done.length} completed`, restore: () => before }, cx);
            this.commit(cx);
          },
        },
      ],
    });
  }

  offerUndo(entry, cx) {
    this.undo?.timer.cancel();
    const timer = cx.timer.after(UNDO_MS, (cx) => {
      this.undo = null;
      cx.notify();
    });
    this.undo = { ...entry, timer };
  }

  restore(cx) {
    if (!this.undo) return;
    this.undo.timer.cancel();
    this.items = this.undo.restore(this.items);
    this.undo = null;
    this.commit(cx);
  }

  /** The list's rows: open tasks, then a disclosure header for the completed ones. */
  rows() {
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
    const win = cx.window();
    const open = this.open.length;
    const overdue = this.open.filter((i) => i.due && describeDue(i.due).overdue).length;
    const status = open === 0 ? "All done" : `${open} to do${overdue > 0 ? ` · ${overdue} overdue` : ""}`;
    const rows = this.rows();

    return v_flex()
      .size_full()
      .bg(t.background)
      .line_height(1.375)
      .child(largeTitle(cx, "Tasks", this.items.length > 0 ? status : null))
      .when(this.saveFailed, (el) => el.child(this.saveError(cx)))
      .child(
        this.items.length === 0
          ? emptyState(cx, "circle_check", "No tasks", "Add one below. It stays on this device until you complete it.")
          : list(
              "tasks",
              rows.length,
              (i) => rows[i].key,
              (i, cx) => (rows[i].task ? this.taskRow(rows[i].task, cx) : this.completedHeader(rows[i].header, cx)),
            )
              .test_id("tasks")
              .flex_1()
              .estimated_row_height(56),
      )
      .when(this.undo !== null, (el) => el.child(this.undoBar(cx)))
      .child(this.composer(cx, win));
  }

  taskRow(task, cx) {
    const t = cx.theme();
    const due = task.due && !task.done ? describeDue(task.due) : null;
    return v_flex()
      .child(
        h_flex()
          .items_start()
          .gap_3()
          .px_4()
          .py_3()
          .child(
            Checkbox.new(`check-${task.id}`)
              .test_id(`check-${task.id}`)
              .accessibility_label(task.title)
              .checked(task.done)
              .hit_slop(12)
              .size_6()
              .flex_none()
              .flex()
              .items_center()
              .justify_center()
              .rounded_full()
              .border_2()
              .border_color(task.done ? t.primary : t.muted_foreground)
              // Always drawn: a Checkbox without children would be the bare native control.
              .when(task.done, (el) => el.bg(t.primary))
              .child(task.done ? icon("check", t.primary_foreground, 14) : div())
              .active((s) => s.opacity(0.5))
              .on_change((done, cx) => this.setDone(task, done, cx)),
          )
          .child(
            Button.new(`task-${task.id}`)
              .test_id(`task-${task.id}`)
              .accessibility_label(`Edit “${task.title}”`)
              .flex_1()
              .min_w_0()
              .gap_1()
              .flex()
              .flex_col()
              .active((s) => s.opacity(0.5))
              .on_click((_e, cx) =>
                openEditor(cx, task, {
                  save: (next, cx) => this.update(next, cx),
                  remove: (gone, cx) => this.remove(gone, cx),
                }),
              )
              .child(
                div()
                  .test_id(`title-${task.id}`)
                  .text_base()
                  .text_color(task.done ? t.muted_foreground : t.foreground)
                  .when(task.done, (el) => el.line_through())
                  .child(task.title),
              )
              .when(due !== null, (el) =>
                el.child(
                  h_flex()
                    .gap_1()
                    .child(icon(due.overdue ? "alert" : "calendar", due.overdue ? t.destructive : t.muted_foreground, 14))
                    .child(
                      div()
                        .test_id(`due-${task.id}`)
                        .text_sm()
                        .text_color(due.overdue ? t.destructive : t.muted_foreground)
                        .child(due.text),
                    ),
                ),
              ),
          ),
      )
      .child(hairline(cx, 52));
  }

  completedHeader(count, cx) {
    const t = cx.theme();
    return h_flex()
      .pt_4()
      .pb_1()
      .pl_4()
      .pr_2()
      .gap_2()
      .child(
        Button.new("toggle-completed")
          .test_id("toggle-completed")
          .accessibility_label(`Completed, ${count}`)
          .aria_selected(this.showCompleted)
          .flex_1()
          .flex()
          .items_center()
          .gap_2()
          .py_2()
          .active((s) => s.opacity(0.5))
          .on_click((_e, cx) => {
            this.showCompleted = !this.showCompleted;
            cx.notify();
          })
          .child(div().text_sm().font_semibold().text_color(t.muted_foreground).child(`Completed · ${count}`))
          .child(icon(this.showCompleted ? "chevron_down" : "chevron_right", t.muted_foreground, 16)),
      )
      .child(
        Button.new("clear-completed")
          .test_id("clear-completed")
          .px_2()
          .py_2()
          .rounded_md()
          .text_sm()
          .font_medium()
          .text_color(t.primary)
          .active((s) => s.bg(t.muted))
          .on_click((_e, cx) => this.clearCompleted(cx))
          .child("Clear…"),
      );
  }

  composer(cx, win) {
    const t = cx.theme();
    const empty = this.draft.value().trim() === "";
    return h_flex()
      .gap_2()
      .px_4()
      .pt_2()
      .pb(Math.max(win.keyboard_height, win.safe_area.bottom) + 8)
      .border_t_1()
      .border_color(t.border)
      .bg(t.background)
      .child(
        Input.new(this.draft)
          .test_id("draft")
          .accessibility_label("New task")
          .flex_1()
          .min_h_11()
          .px_4()
          .py_2()
          .rounded_full()
          .bg(t.muted)
          .text_base()
          .text_color(t.foreground),
      )
      .child(
        Button.new("add")
          .test_id("add")
          .accessibility_label("Add task")
          .disabled(empty)
          .size_11()
          .flex_none()
          .flex()
          .items_center()
          .justify_center()
          .rounded_full()
          .bg(empty ? t.muted : t.primary)
          .when(!empty, (el) => el.active((s) => s.opacity(0.7)))
          .on_click((_e, cx) => this.add(cx))
          .child(icon("arrow_up", empty ? t.muted_foreground : t.primary_foreground, 20)),
      );
  }

  undoBar(cx) {
    const t = cx.theme();
    return h_flex()
      .test_id("undo-bar")
      .role("alert")
      .mx_4()
      .mb_2()
      .pl_4()
      .pr_1()
      .py_1()
      .gap_3()
      .rounded_lg()
      .bg(t.foreground)
      .shadow_md()
      .child(div().test_id("undo-label").flex_1().min_w_0().text_sm().text_color(t.background).truncate().child(this.undo.label))
      .child(
        Button.new("undo")
          .test_id("undo")
          .px_3()
          .py_2()
          .rounded_md()
          .text_sm()
          .font_semibold()
          .text_color(t.background)
          .active((s) => s.opacity(0.6))
          .on_click((_e, cx) => this.restore(cx))
          .child("Undo"),
      );
  }

  saveError(cx) {
    const t = cx.theme();
    return h_flex()
      .test_id("save-error")
      .role("alert")
      .gap_2()
      .mx_4()
      .mb_2()
      .px_3()
      .py_2()
      .rounded_lg()
      .border_1()
      .border_color(t.destructive)
      .child(icon("alert", t.destructive, 16))
      .child(div().flex_1().text_sm().text_color(t.foreground).child("Couldn’t save changes. They’ll be lost when the app closes."));
  }
}
