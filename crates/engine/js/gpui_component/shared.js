// Pieces the "gpui-component" data and navigation modules share: the iOS
// text styles they draw with, size checking, separators and retained state.

import { scope } from "nsh:core";
import { div } from "nsh:elements";

/** The iOS text styles used here, in points (renderers scale text with the user's text size). */
export const TEXT = { body: 17, callout: 16, subheadline: 15, footnote: 13, caption: 12 };

/** Their line heights, as multiples of the size (body 22 pt, footnote 18 pt…). */
export const LEADING = { body: 22 / 17, callout: 21 / 16, subheadline: 20 / 15, footnote: 18 / 13, caption: 16 / 12 };

const SIZES = ["xsmall", "small", "medium", "large"];

/** Whether `size` names a component size; anything else is the `size` style (width and height), as on any element. */
export function isComponentSize(size) {
  return typeof size === "string" && SIZES.includes(size);
}

/**
 * An inset grouped card, as preset ops: the surface iOS draws a group of rows
 * on, set on `grouped_background` (systemGroupedBackground), with no border.
 */
export const CARD = [["rounded", "0.625rem"], ["bg", "surface"], ["overflow_hidden"]];

/** A hairline between rows, inset to the text column as iOS separators are. */
export function separator(inset = 16) {
  return div().h(0.5).ml(inset).bg("border").flex_none();
}

/** An inset grouped section's header: footnote, secondary, in capitals, over the card's text column. */
export function sectionHeader(title) {
  return div()
    .role("header")
    .px_4()
    .pb_1p5()
    .text_size(TEXT.footnote)
    .line_height(LEADING.footnote)
    .text_color("muted_foreground")
    .accessibility_label(String(title))
    .child(String(title).toUpperCase());
}

const retained = new Map();

/**
 * State a component keeps across renders under its id, per view (a tree's
 * expanded folders, a settings surface's page): what gpui-shell keeps in the
 * native element keyed by the same id.
 */
export function retainedState(kind, id, init) {
  const key = `${kind}:${scope.owner?.id ?? 0}:${id}`;
  let state = retained.get(key);
  if (state === undefined) {
    state = init();
    retained.set(key, state);
  }
  return state;
}
