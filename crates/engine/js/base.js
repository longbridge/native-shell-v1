// The rest of gpui-base's catalog that has a phone counterpart, with
// gpui-base's names, constructors and (headless) behavior: the components
// carry state, roles and events, and the script draws them.
//
// Slots (`content`, `header`, `panel`, `image`, `fallback`, `trigger`) are kept
// apart from ordinary children and placed by Rust: `_before` precedes the
// children, `_after` follows them.
//
// Anchored surfaces (`Popover`, `HoverCard`, `Popup`) cannot anchor on a
// phone; their content is presented as a sheet while open (see `core.js`).

import { N, scope, refuseStateInRender, describePopover, deferPopover, popoverState, windowObject } from "nsh:core";
import { Element, chainKey } from "nsh:elements";
import { InputState, Input, HeadlessPickerElement } from "nsh:components";

function view(role) {
  const el = new Element("view");
  if (role) el._P().role = role;
  return el;
}

function released(what, kind) {
  return new Error(`${what}: this ${kind} has been released`);
}

// Toggles and groups -----------------------------------------------------------

/** A button that stays down: `pressed(...)` in, `on_change(pressed)` out. No styling. */
class ToggleElement extends Element {
  constructor(id) {
    super("view");
    this._k = String(id);
    this._P().role = "button";
    this._pressed = false;
  }
  pressed(value = true) {
    this._pressed = !!value;
    return super.pressed(value);
  }
  on_change(fn) {
    if (typeof fn !== "function") throw new TypeError(".on_change(handler) expects a function");
    return this._on("click", (_e, cx) => fn(!this._pressed, cx), ".on_change(handler)");
  }
}

export const Toggle = Object.freeze({ new: (id) => new ToggleElement(id) });

/** A set announced as one group; the axis is announced, not drawn. Holds no state. */
class GroupElement extends Element {
  constructor(id, role) {
    super("view");
    this._k = String(id);
    this._P().role = role;
  }
  axis(_axis) {
    return this;
  }
}

export const RadioGroup = Object.freeze({ new: (id) => new GroupElement(id, "list") });
export const ToggleGroup = Object.freeze({ new: (id) => new GroupElement(id, "list") });

// Table ----------------------------------------------------------------------

/**
 * gpui-base's semantic table: groups, rows and cells the script nests and lays
 * out itself (`.flex()`, `.grid_cols(n)`…) — no styling, as in gpui-base. The
 * protocol has no table roles; rows announce as list items and header cells
 * as headers, with their one-based position in the accessibility value.
 */
class TableElement extends Element {
  constructor(id, role) {
    super("view");
    this._k = String(id);
    if (role) this._P().role = role;
  }
  row_count(n) {
    this._rows = Number(n);
    return this._describe();
  }
  column_count(n) {
    this._columns = Number(n);
    return this._describe();
  }
  _describe() {
    const parts = [];
    if (this._rows !== undefined) parts.push(`${this._rows} rows`);
    if (this._columns !== undefined) parts.push(`${this._columns} columns`);
    this._P().a11y_value = parts.join(", ");
    return this;
  }
}

function positioned(id, index, role, what) {
  const i = Number(index);
  if (!Number.isInteger(i) || i < 1) throw new TypeError(`${what}: the index is one-based, got ${JSON.stringify(index)}`);
  const el = new TableElement(id, role);
  el._P().a11y_value = String(i);
  return el;
}

export const Table = Object.freeze({ new: (id) => new TableElement(id, "list") });
export const TableHeader = Object.freeze({ new: (id) => new TableElement(id, null) });
export const TableBody = Object.freeze({ new: (id) => new TableElement(id, null) });
export const TableRow = Object.freeze({ new: (id, row) => positioned(id, row, "list_item", "TableRow.new(id, row_index)") });
export const TableHead = Object.freeze({ new: (id, column) => positioned(id, column, "header", "TableHead.new(id, column_index)") });
export const TableCell = Object.freeze({ new: (id, column) => positioned(id, column, null, "TableCell.new(id, column_index)") });
export const TableCaption = Object.freeze({ new: (id) => new TableElement(id, null) });

// Collapsible and accordion ------------------------------------------------------

/** Its `content` slot is rendered only while `open`; ordinary children always. */
class CollapsibleElement extends Element {
  constructor() {
    super("view");
    this._open = false;
    this._content = null;
  }
  open(value = true) {
    this._open = !!value;
    return this._sync();
  }
  content(el) {
    this._content = el;
    return this._sync();
  }
  _sync() {
    this._after = this._open && this._content !== null ? [this._content] : null;
    return this;
  }
}

export const Collapsible = Object.freeze({ new: () => new CollapsibleElement() });

export const Accordion = Object.freeze({
  new: (id) => {
    const el = view("list");
    el._k = String(id);
    return el;
  },
});

/** One item: `open(...)` in; the trigger's `on_change(open)` out. It passes its state down. */
class AccordionItemElement extends Element {
  constructor() {
    super("view");
    this._open = false;
    this._header = null;
    this._panel = null;
  }
  open(value = true) {
    this._open = !!value;
    return this._sync();
  }
  disabled(value = true) {
    super.disabled(value);
    return this._sync();
  }
  header(el) {
    this._header = el;
    return this._sync();
  }
  panel(el) {
    this._panel = el;
    return this._sync();
  }
  _sync() {
    const trigger = this._header?._trigger;
    if (trigger) {
      trigger._open = this._open;
      trigger._dis = this._dis;
      trigger._state("expanded", this._open);
      trigger._state("disabled", this._dis);
    }
    this._before = this._header ? [this._header] : null;
    const panel = this._panel;
    if (panel && (this._open || panel._keep)) {
      if (!this._open) panel._sk += HIDDEN;
      this._after = [panel];
    } else {
      this._after = null;
    }
    return this;
  }
}

const HIDDEN = chainKey([["hidden"]]);

export const AccordionItem = Object.freeze({ new: () => new AccordionItemElement() });

export const AccordionHeader = Object.freeze({
  new: (trigger) => {
    const el = view("header");
    el._trigger = trigger;
    el.child(trigger);
    return el;
  },
});

export const AccordionPanel = Object.freeze({ new: () => new Element("view") });

/** The button. `on_change(open)` asks for the opposite of the item's state. */
class AccordionTriggerElement extends Element {
  constructor(id) {
    super("view");
    this._k = String(id);
    this._P().role = "button";
    this._open = false;
  }
  on_change(fn) {
    if (typeof fn !== "function") throw new TypeError(".on_change(handler) expects a function");
    return this._on("click", (_e, cx) => fn(!this._open, cx), ".on_change(handler)");
  }
}

export const AccordionTrigger = Object.freeze({ new: (id) => new AccordionTriggerElement(id) });

// Pagination -----------------------------------------------------------------

export const Pagination = Object.freeze({
  new: (id) => {
    const el = view("list");
    el._k = String(id);
    return el;
  },
});

/**
 * Which page numbers to draw: the first, the last and a window around the
 * current one, each broken run collapsed into an ellipsis naming the pages it
 * stands for (inclusive). `visible_pages` defaults to 7, at least 5.
 */
export function pagination_items(current, total, visible = 7) {
  total = Math.floor(Number(total));
  if (!(total > 1)) return [];
  current = Math.min(total, Math.max(1, Math.floor(Number(current))));
  visible = Math.max(5, Math.floor(Number(visible) || 7));
  const page = (p) => ({ page: p });
  if (total <= visible) return Array.from({ length: total }, (_, i) => page(i + 1));
  const middle = visible - 2;
  let start = Math.max(2, current - Math.floor((middle - 1) / 2));
  let end = start + middle - 1;
  if (end > total - 1) {
    end = total - 1;
    start = end - middle + 1;
  }
  const out = [page(1)];
  if (start > 2) {
    out.push({ ellipsis: [2, start] });
    start += 1;
  }
  const tailGap = end < total - 1;
  if (tailGap) end -= 1;
  for (let p = start; p <= end; p++) out.push(page(p));
  if (tailGap) out.push({ ellipsis: [end + 1, total - 1] });
  out.push(page(total));
  return out;
}

// Avatar -------------------------------------------------------------------

/** Renders its `image` slot, else its `fallback`, and never both. */
class AvatarElement extends Element {
  constructor() {
    super("view");
    this._image = null;
    this._fallback = null;
  }
  image(el) {
    this._image = el;
    return this._sync();
  }
  fallback(el) {
    this._fallback = el;
    return this._sync();
  }
  _sync() {
    const slot = this._image ?? this._fallback;
    this._before = slot ? [slot] : null;
    return this;
  }
}

export const Avatar = Object.freeze({ new: () => new AvatarElement() });
export const AvatarImage = Object.freeze({
  new: (path) => {
    const el = new Element("image");
    el._P().src = N.check_image(String(path));
    el._P().content_mode = 0;
    return el;
  },
});
export const AvatarFallback = Object.freeze({ new: () => new Element("view") });

// Number input ---------------------------------------------------------------

/**
 * A spinbutton over an `InputState` given `set_step(...)` (and optionally
 * `set_min`/`set_max`): the field on a numeric keyboard between two step
 * buttons. The `input`, `decrement_button` and `increment_button` slots
 * replace the defaults.
 */
class NumberInputElement extends Element {
  constructor(state) {
    super("view");
    if (!(state instanceof InputState)) throw new TypeError("NumberInput.new(state) expects an InputState");
    this._state = state;
    this._sk = NUMBER_ROW;
    this._P().role = "slider";
    this._onStep = null;
    this._dec = this._stepper(view("button").child("−"), "decrement");
    this._inc = this._stepper(view("button").child("+"), "increment");
    this._input = this._field(Input.new(state).flex_1());
    this._sync();
  }
  input(el) {
    this._input = this._field(el);
    return this._sync();
  }
  decrement_button(el) {
    this._dec = this._stepper(el, "decrement");
    return this._sync();
  }
  increment_button(el) {
    this._inc = this._stepper(el, "increment");
    return this._sync();
  }
  controls_right() {
    return this;
  }
  /** `(action: "increment" | "decrement", cx)`, after the state moved. */
  on_step(fn) {
    if (typeof fn !== "function") throw new TypeError(".on_step(handler) expects a function");
    this._onStep = fn;
    return this;
  }
  _stepper(el, action) {
    if (el instanceof Element) {
      el._on(
        "click",
        (_e, cx) => {
          this._state._step(action === "increment" ? 1 : -1, cx);
          this._onStep?.(action, cx);
        },
        "NumberInput step",
      );
    }
    return el;
  }
  _field(el) {
    if (el instanceof Element) {
      el._P().keyboard = 2;
      el._P().a11y_value = this._state._value;
    }
    return el;
  }
  _sync() {
    this._before = [this._dec, this._input, this._inc];
    return this;
  }
}

const NUMBER_ROW = chainKey([["flex"], ["flex_row"], ["items_center"]]);

export const NumberInput = Object.freeze({ new: (state) => new NumberInputElement(state) });

// One-time code --------------------------------------------------------------

/**
 * A one-time code of `length` digits: one native field with the one-time-code
 * content type (the system offers the code from a message) on a number pad.
 * Events: `change` after each edit, `complete` when the last digit lands.
 */
export class OtpState extends InputState {
  static new(length, options = {}) {
    return new OtpState(length, options);
  }

  constructor(length, options = {}) {
    const len = Math.floor(Number(length));
    if (!(len >= 1 && len <= 64)) throw new TypeError("OtpState.new(length): length must be a whole number between 1 and 64");
    super({
      value: options.value ?? "",
      secure: !!options.masked,
      keyboard: "number",
      content_type: "one_time_code",
      max_length: len,
      allowed_chars: "0123456789",
      auto_capitalize: "none",
    });
    this._len = len;
    this._listeners.complete = [];
  }
  len() {
    return this._len;
  }
  is_masked() {
    return this.secure;
  }
  on(event, fn) {
    if (event === "complete") {
      if (typeof fn !== "function") throw new TypeError("state.on(event, fn) expects a function");
      this._listeners.complete.push(fn);
      return true;
    }
    if (event === "submit") throw new TypeError('OtpState.on(): use "change", "complete", "focus" or "blur"');
    return super.on(event, fn);
  }
  _native(e, cx) {
    super._native(e, cx);
    if (!e?.composing && this._value.length >= this._len) this._emit("complete", {}, cx);
  }
}

export const OtpInput = Object.freeze({
  new: (state) => {
    if (!(state instanceof OtpState)) throw new TypeError("OtpInput.new(state) expects an OtpState");
    return Input.new(state);
  },
});

// Virtual list scrolling and scrollbars --------------------------------------------

/** Scrolls a list from code; hand it to the list with `.track_scroll(handle)`. */
export class VirtualListScrollHandle {
  #released = false;
  static new() {
    refuseStateInRender("VirtualListScrollHandle.new()");
    return new VirtualListScrollHandle();
  }
  constructor() {
    this._list = null;
    this._count = 0;
  }
  #live(what) {
    if (this.#released) throw released(what, "scroll handle");
    if (this._list === null) throw new Error(`${what}: the handle is not tracking a list (use .track_scroll(handle))`);
  }
  scroll_to_item(index, strategy = "top") {
    this.#live("handle.scroll_to_item(index)");
    N.command("scroll_to_index", "__list:" + this._list, JSON.stringify({ index: Number(index) | 0, position: strategy === "center" ? "center" : "top", animated: true }));
  }
  scroll_to_bottom() {
    this.#live("handle.scroll_to_bottom()");
    N.command("scroll_to_index", "__list:" + this._list, JSON.stringify({ index: Math.max(0, this._count - 1), position: "bottom", animated: true }));
  }
  release() {
    const was = !this.#released;
    this.#released = true;
    return was;
  }
}

/** Mobile scroll views draw their own indicators; a placed bar is accepted and draws nothing. */
export const Scrollbar = Object.freeze({
  new: () => new Element("view"),
  horizontal: () => new Element("view"),
  vertical: () => new Element("view"),
});

// Anchored surfaces ----------------------------------------------------------

/**
 * Opened by a press on its trigger; presented as a sheet. Controlled with
 * `open(...)` + `on_open_change(open)`, or uncontrolled from `default_open`.
 */
class PopoverElement extends Element {
  constructor(id, gesture) {
    super("view");
    this._id = String(id);
    this._k = this._id;
    this._gesture = gesture;
    this._open = null;
    this._default = false;
    this._content = null;
    this._onOpen = null;
    this._trigger = null;
    this._wired = false;
  }
  trigger(el) {
    this._trigger = el;
    this._before = [el];
    return this._wire();
  }
  content(el) {
    this._content = el;
    return this._wire();
  }
  open(value = true) {
    this._open = !!value;
    return this._wire();
  }
  default_open(value = true) {
    this._default = !!value;
    return this._wire();
  }
  on_open_change(fn) {
    if (typeof fn !== "function") throw new TypeError(".on_open_change(handler) expects a function");
    this._onOpen = fn;
    return this._wire();
  }
  anchor(_value) {
    return this;
  }
  open_delay(_ms) {
    return this;
  }
  close_delay(_ms) {
    return this;
  }
  overlay_closable(_value) {
    return this;
  }
  /** Asks for `open`: the script's handler when controlled, else the kept state. */
  _request(open, cx) {
    if (this._open !== null) this._onOpen?.(open, cx);
    else {
      popoverState(this._id, this._default).set(open);
      this._onOpen?.(open, cx);
      cx.notify();
    }
  }
  _wire() {
    if (!this._wired && scope.phase === "render") {
      this._wired = true;
      this._on(this._gesture, (_e, cx) => this._request(!this._isOpen(), cx), "Popover trigger");
      deferPopover(() =>
        this._content !== null && this._isOpen()
          ? { id: this._id, content: this._content, onDismiss: (cx) => this._request(false, cx) }
          : null,
      );
    }
    return this;
  }
  _isOpen() {
    return this._open ?? popoverState(this._id, this._default).get();
  }
}

export const Popover = Object.freeze({ new: (id) => new PopoverElement(id, "click") });
/** Touch screens cannot hover: a long press opens it. */
export const HoverCard = Object.freeze({ new: (id) => new PopoverElement(id, "long_press") });

/** The bare surface: open while its `content` slot is filled. */
class PopupElement extends Element {
  constructor(id, trigger) {
    super("view");
    this._id = String(id);
    this._k = this._id;
    if (trigger !== undefined) this.child(trigger);
  }
  content(el) {
    describePopover(this._id, el, null);
    return this;
  }
  anchor(_value) {
    return this;
  }
}

export const Popup = Object.freeze({ new: (id, trigger) => new PopupElement(id, trigger) });

// Headless combobox ------------------------------------------------------------

export const Combobox = Object.freeze({ new: (id) => new HeadlessPickerElement(id, "menu") });

// Text views -------------------------------------------------------------------

/** Markdown or (a subset of) HTML, laid out by Rust into text runs and block views. */
class TextViewElement extends Element {
  constructor(id, format, source) {
    super("textview");
    this._k = String(id);
    this._tv = format;
    this._src = String(source);
  }
  /** `(url, cx)`: overrides the native link opening. */
  on_link_click(fn) {
    return this._on("link", (e, cx) => fn(e?.url, cx), ".on_link_click(handler)");
  }
  selectable(value = true) {
    this._P().selectable = !!value;
    return this;
  }
  scrollable(value = true) {
    if (value) this._sk += SCROLLABLE;
    return this;
  }
}

const SCROLLABLE = chainKey([["overflow_y_scroll"]]);

export const TextView = Object.freeze({
  markdown: (id, markdown) => new TextViewElement(id, "markdown", markdown),
  html: (id, html) => new TextViewElement(id, "html", html),
});

// Paths --------------------------------------------------------------------------

function coordinate(v) {
  if (typeof v === "number" && Number.isFinite(v)) return { v, pct: false };
  if (typeof v === "string" && /^-?\d+(\.\d+)?%$/.test(v)) return { v: parseFloat(v), pct: true };
  throw new TypeError(`PathBuilder: a coordinate is a number or a "N%" string, got ${JSON.stringify(v)}`);
}

/** Immutable geometry for `window.paint_path`, built like GPUI's `PathBuilder`. */
class PathBuilderObject {
  constructor(stroke) {
    this._stroke = stroke;
    this._d = [];
    this._pct = null;
  }
  _c(...vs) {
    return vs.map((x) => {
      const c = coordinate(x);
      if (this._pct === null) this._pct = c.pct;
      else if (this._pct !== c.pct) throw new TypeError("PathBuilder: mix of percentage and absolute coordinates");
      return c.v;
    });
  }
  move_to(x, y) {
    this._d.push("M", ...this._c(x, y));
    return this;
  }
  line_to(x, y) {
    this._d.push("L", ...this._c(x, y));
    return this;
  }
  curve_to(toX, toY, cx, cy) {
    const [a, b, c, d] = this._c(cx, cy, toX, toY);
    this._d.push("Q", a, b, c, d);
    return this;
  }
  cubic_bezier_to(toX, toY, ax, ay, bx, by) {
    const [a, b, c, d, e, f] = this._c(ax, ay, bx, by, toX, toY);
    this._d.push("C", a, b, c, d, e, f);
    return this;
  }
  arc_to(rx, ry, rotation, large, sweep, toX, toY) {
    const [a, b] = this._c(rx, ry);
    const [x, y] = this._c(toX, toY);
    this._d.push("A", a, b, Number(rotation), large ? 1 : 0, sweep ? 1 : 0, x, y);
    return this;
  }
  add_polygon(points, closed = true) {
    points.forEach(([x, y], i) => this._d.push(i === 0 ? "M" : "L", ...this._c(x, y)));
    if (closed) this._d.push("Z");
    return this;
  }
  close() {
    this._d.push("Z");
    return this;
  }
  /** Dashes are not drawn by mobile shapes; accepted. */
  dash_array(_values) {
    return this;
  }
  build() {
    return Object.freeze({ __path: true, d: this._d.join(" "), stroke: this._stroke, pct: this._pct === true });
  }
}

export const PathBuilder = Object.freeze({
  fill: () => new PathBuilderObject(null),
  stroke: (width) => new PathBuilderObject(Number(width)),
});

/** gpui's `Background`: a solid color; gradients and patterns paint their first color. */
class BackgroundObject {
  constructor(color) {
    this._color = color;
    this._opacity = 1;
  }
  opacity(factor) {
    this._opacity = Number(factor);
    return this;
  }
  color_space(_space) {
    return this;
  }
}

const stopColor = (s) => (s && typeof s === "object" && "color" in s ? s.color : s);

export const Background = Object.freeze({
  solid: (color) => new BackgroundObject(String(color)),
  stop: (color, percentage) => ({ color: String(color), percentage: Number(percentage) }),
  linear_gradient: (_angle, from, _to) => new BackgroundObject(String(stopColor(from))),
  pattern_slash: (color) => new BackgroundObject(String(color)),
  checkerboard: (color) => new BackgroundObject(String(color)),
});

/** `window.paint_path(path, background | color)`: a native shape node. */
export function paintPath(path, paint) {
  if (!path?.__path) throw new TypeError("window.paint_path(path, background) expects a Path built by PathBuilder");
  const bg = paint instanceof BackgroundObject ? paint : new BackgroundObject(String(paint));
  const el = new Element("shape");
  const p = el._P();
  p.path = path.d;
  if (path.pct) p.view_box = [0, 0, 100, 100];
  if (path.stroke === null) p.fill = bg._color;
  else {
    p.stroke = bg._color;
    p.stroke_width = path.stroke;
  }
  if (bg._opacity < 1) el._sk += chainKey([["opacity", bg._opacity]]);
  return el;
}


/** gpui's `Window::paint_path`, so it lives on `window`. */
windowObject.paint_path = paintPath;
