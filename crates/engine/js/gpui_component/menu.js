// "gpui-component" menus on a phone: the platform's own pull-down menu (a
// UIMenu on iOS, a popup menu on Android), opened from a button that names
// it. `Menu` / `MenuItem` / `MenuSeparator` are gpui-component's menu data:
// choosing an item dispatches its action to the nearest `.on_action(action,
// …)` above the menu, as a key binding would. `DropdownMenu` and
// `DropdownButton` call their items' callbacks.
//
// The native menu is the protocol's select menu with no selection: the button
// keeps the menu's name and shows no check mark. The renderer marks what was
// chosen as selected; the "none" index alternates between -1 and -2 after
// every choice, so the next render clears that mark.

import { theme } from "nsh:core";
import { Element, chainKey } from "nsh:elements";
import { checkSize, retainedState } from "nsh:component/shared";

/**
 * The native menu button: `_entries()` answers its items (`{label, disabled?,
 * destructive?, run(cx)}`), read again when one is chosen.
 */
class MenuButton extends Element {
  constructor(id, label) {
    super("select");
    this._k = String(id);
    this._label = String(label);
    this._chosen = retainedState("menu", this._k, () => ({ count: 0 }));
    this._enabled = true;
    this._P().role = "menu";
    this._on(
      "change",
      (e, cx) => {
        const item = this._entries()[Number(e?.index)];
        this._chosen.count += 1;
        cx.notify();
        if (item && !item.disabled) item.run(cx);
      },
      "menu item",
    );
  }

  _entries() {
    return [];
  }

  _render() {
    const p = this._P();
    p.menu = JSON.stringify({
      title: "",
      placeholder: this._label,
      items: this._entries().map((it, i) => ({ label: it.label, value: i, disabled: !!it.disabled, destructive: !!it.destructive })),
    });
    p.placeholder = this._label;
    p.selected_index = -1 - (this._chosen.count % 2);
    p.enabled = this._enabled;
    if (this._label !== "") p.a11y_label = this._label;
    return this;
  }
}

/** One action in a `Menu`. */
export class MenuItem extends Element {
  static new(label, action) {
    return new MenuItem(label, action);
  }

  constructor(label, action) {
    super("view");
    this._label = String(label);
    this._action = String(action);
    this._disabled = false;
    this._checked = false;
  }

  disabled(disabled) {
    this._disabled = !!disabled;
    return this;
  }

  /** Kept for desktop code; a phone's pull-down menu marks no item. */
  checked(checked) {
    this._checked = !!checked;
    return this;
  }
}

/** A divider between groups of items; a phone's pull-down menu lists items without one. */
export class MenuSeparator extends Element {
  static new() {
    return new MenuSeparator();
  }

  constructor() {
    super("view");
  }
}

const MENU_KEY = chainKey([["flex_none"], ["self_start"], ["rounded_xl"], ["bg", "secondary"]]);

/**
 * gpui-component's `Menu(label)`: a button with the label that opens the
 * items. A choice runs the nearest `.on_action(action, handler)` on the
 * menu's ancestors.
 */
export class Menu extends MenuButton {
  static new(label) {
    return new Menu(label);
  }

  constructor(label) {
    super(`menu:${label}`, label);
    this._items = [];
    this._preset(MENU_KEY);
    this._render();
  }

  disabled(disabled) {
    this._enabled = !disabled;
    return this._render();
  }

  child(x) {
    if (Array.isArray(x)) return this.children(x);
    if (x === null || x === undefined || typeof x === "boolean") return this;
    if (x instanceof MenuSeparator) return this;
    if (!(x instanceof MenuItem)) throw new TypeError("Menu accepts only MenuItem and MenuSeparator children");
    this._items.push(x);
    return this._render();
  }

  _entries() {
    return this._items.map((it) => ({ label: it._label, disabled: it._disabled, run: (cx) => this._dispatch(it._action, cx) }));
  }

  _dispatch(action, cx) {
    for (let el = this._parent ?? this._owner?.mount ?? null; el; el = el._parent ?? el._owner?.mount ?? null) {
      const fn = el._k_routes?.actions.get(action);
      if (fn) {
        fn({ action }, cx);
        return;
      }
    }
  }
}

/**
 * gpui-component's `DropdownMenu(id, label)`: a button with the label whose
 * menu lists `item(label, callback)` entries in call order.
 */
export class DropdownMenu extends MenuButton {
  static new(id, label) {
    return new DropdownMenu(id, label);
  }

  constructor(id, label) {
    super(id, label);
    this._items = [];
    this._preset(MENU_KEY);
    this._render();
  }

  item(label, callback) {
    if (typeof callback !== "function") throw new TypeError("DropdownMenu.item(label, callback) expects a function");
    this._items.push({ label: String(label), run: callback });
    return this._render();
  }

  disabled(disabled = true) {
    this._enabled = !disabled;
    return this._render();
  }

  _entries() {
    return this._items;
  }
}

// The Button presets' sizes and colors (see `components.js`), so a split
// button sits in a row of buttons without standing out.
const DROPDOWN_VARIANTS = {
  primary: { bg: "primary", fg: "primary_foreground" },
  secondary: { bg: "secondary", fg: "foreground" },
  danger: { bg: "destructive", fg: "destructive_foreground" },
  ghost: { bg: null, fg: "primary" },
};
const DROPDOWN_SIZES = {
  xsmall: { h: "1.75rem", radius: "0.5rem", text: 13, slop: 8 },
  small: { h: "2.125rem", radius: "0.5rem", text: 15, slop: 5 },
  medium: { h: "2.75rem", radius: "0.625rem", text: 17, slop: 0 },
  large: { h: "3.125rem", radius: "0.75rem", text: 17, slop: 0 },
};

/** `#rrggbb[aa]` 15% of the way to black: iOS's highlighted filled button. */
function pressed(hex) {
  const m = /^#([0-9a-f]{6})([0-9a-f]{2})?$/i.exec(hex ?? "");
  if (!m) return hex;
  const n = parseInt(m[1], 16);
  const ch = (shift) => Math.round(((n >> shift) & 255) * 0.85).toString(16).padStart(2, "0");
  return `#${ch(16)}${ch(8)}${ch(0)}${m[2] ?? ""}`;
}

/**
 * gpui-component's split `DropdownButton(id, label)`: the label half runs
 * `on_click`, the chevron half opens the `menu_item(label, callback)` menu.
 * It accepts no children.
 */
export class DropdownButton extends Element {
  static new(id, label) {
    return new DropdownButton(id, label);
  }

  constructor(id, label) {
    super("view");
    this._id = String(id);
    this._k = this._id;
    this._label = String(label);
    this._variant = "secondary";
    this._outline = false;
    this._size = "medium";
    this._items = [];
    this._onClick = null;
    this._disabled = false;
    this._selected = false;
    this._sync();
  }

  outline() {
    this._outline = true;
    return this._sync();
  }

  variant(variant) {
    if (!(variant in DROPDOWN_VARIANTS)) throw new TypeError(`DropdownButton.variant(): use one of ${Object.keys(DROPDOWN_VARIANTS).join(", ")}, got ${JSON.stringify(variant)}`);
    this._variant = variant;
    return this._sync();
  }

  size(size) {
    this._size = checkSize(size, "DropdownButton.size()");
    return this._sync();
  }

  disabled(disabled) {
    this._disabled = !!disabled;
    return this._sync();
  }

  selected(selected) {
    this._selected = !!selected;
    return this._sync();
  }

  /** The menu opens below the button wherever the platform places it; accepted. */
  menu_anchor(_anchor) {
    return this;
  }

  on_click(fn) {
    if (typeof fn !== "function") throw new TypeError(".on_click(handler) expects a function");
    this._onClick = fn;
    return this;
  }

  menu_item(label, callback) {
    if (typeof callback !== "function") throw new TypeError("DropdownButton.menu_item(label, callback) expects a function");
    this._items.push({ label: String(label), run: callback });
    return this._sync();
  }

  child() {
    throw new TypeError("DropdownButton accepts no children");
  }

  _sync() {
    const v = DROPDOWN_VARIANTS[this._variant];
    const z = DROPDOWN_SIZES[this._size];
    // `outline()` is iOS's tinted button: the tint at low alpha behind tinted text.
    const bg = this._outline ? "accent" : v.bg;
    const fg = this._outline ? "accent_foreground" : v.fg;
    const box = [["flex"], ["flex_row"], ["items_stretch"], ["flex_none"], ["self_start"], ["h", z.h], ["rounded", z.radius], ["overflow_hidden"]];
    if (bg !== null) box.push(["bg", bg]);
    if (this._disabled) box.push(["opacity", 0.4]);
    this._preset(box);
    this._state("selected", this._selected);

    if (this._action === undefined) {
      this._action = new Element("view");
      this._action._P().role = "button";
      this._action._on("click", (e, cx) => this._onClick?.(e, cx), ".on_click(handler)");
      this._action.child(this._label);
    }
    const press = bg === null ? ["@active", [["opacity", 0.6]]] : ["@active", [["bg", pressed(theme()[bg])]]];
    this._action._preset([
      ["flex"], ["items_center"], ["justify_center"], ["px", "0.875rem"], ["text_size", z.text], ["line_height", 1.3], ["font_semibold"],
      ["text_color", fg], press,
    ]);
    this._action._dis = this._disabled;
    this._action._state("disabled", this._disabled);
    if (z.slop > 0) this._action.hit_slop([z.slop, 0, z.slop, z.slop]);

    if (this._menu === undefined) {
      this._menu = new MenuButton(`${this._id}:menu`, "");
      this._menu._entries = () => this._items;
    }
    const menu = this._menu;
    menu._enabled = !this._disabled;
    menu._render();
    menu._P().a11y_label = `More ${this._label} options`;
    menu._P().tint = fg;
    // A hairline divides the halves, as on a split button.
    menu._preset([["flex_none"], ["border_l_1"], ["border_color", bg === null ? "border" : "surface"]]);
    this._c = null;
    super.child(this._action);
    super.child(menu);
    return this;
  }
}
