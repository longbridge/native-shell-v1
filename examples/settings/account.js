// The account page: edit the profile, or sign in when signed out.
//
// Edits are a draft until Save, so leaving with unsaved changes asks first
// (the back button and the system back gesture alike). Fields validate when
// the user commits, and the message sits under the field it is about.

import { View, h_flex, v_flex, Input, InputState } from "native-shell";
import { avatar, label, navBar, row, section, textButton, textStyle } from "./ui.js";
/** @import { Context } from "native-shell" */

const EMAIL = /^[^\s@]+@[^\s@]+\.[^\s@]+$/;

export default class AccountPage extends View {
  init(props, cx) {
    this.props = props;
    this.signingIn = props.account === null;
    this.error = null;
    this.name = InputState.new({
      value: props.account?.name ?? "",
      placeholder: "Full name",
      return_key: "next",
      auto_capitalize: "words",
    });
    this.email = InputState.new({
      value: props.account?.email ?? "",
      placeholder: "name@example.org",
      keyboard: "email",
      return_key: "done",
      auto_capitalize: "none",
      content_type: "email",
    });
    for (const field of [this.name, this.email]) field.on("change", (_e, cx) => this.edited(cx));
    this.name.on("submit", () => this.email.focus());
    this.email.on("submit", (_e, cx) => this.commit(cx));
    cx.on_back((cx) => this.leave(cx));
  }

  get dirty() {
    const a = this.props.account;
    return this.name.value().trim() !== (a?.name ?? "") || this.email.value().trim() !== (a?.email ?? "");
  }

  get complete() {
    return this.name.value().trim() !== "" && this.email.value().trim() !== "";
  }

  edited(cx) {
    this.error = null;
    // The system back gesture pops at once only while nothing would be lost.
    cx.set_can_go_back(!this.dirty);
    cx.notify();
  }

  commit(cx) {
    if (!this.complete || (!this.dirty && !this.signingIn)) return;
    const email = this.email.value().trim();
    if (!EMAIL.test(email)) {
      this.error = "Enter an email address like name@example.org.";
      cx.haptic("error");
      cx.notify();
      return;
    }
    this.props.save({ name: this.name.value().trim(), email }, cx);
    cx.set_can_go_back(true);
    cx.navigator.pop();
  }

  leave(cx) {
    if (!this.dirty) {
      cx.navigator.pop();
      return;
    }
    cx.alert({
      title: "Discard changes?",
      message: "Your edits to the account haven’t been saved.",
      actions: [
        { label: "Keep Editing", style: "cancel" },
        {
          label: "Discard Changes",
          style: "destructive",
          on_press: (cx) => {
            cx.set_can_go_back(true);
            cx.navigator.pop();
          },
        },
      ],
    });
  }

  signOut(cx) {
    cx.alert({
      title: `Sign out of ${this.props.account.email}?`,
      message: "Your settings stay on this device.",
      actions: [
        { label: "Cancel", style: "cancel" },
        {
          label: "Sign Out",
          style: "destructive",
          on_press: (cx) => {
            this.props.signOut(cx);
            cx.set_can_go_back(true);
            cx.navigator.pop();
          },
        },
      ],
    });
  }

  release() {
    this.name.release();
    this.email.release();
  }

  render(cx) {
    const t = cx.theme();
    const win = cx.window();
    const canSave = this.complete && (this.dirty || this.signingIn);
    return v_flex()
      .size_full()
      .bg(t.grouped_background)
      .child(
        navBar(cx, {
          title: this.signingIn ? "Sign In" : "Account",
          back: { label: "Settings", on_click: (_e, cx) => this.leave(cx) },
          trailing: textButton(cx, "save", this.signingIn ? "Sign In" : "Save", {
            emphasized: true,
            disabled: !canSave,
            on_click: (_e, cx) => this.commit(cx),
          }),
        }),
      )
      .child(
        v_flex()
          .flex_1()
          .overflow_y_scroll()
          .px_4()
          .pt_6()
          .pb(Math.max(win.keyboard_height, win.safe_area.bottom) + 24)
          .gap_8()
          .child(
            v_flex()
              .items_center()
              .gap_3()
              .child(avatar(cx, this.name.value().trim() || "?", 80))
              .when(this.signingIn, (el) =>
                el.child(
                  label(
                    cx,
                    "subheadline",
                    t.muted_foreground,
                    "Sign in to keep your settings in sync on every device you use.",
                  ).text_center(),
                ),
              ),
          )
          .child(
            section(cx, {
              footer: this.error,
              footerColor: t.destructive,
              footerId: "email-error",
              rows: [this.field(cx, "name", "Name", this.name, false), this.field(cx, "email", "Email", this.email, this.error !== null)],
            }),
          )
          .when(!this.signingIn, (el) =>
            el.child(
              section(cx, {
                rows: [row(cx, { id: "sign-out", label: "Sign Out…", destructive: true, on_click: (_e, cx) => this.signOut(cx) })],
              }),
            ),
          ),
      );
  }

  /**
   * A form row: the label in a fixed column, the field filling the rest, as
   * iOS forms align their values.
   * @param {Context} cx @param {string} id @param {string} text @param {InputState} state @param {boolean} invalid
   */
  field(cx, id, text, state, invalid) {
    const t = cx.theme();
    return h_flex()
      .gap_3()
      .px_4()
      .min_h_11()
      .child(
        label(cx, "body", invalid ? t.destructive : t.foreground, text)
          .w_20()
          .flex_none(),
      )
      .child(textStyle(Input.new(state), cx, "body").test_id(id).accessibility_label(text).flex_1().min_h_11().text_color(t.foreground));
  }
}
