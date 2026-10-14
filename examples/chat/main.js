// Chat: one conversation thread.
//
// The thread reads bottom-up like every messenger: it opens at the newest
// message, the composer sits under it and always above the keyboard, Return
// sends. Messages from one sender a few minutes apart form a burst (tight
// spacing, one tail), days and unread messages are marked, and delivery state
// is shown once, under your latest message. Photos and files travel with
// their upload state. A tap or a long press on a message opens its actions,
// so nothing depends on discovering the gesture. See README.md.
//
// The thread is built from gpui-component's conversation parts: a
// `MessageScroller` (a bottom-aligned list that keeps the newest row in view
// as messages arrive and the keyboard rises), `Message`, `Bubble`,
// `Attachment` and `Marker`.

import { View, div, h_flex, v_flex, image, Button, Input, InputState } from "native-shell";
import { Attachment, Bubble, Marker, Message, MessageScroller, MessageScrollerState } from "gpui-component";
import { CONTACT, recentMessages, fetchEarlier, clock, dayLabel, sameBurst, summary } from "./conversation.js";
import { avatar, icon } from "./ui.js";
/** @import { Message as Msg } from "./conversation.js" */
/** @import { Context } from "native-shell" */

const UNDO_MS = 5000;

export default class Chat extends View {
  init() {
    /** @type {Msg[]} */
    this.messages = recentMessages();
    /** "more" | "loading" | "done" | "error" */
    this.history = "more";
    this.undo = null;
    this.nextId = 1;
    this.scroller = MessageScrollerState(this.rows().length);
    this.draft = InputState.new({ placeholder: "Message", return_key: "send", auto_capitalize: "sentences" });
    this.draft.on("change", (_e, cx) => cx.notify());
    this.draft.on("submit", (_e, cx) => this.send(cx));
  }

  rows() {
    /** @type {any[]} */
    const rows = [{ key: "history", kind: "history" }];
    const lastMine = this.messages.findLastIndex((m) => m.from === "me");
    const firstUnread = this.messages.findIndex((m) => m.unread);
    this.messages.forEach((m, i) => {
      const prev = this.messages[i - 1];
      const label = dayLabel(m.at);
      if (!prev || dayLabel(prev.at) !== label) rows.push({ key: `day-${label}`, kind: "day", label, at: m.at });
      if (i === firstUnread) {
        rows.push({ key: "unread", kind: "unread", count: this.messages.filter((x) => x.unread).length });
      }
      rows.push({
        key: m.id,
        kind: "message",
        message: m,
        joined: sameBurst(prev, m) && dayLabel(prev.at) === label && i !== firstUnread,
        tail: !sameBurst(m, this.messages[i + 1]),
        showStatus: i === lastMine,
        last: i === this.messages.length - 1,
      });
    });
    return rows;
  }

  /**
   * Replaces the messages and tells the scroller how the rows moved: older
   * history arrives above, new messages below.
   * @param {Msg[]} messages
   * @param {"above" | "below" | "within"} where
   */
  setMessages(messages, where) {
    const before = this.rows().length;
    this.messages = messages;
    const after = this.rows().length;
    if (where === "above") this.scroller.prepend(after - before);
    else if (where === "below") this.scroller.append(after - before);
    else this.scroller.splice(0, before, after);
  }

  /** @param {string} id @param {Partial<Msg>} change */
  patch(id, change) {
    this.messages = this.messages.map((m) => (m.id === id ? { ...m, ...change } : m));
  }

  /** @param {Context} cx */
  send(cx) {
    const text = this.draft.value().trim();
    if (text === "") return;
    /** @type {Msg} */
    const message = { id: `new-${this.nextId++}`, from: "me", text, at: Date.now(), status: "sending" };
    this.setMessages([...this.messages, message], "below");
    this.draft.set_value("");
    // Sending from further up the thread brings you back to it.
    this.scroller.scroll_to_end();
    cx.notify();
    this.deliver(message.id, cx);
  }

  /**
   * A stand-in for the network round trip that confirms delivery.
   * @param {string} id @param {Context} cx
   */
  deliver(id, cx) {
    cx.timer.after(700, (cx) => {
      this.patch(id, { status: "sent" });
      cx.notify();
    });
  }

  /**
   * A stand-in for an upload: progress in steps, then delivered.
   * @param {Msg} message @param {Context} cx
   */
  upload(message, cx) {
    const attachment = message.attachment;
    if (attachment === undefined) return;
    let progress = 0;
    const task = cx.timer.every(150, (cx) => {
      progress = Math.min(100, progress + 20);
      const done = progress === 100;
      this.patch(message.id, { attachment: { ...attachment, progress: done ? undefined : progress }, status: done ? "sent" : "sending" });
      if (done) task.cancel();
      cx.notify();
    });
  }

  /** @param {Msg} message @param {Context} cx */
  retry(message, cx) {
    if (message.attachment) {
      this.patch(message.id, { status: "sending", attachment: { ...message.attachment, progress: 0 } });
      this.upload(message, cx);
    } else {
      this.patch(message.id, { status: "sending" });
      this.deliver(message.id, cx);
    }
    cx.notify();
  }

  /** @param {Context} cx */
  loadEarlier(cx) {
    this.history = "loading";
    cx.notify();
    cx.spawn(async (cx) => {
      try {
        const page = await fetchEarlier(cx);
        this.setMessages([...page.messages, ...this.messages], "above");
        this.history = page.more ? "more" : "done";
      } catch {
        this.history = "error";
      }
      cx.notify();
    });
  }

  /** @param {Msg} message @param {Context} cx */
  remove(message, cx) {
    const index = this.messages.findIndex((m) => m.id === message.id);
    this.setMessages(
      this.messages.filter((m) => m.id !== message.id),
      "within",
    );
    this.undo?.timer.cancel();
    const timer = cx.timer.after(UNDO_MS, (cx) => {
      this.undo = null;
      cx.notify();
    });
    this.undo = { message, index, timer };
    cx.notify();
  }

  /** @param {Context} cx */
  restore(cx) {
    if (!this.undo) return;
    const { message, index, timer } = this.undo;
    timer.cancel();
    this.setMessages([...this.messages.slice(0, index), message, ...this.messages.slice(index)], "within");
    this.undo = null;
    cx.notify();
  }

  /** @param {Msg} message @param {Context} cx */
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
              .child(div().text_base().text_color(t.foreground).line_clamp(3).child(summary(message))),
          )
          .child(div().h(1).mx_4().mb_1().bg(t.border))
          .when(message.status === "failed", (el) =>
            el.child(action("action-retry", "refresh", "Send again", t.foreground, (cx) => this.retry(message, cx))),
          )
          .when(message.text !== undefined, (el) =>
            el.child(
              action("action-copy", "copy", "Copy text", t.foreground, (cx) => {
                cx.write_to_clipboard(message.text ?? "");
                // Android shows its own clipboard confirmation; a second one would repeat it.
                if (cx.window().platform !== "android") cx.push_toast({ title: "Copied", level: "success", timeout: 1500 });
              }),
            ),
          )
          .child(action("action-delete", "trash", "Delete for me", t.destructive, (cx) => this.remove(message, cx))),
      { detents: ["medium"] },
    );
  }

  render(cx) {
    const t = cx.theme();
    const rows = this.rows();
    return v_flex()
      .size_full()
      .bg(t.background)
      .child(this.header(cx))
      .child(
        new MessageScroller("messages", this.scroller, (i) => this.row(rows[i], cx))
          .test_id("messages")
          .jump_button_label("Latest messages")
          .estimated_row_height(60)
          .flex_1(),
      )
      .when(this.undo !== null, (el) => el.child(this.undoBar(cx)))
      .child(this.composer(cx));
  }

  /** @param {Context} cx */
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

  /** @param {any} row @param {Context} cx */
  row(row, cx) {
    switch (row.kind) {
      case "history":
        return this.historyRow(cx);
      case "day":
        return new Marker(row.key)
          .alignment("center")
          .pt_6()
          .pb_2()
          .child(div().font_semibold().child(row.label))
          .child(clock(row.at));
      case "unread":
        return new Marker("unread")
          .test_id("unread")
          .variant("separator")
          .px_4()
          .pt_6()
          .pb_2()
          .text_color(cx.theme().primary)
          .child(`${row.count} unread ${row.count === 1 ? "message" : "messages"}`);
      default:
        return this.messageRow(row, cx);
    }
  }

  /** @param {Context} cx */
  historyRow(cx) {
    const t = cx.theme();
    const box = h_flex().key("history").justify_center().px_4().pt_4().pb_2().min_h_12();
    switch (this.history) {
      case "loading":
        return box.child(new Marker("history-loading").test_id("history-loading").alignment("center").loading(true).child("Loading earlier messages…"));
      case "done":
        return box.child(
          new Marker("history-start")
            .test_id("history-start")
            .alignment("center")
            .text_center()
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

  /** @param {{ message: Msg, joined: boolean, tail: boolean, showStatus: boolean, last: boolean }} row @param {Context} cx */
  messageRow({ message: m, joined, tail, showStatus, last }, cx) {
    const mine = m.from === "me";
    const failed = m.status === "failed";
    return new Message()
      .key(m.id)
      .alignment(mine ? "end" : "start")
      .px_4()
      .pt(joined ? 2 : 12)
      // The newest message keeps a gap above the composer.
      .when(last, (el) => el.pb_3())
      .when(failed, (el) => el.footer(this.failedLine(m, cx)))
      .when(showStatus && !failed, (el) => el.footer(div().test_id("delivery").child(deliveryText(m))))
      .child(m.attachment ? this.attachment(m, cx) : this.bubble(m, mine, tail));
  }

  /** @param {Msg} m @param {boolean} mine @param {boolean} tail */
  bubble(m, mine, tail) {
    return new Bubble()
      .variant(mine ? "filled" : "secondary")
      .tail(tail)
      .test_id(`bubble-${m.id}`)
      .role("button")
      .accessibility_label(`${mine ? "You" : CONTACT.name}, ${clock(m.at)}: ${m.text}`)
      .accessibility_hint("Shows actions")
      .active((s) => s.opacity(0.7))
      .on_click((_e, cx) => this.openActions(m, cx))
      .on_long_press((_e, cx) => this.openActions(m, cx))
      .child(m.text ?? "");
  }

  /** @param {Msg} m @param {Context} cx */
  attachment(m, cx) {
    const a = /** @type {import("./conversation.js").Attachment} */ (m.attachment);
    const status = m.status === "failed" ? "failed" : m.status === "sending" ? "uploading" : "complete";
    const el = new Attachment(`attachment-${m.id}`)
      .test_id(`attachment-${m.id}`)
      .status(status)
      .accessibility_label(`${m.from === "me" ? "You" : CONTACT.name}, ${clock(m.at)}: ${summary(m)}`)
      .accessibility_hint("Shows actions")
      .active((s) => s.opacity(0.7))
      .on_click((_e, cx) => this.openActions(m, cx))
      .on_long_press((_e, cx) => this.openActions(m, cx));
    if (a.progress !== undefined) el.progress(a.progress);
    if (a.kind === "image" && a.src) return el.axis("vertical").size("large").media(image(a.src));
    return el.title(a.name).description(status === "failed" ? "Upload failed" : a.detail);
  }

  /** @param {Msg} m @param {Context} cx */
  failedLine(m, cx) {
    const t = cx.theme();
    return h_flex()
      .test_id(`failed-${m.id}`)
      .gap_1()
      .child(icon("alert", t.destructive, 14))
      .child(div().text_color(t.destructive).child("Not delivered"))
      .child(
        Button.new(`retry-${m.id}`)
          .test_id(`retry-${m.id}`)
          .accessibility_label("Send again")
          .ml_1()
          .px_2()
          .py_1()
          .rounded_md()
          .hit_slop(10)
          .font_semibold()
          .text_color(t.primary)
          .active((s) => s.bg(t.muted))
          .on_click((_e, cx) => this.retry(m, cx))
          .child("Send again"),
      );
  }

  /** @param {Context} cx */
  composer(cx) {
    const t = cx.theme();
    const win = cx.window();
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

  /** @param {Context} cx */
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

/** @param {Msg} m */
function deliveryText(m) {
  switch (m.status) {
    case "sending":
      return m.attachment ? "Uploading…" : "Sending…";
    case "read":
      return `Read ${clock(m.at)}`;
    default:
      return "Delivered";
  }
}
