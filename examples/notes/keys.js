// Hardware keyboard shortcuts (iPad, desktop): the same chords as a desktop
// notes app. Command on Apple keyboards, Control elsewhere.

/** @import { AsyncContext, Context, Element } from "native-shell" */

// Spelled in the order the runtime writes a keystroke: ctrl, alt, shift, cmd.
const CHORDS = {
  new_note: ["cmd-n", "ctrl-n"],
  new_folder: ["shift-cmd-n", "ctrl-shift-n"],
  search: ["cmd-f", "ctrl-f"],
};

/** Installs the application's key bindings, once. @param {AsyncContext} cx */
export function bindShortcuts(cx) {
  cx.bind_keys(
    Object.entries(CHORDS).flatMap(([action, keystrokes]) => keystrokes.map((keystroke) => ({ keystroke, action }))),
  );
}

/**
 * Handles the shortcuts a page offers, for `.map(...)` on its root element.
 * @param {Partial<Record<keyof typeof CHORDS, (cx: Context) => void>>} handlers
 */
export function shortcuts(handlers) {
  return (/** @type {Element} */ el) => {
    for (const [action, run] of Object.entries(handlers)) el.on_action(action, (_e, cx) => run?.(cx));
    return el;
  };
}
