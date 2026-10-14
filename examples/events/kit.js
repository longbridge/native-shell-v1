// Stand-ins for gpui-component parts still landing (DescriptionList, Field,
// Stepper). Each call site already uses the shape the real component takes,
// so swapping one in touches only this file.

import { div, h_flex, v_flex, Input } from "native-shell";
import { icon } from "./ui.js";
/** @import { Context } from "native-shell" */

/**
 * Label–value pairs about one object, as an inset group.
 * @param {Context} cx
 * @param {string} id
 * @param {{ label: string, value: string }[]} items
 */
export function descriptionList(cx, id, items) {
  const t = cx.theme();
  // At large text sizes a label beside its value leaves neither room: stack them, as iOS does.
  const stacked = cx.window().font_scale > 1.35;
  return v_flex()
    .test_id(id)
    .rounded_xl()
    .bg(t.surface)
    .overflow_hidden()
    .children(
      items.flatMap((item, i) => [
        i > 0 ? div().h(1).ml_4().bg(t.border) : null,
        (stacked ? v_flex().gap_0p5() : h_flex().items_start().gap_4())
          .px_4()
          .py_3()
          .child(div().when(!stacked, (el) => el.w_20().flex_none()).text_base().text_color(t.muted_foreground).child(item.label))
          .child(
            div()
              .test_id(`${id}-${item.label.toLowerCase()}`)
              .flex_1()
              .min_w_0()
              .text_base()
              .text_color(t.foreground)
              .when(!stacked, (el) => el.text_right())
              .child(item.value),
          ),
      ]),
    );
}

/**
 * A labeled text field with optional help, and an error that replaces the help.
 * @param {Context} cx
 * @param {{ id: string, label: string, state: import("native-shell").InputState, help?: string, error?: string }} opts
 */
export function field(cx, { id, label, state, help, error }) {
  const t = cx.theme();
  return v_flex()
    .gap_1p5()
    .child(div().px_1().text_sm().font_medium().text_color(t.foreground).child(label))
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
        .bg(t.surface)
        .text_base()
        .text_color(t.foreground),
    )
    .when(Boolean(error), (el) => el.child(div().test_id(`${id}-error`).role("alert").px_1().text_sm().text_color(t.destructive).child(error)))
    .when(!error && Boolean(help), (el) => el.child(div().px_1().text_sm().text_color(t.muted_foreground).child(help)));
}

/**
 * Where a short flow is: numbered steps, done ones checked, the current one in the tint.
 * @param {Context} cx
 * @param {string} id
 * @param {string[]} labels
 * @param {number} current
 */
export function steps(cx, id, labels, current) {
  const t = cx.theme();
  const compact = cx.window().font_scale > 1.35;
  return h_flex()
    .test_id(id)
    .accessibility_label(`Step ${current + 1} of ${labels.length}, ${labels[current]}`)
    .gap_2()
    .children(
      labels.map((label, i) => {
        const done = i < current;
        const on = i <= current;
        return h_flex()
          .when(i === current || !compact, (el) => el.flex_1())
          .gap_1p5()
          .items_center()
          .child(
            div()
              .size_6()
              .flex_none()
              .rounded_full()
              .flex()
              .items_center()
              .justify_center()
              .bg(on ? t.primary : t.muted)
              .text_xs()
              .font_semibold()
              .text_color(on ? t.primary_foreground : t.muted_foreground)
              .child(done ? icon("check", t.primary_foreground, 14) : String(i + 1)),
          )
          // At large text sizes only the current step keeps its name, so the row still fits.
          .when(i === current || !compact, (el) =>
            el.child(div().text_sm().font_medium().text_color(i === current ? t.foreground : t.muted_foreground).truncate().child(label)),
          );
      }),
    );
}
