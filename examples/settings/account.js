// The account page: edit the profile, or sign in when signed out.
//
// Edits are a draft until Save, so leaving with unsaved changes asks first
// (the back button and the system back gesture alike). Fields validate when
// the user commits, and the message sits under the field it is about.

import { View, div, v_flex, Button, Input, InputState } from "native-shell";
import { avatar, navBar } from "./ui.js";

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
        { label: "Keep editing", style: "cancel" },
        {
          label: "Discard changes",
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
          label: "Sign out",
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
    const canSave = this.complete && (this.dirty || this.signingIn);
    return v_flex()
      .size_full()
      .bg(t.background)
      .line_height(1.375)
      .child(
        navBar(cx, {
          title: this.signingIn ? "Sign in" : "Account",
          back: { label: "Settings", on_click: (_e, cx) => this.leave(cx) },
          trailing: Button.new("save")
            .test_id("save")
            .disabled(!canSave)
            .px_3()
            .py_2()
            .rounded_md()
            .hit_slop(4)
            .text_base()
            .font_semibold()
            .text_color(canSave ? t.primary : t.muted_foreground)
            .when(canSave, (el) => el.active((s) => s.opacity(0.5)).on_click((_e, cx) => this.commit(cx)))
            .child(this.signingIn ? "Sign in" : "Save"),
        }),
      )
      .child(
        v_flex()
          .flex_1()
          .overflow_y_scroll()
          .px_4()
          .pt_6()
          .pb(Math.max(cx.window().keyboard_height, cx.window().safe_area.bottom) + 24)
          .gap_6()
          .child(
            v_flex()
              .items_center()
              .gap_3()
              .child(avatar(cx, this.name.value().trim() || "?", 72))
              .when(this.signingIn, (el) =>
                el.child(
                  div()
                    .text_sm()
                    .text_center()
                    .text_color(t.muted_foreground)
                    .child("Sign in to keep your settings in sync on every device you use."),
                ),
              ),
          )
          .child(
            v_flex()
              .gap_4()
              .child(this.field(cx, "name", "Name", this.name, null))
              .child(this.field(cx, "email", "Email", this.email, this.error)),
          )
          .when(!this.signingIn, (el) =>
            el.child(
              Button.new("sign-out")
                .test_id("sign-out")
                .flex()
                .items_center()
                .justify_center()
                .min_h_12()
                .py_3()
                .rounded_xl()
                .border_1()
                .border_color(t.border)
                .bg(t.surface)
                .text_base()
                .text_color(t.destructive)
                .active((s) => s.bg(t.muted))
                .on_click((_e, cx) => this.signOut(cx))
                .child("Sign out…"),
            ),
          ),
      );
  }

  field(cx, id, label, state, error) {
    const t = cx.theme();
    return v_flex()
      .gap_1p5()
      .child(div().text_sm().font_medium().text_color(t.foreground).child(label))
      .child(
        Input.new(state)
          .test_id(id)
          .accessibility_label(label)
          .min_h_11()
          .px_3()
          .py_2()
          .rounded_lg()
          .border_1()
          .border_color(error ? t.destructive : t.input)
          .bg(t.background)
          .text_base()
          .text_color(t.foreground),
      )
      .when(Boolean(error), (el) => el.child(div().test_id(`${id}-error`).role("alert").text_sm().text_color(t.destructive).child(error)));
  }
}
