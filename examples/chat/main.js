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
import { avatar, icon, label, row, section, textStyle } from "./ui.js";
/** @import { Message as Msg } from "./conversation.js" */
/** @import { Context } from "native-shell" */

const UNDO_MS = 5000;

export default class Chat extends View {
  init() {
    /** @type {Msg[]} */
    this.messages = recentMessages();
    /** "more" | "loading" | "done" | "error" */
    this.history = "more";
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

  /**
   * Deleting is reversible for a few seconds: the toast's Undo puts the
   * message back where it was.
   * @param {Msg} message @param {Context} cx
   */
  remove(message, cx) {
    const index = this.messages.findIndex((m) => m.id === message.id);
    this.setMessages(
      this.messages.filter((m) => m.id !== message.id),
      "within",
    );
    cx.push_toast({
      id: "undo",
      title: "Message deleted",
      timeout: UNDO_MS,
      action: {
        label: "Undo",
        on_click: (cx) => {
          this.setMessages([...this.messages.slice(0, index), message, ...this.messages.slice(index)], "within");
          cx.notify();
        },
      },
    });
    cx.notify();
  }

  /**
   * A message's actions in a medium sheet: what it is, then the actions as an
   * inset grouped list, the destructive one apart and in red.
   * @param {Msg} message @param {Context} cx
   */
  openActions(message, cx) {
    cx.haptic("selection");
    /** @param {Context} cx @param {string} id @param {"refresh" | "copy" | "trash"} name @param {string} text @param {(cx: Context) => void} run @param {boolean} [destructive] */
    const action = (cx, id, name, text, run, destructive = false) =>
      row(cx, {
        id,
        leading: icon(name, destructive ? cx.theme().destructive : cx.theme().primary, 22),
        label: text,
        destructive,
        on_click: (_e, cx) => {
          window.close_sheet();
          run(cx);
        },
      });
    // The content is re-evaluated on theme changes; read the theme inside it.
    cx.open_sheet(
      (cx) => {
        const t = cx.theme();
        const from = `${message.from === "me" ? "You" : CONTACT.name} · ${dayLabel(message.at)}, ${clock(message.at)}`;
        return v_flex()
          .test_id("actions")
          .size_full()
          .bg(t.grouped_background)
          .px_4()
          .pt_6()
          .pb(cx.window().safe_area.bottom + 16)
          .gap_5()
          .child(
            v_flex()
              .gap_1()
              .px_4()
              .child(label(cx, "footnote", t.muted_foreground, from))
              .child(label(cx, "body", t.foreground, summary(message)).line_clamp(3)),
          )
          .child(
            section(cx, {
              inset: 54,
              rows: [
                message.status === "failed" && action(cx, "action-retry", "refresh", "Send Again", (cx) => this.retry(message, cx)),
                message.text !== undefined &&
                  action(cx, "action-copy", "copy", "Copy", (cx) => {
                    cx.write_to_clipboard(message.text ?? "");
                    // Some platforms confirm a copy themselves; a second confirmation would repeat it.
                    if (!cx.window().clipboard_feedback) cx.push_toast({ title: "Copied", level: "success", timeout: 1500 });
                  }),
              ],
            }),
          )
          .child(
            section(cx, { inset: 54, rows: [action(cx, "action-delete", "trash", "Delete", (cx) => this.remove(message, cx), true)] }),
          );
      },
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
      .child(this.composer(cx));
  }

  /** As in Messages: the contact's avatar centered over the name, on a bar with a hairline. @param {Context} cx */
  header(cx) {
    const t = cx.theme();
    return v_flex()
      .items_center()
      .gap_1()
      .px_4()
      .pt(cx.window().safe_area.top + 6)
      .pb_2()
      .bg(t.background)
      .border_b(0.5)
      .border_color(t.border)
      .child(avatar(cx, CONTACT.name, 40))
      .child(
        v_flex()
          .items_center()
          .child(label(cx, "footnote", t.foreground, CONTACT.name).role("header").font_semibold().truncate())
          .child(label(cx, "caption2", t.muted_foreground, CONTACT.status)),
      );
  }

  /** @param {any} row @param {Context} cx */
  row(row, cx) {
    switch (row.kind) {
      case "history":
        return this.historyRow(cx);
      case "day":
        return new Marker(row.key).alignment("center").pt_6().pb_2().child(div().font_semibold().child(row.label)).child(clock(row.at));
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
        return box.child(
          new Marker("history-loading").test_id("history-loading").alignment("center").loading(true).child("Loading earlier messages…"),
        );
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
          textStyle(Button.new("load-earlier"), cx, "subheadline")
            .test_id("load-earlier")
            .flex()
            .items_center()
            .gap_2()
            .px_4()
            .py_2()
            .rounded_full()
            .border_1()
            .border_color(t.border)
            .text_color(t.primary)
            .active((s) => s.bg(t.muted))
            .on_click((_e, cx) => this.loadEarlier(cx))
            .child(icon("history", t.primary, 16))
            .child(this.history === "error" ? "Couldn’t Load. Try Again" : "Load Earlier Messages"),
        );
    }
  }

  /** @param {{ message: Msg, joined: boolean, tail: boolean, showStatus: boolean, last: boolean }} row @param {Context} cx */
  messageRow({ message: m, joined, tail, showStatus, last }, cx) {
    const mine = m.from === "me";
    const failed = m.status === "failed";
    return (
      new Message()
        .key(m.id)
        .alignment(mine ? "end" : "start")
        .px_4()
        .pt(joined ? 2 : 12)
        // The newest message keeps a gap above the composer.
        .when(last, (el) => el.pb_3())
        .when(failed, (el) => el.footer(this.failedLine(m, cx)))
        .when(showStatus && !failed, (el) => el.footer(div().test_id("delivery").child(deliveryText(m))))
        .child(m.attachment ? this.attachment(m, cx) : this.bubble(m, mine, tail))
    );
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
    return textStyle(h_flex(), cx, "footnote")
      .test_id(`failed-${m.id}`)
      .gap_1()
      .child(icon("alert", t.destructive, 14))
      .child(div().text_color(t.destructive).child("Not delivered"))
      .child(
        Button.new(`retry-${m.id}`)
          .test_id(`retry-${m.id}`)
          .accessibility_label("Send again")
          .ml_1()
          .px_1()
          .py_1()
          .hit_slop(12)
          .font_semibold()
          .text_color(t.primary)
          .active((s) => s.opacity(0.4))
          .on_click((_e, cx) => this.retry(m, cx))
          .child("Send Again"),
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
        textStyle(Input.new(this.draft), cx, "body")
          .test_id("draft")
          .accessibility_label(`Message ${CONTACT.name}`)
          .flex_1()
          .min_h_11()
          .px_4()
          .py_2()
          .rounded_full()
          .border(0.5)
          .border_color(t.border)
          .bg(t.background)
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
