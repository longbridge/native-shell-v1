//! Every component's native kind, props and events.

use native_shell::core::protocol::{Prop, PropValue, ViewType};
use native_shell::core::theme::{Appearance, Theme};
use native_shell::testing::Harness;

const EVERY_COMPONENT: &str = r##"
import { View, div, v_flex, h_flex, text, Button, Switch, Checkbox, Radio, Slider, Progress, Spinner, Tabs, Select,
  DatePicker, image, shape } from "native-shell";
export default class C extends View {
  init() {
    this.log = [];
    this.on = false; this.checked = false; this.radio = false; this.value = 0.25; this.tab = 0; this.choice = "b";
    this.date = "2026-01-01";
  }
  note(s, cx) { this.log.push(s); cx.notify(); }
  render(cx) {
    return v_flex().size_full().gap_1()
      .child(text(this.log.join(",")).test_id("log"))
      .child(Button.new("primary").test_id("b1").variant("primary").label("Go").on_click((e, cx) => this.note(`click${e.click_count}`, cx)))
      .child(Button.new("off").test_id("b2").label("Off").disabled(true).on_click((_e, cx) => this.note("never", cx)))
      .child(Button.new("o").test_id("b3").variant("outline").label("Outline"))
      .child(Switch.new("s").test_id("switch").checked(this.on).on_change((v, cx) => { this.on = v; this.note(`switch${v}`, cx); }))
      .child(Checkbox.new("c").test_id("check").label("Agree").checked(this.checked).on_change((v, cx) => { this.checked = v; this.note(`check${v}`, cx); }))
      .child(Radio.new("r").test_id("radio").checked(this.radio).on_change((v, cx) => { this.radio = v; this.note(`radio${v}`, cx); }))
      .child(Slider.new("sl").test_id("slider").min(0).max(1).step(0.05).value(this.value).on_change((v, cx) => { this.value = v; this.note(`slide${v}`, cx); }))
      .child(Progress.new().test_id("progress").value(this.value * 100))
      .child(Spinner.new().test_id("spinner").size_6())
      .child(Tabs.new("t").test_id("tabs").items(["One", "Two", "Three"]).selected(this.tab).on_change((i, cx) => { this.tab = i; this.note(`tab${i}`, cx); }))
      .child(Select.new("sel").test_id("select").placeholder("Pick").items([{ label: "A", value: "a" }, { label: "B", value: "b" }]).value(this.choice).on_change((v, cx) => { this.choice = v; this.note(`select${v}`, cx); }))
      .child(DatePicker.new("d").test_id("date").mode("date").value(this.date).on_change((v, cx) => { this.date = v; this.note(`date${v}`, cx); }))
      .child(image("assets/logo.png").test_id("image").size_8().object_fit("contain").tint("primary"))
      .child(shape("M0 0L10 10", { view_box: "0 0 10 10" }).test_id("shape").size_4().fill("primary").stroke("#ff0000", 2))
      .child(div().test_id("press").child("press").on_long_press((_e, cx) => this.note("long", cx)).on_double_click((_e, cx) => this.note("double", cx)));
  }
}
"##;

fn log(h: &Harness) -> String {
    h.text_of("log")
}

#[test]
fn components_map_to_native_kinds_with_props() {
    let h = Harness::new(&[("main.js", EVERY_COMPONENT)]);
    assert!(h.load_error().is_none(), "{:?} {:?}", h.load_error(), h.logs());
    assert_eq!(h.view_type("switch"), ViewType::Switch);
    assert_eq!(h.view_type("check"), ViewType::Checkbox);
    assert_eq!(h.prop("radio", Prop::CheckStyle), Some(PropValue::I32(1)));
    assert_eq!(h.view_type("slider"), ViewType::Slider);
    assert_eq!(h.prop("slider", Prop::Value), Some(PropValue::F32(0.25)));
    assert_eq!(h.prop("slider", Prop::Step), Some(PropValue::F32(0.05)));
    assert_eq!(h.view_type("progress"), ViewType::Progress);
    assert_eq!(h.view_type("spinner"), ViewType::Spinner);
    assert_eq!(h.view_type("tabs"), ViewType::Segmented);
    assert_eq!(h.prop("tabs", Prop::Items), Some(PropValue::Json(r#"["One","Two","Three"]"#.into())));
    assert_eq!(h.view_type("select"), ViewType::Select);
    assert_eq!(h.prop("select", Prop::SelectedIndex), Some(PropValue::I32(1)));
    assert_eq!(h.view_type("date"), ViewType::DatePicker);
    assert_eq!(h.prop("date", Prop::Date), Some(PropValue::Str("2026-01-01".into())));
    assert_eq!(h.view_type("image"), ViewType::Image);
    assert_eq!(h.prop("image", Prop::ContentMode), Some(PropValue::I32(1)));
    assert_eq!(h.view_type("shape"), ViewType::Shape);
    assert_eq!(h.prop("shape", Prop::ViewBox), Some(PropValue::F32Array(vec![0.0, 0.0, 10.0, 10.0])));
    assert_eq!(h.prop("shape", Prop::StrokeColor), Some(PropValue::Color(0xFFFF_0000)));
    // A labeled checkbox is a row of the control and its label.
    let check = h.find("check").unwrap();
    assert!(h.texts().contains(&"Agree".to_string()), "{}", h.dump());
    assert!(h.native().nodes[&check].frame.is_some());
    // The primary button paints with the theme's primary color.
    let primary = Theme::default_for(Appearance::Light).color("primary").unwrap();
    assert_eq!(h.prop("b1", Prop::BackgroundColor), Some(PropValue::Color(primary)));
    assert!(h.prop("b1", Prop::ActiveOpacity).is_some(), "active styles go native");
    assert_eq!(h.prop("b2", Prop::A11yState), Some(PropValue::Json(r#"{"disabled":true}"#.into())));
}

#[test]
fn component_events_deliver_the_documented_arguments() {
    let mut h = Harness::new(&[("main.js", EVERY_COMPONENT)]);
    h.tap("b1");
    assert_eq!(log(&h), "click1");
    h.tap("b2");
    assert_eq!(log(&h), "click1", "a disabled button has no handler");
    h.toggle("switch");
    assert_eq!(h.prop("switch", Prop::Checked), Some(PropValue::Bool(true)));
    h.toggle("check");
    h.toggle("radio");
    h.select("tabs", 2);
    h.select("select", 0);
    h.pick_date("date", "2026-10-14");
    h.long_press("press");
    let node = h.find("press").unwrap();
    h.dispatch(node, native_shell::core::protocol::EventType::DoubleClick, "{}");
    assert_eq!(log(&h), "click1,switchtrue,checktrue,radiotrue,tab2,selecta,date2026-10-14,long,double");
    assert_eq!(h.prop("tabs", Prop::SelectedIndex), Some(PropValue::I32(2)));
    assert_eq!(h.prop("select", Prop::SelectedIndex), Some(PropValue::I32(0)));
}

#[test]
fn slider_drags_are_coalesced_and_the_final_value_always_arrives() {
    let mut h = Harness::new(&[("main.js", EVERY_COMPONENT)]);
    let before = h.instance().metrics().events_coalesced;
    h.slide("slider", &[0.3, 0.4, 0.5, 0.6]);
    // Three dragging values coalesce to the latest, then the final one is delivered.
    assert_eq!(log(&h), "slide0.5,slide0.6");
    assert!(h.instance().metrics().events_coalesced >= before + 2);
    assert_eq!(h.prop("progress", Prop::Value), Some(PropValue::F32(0.6)));
}

#[test]
fn scroll_events_are_coalesced_until_the_next_tick() {
    let mut h = Harness::new(&[(
        "main.js",
        r#"
import { View, div, v_flex } from "native-shell";
export default class S extends View {
  init() { this.seen = []; }
  render(cx) {
    return v_flex().size_full()
      .child(div().test_id("log").child(this.seen.join(",")))
      .child(div().test_id("scroller").overflow_y_scroll().h_40()
        .on_scroll((e, cx) => { this.seen.push(e.y); cx.notify(); })
        .child(div().h_96()));
  }
}
"#,
    )]);
    let node = h.find("scroller").unwrap();
    for y in [1, 2, 3, 4] {
        h.dispatch_raw(node, native_shell::core::protocol::EventType::Scroll, &format!(r#"{{"x":0,"y":{y}}}"#));
    }
    h.settle();
    assert_eq!(h.text_of("log"), "4");
    assert_eq!(h.view_type("scroller"), ViewType::Scroll);
}

const FORMS: &str = r#"
import { View, div, v_flex, Checkbox, Switch, Progress } from "native-shell";
export default class F extends View {
  init() { this.done = false; }
  render() {
    const mark = div().child("✓");
    return v_flex()
      .child(Checkbox.new("styled").test_id("styled").checked(this.done).size_6()
        .on_change((v, cx) => { this.done = v; cx.notify(); })
        .when(this.done, (el) => el.child(mark)))
      .child(Checkbox.new("bare").test_id("bare").checked(this.done).when(this.done, (el) => el.child("✓")))
      .child(Switch.new("explicit").test_id("explicit").headless())
      .child(Checkbox.new("native").test_id("native").label("Native").checked(this.done).disabled(false))
      .child(Progress.new("drawn").test_id("drawn").value(40).h_2())
      .child(Progress.new("bar").test_id("bar").value(40));
  }
}
"#;

#[test]
fn a_control_is_drawn_or_native_by_how_it_is_built_not_by_this_renders_children() {
    let mut h = Harness::build(&[("main.js", FORMS)]).dev(true).start();
    assert_eq!(h.view_type("styled"), ViewType::View, "a style makes it drawn even with no children yet");
    assert_eq!(h.view_type("explicit"), ViewType::View, ".headless() makes it drawn");
    assert_eq!(h.view_type("drawn"), ViewType::View);
    assert_eq!(h.view_type("bar"), ViewType::Progress);
    assert_eq!(h.view_type("bare"), ViewType::Checkbox);
    assert_eq!(h.view_type("native"), ViewType::Checkbox, "label, checked and disabled keep it native");

    let styled = h.find("styled").unwrap();
    h.tap("styled");
    assert_eq!(h.find("styled"), Some(styled), "the drawn checkbox keeps its node when its check mark appears");
    assert_eq!(h.text_of("styled"), "✓");
    assert_eq!(h.view_type("bare"), ViewType::View, "the bare one gained a child, so it is drawn now");
    let warnings = h
        .logs()
        .into_iter()
        .filter(|l| l.contains("changed from its native form to the drawn one"))
        .collect::<Vec<_>>();
    assert_eq!(warnings.len(), 1, "dev mode names the flip once: {warnings:?}");
    assert!(warnings[0].contains("`bare`"), "{warnings:?}");
}
