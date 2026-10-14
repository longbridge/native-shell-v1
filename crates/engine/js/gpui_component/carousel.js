// "gpui-component" carousel: slides side by side, one on screen, paged by a
// swipe. The strip follows the finger (resisting past the ends unless it
// loops) and settles on a spring to the nearest slide, or the next one when
// the swipe was long or fast enough. Previous and next are round buttons
// over the slide's edges; the pagination is iOS's page dots over the bottom
// of the slide. Only the slide on screen is announced.

import { scope, markDirty, refuseStateInRender } from "nsh:core";
import { Element, div, chainKey } from "nsh:elements";
import { IconElement } from "nsh:component/icons";
import { isComponentSize } from "nsh:component/shared";

const SPRING = JSON.stringify({ response: 0.4, damping: 0.86 });
/** A swipe pages when it travels this far (points) or ends this fast (points per second). */
const PAGE_DISTANCE = 60;
const PAGE_VELOCITY = 400;

function released(what) {
  return new Error(`${what}: this carousel state has been released`);
}

/**
 * Retained carousel state, as gpui-component's: the item count, the selected
 * index, the axis and whether it loops. Only the horizontal axis pages on a
 * phone; a vertical state is laid out horizontally.
 */
class CarouselStateObject {
  #released = false;

  constructor(itemCount, selectedIndex = 0, axis = "horizontal", looping = false) {
    refuseStateInRender("CarouselState(item_count)");
    const n = Number(itemCount);
    if (!Number.isInteger(n) || n < 0) throw new TypeError(`CarouselState(item_count): a whole number, got ${JSON.stringify(itemCount)}`);
    if (axis !== "horizontal" && axis !== "vertical") throw new TypeError(`CarouselState(): axis is "horizontal" or "vertical", got ${JSON.stringify(axis)}`);
    this._count = n;
    this._index = n === 0 ? 0 : Math.min(n - 1, Math.max(0, Number(selectedIndex) | 0));
    this._axis = axis;
    this._looping = !!looping;
    this._drag = 0;
    this._dragging = false;
    this._carousel = null;
    this._owner = scope.owner;
    this._shownBy = null;
  }

  #live(what) {
    if (this.#released) throw released(what);
  }

  item_count() {
    return this._count;
  }

  selected_index() {
    return this._count === 0 ? null : this._index;
  }

  axis() {
    return this._axis;
  }

  is_looping() {
    return this._looping;
  }

  has_previous() {
    return this._count > 1 && (this._looping || this._index > 0);
  }

  has_next() {
    return this._count > 1 && (this._looping || this._index < this._count - 1);
  }

  set_selected_index(index) {
    this.#live("state.set_selected_index(index)");
    if (this._count === 0) return;
    this._index = Math.min(this._count - 1, Math.max(0, Number(index) | 0));
    markDirty(this._shownBy ?? this._owner);
  }

  set_item_count(count) {
    this.#live("state.set_item_count(count)");
    this._count = Math.max(0, Number(count) | 0);
    this._index = Math.min(this._index, Math.max(0, this._count - 1));
    markDirty(this._shownBy ?? this._owner);
  }

  set_looping(looping) {
    this._looping = !!looping;
    markDirty(this._shownBy ?? this._owner);
  }

  select_next() {
    return this.has_next() && (this.set_selected_index((this._index + 1) % this._count), true);
  }

  select_previous() {
    return this.has_previous() && (this.set_selected_index((this._index - 1 + this._count) % this._count), true);
  }

  release() {
    const was = !this.#released;
    this.#released = true;
    return was;
  }

  /** A move the user made: the state changes and the carousel's `on_change` hears it. */
  _go(index, cx) {
    if (this.#released || index === this._index) return;
    this.set_selected_index(index);
    this._carousel?._onChange?.(this._index, cx);
  }

  _pan(e, cx) {
    if (this.#released || this._count === 0) return;
    const phase = e?.phase;
    const dx = Number(e?.dx) || 0;
    if (phase === "began" || phase === "changed") {
      const atEdge = (dx > 0 && !this.has_previous()) || (dx < 0 && !this.has_next());
      this._dragging = true;
      this._drag = atEdge ? dx * 0.35 : dx;
      markDirty(this._shownBy ?? this._owner);
      return;
    }
    const vx = Number(e?.vx) || 0;
    this._dragging = false;
    this._drag = 0;
    markDirty(this._shownBy ?? this._owner);
    if (phase !== "ended") return;
    if ((dx < -PAGE_DISTANCE || vx < -PAGE_VELOCITY) && this.has_next()) this._go((this._index + 1) % this._count, cx);
    else if ((dx > PAGE_DISTANCE || vx > PAGE_VELOCITY) && this.has_previous()) this._go((this._index - 1 + this._count) % this._count, cx);
  }
}

/** gpui-component's `CarouselState(item_count, selected_index?, axis?, looping?)`, with or without `new`. */
export function CarouselState(itemCount, selectedIndex, axis, looping) {
  return new CarouselStateObject(itemCount, selectedIndex, axis, looping);
}
CarouselState.new = CarouselState;

function checkState(state, what) {
  if (!(state instanceof CarouselStateObject)) throw new TypeError(`${what} expects a CarouselState`);
  return state;
}

/**
 * gpui-component's `Carousel(id, state)`: the frame its parts are placed in
 * (`CarouselContent`, `CarouselPrevious`, `CarouselNext`, `CarouselPagination`).
 */
export class Carousel extends Element {
  static new(id, state) {
    return new Carousel(id, state);
  }

  constructor(id, state) {
    super("view");
    this._k = String(id);
    this._carouselState = checkState(state, "Carousel(id, state)");
    this._onChange = null;
    state._carousel = this;
    state._shownBy = scope.owner;
    this._preset([["relative"], ["flex"], ["flex_col"], ["w_full"]]);
    this._P().role = "list";
    this._announce();
  }

  accessibility_label(label) {
    this._label = String(label);
    return this._announce();
  }

  /** Focus rings are drawn natively; accepted. */
  focus_ring(_on) {
    return this;
  }

  item_count(count) {
    this._carouselState._count = Math.max(0, Number(count) | 0);
    return this._announce();
  }

  /** Selects an item without `on_change`, for a script that owns the selection. */
  selected_index(index) {
    const s = this._carouselState;
    if (s._count > 0) s._index = Math.min(s._count - 1, Math.max(0, Number(index) | 0));
    return this._announce();
  }

  /** `(index, cx)` after the user moves to another item (a swipe, the buttons or the dots). */
  on_change(fn) {
    if (typeof fn !== "function") throw new TypeError(".on_change(handler) expects a function");
    this._onChange = fn;
    return this;
  }

  _announce() {
    const s = this._carouselState;
    const p = this._P();
    if (this._label) p.a11y_label = this._label;
    p.a11y_value = s._count === 0 ? "" : `Item ${s._index + 1} of ${s._count}`;
    return this;
  }
}

const VIEWPORT_KEY = chainKey([["relative"], ["w_full"], ["overflow_hidden"]]);

/** The clipped viewport: `CarouselItem` children on a strip that pages. */
export class CarouselContent extends Element {
  static new(state) {
    return new CarouselContent(state);
  }

  constructor(state) {
    super("view");
    const s = checkState(state, "CarouselContent(state)");
    this._preset(VIEWPORT_KEY);
    // The strip sits one viewport width to the left per item before the
    // selected one (percentage margins resolve against the width), and the
    // finger's travel moves it on top of that.
    this._strip = div()
      .flex()
      .flex_row()
      .w_full()
      .ml(`${-100 * s._index}%`)
      .translate(s._drag, 0)
      .when(!s._dragging, (el) => el.spring("frame", SPRING).spring("transform", SPRING));
    super.child(this._strip);
    this.on_pan((e, cx) => s._pan(e, cx));
  }

  child(x) {
    if (Array.isArray(x)) return this.children(x);
    if (x === null || x === undefined || typeof x === "boolean") return this;
    if (!(x instanceof CarouselItem)) throw new TypeError("CarouselContent accepts only CarouselItem children");
    this._strip.child(x);
    return this;
  }
}

/** One slide, as wide as the viewport. Slides off screen are hidden from accessibility. */
export class CarouselItem extends Element {
  static new(id, index, state) {
    return new CarouselItem(id, index, state);
  }

  constructor(id, index, state) {
    super("view");
    const s = checkState(state, "CarouselItem(id, index, state)");
    const i = Number(index);
    this._k = String(id);
    this._preset([["flex"], ["flex_col"], ["w_full"], ["flex_none"]]);
    const p = this._P();
    p.a11y_label = `Item ${i + 1} of ${s._count}`;
    if (i !== s._index) p.a11y_hidden = true;
  }
}

const ARROW_SIZE = { xsmall: 24, small: 28, medium: 32, large: 40 };

class CarouselArrow extends Element {
  constructor(state, step, what) {
    super("view");
    const s = checkState(state, what);
    this._step = step;
    this._size = "medium";
    this._enabled = step < 0 ? s.has_previous() : s.has_next();
    const p = this._P();
    p.role = "button";
    p.a11y_label = step < 0 ? "Previous item" : "Next item";
    this._icon = new IconElement(step < 0 ? "icons/chevron-left.svg" : "icons/chevron-right.svg").color("foreground");
    super.child(this._icon);
    this._sync();
    if (this._enabled) this.on_click((_e, cx) => s._go((s._index + step + s._count) % s._count, cx));
    else this.disabled(true);
  }

  size(size) {
    if (!isComponentSize(size)) return super.size(size);
    this._size = size;
    return this._sync();
  }

  _sync() {
    const d = ARROW_SIZE[this._size];
    this._icon.size(Math.round(d * 0.55));
    // Centered on the slide's edge; the touch target stays 44 pt.
    return this._preset([
      ["absolute"], [this._step < 0 ? "left_3" : "right_3"], ["top", "50%"], ["mt", -d / 2], ["size", d], ["rounded_full"], ["flex"],
      ["items_center"], ["justify_center"], ["bg", "surface"], ["border_1"], ["border_color", "border"],
      this._enabled ? ["@active", [["opacity", 0.6]]] : ["opacity", 0.35],
    ]).hit_slop(Math.max(0, (44 - d) / 2));
  }
}

export class CarouselPrevious extends CarouselArrow {
  static new(state) {
    return new CarouselPrevious(state);
  }

  constructor(state) {
    super(state, -1, "CarouselPrevious(state)");
  }
}

export class CarouselNext extends CarouselArrow {
  static new(state) {
    return new CarouselNext(state);
  }

  constructor(state) {
    super(state, 1, "CarouselNext(state)");
  }
}

/** The page dots, on a capsule over the bottom of the slide; `CarouselPaginationItem` children. */
export class CarouselPagination extends Element {
  static new() {
    return new CarouselPagination();
  }

  constructor() {
    super("view");
    this._P().role = "tab_list";
    this._count = 0;
    this._sync();
  }

  child(x) {
    if (Array.isArray(x)) return this.children(x);
    if (x === null || x === undefined || typeof x === "boolean") return this;
    if (!(x instanceof CarouselPaginationItem)) throw new TypeError("CarouselPagination accepts only CarouselPaginationItem children");
    this._count += 1;
    super.child(x);
    return this._sync();
  }

  _sync() {
    // Centered on the slide: half its own width back from the middle (dots 7, gaps 8, padding 10 a side).
    const width = this._count * 7 + Math.max(0, this._count - 1) * 8 + 20;
    return this._preset([
      ["absolute"], ["bottom_3"], ["left", "50%"], ["ml", -width / 2], ["flex"], ["flex_row"], ["items_center"], ["gap_2"], ["px_2p5"],
      ["py_1p5"], ["rounded_full"], ["bg", "surface"], ["opacity", 0.9],
    ]);
  }
}

/** One dot: the selected item's is the label color, the others secondary. A tap shows its item. */
export class CarouselPaginationItem extends Element {
  static new(id, index, state) {
    return new CarouselPaginationItem(id, index, state);
  }

  constructor(id, index, state) {
    super("view");
    const s = checkState(state, "CarouselPaginationItem(id, index, state)");
    const i = Number(index);
    const on = i === s._index;
    this._k = String(id);
    this._preset([["size", 7], ["rounded_full"], ["bg", on ? "foreground" : "muted_foreground"], ["opacity", on ? 1 : 0.45]]);
    const p = this._P();
    p.role = "tab";
    p.a11y_label = `Item ${i + 1} of ${s._count}`;
    this.selected(on).hit_slop([18, 4, 18, 4]).on_click((_e, cx) => s._go(i, cx));
  }

  /** Children (a number, a thumbnail) are replaced by the dot on a phone. */
  child(_x) {
    return this;
  }

  size(_size) {
    return this;
  }
}
