// Overlays, around one shared document: each uses the smallest surface that
// fits the decision. A sheet for choices that keep the document in view, a
// dialog for a one-field form, a toast for a result that is otherwise
// invisible, and an alert for what cannot be undone.

import { View, div, h_flex, v_flex, Input, InputState } from "native-shell";
import { button, card, heading, icon, page, row } from "../ui.js";

export default class OverlaysPage extends View {
  init() {
    this.name = "Coast trip itinerary.pdf";
    this.deleted = false;
  }

  share(cx) {
    const t = cx.theme();
    const target = (id, name, label) =>
      row(cx, {
        id: `share-${id}`,
        icon: name,
        label,
        on_click: (_e, cx) => {
          window.close_sheet();
          cx.push_toast({ title: `Shared by ${label.toLowerCase()}`, level: "success", timeout: 2000 });
        },
      });
    cx.open_sheet(
      () =>
        v_flex()
          .test_id("share-sheet")
          .size_full()
          .bg(t.background)
          .px_4()
          .pt_6()
          .pb(cx.window().safe_area.bottom + 16)
          .gap_4()
          .child(
            v_flex()
              .gap_1()
              .child(div().role("header").text_lg().font_semibold().text_color(t.foreground).child(`Share “${this.name}”`))
              .child(div().text_sm().text_color(t.muted_foreground).child("People you send it to can view and download it.")),
          )
          .child(
            card(cx)
              .child(target("message", "message", "Message"))
              .child(div().h(1).ml(48).bg(t.border))
              .child(target("mail", "mail", "Mail")),
          ),
      { detents: ["medium"] },
    );
  }

  rename(cx) {
    const t = cx.theme();
    const dot = this.name.lastIndexOf(".");
    const ext = dot > 0 ? this.name.slice(dot) : "";
    const state = InputState.new({
      value: this.name.slice(0, dot > 0 ? dot : undefined),
      return_key: "done",
      auto_capitalize: "sentences",
    });
    const commit = (cx) => {
      const base = state.value().trim();
      if (base === "") return;
      this.name = base + ext;
      state.release();
      window.close_dialog();
      cx.notify();
    };
    state.on("change", (_e, cx) => cx.notify());
    state.on("submit", (_e, cx) => commit(cx));
    window.open_dialog(
      () =>
        v_flex()
          .test_id("rename-dialog")
          .w(Math.min(340, cx.window().width - 48))
          .p_5()
          .gap_4()
          .rounded_xl()
          .bg(t.background)
          .child(div().role("header").text_lg().font_semibold().text_color(t.foreground).child("Rename document"))
          .child(
            Input.new(state)
              .test_id("rename-input")
              .accessibility_label("Document name")
              .min_h_11()
              .px_3()
              .py_2()
              .rounded_lg()
              .border_1()
              .border_color(t.input)
              .text_base()
              .text_color(t.foreground),
          )
          .child(
            h_flex()
              .justify_end()
              .gap_2()
              .child(
                button(cx, "rename-cancel", "Cancel", {
                  variant: "ghost",
                  on_click: () => {
                    state.release();
                    window.close_dialog();
                  },
                }),
              )
              .child(
                button(cx, "rename-save", "Rename", {
                  variant: "primary",
                  disabled: state.value().trim() === "",
                  on_click: (_e, cx) => commit(cx),
                }),
              ),
          ),
      { on_dismiss: () => state.release() },
    );
  }

  remove(cx) {
    cx.alert({
      title: `Delete “${this.name}”?`,
      message: "It’s removed for everyone it’s shared with. This can’t be undone.",
      actions: [
        { label: "Cancel", style: "cancel" },
        {
          label: "Delete",
          style: "destructive",
          on_press: (cx) => {
            this.deleted = true;
            cx.notify();
          },
        },
      ],
    });
  }

  render(cx) {
    const t = cx.theme();
    return page(cx, {
      title: "Overlays",
      intro: "Actions on a shared document. Each opens the smallest surface that fits the decision.",
      children: this.deleted
        ? [
            v_flex()
              .test_id("deleted")
              .items_center()
              .gap_2()
              .py_12()
              .child(icon("trash", t.muted_foreground, 40))
              .child(div().text_lg().font_semibold().text_color(t.foreground).child("Document deleted"))
              .child(
                div()
                  .text_sm()
                  .text_center()
                  .text_color(t.muted_foreground)
                  .child("It’s no longer available to the people you shared it with."),
              ),
          ]
        : [
            card(cx).child(
              h_flex()
                .gap_3()
                .p_4()
                .child(
                  div()
                    .size_12()
                    .flex_none()
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded_lg()
                    .bg(t.muted)
                    .child(icon("file", t.foreground, 24)),
                )
                .child(
                  v_flex()
                    .flex_1()
                    .min_w_0()
                    .child(div().test_id("doc-name").text_base().font_medium().text_color(t.foreground).truncate().child(this.name))
                    .child(div().text_sm().text_color(t.muted_foreground).child("PDF · 1.2 MB · Shared with 3 people")),
                ),
            ),
            v_flex()
              .gap_3()
              .child(heading(cx, "Actions"))
              .child(
                card(cx)
                  .child(
                    row(cx, {
                      id: "share",
                      icon: "share",
                      label: "Share…",
                      detail: "Sheet: choices that keep the document in view",
                      on_click: (_e, cx) => this.share(cx),
                    }),
                  )
                  .child(div().h(1).ml(48).bg(t.border))
                  .child(
                    row(cx, {
                      id: "rename",
                      icon: "pencil",
                      label: "Rename…",
                      detail: "Dialog: a short, focused form",
                      on_click: (_e, cx) => this.rename(cx),
                    }),
                  )
                  .child(div().h(1).ml(48).bg(t.border))
                  .child(
                    row(cx, {
                      id: "offline",
                      icon: "download",
                      label: "Make available offline",
                      detail: "Toast: confirms a result you can’t see",
                      on_click: (_e, cx) => cx.push_toast({ title: "Available offline", level: "success", timeout: 2000 }),
                    }),
                  ),
              ),
            button(cx, "delete", "Delete document…", {
              variant: "outline",
              danger: true,
              full: true,
              icon: "trash",
              on_click: (_e, cx) => this.remove(cx),
            }),
          ],
    });
  }
}
