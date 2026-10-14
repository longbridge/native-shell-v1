// Sliders and progress, in a photo app: sliders for continuous values whose
// result is shown next to them, a spinner while the length of a wait is
// unknown, and a progress bar once it is, with a way to stop it.

import { View, div, h_flex, v_flex, Slider, Progress, Spinner } from "native-shell";
import { button, card, heading, icon, page } from "../ui.js";

const PHOTOS = 12;

export default class ProgressPage extends View {
  init() {
    this.volume = 40;
    this.timer = 15;
    /** "idle" | "preparing" | "uploading" | "done" */
    this.upload = "idle";
    this.sent = 0;
    this.task = null;
  }

  release() {
    this.task?.cancel();
  }

  start(cx) {
    this.upload = "preparing";
    this.sent = 0;
    cx.notify();
    // Stands in for the app reading and compressing the photos, then sending them.
    this.task = cx.timer.after(900, (cx) => {
      this.upload = "uploading";
      cx.notify();
      this.task = cx.timer.every(350, (cx) => {
        this.sent += 1;
        if (this.sent >= PHOTOS) {
          this.task.cancel();
          this.upload = "done";
          cx.haptic("success");
        }
        cx.notify();
      });
    });
  }

  cancel(cx) {
    this.task?.cancel();
    this.upload = "idle";
    cx.notify();
  }

  render(cx) {
    const t = cx.theme();
    const sliderRow = (id, name, label, value, display, min, max, step, set) =>
      v_flex()
        .gap_2()
        .px_4()
        .py_3()
        .child(
          h_flex()
            .gap_3()
            .child(icon(name, t.muted_foreground))
            .child(div().flex_1().text_base().text_color(t.foreground).child(label))
            .child(div().test_id(`${id}-value`).text_base().text_color(t.muted_foreground).child(display)),
        )
        .child(
          Slider.new(id)
            .test_id(id)
            .accessibility_label(label)
            .accessibility_value(display)
            .min(min)
            .max(max)
            .step(step)
            .value(value)
            .on_change((v, cx) => {
              set(v);
              cx.notify();
            }),
        );
    return page(cx, {
      title: "Sliders and progress",
      intro: "A photo app’s playback settings and an upload. The value of a slider is always written next to it.",
      children: [
        v_flex()
          .gap_3()
          .child(heading(cx, "Slideshow"))
          .child(
            card(cx)
              .child(
                sliderRow(
                  "volume",
                  "volume",
                  "Music volume",
                  this.volume,
                  `${Math.round(this.volume)}%`,
                  0,
                  100,
                  1,
                  (v) => (this.volume = v),
                ),
              )
              .child(div().h(1).ml_4().bg(t.border))
              .child(
                sliderRow(
                  "sleep",
                  "sliders",
                  "Stop after",
                  this.timer,
                  this.timer === 0 ? "Never" : `${this.timer} min`,
                  0,
                  60,
                  5,
                  (v) => (this.timer = v),
                ),
              ),
          ),
        v_flex()
          .gap_3()
          .child(heading(cx, "Upload", "Trip photos to the shared album “Coast, October”."))
          .child(this.uploadCard(cx)),
      ],
    });
  }

  uploadCard(cx) {
    const t = cx.theme();
    const status = {
      idle: `${PHOTOS} photos · 48.6 MB`,
      preparing: "Preparing photos…",
      uploading: `${this.sent} of ${PHOTOS} uploaded`,
      done: `${PHOTOS} photos uploaded`,
    }[this.upload];
    return card(cx)
      .test_id("upload")
      .gap_3()
      .p_4()
      .child(
        h_flex()
          .gap_3()
          .child(
            div()
              .size_10()
              .flex_none()
              .flex()
              .items_center()
              .justify_center()
              .rounded_lg()
              .bg(t.muted)
              .child(
                this.upload === "preparing"
                  ? Spinner.new().test_id("upload-spinner")
                  : icon(this.upload === "done" ? "check" : "upload", t.foreground),
              ),
          )
          .child(
            v_flex()
              .flex_1()
              .min_w_0()
              .child(div().text_base().font_medium().text_color(t.foreground).child("Trip photos"))
              .child(div().test_id("upload-status").text_sm().text_color(t.muted_foreground).child(status)),
          ),
      )
      .when(this.upload === "uploading" || this.upload === "preparing", (el) =>
        el.child(
          Progress.new("upload-progress")
            .test_id("upload-progress")
            .accessibility_label("Upload progress")
            .when(this.upload === "preparing", (p) => p.indeterminate())
            .when(this.upload === "uploading", (p) => p.value((this.sent / PHOTOS) * 100)),
        ),
      )
      .child(
        this.upload === "idle" || this.upload === "done"
          ? button(cx, "upload-start", this.upload === "done" ? "Upload again" : "Upload", {
              variant: this.upload === "done" ? "outline" : "primary",
              full: true,
              icon: "upload",
              on_click: (_e, cx) => this.start(cx),
            })
          : button(cx, "upload-cancel", "Cancel upload", { variant: "outline", full: true, on_click: (_e, cx) => this.cancel(cx) }),
      );
  }
}
