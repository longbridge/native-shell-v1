// The range picker and the chart under it, shared by the overview and a
// holding's page so both read the same way.
//
// The chart is a picture of numbers the screen already states, so it carries
// one spoken summary instead of a point-by-point reading, and the caption
// under it says what the line means in words: how much the range gained or
// lost.

import { div, h_flex, v_flex, Tabs } from "native-shell";
import { AreaChart } from "gpui-component";
import { RANGES } from "./market.js";
import { money, signedMoney, signedPercent } from "./portfolio.js";
import { changeColor } from "./ui.js";

const SPANS = {
  "1D": "Today",
  "1W": "Past week",
  "1M": "Past month",
  "1Y": "Past year",
};

/** How a range's first and last values compare. */
export function rangeChange(rows) {
  const first = rows[0]?.value ?? 0;
  const last = rows[rows.length - 1]?.value ?? 0;
  return {
    change: last - first,
    percent: first > 0 ? ((last - first) / first) * 100 : 0,
  };
}

/**
 * @param {import("native-shell").Context} cx
 * @param {{ id: string, title: string, rows: { label: string, value: number }[], range: string, on_range: (range: string, cx: import("native-shell").Context) => void }} opts
 */
export function rangeChart(cx, { id, title, rows, range, on_range }) {
  const t = cx.theme();
  const { change, percent } = rangeChange(rows);
  const color = changeColor(t, change);
  const caption = `${signedMoney(change)} (${signedPercent(percent)})`;
  // The page already states today's change above the chart; under it, the
  // day's baseline is the useful number.
  const today = range === "1D";
  // Prices are read against each other, not against zero: the axis spans the
  // range's own low and high, with a little room above and below.
  const values = rows.map((r) => r.value);
  const [lo, hi] = [Math.min(...values), Math.max(...values)];
  const pad = Math.max((hi - lo) * 0.12, hi * 0.002);
  return v_flex()
    .gap_3()
    .child(
      Tabs.new(`${id}-range`)
        .test_id(`${id}-range`)
        .accessibility_label("Chart range")
        .items(RANGES)
        .selected(RANGES.indexOf(range))
        .on_change((i, cx) => {
          cx.haptic("selection");
          on_range(RANGES[i], cx);
        }),
    )
    .child(
      v_flex()
        .h(184)
        .child(
          AreaChart.new(() => rows)
            .test_id(`${id}-chart`)
            .accessibility_label(`${title}, ${SPANS[range].toLowerCase()}: ${change < 0 ? "down" : "up"} ${caption}`)
            .linear()
            .grid(false)
            .x_axis(false)
            .y_domain(lo - pad, hi + pad)
            .stroke(color)
            .value_format(money)
            .size_full(),
        ),
    )
    .child(
      h_flex()
        .test_id(`${id}-caption`)
        .gap_2()
        .child(
          div()
            .text_sm()
            .text_color(t.muted_foreground)
            .child(today ? "Previous close" : SPANS[range]),
        )
        .child(
          div()
            .test_id(`${id}-range-change`)
            .text_sm()
            .font_medium()
            .text_color(today ? t.foreground : color)
            .child(today ? money(rows[0]?.value ?? 0) : caption),
        ),
    );
}
