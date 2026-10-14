// "gpui-component" charts: bar, line, area, pie and radar charts.
//
// gpui-shell's surface: `new BarChart((cx) => rows)` with rows shaped
// `{ label, value }` and its option methods (`grid`, `label_axis`,
// `value_axis`, `tick_margin`, `dot`, `natural` / `linear` / `step_after`,
// `inner_radius`, `pad_angle`, `labels`, `grid_levels`). gpui-component's
// accessors and styling methods (`x`, `y`, `band`, `value`, `label`, `stroke`,
// `fill`, `color`, tick counts and formats, `y_domain`) are additive: each
// `y` / `value` call adds a series, as in gpui-component.
//
// Charts are drawn from primitives every renderer has. Bars, grid lines, dots
// and labels are views placed in percentages of the plot, so they follow its
// laid-out size; lines and areas are shapes that stretch to the plot
// (`fit: "fill"`); pies and radars are square shapes, scaled to fit. The look
// is Swift Charts': hairline grid, rounded bars, trailing value labels in
// secondary text; a tap selects a point and shows its value.

import { scope, theme, handler } from "nsh:core";
import { Element, div, h_flex, v_flex, shape } from "nsh:elements";

// One axis label line: `text_xs` (0.75 rem) at a 4/3 line height. Grid lines
// sit on the middle of a value label, so the plot is inset by half of one.
const LABEL = "1rem";
const HALF_LABEL = "0.5rem";
// How much the points away from the selected one fade, as gpui-component's hover.
const DIM = 0.45;
// Points the accessibility summary reads out before it says how many are left.
const SPOKEN = 12;

const KIND_NAMES = {
  BarChart: "Bar chart",
  LineChart: "Line chart",
  AreaChart: "Area chart",
  PieChart: "Pie chart",
  RadarChart: "Radar chart",
};

// Formatting -------------------------------------------------------------------

/** `"-1234567.5"` → `"-1,234,567.5"`. */
function group(text) {
  const [int, frac] = text.split(".");
  const sign = int.startsWith("-") ? "-" : "";
  const grouped = (sign ? int.slice(1) : int).replace(/\B(?=(\d{3})+(?!\d))/g, ",");
  return sign + grouped + (frac ? "." + frac : "");
}

/** Decimals a step is written with: 0.25 → 2, 5 → 0. */
function decimalsOf(step) {
  let d = 0;
  while (d < 10 && Math.abs(Math.round(step * 10 ** d) - step * 10 ** d) > 1e-6) d++;
  return d;
}

/** A tick label: at most as many decimals as the step has, thousands grouped. */
function tickText(value, step) {
  const v = Math.abs(value) < 1e-12 ? 0 : value;
  return group(v.toFixed(decimalsOf(step)).replace(/(\.\d*?)0+$/, "$1").replace(/\.$/, ""));
}

/** A value as Rust's `f64::to_string` writes it (gpui-shell's bar labels), within float noise. */
function plain(value) {
  return Number.isInteger(value) ? String(value) : String(Math.round(value * 1e6) / 1e6);
}

// Scales -------------------------------------------------------------------------

/**
 * The smallest 1, 2 or 5 × 10ⁿ step that covers [start, stop] in at most
 * `count - 1` intervals: about `count` ticks, never more, as a phone's short
 * axis wants.
 */
function tickIncrement(start, stop, count) {
  const raw = (stop - start) / Math.max(1, count - 1);
  const power = Math.floor(Math.log10(raw));
  const error = raw / 10 ** power;
  const factor = error <= 1 + 1e-9 ? 1 : error <= 2 + 1e-9 ? 2 : error <= 5 + 1e-9 ? 5 : 10;
  return factor * 10 ** power;
}

function roundTo(value, step) {
  return Number(value.toFixed(decimalsOf(step) + 2));
}

/**
 * A value axis over [lo, hi], widened to round numbers: its ticks, about
 * `count` of them. A flat or empty range still gets an axis (0 to 1, or from
 * zero to the value) so a chart of zeros draws a baseline instead of nothing.
 */
export function niceScale(lo, hi, count = 5) {
  if (!Number.isFinite(lo) || !Number.isFinite(hi)) [lo, hi] = [0, 1];
  if (lo > hi) [lo, hi] = [hi, lo];
  if (lo === hi) {
    if (lo === 0) hi = 1;
    else if (lo > 0) lo = 0;
    else hi = 0;
  }
  let step = tickIncrement(lo, hi, count);
  for (let i = 0; i < 8; i++) {
    const nlo = Math.floor(lo / step + 1e-9) * step;
    const nhi = Math.ceil(hi / step - 1e-9) * step;
    lo = nlo;
    hi = nhi;
    const next = tickIncrement(nlo, nhi, count);
    if (next === step) break;
    step = next;
  }
  const ticks = [];
  const n = Math.round((hi - lo) / step);
  for (let i = 0; i <= n; i++) ticks.push(roundTo(lo + i * step, step));
  return { min: ticks[0], max: ticks[ticks.length - 1], step, ticks };
}

/** A pinned axis (gpui-component's `y_domain`): `count` ticks from min to max, both ends included. */
function pinnedScale(min, max, count) {
  const n = Math.max(2, count);
  const step = (max - min) / (n - 1);
  const ticks = Array.from({ length: n }, (_, i) => (i === n - 1 ? max : min + i * step));
  return { min, max, step, ticks };
}

/** Path numbers at two decimals: precise enough, and short strings to diff. */
function n2(v) {
  return String(Math.round(v * 100) / 100);
}

// Paths --------------------------------------------------------------------------

/**
 * A polyline through `points` (`[x, y]` pairs) as path data: Catmull-Rom
 * cubics for `natural` (gpui-component's default curve), straight segments for
 * `linear`, horizontal-then-vertical steps for `step_after` (and vertical-then-
 * horizontal for `step_before`, an area's lower edge drawn backwards).
 */
export function linePath(points, curve = "natural") {
  if (points.length === 0) return "";
  let d = `M${n2(points[0][0])} ${n2(points[0][1])}`;
  const n = points.length;
  for (let i = 1; i < n; i++) {
    const [x, y] = points[i];
    if (curve === "linear") {
      d += `L${n2(x)} ${n2(y)}`;
    } else if (curve === "step_after") {
      d += `H${n2(x)}V${n2(y)}`;
    } else if (curve === "step_before") {
      d += `V${n2(y)}H${n2(x)}`;
    } else {
      const p0 = points[Math.max(0, i - 2)];
      const p1 = points[i - 1];
      const p3 = points[Math.min(n - 1, i + 1)];
      const c1x = p1[0] + (x - p0[0]) / 6;
      const c1y = p1[1] + (y - p0[1]) / 6;
      const c2x = x - (p3[0] - p1[0]) / 6;
      const c2y = y - (p3[1] - p1[1]) / 6;
      d += `C${n2(c1x)} ${n2(c1y)} ${n2(c2x)} ${n2(c2y)} ${n2(x)} ${n2(y)}`;
    }
  }
  return d;
}

/** The region between `top` and `bottom` (same x positions), closed: an area or a stacked band. */
export function areaPath(top, bottom, curve = "natural") {
  if (top.length === 0) return "";
  // The lower edge runs backwards with the same curve, so a stacked band's
  // bottom follows the top of the band beneath it exactly.
  const back = bottom.slice().reverse();
  const lower = linePath(back, curve === "step_after" ? "step_before" : curve).replace(/^M/, "L");
  return `${linePath(top, curve)}${lower}Z`;
}

/**
 * A pie or donut slice between angles `a0` and `a1` (radians, clockwise from
 * twelve o'clock), centered on the origin.
 */
export function slicePath(a0, a1, outer, inner = 0) {
  const span = a1 - a0;
  if (!(span > 0) || !(outer > 0)) return "";
  // A full turn cannot be one arc (its ends coincide): draw it as two halves.
  if (span >= Math.PI * 2 - 1e-6) {
    const ring = (r, sweep) =>
      `M0 ${n2(-r)}A${n2(r)} ${n2(r)} 0 1 ${sweep} 0 ${n2(r)}A${n2(r)} ${n2(r)} 0 1 ${sweep} 0 ${n2(-r)}Z`;
    return inner > 0 ? ring(outer, 1) + ring(inner, 0) : ring(outer, 1);
  }
  const pt = (r, a) => `${n2(Math.sin(a) * r)} ${n2(-Math.cos(a) * r)}`;
  const large = span > Math.PI ? 1 : 0;
  let d = `M${pt(outer, a0)}A${n2(outer)} ${n2(outer)} 0 ${large} 1 ${pt(outer, a1)}`;
  if (inner > 0) d += `L${pt(inner, a1)}A${n2(inner)} ${n2(inner)} 0 ${large} 0 ${pt(inner, a0)}`;
  else d += "L0 0";
  return d + "Z";
}

function polygon(points) {
  return points.map(([x, y], i) => `${i === 0 ? "M" : "L"}${n2(x)} ${n2(y)}`).join("") + "Z";
}

// Colors -------------------------------------------------------------------------

/** The series palette: the theme's chart colors, in order. */
function palette(t) {
  return [t.chart_1, t.chart_2, t.chart_3, t.chart_4, t.chart_5];
}

/** A resolved `#rrggbb[aa]` color at `alpha` of its opacity; anything else passes through. */
function withAlpha(color, alpha) {
  const m = /^#([0-9a-f]{6})([0-9a-f]{2})?$/i.exec(String(color));
  if (!m) return color;
  const base = m[2] ? parseInt(m[2], 16) / 255 : 1;
  const a = Math.round(Math.max(0, Math.min(1, base * alpha)) * 255);
  return `#${m[1]}${a.toString(16).padStart(2, "0")}`;
}

/** A token name (`"primary"`) resolved against the theme, so it can be faded; colors pass through. */
function resolve(t, color) {
  if (color === null || color === undefined) return null;
  const s = String(color);
  return typeof t[s] === "string" && !s.startsWith("#") ? t[s] : s;
}

// Data ---------------------------------------------------------------------------

function isPlainObject(row) {
  if (row === null || typeof row !== "object" || Array.isArray(row)) return false;
  const proto = Object.getPrototypeOf(row);
  return proto === Object.prototype || proto === null;
}

/**
 * Reads labels and series values. Rows read with the default accessors are
 * checked as gpui-shell checks them, with its messages; custom accessors
 * take any rows but must give finite numbers.
 */
function readRows(chart) {
  const data = chart._data;
  if (!Array.isArray(data)) throw new Error("the row callback must return an array");
  const defaults = chart._labelFn === null || chart._series.some((s) => s.fn === null);
  data.forEach((row, index) => {
    if (defaults && !isPlainObject(row)) throw new Error(`chart row ${index} must be a plain object`);
    if (chart._labelFn === null && typeof row.label !== "string") {
      throw new Error(`chart row ${index} must have string field \`label\``);
    }
  });
  const labels = data.map((row, i) => String(chart._labelFn ? chart._labelFn(row, i) : row.label));
  const series = chart._series.map((s, k) => ({
    ...s,
    values: data.map((row, index) => {
      const v = s.fn ? s.fn(row, index) : row.value;
      if (typeof v !== "number" || !Number.isFinite(v)) {
        throw new Error(
          s.fn
            ? `chart row ${index}: series ${k + 1} must be a finite number, got ${JSON.stringify(v)}`
            : `chart row ${index} must have finite number field \`value\``,
        );
      }
      return v;
    }),
  }));
  return { labels, series };
}

/** "Bar chart, 7 values: Mon 8,412, Tue 9,120, …, and 5 more", for screen readers. */
function summary(kind, labels, series, format) {
  const name = KIND_NAMES[kind];
  if (labels.length === 0) return `${name}, no data`;
  const parts = [];
  for (let i = 0; i < Math.min(labels.length, SPOKEN); i++) {
    parts.push(`${labels[i]} ${series.map((s) => format(s.values[i])).join(" and ")}`);
  }
  const more = labels.length > SPOKEN ? `, and ${labels.length - SPOKEN} more` : "";
  return `${name}, ${labels.length} ${labels.length === 1 ? "value" : "values"}: ${parts.join(", ")}${more}`;
}

// Selection ----------------------------------------------------------------------

// The selected point of each chart: a tap selects it, a second tap on it clears
// it. Charts are rebuilt on every render, so the selection is kept for the
// rendering view under the chart's key: its `key`, `id` or `test_id`, else its
// position among the view's charts in that render.
const selections = new WeakMap();
const ordinals = { gen: -1, owner: null, next: 0 };

function nextOrdinal() {
  if (ordinals.gen !== scope.gen || ordinals.owner !== scope.owner) {
    ordinals.gen = scope.gen;
    ordinals.owner = scope.owner;
    ordinals.next = 0;
  }
  return ordinals.next++;
}

function selectionsOf(owner) {
  if (!owner) return null;
  let m = selections.get(owner);
  if (!m) selections.set(owner, (m = new Map()));
  return m;
}

// Checks, with gpui-shell's messages ---------------------------------------------

function bool(name, value) {
  if (typeof value !== "boolean") throw new TypeError(`Chart.${name} expects one boolean`);
  return value;
}

function positiveInt(name, value) {
  if (!Number.isInteger(value) || value < 1) throw new TypeError(`${name} expects a positive integer`);
  return value;
}

function nonNegative(name, value) {
  if (typeof value !== "number" || !Number.isFinite(value) || value < 0) {
    throw new TypeError(`${name} expects a non-negative finite number`);
  }
  return value;
}

function accessor(kind, name, fn) {
  if (typeof fn !== "function") throw new TypeError(`${kind}.${name} expects a function of the row`);
  return fn;
}

// Shared parts -------------------------------------------------------------------

function withHandler(el, id) {
  if (id !== undefined) el._h = ["click", id];
  return el;
}

/** A layer under everything that carries the chart's summary for screen readers. */
function described(text) {
  return div().absolute().inset_0().accessibility_label(text);
}

function axisText(t, content) {
  return div().whitespace_nowrap().text_xs().line_height(4 / 3).text_color(t.muted_foreground).child(content);
}

function emptyState(t) {
  return div()
    .absolute()
    .inset_0()
    .flex()
    .items_center()
    .justify_center()
    .accessibility_hidden()
    .child(div().text_sm().text_color(t.muted_foreground).child("No data"));
}

function failure(t, message) {
  return div()
    .size_full()
    .flex()
    .items_center()
    .justify_center()
    .p_2()
    .role("alert")
    .text_sm()
    .text_center()
    .text_color(t.muted_foreground)
    .child(message);
}

// The element --------------------------------------------------------------------

/**
 * What every chart shares: rows from the constructor's callback, series, the
 * selection, and children built when Rust reads the description (after every
 * option method has run). Tap handlers are registered up front, one per row,
 * because handlers can only be made while rendering.
 */
class Chart extends Element {
  constructor(kind, rows) {
    super("view");
    if (typeof rows !== "function" && !Array.isArray(rows)) {
      throw new TypeError("chart expects a row snapshot callback");
    }
    this._kind = kind;
    this._data = typeof rows === "function" ? rows(scope.cx) : rows;
    this._labelFn = null;
    this._series = [{ fn: null, stroke: null, fill: null }];
    this._custom = false;
    this._grid = true;
    this._format = null;
    this._owner = scope.owner;
    this._ord = nextOrdinal();
    this._built = undefined;
    this._preset([["flex"], ["flex_col"], ["w_full"], ["h", 200]]);
    const n = Array.isArray(this._data) ? this._data.length : 0;
    this._taps = Array.from({ length: n }, (_, i) =>
      handler((_e, cx) => {
        const m = selectionsOf(this._owner);
        if (!m) return;
        if (m.get(this._selKey) === i) m.delete(this._selKey);
        else m.set(this._selKey, i);
        cx.notify();
      }, `${kind} selection`),
    );
  }

  static new(rows) {
    return new this(rows);
  }

  child() {
    throw new TypeError("charts do not accept children");
  }

  children() {
    throw new TypeError("charts do not accept children");
  }

  grid(value) {
    this._grid = bool("grid", value);
    return this;
  }

  /** The stroke color of the last series. */
  stroke(color) {
    this._series[this._series.length - 1].stroke = String(color);
    return this;
  }

  /** The fill color of the last series. */
  fill(color) {
    this._series[this._series.length - 1].fill = String(color);
    return this;
  }

  /** Mobile: how a value is written in labels, the callout and for screen readers. */
  value_format(fn) {
    if (typeof fn !== "function") throw new TypeError(`${this._kind}.value_format expects a function of the value`);
    this._format = fn;
    return this;
  }

  // The first `y` / `value` replaces the default series (`row.value`); later ones add series.
  _addSeries(name, fn) {
    accessor(this._kind, name, fn);
    if (this._custom) this._series.push({ fn, stroke: null, fill: null });
    else this._series = [{ fn, stroke: null, fill: null }];
    this._custom = true;
    return this;
  }

  _setLabel(name, fn) {
    this._labelFn = accessor(this._kind, name, fn);
    return this;
  }

  _text(v) {
    return this._format ? String(this._format(v)) : group(plain(v));
  }

  /**
   * The chart's description for screen readers, read before its points. It
   * replaces the generated summary rather than labelling the chart's root, which
   * would hide the points from VoiceOver.
   */
  accessibility_label(text) {
    this._a11y = String(text);
    return this;
  }

  _summary(labels, series, format) {
    return this._a11y ?? summary(this._kind, labels, series, format);
  }

  /** A chart with a `test_id` names its points `<test_id>-point-<index>`, for UI tests. */
  _pointId(el, i) {
    const id = this._p?.test_id;
    return id === undefined ? el : el.test_id(`${id}-point-${i}`);
  }

  get _selKey() {
    return this._k ?? this._rid ?? this._p?.test_id ?? `#${this._ord}`;
  }

  get _selected() {
    const i = selectionsOf(this._owner)?.get(this._selKey);
    return i !== undefined && i < this._taps.length ? i : null;
  }

  // Rust reads the children after `render` returns, once every option is set.
  get _before() {
    if (this._built === undefined) {
      const t = theme();
      let model = null;
      let content;
      try {
        model = readRows(this);
      } catch (e) {
        content = failure(t, `Failed to build ${this._kind} data: ${e.message}`);
      }
      this._built = [model ? this._draw(t, model) : content];
    }
    return this._built;
  }
}

// Cartesian charts ----------------------------------------------------------------

/**
 * Bar, line and area charts: a plot with grid lines at the value ticks,
 * category labels under it and value labels on its trailing edge, where iOS
 * charts put them.
 */
class CartesianChart extends Chart {
  constructor(kind, rows) {
    super(kind, rows);
    this._axis = true;
    this._valueAxis = false;
    this._tickMargin = 1;
    this._tickCount = 5;
    this._tickFormat = null;
    this._domain = null;
  }

  tick_margin(value) {
    this._tickMargin = positiveInt("tick_margin", value);
    return this;
  }

  _setTickCount(name, value) {
    this._tickCount = positiveInt(name, value);
    return this;
  }

  _setTickFormat(name, fn) {
    if (typeof fn !== "function") throw new TypeError(`${this._kind}.${name} expects a function of the value`);
    this._tickFormat = fn;
    return this;
  }

  /** The value scale: the data from zero, widened to round ticks, or the pinned domain. */
  _scale(values) {
    if (this._domain) return pinnedScale(this._domain[0], this._domain[1], this._tickCount);
    return niceScale(Math.min(0, ...values), Math.max(0, ...values), this._tickCount);
  }

  _tick(v, scale) {
    return this._tickFormat ? String(this._tickFormat(v)) : tickText(v, scale.step);
  }

  /**
   * The frame every cartesian chart shares. `layers(pct)` draws inside the
   * plot, where `pct(value)` is the distance from its top in percent.
   */
  _frame(t, labels, series, scale, layers, { headroom = false } = {}) {
    const n = labels.length;
    const span = scale.max - scale.min || 1;
    const pct = (v) => (1 - (Math.max(scale.min, Math.min(scale.max, v)) - scale.min) / span) * 100;
    const inner = div().absolute().inset_0().child(described(this._summary(labels, series, (v) => this._text(v))));
    if (this._grid || n === 0) {
      // With nothing to plot, only the baseline: no scale to read.
      for (const tick of n === 0 ? [scale.min] : scale.ticks) {
        inner.child(
          div()
            .absolute()
            .left_0()
            .right_0()
            .top(`${pct(tick)}%`)
            .h(1)
            .bg(tick === 0 ? t.border : withAlpha(t.border, 0.6)),
        );
      }
    }
    if (n === 0) inner.child(emptyState(t));
    else inner.children(layers(pct));

    const plot = div()
      .relative()
      .flex_1()
      .min_h_0()
      .when(this._valueAxis && n > 0, (el) => el.my(HALF_LABEL))
      // Bar labels sit above their bars: the tallest keeps a line of room.
      .child(headroom && n > 0 ? div().absolute().inset_0().pt(LABEL).child(div().relative().size_full().child(inner)) : inner);
    const column = v_flex().flex_1().min_w_0().child(plot);
    if (this._axis && n > 0) {
      column.child(
        h_flex()
          .h(LABEL)
          .mt_1()
          .flex_none()
          .accessibility_hidden()
          .children(
            labels.map((label, i) =>
              div()
                .flex_1()
                .min_w_0()
                .flex()
                .justify_center()
                .child(i % this._tickMargin === 0 ? axisText(t, label) : null),
            ),
          ),
      );
    }
    const body = h_flex().size_full().items_stretch().child(column);
    if (this._valueAxis && n > 0) {
      body.child(
        v_flex()
          .flex_none()
          .pl_2()
          .accessibility_hidden()
          .child(
            v_flex()
              .flex_1()
              .justify_between()
              .children(
                scale.ticks
                  .slice()
                  .reverse()
                  .map((tick) => div().h(LABEL).flex().items_center().child(axisText(t, this._tick(tick, scale)))),
              ),
          )
          .when(this._axis && n > 0, (el) => el.child(div().h(LABEL).mt_1().flex_none())),
      );
    }
    return body;
  }

  /** One tap target per category, carrying that point's values for screen readers. */
  _targets(labels, describe, selected) {
    return h_flex()
      .absolute()
      .inset_0()
      .children(
        labels.map((label, i) =>
          withHandler(
            div()
              .flex_1()
              .h_full()
              .accessibility_label(`${label}, ${describe(i)}`)
              .map((el) => this._pointId(el, i))
              .when(selected === i, (el) => el.selected(true)),
            this._taps[i],
          ),
        ),
      );
  }

  /** The selected point's callout, along the top of the plot, kept inside it at the ends. */
  _callout(t, n, i, title, lines) {
    const center = ((i + 0.5) / n) * 100;
    const box = v_flex()
      .flex_none()
      .px_2()
      .py_1()
      .rounded_md()
      .bg(t.surface)
      .border_1()
      .border_color(t.border)
      .shadow_sm()
      .child(axisText(t, title))
      .children(
        lines.map(([color, text]) =>
          h_flex()
            .gap_1()
            .items_center()
            .when(lines.length > 1, (el) => el.child(div().size_2().rounded_full().bg(color)))
            .child(div().whitespace_nowrap().text_sm().font_semibold().text_color(t.foreground).child(text)),
        ),
      );
    const lane = h_flex().absolute().top_0().w(160).accessibility_hidden();
    if (center < 25) lane.left_0().justify_start();
    else if (center > 75) lane.right_0().justify_end();
    else lane.left(`${center}%`).ml(-80).justify_center();
    return lane.child(box);
  }
}

export class BarChart extends CartesianChart {
  constructor(rows) {
    super("BarChart", rows);
    this._barLabels = true;
    this._labelText = null;
    this._colorFn = null;
  }

  label_axis(value) {
    this._axis = bool("label_axis", value);
    return this;
  }

  value_axis(value) {
    this._valueAxis = bool("value_axis", value);
    return this;
  }

  /** gpui-component: the category of a row. */
  band(fn) {
    return this._setLabel("band", fn);
  }

  /** gpui-component: the value of a row. */
  value(fn) {
    return this._addSeries("value", fn);
  }

  /**
   * gpui-component: the text over each bar. gpui-shell writes each bar's
   * value; `false` draws none.
   */
  label(fn) {
    if (fn === false) {
      this._barLabels = false;
      return this;
    }
    this._labelText = accessor(this._kind, "label", fn);
    this._barLabels = true;
    return this;
  }

  /** A color for every bar, or gpui-component's per-row `fill` function. */
  fill(colorOrFn) {
    if (typeof colorOrFn === "function") this._colorFn = colorOrFn;
    else super.fill(colorOrFn);
    return this;
  }

  value_tick_count(n) {
    return this._setTickCount("value_tick_count", n);
  }

  value_tick_format(fn) {
    return this._setTickFormat("value_tick_format", fn);
  }

  _draw(t, { labels, series }) {
    const values = series[0].values;
    const scale = this._scale(values);
    const sel = this._selected;
    const base = resolve(t, series[0].fill ?? series[0].stroke) ?? palette(t)[0];
    const color = (i) => (this._colorFn ? resolve(t, this._colorFn(this._data[i], i)) : base);
    const barText = (i) => (this._labelText ? String(this._labelText(this._data[i], i)) : plain(values[i]));
    return this._frame(
      t,
      labels,
      series,
      scale,
      (pct) => {
        const zero = pct(0);
        const bars = h_flex()
          .absolute()
          .inset_0()
          .accessibility_hidden()
          .children(
            values.map((v, i) => {
              const top = Math.min(pct(v), zero);
              const height = Math.abs(zero - pct(v));
              const up = v >= 0;
              const cell = div()
                .relative()
                .flex_1()
                .h_full()
                .child(
                  div()
                    .absolute()
                    .left("18%")
                    .right("18%")
                    .top(`${top}%`)
                    .h(`${height}%`)
                    .bg(color(i))
                    .map((el) => (up ? el.rounded_t(4) : el.rounded_b(4)))
                    .when(sel !== null && sel !== i, (el) => el.opacity(DIM)),
                );
              if (this._barLabels) {
                const label = div().absolute().left(-12).right(-12).flex().justify_center().child(axisText(t, barText(i)));
                cell.child(up ? label.bottom(`${100 - top}%`) : label.top(`${top + height}%`));
              }
              return cell;
            }),
          );
        const layers = [bars, this._targets(labels, (i) => this._text(values[i]), sel)];
        if (sel !== null) layers.push(this._callout(t, labels.length, sel, labels[sel], [[color(sel), this._text(values[sel])]]));
        return layers;
      },
      { headroom: this._barLabels },
    );
  }
}

/** Line and area charts: a point in the middle of each category, joined by a curve. */
class SeriesChart extends CartesianChart {
  constructor(kind, rows) {
    super(kind, rows);
    this._curve = "natural";
    this._dots = false;
  }

  x_axis(value) {
    this._axis = bool("x_axis", value);
    return this;
  }

  /** gpui-component: value labels along the plot (on its trailing edge here). */
  y_axis(value) {
    this._valueAxis = bool("y_axis", value);
    return this;
  }

  natural() {
    this._curve = "natural";
    return this;
  }

  linear() {
    this._curve = "linear";
    return this;
  }

  step_after() {
    this._curve = "step_after";
    return this;
  }

  /** gpui-component: the category of a row. */
  x(fn) {
    return this._setLabel("x", fn);
  }

  /** gpui-component: a series' value; each call adds a series. */
  y(fn) {
    return this._addSeries("y", fn);
  }

  y_tick_count(n) {
    return this._setTickCount("y_tick_count", n);
  }

  y_tick_format(fn) {
    return this._setTickFormat("y_tick_format", fn);
  }

  /** gpui-component: pins the value axis to [min, max] instead of fitting the data from zero. */
  y_domain(min, max) {
    if (!Number.isFinite(min) || !Number.isFinite(max) || !(max > min)) {
      throw new TypeError(`${this._kind}.y_domain expects finite numbers with min < max`);
    }
    this._domain = [min, max];
    return this;
  }

  /** Each series' top edge, per point: its values (areas stack them). */
  _tops(series) {
    return series.map((s) => s.values);
  }

  _draw(t, { labels, series }) {
    const n = labels.length;
    const colors = series.map((s, i) => resolve(t, s.stroke) ?? palette(t)[i % 5]);
    const tops = this._tops(series);
    const scale = this._scale(tops.flat());
    const sel = this._selected;
    const area = this._kind === "AreaChart";
    return this._frame(t, labels, series, scale, (pct) => {
      // View-box units: one per category across, percent of the plot down.
      const viewBox = [0, 0, Math.max(n, 1), 100];
      const points = (values) => values.map((v, i) => [i + 0.5, pct(v)]);
      const layers = [];
      tops.forEach((values, k) => {
        const pts = points(values);
        if (area) {
          const below = k === 0 ? pts.map(([x]) => [x, pct(0)]) : points(tops[k - 1]);
          const fill = resolve(t, series[k].fill) ?? withAlpha(colors[k], 0.22);
          layers.push(stretched(areaPath(pts, below, this._curve), viewBox).fill(fill));
        }
        layers.push(stretched(linePath(pts, this._curve), viewBox).stroke(colors[k], 2));
      });
      if (sel !== null) {
        layers.push(
          div()
            .absolute()
            .top_0()
            .bottom_0()
            .left(`${((sel + 0.5) / n) * 100}%`)
            .w(1)
            .bg(withAlpha(t.muted_foreground, 0.6))
            .accessibility_hidden(),
        );
      }
      tops.forEach((values, k) => {
        values.forEach((v, i) => {
          if (!this._dots && sel !== i) return;
          const r = sel === i ? 5 : 3.5;
          layers.push(
            div()
              .absolute()
              .left(`${((i + 0.5) / n) * 100}%`)
              .top(`${pct(v)}%`)
              .size(r * 2)
              .ml(-r)
              .mt(-r)
              .rounded_full()
              .bg(colors[k])
              .border_2()
              .border_color(t.surface)
              .accessibility_hidden(),
          );
        });
      });
      layers.push(this._targets(labels, (i) => series.map((s) => this._text(s.values[i])).join(", "), sel));
      if (sel !== null) {
        layers.push(this._callout(t, n, sel, labels[sel], series.map((s, k) => [colors[k], this._text(s.values[sel])])));
      }
      return layers;
    });
  }
}

/** A path that stretches to its laid-out frame: view-box x and y scale apart. */
function stretched(path, viewBox) {
  return shape(path, { view_box: viewBox, fit: "fill" }).absolute().inset_0().accessibility_hidden();
}

export class LineChart extends SeriesChart {
  constructor(rows) {
    super("LineChart", rows);
  }

  dot() {
    this._dots = true;
    return this;
  }
}

export class AreaChart extends SeriesChart {
  constructor(rows) {
    super("AreaChart", rows);
  }

  // Series stack, as gpui-component's: each area starts where the one before ends.
  _tops(series) {
    const out = [];
    series.forEach((s, k) => out.push(s.values.map((v, i) => v + (k > 0 ? out[k - 1][i] : 0))));
    return out;
  }
}

// Pie --------------------------------------------------------------------------------

/** A square box as tall as the chart, centered in it; shapes in it share one view box. */
function squareBox() {
  return div().relative().h_full().aspect_ratio(1).max_w_full().flex_none();
}

/**
 * A pie, or a donut with `inner_radius`. Radii are proportions: the ring is
 * `outer_radius` (default 100) from its center and scaled to fit the chart,
 * so `inner_radius(60)` leaves a hole 60% of its width. `labels(true)` lists
 * each slice with its share beside the ring; tapping a row selects its slice.
 */
export class PieChart extends Chart {
  constructor(rows) {
    super("PieChart", rows);
    this._outer = 100;
    this._inner = 0;
    this._pad = 0;
    this._labels = false;
    this._colorFn = null;
  }

  inner_radius(value) {
    this._inner = nonNegative("inner_radius", value);
    return this;
  }

  outer_radius(value) {
    this._outer = nonNegative("outer_radius", value);
    return this;
  }

  /** The gap between slices, in radians as gpui-component's. */
  pad_angle(value) {
    this._pad = nonNegative("pad_angle", value);
    return this;
  }

  labels(value) {
    this._labels = bool("labels", value);
    return this;
  }

  value(fn) {
    return this._addSeries("value", fn);
  }

  /** gpui-component: the label of a row. */
  label(fn) {
    return this._setLabel("label", fn);
  }

  /** gpui-component: the color of a row's slice. */
  color(fn) {
    this._colorFn = accessor(this._kind, "color", fn);
    return this;
  }

  _draw(t, { labels, series }) {
    const values = series[0].values.map((v) => Math.max(0, v));
    const total = values.reduce((a, b) => a + b, 0);
    const sel = this._selected;
    const colors = values.map((_, i) => (this._colorFn ? resolve(t, this._colorFn(this._data[i], i)) : palette(t)[i % 5]));
    const share = (v) => (total > 0 ? `${Math.round((v / total) * 100)}%` : "0%");
    const R = this._outer || 100;
    const r = Math.min(this._inner, R * 0.95);
    const viewBox = [-R, -R, 2 * R, 2 * R];
    const root = h_flex()
      .relative()
      .size_full()
      .gap_4()
      .items_center()
      .child(described(this._summary(labels, [{ values }], (v) => `${this._text(v)}, ${share(v)}`)));
    if (labels.length === 0) return root.child(emptyState(t));

    const ring = squareBox().accessibility_hidden();
    if (total <= 0) {
      ring.child(shape(slicePath(0, Math.PI * 2, R, r), { view_box: viewBox }).absolute().inset_0().fill(t.muted));
    } else {
      const pad = values.filter((v) => v > 0).length > 1 ? this._pad : 0;
      let a = 0;
      values.forEach((v, i) => {
        const span = (v / total) * Math.PI * 2;
        if (v > 0) {
          const gap = Math.min(pad, span / 2);
          ring.child(
            shape(slicePath(a + gap / 2, a + span - gap / 2, R, r), { view_box: viewBox })
              .absolute()
              .inset_0()
              .fill(colors[i])
              .when(sel !== null && sel !== i, (el) => el.opacity(DIM)),
          );
        }
        a += span;
      });
    }
    if (sel !== null && r > 0) {
      ring.child(
        v_flex()
          .absolute()
          .inset_0()
          .items_center()
          .justify_center()
          .child(axisText(t, labels[sel]))
          .child(div().text_lg().font_semibold().text_color(t.foreground).child(this._text(values[sel])))
          .child(axisText(t, share(values[sel]))),
      );
    }
    root.child(div().h_full().flex().justify_center().map((el) => (this._labels ? el.flex_none() : el.flex_1())).child(ring));
    if (this._labels) {
      root.child(
        v_flex()
          .flex_1()
          .min_w_0()
          .children(
            labels.map((label, i) =>
              withHandler(
                h_flex()
                  .gap_2()
                  .items_center()
                  .min_h(32)
                  .py_1()
                  .accessibility_label(`${label}, ${this._text(values[i])}, ${share(values[i])}`)
                  .map((el) => this._pointId(el, i))
                  .when(sel === i, (el) => el.selected(true))
                  .when(sel !== null && sel !== i, (el) => el.opacity(0.6))
                  .active((s) => s.opacity(0.5))
                  .child(div().size_2p5().flex_none().rounded_full().bg(colors[i]))
                  .child(div().flex_1().min_w_0().text_sm().text_color(t.foreground).child(label))
                  .child(div().flex_none().text_sm().text_color(t.muted_foreground).child(share(values[i]))),
                this._taps[i],
              ),
            ),
          ),
      );
    }
    return root;
  }
}

// Radar ------------------------------------------------------------------------------

/**
 * A radar: one spoke per row, rings at even steps up to the outer value, each
 * series a closed polygon, labels outside the ring.
 */
export class RadarChart extends Chart {
  constructor(rows) {
    super("RadarChart", rows);
    this._levels = 4;
    this._dots = false;
    this._max = null;
  }

  grid_levels(value) {
    this._levels = positiveInt("grid_levels", value);
    return this;
  }

  dot() {
    this._dots = true;
    return this;
  }

  value(fn) {
    return this._addSeries("value", fn);
  }

  label(fn) {
    return this._setLabel("label", fn);
  }

  /** The value at the outer ring; by default the largest value, rounded up. */
  max_value(value) {
    this._max = nonNegative("max_value", value);
    return this;
  }

  _draw(t, { labels, series }) {
    const n = labels.length;
    const root = div()
      .relative()
      .size_full()
      .flex()
      .justify_center()
      .child(described(this._summary(labels, series, (v) => this._text(v))));
    if (n === 0) return root.child(emptyState(t));
    const colors = series.map((s, i) => resolve(t, s.stroke) ?? palette(t)[i % 5]);
    const top = Math.max(0, ...series.flatMap((s) => s.values));
    const max = this._max || niceScale(0, top, this._levels).max;
    // The ring has radius 1 in a view box 2.8 across: the rest is room for labels.
    const R = 1.4;
    const viewBox = [-R, -R, 2 * R, 2 * R];
    const at = (radius, i) => {
      const a = (i / n) * Math.PI * 2;
      return [Math.sin(a) * radius, -Math.cos(a) * radius];
    };
    const percent = (v) => `${((v + R) / (2 * R)) * 100}%`;
    const box = squareBox();
    if (this._grid) {
      let d = "";
      for (let level = 1; level <= this._levels; level++) d += polygon(labels.map((_, i) => at(level / this._levels, i)));
      for (let i = 0; i < n; i++) {
        const [x, y] = at(1, i);
        d += `M0 0L${n2(x)} ${n2(y)}`;
      }
      box.child(shape(d, { view_box: viewBox }).absolute().inset_0().stroke(t.border, 0.012).accessibility_hidden());
    }
    series.forEach((s, k) => {
      const pts = s.values.map((v, i) => at(Math.max(0, Math.min(1, v / max)), i));
      box.child(
        shape(polygon(pts), { view_box: viewBox })
          .absolute()
          .inset_0()
          .fill(resolve(t, s.fill) ?? withAlpha(colors[k], 0.2))
          .stroke(colors[k], 0.016)
          .accessibility_hidden(),
      );
      if (this._dots) {
        for (const [x, y] of pts) {
          box.child(div().absolute().left(percent(x)).top(percent(y)).size_2().ml(-4).mt(-4).rounded_full().bg(colors[k]).accessibility_hidden());
        }
      }
    });
    labels.forEach((label, i) => {
      const [x, y] = at(1.12, i);
      // Labels grow away from the ring: leftward on its left, rightward on its right.
      const side = x < -0.05 ? "left" : x > 0.05 ? "right" : "center";
      box.child(
        div()
          .absolute()
          .left(percent(x))
          .top(percent(y))
          .w(120)
          .ml(side === "left" ? -120 : side === "right" ? 0 : -60)
          .h(LABEL)
          .mt(y < -0.05 ? "-1rem" : y > 0.05 ? "0rem" : "-0.5rem")
          .flex()
          .map((el) => (side === "left" ? el.justify_end() : side === "right" ? el.justify_start() : el.justify_center()))
          .accessibility_label(`${label}, ${series.map((s) => this._text(s.values[i])).join(", ")}`)
          .child(axisText(t, label)),
      );
    });
    return root.child(box);
  }
}
