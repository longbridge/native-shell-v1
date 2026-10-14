// Components, with gpui-base's names and behavior.
//
// gpui-base's components are headless: they carry activation, focus, value and
// accessibility, and the script draws them. The same code works here: a
// `Checkbox` or `Switch` the script draws (it was given children,
// `.headless()`, or a style that draws a box: background, border, radius,
// shadow, padding, text) is a pressable view that announces its state, a `Progress`
// with a `ProgressTrack` is a plain view. Left bare, the controls a phone has
// natively (switch, checkbox, slider, progress, segmented tabs, menus, date
// pickers) become those native controls instead. Which one is decided by how
// the element is built, never by what this render happens to pass `.child`:
// `.when(done, (el) => el.child(check))` on an otherwise bare checkbox would
// otherwise swap the view kind (and lose taps) every time `done` flips. And
// `Button(...).variant(...)` adds a native-looking preset drawn from theme
// tokens. Script style methods always apply after a preset, so they win.
//
// Controlled values follow gpui: a control reports what the user asked for
// (`on_change(checked)`) and shows what the script renders (`.checked(b)`).
// Text, and a slider's value, live in retained state (`InputState`,
// `SliderState`) so a drag or an IME composition is never interrupted by a
// re-render.

import { N, scope, markDirty, refuseInRender, refuseStateInRender, setTheme, theme, FocusHandle } from "nsh:core";
import { Element, chainKey } from "nsh:elements";

export const set_theme = (theme) => setTheme(theme);

function released(what, kind) {
  return new Error(`${what}: this ${kind} has been released`);
}

// Buttons and links -----------------------------------------------------------

// iOS button presets, under gpui-component's Button names: sizes `xsmall`,
// `small`, `medium` (default), `large` (the filled primary action), variants
// `primary`, `secondary`, `outline` (iOS "tinted"), `danger`, `success`,
// `warning`, `ghost`, `link`. Lengths are in rems so they follow the rem zoom;
// text sizes are points (renderers scale text by the font scale).
const BUTTON_SIZES = {
  xsmall: { h: "1.75rem", px: "0.625rem", radius: "0.5rem", text: 13, weight: [], slop: 8 },
  small: { h: "2.125rem", px: "0.75rem", radius: "0.5rem", text: 15, weight: [], slop: 5 },
  medium: { h: "2.75rem", px: "0.875rem", radius: "0.625rem", text: 17, weight: [], slop: 0 },
  large: { h: "3.125rem", px: "1rem", radius: "0.75rem", text: 17, weight: [["font_semibold"]], slop: 0 },
};

/** Fill and text tokens; filled variants darken when pressed, plain ones fade. */
const VARIANTS = {
  primary: { bg: "primary", fg: "primary_foreground" },
  secondary: { bg: "secondary", fg: "foreground" },
  outline: { bg: "accent", fg: "accent_foreground" },
  danger: { bg: "destructive", fg: "destructive_foreground" },
  success: { bg: "success", fg: "success_foreground" },
  warning: { bg: "warning", fg: "warning_foreground" },
  ghost: { bg: null, fg: "primary" },
  link: { bg: null, fg: "primary", bare: true },
};
const VARIANT_ALIASES = { destructive: "danger", tinted: "outline" };

/** `#rrggbb[aa]` 15% of the way to black: iOS's highlighted filled button. */
function pressed(hex) {
  const m = /^#([0-9a-f]{6})([0-9a-f]{2})?$/i.exec(hex ?? "");
  if (!m) return hex;
  const n = parseInt(m[1], 16);
  const ch = (shift) => Math.round(((n >> shift) & 255) * 0.85).toString(16).padStart(2, "0");
  return `#${ch(16)}${ch(8)}${ch(0)}${m[2] ?? ""}`;
}

function buttonPreset(variant, size, disabled) {
  const v = VARIANTS[variant];
  const z = BUTTON_SIZES[size];
  const ops = [["flex"], ["flex_row"], ["items_center"], ["justify_center"], ["gap_2"], ["text_size", z.text], ...z.weight];
  if (!v.bare) ops.push(["h", z.h], ["px", z.px], ["rounded", z.radius]);
  ops.push(["text_color", v.fg]);
  if (v.bg) ops.push(["bg", v.bg], ["@active", [["bg", pressed(theme()[v.bg])]]]);
  else ops.push(["@active", [["opacity", 0.6]]]);
  if (disabled) ops.push(["opacity", 0.4]);
  return chainKey(ops);
}

const ROW_KEY = chainKey([["flex"], ["flex_row"], ["items_center"]]);
const LABELED_KEY = chainKey([["flex"], ["flex_row"], ["items_center"], ["gap_2"]]);
const INPUT_KEY = chainKey([["flex"], ["items_center"]]);

/**
 * gpui-base's `Button`: activation, disabled and selected state, no styling
 * until a variant or a size is chosen (gpui-component's styled Button).
 */
class ButtonElement extends Element {
  constructor(id, role) {
    super("view");
    this._k = String(id);
    this._P().role = role;
    this._variant = null;
    this._size = null;
    this._slopSet = false;
  }

  _sync() {
    if (this._variant === null && this._size === null) return this;
    const size = this._size ?? "medium";
    // A small button keeps a 44 pt touch target.
    if (!this._slopSet) {
      const slop = BUTTON_SIZES[size].slop;
      if (slop > 0) this._P().hit_slop = [slop, slop, slop, slop];
      else delete this._P().hit_slop;
    }
    return this._preset(buttonPreset(this._variant ?? "secondary", size, this._dis));
  }

  /** Mobile: the button's text. */
  label(s) {
    return this.child(String(s));
  }

  /** The preset by name: gpui-component's variants (`destructive` and `tinted` are accepted too). */
  variant(v) {
    const name = VARIANT_ALIASES[v] ?? v;
    if (!(name in VARIANTS)) {
      throw new TypeError(`Button.variant(): use one of ${Object.keys(VARIANTS).join(", ")}, got ${JSON.stringify(v)}`);
    }
    this._variant = name;
    return this._sync();
  }

  primary() { return this.variant("primary"); }
  secondary() { return this.variant("secondary"); }
  outline() { return this.variant("outline"); }
  danger() { return this.variant("danger"); }
  success() { return this.variant("success"); }
  warning() { return this.variant("warning"); }
  ghost() { return this.variant("ghost"); }
  link() { return this.variant("link"); }

  /**
   * gpui-component's `size("xsmall" | "small" | "medium" | "large")`; a number
   * or length is the `size` style (width and height), as on any element.
   */
  size(v) {
    if (typeof v === "string" && v in BUTTON_SIZES) {
      this._size = v;
      return this._sync();
    }
    return super.size(v);
  }

  hit_slop(px) {
    this._slopSet = true;
    return super.hit_slop(px);
  }

  disabled(value = true) {
    super.disabled(value);
    return this._sync();
  }
}

export const Button = Object.freeze({ new: (id) => new ButtonElement(id, "button") });
/** An external link: `Link.new(id).href(url)`. */
export const Link = Object.freeze({ new: (id) => new ButtonElement(id, "link") });

// Toggles -------------------------------------------------------------------

/**
 * Mixed into the controls that have a native and a drawn form: any
 * `.child`/`.children` call (even one adding nothing), `.headless()`, or a
 * box-drawing style makes the element the drawn form; layout-only styles
 * place the native control (see `Converter::element`).
 */
const Drawable = (Base) =>
  class extends Base {
    child(x) {
      this._hl = true;
      return super.child(x);
    }

    children(xs) {
      this._hl = true;
      return super.children(xs);
    }

    /** Mobile: draw this control from its children and styles, even when it has none yet. */
    headless() {
      this._hl = true;
      return this;
    }
  };

/**
 * `Switch`, `Checkbox`, `Radio`: a native control when bare or only placed by
 * layout styles; given children or box styles, a pressable view the script
 * draws (as gpui-base's are), toggled by a tap.
 */
class ToggleElement extends Drawable(Element) {
  constructor(kind, id, style) {
    super(kind);
    this._k = String(id);
    this._lbl = null;
    this._P().checked = false;
    this._P().enabled = true;
    if (style !== undefined) this._P().check_style = style;
  }

  checked(value = true) {
    this._P().checked = !!value;
    return this;
  }

  disabled(value = true) {
    this._P().enabled = !value;
    return super.disabled(value);
  }

  /** Mobile: a text label beside the control. */
  label(s) {
    this._lbl = String(s);
    this._P().a11y_label = this._lbl;
    return this._preset(LABELED_KEY);
  }

  /** `(checked, cx)`: the state the user asked for. */
  on_change(fn) {
    if (typeof fn !== "function") throw new TypeError(".on_change(handler) expects a function");
    // The native control reports `{checked}`; a drawn one is a tap that flips the rendered state.
    this._on("change", (e, cx) => fn(!!e?.checked, cx), ".on_change(handler)");
    return this._on("click", (_e, cx) => fn(!this._P().checked, cx), ".on_change(handler)");
  }
}

export const Switch = Object.freeze({ new: (id) => new ToggleElement("switch", id) });
export const Checkbox = Object.freeze({ new: (id) => new ToggleElement("checkbox", id, 0) });
export const Radio = Object.freeze({ new: (id) => new ToggleElement("checkbox", id, 1) });

// Progress and spinner --------------------------------------------------------

/** `Progress.new()`: `.value(percent)` in `0..=100`, as gpui-base's. */
class ProgressElement extends Drawable(Element) {
  constructor(id) {
    super("progress");
    if (id !== undefined) this._k = String(id);
    this._P().value = 0;
    this._P().role = "progress_bar";
  }

  value(percent) {
    const p = Math.min(100, Math.max(0, Number(percent) || 0));
    this._P().value = p / 100;
    this._P().a11y_value = `${Math.round(p)}%`;
    this._P().indeterminate = false;
    return this;
  }

  indeterminate(value = true) {
    this._P().indeterminate = !!value;
    return this;
  }
}

export const Progress = Object.freeze({ new: (id) => new ProgressElement(id) });
/** The parts of a drawn progress bar: plain views the script sizes and colors. */
export const ProgressTrack = Object.freeze({ new: () => new Element("view") });
export const ProgressIndicator = Object.freeze({ new: () => new Element("view") });
export const Spinner = Object.freeze({ new: () => new Element("spinner") });

// Slider ----------------------------------------------------------------------

/** Retained slider state, as gpui-base's: `0..100` in steps of 1 unless told otherwise. */
export class SliderState {
  #released = false;

  static new(options = {}) {
    return new SliderState(options);
  }

  constructor(options = {}) {
    refuseStateInRender("SliderState.new(...)");
    this._min = Number(options.min ?? 0);
    this._max = Number(options.max ?? 100);
    this._step = Number(options.step ?? 1);
    this._value = Number(options.value ?? this._min);
    this._listeners = { change: [], release: [] };
    this._owner = scope.owner;
    this._shownBy = null;
  }

  #live(what) {
    if (this.#released) throw released(what, "slider state");
  }

  value() {
    this.#live("state.value()");
    return this._value;
  }

  set_value(next) {
    this.#live("state.set_value(value)");
    this._value = Math.min(this._max, Math.max(this._min, Number(next)));
    markDirty(this._shownBy ?? this._owner);
  }

  min_value() {
    return this._min;
  }

  max_value() {
    return this._max;
  }

  step_value() {
    return this._step;
  }

  /** `change` on every move of a drag, `release` once when it ends. */
  on(event, fn) {
    this.#live("state.on(event, fn)");
    if (!(event in this._listeners)) throw new TypeError(`SliderState.on(): unknown event ${JSON.stringify(event)}; use "change" or "release"`);
    if (typeof fn !== "function") throw new TypeError("state.on(event, fn) expects a function");
    this._listeners[event].push(fn);
    return true;
  }

  release() {
    const was = !this.#released;
    this.#released = true;
    this._listeners = { change: [], release: [] };
    return was;
  }

  _native(e, cx) {
    if (this.#released || !e) return;
    this._value = Number(e.value);
    for (const fn of [...this._listeners.change]) fn(this._value, cx);
    if (e.final !== false) for (const fn of [...this._listeners.release]) fn(this._value, cx);
  }
}

/**
 * A native slider. `Slider.new(state)` binds a `SliderState` (gpui-base's
 * form; the `SliderTrack`/`SliderIndicator`/`SliderThumb` parts are accepted
 * and drawn natively); `Slider.new(id).value(v).min(a).max(b).on_change(fn)`
 * is the controlled mobile form.
 */
class SliderElement extends Element {
  constructor(idOrState) {
    super("slider");
    Object.assign(this._P(), { value: 0, min: 0, max: 1, enabled: true, role: "slider" });
    if (idOrState instanceof SliderState) {
      const state = idOrState;
      state._shownBy = scope.owner;
      Object.assign(this._P(), { value: state._value, min: state._min, max: state._max, step: state._step });
      this._on("change", (e, cx) => state._native(e, cx), "Slider");
    } else {
      this._k = String(idOrState);
    }
  }
  value(v) {
    this._P().value = Number(v);
    return this;
  }
  min(v) {
    this._P().min = Number(v);
    return this;
  }
  max(v) {
    this._P().max = Number(v);
    return this;
  }
  step(v) {
    this._P().step = Number(v);
    return this;
  }
  disabled(value = true) {
    this._P().enabled = !value;
    return super.disabled(value);
  }
  on_change(fn) {
    return this._on("change", (e, cx) => fn(Number(e?.value), cx), ".on_change(handler)");
  }
}

export const Slider = Object.freeze({ new: (idOrState) => new SliderElement(idOrState) });
const sliderPart = Object.freeze({ new: () => new Element("view") });
export const SliderTrack = sliderPart;
export const SliderIndicator = sliderPart;
export const SliderThumb = sliderPart;

// Text input ----------------------------------------------------------------

const KEYBOARDS = { default: 0, number: 1, decimal: 2, email: 3, phone: 4, url: 5 };
const RETURN_KEYS = { default: 0, done: 1, go: 2, next: 3, search: 4, send: 5 };
const CONTENT_TYPES = { none: 0, one_time_code: 1, username: 2, password: 3, email: 4 };
const CAPITALIZE = { none: 0, words: 1, sentences: 2, characters: 3 };
const INPUT_EVENTS = ["change", "submit", "focus", "blur"];

function enumValue(table, value, what) {
  if (value === undefined || value === null) return undefined;
  if (!(value in table)) throw new TypeError(`${what}: use one of ${Object.keys(table).join(", ")}, got ${JSON.stringify(value)}`);
  return table[value];
}

let nextInput = 1;

/**
 * Retained text state, as gpui-base's: `InputState.new({placeholder, value})`,
 * `value()`, `set_value(text)`, `on("change" | "submit" | "focus" | "blur",
 * (event, cx) => …)`, `release()`. The mobile options (`keyboard`,
 * `return_key`, `max_length`, `allowed_chars`, `content_type`,
 * `auto_capitalize`, `secure`, `editable`) configure the native field.
 */
export class InputState {
  #released = false;

  static new(options = {}) {
    return new this(options);
  }

  constructor(options = {}) {
    refuseStateInRender(`${this.constructor.name}.new(...)`);
    this.id = nextInput++;
    this.placeholder = options.placeholder ?? "";
    this.secure = !!(options.secure ?? options.masked);
    this.keyboard = options.keyboard ?? "default";
    this.max_length = options.max_length ?? null;
    this.allowed_chars = options.allowed_chars ?? "";
    this.return_key = options.return_key ?? "default";
    this.content_type = options.content_type ?? null;
    this.auto_capitalize = options.auto_capitalize ?? null;
    this.editable = options.editable ?? true;
    this._value = String(options.value ?? "");
    // The latest native event count seen: a write carries it, and a renderer
    // that has seen newer keystrokes ignores the write.
    this._count = 0;
    this._composing = false;
    this._selection = [this._value.length, this._value.length];
    this._listeners = { change: [], submit: [], focus: [], blur: [] };
    this._numStep = null;
    this._numMin = null;
    this._numMax = null;
    this._loading = false;
    this._owner = scope.owner;
    this._shownBy = null;
    enumValue(KEYBOARDS, this.keyboard, "keyboard");
    enumValue(RETURN_KEYS, this.return_key, "return_key");
  }

  get multiline() {
    return false;
  }

  #live(what) {
    if (this.#released) throw released(what, "input state");
  }

  value() {
    this.#live("state.value()");
    return this._value;
  }

  set_value(next) {
    this.#live("state.set_value(text)");
    refuseInRender("state.set_value(text)", "update input state from an event handler");
    this._value = typeof next === "object" && next !== null ? String(next.text ?? "") : String(next);
    this._selection = [this._value.length, this._value.length];
    markDirty(this._shownBy ?? this._owner);
  }

  /** Mobile: the selection `[start, end]` in UTF-16 units, as the field last reported it. */
  selection() {
    return this._selection;
  }

  /** Mobile: whether an IME composition is in progress. */
  composing() {
    return this._composing;
  }

  set_masked(masked) {
    this.secure = !!masked;
    markDirty(this._shownBy ?? this._owner);
  }

  /** How much one `NumberInput` step moves the value; `null` stops stepping. */
  set_step(step) {
    this._numStep = step === null ? null : Number(step);
  }

  /** The lowest value stepping and blurring clamp to; `null` removes it. */
  set_min(min) {
    this._numMin = min === null ? null : Number(min);
  }

  set_max(max) {
    this._numMax = max === null ? null : Number(max);
  }

  /** Marks the state as working; the presentation is the script's. */
  set_loading(loading) {
    this._loading = !!loading;
  }

  _clamp(v) {
    if (this._numMin != null) v = Math.max(this._numMin, v);
    if (this._numMax != null) v = Math.min(this._numMax, v);
    return v;
  }

  /** A `NumberInput` step: the value moves by `dir` steps, clamped, and `change` fires. */
  _step(dir, cx) {
    if (this._numStep == null) return;
    const decimals = (String(this._numStep).split(".")[1] ?? "").length;
    const next = this._clamp((parseFloat(this._value) || 0) + dir * this._numStep);
    this._value = next.toFixed(decimals);
    this._selection = [this._value.length, this._value.length];
    markDirty(this._shownBy ?? this._owner);
    this._emit("change", { text: this._value, selection: this._selection, composing: false }, cx);
  }

  /** Blurring a bounded number field clamps what was typed. */
  _blurred(cx) {
    if (this._numMin != null || this._numMax != null) {
      const v = parseFloat(this._value);
      if (!Number.isNaN(v) && this._clamp(v) !== v) {
        this._value = String(this._clamp(v));
        markDirty(this._shownBy ?? this._owner);
      }
    }
    this._emit("blur", {}, cx);
  }

  focus() {
    this.#live("state.focus()");
    N.command("focus", this._ref, "{}");
  }

  blur() {
    this.#live("state.blur()");
    N.command("blur", this._ref, "{}");
  }

  on(event, fn) {
    this.#live("state.on(event, fn)");
    if (!INPUT_EVENTS.includes(event)) {
      throw new TypeError(`state.on(): unknown event ${JSON.stringify(event)}; use one of ${INPUT_EVENTS.join(", ")}`);
    }
    if (typeof fn !== "function") throw new TypeError("state.on(event, fn) expects a function");
    this._listeners[event].push(fn);
    return true;
  }

  release() {
    const was = !this.#released;
    this.#released = true;
    for (const k of INPUT_EVENTS) this._listeners[k] = [];
    return was;
  }

  get _ref() {
    return "__input:" + this.id;
  }

  _emit(event, payload, cx) {
    if (this.#released) return;
    for (const fn of [...this._listeners[event]]) fn(payload, cx);
  }

  /** A native Change: the text the user produced, already recorded by the tree. */
  _native(e, cx) {
    if (this.#released || !e || typeof e.text !== "string") return;
    this._value = e.text;
    this._count = Math.max(this._count, Number(e.count) | 0);
    this._composing = !!e.composing;
    this._selection = Array.isArray(e.selection) ? e.selection : [e.text.length, e.text.length];
    this._emit("change", { text: e.text, selection: this._selection, composing: this._composing }, cx);
  }
}

/** Retained multi-line text state: `TextareaState.new({placeholder, value, rows})`. */
export class TextareaState extends InputState {
  constructor(options = {}) {
    super(options);
    this.rows = options.rows ?? null;
  }

  get multiline() {
    return true;
  }

  set_rows(rows) {
    this.rows = Number(rows);
    markDirty(this._shownBy ?? this._owner);
  }

  /** The native field grows with its content; the bounds become a minimum row count. */
  set_auto_grow(min_rows, _max_rows) {
    this.set_rows(min_rows);
  }

  /** Native multi-line fields always wrap. */
  set_soft_wrap(_wrap) {}
}

function inputElement(state, multiline) {
  if (!(state instanceof InputState)) throw new TypeError(`${multiline ? "Textarea" : "Input"}.new(state) expects an InputState or TextareaState`);
  const el = new Element("input");
  // gpui-shell frames an input as a centered row; size and color are the script's.
  el._preset(multiline && state.rows ? INPUT_KEY + chainKey([["min_h", state.rows * 20]]) : INPUT_KEY);
  el._rid = state._ref;
  state._shownBy = scope.owner;
  const p = el._P();
  p.text_value = JSON.stringify({ text: state._value, count: state._count });
  p.placeholder = String(state.placeholder);
  p.placeholder_color = "muted_foreground";
  p.secure = state.secure;
  p.keyboard = enumValue(KEYBOARDS, state.keyboard, "keyboard");
  if (state.max_length !== null) p.max_length = Number(state.max_length) | 0;
  if (state.allowed_chars) p.allowed_chars = String(state.allowed_chars);
  p.return_key = enumValue(RETURN_KEYS, state.return_key, "return_key");
  const ct = enumValue(CONTENT_TYPES, state.content_type, "content_type");
  if (ct !== undefined) p.content_type = ct;
  const cap = enumValue(CAPITALIZE, state.auto_capitalize, "auto_capitalize");
  if (cap !== undefined) p.auto_capitalize = cap;
  p.editable = !!state.editable;
  p.multiline = multiline;
  p.role = "text_field";
  el._on("change", (e, cx) => state._native(e, cx), "Input change");
  el._on("submit", (_e, cx) => state._emit("submit", { text: state._value }, cx), "Input submit");
  el._on("focus", (_e, cx) => state._emit("focus", {}, cx), "Input focus");
  el._on("blur", (_e, cx) => state._blurred(cx), "Input blur");
  return el;
}

export const Input = Object.freeze({ new: (state) => inputElement(state, false) });
export const Textarea = Object.freeze({ new: (state) => inputElement(state, true) });

// Tabs, select, date picker -------------------------------------------------

/**
 * gpui-base's `Tabs`/`Tab`: a row of `Tab`s, each `selected(...)` in and
 * `on_click(...)` out. Mobile: `.items([...]).selected(i).on_change(fn)` makes
 * it a native segmented control instead.
 */
class TabsElement extends Element {
  constructor(id) {
    super("view");
    this._k = String(id);
    this._P().role = "tab_list";
    this._preset(ROW_KEY);
  }
  items(items) {
    this.__el = "segmented";
    this._preset("");
    this._P().items = JSON.stringify(items.map((i) => (typeof i === "object" && i !== null ? String(i.label) : String(i))));
    this._P().selected_index ??= 0;
    return this;
  }
  selected(i) {
    if (typeof i === "boolean") return super.selected(i);
    this._P().selected_index = Number(i) | 0;
    return this;
  }
  on_change(fn) {
    return this._on("change", (e, cx) => fn(Number(e?.index) | 0, cx), ".on_change(handler)");
  }
}

export const Tabs = Object.freeze({ new: (id) => new TabsElement(id) });
export const Tab = Object.freeze({
  new: (id) => {
    const el = new ButtonElement(id, "tab");
    return el;
  },
});

class SelectElement extends Element {
  constructor(id) {
    super("select");
    this._k = String(id);
    this._items = [];
    this._value = undefined;
    this._placeholder = "";
    this._sync();
  }
  _sync() {
    const index = this._items.findIndex((i) => i.value === this._value);
    this._P().menu = JSON.stringify({
      title: this._placeholder,
      placeholder: this._placeholder,
      items: this._items.map(({ label, value }) => ({ label, value })),
    });
    this._P().selected_index = index;
    this._P().placeholder = this._placeholder;
    return this;
  }
  items(items) {
    this._items = items.map((i) => (typeof i === "object" && i !== null ? { label: String(i.label), value: i.value } : { label: String(i), value: i }));
    return this._sync();
  }
  value(v) {
    this._value = v;
    return this._sync();
  }
  placeholder(s) {
    this._placeholder = String(s);
    return this._sync();
  }
  on_change(fn) {
    const items = () => this._items;
    return this._on("change", (e, cx) => fn(e && "value" in e ? e.value : items()[Number(e?.index)]?.value, cx), ".on_change(handler)");
  }
}

/**
 * gpui-base's headless roots (`Select`, `Combobox`, `DatePicker`): role,
 * announced open state, `open(...)` in and `on_open_change(open)` out; the
 * trigger and the list (a `Popup`) are the script's children.
 */
export class HeadlessPickerElement extends Element {
  constructor(id, role) {
    super("view");
    this._k = String(id);
    this._P().role = role;
    this._open = false;
  }
  open(value = true) {
    this._open = !!value;
    return this._state("expanded", this._open);
  }
  on_open_change(fn) {
    if (typeof fn !== "function") throw new TypeError(".on_open_change(handler) expects a function");
    return this._on("click", (_e, cx) => fn(!this._open, cx), ".on_open_change(handler)");
  }
}

/**
 * `Select.new(id)` is both: the native menu when given `.items(...)`, and
 * gpui-base's headless root when used with `.open(...)`, `.on_open_change(...)`
 * or children (the first such call turns it headless).
 */
class DualSelectElement extends SelectElement {
  _headless() {
    if (this.__el !== "view") {
      this.__el = "view";
      const p = this._P();
      for (const k of ["menu", "selected_index", "placeholder"]) delete p[k];
      p.role = "menu";
      this._open = false;
    }
    return this;
  }
  open(value = true) {
    this._headless()._open = !!value;
    return this._state("expanded", this._open);
  }
  on_open_change(fn) {
    if (typeof fn !== "function") throw new TypeError(".on_open_change(handler) expects a function");
    this._headless();
    return this._on("click", (_e, cx) => fn(!this._open, cx), ".on_open_change(handler)");
  }
  child(x) {
    this._headless();
    return super.child(x);
  }
  _sync() {
    return this.__el === "view" ? this : super._sync();
  }
}

export const Select = Object.freeze({ new: (id) => new DualSelectElement(id) });

const DATE_MODES = { date: 0, time: 1, datetime: 2 };

class DatePickerElement extends Element {
  constructor(id) {
    super("date");
    this._k = String(id);
    this._P().date_mode = 0;
  }
  value(s) {
    this._P().date = String(s);
    return this;
  }
  mode(m) {
    this._P().date_mode = enumValue(DATE_MODES, m, "DatePicker.mode()");
    return this;
  }
  on_change(fn) {
    return this._on("change", (e, cx) => fn(e?.date, cx), ".on_change(handler)");
  }
}

/**
 * `DatePicker.new(id)` is the native picker; `DatePicker.new(id, focus_handle)`
 * is gpui-base's headless root, which holds no date.
 */
export const DatePicker = Object.freeze({
  new: (id, focusHandle) => {
    if (focusHandle === undefined) return new DatePickerElement(id);
    if (!(focusHandle instanceof FocusHandle)) throw new TypeError("DatePicker.new(id, focus_handle) expects a FocusHandle from cx.focus_handle()");
    return new HeadlessPickerElement(id, "menu").track_focus(focusHandle);
  },
});
