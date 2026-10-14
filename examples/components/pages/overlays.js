// Overlays, around one shared document: each uses the smallest surface that
// fits the decision. A sheet for choices that keep the document in view, an
// alert-style dialog for a one-field form, a toast for a result that is
// otherwise invisible, and an alert for what cannot be undone.

import { View, div, h_flex, v_flex, Button, Input, InputState } from "native-shell";
import { icon, label, page, row, section, textStyle, tile } from "../ui.js";
/** @import { Context } from "native-shell" */

export default class OverlaysPage extends View {
  init() {
    this.name = "Coast trip itinerary.pdf";
    this.deleted = false;
  }

  /** @param {Context} cx */
  share(cx) {
    /** @param {Context} cx @param {string} id @param {"message" | "mail"} name @param {string} text @param {import("native-shell").Color} color */
    const target = (cx, id, name, text, color) =>
      row(cx, {
        id: `share-${id}`,
        leading: tile(cx, name, color),
        label: text,
        on_click: (_e, cx) => {
          window.close_sheet();
          cx.push_toast({ title: `Shared in ${text}`, level: "success", timeout: 2000 });
        },
      });
    // The content is re-evaluated on theme changes; read the theme inside it.
    cx.open_sheet(
      (cx) => {
        const t = cx.theme();
        return v_flex()
          .test_id("share-sheet")
          .size_full()
          .bg(t.grouped_background)
          .px_4()
          .pt_6()
          .pb(cx.window().safe_area.bottom + 16)
          .gap_5()
          .child(
            v_flex()
              .items_center()
              .gap_1()
              .child(label(cx, "headline", t.foreground, `Share “${this.name}”`).role("header").text_center())
              .child(label(cx, "subheadline", t.muted_foreground, "People you send it to can view and download it.").text_center()),
          )
          .child(
            section(cx, {
              inset: 60,
              rows: [target(cx, "message", "message", "Messages", t.success), target(cx, "mail", "mail", "Mail", t.primary)],
            }),
          );
      },
      { detents: ["medium"] },
    );
  }

  /** @param {Context} cx */
  rename(cx) {
    const dot = this.name.lastIndexOf(".");
    const ext = dot > 0 ? this.name.slice(dot) : "";
    const state = InputState.new({
      value: this.name.slice(0, dot > 0 ? dot : undefined),
      return_key: "done",
      auto_capitalize: "sentences",
    });
    /** @param {Context} cx */
    const commit = (cx) => {
      const base = state.value().trim();
      if (base === "") return;
      this.name = base + ext;
      state.release();
      window.close_dialog();
      cx.notify();
    };
    const cancel = () => {
      state.release();
      window.close_dialog();
    };
    state.on("change", (_e, cx) => cx.notify());
    state.on("submit", (_e, cx) => commit(cx));
    // Laid out as an iOS alert with a text field: centered title and message,
    // the field, then Cancel and the commit side by side under a hairline.
    window.open_dialog(
      (cx) => {
        const t = cx.theme();
        const empty = state.value().trim() === "";
        /** @param {string} id @param {string} text @param {boolean} bold @param {boolean} disabled @param {(cx: Context) => void} run */
        const action = (id, text, bold, disabled, run) =>
          textStyle(Button.new(id), cx, bold ? "headline" : "body")
            .test_id(id)
            .disabled(disabled)
            .flex_1()
            .flex()
            .items_center()
            .justify_center()
            .min_h_11()
            .text_color(disabled ? t.muted_foreground : t.primary)
            .when(!disabled, (el) => el.active((s) => s.bg(t.secondary)).on_click((_e, cx) => run(cx)));
        return v_flex()
          .test_id("rename-dialog")
          .w("16.875rem")
          .rounded(t.radius.xl)
          .bg(t.surface)
          .overflow_hidden()
          .child(
            v_flex()
              .items_center()
              .gap_1()
              .px_4()
              .pt_5()
              .pb_4()
              .child(label(cx, "headline", t.foreground, "Rename Document").role("header"))
              .child(label(cx, "footnote", t.foreground, "Enter a new name for this document.").text_center())
              .child(
                textStyle(Input.new(state), cx, "subheadline")
                  .test_id("rename-input")
                  .accessibility_label("Document name")
                  .mt_3()
                  .w_full()
                  .min_h_9()
                  .px_2()
                  .rounded(t.radius.sm)
                  .border(0.5)
                  .border_color(t.border)
                  .bg(t.background)
                  .text_color(t.foreground),
              ),
          )
          .child(
            h_flex()
              .border_t(0.5)
              .border_color(t.border)
              .child(action("rename-cancel", "Cancel", false, false, cancel).child("Cancel"))
              .child(div().w(0.5).self_stretch().bg(t.border))
              .child(action("rename-save", "Rename", true, empty, commit).child("Rename")),
          );
      },
      { on_dismiss: () => state.release() },
    );
  }

  /** @param {Context} cx */
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
              .child(icon("trash", t.muted_foreground, 44))
              .child(label(cx, "title2", t.foreground, "Document Deleted").font_bold())
              .child(label(cx, "body", t.muted_foreground, "It’s no longer available to the people you shared it with.").text_center()),
          ]
        : [
            section(cx, {
              inset: 60,
              rows: [
                row(cx, {
                  id: "doc",
                  leading: tile(cx, "file", t.primary),
                  label: this.name,
                  detail: "PDF · 1.2 MB · Shared with 3 people",
                }),
              ],
            }),
            section(cx, {
              header: "Actions",
              inset: 52,
              rows: [
                row(cx, {
                  id: "share",
                  leading: icon("share", t.primary, 24),
                  label: "Share…",
                  detail: "Sheet: choices that keep the document in view",
                  on_click: (_e, cx) => this.share(cx),
                }),
                row(cx, {
                  id: "rename",
                  leading: icon("pencil", t.primary, 24),
                  label: "Rename…",
                  detail: "Dialog: a short, focused form",
                  on_click: (_e, cx) => this.rename(cx),
                }),
                row(cx, {
                  id: "offline",
                  leading: icon("download", t.primary, 24),
                  label: "Make Available Offline",
                  detail: "Toast: confirms a result you can’t see",
                  on_click: (_e, cx) => cx.push_toast({ title: "Available offline", level: "success", timeout: 2000 }),
                }),
              ],
            }),
            section(cx, {
              footer: "Alert: confirms what can’t be undone.",
              rows: [row(cx, { id: "delete", label: "Delete Document…", destructive: true, on_click: (_e, cx) => this.remove(cx) })],
            }),
          ],
    });
  }
}
