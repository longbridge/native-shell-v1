// "gpui-component" calendar: an inline month grid, drawn as iOS's inline
// calendar draws one. The month and year lead with the previous and next
// month buttons trailing; weekday initials head the columns; the selected
// day is a tinted disc, a range a tinted band between two discs, today is in
// the tint, unavailable days are dimmed. The week starts on the locale's
// first day. Dates are ISO strings (`"2026-10-14"`) throughout.

import { scope, markDirty, refuseStateInRender, windowInfo } from "nsh:core";
import { Element, div, h_flex, v_flex, chainKey } from "nsh:elements";
import { IconElement } from "nsh:component/icons";
import { TEXT, LEADING } from "nsh:component/shared";

const MONTHS = ["January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December"];
const WEEKDAYS = ["Sunday", "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday"];

/** Regions whose week starts on Sunday or Saturday (CLDR); the rest start on Monday. */
const SUNDAY = new Set(
  "AG AS BD BR BS BT BW BZ CA CO DM DO ET GT GU HK HN ID IL IN JM JP KE KH KR LA MH MM MO MT MX MZ NI NP PA PE PH PK PR PT PY SA SG SV TH TT TW UM US VE VI WS YE ZA ZW".split(" "),
);
const SATURDAY = new Set("AE AF BH DJ DZ EG IQ IR JO KW LY OM QA SD SY".split(" "));

/** The first day of the week for a BCP 47 locale (`"en-US"` → 0, Sunday; `"de-DE"` → 1, Monday). */
export function firstDayOfWeek(locale) {
  const region = /[-_]([A-Za-z]{2})\b/.exec(String(locale ?? ""))?.[1]?.toUpperCase() ?? "US";
  return SUNDAY.has(region) ? 0 : SATURDAY.has(region) ? 6 : 1;
}

const pad = (n) => String(n).padStart(2, "0");
const iso = (y, m, d) => `${y}-${pad(m)}-${pad(d)}`;

function parse(date, what) {
  const m = /^(\d{4})-(\d{2})-(\d{2})$/.exec(String(date));
  if (m === null) throw new TypeError(`${what}: a date is "YYYY-MM-DD", got ${JSON.stringify(date)}`);
  return [Number(m[1]), Number(m[2]), Number(m[3])];
}

function todayIso() {
  const now = new Date();
  return iso(now.getFullYear(), now.getMonth() + 1, now.getDate());
}

const daysIn = (y, m) => new Date(Date.UTC(y, m, 0)).getUTCDate();
const weekday = (y, m, d) => new Date(Date.UTC(y, m - 1, d)).getUTCDay();

function released(what) {
  return new Error(`${what}: this calendar state has been released`);
}

/**
 * Retained calendar state: the selection (one date, or a range once
 * `set_date([start, end])` has put it in range mode), the month on screen and
 * which days can be chosen. `on("change", (date, cx))` reports a selection the
 * user made, with `date()`'s shape.
 */
class CalendarStateObject {
  #released = false;

  constructor() {
    refuseStateInRender("CalendarState()");
    this._range = false;
    this._start = null;
    this._end = null;
    this._today = todayIso();
    const [y, m] = parse(this._today, "CalendarState");
    this._year = y;
    this._month = m;
    this._disabled = null;
    this._marker = null;
    this._listeners = [];
    this._owner = scope.owner;
    this._shownBy = null;
  }

  #live(what) {
    if (this.#released) throw released(what);
  }

  _changed() {
    markDirty(this._shownBy ?? this._owner);
  }

  /** The selected date (`null` if none), or `[start, end]` in range mode. */
  date() {
    this.#live("state.date()");
    return this._range ? [this._start, this._end] : this._start;
  }

  /** A date (or `null`) selects one day; `[start, end]` (either may be `null`) selects a range. Shows its month. */
  set_date(date) {
    this.#live("state.set_date(date)");
    if (Array.isArray(date)) {
      const [start, end] = date.map((d) => (d === null || d === undefined ? null : iso(...parse(d, "state.set_date([start, end])"))));
      this._range = true;
      this._start = start;
      this._end = start !== null && end !== null && end < start ? null : end;
    } else {
      this._range = false;
      this._start = date === null || date === undefined ? null : iso(...parse(date, "state.set_date(date)"));
      this._end = null;
    }
    if (this._start !== null) [this._year, this._month] = parse(this._start, "state.set_date(date)");
    this._changed();
  }

  /** Mobile: the month on screen, `month` from 1 to 12. */
  set_month(year, month) {
    this.#live("state.set_month(year, month)");
    const y = Number(year);
    const m = Number(month);
    if (!Number.isInteger(y) || !Number.isInteger(m) || m < 1 || m > 12) throw new TypeError(`state.set_month(year, month): month is 1 to 12, got ${year}-${month}`);
    this._year = y;
    this._month = m;
    this._changed();
  }

  /** Mobile: `{year, month}` on screen. */
  month() {
    return { year: this._year, month: this._month };
  }

  /** gpui-component's disabled matcher: `(date) => boolean`, true for days that cannot be chosen; `null` clears it. */
  set_disabled_matcher(matcher) {
    this.#live("state.set_disabled_matcher(matcher)");
    if (matcher !== null && typeof matcher !== "function") throw new TypeError("state.set_disabled_matcher(matcher) expects a function or null");
    this._disabled = matcher;
    this._changed();
  }

  /**
   * Mobile: a dot under a day's number, `(date) => null | "default" | "tinted"`:
   * "default" in the secondary color (say, a day with events), "tinted" in the
   * tint (the reader's own plans). `null` clears it.
   */
  set_marker_matcher(matcher) {
    this.#live("state.set_marker_matcher(matcher)");
    if (matcher !== null && typeof matcher !== "function") throw new TypeError("state.set_marker_matcher(matcher) expects a function or null");
    this._marker = matcher;
    this._changed();
  }

  on(event, fn) {
    this.#live("state.on(event, fn)");
    if (event !== "change") throw new TypeError(`CalendarState.on(): unknown event ${JSON.stringify(event)}; use "change"`);
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

  _isDisabled(date) {
    return this._disabled !== null && !!this._disabled(date);
  }

  _markerOf(date) {
    const m = this._marker === null ? null : this._marker(date);
    if (m === null || m === undefined || m === false) return null;
    if (m !== "default" && m !== "tinted") throw new TypeError(`set_marker_matcher: answer null, "default" or "tinted", got ${JSON.stringify(m)}`);
    return m;
  }

  /** A tap on a day: single mode selects it; range mode starts a range, then ends it (or restarts it before the start). */
  _pick(date, cx) {
    if (this.#released) return;
    if (!this._range) this._start = date;
    else if (this._start === null || this._end !== null || date < this._start) {
      this._start = date;
      this._end = null;
    } else this._end = date;
    this._changed();
    const value = this.date();
    for (const fn of [...this._listeners]) fn(value, cx);
  }

  _shift(months) {
    const total = this._year * 12 + (this._month - 1) + months;
    this._year = Math.floor(total / 12);
    this._month = (total % 12) + 1;
    this._changed();
  }
}

/** gpui-component's `CalendarState()`: callable with or without `new`, and as `CalendarState.new()`. */
export function CalendarState() {
  return new CalendarStateObject();
}
CalendarState.new = CalendarState;

const CELL_KEY = chainKey([["relative"], ["flex"], ["flex_col"], ["items_center"], ["justify_center"], ["gap_0p5"], ["w", `${100 / 7}%`], ["min_h", "2.75rem"]]);
// The disc keeps to its column at large text sizes: 40 pt, or the column's width if that is less.
const DISC_KEY = chainKey([["relative"], ["flex"], ["items_center"], ["justify_center"], ["w_full"], ["max_w", "2.5rem"], ["aspect_ratio", 1], ["rounded_full"]]);
// Under the disc rather than in it, so a number at the largest text sizes never covers it.
const DOT_KEY = chainKey([["flex_none"], ["size", 6], ["rounded_full"]]);
const NAV_KEY = chainKey([["flex"], ["items_center"], ["justify_center"], ["size", "2.75rem"], ["rounded_full"], ["@active", [["bg", "muted"]]]]);

/**
 * gpui-component's `Calendar(state)`. `number_of_months(n)` stacks `n`
 * consecutive months; `first_day_of_week(day)` (0 Sunday … 6 Saturday,
 * gpui-component's `first_day_of_week`) overrides the locale's.
 */
export class Calendar extends Element {
  static new(state) {
    return new Calendar(state);
  }

  constructor(state) {
    super("view");
    if (!(state instanceof CalendarStateObject)) throw new TypeError("Calendar(state) expects a CalendarState");
    this._cal = state;
    state._shownBy = scope.owner;
    this._months = 1;
    this._firstDay = firstDayOfWeek(windowInfo().locale);
    this._tid = null;
    this._preset([["flex"], ["flex_col"], ["gap_4"]]);
    this._sync();
  }

  /** Mobile: the parts get ids too, `<id>-YYYY-MM-DD` for each day and `<id>-prev` / `<id>-next` for the month buttons. */
  test_id(id) {
    this._tid = String(id);
    super.test_id(id);
    return this._sync();
  }

  number_of_months(count) {
    const n = Number(count);
    if (!Number.isInteger(n) || n < 1) throw new TypeError(`Calendar.number_of_months(): a positive whole number, got ${JSON.stringify(count)}`);
    this._months = n;
    return this._sync();
  }

  first_day_of_week(day) {
    const d = Number(day);
    if (!Number.isInteger(d) || d < 0 || d > 6) throw new TypeError(`Calendar.first_day_of_week(): 0 (Sunday) to 6 (Saturday), got ${JSON.stringify(day)}`);
    this._firstDay = d;
    return this._sync();
  }

  _sync() {
    this._c = null;
    for (let i = 0; i < this._months; i++) {
      const total = this._cal._year * 12 + (this._cal._month - 1) + i;
      super.child(this._month(Math.floor(total / 12), (total % 12) + 1, i === 0));
    }
    return this;
  }

  _month(year, month, first) {
    const title = `${MONTHS[month - 1]} ${year}`;
    const header = h_flex()
      .justify_between()
      .pl_1()
      .child(div().role("header").text_size(TEXT.body).line_height(LEADING.body).font_semibold().text_color("foreground").child(title))
      .when(first, (el) =>
        el.child(
          h_flex()
            .child(this._nav("prev", "Previous month", "icons/chevron-left.svg", -1))
            .child(this._nav("next", "Next month", "icons/chevron-right.svg", 1)),
        ),
      );
    const days = Array.from({ length: 7 }, (_, i) => (this._firstDay + i) % 7);
    const narrow = windowInfo().font_scale >= 1.5;
    const heads = h_flex().children(
      days.map((d) =>
        div()
          .w(`${100 / 7}%`)
          .text_center()
          .text_size(TEXT.caption).line_height(LEADING.caption)
          .font_semibold()
          .text_color("muted_foreground")
          .accessibility_label(WEEKDAYS[d])
          // At the largest text sizes a column has room for one letter, as iOS's narrow weekday symbols.
          .child(WEEKDAYS[d].slice(0, narrow ? 1 : 3).toUpperCase()),
      ),
    );
    const lead = (weekday(year, month, 1) - this._firstDay + 7) % 7;
    const count = daysIn(year, month);
    const weeks = [];
    for (let start = 1 - lead; start <= count; start += 7) {
      weeks.push(h_flex().children(Array.from({ length: 7 }, (_, i) => this._day(year, month, start + i, count))));
    }
    return v_flex()
      .key(`month:${year}-${month}`)
      .gap_2()
      .accessibility_label(title)
      .child(header)
      .child(heads)
      .child(v_flex().children(weeks));
  }

  _nav(key, label, icon, step) {
    return div()
      .key(`nav:${key}`)
      ._preset(NAV_KEY)
      .role("button")
      .accessibility_label(label)
      .when(this._tid !== null, (el) => el.test_id(`${this._tid}-${key}`))
      .child(new IconElement(icon).color("primary"))
      .on_click((_e, cx) => {
        this._cal._shift(step);
        cx.notify();
      });
  }

  _day(year, month, day, count) {
    const cell = new Element("view")._preset(CELL_KEY);
    if (day < 1 || day > count) return cell.accessibility_hidden(true);
    const s = this._cal;
    const date = iso(year, month, day);
    const complete = s._range && s._start !== null && s._end !== null && s._start !== s._end;
    const isStart = date === s._start;
    const isEnd = s._range && date === s._end;
    const inside = complete && date > s._start && date < s._end;
    const disc = isStart || isEnd;
    const today = date === s._today;
    const disabled = s._isDisabled(date);
    const marker = s._markerOf(date);
    // The range's band runs behind the discs, from the middle of its first day to the middle of its last.
    if (complete && (inside || isStart || isEnd)) {
      const band = div().absolute().top(4).bottom(4).bg("accent");
      cell.child(isStart ? band.left("50%").right_0() : isEnd ? band.left_0().right("50%") : band.left_0().right_0());
    }
    const label = `${WEEKDAYS[weekday(year, month, day)]}, ${MONTHS[month - 1]} ${day}, ${year}${today ? ", today" : ""}${marker ? ", has events" : ""}`;
    cell
      .key(date)
      .role("button")
      .accessibility_label(label)
      .selected(disc || inside)
      .when(this._tid !== null, (el) => el.test_id(`${this._tid}-${date}`))
      .child(
        div()
          ._preset(DISC_KEY)
          .when(disc, (el) => el.bg("primary"))
          .when(!disc && !disabled, (el) => el.active((st) => st.bg("muted")))
          .child(
            div()
              .text_size(TEXT.body).line_height(LEADING.body)
              .text_color(disc ? "primary_foreground" : disabled ? "muted_foreground" : today ? "primary" : "foreground")
              .when(disc || today, (el) => el.font_semibold())
              .when(disabled, (el) => el.opacity(0.5))
              .child(String(day)),
          ),
      )
      // Every day keeps the dot's row once markers are in use, so the weeks stay aligned.
      .when(s._marker !== null, (el) =>
        el.child(div()._preset(DOT_KEY).when(marker !== null, (d) => d.bg(marker === "tinted" ? "primary" : "muted_foreground"))),
      );
    if (disabled) return cell.disabled(true);
    return cell.on_click((_e, cx) => {
      s._pick(date, cx);
      cx.notify();
    });
  }
}
