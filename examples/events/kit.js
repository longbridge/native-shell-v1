// A stand-in for gpui-component's DescriptionList, still landing. Its call
// sites already pass what the real component takes.

import { div, h_flex, v_flex } from "native-shell";
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
