// Buttons, in a post composer: one primary commit (Publish), a quieter
// alternative (Save draft), a low-emphasis exit (Discard…), and a destructive
// commit in its own zone. Shows rest, pressed, disabled and loading states.

import { View, div, h_flex, v_flex, Button, Textarea, TextareaState } from "native-shell";
import { button, card, heading, icon, page } from "../ui.js";

const MONTHS = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];

function today() {
  const d = new Date();
  return `${MONTHS[d.getMonth()]} ${d.getDate()}`;
}

export default class ButtonsPage extends View {
  init(_props, cx) {
    this.post = TextareaState.new({ placeholder: "What’s new with the garden?", rows: 4, auto_capitalize: "sentences" });
    this.post.on("change", (_e, cx) => {
      this.state = "draft";
      cx.notify();
    });
    /** "draft" | "publishing" | "published" | "saved" */
    this.state = "draft";
  }

  release() {
    this.post.release();
  }

  publish(cx) {
    this.state = "publishing";
    cx.notify();
    // Publishing is a network request in a real app; the button shows the wait.
    cx.timer.after(1200, (cx) => {
      this.state = "published";
      this.post.set_value("");
      cx.notify();
    });
  }

  discard(cx) {
    cx.alert({
      title: "Discard this post?",
      message: "The text you wrote will be lost.",
      actions: [
        { label: "Keep editing", style: "cancel" },
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

  render(cx) {
    const t = cx.theme();
    const empty = this.post.value().trim() === "";
    const busy = this.state === "publishing";
    return page(cx, {
      title: "Buttons",
      intro:
        "A post composer. Publish is the one primary action; the others step down in emphasis by how often and how safely they are used.",
      children: [
        card(cx)
          .child(
            Textarea.new(this.post)
              .test_id("post")
              .accessibility_label("Post")
              .min_h_32()
              .px_4()
              .py_3()
              .text_base()
              .text_color(t.foreground),
          )
          .child(div().h(1).bg(t.border))
          .child(
            h_flex()
              .gap_2()
              .p_3()
              .child(
                button(cx, "discard", "Discard…", {
                  variant: "ghost",
                  disabled: empty || busy,
                  on_click: (_e, cx) => this.discard(cx),
                }),
              )
              .child(div().flex_1())
              .child(
                button(cx, "save-draft", this.state === "saved" ? "Saved" : "Save draft", {
                  variant: "outline",
                  disabled: empty || busy || this.state === "saved",
                  icon: this.state === "saved" ? "check" : undefined,
                  on_click: (_e, cx) => {
                    this.state = "saved";
                    cx.notify();
                  },
                }),
              )
              .child(
                button(cx, "publish", busy ? "Publishing…" : "Publish", {
                  variant: "primary",
                  disabled: empty,
                  loading: busy,
                  on_click: (_e, cx) => this.publish(cx),
                }),
              ),
          ),
        this.state === "published"
          ? h_flex()
              .test_id("published")
              .role("alert")
              .gap_2()
              .child(icon("check", t.foreground, 18))
              .child(div().text_sm().text_color(t.foreground).child("Published to your followers."))
          : null,
        v_flex()
          .gap_3()
          .child(
            heading(
              cx,
              "Icon buttons",
              "Shortcuts into the post: a familiar symbol, a 44 pt target and an accessible name. Attaching a photo stays unavailable until photo access is allowed.",
            ),
          )
          .child(
            h_flex()
              .gap_2()
              .children(
                [
                  ["date", "calendar", "Insert today’s date", today()],
                  ["link", "link", "Insert a link", "https://example.org/garden"],
                  ["photo", "upload", "Attach a photo", null],
                ].map(([id, name, label, text]) => {
                  const disabled = text === null || busy;
                  return Button.new(`icon-${id}`)
                    .test_id(`icon-${id}`)
                    .accessibility_label(label)
                    .disabled(disabled)
                    .size_11()
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded_lg()
                    .border_1()
                    .border_color(t.border)
                    .when(disabled, (el) => el.opacity(0.4))
                    .when(!disabled, (el) =>
                      el
                        .active((s) => s.bg(t.muted))
                        .on_click((_e, cx) => {
                          const value = this.post.value();
                          this.post.set_value(value === "" || value.endsWith(" ") ? value + text : `${value} ${text}`);
                          this.state = "draft";
                          cx.notify();
                        }),
                    )
                    .child(icon(name, disabled ? t.muted_foreground : t.foreground));
                }),
              ),
          ),
        v_flex()
          .gap_3()
          .child(heading(cx, "Danger zone", "A destructive commit is separated from everyday actions and always confirms."))
          .child(
            button(cx, "delete-account", "Delete account…", {
              variant: "destructive",
              full: true,
              on_click: (_e, cx) =>
                cx.alert({
                  title: "Delete your account?",
                  message: "Your posts and followers are removed for good. This can’t be undone.",
                  actions: [
                    { label: "Cancel", style: "cancel" },
                    { label: "Delete account", style: "destructive" },
                  ],
                }),
            }),
          ),
      ].filter(Boolean),
    });
  }
}
