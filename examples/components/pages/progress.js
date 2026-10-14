// Sliders and progress, in a photo app: sliders for continuous values with
// their result written beside them, a spinner while the length of a wait is
// unknown, then a progress bar once it is, with a way to stop it.

import { View, h_flex, v_flex, Slider, Progress, Spinner } from "native-shell";
import { filledButton, icon, label, page, section, textButton, tile } from "../ui.js";
/** @import { Context, Task } from "native-shell" */

const PHOTOS = 12;

export default class ProgressPage extends View {
  init() {
    this.volume = 40;
    this.timer = 15;
    /** @type {"idle" | "preparing" | "uploading" | "done"} */
    this.upload = "idle";
    this.sent = 0;
    /** @type {Task | null} */
    this.task = null;
  }

  release() {
    this.task?.cancel();
  }

  /** @param {Context} cx */
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
          this.task?.cancel();
          this.upload = "done";
          cx.haptic("success");
        }
        cx.notify();
      });
    });
  }

  /** @param {Context} cx */
  cancel(cx) {
    this.task?.cancel();
    this.upload = "idle";
    cx.notify();
  }

  render(cx) {
    const t = cx.theme();
    /**
     * @param {string} id @param {string} text @param {number} value @param {string} display
     * @param {number} min @param {number} max @param {number} step @param {(v: number) => void} set
     */
    const sliderRow = (id, text, value, display, min, max, step, set) =>
      v_flex()
        .gap_1()
        .px_4()
        .py_3()
        .child(
          h_flex()
            .justify_between()
            .child(label(cx, "body", t.foreground, text))
            .child(label(cx, "body", t.muted_foreground, display).test_id(`${id}-value`)),
        )
        .child(
          h_flex()
            .gap_3()
            .child(icon("speaker_low", t.muted_foreground, 16))
            .child(
              Slider.new(id)
                .test_id(id)
                .accessibility_label(text)
                .accessibility_value(display)
                .flex_1()
                .min(min)
                .max(max)
                .step(step)
                .value(value)
                .on_change((v, cx) => {
                  set(v);
                  cx.notify();
                }),
            )
            .child(icon("speaker_high", t.muted_foreground, 18)),
        );
    return page(cx, {
      title: "Sliders and Progress",
      intro: "A photo app’s slideshow and an upload. The value of a slider is always written beside it.",
      children: [
        section(cx, {
          header: "Slideshow",
          rows: [
            sliderRow("volume", "Music Volume", this.volume, `${Math.round(this.volume)}%`, 0, 100, 1, (v) => (this.volume = v)),
            sliderRow(
              "sleep",
              "Stop After",
              this.timer,
              this.timer === 0 ? "Never" : `${this.timer} min`,
              0,
              60,
              5,
              (v) => (this.timer = v),
            ),
          ],
        }),
        section(cx, {
          header: "Upload",
          footer: "Trip photos go to the shared album “Coast, October”.",
          rows: [this.uploadRow(cx)],
        }),
        this.upload === "idle" || this.upload === "done"
          ? filledButton(cx, "upload-start", this.upload === "done" ? "Upload Again" : "Upload", {
              tone: this.upload === "done" ? "tinted" : "primary",
              on_click: (_e, cx) => this.start(cx),
            })
          : h_flex()
              .justify_center()
              .child(textButton(cx, "upload-cancel", "Cancel Upload", { on_click: (_e, cx) => this.cancel(cx) })),
      ],
    });
  }

  /** @param {Context} cx */
  uploadRow(cx) {
    const t = cx.theme();
    const status = {
      idle: `${PHOTOS} photos · 48.6 MB`,
      preparing: "Preparing photos…",
      uploading: `${this.sent} of ${PHOTOS} uploaded`,
      done: `${PHOTOS} photos uploaded`,
    }[this.upload];
    return v_flex()
      .test_id("upload")
      .gap_3()
      .px_4()
      .py_3()
      .child(
        h_flex()
          .gap_3()
          .child(
            this.upload === "preparing"
              ? Spinner.new().test_id("upload-spinner").size_7()
              : tile(cx, this.upload === "done" ? "check" : "upload", this.upload === "done" ? t.success : t.primary),
          )
          .child(
            v_flex()
              .flex_1()
              .min_w_0()
              .child(label(cx, "body", t.foreground, "Trip Photos"))
              .child(label(cx, "subheadline", t.muted_foreground, status).test_id("upload-status")),
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
      );
  }
}
