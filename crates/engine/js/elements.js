// Element builders. An element is a plain record Rust walks after `render`
// returns: `__el` (kind), `_sk` (the style chain, encoded), `_pk` (a
// component's preset chain, applied first), `_k` (key), `_rid` (ref id), `_p`
// (typed props by name), `_h` (flat `event, handler id` pairs), `_c`
// (children), `_dis` (disabled: handlers dropped). Fields start with `_`
// because every bare short name (`p`, `h`, `w`…) is a style method; their
// defaults live on the prototype, so a fresh element costs one property.
//
// A style call appends to `_sk` and nothing else: the string is both the
// chain and its intern key. Rust parses it only the first time it meets it
// (see `convert.rs`), so a chain seen before costs one hash lookup. The
// encoding: every op is `RS name`, then `US type value` per argument (`n`
// number, `s` string, `b` boolean); `.active(...)` is `RS @active GS inner FS`.
//
// Style methods come from the vocabulary in Rust, so the two cannot drift. The prototype chain ends in a guard: reading any other name
// throws with the "did you mean" hint at the call site, instead of the
// `undefined is not a function` an author would otherwise get.

import { N, scope, handler, Entity, routeKey } from "nsh:core";

// Style method kinds, from Rust (`N.style_kind`): 1 nullary, 2 takes arguments, 3 both.
const NULLARY = 1;
const PARAM = 2;

// Names a runtime probes without meaning a style: promise detection, JSON,
// inspection by console and test frameworks.
const PROBES = new Set(["then", "toJSON", "constructor", "$$typeof", "asymmetricMatch", "nodeType", "inspect", "length"]);

function unknownMessage(name) {
  const hint = N.style_suggest(name);
  return hint ? `unknown style method \`${name}\` (did you mean: ${hint}?)` : `unknown style method \`${name}\``;
}

// Style methods are defined on first use: the guard sees the miss, asks Rust
// whether the name is a style, and installs the method on `Element.prototype`,
// where every later call finds it without reaching the guard again. A program
// pays for the few dozen styles it uses, not for the whole vocabulary.
const guard = new Proxy(
  {},
  {
    get(target, name, receiver) {
      if (typeof name === "symbol" || name in target || PROBES.has(name)) {
        return Reflect.get(target, name, receiver);
      }
      // Internal fields (no style name starts with `_`) default to undefined.
      if (name.startsWith("_")) return undefined;
      const kind = N.style_kind(name);
      if (kind === 0) throw new TypeError(unknownMessage(name));
      const method = styleMethod(name, kind);
      Object.defineProperty(Element.prototype, name, { value: method, writable: false, enumerable: false, configurable: false });
      return method;
    },
  },
);

const RS = "\x1e";
const US = "\x1f";
const GS = "\x1d";
const FS = "\x1c";
const SEPARATORS = /[\x1c-\x1f]/;

function encodeArg(name, v) {
  switch (typeof v) {
    case "number":
      return US + "n" + v;
    case "string":
      if (SEPARATORS.test(v)) throw new TypeError(`\`${name}\`: control characters are not allowed in style arguments`);
      return US + "s" + v;
    case "boolean":
      return v ? US + "b1" : US + "b0";
    case "object":
      if (v !== null) return US + "s" + JSON.stringify(v);
  }
  throw new TypeError(`\`${name}\`: unsupported argument ${String(v)}`);
}

/** The encoded chain of a preset written as `[name, ...args]` ops (`["@active", ops]` for a state). */
export function chainKey(ops) {
  let key = "";
  for (const op of ops) {
    if (op[0] === "@active") {
      key += RS + "@active" + GS + chainKey(op[1]) + FS;
    } else {
      key += RS + op[0];
      for (let i = 1; i < op.length; i++) key += encodeArg(op[0], op[i]);
    }
  }
  return key;
}

const A11Y_ROLES = new Set([
  "none", "button", "link", "header", "image", "text", "text_field", "switch", "checkbox", "radio", "slider",
  "progress_bar", "tab", "tab_list", "list", "list_item", "menu", "alert", "search",
]);

const EVENTS = {
  on_double_click: "double_click",
  on_long_press: "long_press",
  on_press_in: "press_in",
  on_press_out: "press_out",
  on_pan: "pan",
  on_pinch: "pinch",
  on_scroll: "scroll",
  on_submit: "submit",
  on_focus: "focus",
  on_blur: "blur",
};

/**
 * gpui-shell element methods for pointers, keyboards and windows that a touch
 * screen does not have. Accepted and ignored, so desktop render code runs.
 */
const DESKTOP_ONLY = [
  "on_mouse_move", "on_mouse_down", "on_mouse_up", "on_mouse_down_out", "on_scroll_wheel",
  "on_modifiers_changed", "tab_index", "tab_stop", "aria_level", "aria_active_descendant",
  "window_control_area", "content_focus_handle", "with_item_to_measure_index", "mode", "axis_scrollbar",
];

function isChild(x) {
  return typeof x === "string" || typeof x === "number" || x instanceof Element || x instanceof Entity;
}

export class Element {
  constructor(kind) {
    this.__el = kind;
  }

  /** The props object, created on first use. */
  _P() {
    return (this._p ??= {});
  }

  /** A component's own styles (ops as `[name, ...args]`), applied before the script's. */
  _preset(ops) {
    this._pk = typeof ops === "string" ? ops : chainKey(ops);
    return this;
  }

  _on(event, fn, what) {
    (this._h ??= []).push(event, handler(fn, what));
    return this;
  }

  // Composition -------------------------------------------------------------

  child(x) {
    if (Array.isArray(x)) return this.children(x);
    if (x === null || x === undefined || typeof x === "boolean") return this;
    if (!isChild(x)) {
      throw new TypeError(`.child() expects an element, an entity, a string or a number, got ${Object.prototype.toString.call(x)}`);
    }
    if (typeof x === "object") {
      // Parent links let a hardware key travel up the focus path (see core.js `routeKey`).
      if (x instanceof Element) x._parent = this;
      else x._mount(this);
    }
    (this._c ??= []).push(typeof x === "number" ? String(x) : x);
    return this;
  }

  children(xs) {
    if (xs === null || xs === undefined) return this;
    if (typeof xs[Symbol.iterator] !== "function") throw new TypeError(".children() expects an array or iterable");
    for (const x of xs) this.child(x);
    return this;
  }

  when(cond, fn) {
    return cond ? (fn(this) ?? this) : this;
  }

  /** Passes this element to `transform` and returns exactly what it returns (GPUI's `FluentBuilder::map`). */
  map(transform) {
    return transform(this);
  }

  key(k) {
    this._k = String(k);
    return this;
  }

  id(name) {
    this._rid = String(name);
    return this;
  }

  // State styles ------------------------------------------------------------

  active(fn) {
    return this._stateStyle("active", fn);
  }

  /** Styles while a pointer is over the element (pointer platforms; touch screens have none). */
  hover(fn) {
    return this._stateStyle("hover", fn);
  }

  _stateStyle(name, fn) {
    const s = new Element("style");
    fn(s);
    this._sk += RS + "@" + name + GS + s._sk + FS;
    return this;
  }

  /** `(hovered, cx)` when a pointer enters (`true`) or leaves the element, as gpui-shell's. */
  on_hover(fn) {
    if (typeof fn !== "function") throw new TypeError(".on_hover(handler) expects a function");
    return this._on("hover", (e, cx) => fn(!!e?.entered, cx), ".on_hover(handler)");
  }

  /**
   * Text the platform shows after a pointer rests on the element (macOS,
   * Android, Linux); a string only, as in gpui-shell.
   */
  tooltip(text) {
    if (typeof text !== "string") {
      throw new TypeError("tooltip(text) expects a string; a tooltip built from an element is not bound yet");
    }
    this._P().tooltip = text;
    return this;
  }

  // Focus rings are drawn natively: accepted (and still checked) so desktop
  // code keeps working, then ignored.

  focus(fn) {
    fn(new Element("style"));
    return this;
  }

  // Events ------------------------------------------------------------------

  on(name, fn) {
    return this._on(String(name), fn, `.on(${JSON.stringify(name)}, fn)`);
  }

  /** `(event: ClickEvent, cx)`: `click_count` and `modifiers` as in gpui-shell, plus the touch point. */
  on_click(fn) {
    return this._on("click", fn, ".on_click(handler)");
  }

  /** Opens `url` natively when the element is tapped (needs `capabilities.open_url`). */
  href(url) {
    const target = String(url);
    this._P().role ??= "link";
    return this._on("click", (_e, cx) => cx.open_url(target), ".href(url)");
  }

  _state(key, value) {
    const state = this._p?.a11y_state ? JSON.parse(this._p.a11y_state) : {};
    if (value) state[key] = true;
    else delete state[key];
    this._P().a11y_state = JSON.stringify(state);
    return this;
  }

  /** Drops the element's handlers and announces it disabled. */
  disabled(value = true) {
    this._dis = !!value;
    return this._state("disabled", this._dis);
  }

  selected(value = true) {
    return this._state("selected", !!value);
  }

  pressed(value = true) {
    return this._state("checked", !!value);
  }

  aria_selected(value = true) {
    return this.selected(value);
  }

  /** Moves the keyboard here when `handle.focus()` is called. */
  track_focus(handle) {
    this._rid = handle._ref;
    this._on("focus", () => (handle._focused = true), ".track_focus(handle)");
    return this._on("blur", () => (handle._focused = false), ".track_focus(handle)");
  }

  /**
   * Hardware-keyboard events reach an element through one `key` handler that
   * routes to `on_key_down`, `on_key_up` and the `on_action` handlers of any
   * binding (`cx.bind_keys`) whose context is this element's `key_context`.
   */
  _keys() {
    if (this._k_routes) return this._k_routes;
    this._k_routes = { down: null, up: null, actions: new Map(), context: null };
    this._on("key", (e, cx) => routeKey(this, e, cx), "key handler");
    return this._k_routes;
  }

  on_key_down(fn) {
    if (typeof fn !== "function") throw new TypeError(".on_key_down(handler) expects a function");
    this._keys().down = fn;
    return this;
  }

  on_key_up(fn) {
    if (typeof fn !== "function") throw new TypeError(".on_key_up(handler) expects a function");
    this._keys().up = fn;
    return this;
  }

  /** Runs `fn({action}, cx)` when a keystroke bound to `action` reaches this element. */
  on_action(action, fn) {
    if (typeof fn !== "function") throw new TypeError(".on_action(action, handler) expects a function");
    this._keys().actions.set(String(action), fn);
    return this;
  }

  /** The key context `cx.bind_keys` bindings match against. */
  key_context(context) {
    this._keys().context = String(context);
    return this;
  }

  /** Mobile: keep the element in the tree while hidden (gpui-base's `AccordionPanel`). */
  keep_mounted(value = true) {
    this._keep = !!value;
    return this;
  }

  on_refresh(fn) {
    this._P().refresh_enabled = true;
    return this._on("refresh", fn, ".on_refresh(fn)");
  }

  refreshing(b) {
    this._P().refreshing = !!b;
    return this;
  }

  // Accessibility and testing -----------------------------------------------

  role(name) {
    if (!A11Y_ROLES.has(name)) throw new TypeError(`.role(): unknown role ${JSON.stringify(name)}; use one of ${[...A11Y_ROLES].join(", ")}`);
    this._P().role = name;
    return this;
  }

  accessibility_label(s) {
    this._P().a11y_label = String(s);
    return this;
  }

  accessibility_hint(s) {
    this._P().a11y_hint = String(s);
    return this;
  }

  accessibility_value(s) {
    this._P().a11y_value = String(s);
    return this;
  }

  accessibility_hidden(b = true) {
    this._P().a11y_hidden = !!b;
    return this;
  }

  test_id(s) {
    this._P().test_id = String(s);
    return this;
  }

  hit_slop(px) {
    this._P().hit_slop = Array.isArray(px) ? px.map(Number) : [px, px, px, px].map(Number);
    return this;
  }
}

Object.assign(Element.prototype, {
  _k: null,
  _rid: null,
  _sk: "",
  _pk: "",
  _p: null,
  _h: null,
  _c: null,
  _dis: false,
  _lbl: null,
  _before: null,
  _after: null,
  _k_routes: null,
  _parent: null,
});

/** One generated method per style name; it knows its own arity rules. */
function styleMethod(name, kind) {
  const key = RS + name;
  const nullary = (kind & NULLARY) !== 0;
  const param = (kind & PARAM) !== 0;
  return function (a, b, c, d, e) {
    const n = arguments.length;
    if (n === 0) {
      if (!nullary) throw new TypeError(`\`${name}\` takes an argument`);
      this._sk += key;
      return this;
    }
    if (!param) throw new TypeError(`\`${name}\` takes no arguments`);
    let k = key + encodeArg(name, a);
    if (n > 1) k += encodeArg(name, b);
    if (n > 2) k += encodeArg(name, c);
    if (n > 3) k += encodeArg(name, d);
    if (n > 4) k += encodeArg(name, e);
    for (let i = 5; i < n; i++) k += encodeArg(name, arguments[i]);
    this._sk += k;
    return this;
  };
}

for (const name of DESKTOP_ONLY) {
  Object.defineProperty(Element.prototype, name, {
    value: function () {
      return this;
    },
  });
}

for (const [method, event] of Object.entries(EVENTS)) {
  Object.defineProperty(Element.prototype, method, {
    value: function (fn) {
      return this._on(event, fn, `.${method}(fn)`);
    },
  });
}

Object.setPrototypeOf(Element.prototype, guard);

// Builders ----------------------------------------------------------------

export function div() {
  return new Element("view");
}

const H_FLEX = chainKey([["flex"], ["flex_row"], ["items_center"]]);
const H_SCROLL = chainKey([["flex"], ["flex_row"], ["overflow_x_scroll"]]);
const V_FLEX = chainKey([["flex"], ["flex_col"]]);

/** A row: `div().flex().flex_row().items_center()`, as gpui-base's. */
export function h_flex() {
  const el = new Element("view");
  el._sk = H_FLEX;
  return el;
}

/** A column: `div().flex().flex_col()`. */
export function v_flex() {
  const el = new Element("view");
  el._sk = V_FLEX;
  return el;
}

class TextElement extends Element {}

/** A text leaf: a string, or an array of strings and spans. */
export function text(content) {
  const el = new TextElement("text");
  const parts = Array.isArray(content) ? content : [content];
  for (const part of parts) {
    if (part === null || part === undefined || part === false) continue;
    if (part instanceof SpanElement || typeof part === "string" || typeof part === "number") {
      (el._c ??= []).push(typeof part === "number" ? String(part) : part);
    } else {
      throw new TypeError("text() takes strings, numbers and span()s");
    }
  }
  return el;
}

class SpanElement extends Element {
  constructor(content, url) {
    super("span");
    this._url = url;
    this._c = [String(content)];
  }
}

/** A styled run of text. Style methods apply to the run only. */
export function span(content) {
  return new SpanElement(content ?? "", null);
}

const LINK_KEY = chainKey([["underline"], ["text_color", "primary"]]);

/** A link run: opens natively unless the element listens to `.on("link", …)`. */
export function link(url, label) {
  return new SpanElement(label ?? url, String(url))._preset(LINK_KEY);
}

const FITS = { cover: 0, contain: 1, fill: 2, center: 3 };

class ImageElement extends Element {
  object_fit(mode) {
    if (!(mode in FITS)) throw new TypeError(`.object_fit(): use "cover", "contain", "fill" or "center", got ${JSON.stringify(mode)}`);
    this._P().content_mode = FITS[mode];
    return this;
  }

  tint(color) {
    this._P().tint = String(color);
    return this;
  }
}

export function image(src) {
  const el = new ImageElement("image");
  el._P().src = N.check_image(String(src));
  return el;
}

/**
 * A vector image from the program's directory. As in gpui-shell it takes the
 * text color: its own `text_color(...)`, else the theme's foreground.
 */
export function svg(path) {
  const el = new ImageElement("svg");
  el._P().src = N.check_image(String(path));
  return el;
}

class ShapeElement extends Element {
  fill(color) {
    this._P().fill = String(color);
    return this;
  }

  stroke(color, width = 1) {
    this._P().stroke = String(color);
    this._P().stroke_width = Number(width);
    return this;
  }
}

export function shape(path, opts = {}) {
  const el = new ShapeElement("shape");
  el._P().path = String(path);
  let box = opts.view_box ?? [0, 0, 24, 24];
  if (typeof box === "string") box = box.trim().split(/[\s,]+/).map(Number);
  if (!Array.isArray(box) || box.length !== 4 || box.some((n) => !Number.isFinite(n))) {
    throw new TypeError("shape(): view_box must be [min_x, min_y, width, height]");
  }
  el._P().view_box = box;
  if (opts.fill_rule !== undefined) {
    if (opts.fill_rule !== "nonzero" && opts.fill_rule !== "evenodd") {
      throw new TypeError(`shape(): fill_rule must be "nonzero" or "evenodd", got ${JSON.stringify(opts.fill_rule)}`);
    }
    el._P().fill_rule = opts.fill_rule === "evenodd" ? 1 : 0;
  }
  return el;
}

class ListElement extends Element {
  estimated_row_height(px) {
    this._est = Number(px);
    return this;
  }

  /** `(key, cx)` for a tap on any row, with the row's stable key (gpui-shell). */
  on_item_click(fn) {
    if (typeof fn !== "function") throw new TypeError(".on_item_click(handler) expects a function");
    for (const r of this._c ?? []) {
      const key = r._k;
      const content = r._c?.[0];
      if (content instanceof Element && !(content._h ?? []).includes("click")) content._on("click", (_e, cx) => fn(key, cx), ".on_item_click(handler)");
    }
    return this;
  }

  /** Lets a `VirtualListScrollHandle` scroll this list from code. */
  track_scroll(handle) {
    handle._list = this._lid;
    handle._count = this._count;
    return this;
  }
}

function renderCx(what) {
  if (scope.phase !== "render") throw new Error(`${what} can only be built while rendering (inside render(cx))`);
  return scope.cx;
}

/** Each list's item count when it was last built. */
const builtCounts = new Map();

function listShell(id, count) {
  const el = new ListElement("list");
  el._lid = String(id);
  el._count = Math.max(0, Number(count) | 0);
  el._est = 44;
  el._uniform = false;
  el._rid = "__list:" + el._lid;
  let [first, last] = N.list_window(el._lid);
  // A window that reached the end of the list follows it as items are added,
  // until the renderer reports the new range (rows appended at the bottom of a
  // short list would otherwise stay missing for a frame or more).
  const before = builtCounts.get(el._lid);
  if (before !== undefined && last >= before && el._count > before) {
    last = Math.max(last, Math.min(el._count, first + Math.max(last - first, 20)));
  }
  builtCounts.set(el._lid, el._count);
  return [el, Math.min(first, el._count), Math.min(last, el._count)];
}

function row(index, key, content, list) {
  const r = new Element("row");
  r._parent = list;
  r._idx = index;
  r._k = String(key);
  r.child(content);
  return r;
}

/** A virtualized list: only the window the renderer reports is rendered. */
export function list(id, count, keyFn, rowFn) {
  const cx = renderCx("list()");
  const [el, first, last] = listShell(id, count);
  for (let i = first; i < last; i++) (el._c ??= []).push(row(i, keyFn(i), rowFn(i, cx), el));
  return el;
}

/**
 * Rows of equal height, rendered a range at a time. gpui-shell's form is
 * `uniform_list(id, count, get_key, (range, cx) => [el…])`; the key function
 * may be left out (`uniform_list(id, count, (range, cx) => [el…])`).
 */
/**
 * gpui-base's virtual lists. `item_sizes` is one extent or one per item; rows
 * are rendered for the reported range, as `uniform_list`. The horizontal one
 * is a horizontally scrolling row of every item (mobile lists are vertical).
 */
export function v_virtual_list(id, count, itemSizes, keyFn, rowsFn) {
  const el = uniform_list(id, count, keyFn, rowsFn);
  el._uniform = typeof itemSizes === "number";
  el._est = typeof itemSizes === "number" ? itemSizes : itemSizes.reduce((a, b) => a + Number(b), 0) / Math.max(1, itemSizes.length);
  return el;
}

export function h_virtual_list(id, count, itemSizes, keyFn, rowsFn) {
  const cx = renderCx("h_virtual_list()");
  const el = new ListElement("view");
  el._sk = H_SCROLL;
  el._rid = String(id);
  el._lid = String(id);
  el._count = count;
  const rows = rowsFn({ start: 0, end: count }, cx);
  if (!Array.isArray(rows)) throw new TypeError("h_virtual_list(): the row function must return an array");
  rows.forEach((content, i) => {
    if (content instanceof Element) {
      content._k ??= String(keyFn(i));
      if (typeof itemSizes === "number") content._sk += chainKey([["flex_none"], ["w", itemSizes]]);
      else if (Array.isArray(itemSizes)) content._sk += chainKey([["flex_none"], ["w", Number(itemSizes[i])]]);
    }
    el.child(content);
  });
  return el;
}

export function uniform_list(id, count, keyOrRows, maybeRows) {
  const cx = renderCx("uniform_list()");
  const [keyFn, rowsFn] = maybeRows === undefined ? [(i) => i, keyOrRows] : [keyOrRows, maybeRows];
  const [el, first, last] = listShell(id, count);
  el._uniform = true;
  if (last > first) {
    const rows = rowsFn({ start: first, end: last }, cx);
    if (!Array.isArray(rows)) throw new TypeError("uniform_list(): the row function must return an array");
    rows.slice(0, last - first).forEach((content, n) => (el._c ??= []).push(row(first + n, keyFn(first + n), content, el)));
  }
  return el;
}

class HostElement extends Element {
  on(name, fn) {
    if (this._events.length > 0 && !this._events.includes(name)) {
      throw new TypeError(`host component \`${this._hn}\` has no event ${JSON.stringify(name)}; it emits ${this._events.join(", ")}`);
    }
    if (typeof fn !== "function") throw new TypeError(`.on(${JSON.stringify(name)}, fn) expects a function`);
    this._hev[name] = fn;
    if (!this._h || !this._h.includes("host_event")) {
      const routes = this._hev;
      this._on("host_event", (e, cx) => routes[e?.name]?.(e.payload, cx), ".on(event, fn)");
    }
    return this;
  }
}

/** A native view the host app registered by name. */
export function host_component(name, props = {}) {
  const spec = JSON.parse(N.host_component(String(name)));
  const el = new HostElement("host");
  el._hn = String(name);
  el._events = spec.events ?? [];
  el._hev = {};
  el._P().host_props = JSON.stringify(props ?? {});
  return el;
}
