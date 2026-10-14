// "gpui-component" data and navigation: description lists, trees, lists,
// tab bars, breadcrumbs, settings, calendar, carousel, menus and toolbars.
//
// Constructors and methods are gpui-shell's (`new X(...)` and `X.new(...)`);
// the look is iOS: inset grouped cards, 44 pt rows, the tint for what can be
// tapped, secondary label color for what describes. Every color is a theme
// token and rows are sized by padding and minimum heights in rems, so the
// same component is right in both appearances and grows with the text size.

import { scope } from "nsh:core";
import { Element, div, h_flex, v_flex, list, chainKey } from "nsh:elements";
import { IconElement } from "nsh:component/icons";
import { TEXT, LEADING, CARD, checkSize, separator } from "nsh:component/shared";

export { IconElement as Icon } from "nsh:component/icons";
export { Tree, TreeItem } from "nsh:component/tree";
export { Settings, SettingPage, SettingGroup, SettingItem } from "nsh:component/settings";
export { Calendar, CalendarState } from "nsh:component/calendar";
export {
  Carousel, CarouselState, CarouselContent, CarouselItem, CarouselPrevious, CarouselNext, CarouselPagination,
  CarouselPaginationItem,
} from "nsh:component/carousel";
export { Menu, MenuItem, MenuSeparator, DropdownMenu, DropdownButton } from "nsh:component/menu";

// Description list -----------------------------------------------------------

/** Row padding and minimum height by size; medium and up keep the 44 pt row. */
const DL_PAD = { xsmall: [6, 1.75], small: [8, 2.25], medium: [11, 2.75], large: [14, 3.25] };

/**
 * One label/value pair: data for its `DescriptionList`, which draws it. The
 * value is a string, or (mobile) an element for a value that is more than text.
 */
export class DescriptionItem extends Element {
  static new(label) {
    return new DescriptionItem(label);
  }

  constructor(label) {
    super("view");
    this._label = String(label);
    this._value = "";
    this._span = 1;
  }

  value(value) {
    this._value = value instanceof Element ? value : String(value);
    return this;
  }

  span(span) {
    const n = Number(span);
    if (!Number.isInteger(n) || n < 1) throw new TypeError(`DescriptionItem.span(): a whole number of at least 1, got ${JSON.stringify(span)}`);
    this._span = n;
    return this;
  }
}

const CARD_KEY = chainKey([["flex"], ["flex_col"], ...CARD]);
const PLAIN_KEY = chainKey([["flex"], ["flex_col"], ["gap_2"]]);

/**
 * gpui-component's `DescriptionList`. The horizontal layout is iOS's value
 * row: the label leading, the value trailing in secondary color, one item per
 * row (a phone has no width for pairs side by side, so `columns` applies to
 * the vertical layout only). The vertical layout puts each label above its
 * value in a grid of `columns` (default 2) cells; `span` widens a cell.
 */
export class DescriptionList extends Element {
  static new() {
    return new DescriptionList();
  }

  constructor() {
    super("view");
    this._items = [];
    this._vertical = false;
    this._bordered = true;
    this._columns = null;
    this._size = "medium";
    this._P().role = "list";
    this._sync();
  }

  vertical() {
    this._vertical = true;
    return this._sync();
  }

  /** gpui-component's `layout(Axis)`. */
  layout(axis) {
    if (axis !== "horizontal" && axis !== "vertical") throw new TypeError(`DescriptionList.layout(): "horizontal" or "vertical", got ${JSON.stringify(axis)}`);
    this._vertical = axis === "vertical";
    return this._sync();
  }

  bordered(bordered) {
    this._bordered = !!bordered;
    return this._sync();
  }

  columns(columns) {
    const n = Number(columns);
    if (!Number.isInteger(n) || n < 1 || n > 10) throw new TypeError(`DescriptionList.columns(): a whole number from 1 through 10, got ${JSON.stringify(columns)}`);
    this._columns = n;
    return this._sync();
  }

  size(size) {
    this._size = checkSize(size, "DescriptionList.size()");
    return this._sync();
  }

  child(x) {
    if (Array.isArray(x)) return this.children(x);
    if (x === null || x === undefined || typeof x === "boolean") return this;
    if (!(x instanceof DescriptionItem)) throw new TypeError("DescriptionList accepts only DescriptionItem children");
    this._items.push(x);
    return this._sync();
  }

  /** gpui-component's `item(label, value, span)`. */
  item(label, value, span = 1) {
    return this.child(new DescriptionItem(label).value(value).span(span));
  }

  _sync() {
    const [py, minH] = DL_PAD[this._size];
    const rows = this._vertical ? this._grid(py) : this._items.map((it) => this._row(it, py, minH));
    this._preset(this._bordered ? CARD_KEY : PLAIN_KEY);
    this._c = null;
    rows.forEach((r, i) => {
      if (i > 0 && this._bordered) super.child(separator());
      super.child(r);
    });
    return this;
  }

  _label(item) {
    return item._value instanceof Element ? item._label : `${item._label}, ${item._value}`;
  }

  _row(item, py, minH) {
    const value =
      item._value instanceof Element
        ? h_flex().flex_1().min_w_0().justify_end().child(item._value)
        : div().flex_1().min_w_0().text_size(TEXT.body).line_height(LEADING.body).text_color("muted_foreground").text_right().child(item._value);
    return h_flex()
      .role("list_item")
      .accessibility_label(this._label(item))
      .items_start()
      .justify_between()
      .gap_4()
      .px(this._bordered ? 16 : 0)
      .py(py)
      .min_h(`${minH}rem`)
      .child(div().flex_none().max_w("50%").text_size(TEXT.body).line_height(LEADING.body).text_color("foreground").child(item._label))
      .child(value);
  }

  _grid(py) {
    const columns = this._columns ?? 2;
    const lines = [];
    let line = [];
    let used = 0;
    for (const item of this._items) {
      const span = Math.min(item._span, columns);
      if (used + span > columns) {
        lines.push(line);
        line = [];
        used = 0;
      }
      line.push([item, span]);
      used += span;
    }
    if (line.length > 0) lines.push(line);
    return lines.map((cells) =>
      h_flex()
        .items_start()
        .px(this._bordered ? 16 : 0)
        .py(py)
        .children(
          cells.map(([item, span]) =>
            v_flex()
              .role("list_item")
              .accessibility_label(this._label(item))
              .w(`${(span / columns) * 100}%`)
              .pr_3()
              .min_w_0()
              .gap_0p5()
              .child(div().text_size(TEXT.footnote).line_height(LEADING.footnote).text_color("muted_foreground").child(item._label))
              .child(item._value instanceof Element ? item._value : div().text_size(TEXT.body).line_height(LEADING.body).text_color("foreground").child(item._value)),
          ),
        ),
    );
  }
}

// Group box ------------------------------------------------------------------

const GROUP_KEYS = {
  normal: chainKey([["flex"], ["flex_col"], ["gap_3"]]),
  fill: chainKey([["flex"], ["flex_col"], ["gap_3"], ["p_4"], ...CARD]),
  outline: chainKey([["flex"], ["flex_col"], ["gap_3"], ["p_4"], ["rounded", "0.625rem"], ["border_1"], ["border_color", "border"]]),
};

/**
 * A titled group: the title in iOS's section-header style (footnote,
 * secondary) above the content. `normal` draws no box, `fill` sets the
 * content on a grouped card, `outline` frames it.
 */
export class GroupBox extends Element {
  static new() {
    return new GroupBox();
  }

  constructor() {
    super("view");
    this._preset([["flex"], ["flex_col"], ["gap_2"]]);
    this._title = null;
    this._content = new Element("view")._preset(GROUP_KEYS.normal);
    this._sync();
  }

  title(title) {
    this._title = title instanceof Element ? title : div().role("header").px_4().text_size(TEXT.footnote).line_height(LEADING.footnote).text_color("muted_foreground").child(String(title));
    return this._sync();
  }

  variant(variant) {
    if (!(variant in GROUP_KEYS)) throw new TypeError(`GroupBox.variant(): use one of normal, fill, outline, got ${JSON.stringify(variant)}`);
    this._content._preset(GROUP_KEYS[variant]);
    return this;
  }

  child(x) {
    this._content.child(x);
    return this;
  }

  children(xs) {
    this._content.children(xs);
    return this;
  }

  _sync() {
    this._c = null;
    if (this._title) super.child(this._title);
    super.child(this._content);
    return this;
  }
}

// Toolbar --------------------------------------------------------------------

/**
 * A row of controls in source order, spaced as iOS's toolbar spaces them:
 * at both ends and evenly between, 44 pt tall, on a transparent background.
 */
export class Toolbar extends Element {
  static new(id) {
    return new Toolbar(id);
  }

  constructor(id) {
    super("view");
    this._k = String(id);
    this._preset([["flex"], ["flex_row"], ["items_center"], ["justify_between"], ["gap_4"], ["min_h", "2.75rem"], ["px_4"]]);
  }
}

// Breadcrumb -----------------------------------------------------------------

/**
 * A path of places, the current one last. The earlier ones are tinted
 * buttons that go back there (`on_click(index)`, mobile); the row scrolls
 * sideways when the path is longer than the screen.
 */
export class Breadcrumb extends Element {
  static new(labels) {
    return new Breadcrumb(labels);
  }

  constructor(labels) {
    super("view");
    if (!Array.isArray(labels) || labels.some((l) => typeof l !== "string")) throw new TypeError("Breadcrumb(labels) expects an array of strings");
    this._onClick = null;
    this._preset([["flex"], ["flex_row"], ["items_center"], ["overflow_x_scroll"]]);
    this._P().role = "list";
    const last = labels.length - 1;
    labels.forEach((label, i) => {
      if (i > 0) super.child(new IconElement("icons/chevron-right.svg").size(14).color("muted_foreground").mx_0p5());
      const item = div().flex_none().py_2().px_1().text_size(TEXT.subheadline).line_height(LEADING.subheadline).child(label);
      if (i === last) {
        item.text_color("foreground").font_semibold().accessibility_label(`${label}, current`);
      } else {
        item
          .role("button")
          .text_color("primary")
          .hit_slop([8, 0, 8, 0])
          .active((s) => s.opacity(0.4))
          .on_click((_e, cx) => this._onClick?.(i, cx));
      }
      super.child(item);
    });
  }

  /** Mobile: `(index, cx)` when an earlier place in the path is tapped. */
  on_click(fn) {
    if (typeof fn !== "function") throw new TypeError(".on_click(handler) expects a function");
    this._onClick = fn;
    return this;
  }

  child() {
    throw new TypeError("Breadcrumb does not accept children; pass its labels to the constructor");
  }
}

// List -----------------------------------------------------------------------

function rowKey(row, i) {
  return row !== null && typeof row === "object" && typeof row.id === "string" ? row.id : String(i);
}

/**
 * gpui-component's `List`: a native recycled list over a snapshot of rows,
 * each rendered when it scrolls into view. It is the `list()` element (so
 * `.on_item_click`, `.track_scroll` and `.alignment` work on it), keyed by
 * each row's `id`, else its position.
 */
export class List {
  static new(id, rows, renderRow) {
    return new List(id, rows, renderRow);
  }

  constructor(id, rows, renderRow) {
    if (typeof rows !== "function") throw new TypeError("List(id, rows, render_row): rows must be a function returning an array");
    if (typeof renderRow !== "function") throw new TypeError("List(id, rows, render_row): render_row must be a function");
    if (scope.phase !== "render") throw new Error("List can only be built while rendering (inside render(cx))");
    const data = rows();
    if (!Array.isArray(data)) throw new TypeError("List(id, rows, render_row): rows() must return an array");
    const snapshot = Object.freeze([...data]);
    // A constructor that returns an object hands back that object: the list element itself.
    return list(String(id), snapshot.length, (i) => rowKey(snapshot[i], i), (i) => renderRow(snapshot[i]) ?? div());
  }
}

// Tab bar ----------------------------------------------------------------------

const TAB_VARIANTS = ["tab", "outline", "pill", "segmented", "underline"];
const TAB_TEXT = { xsmall: TEXT.footnote, small: TEXT.subheadline, medium: TEXT.callout, large: TEXT.body };

/** One tab of a `TabBar`; its look comes from the bar's variant and selection. */
export class Tab extends Element {
  static new() {
    return new Tab();
  }

  constructor() {
    super("view");
    this._label = "";
    this._selected = false;
    this._bar = null;
    this._index = -1;
    this._P().role = "tab";
  }

  label(label) {
    this._label = String(label);
    return super.child(this._label);
  }

  aria_label(label) {
    return this.accessibility_label(label);
  }

  selected(selected = true) {
    this._selected = !!selected;
    this._bar?._sync();
    return this;
  }

  disabled(disabled = true) {
    super.disabled(disabled);
    this._bar?._sync();
    return this;
  }
}

/**
 * gpui-component's `TabBar` (`Tabs` is its old name). `segmented` is the
 * native segmented control when every tab is a plain label; the other
 * variants are a row that scrolls sideways: `tab` and `underline` mark the
 * selection with a tinted bar under the label, `pill` fills it, `outline`
 * rings it.
 */
export class TabBar extends Element {
  static new(id) {
    return new TabBar(id);
  }

  constructor(id) {
    super("view");
    this._k = String(id);
    this._tabs = [];
    this._index = null;
    this._variant = "tab";
    this._size = "medium";
    this._onChange = null;
    this._segWired = false;
    this._P().role = "tab_list";
    this._sync();
  }

  selected_index(index) {
    this._index = Number(index) | 0;
    return this._sync();
  }

  variant(variant) {
    if (!TAB_VARIANTS.includes(variant)) throw new TypeError(`TabBar.variant(): use one of ${TAB_VARIANTS.join(", ")}, got ${JSON.stringify(variant)}`);
    this._variant = variant;
    return this._sync();
  }

  /** The overflow menu is desktop chrome; on a phone the row scrolls instead. */
  menu(_menu) {
    return this;
  }

  size(size) {
    this._size = checkSize(size, "TabBar.size()");
    return this._sync();
  }

  /** `(index, cx)`: the tab the user chose. */
  on_change(fn) {
    if (typeof fn !== "function") throw new TypeError(".on_change(handler) expects a function");
    this._onChange = fn;
    return this._sync();
  }

  child(x) {
    if (Array.isArray(x)) return this.children(x);
    if (x === null || x === undefined || typeof x === "boolean") return this;
    if (!(x instanceof Tab)) throw new TypeError("TabBar accepts only Tab children");
    x._bar = this;
    x._index = this._tabs.length;
    this._tabs.push(x);
    x._on("click", (_e, cx) => this._onChange?.(x._index, cx), "TabBar tab");
    return this._sync();
  }

  _selectedIndex() {
    if (this._index !== null) return this._index;
    const i = this._tabs.findIndex((t) => t._selected);
    return i < 0 ? 0 : i;
  }

  _sync() {
    const p = this._P();
    const plain = this._tabs.every((t) => (t._c ?? []).every((c) => typeof c === "string"));
    this._c = null;
    if (this._variant === "segmented" && plain) {
      this.__el = "segmented";
      this._preset("");
      p.items = JSON.stringify(this._tabs.map((t) => t._label));
      p.selected_index = this._selectedIndex();
      if (!this._segWired && this._onChange !== null) {
        this._segWired = true;
        this._on("change", (e, cx) => this._onChange?.(Number(e?.index) | 0, cx), ".on_change(handler)");
      }
      return this;
    }
    this.__el = "view";
    delete p.items;
    delete p.selected_index;
    const underline = this._variant === "tab" || this._variant === "underline";
    const ops = [["flex"], ["flex_row"], ["items_center"], ["overflow_x_scroll"], [underline ? "gap_5" : "gap_2"]];
    if (underline) ops.push(["border_b_1"], ["border_color", "border"]);
    this._preset(ops);
    for (const t of this._tabs) {
      this._style(t);
      super.child(t);
    }
    return this;
  }

  _style(tab) {
    const on = tab._index === this._selectedIndex();
    const ops = [["flex"], ["flex_row"], ["flex_none"], ["items_center"], ["justify_center"], ["gap_1p5"], ["text_size", TAB_TEXT[this._size]], ["line_height", 1.3]];
    switch (this._variant) {
      case "pill":
      case "segmented":
        ops.push(["px_4"], ["py_1p5"], ["min_h", "2rem"], ["rounded_full"], ["bg", on ? "primary" : "secondary"], ["text_color", on ? "primary_foreground" : "secondary_foreground"]);
        break;
      case "outline":
        ops.push(["px_4"], ["py_1p5"], ["min_h", "2rem"], ["rounded_full"], ["border_1"], ["border_color", on ? "primary" : "border"], ["text_color", on ? "primary" : "foreground"]);
        if (on) ops.push(["bg", "accent"]);
        break;
      default:
        // The tinted bar overlaps the row's hairline, as a selected tab's underline does.
        ops.push(["py_2p5"], ["min_h", "2.75rem"], ["border_b_2"], ["border_color", on ? "primary" : "transparent"], ["text_color", on ? "foreground" : "muted_foreground"], ["mb", -1]);
    }
    if (on) ops.push(["font_semibold"]);
    if (tab._dis) ops.push(["opacity", 0.4]);
    else ops.push(["@active", [["opacity", 0.6]]]);
    tab._preset(ops);
    tab._state("selected", on);
    if (this._variant !== "tab" && this._variant !== "underline") tab.hit_slop([6, 0, 6, 0]);
  }
}

export { TabBar as Tabs };
