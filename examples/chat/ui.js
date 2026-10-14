// The example's presentation vocabulary: icons and an initials avatar.
//
// Colors come from `cx.theme()` by role and lengths from GPUI's rem scale, so
// the same code is right in light and dark and follows the user's text size.

import { div, shape } from "native-shell";

/** Outline icons on a 24-unit grid, drawn with a 2-unit round stroke. */
const ICONS = {
  arrow_up: "M12 19V5M5 12l7-7 7 7",
  copy: "M10 8h10a2 2 0 0 1 2 2v10a2 2 0 0 1-2 2H10a2 2 0 0 1-2-2V10a2 2 0 0 1 2-2ZM4 16a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h10a2 2 0 0 1 2 2",
  trash: "M3 6h18M8 6V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2M19 6l-1 14a2 2 0 0 1-2 2H8a2 2 0 0 1-2-2L5 6M10 11v6M14 11v6",
  refresh: "M21 12a9 9 0 1 1-3-6.7L21 8M21 3v5h-5",
  alert: "M22 12a10 10 0 1 1-20 0 10 10 0 0 1 20 0ZM12 8v4M12 16h.01",
  history: "M3 12a9 9 0 1 0 3-6.7L3 8M3 3v5h5M12 7v5l4 2",
};

/** @param {keyof typeof ICONS} name @param {string} color */
export function icon(name, color, size = 20) {
  return shape(ICONS[name], { view_box: [0, 0, 24, 24] })
    .stroke(color, 2)
    .w(size)
    .h(size)
    .flex_none();
}

/** A circle of initials, standing in for a profile photo. */
export function avatar(cx, name, size = 40) {
  const t = cx.theme();
  const initials = name
    .split(/\s+/)
    .slice(0, 2)
    .map((w) => w[0].toUpperCase())
    .join("");
  return div()
    .flex_none()
    // The initials scale with the user's text size, so the circle does too.
    .size(size * Math.min(cx.window().font_scale, 2))
    .rounded_full()
    .bg(t.secondary)
    .flex()
    .items_center()
    .justify_center()
    .text_color(t.secondary_foreground)
    .font_semibold()
    .text_sm()
    .child(initials);
}
