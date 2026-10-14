// "gpui-component" tree: nested items shown as an outline. A folder's row
// expands and collapses it with a disclosure chevron that turns; any row can
// be selected. Expansion and selection are kept under the tree's id across
// renders, as gpui-shell keeps them in the native tree. With a hardware
// keyboard, the arrows move the selection and open or close folders.

import { Element, div, h_flex, chainKey } from "nsh:elements";
import { IconElement } from "nsh:component/icons";
import { TEXT, LEADING, retainedState } from "nsh:component/shared";

/** Points each level of nesting indents its row. */
const INDENT = 20;

/** One node: an id unique in its tree, a label, and nested `TreeItem` children. */
export class TreeItem extends Element {
  static new(id, label) {
    return new TreeItem(id, label);
  }

  constructor(id, label) {
    super("view");
    this._id = String(id);
    this._label = String(label);
    this._items = [];
    this._expanded = false;
    this._disabled = false;
    this._icon = null;
  }

  child(x) {
    if (Array.isArray(x)) return this.children(x);
    if (x === null || x === undefined || typeof x === "boolean") return this;
    if (!(x instanceof TreeItem)) throw new TypeError("TreeItem accepts only TreeItem children");
    this._items.push(x);
    return this;
  }

  /** Whether the item starts expanded; the user's choice wins afterwards. */
  expanded(expanded) {
    this._expanded = !!expanded;
    return this;
  }

  disabled(disabled) {
    this._disabled = !!disabled;
    return this;
  }

  /** Mobile: a leading icon, an icon path or name (`"folder"`) or an element. */
  icon(icon) {
    this._icon = icon;
    return this;
  }
}

const ROW_KEY = chainKey([
  ["flex"], ["flex_row"], ["items_center"], ["gap_2"], ["pr_3"], ["py_2p5"], ["min_h", "2.75rem"], ["rounded_lg"],
]);
const SELECTED_KEY = chainKey([["bg", "selection"]]);
const PRESSABLE_KEY = chainKey([["@active", [["bg", "muted"]]]]);
const DISABLED_KEY = chainKey([["opacity", 0.4]]);

/**
 * gpui-component's `Tree(id)`. Mobile: `on_select((id, cx))` reports the
 * row the user selected.
 */
export class Tree extends Element {
  static new(id) {
    return new Tree(id);
  }

  constructor(id) {
    super("view");
    this._id = String(id);
    this._k = this._id;
    this._items = [];
    this._onSelect = null;
    this._state = retainedState("tree", this._id, () => ({ expanded: new Map(), selected: null }));
    this._preset([["flex"], ["flex_col"]]);
    this._P().role = "list";
  }

  child(x) {
    if (Array.isArray(x)) return this.children(x);
    if (x === null || x === undefined || typeof x === "boolean") return this;
    if (!(x instanceof TreeItem)) throw new TypeError("Tree accepts only TreeItem children");
    this._items.push(x);
    for (const entry of this._visible([x], 0, null)) super.child(this._row(entry));
    return this;
  }

  /** `(id, cx)` after the user selects a row (a tap, or the arrow keys). */
  on_select(fn) {
    if (typeof fn !== "function") throw new TypeError(".on_select(handler) expects a function");
    this._onSelect = fn;
    return this;
  }

  _isExpanded(item) {
    return this._state.expanded.get(item._id) ?? item._expanded;
  }

  /** The rows on screen: every item whose ancestors are all expanded, in order. */
  _visible(items, depth, parent) {
    const out = [];
    for (const item of items) {
      const folder = item._items.length > 0;
      const open = folder && this._isExpanded(item);
      out.push({ item, depth, parent, folder, open });
      if (open) out.push(...this._visible(item._items, depth + 1, item));
    }
    return out;
  }

  _row({ item, depth, folder, open }) {
    const selected = this._state.selected === item._id;
    const chevron = folder
      ? new IconElement("icons/chevron-right.svg").size(14).color("muted_foreground").rotate(open ? Math.PI / 2 : 0).spring("transform")
      : null;
    const icon =
      item._icon === null ? null : item._icon instanceof Element ? item._icon : new IconElement(item._icon).color(folder ? "primary" : "muted_foreground");
    const row = h_flex()
      .key(`tree:${item._id}`)
      ._preset(ROW_KEY + (selected ? SELECTED_KEY : "") + (item._disabled ? DISABLED_KEY : PRESSABLE_KEY))
      .pl(8 + depth * INDENT)
      .role("button")
      .accessibility_label(item._label)
      .accessibility_value(`level ${depth + 1}${folder ? `, ${item._items.length} items` : ""}`)
      .child(div().w(16).flex_none().flex().items_center().justify_center().child(chevron))
      .child(icon)
      .child(div().flex_1().min_w_0().truncate().text_size(TEXT.body).line_height(LEADING.body).text_color("foreground").when(selected, (el) => el.font_medium()).child(item._label));
    if (folder) row._state("expanded", open);
    row.selected(selected);
    if (item._disabled) return row.disabled(true);
    return row
      .on_click((_e, cx) => this._activate(item, folder, cx))
      .on_key_down((e, cx) => this._key(item, e, cx));
  }

  _activate(item, folder, cx) {
    if (folder) this._state.expanded.set(item._id, !this._isExpanded(item));
    this._select(item, cx);
  }

  _select(item, cx) {
    this._state.selected = item._id;
    this._onSelect?.(item._id, cx);
    cx.notify();
  }

  /** Arrow keys walk the visible rows; right and left open and close folders; return and space activate. */
  _key(item, e, cx) {
    const rows = this._visible(this._items, 0, null).filter((r) => !r.item._disabled);
    const at = rows.findIndex((r) => r.item === item);
    const here = rows[at];
    if (here === undefined) return;
    switch (e.key) {
      case "down":
        if (at + 1 < rows.length) this._select(rows[at + 1].item, cx);
        break;
      case "up":
        if (at > 0) this._select(rows[at - 1].item, cx);
        break;
      case "right":
        if (here.folder && !here.open) {
          this._state.expanded.set(item._id, true);
          cx.notify();
        } else if (here.open && at + 1 < rows.length) this._select(rows[at + 1].item, cx);
        break;
      case "left":
        if (here.open) {
          this._state.expanded.set(item._id, false);
          cx.notify();
        } else if (here.parent) this._select(here.parent, cx);
        break;
      case "enter":
      case "space":
        this._activate(item, here.folder, cx);
        break;
    }
  }
}
