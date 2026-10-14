// "gpui-component" settings: pages of grouped settings, drawn as iOS draws a
// settings screen. Each group is an inset grouped card under a section
// header, its description the footer that explains it; each item is a 44 pt
// row with its title (and description) leading and its control trailing.
// Several pages are peers switched by a segmented control at the top: the
// desktop's resizable sidebar has no room on a phone.

import { Element, div, h_flex, v_flex, chainKey } from "nsh:elements";
import { Tabs } from "nsh:components";
import { TEXT, LEADING, CARD, checkSize, separator, retainedState } from "nsh:component/shared";

const ITEM_PAD = { xsmall: 6, small: 8, medium: 11, large: 14 };

/** One setting: a title, an optional description and its control (`content`). */
export class SettingItem extends Element {
  static new(title) {
    return new SettingItem(title);
  }

  constructor(title) {
    super("view");
    this._title = String(title);
    this._description = null;
    this._vertical = false;
    this._disabled = false;
    this._content = null;
  }

  description(text) {
    this._description = String(text);
    return this;
  }

  layout(axis) {
    if (axis !== "horizontal" && axis !== "vertical") throw new TypeError(`SettingItem.layout(): "horizontal" or "vertical", got ${JSON.stringify(axis)}`);
    this._vertical = axis === "vertical";
    return this;
  }

  /** Search keywords: settings on a phone have no search field, so they are kept and unused. */
  keywords(keywords) {
    if (!Array.isArray(keywords)) throw new TypeError("SettingItem.keywords() expects an array of strings");
    return this;
  }

  disabled(disabled) {
    this._disabled = !!disabled;
    return this;
  }

  /** The control: a switch, a select, a button, a value. */
  content(element) {
    if (!(element instanceof Element)) throw new TypeError("SettingItem.content(element) expects an element");
    this._content = element;
    return this;
  }

  /** Built once: a settings surface rebuilds its pages as children arrive. */
  _row(size) {
    if (this._built?.size !== size) this._built = { size, row: this._build(size) };
    return this._built.row;
  }

  _build(size) {
    if (this._content === null) throw new Error("SettingItem requires content(element)");
    if (this._disabled) this._content.disabled(true);
    const text = v_flex()
      .flex_1()
      .min_w_0()
      .gap_0p5()
      .child(div().text_size(TEXT.body).line_height(LEADING.body).text_color("foreground").child(this._title))
      .when(this._description !== null, (el) => el.child(div().text_size(TEXT.footnote).line_height(LEADING.footnote).text_color("muted_foreground").child(this._description)));
    const row = (this._vertical ? v_flex().items_stretch().gap_2() : h_flex().gap_3())
      .key(`setting:${this._title}`)
      .px_4()
      .py(ITEM_PAD[size])
      .min_h("2.75rem")
      .accessibility_label(this._title)
      .child(text)
      .child(this._vertical ? this._content : div().flex_none().child(this._content));
    // The style the script gave the item applies to its control, as in gpui-shell.
    if (this._sk !== "" && this._built === undefined) this._content._sk += this._sk;
    return this._disabled ? row.opacity(0.4) : row;
  }
}

/** A titled group of items: the section header, the card, the footer. */
export class SettingGroup extends Element {
  static new() {
    return new SettingGroup();
  }

  constructor() {
    super("view");
    this._title = null;
    this._description = null;
    this._items = [];
  }

  title(text) {
    this._title = String(text);
    return this;
  }

  description(text) {
    this._description = String(text);
    return this;
  }

  child(x) {
    if (Array.isArray(x)) return this.children(x);
    if (x === null || x === undefined || typeof x === "boolean") return this;
    if (!(x instanceof SettingItem)) throw new TypeError("SettingGroup accepts only SettingItem children");
    this._items.push(x);
    return this;
  }

  _view(size, index) {
    const rows = [];
    this._items.forEach((item, i) => {
      if (i > 0) rows.push(separator());
      rows.push(item._row(size));
    });
    return v_flex()
      .key(`group:${this._title ?? index}`)
      .gap_2()
      .when(this._title !== null, (el) =>
        el.child(div().role("header").px_4().text_size(TEXT.footnote).line_height(LEADING.footnote).text_color("muted_foreground").child(this._title)),
      )
      .child(v_flex()._preset([["flex"], ["flex_col"], ...CARD]).children(rows))
      .when(this._description !== null, (el) =>
        el.child(div().px_4().text_size(TEXT.footnote).line_height(LEADING.footnote).text_color("muted_foreground").child(this._description)),
      );
  }
}

/** A page of groups. Its `content` sits beside the page's title, as gpui-component's title suffix does. */
export class SettingPage extends Element {
  static new(title) {
    return new SettingPage(title);
  }

  constructor(title) {
    super("view");
    this._title = String(title);
    this._description = null;
    this._suffix = null;
    this._groups = [];
  }

  description(text) {
    this._description = String(text);
    return this;
  }

  /** Groups are always open on a phone: there is no collapsed sidebar tree. */
  default_open(_open) {
    return this;
  }

  /** gpui-component's reset control is a desktop affordance; accepted. */
  resettable(_resettable) {
    return this;
  }

  content(element) {
    if (!(element instanceof Element)) throw new TypeError("SettingPage.content(element) expects an element");
    this._suffix = element;
    return this;
  }

  child(x) {
    if (Array.isArray(x)) return this.children(x);
    if (x === null || x === undefined || typeof x === "boolean") return this;
    if (!(x instanceof SettingGroup)) throw new TypeError("SettingPage accepts only SettingGroup children");
    this._groups.push(x);
    return this;
  }

  _view(size, alone) {
    const header =
      this._description !== null || this._suffix !== null || alone
        ? h_flex()
            .items_start()
            .gap_3()
            .px_4()
            .child(
              v_flex()
                .flex_1()
                .min_w_0()
                .gap_1()
                .when(alone, (el) => el.child(div().role("header").text_size(22).line_height(28 / 22).font_bold().text_color("foreground").child(this._title)))
                .when(this._description !== null, (el) =>
                  el.child(div().text_size(TEXT.subheadline).line_height(LEADING.subheadline).text_color("muted_foreground").child(this._description)),
                ),
            )
            .child(this._suffix)
        : null;
    return v_flex()
      .key(`page:${this._title}`)
      .gap_6()
      .child(header)
      .children(this._groups.map((g, i) => g._view(size, i)));
  }
}

const SETTINGS_KEY = chainKey([["flex"], ["flex_col"], ["gap_6"]]);

/**
 * gpui-component's `Settings(id)`: `SettingPage` children, the one shown
 * kept under the id across renders (`default_selected_page` picks the first).
 */
export class Settings extends Element {
  static new(id) {
    return new Settings(id);
  }

  constructor(id) {
    super("view");
    this._id = String(id);
    this._k = this._id;
    this._pages = [];
    this._size = "medium";
    this._state = retainedState("settings", this._id, () => ({ page: null }));
    this._switcher = null;
    this._preset(SETTINGS_KEY);
  }

  size(size) {
    this._size = checkSize(size, "Settings.size()");
    return this._sync();
  }

  /** The desktop sidebar's width; a phone has no sidebar. */
  sidebar_width(_pixels) {
    return this;
  }

  sidebar_size_range(_minimum, _maximum) {
    return this;
  }

  default_selected_page(index) {
    const i = Number(index);
    if (!Number.isInteger(i) || i < 0) throw new TypeError(`Settings.default_selected_page(): a page index, got ${JSON.stringify(index)}`);
    this._state.page ??= i;
    return this._sync();
  }

  child(x) {
    if (Array.isArray(x)) return this.children(x);
    if (x === null || x === undefined || typeof x === "boolean") return this;
    if (!(x instanceof SettingPage)) throw new TypeError("Settings accepts only SettingPage children");
    this._pages.push(x);
    return this._sync();
  }

  _sync() {
    this._c = null;
    if (this._pages.length === 0) return this;
    const current = Math.min(this._state.page ?? 0, this._pages.length - 1);
    if (this._pages.length > 1) {
      this._switcher ??= Tabs.new(`${this._id}:pages`).on_change((i, cx) => {
        this._state.page = i;
        cx.notify();
      });
      this._switcher.items(this._pages.map((p) => p._title)).selected(current);
      super.child(this._switcher);
    }
    super.child(this._pages[current]._view(this._size, this._pages.length === 1));
    return this;
  }
}
