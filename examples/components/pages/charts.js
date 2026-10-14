// Charts, in a health and money dashboard: steps per day as bars, a
// portfolio's value as an area, a month's spending as a donut, language
// skills as a radar, and a chart that has nothing to show yet. A tap on a bar,
// a point or a category reads its value out; a second tap puts it away.

import { View, div, h_flex, v_flex, Tabs, Spinner } from "native-shell";
import { BarChart, AreaChart, PieChart, RadarChart } from "gpui-component";
import { button, card, heading, page } from "../ui.js";

const WEEK = [
  { label: "Mon", value: 8412 },
  { label: "Tue", value: 10230 },
  { label: "Wed", value: 6120 },
  { label: "Thu", value: 9875 },
  { label: "Fri", value: 12480 },
  { label: "Sat", value: 4310 },
  { label: "Sun", value: 7655 },
];

// Thirty days of steps, ending today (Sunday, October 11).
const MONTH = [
  6210, 7340, 9120, 8805, 11240, 5320, 4980, 7710, 8630, 10120, 9450, 12010, 6040, 5510, 8120, 9230, 7740, 10870, 11630,
  4890, 6620, 8412, 10230, 6120, 9875, 12480, 4310, 7655, 9020, 8350,
].map((value, i) => ({ label: dayLabel(i), value }));

// A portfolio's closing value over the last month.
const PORTFOLIO = [
  11820.4, 11865.1, 11790.75, 11902.3, 11988.6, 11954.2, 12031.9, 12110.45, 12064.8, 11998.3, 12086.15, 12152.6,
  12240.1, 12195.35, 12271.9, 12330.55, 12286.2, 12352.75, 12410.3, 12368.9, 12425.6, 12480.32,
].map((value, i) => ({ label: `Oct ${i + 1}`, value }));

const SPENDING = [
  { label: "Rent and bills", value: 1061.35 },
  { label: "Groceries", value: 412.5 },
  { label: "Dining out", value: 286.4 },
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

function dayLabel(i) {
  // Sep 12 … Oct 11.
  const day = 12 + i;
  return day <= 30 ? `Sep ${day}` : `Oct ${day - 30}`;
}

/** `12480.32` → `"12,480.32"`. */
function grouped(n, decimals = 0) {
  return n.toFixed(decimals).replace(/\B(?=(\d{3})+(?!\d))/g, ",");
}

const money = (n) => `$${grouped(n, 2)}`;

export default class ChartsPage extends View {
  init(_props, cx) {
    this.range = 0; // 0 week, 1 month
    /** "loading" | "ready" */
    this.prices = "loading";
    this.load = null;
    this.fetchPrices(cx);
  }

  release() {
    this.load?.cancel();
  }

  // Stands in for a quote request.
  fetchPrices(cx) {
    this.prices = "loading";
    this.load?.cancel();
    this.load = cx.timer.after(700, (cx) => {
      this.prices = "ready";
      cx.notify();
    });
    cx.notify();
  }

  render(cx) {
    return page(cx, {
      title: "Charts",
      intro: "A health and money dashboard. Tap a bar, a point or a category to read its value; tap it again to put it away.",
      children: [this.steps(cx), this.portfolio(cx), this.spending(cx), this.skills(cx), this.sleep(cx)],
    });
  }

  steps(cx) {
    const t = cx.theme();
    const rows = this.range === 0 ? WEEK : MONTH;
    const average = rows.reduce((sum, r) => sum + r.value, 0) / rows.length;
    return v_flex()
      .gap_3()
      .child(heading(cx, "Steps"))
      .child(
        card(cx)
          .p_4()
          .gap_3()
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
          .child(summary(cx, "Daily average", grouped(average), "steps", this.range === 0 ? "Oct 5–11" : "Sep 12–Oct 11"))
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
      );
  }

  portfolio(cx) {
    const t = cx.theme();
    const first = PORTFOLIO[0].value;
    const last = PORTFOLIO[PORTFOLIO.length - 1].value;
    const change = last - first;
    const values = PORTFOLIO.map((r) => r.value);
    // Pinned to round numbers around the month's range: from zero the line would lie flat.
    const low = Math.floor(Math.min(...values) / 250) * 250;
    const high = Math.ceil(Math.max(...values) / 250) * 250;
    const body =
      this.prices === "loading"
        ? h_flex()
            .test_id("portfolio-loading")
            .h(200)
            .gap_2()
            .items_center()
            .justify_center()
            .child(Spinner.new())
            .child(div().text_sm().text_color(t.muted_foreground).child("Loading prices…"))
        : new AreaChart(() => PORTFOLIO)
            .test_id("portfolio-chart")
            .y_domain(low, high)
            .y_tick_count(4)
            .y_axis(true)
            .y_tick_format((v) => `$${grouped(v)}`)
            .value_format(money)
            .tick_margin(7)
            .stroke(t.success)
            .h(200);
    return v_flex()
      .gap_3()
      .child(heading(cx, "Portfolio", "Brokerage account, closing value each trading day."))
      .child(
        card(cx)
          .p_4()
          .gap_3()
          .child(
            h_flex()
              .items_start()
              .gap_3()
              .child(
                div()
                  .flex_1()
                  .min_w_0()
                  .child(summary(cx, "Value", money(last), null, `+${money(change)} (${grouped((change / first) * 100, 1)}%) this month`, t.success)),
              )
              .child(
                button(cx, "portfolio-refresh", "Refresh", {
                  variant: "ghost",
                  loading: this.prices === "loading",
                  on_click: (_e, cx) => this.fetchPrices(cx),
                }),
              ),
          )
          .child(body),
      );
  }

  spending(cx) {
    const total = SPENDING.reduce((sum, r) => sum + r.value, 0);
    return v_flex()
      .gap_3()
      .child(heading(cx, "Spending", "September, by category. Tap a category to see its amount."))
      .child(
        card(cx)
          .p_4()
          .gap_3()
          .child(summary(cx, "Total spent", money(total), null, "4% less than August"))
          .child(
            new PieChart(() => SPENDING)
              .test_id("spending-chart")
              .inner_radius(64)
              .pad_angle(0.03)
              .labels(true)
              .value_format(money)
              .h(176),
          ),
      );
  }

  skills(cx) {
    const t = cx.theme();
    return v_flex()
      .gap_3()
      .child(heading(cx, "Spanish", "Skill scores from this month’s lessons, against last month."))
      .child(
        card(cx)
          .p_4()
          .gap_3()
          .child(
            h_flex()
              .gap_4()
              .child(legend(cx, t.primary, "October"))
              .child(legend(cx, t.muted_foreground, "September")),
          )
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
      );
  }

  sleep(cx) {
    const t = cx.theme();
    return v_flex()
      .gap_3()
      .child(heading(cx, "Sleep"))
      .child(
        card(cx)
          .p_4()
          .gap_3()
          .child(summary(cx, "Time asleep", "—", null, "Wear your watch to bed to record sleep."))
          .child(new BarChart(() => []).test_id("sleep-chart").label(false).value_axis(true).h(140))
          .child(div().text_sm().text_color(t.muted_foreground).child("Nights you sleep with your watch on appear here the next morning.")),
      );
  }
}

/** A chart's headline, as iOS Health writes it: caption, large value and unit, then context. */
function summary(cx, caption, value, unit, detail, detailColor) {
  const t = cx.theme();
  return v_flex()
    .gap_0p5()
    .child(div().text_xs().font_medium().text_color(t.muted_foreground).child(caption.toUpperCase()))
    .child(
      h_flex()
        .items_end()
        .gap_1()
        .child(div().text_2xl().font_semibold().text_color(t.foreground).child(value))
        .when(Boolean(unit), (el) => el.child(div().pb_0p5().text_sm().text_color(t.muted_foreground).child(unit))),
    )
    .when(Boolean(detail), (el) => el.child(div().text_sm().text_color(detailColor ?? t.muted_foreground).child(detail)));
}

function legend(cx, color, label) {
  const t = cx.theme();
  return h_flex()
    .gap_1p5()
    .items_center()
    .child(div().size_2p5().rounded_full().bg(color))
    .child(div().text_sm().text_color(t.foreground).child(label));
}
