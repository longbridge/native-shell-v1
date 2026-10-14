// "gpui-component" feedback: alerts, badges, tags, skeletons, empty states,
// labels, keycaps, separators, shimmer text and notifications.
//
// Names, constructors, methods and error texts follow gpui-shell; the look is
// iOS. Each component is composed from the primitives, so every renderer
// draws it without knowing it exists. Colors are theme tokens; a tinted fill
// (a token at low alpha) or a legible ink (a token pulled towards the label
// color) is derived from the token in force when the view renders, and a
// theme or appearance change renders every view again.
//
// Constructors work both ways gpui-shell's do: `Tag.new()` and `new Tag()`.

import { scope, theme, windowInfo, startTimer, cancelTimer, windowObject } from "nsh:core";
import { Element, div, h_flex, v_flex, span, shape } from "nsh:elements";

function component(make) {
  function C(...args) {
    return make(...args);
  }
  C.new = make;
  return Object.freeze(C);
}

const SIZES = ["xsmall", "small", "medium", "large"];

/**
 * `px` points at the default text size, as a rem length: it grows with the
 * user's text size, so glyphs, capsules and padding keep pace with the text.
 */
function rem(px) {
  return `${px / 16}rem`;
}

/** The theme's iOS corner radius `name` (sm 6, md 10, lg 12, xl 16) as a preset op. */
function radius(name) {
  return ["rounded", theme().radius[name]];
}

function checkSize(what, size) {
  if (!SIZES.includes(size)) throw new TypeError(`${what}.size expects one of ${SIZES.join(", ")}, got ${JSON.stringify(size)}`);
  return size;
}

/**
 * `size(...)`: a semantic size names one of `SIZES`; anything else is the
 * style method (`.size(40)`, `.size("2rem")`), as on gpui-component's Button.
 */
function isLength(value) {
  return typeof value === "number" || (typeof value === "string" && /^(-?[\d.]+(px|rem|%)|auto)$/.test(value));
}

// Colors ------------------------------------------------------------------------

const HEX = /^#([0-9a-f]{3,4}|[0-9a-f]{6}|[0-9a-f]{8})$/i;

function checkColor(what, color) {
  if (typeof color !== "string") throw new TypeError(`${what} expects a color string`);
  if (theme()[color] === undefined && !HEX.test(color)) {
    throw new TypeError(`invalid ${what.split(".")[0]} color: \`${color}\` is not a theme token or a #rgb, #rrggbb or #rrggbbaa literal`);
  }
  return color;
}

function rgba(hex) {
  const h = String(hex).replace(/^#/, "");
  const n = (i) => parseInt(h.slice(i, i + 2), 16);
  return [n(0), n(2), n(4), h.length >= 8 ? n(6) : 255];
}

function hex([r, g, b, a]) {
  const two = (v) => Math.round(Math.min(255, Math.max(0, v))).toString(16).padStart(2, "0");
  return "#" + two(r) + two(g) + two(b) + (a >= 255 ? "" : two(a));
}

/** A token as a translucent fill, as iOS tints a row or a capsule. */
function tint(token, alpha) {
  const [r, g, b] = rgba(theme()[token]);
  return hex([r, g, b, alpha * 255]);
}

/**
 * A token as text on its own tint: pulled towards the label color, so green
 * and orange stay legible on white and on black alike.
 */
function ink(token) {
  const c = rgba(theme()[token]);
  const f = rgba(theme().foreground);
  return hex(c.map((v, i) => (i === 3 ? 255 : v + (f[i] - v) * 0.3)));
}

// The token a fill takes its text color from (`primary` → `primary_foreground`).
function foregroundOf(color) {
  return `${color}_foreground` in theme() ? `${color}_foreground` : "white";
}

// Outline glyphs on a 24-unit grid, after the SF Symbols iOS uses for each.
const GLYPHS = {
  info: "M12 22a10 10 0 1 0 0-20 10 10 0 0 0 0 20ZM12 16v-4M12 8h.01",
  success: "M12 22a10 10 0 1 0 0-20 10 10 0 0 0 0 20ZM8 12l3 3 5-6",
  warning: "M10.3 3.9 1.8 18a2 2 0 0 0 1.7 3h17a2 2 0 0 0 1.7-3L13.7 3.9a2 2 0 0 0-3.4 0ZM12 9v4M12 17h.01",
  error: "M12 22a10 10 0 1 0 0-20 10 10 0 0 0 0 20ZM15 9l-6 6M9 9l6 6",
  close: "M18 6 6 18M6 6l12 12",
};

function glyph(name, color, size) {
  return shape(GLYPHS[name], { view_box: [0, 0, 24, 24] }).stroke(color, 2).w(rem(size)).h(rem(size)).flex_none();
}

/** A tappable xmark with a 44 pt target, for `on_close`. */
function closeButton(what, fn, color, size) {
  return new Element("view")
    .role("button")
    .accessibility_label("Dismiss")
    .hit_slop(Math.max(0, (44 - size) / 2))
    .flex_none()
    .active((s) => s.opacity(0.5))
    .child(glyph("close", color, size).accessibility_hidden())
    ._on("click", fn, `${what}.on_close(handler)`);
}

// Alert -------------------------------------------------------------------------

const ALERT_VARIANTS = {
  Alert: { token: null, icon: "info" },
  InfoAlert: { token: "info", icon: "info" },
  SuccessAlert: { token: "success", icon: "success" },
  WarningAlert: { token: "warning", icon: "warning" },
  ErrorAlert: { token: "destructive", icon: "error" },
};

// [horizontal padding, vertical padding, gap, text, icon] per size.
const ALERT_METRICS = {
  xsmall: [12, 6, 6, "text_xs", 16],
  small: [12, 8, 8, "text_sm", 18],
  medium: [16, 12, 12, "text_sm", 20],
  large: [20, 16, 12, "text_base", 22],
};

/**
 * A message banner: an inline card (12 pt corners, the variant's tint) or,
 * with `banner()`, a full-width strip without a title. Renders nothing while
 * `visible(false)`. Announced as an alert, title and message together.
 */
class AlertElement extends Element {
  constructor(name, id, message) {
    super("view");
    if (typeof id !== "string" || typeof message !== "string") throw new TypeError(`${name}(id, message) expects two strings`);
    if (id.length === 0) throw new TypeError("Alert id must not be empty");
    this._k = id;
    this._variant = ALERT_VARIANTS[name];
    this._message = message;
    this._title = null;
    this._banner = false;
    this._visible = true;
    this._size = "medium";
    this._icon = null;
    this._close = null;
    this._P().role = "alert";
    this._sync();
  }

  title(title) {
    if (typeof title !== "string") throw new TypeError("Alert.title(title) expects a string");
    this._title = title;
    return this._sync();
  }

  banner() {
    this._banner = true;
    return this._sync();
  }

  visible(visible) {
    // Bare, it is GPUI's `visible()` style (visibility: visible).
    if (arguments.length === 0) return super.visible();
    if (typeof visible !== "boolean") throw new TypeError("Alert.visible(visible) expects a boolean");
    this._visible = visible;
    return this._sync();
  }

  size(size) {
    if (isLength(size)) return super.size(size);
    this._size = checkSize("Alert", size);
    return this._sync();
  }

  /** gpui-component's `Alert::icon`: an element in place of the variant's glyph. */
  icon(el) {
    if (!(el instanceof Element)) throw new TypeError("Alert.icon(icon) expects an element");
    this._icon = el;
    return this._sync();
  }

  /** gpui-component's `Alert::on_close`: a dismiss button that calls `(event, cx)`. */
  on_close(fn) {
    if (typeof fn !== "function") throw new TypeError("Alert.on_close(handler) expects a function");
    this._close = fn;
    return this._sync();
  }

  _sync() {
    const { token, icon } = this._variant;
    const [px, py, gap, textSize, iconSize] = ALERT_METRICS[this._size];
    const ops = [["flex"], ["flex_row"], ["w_full"], ["px", rem(px)], ["py", rem(py)], ["gap", rem(gap)], [textSize], ["text_color", "foreground"]];
    if (token) ops.push(["bg", tint(token, 0.14)]);
    else ops.push(["bg", "surface"], ["border_1"], ["border_color", "border"]);
    ops.push(this._banner ? ["items_center"] : ["items_start"]);
    if (!this._banner) ops.push(radius("lg"));
    if (!this._visible) ops.push(["hidden"]);
    this._preset(ops);

    const title = this._title !== null && !this._banner ? this._title : null;
    this._P().a11y_label = title ? `${title}. ${this._message}` : this._message;
    this._P().a11y_hidden = !this._visible;
    if (!this._visible) {
      this._before = null;
      return this;
    }
    const lead = (this._icon ?? glyph(icon, token ?? "muted_foreground", iconSize)).accessibility_hidden();
    const body = v_flex()
      .flex_1()
      .min_w_0()
      .gap_0p5()
      .when(title !== null, (el) => el.child(div().font_semibold().child(title)))
      .child(div().child(this._message));
    this._before = [div().flex_none().child(lead), body];
    if (this._close) this._before.push(closeButton("Alert", this._close, "muted_foreground", iconSize - 4));
    return this;
  }
}

export const Alert = component((id, message) => new AlertElement("Alert", id, message));
export const InfoAlert = component((id, message) => new AlertElement("InfoAlert", id, message));
export const SuccessAlert = component((id, message) => new AlertElement("SuccessAlert", id, message));
export const WarningAlert = component((id, message) => new AlertElement("WarningAlert", id, message));
export const ErrorAlert = component((id, message) => new AlertElement("ErrorAlert", id, message));

// Badge -------------------------------------------------------------------------

// [capsule height, text size, dot size] per size: iOS's 18 pt count badge at medium.
const BADGE_METRICS = {
  xsmall: [14, 9, 6],
  small: [16, 10, 8],
  medium: [18, 12, 10],
  large: [22, 14, 12],
};

function natural(what, value) {
  if (typeof value !== "number" || !Number.isInteger(value) || value < 0 || value > Number.MAX_SAFE_INTEGER) {
    throw new TypeError(`Badge.${what} expects a non-negative integer`);
  }
  return value;
}

/**
 * A count or dot over its children's top trailing corner: the red capsule
 * iOS puts on app icons and tab items. A count of zero shows nothing; a count
 * over `max` shows `max+`. The count is the element's accessibility value.
 */
class BadgeElement extends Element {
  constructor() {
    super("view");
    this._dot = false;
    this._count = 0;
    this._max = 99;
    this._color = "destructive";
    this._size = "medium";
    this._preset([["relative"], ["flex_none"]]);
  }

  dot() {
    this._dot = true;
    return this._sync();
  }

  count(count) {
    this._count = natural("count", count);
    return this._sync();
  }

  max(max) {
    this._max = natural("max", max);
    return this._sync();
  }

  color(color) {
    this._color = checkColor("Badge.color", color);
    return this._sync();
  }

  size(size) {
    if (isLength(size)) return super.size(size);
    this._size = checkSize("Badge", size);
    return this._sync();
  }

  _sync() {
    const [height, textSize, dotSize] = BADGE_METRICS[this._size];
    if (this._p) delete this._p.a11y_value;
    if (this._dot) {
      const at = rem(-dotSize / 4);
      this._after = [div().absolute().top(at).right(at).size(rem(dotSize)).rounded_full().bg(this._color)];
    } else if (this._count > 0) {
      const label = this._count > this._max ? `${this._max}+` : String(this._count);
      this._P().a11y_value = label;
      this._after = [
        h_flex()
          .absolute()
          .top(rem(-height / 3))
          .right(rem(-height / 3))
          .h(rem(height))
          .min_w(rem(height))
          .px(rem(height / 4))
          .justify_center()
          .rounded_full()
          .bg(this._color)
          .text_color(foregroundOf(this._color))
          .text_size(textSize)
          .font_semibold()
          .accessibility_hidden()
          .child(label),
      ];
    } else {
      this._after = null;
    }
    return this;
  }
}

export const Badge = component(() => new BadgeElement());

// Tag ---------------------------------------------------------------------------

const TAG_VARIANTS = ["primary", "secondary", "danger", "success", "warning", "info"];
const TAG_TOKENS = { primary: "primary", danger: "destructive", success: "success", warning: "warning", info: "info" };

// [horizontal padding, vertical padding, text, close glyph] per size.
const TAG_METRICS = {
  xsmall: [4, 0, "text_xs", 10],
  small: [6, 2, "text_xs", 10],
  medium: [8, 3, "text_sm", 12],
  large: [10, 5, "text_base", 14],
};

/**
 * A compact status capsule. `primary` and `secondary` are filled; the status
 * variants are tinted with legible ink; `outline()` keeps only a hairline.
 */
class TagElement extends Element {
  constructor() {
    super("view");
    this._variant = "secondary";
    this._outline = false;
    this._full = false;
    this._size = "medium";
    this._close = null;
    this._sync();
  }

  variant(variant) {
    if (!TAG_VARIANTS.includes(variant)) throw new TypeError(`unsupported Tag variant \`${variant}\``);
    this._variant = variant;
    return this._sync();
  }

  outline() {
    this._outline = true;
    return this._sync();
  }

  rounded_full() {
    this._full = true;
    return this._sync();
  }

  size(size) {
    if (isLength(size)) return super.size(size);
    this._size = checkSize("Tag", size);
    return this._sync();
  }

  /** Mobile: a trailing dismiss button, for a removable tag (a filter, a recipient). */
  on_close(fn) {
    if (typeof fn !== "function") throw new TypeError("Tag.on_close(handler) expects a function");
    this._close = fn;
    return this._sync();
  }

  _sync() {
    const [px, py, textSize, closeSize] = TAG_METRICS[this._size];
    const token = TAG_TOKENS[this._variant];
    let fill;
    let fg;
    if (this._variant === "secondary") {
      fill = "secondary";
      fg = this._outline ? "foreground" : "secondary_foreground";
    } else if (this._variant === "primary" && !this._outline) {
      fill = "primary";
      fg = "primary_foreground";
    } else {
      fill = tint(token, 0.15);
      fg = ink(token);
    }
    const ops = [["flex"], ["flex_row"], ["items_center"], ["flex_none"], ["gap_1"], ["px", rem(px)], ["py", rem(py)], [textSize], ["font_medium"]];
    ops.push(this._full ? ["rounded_full"] : radius("sm"), ["text_color", fg]);
    if (this._outline) ops.push(["border_1"], ["border_color", this._variant === "secondary" ? "border" : fg]);
    else ops.push(["bg", fill]);
    this._preset(ops);
    this._after = this._close ? [closeButton("Tag", this._close, fg, closeSize)] : null;
    return this;
  }
}

export const Tag = component(() => new TagElement());

// Skeleton ----------------------------------------------------------------------

/**
 * A placeholder block where content will appear, sized by the script
 * (`.h_4().w_32()`, `.size_10().rounded_full()`). It is still, as iOS's
 * redacted placeholders are: nothing moves, so there is nothing for Reduce
 * Motion to stop and no list of shimmering bars while a load takes seconds.
 * Hidden from assistive technology; the script labels the loading region.
 */
class SkeletonElement extends Element {
  constructor() {
    super("view");
    this._P().a11y_hidden = true;
    this._preset([["w_full"], ["h_4"], ["rounded_md"], ["bg", "secondary"]]);
  }

  secondary() {
    return this._preset([["w_full"], ["h_4"], ["rounded_md"], ["bg", "secondary"], ["opacity", 0.5]]);
  }
}

export const Skeleton = component(() => new SkeletonElement());

// Empty -------------------------------------------------------------------------

class EmptyPart extends Element {
  constructor(name, ops, role) {
    super("view");
    this._part = name;
    this._preset(ops);
    if (role) this._P().role = role;
  }
}

function expectPart(method, el, name) {
  if (!(el instanceof EmptyPart) || el._part !== name) throw new TypeError(`${method} expects one registered Empty part`);
  return el;
}

/**
 * A stateless empty state, as iOS's content-unavailable view: centered and
 * borderless, a large symbol over a title and an explanation, then actions.
 * `header` and `content` come first; ordinary children follow.
 */
class EmptyElement extends EmptyPart {
  constructor() {
    super("Empty", [["flex"], ["flex_col"], ["items_center"], ["justify_center"], ["gap_6"], ["px_6"], ["py_8"], ["text_center"]]);
    this._header = null;
    this._content = null;
  }

  header(el) {
    this._header = expectPart("header", el, "EmptyHeader");
    return this._sync();
  }

  content(el) {
    this._content = expectPart("content", el, "EmptyContent");
    return this._sync();
  }

  _sync() {
    const slots = [this._header, this._content].filter(Boolean);
    this._before = slots.length ? slots : null;
    return this;
  }
}

const NO_HEADER_CHILDREN = "EmptyHeader does not accept children; use media, title, and description";

class EmptyHeaderElement extends EmptyPart {
  constructor() {
    super("EmptyHeader", [["flex"], ["flex_col"], ["items_center"], ["gap_2"], ["max_w", 360]]);
    this._slots = { media: null, title: null, description: null };
  }

  media(el) {
    return this._slot("media", el, "EmptyMedia");
  }

  title(el) {
    return this._slot("title", el, "EmptyTitle");
  }

  description(el) {
    return this._slot("description", el, "EmptyDescription");
  }

  child(_x) {
    throw new TypeError(NO_HEADER_CHILDREN);
  }

  children(_xs) {
    throw new TypeError(NO_HEADER_CHILDREN);
  }

  _slot(key, el, name) {
    this._slots[key] = expectPart(key, el, name);
    const { media, title, description } = this._slots;
    this._before = [media, title, description].filter(Boolean);
    return this;
  }
}

const MEDIA = [["flex"], ["items_center"], ["justify_center"], ["flex_none"], ["mb_2"], ["text_color", "muted_foreground"]];
const MEDIA_VARIANTS = {
  default: () => MEDIA,
  icon: () => [...MEDIA, ["size_16"], radius("xl"), ["bg", "secondary"]],
};

class EmptyMediaElement extends EmptyPart {
  constructor() {
    super("EmptyMedia", MEDIA);
    this._P().a11y_hidden = true;
  }

  variant(variant) {
    if (!(variant in MEDIA_VARIANTS)) throw new TypeError("EmptyMedia.variant expects default or icon");
    return this._preset(MEDIA_VARIANTS[variant]());
  }
}

export const Empty = component(() => new EmptyElement());
export const EmptyHeader = component(() => new EmptyHeaderElement());
export const EmptyMedia = component(() => new EmptyMediaElement());
export const EmptyTitle = component(() => new EmptyPart("EmptyTitle", [["text_xl"], ["font_bold"], ["text_color", "foreground"]], "header"));
export const EmptyDescription = component(
  () => new EmptyPart("EmptyDescription", [["text_base"], ["line_height", 1.35], ["text_color", "muted_foreground"]]),
);
export const EmptyContent = component(
  () => new EmptyPart("EmptyContent", [["flex"], ["flex_col"], ["items_center"], ["gap_3"], ["w_full"], ["max_w", 360]]),
);

// Label -------------------------------------------------------------------------

const MASKED = "•";
const NO_LABEL_CHILDREN = "Label does not accept children; its text is `Label.new(text)`";

/** `s` as runs, the case-insensitive matches of `needle` marked. */
function matches(s, needle) {
  if (!needle) return [[s, false]];
  const lower = s.toLowerCase();
  const n = needle.toLowerCase();
  const runs = [];
  let at = 0;
  for (let i = lower.indexOf(n); i !== -1; i = lower.indexOf(n, i + n.length)) {
    if (i > at) runs.push([s.slice(at, i), false]);
    runs.push([s.slice(i, i + n.length), true]);
    at = i + n.length;
  }
  if (at < s.length) runs.push([s.slice(at), false]);
  return runs;
}

/**
 * A line of text with an optional muted secondary part, masking (one `•` per
 * character, for a balance or a code) and matches highlighted in the tint
 * (search results).
 */
class LabelElement extends Element {
  constructor(label) {
    super("text");
    if (typeof label !== "string") throw new TypeError("Label expects one string text");
    if (label.length === 0) throw new TypeError("Label text must not be empty");
    this._label = label;
    this._secondary = null;
    this._masked = false;
    this._highlights = null;
    this._sync();
  }

  secondary(secondary) {
    if (typeof secondary !== "string") throw new TypeError("Label.secondary expects one string");
    this._secondary = secondary;
    return this._sync();
  }

  masked(masked) {
    if (typeof masked !== "boolean") throw new TypeError("Label.masked expects one boolean");
    this._masked = masked;
    return this._sync();
  }

  highlights(highlights) {
    if (typeof highlights !== "string") throw new TypeError("Label.highlights expects one string");
    this._highlights = highlights;
    return this._sync();
  }

  child(_x) {
    throw new TypeError(NO_LABEL_CHILDREN);
  }

  children(_xs) {
    throw new TypeError(NO_LABEL_CHILDREN);
  }

  _sync() {
    const full = this._secondary !== null ? `${this._label} ${this._secondary}` : this._label;
    if (this._masked) {
      this._c = [MASKED.repeat([...full].length)];
      this._P().a11y_label = "Hidden";
      return this;
    }
    if (this._p) delete this._p.a11y_label;
    const runs = [];
    const push = (s, muted) => {
      for (const [run, hit] of matches(s, this._highlights)) {
        if (hit) runs.push(span(run).text_color("primary"));
        else runs.push(muted ? span(run).text_color("muted_foreground") : run);
      }
    };
    push(this._label, false);
    if (this._secondary !== null) push(` ${this._secondary}`, true);
    this._c = runs;
    return this;
  }
}

export const Label = component((label) => new LabelElement(label));

// Kbd ---------------------------------------------------------------------------

const MODIFIERS = ["ctrl", "alt", "shift", "cmd"];
const MODIFIER_ALIASES = { control: "ctrl", option: "alt", super: "cmd", win: "cmd", platform: "cmd" };
const APPLE_KEYS = {
  ctrl: "⌃", alt: "⌥", shift: "⇧", cmd: "⌘", backspace: "⌫", delete: "⌦", escape: "⎋", enter: "↩", tab: "⇥",
  left: "←", right: "→", up: "↑", down: "↓",
};
const OTHER_KEYS = { ctrl: "Ctrl", alt: "Alt", shift: "Shift", cmd: "Win", escape: "Esc" };
const SPOKEN = {
  ctrl: "Control", alt: "Option", shift: "Shift", cmd: "Command", backspace: "Delete", delete: "Forward Delete",
  escape: "Escape", enter: "Return", left: "Left Arrow", right: "Right Arrow", up: "Up Arrow", down: "Down Arrow",
};
const NAMED_KEYS = { pageup: "Page Up", pagedown: "Page Down" };

function isApple() {
  const p = windowInfo().platform;
  return p === "ios" || p === "macos";
}

/** gpui's keystroke spelling (`cmd-shift-k`, `secondary-s`, `cmd--`) as `[...modifiers, key]`. */
function parseKeystroke(source) {
  if (typeof source !== "string") throw new TypeError("Kbd expects one keystroke string");
  const s = source.trim();
  const invalid = (why) => new TypeError(`invalid Kbd keystroke: ${why}`);
  if (s.length === 0 || /\s/.test(s)) throw invalid(`\`${source}\` is not one keystroke like "cmd-k"`);
  const parts = s.endsWith("--") ? [...s.slice(0, -2).split("-").filter(Boolean), "-"] : s.split("-");
  const key = parts.pop();
  if (!key) throw invalid(`\`${source}\` has no key`);
  const modifiers = new Set();
  for (const raw of parts) {
    const m = raw.toLowerCase();
    // gpui's `secondary`: the platform's command key.
    const name = m === "secondary" ? (isApple() ? "cmd" : "ctrl") : (MODIFIER_ALIASES[m] ?? m);
    if (!MODIFIERS.includes(name)) throw invalid(`unknown modifier \`${raw}\` in \`${source}\``);
    modifiers.add(name);
  }
  return [...MODIFIERS.filter((m) => modifiers.has(m)), key.toLowerCase()];
}

function keyName(k, table) {
  return table[k] ?? NAMED_KEYS[k] ?? (k.length === 1 ? k.toUpperCase() : k[0].toUpperCase() + k.slice(1));
}

/**
 * A hardware-keyboard shortcut as a keycap, in the platform's spelling: `⌘K`
 * on iOS and macOS, `Ctrl+K` elsewhere. Read aloud in words.
 */
class KbdElement extends Element {
  constructor(keystroke) {
    super("view");
    const keys = parseKeystroke(keystroke);
    const apple = isApple();
    this._c = [keys.map((k) => keyName(k, apple ? APPLE_KEYS : OTHER_KEYS)).join(apple ? "" : "+")];
    this._P().a11y_label = keys.map((k) => keyName(k, SPOKEN)).join(" ");
    this._appearance = true;
    this._outline = false;
    this._sync();
  }

  appearance(appearance) {
    if (typeof appearance !== "boolean") throw new TypeError("Kbd.appearance expects one boolean");
    this._appearance = appearance;
    return this._sync();
  }

  outline() {
    this._outline = true;
    return this._sync();
  }

  _sync() {
    const ops = [["flex"], ["flex_row"], ["items_center"], ["justify_center"], ["flex_none"], ["text_xs"], ["font_medium"]];
    ops.push(["text_color", "muted_foreground"]);
    if (this._appearance) {
      ops.push(["min_w_5"], ["h_5"], ["px_1p5"], radius("sm"), ["border_1"], ["border_color", "border"]);
      if (!this._outline) ops.push(["bg", "muted"], ["border_b_2"]);
    }
    return this._preset(ops);
  }
}

export const Kbd = component((keystroke) => new KbdElement(keystroke));

// Separator ---------------------------------------------------------------------

// 4-unit dashes with 2-unit gaps, longer than any screen; the line clips them.
const DASH_RUN = 2400;
const H_DASHES = "M0 0.5" + "h4m2 0".repeat(DASH_RUN / 6);
const V_DASHES = "M0.5 0" + "v4m0 2".repeat(DASH_RUN / 6);

/**
 * A hairline (one device pixel, as iOS separators are) in the border color,
 * horizontal or vertical, solid or dashed; a label sits centered between two
 * lines. Decorative, and hidden from assistive technology, unless labeled.
 */
class SeparatorElement extends Element {
  constructor(vertical, dashed) {
    super("view");
    this._vertical = vertical;
    this._dashed = dashed;
    this._label = null;
    this._color = "border";
    this._sync();
  }

  label(label) {
    if (typeof label !== "string") throw new TypeError("Separator.label(label) expects a string");
    this._label = label;
    return this._sync();
  }

  color(color) {
    this._color = checkColor("Separator.color(color)", color);
    return this._sync();
  }

  dashed() {
    this._dashed = true;
    return this._sync();
  }

  _line() {
    const hairline = 1 / Math.max(1, windowInfo().scale || 1);
    const v = this._vertical;
    const line = div().flex_1().overflow_hidden();
    if (v) line.w(hairline).min_h(hairline);
    else line.h(hairline).min_w(hairline);
    if (!this._dashed) return line.bg(this._color);
    const dashes = v
      ? shape(V_DASHES, { view_box: [0, 0, 1, DASH_RUN] }).w(1).h(DASH_RUN)
      : shape(H_DASHES, { view_box: [0, 0, DASH_RUN, 1] }).w(DASH_RUN).h(1);
    return line.child(dashes.flex_none().stroke(this._color, 1));
  }

  _sync() {
    const v = this._vertical;
    // Stretched across the parent rather than `w_full`, so an inset (`.ml_4()`) shortens the line.
    const ops = [["flex"], v ? ["flex_col"] : ["flex_row"], ["items_center"], ["self_stretch"]];
    ops.push(["flex_none"]);
    if (this._label === null) {
      this._P().a11y_hidden = true;
      this._preset(ops);
      this._before = [this._line()];
      return this;
    }
    if (this._p) delete this._p.a11y_hidden;
    this._preset([...ops, v ? ["gap_2"] : ["gap_3"]]);
    const caption = div().flex_none().text_xs().text_color("muted_foreground").child(this._label);
    this._before = [this._line(), caption, this._line()];
    return this;
  }
}

export const Separator = component(() => new SeparatorElement(false, false));
export const VerticalSeparator = component(() => new SeparatorElement(true, false));
export const DashedSeparator = component(() => new SeparatorElement(false, true));
export const VerticalDashedSeparator = component(() => new SeparatorElement(true, true));

// ShimmerText -------------------------------------------------------------------

/**
 * Text that stands for work in progress ("Thinking…"), in the secondary label
 * color and announced as busy. The sweep's settings are checked and kept, but
 * the text holds still: a renderer only tweens between frames the script
 * draws, and a loop that redrew the view for as long as the work runs would
 * also have to stop under Reduce Motion, which programs cannot see.
 */
class ShimmerTextElement extends Element {
  constructor(content) {
    super("text");
    if (typeof content !== "string") throw new TypeError("ShimmerText expects text");
    this._c = [content];
    this._sweep = { duration_ms: 2000, spread: 0.2, reverse: false, once: false };
    this._P().a11y_state = JSON.stringify({ busy: true });
    this._preset([["text_color", "muted_foreground"]]);
  }

  id(id) {
    if (typeof id !== "string" || id.length === 0) throw new TypeError("ShimmerText.id expects a non-empty stable id");
    this._k = id;
    return this;
  }

  duration_ms(duration_ms) {
    if (typeof duration_ms !== "number" || !Number.isFinite(duration_ms) || duration_ms < 0) {
      throw new TypeError("ShimmerText.duration_ms expects a finite non-negative duration");
    }
    this._sweep.duration_ms = duration_ms;
    return this;
  }

  spread(spread) {
    if (typeof spread !== "number" || !Number.isFinite(spread)) throw new TypeError("ShimmerText.spread expects a finite f32 fraction");
    this._sweep.spread = spread;
    return this;
  }

  reverse(reverse) {
    if (typeof reverse !== "boolean") throw new TypeError("ShimmerText.reverse expects a boolean");
    this._sweep.reverse = reverse;
    return this;
  }

  once(once) {
    if (typeof once !== "boolean") throw new TypeError("ShimmerText.once expects a boolean");
    this._sweep.once = once;
    return this;
  }
}

export const ShimmerText = component((content) => new ShimmerTextElement(content));

// Notification ------------------------------------------------------------------

const NOTIFICATION_TYPES = ["info", "success", "warning", "error"];
// How long an autohiding notification stays: the toast's own default.
const AUTOHIDE_MS = 3000;

/**
 * gpui-shell's `Notification`: a button labeled `label` that shows a
 * notification when tapped — on a phone, the platform's toast. `on_click`
 * becomes the toast's *View* action; `on_close` runs when the toast goes on
 * its own or through that action. A toast the user swipes away is not
 * reported by the platforms, so it does not run `on_close`. A failure to show
 * it goes to `on_effect_error(message, cx)`.
 */
class NotificationElement extends Element {
  constructor(id, label, onEffectError) {
    super("view");
    if (typeof id !== "string" || id.length === 0) throw new TypeError("Notification id must not be empty");
    if (typeof label !== "string") throw new TypeError("Notification(id, label, on_effect_error) expects a string label");
    if (typeof onEffectError !== "function") throw new TypeError("Notification(id, label, on_effect_error) expects a function");
    this._k = id;
    this._spec = { title: null, message: null, type: "info", autohide: true, on_click: null, on_close: null };
    this._P().role = "button";
    this._preset([
      ["flex"], ["flex_row"], ["items_center"], ["justify_center"], ["min_h_11"], ["px_4"], radius("lg"), ["text_base"],
      ["font_medium"], ["text_color", "primary"], ["bg", tint("primary", 0.12)], ["@active", [["opacity", 0.6]]],
    ]);
    this._c = [label];
    this._on("click", (_e, cx) => this._show(id, onEffectError, cx), "Notification");
  }

  title(text) {
    if (typeof text !== "string") throw new TypeError("title(text) expects string");
    this._spec.title = text;
    return this;
  }

  message(text) {
    if (typeof text !== "string") throw new TypeError("message(text) expects string");
    this._spec.message = text;
    return this;
  }

  type(type) {
    if (!NOTIFICATION_TYPES.includes(type)) throw new TypeError("type expects info, success, warning, or error");
    this._spec.type = type;
    return this;
  }

  on_click(fn) {
    if (typeof fn !== "function") throw new TypeError("on_click(callback) expects a function");
    this._spec.on_click = fn;
    return this;
  }

  on_close(fn) {
    if (typeof fn !== "function") throw new TypeError("on_close(callback) expects a function");
    this._spec.on_close = fn;
    return this;
  }

  autohide(value) {
    if (typeof value !== "boolean") throw new TypeError("autohide(value) expects boolean");
    this._spec.autohide = value;
    return this;
  }

  _show(id, onEffectError, cx) {
    const spec = this._spec;
    let timer = null;
    let closed = false;
    const close = (cx) => {
      if (closed) return;
      closed = true;
      if (timer !== null) cancelTimer(timer);
      if (spec.on_close) spec.on_close(cx);
    };
    const options = { id, level: spec.type, title: spec.title ?? spec.message ?? "", timeout: spec.autohide ? AUTOHIDE_MS : null };
    if (spec.title !== null && spec.message !== null) options.description = spec.message;
    if (spec.on_click) {
      options.action = {
        label: "View",
        on_click: (cx) => {
          spec.on_click(cx);
          close(cx);
        },
      };
    }
    try {
      windowObject.push_toast(options);
    } catch (e) {
      onEffectError(String(e?.message ?? e), cx);
      return;
    }
    if (spec.autohide && spec.on_close) timer = startTimer(AUTOHIDE_MS, false, scope.owner, () => close(scope.cx));
  }
}

export const Notification = component((id, label, onEffectError) => new NotificationElement(id, label, onEffectError));
