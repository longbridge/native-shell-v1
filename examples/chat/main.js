// Chat: one conversation thread.
//
// The thread reads bottom-up like every messenger: newest at the bottom, the
// composer under it and always above the keyboard, Return sends. Messages
// from one sender a few minutes apart form a burst (tight spacing, one tail),
// days are separated by a quiet label, and delivery state is shown once, under
// your latest message. A tap or a long press on a message opens its actions,
// so nothing depends on discovering the gesture. See README.md.

import { View, div, h_flex, v_flex, list, Button, Input, InputState, Spinner } from "native-shell";
import { CONTACT, recentMessages, fetchEarlier, clock, dayLabel, sameBurst } from "./conversation.js";
import { avatar, icon } from "./ui.js";

const UNDO_MS = 5000;

export default class Chat extends View {
  init(_props, cx) {
    /** @type {import("./conversation.js").Message[]} */
    this.messages = recentMessages();
    /** "more" | "loading" | "done" | "error" */
    this.history = "more";
    this.undo = null;
    this.nextId = 1;
    this.draft = InputState.new({ placeholder: "Message", return_key: "send", auto_capitalize: "sentences" });
    this.draft.on("change", (_e, cx) => cx.notify());
    this.draft.on("submit", (_e, cx) => this.send(cx));
    // The keyboard rises over a few hundred milliseconds, and its height reaches
    // layout at a different moment on each platform; keep the newest message in
    // view once it has settled.
    this.draft.on("focus", (_e, cx) => {
      for (const ms of [300, 700]) cx.timer.after(ms, (cx) => this.scrollToEnd(cx));
    });
    cx.timer.after(0, (cx) => this.scrollToEnd(cx));
  }

  rows() {
    const rows = [{ key: "history", kind: "history" }];
    const lastMine = this.messages.findLastIndex((m) => m.from === "me");
    this.messages.forEach((m, i) => {
      const prev = this.messages[i - 1];
      const label = dayLabel(m.at);
      if (!prev || dayLabel(prev.at) !== label) rows.push({ key: `day-${label}`, kind: "day", label, at: m.at });
      rows.push({
        key: m.id,
        kind: "message",
        message: m,
        joined: sameBurst(prev, m) && dayLabel(prev.at) === label,
        tail: !sameBurst(m, this.messages[i + 1]),
        showStatus: i === lastMine,
      });
    });
    return rows;
  }

  scrollToEnd(cx) {
    cx.scroll_to_item("messages", this.rows().length - 1, "bottom");
  }

  send(cx) {
    const text = this.draft.value().trim();
    if (text === "") return;
    const message = { id: `new-${this.nextId++}`, from: "me", text, at: Date.now(), status: "sending" };
    this.messages = [...this.messages, message];
    this.draft.set_value("");
    cx.notify();
    this.scrollToEnd(cx);
    this.deliver(message.id, cx);
  }

  // A stand-in for the network round trip that confirms delivery.
  deliver(id, cx) {
    cx.timer.after(700, (cx) => {
      this.setStatus(id, "sent");
      cx.notify();
    });
  }

  setStatus(id, status) {
    this.messages = this.messages.map((m) => (m.id === id ? { ...m, status } : m));
  }

  retry(message, cx) {
    this.setStatus(message.id, "sending");
    cx.notify();
    this.deliver(message.id, cx);
  }

  loadEarlier(cx) {
    this.history = "loading";
    cx.notify();
    cx.spawn(async (cx) => {
      try {
        const page = await fetchEarlier(cx);
        this.messages = [...page.messages, ...this.messages];
        this.history = page.more ? "more" : "done";
      } catch {
        this.history = "error";
      }
      cx.notify();
    });
  }

  remove(message, cx) {
    const index = this.messages.findIndex((m) => m.id === message.id);
    this.messages = this.messages.filter((m) => m.id !== message.id);
    this.undo?.timer.cancel();
    const timer = cx.timer.after(UNDO_MS, (cx) => {
      this.undo = null;
      cx.notify();
    });
    this.undo = { message, index, timer };
    cx.notify();
  }

  restore(cx) {
    if (!this.undo) return;
    const { message, index, timer } = this.undo;
    timer.cancel();
    this.messages = [...this.messages.slice(0, index), message, ...this.messages.slice(index)];
    this.undo = null;
    cx.notify();
  }

  openActions(message, cx) {
    cx.haptic("selection");
    const t = cx.theme();
    const action = (id, name, label, color, run) =>
      Button.new(id)
        .test_id(id)
        .flex()
        .items_center()
        .gap_3()
        .px_4()
        .py_3()
        .min_h_12()
        .rounded_lg()
        .text_base()
        .text_color(color)
        .active((s) => s.bg(t.muted))
        .on_click((_e, cx) => {
          window.close_sheet();
          run(cx);
        })
        .child(icon(name, color))
        .child(label);
    cx.open_sheet(
      () =>
        v_flex()
          .test_id("actions")
          .size_full()
          .bg(t.background)
          .px_2()
          .pt_4()
          .pb(cx.window().safe_area.bottom + 8)
          .gap_1()
          .child(
            v_flex()
              .px_4()
              .pb_3()
              .gap_1()
              .child(
                div()
                  .text_xs()
                  .text_color(t.muted_foreground)
                  .child(`${message.from === "me" ? "You" : CONTACT.name} · ${dayLabel(message.at)}, ${clock(message.at)}`),
              )
              .child(div().text_base().text_color(t.foreground).line_clamp(3).child(message.text)),
          )
          .child(div().h(1).mx_4().mb_1().bg(t.border))
          .when(message.status === "failed", (el) =>
            el.child(action("action-retry", "refresh", "Send again", t.foreground, (cx) => this.retry(message, cx))),
          )
          .child(
            action("action-copy", "copy", "Copy text", t.foreground, (cx) => {
              cx.write_to_clipboard(message.text);
              // Android shows its own clipboard confirmation; a second one would repeat it.
              if (cx.window().platform !== "android") cx.push_toast({ title: "Copied", level: "success", timeout: 1500 });
            }),
          )
          .child(action("action-delete", "trash", "Delete for me", t.destructive, (cx) => this.remove(message, cx))),
      { detents: ["medium"] },
    );
  }

  render(cx) {
    const t = cx.theme();
    const win = cx.window();
    const rows = this.rows();
    return v_flex()
      .size_full()
      .bg(t.background)
      .line_height(1.375)
      .child(this.header(cx))
      .child(
        list(
          "messages",
          rows.length,
          (i) => rows[i].key,
          (i, cx) => this.row(rows[i], cx, win),
        )
          .test_id("messages")
          .flex_1()
          .estimated_row_height(60),
      )
      .when(this.undo !== null, (el) => el.child(this.undoBar(cx)))
      .child(this.composer(cx, win));
  }

  header(cx) {
    const t = cx.theme();
    return h_flex()
      .gap_3()
      .px_4()
      .pt(cx.window().safe_area.top + 8)
      .pb_3()
      .border_b_1()
      .border_color(t.border)
      .child(avatar(cx, CONTACT.name))
      .child(
        v_flex()
          .flex_1()
          .min_w_0()
          .child(div().role("header").text_base().font_semibold().text_color(t.foreground).truncate().child(CONTACT.name))
          .child(div().text_xs().text_color(t.muted_foreground).child(CONTACT.status)),
      );
  }

  row(row, cx, win) {
    if (row.kind === "history") return this.historyRow(cx);
    const t = cx.theme();
    if (row.kind === "day") {
      return div()
        .pt_6()
        .pb_2()
        .text_center()
        .text_xs()
        .font_medium()
        .text_color(t.muted_foreground)
        .child(`${row.label} · ${clock(row.at)}`);
    }
    return this.messageRow(row, cx, win);
  }

  historyRow(cx) {
    const t = cx.theme();
    const box = h_flex().justify_center().gap_2().px_4().pt_4().pb_2().min_h_12();
    switch (this.history) {
      case "loading":
        return box
          .test_id("history-loading")
          .child(Spinner.new())
          .child(div().text_sm().text_color(t.muted_foreground).child("Loading earlier messages…"));
      case "done":
        return box
          .test_id("history-start")
          .child(
            div()
              .text_sm()
              .text_center()
              .text_color(t.muted_foreground)
              .child(`This is the beginning of your conversation with ${CONTACT.name}.`),
          );
      default:
        return box.child(
          Button.new("load-earlier")
            .test_id("load-earlier")
            .flex()
            .items_center()
            .gap_2()
            .px_4()
            .py_2()
            .rounded_full()
            .border_1()
            .border_color(t.border)
            .text_sm()
            .font_medium()
            .text_color(t.foreground)
            .active((s) => s.bg(t.muted))
            .on_click((_e, cx) => this.loadEarlier(cx))
            .child(icon("history", t.muted_foreground, 16))
            .child(this.history === "error" ? "Couldn’t load. Try again" : "Load earlier messages"),
        );
    }
  }

  messageRow({ message: m, joined, tail, showStatus }, cx, win) {
    const t = cx.theme();
    const mine = m.from === "me";
    const failed = m.status === "failed";
    const bubble = Button.new(`bubble-${m.id}`)
      .test_id(`bubble-${m.id}`)
      .accessibility_label(`${mine ? "You" : CONTACT.name}, ${clock(m.at)}: ${m.text}`)
      .accessibility_hint("Shows actions")
      .max_w(Math.round(win.width * 0.78))
      .px_4()
      .py_2()
      .rounded_2xl()
      .when(tail && mine, (el) => el.rounded_br(6))
      .when(tail && !mine, (el) => el.rounded_bl(6))
      .bg(mine ? t.primary : t.muted)
      .text_base()
      .text_color(mine ? t.primary_foreground : t.foreground)
      .active((s) => s.opacity(0.7))
      .on_click((_e, cx) => this.openActions(m, cx))
      .on_long_press((_e, cx) => this.openActions(m, cx))
      .child(m.text);
    return v_flex()
      .px_4()
      .pt(joined ? 2 : 12)
      .when(mine, (el) => el.items_end())
      .when(!mine, (el) => el.items_start())
      .child(bubble)
      .when(failed, (el) => el.child(this.failedLine(m, cx)))
      .when(showStatus && !failed, (el) =>
        el.child(div().test_id("delivery").pt_1().text_xs().text_color(t.muted_foreground).child(deliveryText(m))),
      );
  }

  failedLine(m, cx) {
    const t = cx.theme();
    return h_flex()
      .test_id(`failed-${m.id}`)
      .gap_1()
      .pt_1()
      .child(icon("alert", t.destructive, 14))
      .child(div().text_xs().text_color(t.destructive).child("Not delivered"))
      .child(
        Button.new(`retry-${m.id}`)
          .test_id(`retry-${m.id}`)
          .accessibility_label("Send again")
          .ml_1()
          .px_2()
          .py_1()
          .rounded_md()
          .hit_slop(10)
          .text_xs()
          .font_semibold()
          .text_color(t.primary)
          .active((s) => s.bg(t.muted))
          .on_click((_e, cx) => this.retry(m, cx))
          .child("Send again"),
      );
  }

  composer(cx, win) {
    const t = cx.theme();
    const empty = this.draft.value().trim() === "";
    return h_flex()
      .gap_2()
      .px_3()
      .pt_2()
      .pb(Math.max(win.keyboard_height, win.safe_area.bottom) + 8)
      .border_t_1()
      .border_color(t.border)
      .bg(t.background)
      .child(
        Input.new(this.draft)
          .test_id("draft")
          .accessibility_label(`Message ${CONTACT.name}`)
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
        Button.new("send")
          .test_id("send")
          .accessibility_label("Send")
          .disabled(empty)
          .size_11()
          .flex_none()
          .flex()
          .items_center()
          .justify_center()
          .rounded_full()
          .bg(empty ? t.muted : t.primary)
          .when(!empty, (el) => el.active((s) => s.opacity(0.7)))
          .on_click((_e, cx) => this.send(cx))
          .child(icon("arrow_up", empty ? t.muted_foreground : t.primary_foreground)),
      );
  }

  undoBar(cx) {
    const t = cx.theme();
    return h_flex()
      .test_id("undo-bar")
      .mx_4()
      .mb_2()
      .pl_4()
      .pr_1()
      .py_1()
      .gap_3()
      .rounded_lg()
      .bg(t.foreground)
      .shadow_md()
      .child(div().role("alert").flex_1().text_sm().text_color(t.background).child("Message deleted"))
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
}

function deliveryText(m) {
  switch (m.status) {
    case "sending":
      return "Sending…";
    case "read":
      return `Read ${clock(m.at)}`;
    default:
      return "Delivered";
  }
}
