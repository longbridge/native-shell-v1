// "gpui-component" conversation: messages, bubbles, attachments, markers and
// the message scroller, with gpui-component's names and the iOS Messages look.
//
// Each part is composed from the primitives, so every renderer draws it with
// no renderer change. Colors are theme tokens by name (the core resolves them,
// so both appearances follow the theme); text sizes are points the renderers
// scale with the user's text size; lengths are rems.
//
// As in gpui-component, the parts own layout and look only. The message
// record, delivery state, upload task and unread meaning stay with the
// program, which renders their current state into these elements.

import { N, refuseStateInRender, theme } from "nsh:core";
import { Element, chainKey, list, shape } from "nsh:elements";
import { Button, Spinner } from "nsh:components";

const ALIGNMENTS = ["start", "end"];

function oneOf(component, method, values, value) {
  if (!values.includes(value)) {
    throw new TypeError(`unsupported ${component}.${method} value \`${value}\` (use one of ${values.join(", ")})`);
  }
  return value;
}

function stableId(component, id) {
  if (typeof id !== "string" || id.trim() === "") throw new TypeError(`${component} expects a non-empty stable id`);
  return id;
}

/** An internal part with a fixed chain. */
function part(ops) {
  const el = new Element("view");
  el._sk = chainKey(ops);
  return el;
}

/** A styled slot holding a string or an element. */
function slotOf(ops, x) {
  return part(ops).child(x);
}

/** A token with an alpha, from the current theme (`#rrggbb` → `#rrggbbaa`). */
function translucent(token, alpha) {
  const hex = String(theme()[token] ?? "#000000");
  return hex.slice(0, 7) + Math.round(alpha * 255).toString(16).padStart(2, "0");
}

// Outline glyphs on a 24-unit grid with a round 2-unit stroke.
const GLYPHS = {
  file: "M14 3H7a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h10a2 2 0 0 0 2-2V8zM14 3v5h5",
  alert: "M12 8v4M12 16h.01M22 12a10 10 0 1 1-20 0 10 10 0 0 1 20 0Z",
  retry: "M21 12a9 9 0 1 1-3-6.7L21 8M21 3v5h-5",
  close: "M18 6 6 18M6 6l12 12",
  down: "M12 5v14M5 12l7 7 7-7",
};

function glyph(name, color, size) {
  return shape(GLYPHS[name], { view_box: [0, 0, 24, 24] })
    .stroke(color, 2)
    .w(size)
    .h(size)
    .flex_none()
    .accessibility_hidden();
}

/** Routes ordinary children into `this._slot`, a part the component places. */
const Slotted = (Base) =>
  class extends Base {
    child(x) {
      this._slot.child(x);
      return this._slotChanged();
    }

    children(xs) {
      this._slot.children(xs);
      return this._slotChanged();
    }

    _slotChanged() {
      return this;
    }
  };

// Bubble ----------------------------------------------------------------------

const BUBBLE_VARIANTS = ["filled", "secondary", "muted", "tinted", "outline", "ghost", "destructive"];

// iOS Messages: 17 pt body text on an 18 pt radius, 12 × 7 padding, at most
// 80% of the row so the sender reads from the edge.
const BUBBLE_BASE = [
  ["flex"], ["flex_col"], ["min_w_0"], ["max_w", "80%"], ["overflow_hidden"], ["rounded", 18], ["px_3"], ["py", 7],
  ["text_size", 17], ["line_height", 22 / 17], ["border_1"], ["border_color", "transparent"],
];

const BUBBLE_SURFACES = {
  filled: [["bg", "primary"], ["text_color", "primary_foreground"]],
  // The incoming gray of Messages is iOS's systemGray5: the `secondary` token.
  secondary: [["bg", "secondary"], ["text_color", "secondary_foreground"]],
  muted: [["bg", "muted"], ["text_color", "foreground"]],
  tinted: [["bg", "accent"], ["text_color", "foreground"]],
  outline: [["bg", "background"], ["border_color", "border"], ["text_color", "foreground"]],
  ghost: [["max_w_full"], ["rounded", 0], ["border_0"], ["p_0"], ["text_color", "foreground"]],
};

/**
 * A message bubble whose children are its visible content. `.tail()`
 * (mobile) tightens the corner toward the sender, as the last bubble of a
 * burst in Messages.
 */
export class Bubble extends Element {
  constructor() {
    super("view");
    this._align = null;
    this._inherited = null;
    this._variant = "filled";
    this._tail = false;
    this._sync();
  }

  alignment(value) {
    this._align = oneOf("Bubble", "alignment", ALIGNMENTS, value);
    return this._sync();
  }

  variant(value) {
    this._variant = oneOf("Bubble", "variant", BUBBLE_VARIANTS, value);
    return this._sync();
  }

  /** Mobile: the last bubble of a burst, its corner toward the sender tightened. */
  tail(value = true) {
    this._tail = Boolean(value);
    return this._sync();
  }

  _sync() {
    const align = this._align ?? this._inherited;
    const v = this._variant;
    const ops = [...BUBBLE_BASE];
    if (v === "destructive") {
      // Color is not the only cue: the program says what failed in the text.
      ops.push(["bg", translucent("destructive", theme().is_dark ? 0.2 : 0.1)], ["text_color", "destructive"]);
    } else {
      ops.push(...BUBBLE_SURFACES[v]);
    }
    if (align === "start") ops.push(["self_start"]);
    if (align === "end") ops.push(["self_end"]);
    if (this._tail && v !== "ghost") ops.push([align === "end" ? "rounded_br" : "rounded_bl", 6]);
    return this._preset(ops);
  }
}

// Message ---------------------------------------------------------------------

const AVATAR_COLUMN = 40; // a 32 pt avatar and the 8 pt gap beside it

/** The content stack: bubbles and attachments two points apart, on the message's edge. */
class MessageContent extends Element {
  constructor() {
    super("view");
    this._align = "start";
    this._bubbles = [];
  }

  child(x) {
    if (x instanceof Bubble) {
      this._bubbles.push(x);
      x._inherited = this._align;
      x._sync();
    }
    return super.child(x);
  }

  _inherit(align) {
    this._align = align;
    this._sk = chainKey([["flex"], ["flex_col"], ["w_full"], ["min_w_0"], ["gap_0p5"], [align === "end" ? "items_end" : "items_start"]]);
    for (const b of this._bubbles) {
      b._inherited = align;
      b._sync();
    }
  }
}

/**
 * A message row: an optional avatar, header and footer around a content stack
 * that holds the ordinary children (bubbles, attachments). `alignment("end")`
 * puts the row on the trailing edge, for your own messages, and the bubbles
 * in it follow unless one sets its own alignment.
 */
export class Message extends Slotted(Element) {
  constructor() {
    super("view");
    this._align = "start";
    this._avatar = null;
    this._header = null;
    this._footer = null;
    this._slot = new MessageContent();
    this._sync();
  }

  alignment(value) {
    this._align = oneOf("Message", "alignment", ALIGNMENTS, value);
    return this._sync();
  }

  /** The sender's picture, level with the last bubble. */
  avatar(el) {
    this._avatar = slotOf([["flex_none"], ["min_w_8"]], el);
    return this._sync();
  }

  /** Metadata above the content: the sender in a group, a time. */
  header(x) {
    this._header = slotOf(
      [["flex"], ["flex_row"], ["items_center"], ["gap_1"], ["px_3"], ["text_size", 13], ["text_color", "muted_foreground"]],
      x,
    );
    return this._sync();
  }

  /** A line under the row: delivery state, or a failure and its retry. */
  footer(x) {
    this._footer = slotOf(
      [["flex"], ["flex_row"], ["items_center"], ["gap_1"], ["px_1"], ["text_size", 12], ["text_color", "muted_foreground"]],
      x,
    );
    return this._sync();
  }

  _sync() {
    const end = this._align === "end";
    const edge = end ? "items_end" : "items_start";
    this._preset([["flex"], ["flex_col"], ["w_full"], ["min_w_0"], ["gap_1"], [edge]]);
    this._slot._inherit(this._align);
    const stack = part([["flex"], ["flex_col"], ["flex_1"], ["min_w_0"], ["gap_1"], [edge]]);
    stack._before = this._header ? [this._header, this._slot] : [this._slot];
    const row = part([["flex"], [end ? "flex_row_reverse" : "flex_row"], ["items_end"], ["w_full"], ["min_w_0"], ["gap_2"]]);
    row._before = this._avatar ? [this._avatar, stack] : [stack];
    let footer = this._footer;
    // Under the content column, not under the avatar.
    if (footer && this._avatar) footer = part([[end ? "mr" : "ml", AVATAR_COLUMN]]).child(footer);
    this._before = footer ? [row, footer] : [row];
    return this;
  }
}

// Marker ----------------------------------------------------------------------

const MARKER_VARIANTS = ["plain", "separator", "border"];
const MARKER_ALIGNMENTS = ["start", "center", "end"];
const LOADING_STYLES = ["spinner", "shimmer"];
const JUSTIFY = { start: "justify_start", center: "justify_center", end: "justify_end" };

/**
 * A compact status row: a day label, an unread line, a system notice. The
 * program supplies the words; the marker places them and draws the
 * separator lines, the border or the loading treatment.
 */
export class Marker extends Slotted(Element) {
  constructor(id) {
    super("view");
    this._k = stableId("Marker", id);
    this._variant = "plain";
    this._align = null;
    this._loading = false;
    this._loadingStyle = "spinner";
    this._slot = part([["flex"], ["flex_row"], ["items_center"], ["gap_1"], ["min_w_0"]]);
    this._sync();
  }

  variant(value) {
    this._variant = oneOf("Marker", "variant", MARKER_VARIANTS, value);
    return this._sync();
  }

  /** Where the content sits in the row (gpui-component's `MarkerAlignment`). */
  alignment(value) {
    this._align = oneOf("Marker", "alignment", MARKER_ALIGNMENTS, value);
    return this._sync();
  }

  loading(value = true) {
    this._loading = Boolean(value);
    return this._sync();
  }

  loading_style(value) {
    this._loadingStyle = oneOf("Marker", "loading_style", LOADING_STYLES, value);
    return this._sync();
  }

  _sync() {
    const v = this._variant;
    const align = this._align ?? (v === "separator" ? "center" : "start");
    // Caption 1 in the secondary label color, as the timestamps of Messages.
    const ops = [
      ["flex"], ["flex_row"], ["items_center"], [JUSTIFY[align]], ["w_full"], ["min_h", "1rem"], ["gap_2"],
      ["text_size", 12], ["font_weight", 500], ["text_color", "muted_foreground"],
    ];
    if (v === "border") ops.push(["border_b_1"], ["border_color", "border"], ["pb_2"]);
    this._preset(ops);
    // Shimmer is ShimmerText's treatment: the secondary label, still, announced as busy.
    if (this._loading) this._P().a11y_state = JSON.stringify({ busy: true });
    else if (this._p) delete this._p.a11y_state;

    const content = [];
    if (this._loading && this._loadingStyle === "spinner") content.push(Spinner.new().flex_none());
    content.push(this._slot);
    if (v !== "separator") {
      this._before = content;
      return this;
    }
    // An explicit edge keeps only the line on the far side of the label.
    const line = () => part([["flex_1"], ["h_px"], ["bg", "border"]]).accessibility_hidden();
    this._before = [...(align === "start" ? [] : [line()]), ...content, ...(align === "end" ? [] : [line()])];
    return this;
  }
}

// Attachment ------------------------------------------------------------------

const STATUSES = ["pending", "uploading", "processing", "failed", "complete"];
const AXES = ["horizontal", "vertical"];
const SIZES = ["xsmall", "small", "medium", "large"];

// gpui-component's chip metrics in rems (height, width, media, glyph, gap,
// padding), its title and description sizes in points, and the side of a
// vertical tile's square preview.
const METRICS = {
  xsmall: { height: 2.5, width: 11, media: 1.75, glyph: 0.875, gap: 0.375, pad: 0.25, title: 11, description: 10, tile: 4 },
  small: { height: 3, width: 12.5, media: 2, glyph: 1, gap: 0.5, pad: 0.375, title: 12, description: 11, tile: 6 },
  medium: { height: 3.5, width: 14.5, media: 2.375, glyph: 1.25, gap: 0.625, pad: 0.5, title: 15, description: 13, tile: 10 },
  large: { height: 4, width: 17, media: 2.75, glyph: 1.5, gap: 0.75, pad: 0.625, title: 17, description: 15, tile: 15 },
};

const rem = (n) => `${n}rem`;

/**
 * One file or image: a chip in a composer or a thread (horizontal), or an
 * image tile (vertical). The status draws the lifecycle: a spinner, or the
 * upload bar and percentage, while in progress; a destructive edge and a
 * retry control once failed; a dashed edge while pending. Ordinary children
 * join the metadata under the title and description.
 */
export class Attachment extends Slotted(Element) {
  constructor(id) {
    super("view");
    this._id = stableId("Attachment", id);
    this._k = this._id;
    this._status = "complete";
    this._axis = "horizontal";
    this._size = "medium";
    this._media = null;
    this._title = null;
    this._description = null;
    this._progress = null;
    this._onRemove = null;
    this._onRetry = null;
    this._slot = part([["flex"], ["flex_col"], ["min_w_0"]]);
    this._sync();
  }

  status(value) {
    this._status = oneOf("Attachment", "status", STATUSES, value);
    return this._sync();
  }

  axis(value) {
    this._axis = oneOf("Attachment", "axis", AXES, value);
    return this._sync();
  }

  /** The semantic size (`xsmall` to `large`); a length is GPUI's `size` style. */
  size(value) {
    if (typeof value === "string" && SIZES.includes(value)) {
      this._size = value;
      return this._sync();
    }
    return super.size(value);
  }

  /** Mobile: the preview, an icon or an `image(src)` that fills the media area. */
  media(el) {
    if (el instanceof Element && el.__el === "image") el.object_fit("cover");
    this._media = el;
    return this._sync();
  }

  /** Mobile: the file name; it shows the status with the attachment. */
  title(text) {
    this._title = String(text);
    return this._sync();
  }

  /** Mobile: kind and size ("PDF · 2.4 MB"); red once failed, the percentage appended while uploading. */
  description(text) {
    this._description = String(text);
    return this._sync();
  }

  /** Upload progress, 0 to 100, drawn while uploading. */
  progress(percent) {
    const p = Number(percent);
    this._progress = Number.isFinite(p) ? Math.min(100, Math.max(0, p)) : null;
    return this._sync();
  }

  on_remove(fn) {
    if (typeof fn !== "function") throw new TypeError("Attachment.on_remove(handler) expects a function");
    this._onRemove = fn;
    return this._sync();
  }

  on_retry(fn) {
    if (typeof fn !== "function") throw new TypeError("Attachment.on_retry(handler) expects a function");
    this._onRetry = fn;
    return this._sync();
  }

  /** Opens the attachment; the card becomes a button. */
  on_click(fn) {
    this._P().role = "button";
    return super.on_click(fn);
  }

  _slotChanged() {
    return this._sync();
  }

  _percent() {
    return this._status === "uploading" && this._progress !== null ? Math.round(this._progress) : null;
  }

  _sync() {
    const m = METRICS[this._size];
    const failed = this._status === "failed";
    const tile = this._axis === "vertical";
    const hasContent = this._title !== null || this._description !== null || (this._slot._c?.length ?? 0) > 0;
    const bare = tile && !hasContent;

    const ops = [["flex"], [tile ? "flex_col" : "flex_row"], ["relative"], ["overflow_hidden"], ["max_w_full"]];
    if (bare) {
      ops.push(["rounded", 18], ["w", rem(m.tile)]);
    } else {
      ops.push(
        ["rounded_lg"], ["border_1"], ["border_color", failed ? "destructive" : "border"], ["bg", "background"],
        ["gap", rem(m.gap)], ["p", rem(m.pad)], ["w", rem(tile ? m.tile + 2 * m.pad : m.width)],
      );
      if (!tile) ops.push(["items_center"], ["min_h", rem(m.height)]);
    }
    if (this._status === "pending") ops.push(["border_dashed"]);
    this._preset(ops);

    const parts = [this._mediaPart(m, tile, bare)];
    if (hasContent) parts.push(this._contentPart(m));
    if (tile) {
      if (this._onRemove) parts.push(part([["absolute"], ["top_1"], ["right_1"]]).child(this._control("remove", "Remove", "close", "foreground", this._onRemove, true)));
    } else {
      if (failed && this._onRetry) parts.push(this._control("retry", "Retry", "retry", "destructive", this._onRetry));
      if (this._onRemove) parts.push(this._control("remove", "Remove", "close", "muted_foreground", this._onRemove));
      const percent = this._percent();
      if (percent !== null) parts.push(this._bar(percent));
    }
    this._before = parts;
    return this;
  }

  _mediaPart(m, tile, bare) {
    const failed = this._status === "failed";
    const busy = this._status === "uploading" || this._status === "processing";
    const image = this._media instanceof Element && this._media.__el === "image";
    const ops = [["relative"], ["flex"], ["items_center"], ["justify_center"], ["overflow_hidden"], ["flex_none"]];
    if (tile) ops.push(["w", rem(m.tile)], ["h", rem(m.tile)], ["rounded", bare ? 18 : 8]);
    else ops.push(["w", rem(m.media)], ["h", rem(m.media)], ["rounded_md"]);
    if (!image) ops.push(["bg", failed ? translucent("destructive", theme().is_dark ? 0.2 : 0.1) : "muted"]);
    const media = part(ops);
    const glyphSize = rem(tile ? m.glyph * 1.5 : m.glyph);

    if (image) {
      media.child(part([["absolute"], ["inset_0"]]).child(this._media.size_full()));
      if (!busy && !failed) return media;
      // A scrim keeps the photo recognizable under the status.
      const scrim = part([
        ["absolute"], ["inset_0"], ["flex"], ["flex_col"], ["items_center"], ["justify_center"], ["gap_1"],
        ["bg", failed ? "#00000080" : "#00000059"],
      ]);
      if (failed && this._onRetry) scrim.child(this._control("retry", "Retry", "retry", "destructive", this._onRetry, true));
      else if (failed) scrim.child(glyph("alert", "#ffffff", glyphSize));
      else scrim.child(spinner("#ffffff"));
      const percent = this._percent();
      if (percent !== null) scrim.child(slotOf([["text_size", 12], ["font_weight", 600], ["text_color", "#ffffff"]], `${percent}%`));
      return media.child(scrim);
    }
    if (busy) return media.child(spinner("primary"));
    if (failed) {
      if (tile && this._onRetry) return media.child(this._control("retry", "Retry", "retry", "destructive", this._onRetry, true));
      return media.child(glyph("alert", "destructive", glyphSize));
    }
    return media.child(this._media ?? glyph("file", "muted_foreground", glyphSize));
  }

  _contentPart(m) {
    const failed = this._status === "failed";
    const content = part([["flex"], ["flex_col"], ["flex_1"], ["min_w_0"], ["gap_0p5"]]);
    if (this._title !== null) {
      content.child(slotOf([["text_size", m.title], ["font_weight", 500], ["text_color", "foreground"], ["truncate"]], this._title));
    }
    let description = this._description;
    const percent = this._percent();
    if (percent !== null) description = description ? `${description} · ${percent}%` : `${percent}%`;
    if (description !== null) {
      content.child(slotOf([["text_size", m.description], ["text_color", failed ? "destructive" : "muted_foreground"], ["truncate"]], description));
    }
    return content.child(this._slot);
  }

  /** The upload bar along the chip's bottom edge. */
  _bar(percent) {
    return part([["absolute"], ["left_0"], ["right_0"], ["bottom_0"], ["h", 3], ["bg", "muted"]])
      .role("progress_bar")
      .accessibility_label("Upload progress")
      .accessibility_value(`${percent}%`)
      .child(part([["h_full"], ["bg", "primary"], ["w", `${percent}%`]]));
  }

  /** Remove or retry: a 24 pt disc with a 44 pt touch area. */
  _control(kind, label, name, color, fn, disc = false) {
    return Button.new(`${this._id}-${kind}`)
      .test_id(`${this._id}-${kind}`)
      .accessibility_label(label)
      .flex()
      .flex_none()
      .items_center()
      .justify_center()
      .size_6()
      .rounded_full()
      .bg(disc ? "background" : "muted")
      .when(disc, (b) => b.shadow_sm())
      .hit_slop(10)
      .active((s) => s.opacity(0.6))
      .on_click(fn)
      .child(glyph(name, color, 14));
  }
}

function spinner(color) {
  const s = Spinner.new().flex_none();
  s._P().tint = color;
  return s;
}

// MessageScroller -------------------------------------------------------------

const AT_END = 24; // points from the end that still read as "at the latest"

function itemCount(what, value) {
  const n = Number(value);
  if (!Number.isInteger(n) || n < 0) throw new TypeError(`${what} expects a non-negative integer item_count`);
  return n;
}

/** gpui-component's `MessageScrollerState`: the row count and whether the reader is at the end. */
class ScrollerState {
  constructor(count) {
    this._count = count;
    this._list = null;
    this._scrolledUp = false;
  }

  item_count() {
    return this._count;
  }

  /** Whether the reader has scrolled away from the latest rows. */
  is_scrolled_up() {
    return this._scrolledUp;
  }

  /** Whether added rows keep the end in view: the reader is at the end. */
  is_following_tail() {
    return !this._scrolledUp;
  }

  /** Replaces the row count and returns to the latest row. */
  reset(count) {
    this._count = itemCount("MessageScrollerState.reset", count);
    this.scroll_to_end();
  }

  /** Replaces rows `[start, end)` with `count` rows; false when the range is outside the list. */
  splice(start, end, count) {
    if (!(Number.isInteger(start) && Number.isInteger(end) && start >= 0 && start <= end && end <= this._count)) return false;
    this._count += itemCount("MessageScrollerState.splice", count) - (end - start);
    return true;
  }

  append(count) {
    return this.splice(this._count, this._count, count);
  }

  /** Older rows above the current ones; the reader's position is kept. */
  prepend(count) {
    return this.splice(0, 0, count);
  }

  /** Native lists measure rows as they change; kept for gpui-component code. */
  remeasure() {}

  remeasure_items(start, end) {
    return start >= 0 && start <= end && end <= this._count;
  }

  scroll_to_item(index) {
    const i = Number(index);
    if (!Number.isInteger(i) || i < 0 || i >= this._count || this._list === null) return false;
    N.command("scroll_to_index", "__list:" + this._list, JSON.stringify({ index: i, position: "top", animated: true }));
    return true;
  }

  /** Back to the latest row, following it again. */
  scroll_to_end() {
    this._scrolledUp = false;
    if (this._list === null || this._count === 0) return;
    N.command("scroll_to_index", "__list:" + this._list, JSON.stringify({ index: this._count - 1, position: "bottom", animated: true }));
  }
}

/** Creates the retained state of a `MessageScroller` (gpui-shell's call form, no `new`). */
export function MessageScrollerState(item_count) {
  refuseStateInRender("MessageScrollerState(item_count)");
  return new ScrollerState(itemCount("MessageScrollerState", item_count));
}

/**
 * A conversation transcript: a virtualized list that opens at its end and
 * keeps the newest row in view as rows arrive and the keyboard rises (a
 * bottom-aligned `list`). Scrolled away from the end, a jump-to-latest button
 * appears. Rows come from `render_item(index)`; a row's `.key()` or `.id()`
 * keeps its identity when history is prepended. `.test_id()` names the list.
 */
export class MessageScroller extends Element {
  constructor(id, state, render_item) {
    super("view");
    if (typeof id !== "string" || id.trim() === "" || !(state instanceof ScrollerState) || typeof render_item !== "function") {
      throw new TypeError("MessageScroller expects a non-empty id, MessageScrollerState, and row renderer");
    }
    this._id = id;
    this._state = state;
    this._jump = true;
    this._jumpLabel = "Jump to latest";
    state._list = id;

    // `list` asks for a row's key, then for the row: render each index once.
    const rows = new Map();
    const rowAt = (i) => {
      if (!rows.has(i)) rows.set(i, render_item(i) ?? new Element("view"));
      return rows.get(i);
    };
    const keyOf = (i) => {
      const el = rowAt(i);
      return (el instanceof Element && (el._k ?? el._rid)) || `row-${i}`;
    };
    this._list = list(id, state._count, keyOf, rowAt)
      .alignment("bottom")
      .size_full()
      .on_scroll((e, cx) => {
        const fromEnd = e.content_height - (e.y + e.viewport_height);
        const up = Number.isFinite(fromEnd) && fromEnd > AT_END;
        if (up !== state._scrolledUp) {
          state._scrolledUp = up;
          cx.notify();
        }
      });
    this._sync();
  }

  /** Native lists draw their own scroll indicator; accepted. */
  scrollbar(_value) {
    return this;
  }

  jump_button(value) {
    this._jump = Boolean(value);
    return this._sync();
  }

  jump_button_label(label) {
    if (typeof label !== "string" || label.trim() === "") throw new TypeError("MessageScroller.jump_button_label expects non-empty text");
    this._jumpLabel = label;
    return this._sync();
  }

  /** Mobile: the row height assumed before a row is measured (as `list`). */
  estimated_row_height(px) {
    this._list.estimated_row_height(px);
    return this;
  }

  /** The list carries the test id: it is what a test scrolls and reads. */
  test_id(s) {
    this._list.test_id(s);
    return this;
  }

  _sync() {
    this._preset([["relative"], ["flex"], ["flex_col"], ["min_h_0"]]);
    if (!(this._jump && this._state._scrolledUp)) {
      this._before = [this._list];
      return this;
    }
    const state = this._state;
    const jump = Button.new(`${this._id}-jump`)
      .test_id(`${this._id}-jump`)
      .accessibility_label(this._jumpLabel)
      .flex()
      .items_center()
      .justify_center()
      .size_9()
      .rounded_full()
      .bg("background")
      .border_1()
      .border_color("border")
      .shadow_md()
      .hit_slop(4)
      .active((s) => s.opacity(0.6))
      .on_click((_e, cx) => {
        state.scroll_to_end();
        cx.notify();
      })
      .child(glyph("down", "primary", 18));
    this._before = [this._list, part([["absolute"], ["right_4"], ["bottom_3"]]).child(jump)];
    return this;
  }
}
