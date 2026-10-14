// Charts, in a health and money dashboard: steps per day as bars, a
// portfolio's value as an area, a month's spending as a donut, language
// skills as a radar, and a chart with nothing to show yet. A tap on a bar, a
// point or a category reads its value out; a second tap puts it away.

import { View, h_flex, v_flex, Tabs, Spinner } from "native-shell";
import { BarChart, AreaChart, PieChart, RadarChart } from "gpui-component";
import { label, page, section, textButton } from "../ui.js";
/** @import { Color, Context, Task } from "native-shell" */

const WEEK = [
  { label: "Mon", value: 8412 },
  { label: "Tue", value: 10230 },
  { label: "Wed", value: 6120 },
  { label: "Thu", value: 9875 },
  { label: "Fri", value: 12480 },
  { label: "Sat", value: 4310 },
  { label: "Sun", value: 7655 },
];

// Thirty days of steps, Sep 12 to Oct 11.
const MONTH = [
  6210, 7340, 9120, 8805, 11240, 5320, 4980, 7710, 8630, 10120, 9450, 12010, 6040, 5510, 8120, 9230, 7740, 10870, 11630,
  4890, 6620, 8412, 10230, 6120, 9875, 12480, 4310, 7655, 9020, 8350,
].map((value, i) => ({ label: i < 19 ? `Sep ${12 + i}` : `Oct ${i - 18}`, value }));

// A brokerage account's closing value on each trading day of October so far.
const PORTFOLIO = [
  11820.4, 11865.1, 11790.75, 11902.3, 11988.6, 11954.2, 12031.9, 12110.45, 12064.8, 11998.3, 12086.15, 12152.6,
  12240.1, 12195.35, 12271.9, 12330.55, 12286.2, 12352.75, 12410.3, 12368.9, 12425.6, 12480.32,
].map((value, i) => ({ label: `Oct ${i + 1}`, value }));

const SPENDING = [
  { label: "Rent and Bills", value: 1061.35 },
  { label: "Groceries", value: 412.5 },
  { label: "Dining Out", value: 286.4 },
  { label: "Shopping", value: 231.75 },
  { label: "Transport", value: 148.0 },
];

const SKILLS = [
  { label: "Listening", now: 82, before: 70 },
  { label: "Reading", now: 74, before: 68 },
  { label: "Writing", now: 58, before: 51 },
  { label: "Speaking", now: 66, before: 49 },
  { label: "Vocabulary", now: 90, before: 81 },
  { label: "Grammar", now: 71, before: 64 },
];

/** `12480.32` → `"12,480.32"`. @param {number} n */
function grouped(n, decimals = 0) {
  return n.toFixed(decimals).replace(/\B(?=(\d{3})+(?!\d))/g, ",");
}

/** @param {number} n */
const money = (n) => `$${grouped(n, 2)}`;

export default class ChartsPage extends View {
  /** @param {unknown} _props @param {Context} cx */
  init(_props, cx) {
    /** 0 the week, 1 the month. */
    this.range = 0;
    /** @type {"loading" | "ready"} */
    this.prices = "loading";
    /** @type {Task | null} */
    this.load = null;
    this.fetchPrices(cx);
  }

  release() {
    this.load?.cancel();
  }

  /** Stands in for a quote request. @param {Context} cx */
  fetchPrices(cx) {
    this.prices = "loading";
    this.load?.cancel();
    this.load = cx.timer.after(700, (cx) => {
      this.prices = "ready";
      cx.notify();
    });
    cx.notify();
  }

  /** @param {Context} cx */
  render(cx) {
    return page(cx, {
      title: "Charts",
      intro: "A health and money dashboard. Tap a bar, a point or a category to read its value; tap it again to put it away.",
      children: [this.steps(cx), this.portfolio(cx), this.spending(cx), this.skills(cx), this.sleep(cx)],
    });
  }

  /** @param {Context} cx */
  steps(cx) {
    const t = cx.theme();
    const rows = this.range === 0 ? WEEK : MONTH;
    const average = rows.reduce((sum, r) => sum + r.value, 0) / rows.length;
    return section(cx, {
      header: "Steps",
      rows: [
        v_flex()
          .gap_3()
          .p_4()
          .child(
            Tabs.new("steps-range")
              .test_id("steps-range")
              .items(["Week", "Month"])
              .selected(this.range)
              .on_change((i, cx) => {
                this.range = i;
                cx.notify();
              }),
          )
          .child(headline(cx, "Daily Average", grouped(average), "steps", this.range === 0 ? "Oct 5–11" : "Sep 12–Oct 11"))
          .child(
            new BarChart(() => rows)
              .test_id("steps-chart")
              .label(false)
              .value_axis(true)
              .value_tick_format((v) => (v === 0 ? "0" : `${v / 1000}k`))
              .value_format((v) => `${grouped(v)} steps`)
              .tick_margin(this.range === 0 ? 1 : 7)
              .fill(t.primary)
              .h(200),
          ),
      ],
    });
  }

  /** @param {Context} cx */
  portfolio(cx) {
    const t = cx.theme();
    const first = PORTFOLIO[0].value;
    const last = PORTFOLIO[PORTFOLIO.length - 1].value;
    const change = last - first;
    const values = PORTFOLIO.map((r) => r.value);
    // Pinned to round numbers around the month's range: from zero the line would lie flat.
    const low = Math.floor(Math.min(...values) / 250) * 250;
    const high = Math.ceil(Math.max(...values) / 250) * 250;
    const loading = this.prices === "loading";
    return section(cx, {
      header: "Portfolio",
      footer: "Brokerage account, closing value on each trading day.",
      rows: [
        v_flex()
          .gap_3()
          .p_4()
          .child(
            h_flex()
              .items_start()
              .gap_3()
              .child(
                v_flex()
                  .flex_1()
                  .min_w_0()
                  .child(headline(cx, "Value", money(last), null, `+${money(change)} (${grouped((change / first) * 100, 1)}%) this month`, t.success)),
              )
              .child(textButton(cx, "portfolio-refresh", "Refresh", { disabled: loading, on_click: (_e, cx) => this.fetchPrices(cx) })),
          )
          .child(
            loading
              ? h_flex()
                  .test_id("portfolio-loading")
                  .h(200)
                  .gap_2()
                  .items_center()
                  .justify_center()
                  .child(Spinner.new())
                  .child(label(cx, "subheadline", t.muted_foreground, "Loading prices…"))
              : new AreaChart(() => PORTFOLIO)
                  .test_id("portfolio-chart")
                  .y_domain(low, high)
                  .y_tick_count(4)
                  .y_axis(true)
                  .y_tick_format((v) => `$${grouped(v)}`)
                  .value_format(money)
                  .tick_margin(7)
                  .stroke(t.success)
                  .h(200),
          ),
      ],
    });
  }

  /** @param {Context} cx */
  spending(cx) {
    const total = SPENDING.reduce((sum, r) => sum + r.value, 0);
    return section(cx, {
      header: "Spending",
      footer: "September, by category. Tap a category to see its amount.",
      rows: [
        v_flex()
          .gap_3()
          .p_4()
          .child(headline(cx, "Total Spent", money(total), null, "4% less than August"))
          .child(
            new PieChart(() => SPENDING)
              .test_id("spending-chart")
              .inner_radius(64)
              .pad_angle(0.03)
              .labels(true)
              .value_format(money)
              .h(176),
          ),
      ],
    });
  }

  /** @param {Context} cx */
  skills(cx) {
    const t = cx.theme();
    return section(cx, {
      header: "Spanish",
      footer: "Skill scores from this month’s lessons, against last month’s.",
      rows: [
        v_flex()
          .gap_3()
          .p_4()
          .child(h_flex().gap_4().child(legend(cx, t.primary, "October")).child(legend(cx, t.muted_foreground, "September")))
          .child(
            new RadarChart(() => SKILLS)
              .test_id("skills-chart")
              .value((r) => r.before)
              .stroke(t.muted_foreground)
              .value((r) => r.now)
              .stroke(t.primary)
              .max_value(100)
              .value_format((v) => `${v} of 100`)
              .dot()
              .h(240),
          ),
      ],
    });
  }

  /** @param {Context} cx */
  sleep(cx) {
    return section(cx, {
      header: "Sleep",
      footer: "Nights you sleep with your watch on appear here the next morning.",
      rows: [
        v_flex()
          .gap_3()
          .p_4()
          .child(headline(cx, "Time Asleep", "—", null, "Wear your watch to bed to record sleep."))
          .child(new BarChart(() => []).test_id("sleep-chart").label(false).value_axis(true).h(140)),
      ],
    });
  }
}

/**
 * A chart's headline, as iOS Health writes it: caption, large value and unit, then context.
 * @param {Context} cx @param {string} caption @param {string} value @param {string | null} unit
 * @param {string} detail @param {Color} [detailColor]
 */
function headline(cx, caption, value, unit, detail, detailColor) {
  const t = cx.theme();
  return v_flex()
    .child(label(cx, "caption1", t.muted_foreground, caption.toUpperCase()).font_medium())
    .child(
      h_flex()
        .items_end()
        .gap_1()
        .child(label(cx, "title1", t.foreground, value).font_semibold())
        .when(Boolean(unit), (el) => el.child(label(cx, "subheadline", t.muted_foreground, unit ?? "").pb_1())),
    )
    .child(label(cx, "subheadline", detailColor ?? t.muted_foreground, detail));
}

/** @param {Context} cx @param {Color} color @param {string} text */
function legend(cx, color, text) {
  const t = cx.theme();
  return h_flex()
    .gap_1p5()
    .items_center()
    .child(h_flex().size_2p5().rounded_full().bg(color))
    .child(label(cx, "subheadline", t.foreground, text));
}
