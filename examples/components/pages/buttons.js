// Buttons, in a post composer: one filled primary commit (Publish), a tinted
// alternative (Save Draft), a plain exit (Discard…), tinted icon
// buttons as shortcuts into the post, and a destructive row set apart. Shows
// rest, pressed, disabled and loading states.

import { View, div, h_flex, v_flex, Button, Spinner, Textarea, TextareaState } from "native-shell";
import { filledButton, icon, label, page, row, section, textButton, textStyle } from "../ui.js";
/** @import { Context } from "native-shell" */

const MONTHS = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];

function today() {
  const d = new Date();
  return `${MONTHS[d.getMonth()]} ${d.getDate()}`;
}

/** @type {{ id: string, icon: "calendar" | "link" | "photo", label: string, text: string | null }[]} */
const SHORTCUTS = [
  { id: "date", icon: "calendar", label: "Insert today’s date", text: today() },
  { id: "link", icon: "link", label: "Insert a link", text: "https://example.org/garden" },
  { id: "photo", icon: "photo", label: "Attach a photo", text: null },
];

export default class ButtonsPage extends View {
  init() {
    this.post = TextareaState.new({ placeholder: "What’s new with the garden?", rows: 5, auto_capitalize: "sentences" });
    this.post.on("change", (_e, cx) => {
      this.state = "draft";
      cx.notify();
    });
    /** @type {"draft" | "publishing" | "published" | "saved"} */
    this.state = "draft";
  }

  release() {
    this.post.release();
  }

  /** @param {Context} cx */
  publish(cx) {
    this.state = "publishing";
    cx.blur();
    cx.notify();
    // Publishing is a network request in a real app; the button shows the wait.
    cx.timer.after(1200, (cx) => {
      this.state = "published";
      this.post.set_value("");
      cx.haptic("success");
      cx.notify();
    });
  }

  /** @param {Context} cx */
  discard(cx) {
    cx.alert({
      title: "Discard this post?",
      message: "The text you wrote will be lost.",
      actions: [
        { label: "Keep Editing", style: "cancel" },
        {
          label: "Discard",
          style: "destructive",
          on_press: (cx) => {
            this.post.set_value("");
            this.state = "draft";
            cx.notify();
          },
        },
      ],
    });
  }

  /** @param {string} text @param {Context} cx */
  insert(text, cx) {
    const value = this.post.value();
    this.post.set_value(value === "" || value.endsWith(" ") ? value + text : `${value} ${text}`);
    this.state = "draft";
    cx.notify();
  }

  render(cx) {
    const t = cx.theme();
    const empty = this.post.value().trim() === "";
    const busy = this.state === "publishing";
    return page(cx, {
      title: "Buttons",
      intro:
        "A post composer. Publish is the one filled button; the others step down in emphasis by how often and how safely they are used.",
      children: [
        section(cx, {
          footer: this.state === "published" ? "Published to your followers." : null,
          footerId: "published",
          rows: [
            textStyle(Textarea.new(this.post), cx, "body")
              .test_id("post")
              .accessibility_label("Post")
              .min_h_32()
              .px_4()
              .py_3()
              .text_color(t.foreground),
          ],
        }),
        v_flex()
          .gap_3()
          .child(
            filledButton(cx, "publish", busy ? "Publishing…" : "Publish", {
              disabled: empty || busy,
              leading: busy ? Spinner.new().size_4() : undefined,
              on_click: (_e, cx) => this.publish(cx),
            }),
          )
          .child(
            filledButton(cx, "save-draft", this.state === "saved" ? "Saved" : "Save Draft", {
              tone: "tinted",
              disabled: empty || busy || this.state === "saved",
              leading: this.state === "saved" ? icon("check", t.primary, 18) : undefined,
              on_click: (_e, cx) => {
                this.state = "saved";
                cx.haptic("success");
                cx.notify();
              },
            }),
          )
          .child(
            div()
              .flex()
              .justify_center()
              .child(textButton(cx, "discard", "Discard…", { disabled: empty || busy, on_click: (_e, cx) => this.discard(cx) })),
          ),
        v_flex()
          .gap_2()
          .child(label(cx, "footnote", t.muted_foreground, "SHORTCUTS").px_4())
          .child(
            h_flex()
              .gap_3()
              .children(
                SHORTCUTS.map(({ id, icon: name, label: text, text: insert }) => {
                  const disabled = insert === null || busy;
                  return Button.new(`icon-${id}`)
                    .test_id(`icon-${id}`)
                    .accessibility_label(text)
                    .disabled(disabled)
                    .size_11()
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded_full()
                    .bg(t.accent)
                    .when(disabled, (el) => el.opacity(0.4))
                    .when(!disabled, (el) => el.active((s) => s.opacity(0.5)).on_click((_e, cx) => this.insert(insert ?? "", cx)))
                    .child(icon(name, t.accent_foreground, 20));
                }),
              ),
          )
          .child(label(cx, "footnote", t.muted_foreground, "Attaching a photo stays unavailable until photo access is allowed.").px_4()),
        section(cx, {
          footer: "Your posts and followers are removed for good.",
          rows: [
            row(cx, {
              id: "delete-account",
              label: "Delete Account…",
              destructive: true,
              on_click: (_e, cx) =>
                cx.alert({
                  title: "Delete your account?",
                  message: "Your posts and followers are removed for good. This can’t be undone.",
                  actions: [
                    { label: "Cancel", style: "cancel" },
                    { label: "Delete Account", style: "destructive" },
                  ],
                }),
            }),
          ],
        }),
      ],
    });
  }
}
