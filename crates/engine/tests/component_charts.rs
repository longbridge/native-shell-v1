//! "gpui-component" charts: BarChart, LineChart, AreaChart, PieChart and RadarChart.

use native_shell::core::host::Environment;
use native_shell::core::protocol::{EventType, Prop, PropValue, ViewType};
use native_shell::core::theme::{Appearance, Theme};
use native_shell::testing::Harness;

/// A program whose root renders `chart`, an expression in which `this.rows` holds `rows`.
fn chart(rows: &str, chart: &str) -> Harness {
    let main = format!(
        r#"
import {{ View, v_flex }} from "native-shell";
import {{ BarChart, LineChart, AreaChart, PieChart, RadarChart }} from "gpui-component";
export default class C extends View {{
  init() {{ this.rows = {rows}; }}
  render(cx) {{ return v_flex().size_full().child({chart}); }}
}}
"#
    );
    let h = Harness::new(&[("main.js", &main)]);
    assert!(h.load_error().is_none(), "{:?} {:?}", h.load_error(), h.logs());
    h
}

const DAYS: &str = r#"[{ label: "Mon", value: 42 }, { label: "Tue", value: 68 }, { label: "Wed", value: 31 }]"#;

fn str_prop(h: &Harness, node: u32, prop: Prop) -> Option<String> {
    match h.native().prop(node, prop) {
        Some(PropValue::Str(s)) => Some(s.clone()),
        _ => None,
    }
}

fn shapes(h: &Harness) -> Vec<u32> {
    let mut v = h.nodes_of(ViewType::Shape);
    v.sort_unstable();
    v
}

fn paths(h: &Harness) -> Vec<String> {
    shapes(h).into_iter().filter_map(|n| str_prop(h, n, Prop::PathData)).collect()
}

/// Nodes painted `color`, in creation order: the bars of a bar chart, the dots of a line.
fn painted(h: &Harness, color: u32) -> Vec<u32> {
    let mut v: Vec<u32> = h
        .native()
        .nodes
        .iter()
        .filter(|(_, n)| n.props.get(&Prop::BackgroundColor) == Some(&PropValue::Color(color)))
        .map(|(id, _)| *id)
        .collect();
    v.sort_unstable();
    v
}

fn labelled(h: &Harness, label: &str) -> bool {
    h.native().nodes.values().any(|n| n.props.get(&Prop::A11yLabel) == Some(&PropValue::Str(label.into())))
}

fn has_text(h: &Harness, text: &str) -> bool {
    h.texts().iter().any(|t| t == text)
}

fn color(appearance: Appearance, token: &str) -> u32 {
    Theme::default_for(appearance).color(token).unwrap()
}

/// The first series color.
fn primary() -> u32 {
    color(Appearance::Light, "chart_1")
}

fn frame(h: &Harness, node: u32) -> native_shell::core::protocol::Frame {
    h.native().nodes[&node].frame.unwrap()
}

fn near(a: f32, b: f32) -> bool {
    (a - b).abs() < 0.7
}

#[test]
fn bars_rise_from_zero_to_their_share_of_a_rounded_scale() {
    let h = chart(DAYS, "new BarChart(() => this.rows).label(false).label_axis(false).grid(false).w(300).h(100)");
    // 68 rounds the axis up to 80 (steps of 20): the bars are 42/80, 68/80 and 31/80 of the plot.
    let bars = painted(&h, primary());
    assert_eq!(bars.len(), 3, "{}", h.dump());
    let heights: Vec<f32> = bars.iter().map(|b| frame(&h, *b).height).collect();
    for (got, want) in heights.iter().zip([52.5, 85.0, 38.75]) {
        assert!(near(*got, want), "{heights:?}");
    }
    let tue = frame(&h, bars[1]);
    assert!(near(tue.y, 15.0) && near(tue.y + tue.height, 100.0), "{tue:?}");
    // Centered in equal bands, rounded at the tip.
    assert!(near(tue.x + tue.width / 2.0, 150.0), "{tue:?}");
    assert_eq!(h.native().prop(bars[1], Prop::CornerRadii), Some(&PropValue::F32Array(vec![4.0, 4.0, 0.0, 0.0])));
}

#[test]
fn bar_labels_write_each_value_as_gpui_shell_does() {
    let h = chart(DAYS, "new BarChart(() => this.rows)");
    for t in ["42", "68", "31", "Mon", "Tue", "Wed"] {
        assert!(has_text(&h, t), "{:?}", h.texts());
    }
    let h = chart(DAYS, "new BarChart(() => this.rows).label((r) => `${r.value} min`)");
    assert!(has_text(&h, "68 min"));
    let h = chart(DAYS, "new BarChart(() => this.rows).label(false)");
    assert!(!has_text(&h, "68"));
}

#[test]
fn value_axis_ticks_are_round_numbers_with_grouped_thousands() {
    let rows = r#"[8412, 10230, 6120, 9875, 12480, 4310, 7655].map((value, i) => ({ label: `d${i}`, value }))"#;
    let h = chart(rows, "new BarChart(() => this.rows).value_axis(true).label(false)");
    let texts = h.texts();
    for t in ["15,000", "10,000", "5,000", "0"] {
        assert!(texts.contains(&t.to_string()), "{texts:?}");
    }
    // Top to bottom on the trailing edge, a grid line at each.
    let pos = |t: &str| texts.iter().position(|x| x == t).unwrap();
    assert!(pos("15,000") < pos("10,000") && pos("10,000") < pos("0"));
    assert_eq!(painted(&h, Theme::default_for(Appearance::Light).color("border").unwrap()).len(), 1, "the zero line is solid");

    let h = chart(rows, "new BarChart(() => this.rows).value_axis(true).label(false).value_tick_format((v) => `${v / 1000}k`)");
    assert!(has_text(&h, "15k"), "{:?}", h.texts());
    // Fractional steps keep their decimals.
    let h = chart(r#"[{ label: "a", value: 0.9 }]"#, "new BarChart(() => this.rows).value_axis(true).label(false)");
    assert_eq!(h.texts()[1..], ["1", "0.5", "0"], "{:?}", h.texts());
}

#[test]
fn tick_margin_keeps_every_nth_category_label() {
    let rows = r#"Array.from({ length: 9 }, (_, i) => ({ label: `D${i}`, value: i }))"#;
    let h = chart(rows, "new BarChart(() => this.rows).label(false).tick_margin(4)");
    assert!(has_text(&h, "D0") && has_text(&h, "D4") && has_text(&h, "D8"));
    assert!(!has_text(&h, "D1") && !has_text(&h, "D7"), "{:?}", h.texts());
    let h = chart(rows, "new BarChart(() => this.rows).label(false).label_axis(false)");
    assert!(!has_text(&h, "D0"));
}

#[test]
fn negative_bars_hang_below_the_zero_line() {
    let rows = r#"[{ label: "Q1", value: 30 }, { label: "Q2", value: -20 }]"#;
    let h = chart(rows, "new BarChart(() => this.rows).label(false).label_axis(false).grid(false).w(200).h(90)");
    let bars = painted(&h, primary());
    let (up, down) = (frame(&h, bars[0]), frame(&h, bars[1]));
    // -20…40 in steps of 20: zero is two thirds of the way down.
    assert!(near(up.y + up.height, 60.0) && near(down.y, 60.0) && near(down.height, 30.0), "{up:?} {down:?}");
    assert_eq!(h.native().prop(bars[1], Prop::CornerRadii), Some(&PropValue::F32Array(vec![0.0, 0.0, 4.0, 4.0])));
}

#[test]
fn line_paths_stretch_to_the_plot_in_points() {
    let rows = r#"[{ label: "a", value: 0 }, { label: "b", value: 50 }, { label: "c", value: 100 }]"#;
    let h = chart(rows, "new LineChart(() => this.rows).linear().x_axis(false).grid(false).w(300).h(100)");
    let line = shapes(&h)[0];
    // A point in the middle of each third, in the plot's own points.
    assert_eq!(str_prop(&h, line, Prop::PathData).as_deref(), Some("M50 100L150 50L250 0"));
    assert_eq!(h.native().prop(line, Prop::ViewBox), Some(&PropValue::F32Array(vec![0.0, 0.0, 300.0, 100.0])));
    assert_eq!(h.native().prop(line, Prop::StrokeWidth), Some(&PropValue::F32(2.0)));

    let h = chart(rows, "new LineChart(() => this.rows).step_after().x_axis(false).grid(false).w(600).h(50)");
    assert_eq!(paths(&h)[0], "M100 50H300V25H500V0");
    // The default is gpui-component's natural curve: cubics through every point.
    let h = chart(rows, "new LineChart(() => this.rows).x_axis(false).grid(false).w(300).h(100)");
    let d = &paths(&h)[0];
    assert!(d.starts_with("M50 100C") && d.ends_with(" 250 0") && d.matches('C').count() == 2, "{d}");
}

#[test]
fn a_stretched_line_is_redrawn_when_its_plot_resizes() {
    let rows = r#"[{ label: "a", value: 0 }, { label: "b", value: 100 }]"#;
    let mut h = chart(rows, "new LineChart(() => this.rows).linear().x_axis(false).grid(false).w_full().h(100)");
    assert_eq!(paths(&h)[0], "M97.5 100L292.5 0");
    h.set_environment(Environment { width: 300.0, ..Environment::default() });
    assert_eq!(paths(&h)[0], "M75 100L225 0");
    assert_eq!(h.native().prop(shapes(&h)[0], Prop::ViewBox), Some(&PropValue::F32Array(vec![0.0, 0.0, 300.0, 100.0])));
}

#[test]
fn line_series_and_dots_take_the_palette_in_order() {
    let rows = r#"[{ label: "a", x: 1, y: 2 }, { label: "b", x: 3, y: 4 }]"#;
    let h = chart(rows, "new LineChart(() => this.rows).y((r) => r.x).y((r) => r.y).dot().x_axis(false)");
    let strokes: Vec<_> = shapes(&h).iter().map(|s| h.native().prop(*s, Prop::StrokeColor).cloned()).collect();
    assert_eq!(
        strokes,
        vec![Some(PropValue::Color(primary())), Some(PropValue::Color(color(Appearance::Light, "chart_2")))]
    );
    assert_eq!(painted(&h, color(Appearance::Light, "chart_2")).len(), 2, "a dot per point");
    // `stroke` colors the series it follows; token names resolve.
    let h = chart(rows, r#"new LineChart(() => this.rows).y((r) => r.x).stroke("warning")"#);
    assert_eq!(h.native().prop(shapes(&h)[0], Prop::StrokeColor), Some(&PropValue::Color(color(Appearance::Light, "warning"))));
}

#[test]
fn areas_close_on_the_baseline_and_stack() {
    let rows = r#"[{ label: "a", m: 10, d: 20 }, { label: "b", m: 30, d: 40 }]"#;
    let h = chart(rows, "new AreaChart(() => this.rows).y((r) => r.m).y((r) => r.d).linear().x_axis(false).grid(false).w(200).h(100)");
    // 30 + 40 stacks to 70: the axis runs to 80. Shapes: fill, line, fill, line.
    let p = paths(&h);
    assert_eq!(p.len(), 4, "{p:?}");
    assert_eq!(p[0], "M50 87.5L150 62.5L150 100L50 100Z");
    assert_eq!(p[1], "M50 87.5L150 62.5");
    // The second band sits on the first.
    assert_eq!(p[2], "M50 62.5L150 12.5L150 62.5L50 87.5Z");
    // Fills are the series color, faded.
    let fill = h.native().prop(shapes(&h)[0], Prop::FillColor).cloned();
    assert_eq!(fill, Some(PropValue::Color((0x38 << 24) | (primary() & 0xFF_FFFF))));
}

#[test]
fn y_domain_pins_the_axis_and_clamps_values_outside_it() {
    let rows = r#"[{ label: "a", value: 100 }, { label: "b", value: 150 }, { label: "c", value: 250 }]"#;
    let h = chart(
        rows,
        "new LineChart(() => this.rows).linear().y_domain(100, 200).y_axis(true).y_tick_count(3).x_axis(false).grid(false).w(300).h(100)",
    );
    for t in ["200", "150", "100"] {
        assert!(has_text(&h, t), "{:?}", h.texts());
    }
    // The plot is inset by half a label line top and bottom; 250 stops at its top edge.
    let d = &paths(&h)[0];
    assert!(d.ends_with(" 0"), "{d}");
}

#[test]
fn pie_slices_start_at_twelve_and_turn_clockwise() {
    let rows = r#"[{ label: "Rent", value: 50 }, { label: "Food", value: 50 }]"#;
    let h = chart(rows, "new PieChart(() => this.rows).h(100)");
    assert_eq!(paths(&h), vec!["M0 -100A100 100 0 0 1 0 100L0 0Z", "M0 100A100 100 0 0 1 0 -100L0 0Z"]);
    assert_eq!(h.native().prop(shapes(&h)[0], Prop::ViewBox), Some(&PropValue::F32Array(vec![-100.0, -100.0, 200.0, 200.0])));
    let fills: Vec<_> = shapes(&h).iter().map(|s| h.native().prop(*s, Prop::FillColor).cloned()).collect();
    assert_eq!(fills, vec![Some(PropValue::Color(primary())), Some(PropValue::Color(color(Appearance::Light, "chart_2")))]);
    // A donut's hole runs back counterclockwise; one slice is a whole ring.
    let h = chart(r#"[{ label: "All", value: 3 }]"#, "new PieChart(() => this.rows).inner_radius(50)");
    assert_eq!(paths(&h)[0], "M0 -100A100 100 0 1 1 0 100A100 100 0 1 1 0 -100ZM0 -50A50 50 0 1 0 0 50A50 50 0 1 0 0 -50Z");
    // Padding pulls each slice's edges off the shared boundary.
    let h = chart(rows, "new PieChart(() => this.rows).inner_radius(60).pad_angle(0.1)");
    assert_eq!(paths(&h)[0], "M5 -99.88A100 100 0 0 1 5 99.88L3 59.93A60 60 0 0 0 3 -59.93Z");
}

#[test]
fn pie_labels_list_shares_and_a_tap_selects_a_slice() {
    let rows = r#"[{ label: "Rent", value: 600 }, { label: "Food", value: 300 }, { label: "Fun", value: 100 }]"#;
    let mut h = chart(rows, r#"new PieChart(() => this.rows).test_id("pie").inner_radius(60).labels(true).value_format((v) => `$${v}`)"#);
    for t in ["Rent", "60%", "Food", "30%", "Fun", "10%"] {
        assert!(has_text(&h, t), "{:?}", h.texts());
    }
    assert!(labelled(&h, "Food, $300, 30%"));
    assert!(labelled(&h, "Pie chart, 3 values: Rent $600, 60%, Food $300, 30%, Fun $100, 10%"), "{}", h.dump());
    h.tap("pie-point-1");
    // The donut's middle reads the selected slice; the others fade.
    assert!(has_text(&h, "$300"), "{:?}", h.texts());
    assert_eq!(shapes(&h).iter().filter(|s| h.native().prop(**s, Prop::Opacity).is_some()).count(), 2);
    assert_eq!(h.prop("pie-point-1", Prop::A11yState), Some(PropValue::Json(r#"{"selected":true}"#.into())));
    h.tap("pie-point-1");
    assert!(!has_text(&h, "$300"));
}

#[test]
fn radar_draws_rings_spokes_and_a_polygon_per_series() {
    let rows = r#"[{ label: "A", v: 100, w: 50 }, { label: "B", v: 50, w: 50 }, { label: "C", v: 100, w: 50 }, { label: "D", v: 50, w: 50 }]"#;
    let h = chart(rows, "new RadarChart(() => this.rows).value((r) => r.v).value((r) => r.w).max_value(100).grid_levels(2)");
    let p = paths(&h);
    assert_eq!(p.len(), 3, "{p:?}");
    // Rings at half and full radius, then a spoke per axis.
    assert!(p[0].starts_with("M0 -0.5L0.5 0L0 0.5L-0.5 0ZM0 -1L1 0L0 1L-1 0Z"), "{}", p[0]);
    assert_eq!(p[0].matches("M0 0L").count(), 4);
    assert_eq!(p[1], "M0 -1L0.5 0L0 1L-0.5 0Z");
    assert_eq!(p[2], "M0 -0.5L0.5 0L0 0.5L-0.5 0Z");
    assert!(labelled(&h, "B, 50, 50"));
    for t in ["A", "B", "C", "D"] {
        assert!(has_text(&h, t));
    }
    // Without `max_value` the outer ring is the largest value, rounded up.
    let h = chart(r#"[{ label: "A", value: 70 }, { label: "B", value: 35 }, { label: "C", value: 70 }]"#, "new RadarChart(() => this.rows).grid(false)");
    assert!(paths(&h)[0].starts_with("M0 -0.7L"), "{:?}", paths(&h));
}

#[test]
fn a_tap_selects_a_point_and_a_second_tap_clears_it() {
    let mut h = chart(DAYS, r#"new BarChart(() => this.rows).test_id("bars").label(false).value_format((v) => `${v} min`)"#);
    assert!(labelled(&h, "Tue, 68 min"));
    assert!(labelled(&h, "Bar chart, 3 values: Mon 42 min, Tue 68 min, Wed 31 min"));
    h.tap("bars-point-1");
    assert!(has_text(&h, "68 min"), "{:?}", h.texts());
    assert_eq!(h.prop("bars-point-1", Prop::A11yState), Some(PropValue::Json(r#"{"selected":true}"#.into())));
    let faded = painted(&h, primary()).iter().filter(|b| h.native().prop(**b, Prop::Opacity).is_some()).count();
    assert_eq!(faded, 2, "the other bars fade");
    h.tap("bars-point-2");
    assert!(has_text(&h, "31 min") && !has_text(&h, "68 min"));
    h.tap("bars-point-2");
    assert!(!has_text(&h, "31 min"));

    // A line shows every series' value at the point.
    let rows = r#"[{ label: "a", m: 1, d: 2 }, { label: "b", m: 3, d: 4 }]"#;
    let mut h = chart(rows, r#"new LineChart(() => this.rows).test_id("line").y((r) => r.m).y((r) => r.d)"#);
    h.tap("line-point-1");
    assert!(has_text(&h, "3") && has_text(&h, "4"), "{:?}", h.texts());
}

#[test]
fn charts_of_one_view_keep_their_own_selections() {
    let main = r#"
import { View, v_flex } from "native-shell";
import { BarChart } from "gpui-component";
const rows = [{ label: "a", value: 1 }, { label: "b", value: 2 }];
export default class C extends View {
  render() {
    return v_flex()
      .child(new BarChart(() => rows).test_id("one").label(false).value_format((v) => `one ${v}`))
      .child(new BarChart(() => rows).test_id("two").label(false).value_format((v) => `two ${v}`));
  }
}
"#;
    let mut h = Harness::new(&[("main.js", main)]);
    h.tap("one-point-0");
    h.tap("two-point-1");
    assert!(has_text(&h, "one 1") && has_text(&h, "two 2"), "{:?}", h.texts());
}

#[test]
fn empty_data_draws_a_baseline_and_says_so() {
    for (kind, name) in [
        ("BarChart", "Bar chart"),
        ("LineChart", "Line chart"),
        ("AreaChart", "Area chart"),
        ("PieChart", "Pie chart"),
        ("RadarChart", "Radar chart"),
    ] {
        let h = chart("[]", &format!("new {kind}(() => this.rows).test_id(\"c\")"));
        assert!(has_text(&h, "No data"), "{kind}: {:?}", h.texts());
        assert!(labelled(&h, &format!("{name}, no data")), "{kind}");
    }
    // An empty plot shows no scale to read.
    let h = chart("[]", "new BarChart(() => this.rows).value_axis(true)");
    assert_eq!(h.texts(), vec!["No data".to_string()]);
    // Zeros still get an axis and bars of no height.
    let h = chart(r#"[{ label: "a", value: 0 }]"#, "new BarChart(() => this.rows).value_axis(true)");
    assert!(has_text(&h, "1") && has_text(&h, "0"), "{:?}", h.texts());
}

#[test]
fn bad_rows_show_gpui_shells_message() {
    for (rows, message) in [
        (r#"[{ label: "a", value: "x" }]"#, "chart row 0 must have finite number field `value`"),
        (r#"[{ label: "a", value: 1 }, { value: 1 }]"#, "chart row 1 must have string field `label`"),
        ("[3]", "chart row 0 must be a plain object"),
        (r#"[{ label: "a", value: Infinity }]"#, "chart row 0 must have finite number field `value`"),
    ] {
        let h = chart(rows, "new BarChart(() => this.rows)");
        assert!(has_text(&h, &format!("Failed to build BarChart data: {message}")), "{:?}", h.texts());
    }
    let h = chart(r#"[{ label: "a" }]"#, "new LineChart(() => this.rows).y((r) => r.missing)");
    assert!(has_text(&h, "Failed to build LineChart data: chart row 0: series 1 must be a finite number, got undefined"), "{:?}", h.texts());
}

#[test]
fn options_are_checked_with_gpui_shells_messages() {
    for (expr, message) in [
        (r#"new BarChart(() => this.rows).grid("yes")"#, "Chart.grid expects one boolean"),
        ("new BarChart(() => this.rows).tick_margin(0)", "tick_margin expects a positive integer"),
        ("new PieChart(() => this.rows).inner_radius(-1)", "inner_radius expects a non-negative finite number"),
        ("new RadarChart(() => this.rows).grid_levels(1.5)", "grid_levels expects a positive integer"),
        ("new BarChart(42)", "chart expects a row snapshot callback"),
        (r#"new BarChart(() => this.rows).child("x")"#, "charts do not accept children"),
    ] {
        let main = format!(
            r#"
import {{ View, v_flex }} from "native-shell";
import {{ BarChart, PieChart, RadarChart }} from "gpui-component";
export default class C extends View {{
  init() {{ this.rows = []; }}
  render() {{ return v_flex().child({expr}); }}
}}
"#
        );
        let h = Harness::new(&[("main.js", &main)]);
        let all = format!("{:?} {:?} {:?}", h.load_error(), h.error_logs(), h.texts());
        assert!(all.contains(message), "{expr}: {all}");
    }
}

#[test]
fn charts_follow_the_appearance() {
    let main = r#"
import { View } from "native-shell";
import { BarChart } from "gpui-component";
export default class C extends View {
  render() { return new BarChart(() => [{ label: "a", value: 1 }]).label(false); }
}
"#;
    let mut h = Harness::new(&[("main.js", main)]);
    assert_eq!(painted(&h, color(Appearance::Light, "chart_1")).len(), 1);
    h.set_environment(Environment { appearance: Appearance::Dark, ..Environment::default() });
    assert_eq!(painted(&h, color(Appearance::Dark, "chart_1")).len(), 1, "{}", h.dump());
}

#[test]
fn components_example_charts_page() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/components");
    let mut h = Harness::from_dir(&dir).unwrap().start();
    assert!(h.load_error().is_none(), "{:?} {:?}", h.load_error(), h.logs());
    h.tap("open-charts");
    // Prices load first; the chart replaces the spinner.
    assert!(h.find("portfolio-loading").is_some());
    h.advance(800.0);
    assert!(h.find("portfolio-loading").is_none() && h.find("portfolio-chart").is_some());
    // The week has 7 bars, the month 30.
    assert!(h.find("steps-chart-point-6").is_some() && h.find("steps-chart-point-7").is_none());
    let range = h.find("steps-range").unwrap();
    h.dispatch(range, EventType::Change, r#"{"index":1,"value":"Month"}"#);
    assert!(h.find("steps-chart-point-29").is_some());
    h.tap("steps-chart-point-25");
    assert!(has_text(&h, "12,480 steps"), "{:?}", h.texts());
    h.tap("portfolio-chart-point-21");
    assert!(has_text(&h, "$12,480.32") && has_text(&h, "Oct 22"));
    // A category reads its amount in the donut.
    h.tap("spending-chart-point-1");
    assert!(has_text(&h, "$412.50"));
    // Sleep has no data yet.
    assert!(has_text(&h, "No data"));
    assert!(labelled(&h, "Speaking, 49 of 100, 66 of 100"));
}

/// How many times a path or view box went to the renderer since the last clear.
fn path_sends(h: &Harness) -> usize {
    h.mutations()
        .iter()
        .filter(|m| matches!(m, native_shell::core::protocol::Mutation::SetProp { prop: Prop::PathData | Prop::ViewBox, .. }))
        .count()
}

#[test]
fn stretched_paths_are_sent_again_only_when_their_size_or_data_changes() {
    let main = r#"
import { View, v_flex } from "native-shell";
import { LineChart } from "gpui-component";
export default class C extends View {
  init() { this.rows = [{ label: "a", value: 1 }, { label: "b", value: 3 }]; this.color = "chart_1"; }
  render() {
    return v_flex().size_full().child(
      new LineChart(() => this.rows).test_id("line").stroke(this.color).x_axis(false).h(100),
    );
  }
}
"#;
    let mut h = Harness::new(&[("main.js", main)]);
    // A selection re-renders the view: the path is unchanged and is not sent.
    h.clear_mutations();
    h.tap("line-point-1");
    assert!(has_text(&h, "3"));
    assert_eq!(path_sends(&h), 0);
    // A new size sends it once, in that size's points.
    h.clear_mutations();
    h.set_environment(Environment { width: 300.0, ..Environment::default() });
    assert_eq!(path_sends(&h), 2, "path and view box");
    h.clear_mutations();
    h.set_environment(Environment { width: 300.0, ..Environment::default() });
    assert_eq!(path_sends(&h), 0);
}

#[test]
fn fill_mode_refuses_what_it_cannot_stretch() {
    for (path, message) in [
        ("M0 0A2 1 30 0 1 4 0", "a rotated arc (`A` with rotation 30) cannot stretch with fit"),
        ("M0 0X4 0", "unknown path command `X`"),
        ("M0 0L4 #", "cannot read the path near"),
    ] {
        let main = format!(
            r#"
import {{ View, shape }} from "native-shell";
export default class C extends View {{
  render() {{ return shape("{path}", {{ view_box: [0, 0, 4, 4], fit: "fill" }}); }}
}}
"#
        );
        let h = Harness::new(&[("main.js", &main)]);
        let all = format!("{:?} {:?} {:?}", h.load_error(), h.error_logs(), h.texts());
        assert!(all.contains(message), "{path}: {all}");
    }
    // Unrotated arcs and relative commands stretch exactly.
    let main = r#"
import { View, v_flex, shape } from "native-shell";
export default class C extends View {
  render() { return v_flex().child(shape("M0 2a2 2 0 0 1 4 0h-4z", { view_box: [0, 0, 4, 4], fit: "fill" }).test_id("s").w(8).h(4)); }
}
"#;
    let h = Harness::new(&[("main.js", main)]);
    assert_eq!(h.prop("s", Prop::PathData), Some(PropValue::Str("M0 2a4 2 0 0 1 8 0h-8z".into())));
}

#[test]
fn an_accessibility_label_replaces_the_summary_and_keeps_the_points() {
    let h = chart(DAYS, r#"new BarChart(() => this.rows).test_id("c").accessibility_label("Focus minutes, this week")"#);
    assert!(labelled(&h, "Focus minutes, this week"));
    assert!(!labelled(&h, "Bar chart, 3 values: Mon 42, Tue 68, Wed 31"));
    assert!(labelled(&h, "Tue, 68"));
    assert_eq!(h.prop("c", Prop::A11yLabel), None, "the root stays a container");
}
