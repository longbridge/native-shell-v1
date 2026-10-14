// "gpui-component" forms: form layout, fields, input groups, rating,
// stepper, time field, clipboard and color picker.
//
// Names, constructors, methods and their errors are gpui-shell's; the look is
// iOS. A vertical form stacks labeled fields; a horizontal one is an inset
// grouped list whose rows put the label beside the control. Text sizes are
// iOS text styles in rems, so they follow the user's text size, and every
// color is a theme role.
//
// Composite components are laid out once their whole chain has run: a script
// may call `label(...)` after `child(...)`. That happens at the end of the
// render, or at the latest when Rust first reads the element (its preset
// chain, before its props, handlers and children). The parts carry handlers,
// which belong to the render that built the element, so laying out re-enters
// that render's scope; either moment comes before the render's handlers are
// settled.

import { scope, Entity, markDirty, startTimer, cancelTimer, popoverState, deferPopover, refuseStateInRender, windowInfo } from "nsh:core";
import { Element, chainKey, div, h_flex, v_flex, text, span, shape, svg } from "nsh:elements";
import { Input, InputState, TextareaState, DatePicker, Spinner } from "nsh:components";

// Shared --------------------------------------------------------------------------

/** A gpui-shell constructor: `Rating.new(id)` and `new Rating(id)` both work. */
function component(make) {
  function C(...args) {
    return make(...args);
  }
  C.new = make;
  return Object.freeze(C);
}

/** iOS text styles at the default text size (17 pt body), in rems so they scale. */
const TEXT = {
  body: "1.0625rem",
  callout: "1rem",
  subheadline: "0.9375rem",
  footnote: "0.8125rem",
};

const SIZES = ["xsmall", "small", "medium", "large"];

const ICONS = {
  star: "M12 2.6l2.9 5.9 6.5.95-4.7 4.6 1.1 6.5L12 17.5l-5.8 3.05 1.1-6.5-4.7-4.6 6.5-.95Z",
  check: "M20 6 9 17l-5-5",
  copy: "M10 8h9a2 2 0 0 1 2 2v9a2 2 0 0 1-2 2h-9a2 2 0 0 1-2-2v-9a2 2 0 0 1 2-2ZM16 8V5a2 2 0 0 0-2-2H5a2 2 0 0 0-2 2v9a2 2 0 0 0 2 2h3",
};

function icon(name, color, size, width = 2) {
  return shape(ICONS[name]).stroke(color, width).w(size).h(size).flex_none().accessibility_hidden();
}

function hairline() {
  return 1 / (windowInfo().scale || 1);
}

function nonEmptyId(component, id) {
  if (typeof id !== "string" && typeof id !== "number") throw new TypeError(`${component}(id) expects a string`);
  const s = String(id);
  if (s.trim() === "") throw new TypeError(`${component} id must not be empty`);
  return s;
}

function positiveInt(v, message) {
  if (typeof v !== "number" || !Number.isInteger(v) || v < 1 || v > 65535) throw new TypeError(message);
  return v;
}

function bool(v, message) {
  if (typeof v !== "boolean") throw new TypeError(message);
  return v;
}

function checkChild(x) {
  if (!(typeof x === "string" || typeof x === "number" || x instanceof Element || x instanceof Entity)) {
    throw new TypeError(`.child() expects an element, an entity, a string or a number, got ${Object.prototype.toString.call(x)}`);
  }
  return typeof x === "number" ? String(x) : x;
}

function received(x) {
  return x instanceof Element ? "an ordinary element" : typeof x;
}

/** `size("small")` is the component's size; any other argument is the `size` style. */
function componentSize(el, component, args) {
  const v = args[0];
  if (args.length !== 1 || typeof v !== "string" || !/^[a-z]+$/.test(v)) return false;
  if (!SIZES.includes(v)) throw new TypeError(`unsupported ${component} size \`${v}\``);
  el._size = v;
  return true;
}

/** A part's test id, derived from the component's own (`rating` → `rating-star-2`). */
function partId(el, part) {
  const id = el._p?.test_id;
  return id ? `${id}-${part}` : null;
}

function withId(el, id) {
  return id ? el.test_id(id) : el;
}

/** Plain text in one of the iOS text styles. */
function caption(content, style, color) {
  return div().text_size(TEXT[style]).text_color(color).child(content);
}

/**
 * A component laid out from its settings by `_build()`, which returns its
 * children and may set its own preset, props and handlers. The script's
 * children wait in `_kids`.
 */
class Composite extends Element {
  constructor() {
    super("view");
    this._kids = [];
    this._out = null;
    this.__pk = "";
    this._scope = scope.sink === null ? null : { ...scope };
    // Laid out as the render ends, so content handed to an anchored surface
    // (converted after its opener's render settles) is complete by then.
    if (scope.popovers !== null) {
      deferPopover(() => {
        this._realize();
        return null;
      });
    }
  }

  get _pk() {
    this._realize();
    return this.__pk;
  }

  set _pk(chain) {
    this.__pk = chain;
  }

  get _c() {
    this._realize();
    return this._out;
  }

  /** A parent changed a setting this element is laid out from. */
  _invalidate() {
    this._out = null;
  }

  _realize() {
    if (this._out !== null) return;
    const prev = { ...scope };
    if (this._scope !== null) Object.assign(scope, this._scope);
    try {
      this._out = [];
      this._out = this._build().filter((x) => x !== null && x !== undefined);
    } finally {
      Object.assign(scope, prev);
    }
    for (const c of this._out) {
      if (c instanceof Element) c._parent = this;
      else if (c instanceof Entity) c._mount(this);
    }
  }

  child(x) {
    if (Array.isArray(x)) return this.children(x);
    if (x === null || x === undefined || typeof x === "boolean") return this;
    this._kids.push(checkChild(x));
    return this;
  }

  children(xs) {
    if (xs === null || xs === undefined) return this;
    if (typeof xs[Symbol.iterator] !== "function") throw new TypeError(".children() expects an array or iterable");
    for (const x of xs) this.child(x);
    return this;
  }

  _build() {
    return this._kids;
  }
}

// Form and Field ---------------------------------------------------------------------

const FORM_GAP = { xsmall: 12, small: 16, medium: 20, large: 24 };
const FIELD_GAP = { xsmall: 4, small: 6, medium: 8, large: 8 };
const ROW_HEIGHT = { xsmall: 36, small: 40, medium: 44, large: 52 };
const LABEL_TEXT = { xsmall: "footnote", small: "footnote", medium: "subheadline", large: "callout" };

/**
 * `Form.new()` / `VForm.new()` stack their fields, labels above controls;
 * `HForm.new()` is an inset grouped list, each field a row with its label
 * beside the control. Children must be `Field`s. The form neither validates
 * nor submits: the script owns values, errors and the submit button.
 */
class FormElement extends Composite {
  constructor(horizontal) {
    super();
    this._horizontal = horizontal;
    this._columns = 1;
    this._labelWidth = null;
    this._size = "medium";
  }

  /** The number of columns fields are laid out in (`Field.col_span` to widen one). */
  columns(n) {
    this._columns = positiveInt(n, "Form.columns expects an exactly representable positive integer");
    return this;
  }

  /** The label column's width in a horizontal form, in points. */
  label_width(width) {
    if (typeof width !== "number" || !Number.isFinite(width) || width < 0) {
      throw new TypeError("Form.label_width(width) expects a nonnegative finite number");
    }
    this._labelWidth = width;
    return this;
  }

  size(...args) {
    return componentSize(this, "Form", args) ? this : super.size(...args);
  }

  child(x) {
    if (Array.isArray(x)) return this.children(x);
    if (x === null || x === undefined || typeof x === "boolean") return this;
    if (!(x instanceof FieldElement)) throw new TypeError(`Form accepts only registered Field children; received ${received(x)}`);
    this._kids.push(x);
    return this;
  }

  _build() {
    const fields = this._kids.filter((f) => f._visible);
    for (const f of fields) {
      f._form = this;
      f._invalidate();
    }
    if (this._horizontal) {
      this._preset([["flex"], ["flex_col"], ["rounded", 10], ["bg", "surface"], ["overflow_hidden"]]);
      const line = hairline();
      return fields.flatMap((f, i) => (i === 0 ? [f] : [div().h(line).ml_4().bg("border").flex_none(), f]));
    }
    const gap = FORM_GAP[this._size];
    this._preset(
      this._columns > 1
        ? [["grid"], ["grid_cols", this._columns], ["gap_x", 12], ["gap_y", gap]]
        : [["flex"], ["flex_col"], ["gap", gap]],
    );
    return fields;
  }
}

export const Form = component(() => new FormElement(false));
export const VForm = component(() => new FormElement(false));
export const HForm = component(() => new FormElement(true));

const FIELD_ALIGN = { start: "items_start", center: "items_center", end: "items_end" };

/**
 * One labeled control. `required` marks the label, `description` explains
 * the field under it, and (mobile) `error(message)` replaces the description
 * with the reason the value was refused, announced as an alert.
 */
class FieldElement extends Composite {
  constructor() {
    super();
    this._label = null;
    this._description = null;
    this._error = null;
    this._required = false;
    this._visible = true;
    this._indent = true;
    this._align = null;
    this._span = 1;
    this._form = null;
  }

  label(s) {
    if (typeof s !== "string") throw new TypeError("Field.label(label) expects a string");
    this._label = s;
    return this;
  }

  description(s) {
    if (typeof s !== "string") throw new TypeError("Field.description(description) expects a string");
    this._description = s;
    return this;
  }

  /** Mobile: why the value was refused, shown in place of the description; `null` clears it. */
  error(message) {
    if (message !== null && message !== undefined && typeof message !== "string") {
      throw new TypeError("Field.error(message) expects a string or null");
    }
    this._error = message || null;
    return this;
  }

  required(value = true) {
    this._required = bool(value, "Field.required(required) expects one boolean");
    return this;
  }

  visible(value = true) {
    this._visible = bool(value, "Field.visible(visible) expects one boolean");
    return this;
  }

  /** In a horizontal form, keeps an unlabeled field's control aligned with the labeled ones. */
  label_indent(value = true) {
    this._indent = bool(value, "Field.label_indent(label_indent) expects one boolean");
    return this;
  }

  align(value) {
    if (!(value in FIELD_ALIGN)) throw new TypeError(`unsupported Field alignment \`${value}\``);
    this._align = value;
    return this;
  }

  col_span(n) {
    this._span = positiveInt(n, "Field.col_span expects an exactly representable positive integer");
    return this;
  }

  _name() {
    return this._required ? `${this._label}, required` : this._label;
  }

  _labelText(style) {
    const parts = [this._label];
    if (this._required) parts.push(span(" *").text_color("destructive"));
    return withId(text(parts).text_size(TEXT[style]).text_color("foreground").accessibility_label(this._name()), partId(this, "label"));
  }

  _footer() {
    if (this._error !== null) return withId(caption(this._error, "footnote", "destructive").role("alert"), partId(this, "error"));
    if (this._description !== null) return withId(caption(this._description, "footnote", "muted_foreground"), partId(this, "description"));
    return null;
  }

  /** Names a lone control after the label, unless the script (or its group) named it. */
  _nameControl() {
    if (this._label === null || this._kids.length !== 1) return;
    const kid = this._kids[0];
    if (kid instanceof InputGroupElement && kid._name !== null) return;
    const target = kid instanceof InputGroupElement ? kid._input : kid;
    if (!(target instanceof Element) || !target._p?.role) return;
    target._p.a11y_label ??= this._name();
    if (this._error !== null) target._p.a11y_hint = this._error;
  }

  _build() {
    const form = this._form;
    const size = form?._size ?? "medium";
    const horizontal = !!form?._horizontal;
    const ops = [["flex"], ["flex_col"], ["min_w_0"]];
    if (!horizontal) ops.push(["gap", FIELD_GAP[size]]);
    if (this._span > 1) ops.push(["col_span", this._span]);
    if (!this._visible) ops.push(["hidden"]);
    this._preset(ops);
    if (!this._visible) return [];
    this._nameControl();
    if (horizontal) return this._row(form, size);
    return [
      this._label === null ? null : this._labelText(LABEL_TEXT[size]).font_medium(),
      v_flex()
        .gap_2()
        [FIELD_ALIGN[this._align] ?? "items_stretch"]()
        .children(this._kids),
      this._footer(),
    ];
  }

  _row(form, size) {
    for (const kid of this._kids) if (kid instanceof InputGroupElement) kid._setFlat(true);
    const showLabel = this._label !== null || this._indent;
    const footer = this._footer();
    return [
      h_flex()
        .px_4()
        .py_1p5()
        .gap_3()
        .min_h(ROW_HEIGHT[size])
        [FIELD_ALIGN[this._align ?? "center"]]()
        .when(showLabel, (el) =>
          el.child(
            div()
              .w(form._labelWidth ?? "6.25rem")
              .flex_none()
              .when(this._label !== null, (d) => d.child(this._labelText("body"))),
          ),
        )
        // Beside a label the control sits at the trailing edge, as iOS rows put values and switches.
        .child(h_flex().flex_1().min_w_0().gap_2().when(showLabel, (el) => el.justify_end()).children(this._kids)),
      footer === null ? null : div().px_4().pb_2().child(footer),
    ];
  }
}

export const Field = component(() => new FieldElement());

// InputGroup -------------------------------------------------------------------------

const GROUP_HEIGHT = { xsmall: 32, small: 36, medium: 44, large: 50 };
const GROUP_TEXT = { xsmall: "subheadline", small: "callout", medium: "body", large: "body" };

/**
 * A text field with text, icons or buttons around it inside one frame:
 * `.input(InputGroupInput.new(state))` and any number of `.addon(...)`s.
 * Inside a horizontal form's row the frame is dropped, as iOS draws fields in
 * grouped lists.
 */
class InputGroupElement extends Composite {
  constructor(id) {
    super();
    this._k = nonEmptyId("InputGroup", id);
    this._input = null;
    this._addons = [];
    this._disabled = false;
    this._readonly = false;
    this._invalid = false;
    this._size = "medium";
    this._flat = false;
    this._name = null;
  }

  _setFlat(flat) {
    this._flat = flat;
    this._invalidate();
  }

  input(part) {
    if (!(part instanceof InputGroupControl)) throw new TypeError("input expects one registered InputGroup part");
    this._input = part;
    return this;
  }

  addon(part) {
    if (!(part instanceof InputGroupAddonElement)) throw new TypeError("addon expects one registered InputGroup part");
    this._addons.push(part);
    return this;
  }

  disabled(value = true) {
    this._disabled = bool(value, "InputGroup.disabled(value) expects one boolean");
    return this;
  }

  readonly(value = true) {
    this._readonly = bool(value, "InputGroup.readonly(readonly) expects one boolean");
    return this;
  }

  /** Shows the script's validation result; edits are still accepted. */
  invalid(value = true) {
    this._invalid = bool(value, "InputGroup.invalid(invalid) expects one boolean");
    return this;
  }

  /** The platform draws its own focus; accepted so desktop code runs. */
  focus_ring(_value) {
    return this;
  }

  /** Names the group; its text control takes the name unless it has its own. */
  aria_label(s) {
    this._name = String(s);
    return this;
  }

  size(...args) {
    return componentSize(this, "InputGroup", args) ? this : super.size(...args);
  }

  _build() {
    const ops = [["flex"], ["flex_col"], ["min_w_0"]];
    if (this._flat) ops.push(["flex_1"]);
    else ops.push(["rounded", 10], ["border_1"], ["border_color", this._invalid ? "destructive" : "input"], ["bg", "surface"], ["overflow_hidden"]);
    if (this._disabled) ops.push(["opacity", 0.5]);
    this._preset(ops);
    const placed = (align) => this._addons.filter((a) => a._align === align).map((a) => a._place(this));
    const row = h_flex()
      .gap_2()
      .px(this._flat ? 0 : 12)
      .min_h(GROUP_HEIGHT[this._size])
      .children(placed("inline-start"))
      .child(this._input?._place(this) ?? null)
      .children(placed("inline-end"));
    return [...placed("block-start"), row, ...placed("block-end")];
  }
}

export const InputGroup = component((id) => new InputGroupElement(id));

const KEYBOARDS = { number: 1, email: 3, phone: 4, url: 5 };
const NATIVE_CONTENT = { one_time_code: 1, username: 2, password: 3, email: 4 };
const CAPITALIZE = { none: 0, words: 1, characters: 3 };

const WORDS = { cap: "words" };
const DIGITS = { keyboard: "number" };
const CODE = { cap: "characters" };

/** gpui's `InputContentType` names, and what each asks of the phone's keyboard. */
const CONTENT_TYPES = {
  name: WORDS,
  name_prefix: WORDS,
  given_name: WORDS,
  middle_name: WORDS,
  family_name: WORDS,
  name_suffix: WORDS,
  nickname: {},
  job_title: WORDS,
  organization_name: WORDS,
  location: WORDS,
  full_street_address: WORDS,
  street_address_line1: WORDS,
  street_address_line2: WORDS,
  address_city: WORDS,
  address_state: WORDS,
  address_city_and_state: WORDS,
  sublocality: WORDS,
  country_name: WORDS,
  postal_code: CODE,
  telephone_number: { keyboard: "phone" },
  email_address: { keyboard: "email", content: "email", cap: "none" },
  url: { keyboard: "url", cap: "none" },
  credit_card_number: DIGITS,
  credit_card_name: WORDS,
  credit_card_given_name: WORDS,
  credit_card_middle_name: WORDS,
  credit_card_family_name: WORDS,
  credit_card_security_code: DIGITS,
  credit_card_expiration: DIGITS,
  credit_card_expiration_month: DIGITS,
  credit_card_expiration_year: DIGITS,
  credit_card_type: {},
  username: { content: "username", cap: "none" },
  password: { content: "password", cap: "none", secure: true },
  new_password: { content: "password", cap: "none", secure: true },
  one_time_code: { keyboard: "number", content: "one_time_code" },
  shipment_tracking_number: CODE,
  flight_number: CODE,
  date_time: {},
  birthdate: {},
  birthdate_day: DIGITS,
  birthdate_month: DIGITS,
  birthdate_year: DIGITS,
  cellular_eid: DIGITS,
  cellular_imei: DIGITS,
};

/**
 * The text control of a group: the native field of `Input.new(state)`, with
 * the group's controlled-value methods. `on_change(value, cx)` reports edits;
 * `.value(text)` shows the script's text without reporting a change.
 */
class InputGroupControl extends Element {
  constructor(state, multiline) {
    super("input");
    const name = multiline ? "InputGroupTextarea" : "InputGroupInput";
    const kind = multiline ? TextareaState : InputState;
    if (!(state instanceof kind)) throw new TypeError(`${name} expects one ${kind.name} entity`);
    Object.assign(this, Input.new(state));
    this._what = name;
    this._state = state;
    this._disabled = false;
    this._readonly = false;
    this._rows = multiline ? (state.rows ?? 3) : null;
  }

  disabled(value = true) {
    this._disabled = bool(value, `${this._what}.disabled(value) expects one boolean`);
    return this;
  }

  readonly(value = true) {
    this._readonly = bool(value, `${this._what}.readonly(readonly) expects one boolean`);
    return this;
  }

  aria_label(s) {
    return this.accessibility_label(s);
  }

  accessibility_id(s) {
    return this.test_id(s);
  }

  value(s) {
    const next = String(s);
    const state = this._state;
    if (next !== state._value) {
      state._value = next;
      state._selection = [next.length, next.length];
    }
    this._P().text_value = JSON.stringify({ text: state._value, count: state._count });
    return this;
  }

  placeholder(s) {
    this._state.placeholder = String(s);
    this._P().placeholder = String(s);
    return this;
  }

  on_change(fn) {
    if (typeof fn !== "function") throw new TypeError("on_change expects one callback");
    const state = this._state;
    // The last handler for an event wins, so this one also feeds the state.
    return this._on(
      "change",
      (e, cx) => {
        state._native(e, cx);
        if (typeof e?.text === "string") fn(state._value, cx);
      },
      ".on_change(handler)",
    );
  }

  _place(group) {
    const p = this._P();
    const disabled = this._disabled || group._disabled;
    p.editable = !!this._state.editable && !(disabled || this._readonly || group._readonly);
    if (disabled) this._state_("disabled");
    if (group._name !== null) p.a11y_label ??= group._name;
    const ops = [["flex_1"], ["min_w_0"], ["text_size", TEXT[GROUP_TEXT[group._size]]], ["text_color", "foreground"]];
    if (this._rows !== null) ops.push(["py_2"], ["min_h", this._rows * 22 + 16]);
    if (!this._placed) this._pk += chainKey(ops);
    this._placed = true;
    return this;
  }

  _state_(key) {
    return Element.prototype._state.call(this, key, true);
  }
}

class InputGroupInputElement extends InputGroupControl {
  constructor(state) {
    super(state, false);
  }

  masked(value = true) {
    this._state.secure = bool(value, "InputGroupInput.masked(masked) expects one boolean");
    this._P().secure = value;
    return this;
  }

  content_type(value) {
    const spec = CONTENT_TYPES[value];
    if (spec === undefined) throw new TypeError(`unsupported content_type \`${value}\``);
    const p = this._P();
    if (spec.keyboard) p.keyboard = KEYBOARDS[spec.keyboard];
    if (spec.content) p.content_type = NATIVE_CONTENT[spec.content];
    if (spec.cap) p.auto_capitalize = CAPITALIZE[spec.cap];
    if (spec.secure) p.secure = true;
    return this;
  }
}

class InputGroupTextareaElement extends InputGroupControl {
  constructor(state) {
    super(state, true);
  }

  rows(n) {
    this._rows = positiveInt(n, "rows expects one positive integer");
    return this;
  }

  /** The native field grows with its text; the minimum sets its resting height. */
  auto_grow(min, max) {
    const lo = positiveInt(min, "auto_grow expects positive minimum and maximum rows");
    const hi = positiveInt(max, "auto_grow expects positive minimum and maximum rows");
    if (hi < lo) throw new TypeError("auto_grow requires max_rows >= min_rows");
    this._rows = lo;
    return this;
  }
}

export const InputGroupInput = component((state) => new InputGroupInputElement(state));
export const InputGroupTextarea = component((state) => new InputGroupTextareaElement(state));

const ADDON_ALIGN = ["inline-start", "inline-end", "block-start", "block-end"];

/** Text, icons or buttons beside (`inline-*`) or above and below (`block-*`) the field. */
class InputGroupAddonElement extends Composite {
  constructor(id) {
    super();
    this._k = nonEmptyId("InputGroupAddon", id);
    this._align = "inline-start";
  }

  align(value) {
    if (!ADDON_ALIGN.includes(value)) throw new TypeError(`unsupported align \`${value}\``);
    this._align = value;
    return this;
  }

  _place(group) {
    const ops = [["flex"], ["flex_row"], ["items_center"], ["gap_1p5"], ["flex_none"]];
    if (this._align.startsWith("block")) ops.push(["px", group._flat ? 0 : 12], [this._align === "block-start" ? "pt_2" : "pb_2"]);
    this._preset(ops);
    if (group._disabled) for (const kid of this._kids) if (kid instanceof InputGroupButtonElement) kid.disabled(true);
    const input = group._input;
    // As in gpui-component, a tap on the addon's text or empty space puts the caret in the field.
    if (input !== null && !group._disabled && !this._h?.includes("click")) this._on("click", () => input._state.focus(), "InputGroupAddon");
    return this;
  }
}

export const InputGroupAddon = component((id) => new InputGroupAddonElement(id));

const GROUP_TEXT_KEY = chainKey([["flex"], ["flex_row"], ["items_center"], ["flex_none"], ["text_size", TEXT.body], ["text_color", "muted_foreground"]]);

/** Muted helper text: a prefix, a suffix or a counter. */
export const InputGroupText = component(() => new Element("view")._preset(GROUP_TEXT_KEY));

const BUTTON_VARIANTS = ["default", "primary", "secondary", "danger", "warning", "success", "info", "ghost", "link", "text"];
/** Each variant's [text color, fill]; iOS draws in-field buttons as tinted text. */
const BUTTON_COLORS = {
  default: ["primary", null],
  ghost: ["primary", null],
  link: ["primary", null],
  text: ["primary", null],
  primary: ["primary_foreground", "primary"],
  secondary: ["secondary_foreground", "secondary"],
  danger: ["destructive", null],
  warning: ["warning", null],
  success: ["success", null],
  info: ["info", null],
};
const BUTTON_HEIGHT = { xsmall: 28, small: 32, medium: 36, large: 44 };

/** A compact button inside a group; its tap area reaches 44 pt. */
class InputGroupButtonElement extends Composite {
  constructor(id) {
    super();
    this._k = nonEmptyId("InputGroupButton", id);
    this._P().role = "button";
    this._variant = "default";
    this._outline = false;
    this._size = "xsmall";
    this._text = null;
    this._icon = null;
    this._loading = false;
    this._disabled = false;
  }

  label(s) {
    this._text = String(s);
    return this;
  }

  /** An icon from the program's assets, drawn in the button's color. */
  icon(path) {
    this._icon = String(path);
    return this;
  }

  aria_label(s) {
    return this.accessibility_label(s);
  }

  loading(value = true) {
    this._loading = bool(value, "InputGroupButton.loading(loading) expects one boolean");
    return super.disabled(this._disabled || this._loading);
  }

  outline() {
    this._outline = true;
    return this;
  }

  variant(value) {
    if (!BUTTON_VARIANTS.includes(value)) throw new TypeError(`unsupported variant \`${value}\``);
    this._variant = value;
    return this;
  }

  disabled(value = true) {
    this._disabled = bool(value, "InputGroupButton.disabled(value) expects one boolean");
    return super.disabled(this._disabled || this._loading);
  }

  size(...args) {
    return componentSize(this, "InputGroupButton", args) ? this : super.size(...args);
  }

  _build() {
    const [fg, bg] = BUTTON_COLORS[this._variant];
    const h = BUTTON_HEIGHT[this._size];
    const iconOnly = this._text === null && this._icon !== null && this._kids.length === 0;
    const ops = [["flex"], ["flex_row"], ["items_center"], ["justify_center"], ["gap_1"], ["flex_none"], ["min_h", h], ["rounded", 8]];
    ops.push(iconOnly ? ["min_w", h] : ["px", bg || this._outline ? 12 : 4]);
    ops.push(["text_size", TEXT.body], ["text_color", fg]);
    if (bg) ops.push(["bg", bg], ["font_semibold"]);
    if (this._outline) ops.push(["border_1"], ["border_color", "input"]);
    ops.push(this._dis ? ["opacity", 0.4] : ["@active", [["opacity", 0.4]]]);
    this._preset(ops);
    if (h < 44) this.hit_slop((44 - h) / 2);
    const out = [];
    if (this._loading) out.push(Spinner.new().size_4());
    else if (this._icon !== null) out.push(svg(this._icon).text_color(fg).w(18).h(18).flex_none().accessibility_hidden());
    if (this._text !== null) out.push(this._text);
    return [...out, ...this._kids];
  }
}

export const InputGroupButton = component((id) => new InputGroupButtonElement(id));

// Rating ----------------------------------------------------------------------------

const STAR_SIZE = { xsmall: 14, small: 18, medium: 28, large: 36 };
const STAR_GAP = { xsmall: 2, small: 2, medium: 6, large: 8 };

function starCount(v, name, halves) {
  const ok = typeof v === "number" && Number.isFinite(v) && v >= 0 && (halves || Number.isInteger(v));
  if (!ok) throw new TypeError(`Rating.${name}(${name}) expects a non-negative ${halves ? "number" : "integer"}, got ${v}`);
  return v;
}

function stars(n) {
  return `${n} ${n === 1 ? "star" : "stars"}`;
}

/**
 * Stars. Tapping an empty star sets the rating to it; tapping a filled one
 * lowers the rating to the star before it, as gpui-component does. Without
 * `on_change`, or disabled, it is a read-only display (not dimmed) that may
 * show halves (`.value(4.5)`). Arrow keys and digits change it from a
 * hardware keyboard.
 */
class RatingElement extends Composite {
  constructor(id) {
    super();
    this._k = nonEmptyId("Rating", id);
    this._value = 0;
    this._max = 5;
    this._color = "warning";
    this._size = "medium";
    this._disabled = false;
    this._onChange = null;
    this._shown = 0;
  }

  /** The number of filled stars; mobile also takes halves, for an average. */
  value(v) {
    this._value = Math.round(starCount(v, "value", true) * 2) / 2;
    return this;
  }

  max(v) {
    this._max = starCount(v, "max", false);
    return this;
  }

  color(c) {
    if (typeof c !== "string") throw new TypeError("Rating.color(color) expects a color string");
    this._color = c;
    return this;
  }

  disabled(value = true) {
    this._disabled = bool(value, "Rating.disabled(value) expects one boolean");
    return this;
  }

  /** `(value, cx)`: the rating the reader chose, for the script to show with `.value(...)`. */
  on_change(fn) {
    if (typeof fn !== "function") throw new TypeError("Rating.on_change expects one callback");
    this._onChange = fn;
    return this;
  }

  size(...args) {
    return componentSize(this, "Rating", args) ? this : super.size(...args);
  }

  _pick(next, cx) {
    if (next === this._shown) return;
    cx.haptic("selection");
    this._onChange(next, cx);
  }

  _star(ix, value) {
    const s = STAR_SIZE[this._size];
    const color = this._color;
    const filled = ix <= value;
    const star = icon("star", filled ? color : "muted_foreground", s, 1.5).when(filled, (el) => el.fill(color));
    if (filled || ix - 0.5 !== value) return star;
    return div()
      .relative()
      .w(s)
      .h(s)
      .flex_none()
      .child(star)
      .child(div().absolute().top_0().left_0().h(s).w(s / 2).overflow_hidden().child(icon("star", color, s, 1.5).fill(color)));
  }

  _build() {
    const max = this._max;
    const value = Math.min(this._value, max);
    this._shown = value;
    const gap = STAR_GAP[this._size];
    this._preset([["flex"], ["flex_row"], ["items_center"], ["flex_none"]]);
    const p = this._P();
    if (this._disabled || this._onChange === null) {
      // One element for assistive technology: "4.5 out of 5 stars".
      p.role = "image";
      p.a11y_label ??= `${value} out of ${stars(max)}`;
      const out = [];
      for (let ix = 1; ix <= max; ix++) out.push(this._star(ix, value).when(ix > 1, (el) => el.ml(gap)));
      return out;
    }
    p.a11y_value = `${value} of ${max}`;
    this.on_key_down((e, cx) => {
      if (e.key === "right" || e.key === "up") this._pick(Math.min(max, Math.floor(value) + 1), cx);
      else if (e.key === "left" || e.key === "down") this._pick(Math.max(0, Math.ceil(value) - 1), cx);
      else if (/^[0-9]$/.test(e.key) && Number(e.key) <= max) this._pick(Number(e.key), cx);
    });
    const s = STAR_SIZE[this._size];
    const slopY = Math.max(0, (44 - s) / 2);
    const slopX = Math.max(0, (44 - s - gap) / 2);
    const out = [];
    for (let ix = 1; ix <= max; ix++) {
      const next = value >= ix ? ix - 1 : ix;
      out.push(
        withId(div(), partId(this, `star-${ix}`))
          .key(`star-${ix}`)
          .role("button")
          .accessibility_label(stars(ix))
          .when(ix <= value, (el) => el.selected(true))
          .flex_none()
          .px(gap / 2)
          .hit_slop([slopY, slopX, slopY, slopX])
          .active((st) => st.opacity(0.6))
          .on_click((_e, cx) => this._pick(next, cx))
          .child(this._star(ix, value)),
      );
    }
    return out;
  }
}

export const Rating = component((id) => new RatingElement(id));

// Stepper ---------------------------------------------------------------------------

const STEP_DOT = { xsmall: 12, small: 24, medium: 28, large: 32 };

/** One step of a `Stepper`: its title (children), an optional icon, and whether it can be chosen. */
class StepperItemElement extends Composite {
  constructor() {
    super();
    this._disabled = false;
    this._icon = null;
    this._stepper = null;
    this._at = null;
  }

  disabled(value = true) {
    this._disabled = bool(value, "StepperItem.disabled(disabled) expects one boolean");
    return this;
  }

  /** An icon from the program's assets in place of the step number. */
  icon(path) {
    this._icon = String(path);
    return this;
  }

  _dot(done, current, size) {
    const d = STEP_DOT[size];
    const fg = done || current ? "primary_foreground" : "secondary_foreground";
    let mark = null;
    if (size !== "xsmall") {
      if (done) mark = icon("check", fg, d * 0.55, 2.5);
      else if (this._icon !== null) mark = svg(this._icon).text_color(fg).w(d * 0.55).h(d * 0.55);
      else mark = text(String(this._at.index + 1)).text_size(TEXT.footnote).font_semibold().text_color(fg);
    }
    return div()
      .w(d)
      .h(d)
      .flex_none()
      .rounded_full()
      .flex()
      .items_center()
      .justify_center()
      .bg(done || current ? "primary" : "secondary")
      .child(mark);
  }

  _build() {
    const s = this._stepper;
    if (s === null) return this._kids;
    const { index, total } = this._at;
    const selected = s._selected;
    const vertical = s._vertical;
    const center = s._center && !vertical;
    const size = s._size;
    const done = index < selected;
    const current = index === selected;
    const disabled = s._disabled || this._disabled;
    const first = index === 0;
    const last = index === total - 1;

    const p = this._P();
    p.role = "button";
    const title = this._kids.filter((k) => typeof k === "string").join(" ");
    const where = `Step ${index + 1} of ${total}`;
    p.a11y_label ??= title ? `${where}, ${title}` : where;
    if (done) p.a11y_value = "Completed";
    this.selected(current);
    if (disabled) super.disabled(true);
    else if (!this._h?.includes("click")) this._on("click", (_e, cx) => s._choose(index, cx), "StepperItem");

    const line = (lit, shown) =>
      (vertical ? div().w(2).flex_1().min_h_4() : div().h(2).flex_1()).rounded_full().bg(shown ? (lit ? "primary" : "border") : "transparent");
    const body = v_flex()
      .gap_0p5()
      .min_w_0()
      .text_size(size === "large" ? TEXT.subheadline : TEXT.footnote)
      .text_color(done || current ? "foreground" : "muted_foreground")
      .when(current, (el) => el.font_semibold())
      .when(center, (el) => el.items_center().text_center())
      .children(this._kids.map((k) => (typeof k === "string" ? text(k) : k)));
    const pressed = disabled ? ["opacity", 0.4] : ["@active", [["opacity", 0.6]]];
    const dot = this._dot(done, current, size);

    if (vertical) {
      // The connector runs from this step's dot down to the next one.
      this._preset([["flex"], ["flex_row"], ["items_stretch"], ["gap_3"], pressed]);
      return [
        v_flex().items_center().flex_none().gap_1().child(dot).when(!last, (el) => el.pb_1().child(line(done, true))),
        body.flex_1().pt(Math.max(0, (STEP_DOT[size] - 18) / 2)).pb(last ? 0 : 20),
      ];
    }
    const ops = [["flex"], ["flex_col"], ["gap_2"], ["min_w_0"], pressed];
    if (center || !last) ops.push(["flex_1"]);
    this._preset(ops);
    const track = h_flex().gap_2();
    if (center) track.child(line(index <= selected, !first)).child(dot).child(line(done, !last));
    else track.child(dot).when(!last, (el) => el.pr_2().child(line(done, true)));
    return [track, body];
  }
}

export const StepperItem = component(() => new StepperItemElement());

/**
 * Progress through a series of steps, horizontal by default. Steps before the
 * selected one are done (a check), the selected one is current; tapping a
 * step reports its index through `on_change`, for the script to select it.
 */
class StepperElement extends Composite {
  constructor(id) {
    super();
    this._k = nonEmptyId("Stepper", id);
    this._selected = 0;
    this._vertical = false;
    this._center = false;
    this._disabled = false;
    this._size = "medium";
    this._onChange = null;
    this._P().role = "list";
  }

  selected_index(i) {
    if (typeof i !== "number" || !Number.isInteger(i) || i < 0) throw new TypeError("Stepper.selected_index(index) expects a non-negative integer");
    this._selected = i;
    return this;
  }

  vertical(value = true) {
    this._vertical = bool(value, "Stepper.vertical(vertical) expects one boolean");
    return this;
  }

  text_center(value = true) {
    this._center = bool(value, "Stepper.text_center(text_center) expects one boolean");
    return this;
  }

  disabled(value = true) {
    this._disabled = bool(value, "Stepper.disabled(disabled) expects one boolean");
    return this;
  }

  size(...args) {
    return componentSize(this, "Stepper", args) ? this : super.size(...args);
  }

  /** `(index, cx)` when a step is tapped. */
  on_change(fn) {
    if (typeof fn !== "function") throw new TypeError("Stepper.on_change(callback) expects a callback");
    this._onChange = fn;
    return this;
  }

  /** The Rust spelling: `items([...])`. */
  items(items) {
    return this.children(items);
  }

  item(item) {
    return this.child(item);
  }

  child(x) {
    if (Array.isArray(x)) return this.children(x);
    if (x === null || x === undefined || typeof x === "boolean") return this;
    if (!(x instanceof StepperItemElement)) throw new TypeError(`Stepper accepts only registered StepperItem children; received ${received(x)}`);
    this._kids.push(x);
    return this;
  }

  _choose(index, cx) {
    if (index === this._selected) return;
    cx.haptic("selection");
    this._onChange?.(index, cx);
  }

  _build() {
    this._preset(this._vertical ? [["flex"], ["flex_col"], ["w_full"]] : [["flex"], ["flex_row"], ["items_start"], ["w_full"], ["gap_2"]]);
    const total = this._kids.length;
    this._kids.forEach((item, index) => {
      item._stepper = this;
      item._at = { index, total };
      item._invalidate();
      item.key(`step-${index}`);
      const id = partId(this, `step-${index}`);
      if (id && !item._p?.test_id) item.test_id(id);
    });
    return this._kids;
  }
}

export const Stepper = component((id) => new StepperElement(id));

// TimeField ---------------------------------------------------------------------------

const TIME = /^([01]\d|2[0-3]):([0-5]\d)(?::([0-5]\d))?$/;

function released(what, kind) {
  return new Error(`${what}: this ${kind} has been released`);
}

/**
 * A time of day, `"HH:MM"` (`"HH:MM:SS"` with `precision: "second"`), as
 * gpui-component's `TimeFieldState`. It starts at `00:00` unless given a
 * `value`. `set_value` does not report a change; edits do, through
 * `on("change", (time, cx))`.
 */
export class TimeFieldState {
  #released = false;

  static new(options = {}) {
    return new TimeFieldState(options);
  }

  constructor(options = {}) {
    refuseStateInRender("TimeFieldState.new(...)");
    const precision = options.precision ?? "minute";
    if (precision !== "minute" && precision !== "second") throw new TypeError(`unsupported precision \`${precision}\`; use "minute" or "second"`);
    const cycle = options.hour_cycle ?? "h24";
    if (cycle !== "h24" && cycle !== "h12") throw new TypeError(`unsupported hour_cycle \`${cycle}\`; use "h24" or "h12"`);
    this._precision = precision;
    this._cycle = cycle;
    this._value = "00:00:00";
    this._listeners = [];
    this._owner = scope.owner;
    if (options.value !== undefined) this.#set(options.value, "TimeFieldState.new({value})");
  }

  #live(what) {
    if (this.#released) throw released(what, "time field state");
  }

  #set(v, what) {
    const m = TIME.exec(String(v));
    if (!m) throw new TypeError(`${what} expects "HH:MM" or "HH:MM:SS", got ${JSON.stringify(v)}`);
    this._value = `${m[1]}:${m[2]}:${m[3] ?? "00"}`;
  }

  value() {
    this.#live("state.value()");
    return this._precision === "second" ? this._value : this._value.slice(0, 5);
  }

  set_value(v) {
    this.#live("state.set_value(time)");
    this.#set(v, "state.set_value(time)");
    markDirty(this._owner);
  }

  /** The clock asked for; the platform's picker follows the user's 12/24-hour setting. */
  hour_cycle() {
    return this._cycle;
  }

  on(event, fn) {
    this.#live("state.on(event, fn)");
    if (event !== "change") throw new TypeError(`TimeFieldState.on(): unknown event ${JSON.stringify(event)}; use "change"`);
    if (typeof fn !== "function") throw new TypeError("state.on(event, fn) expects a function");
    this._listeners.push(fn);
    return true;
  }

  release() {
    const was = !this.#released;
    this.#released = true;
    this._listeners = [];
    return was;
  }

  _native(time, cx) {
    const m = this.#released || typeof time !== "string" ? null : TIME.exec(time);
    if (!m) return;
    // The picker edits hours and minutes; seconds are kept.
    this._value = `${m[1]}:${m[2]}:${this._value.slice(6)}`;
    const v = this.value();
    for (const fn of [...this._listeners]) fn(v, cx);
  }
}

/**
 * `TimeField.new(state)`: the platform's time picker (a compact, tappable
 * time on iOS) showing the state's time in the user's clock format.
 */
class TimeFieldElement extends Element {
  constructor(state) {
    super("view");
    if (!(state instanceof TimeFieldState)) throw new TypeError("TimeField expects one TimeFieldState entity");
    Object.assign(this, DatePicker.new("time").mode("time").value(state._value.slice(0, 5)));
    this._k = null;
    this._disabledFlag = false;
    this._invalidFlag = false;
    state._owner = scope.owner ?? state._owner;
    this._on("change", (e, cx) => state._native(e?.date, cx), "TimeField");
  }

  _sync() {
    const ops = [];
    if (this._invalidFlag) ops.push(["rounded", 8], ["border_1"], ["border_color", "destructive"]);
    if (this._disabledFlag) ops.push(["opacity", 0.4]);
    return this._preset(ops);
  }

  disabled(value = true) {
    this._disabledFlag = bool(value, "TimeField.disabled(value) expects one boolean");
    this._P().enabled = !value;
    super.disabled(value);
    return this._sync();
  }

  /** Mobile (gpui-component's Rust API): shows the script's validation result. */
  invalid(value = true) {
    this._invalidFlag = bool(value, "TimeField.invalid(invalid) expects one boolean");
    return this._sync();
  }

  size(...args) {
    return componentSize(this, "TimeField", args) ? this : super.size(...args);
  }
}

export const TimeField = component((state) => new TimeFieldElement(state));

// Clipboard ---------------------------------------------------------------------------

/** The copy buttons showing their confirmation, by view and id, with the timer that ends it. */
const copied = new Map();
const COPIED_MS = 2000;

/**
 * A copy button: copies `value` and, unless the platform confirms copies
 * itself (`cx.window().clipboard_feedback`), shows a check for two seconds.
 * Copying needs `capabilities.clipboard.write`.
 */
class ClipboardElement extends Composite {
  constructor(id) {
    super();
    this._k = nonEmptyId("Clipboard", id);
    this._owner = scope.owner;
    this._key = `${scope.owner?.id ?? 0}:${this._k}`;
    this._value = "";
    this._valueFn = null;
    this._onCopied = null;
    this._P().role = "button";
    this.hit_slop(6);
  }

  value(s) {
    if (typeof s !== "string") throw new TypeError("Clipboard.value(value) expects a string");
    this._value = s;
    return this;
  }

  /** Mobile (gpui-component's Rust API): computes the text when the button is tapped. */
  value_fn(fn) {
    if (typeof fn !== "function") throw new TypeError("Clipboard.value_fn(fn) expects a function");
    this._valueFn = fn;
    return this;
  }

  /** Mobile (gpui-component's Rust API): `(value, cx)` once the text is on the clipboard. */
  on_copied(fn) {
    if (typeof fn !== "function") throw new TypeError("Clipboard.on_copied(fn) expects a function");
    this._onCopied = fn;
    return this;
  }

  _copy(cx) {
    const value = this._valueFn ? String(this._valueFn(cx) ?? "") : this._value;
    cx.write_to_clipboard(value);
    if (!cx.window().clipboard_feedback) {
      const owner = this._owner;
      const key = this._key;
      if (copied.has(key)) cancelTimer(copied.get(key));
      const done = () => {
        copied.delete(key);
        markDirty(owner);
      };
      copied.set(key, startTimer(COPIED_MS, false, owner, done));
      cx.haptic("success");
      markDirty(owner);
    }
    this._onCopied?.(value, cx);
  }

  _build() {
    const done = copied.has(this._key);
    this._preset([
      ["flex"], ["items_center"], ["justify_center"], ["flex_none"], ["w", 32], ["h", 32], ["rounded", 8],
      this._dis ? ["opacity", 0.4] : ["@active", [["opacity", 0.4]]],
    ]);
    const p = this._P();
    p.a11y_label ??= "Copy";
    if (done) p.a11y_value = "Copied";
    if (!this._h?.includes("click")) this._on("click", (_e, cx) => this._copy(cx), "Clipboard");
    return [done ? icon("check", "success", 18, 2.25) : icon("copy", "primary", 18)];
  }
}

export const Clipboard = component((id) => new ClipboardElement(id));

// ColorPicker ---------------------------------------------------------------------------

const HEX = /^#?([0-9a-f]{6})$/i;

function normalizeHex(v) {
  const m = HEX.exec(String(v).trim());
  return m ? `#${m[1].toUpperCase()}` : null;
}

function hsl(h, s, l) {
  const k = (n) => (n + h / 30) % 12;
  const a = s * Math.min(l, 1 - l);
  const f = (n) => Math.round(255 * (l - a * Math.max(-1, Math.min(k(n) - 3, 9 - k(n), 1))));
  return `#${[f(0), f(8), f(4)].map((x) => x.toString(16).padStart(2, "0")).join("").toUpperCase()}`;
}

/** The iOS system colors (light appearance), offered first unless `featured_colors` says otherwise. */
const FEATURED = ["#FF3B30", "#FF9500", "#FFCC00", "#34C759", "#00C7BE", "#30B0C7", "#32ADE6", "#007AFF", "#5856D6", "#AF52DE", "#FF2D55", "#A2845E"];

/** gpui-component's palette families (stone, red … pink), five shades each, light to dark. */
const PALETTE = [
  [30, 0.06],
  [4, 0.86],
  [28, 0.92],
  [48, 0.95],
  [142, 0.62],
  [188, 0.8],
  [214, 0.9],
  [268, 0.7],
  [330, 0.78],
].map(([h, s]) => [0.86, 0.72, 0.56, 0.42, 0.28].map((l) => hsl(h, s, l)));

/**
 * The chosen color as `"#RRGGBB"`, or `null` for none, as gpui-component's
 * `ColorPickerState` (`ColorPickerState.new({ value })` sets the default).
 * Choices report `on("change", (hex, cx))`; `set_value` does not.
 */
export class ColorPickerState {
  #released = false;

  static new(options = {}) {
    return new ColorPickerState(options);
  }

  constructor(options = {}) {
    refuseStateInRender("ColorPickerState.new(...)");
    this._value = null;
    if (options.value !== undefined && options.value !== null) {
      this._value = normalizeHex(options.value);
      if (this._value === null) throw new TypeError(`ColorPickerState.new({value}) expects a "#RRGGBB" color, got ${JSON.stringify(options.value)}`);
    }
    this._listeners = [];
    this._owner = scope.owner;
    this._hexError = false;
    this._hex = InputState.new({
      placeholder: "007AFF",
      value: this._value?.slice(1) ?? "",
      max_length: 6,
      allowed_chars: "0123456789abcdefABCDEF",
      auto_capitalize: "characters",
      return_key: "done",
    });
    this._hex.on("change", (_e, cx) => this._typed(cx, false));
    this._hex.on("submit", (_e, cx) => this._typed(cx, true));
  }

  #live(what) {
    if (this.#released) throw released(what, "color picker state");
  }

  value() {
    this.#live("state.value()");
    return this._value;
  }

  set_value(v) {
    this.#live("state.set_value(color)");
    const next = v === null ? null : normalizeHex(v);
    if (v !== null && next === null) throw new TypeError(`state.set_value(color) expects a "#RRGGBB" color or null, got ${JSON.stringify(v)}`);
    this._value = next;
    this._hexError = false;
    this._hex.set_value(next?.slice(1) ?? "");
    markDirty(this._owner);
  }

  on(event, fn) {
    this.#live("state.on(event, fn)");
    if (event !== "change") throw new TypeError(`ColorPickerState.on(): unknown event ${JSON.stringify(event)}; use "change"`);
    if (typeof fn !== "function") throw new TypeError("state.on(event, fn) expects a function");
    this._listeners.push(fn);
    return true;
  }

  release() {
    const was = !this.#released;
    this.#released = true;
    this._listeners = [];
    this._hex.release();
    return was;
  }

  #apply(hex, cx) {
    this._hexError = false;
    markDirty(this._owner);
    if (hex === this._value) return;
    this._value = hex;
    for (const fn of [...this._listeners]) fn(hex, cx);
  }

  _pick(hex, cx) {
    if (this.#released) return;
    if (hex !== this._value) cx.haptic("selection");
    this._hex.set_value(hex.slice(1));
    this.#apply(hex, cx);
  }

  /** A typed hex applies once it is complete; Done on an incomplete one says what is wrong. */
  _typed(cx, committed) {
    if (this.#released) return;
    const typed = this._hex.value();
    const hex = normalizeHex(typed);
    if (hex !== null) return this.#apply(hex, cx);
    this._hexError = committed && typed !== "";
    markDirty(this._owner);
  }
}

const SWATCH = { xsmall: 20, small: 24, medium: 28, large: 36 };

/**
 * A swatch of the chosen color. Tapping it opens a sheet of colors: the
 * featured ones, a palette and a hex field. A choice applies at once; the
 * sheet closes with Done or a swipe.
 */
class ColorPickerElement extends Composite {
  constructor(state) {
    super();
    if (!(state instanceof ColorPickerState)) throw new TypeError("ColorPicker expects one ColorPickerState entity");
    this._state = state;
    state._owner = scope.owner ?? state._owner;
    this._id = `color-picker-${state._hex.id}`;
    this._k = this._id;
    this._label = null;
    this._name = null;
    this._featured = FEATURED;
    this._size = "medium";
    this._disabled = false;
    this._iconPath = null;
    this._open = popoverState(this._id, false);
    if (scope.popovers !== null) {
      deferPopover(() =>
        this._open.get() && !this._disabled ? { id: this._id, content: this._sheet(), onDismiss: (cx) => this._close(cx) } : null,
      );
    }
  }

  label(s) {
    if (typeof s !== "string") throw new TypeError("ColorPicker.label expects a string");
    this._label = s;
    return this;
  }

  accessibility_label(s) {
    if (typeof s !== "string") throw new TypeError("ColorPicker.accessibility_label expects a string");
    this._name = s;
    return this;
  }

  /** The colors offered first, as `"#RRGGBB"` strings. */
  featured_colors(colors) {
    if (!Array.isArray(colors)) throw new TypeError("ColorPicker.featured_colors(colors) expects an array of colors");
    this._featured = colors.map((c) => {
      const hex = normalizeHex(c);
      if (hex === null) throw new TypeError(`ColorPicker.featured_colors(colors): ${JSON.stringify(c)} is not a "#RRGGBB" color`);
      return hex;
    });
    return this;
  }

  /** An icon from the program's assets, drawn in the chosen color, in place of the swatch. */
  icon(path) {
    this._iconPath = String(path);
    return this;
  }

  disabled(value = true) {
    this._disabled = bool(value, "ColorPicker.disabled(value) expects one boolean");
    return this;
  }

  size(...args) {
    return componentSize(this, "ColorPicker", args) ? this : super.size(...args);
  }

  /** Accepted so desktop code runs: the picker is a sheet on a phone. */
  anchor(_value) {
    return this;
  }

  _close(cx) {
    this._open.set(false);
    cx.notify();
  }

  _swatch(hex, selected, size) {
    return withId(div(), partId(this, `swatch-${hex.slice(1)}`))
      .key(hex)
      .role("button")
      .accessibility_label(hex)
      .selected(selected)
      .w(size)
      .h(size)
      .flex_none()
      .rounded_full()
      .p(3)
      .border_2()
      .border_color(selected ? "foreground" : "transparent")
      .active((s) => s.opacity(0.6))
      .on_click((_e, cx) => this._state._pick(hex, cx))
      .child(div().size_full().rounded_full().bg(hex).border_1().border_color("border"));
  }

  _sheet() {
    const state = this._state;
    const value = state._value;
    const heading = (s) => div().role("header").text_size(TEXT.footnote).text_color("muted_foreground").child(s);
    const done = withId(div(), partId(this, "done"))
      .role("button")
      .px_2()
      .py_2()
      .hit_slop(6)
      .text_size(TEXT.body)
      .font_semibold()
      .text_color("primary")
      .active((s) => s.opacity(0.4))
      .on_click((_e, cx) => this._close(cx))
      .child("Done");
    const hex = withId(Input.new(state._hex), partId(this, "hex"))
      .flex_1()
      .min_w_0()
      .text_size(TEXT.body)
      .text_color("foreground")
      .accessibility_label("Hex color");
    return withId(v_flex(), partId(this, "sheet"))
      .px_4()
      .pt_2()
      .pb_6()
      .gap_5()
      .overflow_y_scroll()
      .child(
        h_flex()
          .min_h_11()
          .gap_3()
          .child(div().w_7().h_7().flex_none().rounded_full().border_1().border_color("border").bg(value ?? "muted"))
          .child(div().role("header").flex_1().text_size(TEXT.body).font_semibold().text_color("foreground").child(this._label ?? this._name ?? "Color"))
          .child(done),
      )
      .child(
        v_flex()
          .gap_2()
          .child(heading("Suggested"))
          .child(h_flex().flex_wrap().gap_1().children(this._featured.map((c) => this._swatch(c, c === value, 44)))),
      )
      .child(
        v_flex()
          .gap_2()
          .child(heading("Palette"))
          .child(
            v_flex()
              .gap_1()
              .children(
                [0, 1, 2, 3, 4].map((shade) =>
                  h_flex()
                    .justify_between()
                    .children(PALETTE.map((family) => this._swatch(family[shade], family[shade] === value, 36))),
                ),
              ),
          ),
      )
      .child(
        v_flex()
          .gap_2()
          .child(heading("Hex"))
          .child(
            h_flex()
              .min_h_11()
              .px_3()
              .gap_1()
              .rounded(10)
              .border_1()
              .border_color(state._hexError ? "destructive" : "input")
              .bg("surface")
              .child(div().text_size(TEXT.body).text_color("muted_foreground").child("#"))
              .child(hex),
          )
          .when(state._hexError, (el) =>
            el.child(withId(caption("Enter six hex digits, like 007AFF.", "footnote", "destructive").role("alert"), partId(this, "hex-error"))),
          ),
      );
  }

  _build() {
    const value = this._state._value;
    const s = SWATCH[this._size];
    const swatch = withId(div(), partId(this, "swatch"))
      .role("button")
      .accessibility_label(this._name ?? this._label ?? "Color")
      .accessibility_value(value ?? "None")
      .flex_none()
      .w(s)
      .h(s)
      .rounded_full()
      .p(2)
      .border_1()
      .border_color("border")
      .hit_slop(Math.max(0, (44 - s) / 2))
      .when(this._disabled, (el) => el.opacity(0.4).disabled(true))
      .when(!this._disabled, (el) =>
        el.active((st) => st.opacity(0.6)).on_click((_e, cx) => {
          this._open.set(true);
          cx.notify();
        }),
      )
      .child(
        this._iconPath !== null
          ? svg(this._iconPath).text_color(value ?? "foreground").size_full()
          : div().size_full().rounded_full().bg(value ?? "muted"),
      );
    if (this._label === null) {
      this._preset([["flex"], ["flex_none"]]);
      return [swatch];
    }
    this._preset([["flex"], ["flex_col"], ["gap_1p5"], ["items_start"]]);
    return [caption(this._label, "footnote", "muted_foreground"), swatch];
  }
}

export const ColorPicker = component((state) => new ColorPickerElement(state));
