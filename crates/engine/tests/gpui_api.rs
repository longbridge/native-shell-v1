//! gpui-shell's script surface, as gpui-shell programs use it.

use native_shell::core::host::{HostMethod, HostModuleDescriptor, HostRegistry, MethodKind};
use native_shell::core::protocol::{Command, Prop, PropValue, ViewType, ROOT_NODE};
use native_shell::testing::Harness;

fn app(main: &str) -> Harness {
    let h = Harness::new(&[("main.js", main)]);
    if let Some(e) = h.load_error() {
        panic!("{e}\n{:#?}", h.logs());
    }
    h
}

#[test]
fn window_overlays_toasts_and_measurements() {
    let mut h = app(r#"
import { View, div } from "gpui-kit";
import { v_flex, Button } from "gpui-base";
export default class W extends View {
  init() { this.log = []; }
  render(cx) {
    return v_flex()
      .child(div().test_id("log").child(this.log.join(",")))
      .child(Button.new("d").test_id("dialog").child("dialog").on_click(() => {
        const depth = window.open_dialog(() => div().test_id("body").child("hi"), { backdrop_dismissable: false });
        this.log.push(`depth${depth}`, `active${window.has_active_dialog()}`);
      }))
      .child(Button.new("c").test_id("close").child("close").on_click(() => {
        this.log.push(`closed${window.close_dialog()}`, `closed${window.close_dialog()}`);
      }))
      .child(Button.new("s").test_id("sheet").child("sheet").on_click(() => {
        window.open_sheet(() => div().test_id("one").child("one"));
        window.open_sheet(() => div().test_id("two").child("two"));
      }))
      .child(Button.new("t").test_id("toast").child("toast").on_click((_e, cx) => {
        this.log.push(window.push_toast({ title: "Saved", id: "save", timeout: null }), window.push_toast({ title: "x" }));
        cx.notify();
      }))
      .child(Button.new("r").test_id("retract").child("retract").on_click((_e, cx) => {
        this.log.push(`removed:${window.remove_toast("toast-1")},${window.remove_toast("save")} cleared:${window.clear_toasts()}`);
        cx.notify();
      }))
      .child(div().test_id("measure").child(`${window.viewport_size().width}x${window.viewport_size().height} ${window.appearance()} ${window.rem_size()}`));
  }
}
"#);
    assert_eq!(h.text_of("measure"), "390x844 light 16");
    h.tap("dialog");
    let (dialog, kind) = h.surfaces()[1];
    assert_eq!(kind, ViewType::Dialog);
    assert_eq!(h.native().prop(dialog, Prop::Dismissable), Some(&PropValue::Bool(false)));
    assert_eq!(h.text_of("log"), "", "opening changes the window, not the view");
    h.tap("close");
    assert_eq!(h.text_of("log"), "depth1,activetrue,closedtrue,closedfalse", "closing an overlay redraws its opener");
    h.tap("sheet");
    assert_eq!(h.surfaces().len(), 2, "at most one sheet: the second replaced the first");
    assert!(h.find("two").is_some() && h.find("one").is_none());
    h.clear_commands();
    h.tap("toast");
    assert!(h.text_of("log").ends_with("save,toast-1"));
    let toasts: Vec<_> = h.commands().iter().filter(|(n, c, _)| *n == ROOT_NODE && *c == Command::ShowToast).collect();
    assert_eq!(toasts[0].2, r#"{"id":"save","title":"Saved","level":"info","duration":0}"#);
    assert_eq!(toasts[1].2, r#"{"id":"toast-1","title":"x","level":"info","duration":3000}"#);

    // remove_toast answers whether it was still showing; clear_toasts how many.
    h.clear_commands();
    h.advance(5000.0);
    h.tap("retract");
    assert!(h.text_of("log").ends_with("removed:false,true cleared:0"), "{}", h.text_of("log"));
    let dismissals: Vec<_> =
        h.commands().iter().filter(|(_, c, _)| *c == Command::DismissToast).map(|(_, _, p)| p.clone()).collect();
    assert_eq!(dismissals, vec![r#"{"id":"save"}"#.to_string()]);
}

#[test]
fn context_entities_tasks_and_timers() {
    let mut h = app(r#"
import { View, div } from "gpui-kit";
import { v_flex } from "gpui-base";
class Child extends View {
  init() { this.n = 0; }
  render() { return div().test_id("child").child(`child ${this.n}`); }
}
export default class C extends View {
  init(_p, cx) {
    this.child = cx.new(Child);
    this.ticks = 0;
    this.task = cx.spawn(async (cx) => { await cx.sleep(100); this.ticks += 100; cx.notify(); });
    this.timer = cx.timer.every(10, (cx) => { this.ticks += 1; cx.notify(); });
  }
  render(cx) {
    return v_flex()
      .child(div().test_id("ticks").child(`${this.ticks} ${cx.phase()}`))
      .child(this.child)
      .child(div().test_id("bump").child("bump").on_click((_e, cx) => { this.child.view.n += 1; cx.notify(this.child); }))
      .child(div().test_id("stop").child("stop").on_click(() => {
        this.task.cancel(); this.timer.cancel();
        this.done = `${this.task.is_done()} ${this.timer.is_done()} ${this.child.release()} ${this.child.release()}`;
      }))
      .child(div().test_id("done").child(this.done ?? ""))
      .child(div().test_id("refresh").child("refresh").on_click((_e, cx) => cx.notify()));
  }
}
"#);
    assert_eq!(h.text_of("ticks"), "0 render");
    h.tap("bump");
    assert_eq!(h.text_of("child"), "child 1", "cx.notify(entity) re-renders that entity");
    h.advance(30.0);
    assert_eq!(h.text_of("ticks"), "3 render");
    h.tap("stop");
    h.advance(200.0);
    h.tap("refresh");
    assert!(h.text_of("ticks").starts_with('3'), "the cancelled task never resumed: {}", h.text_of("ticks"));
    assert_eq!(h.text_of("done"), "true true true false");
}

#[test]
fn input_and_slider_state_follow_gpui_base() {
    let mut h = app(r#"
import { View, div } from "gpui-kit";
import { v_flex, Input, InputState, Slider, SliderState, SliderTrack, SliderIndicator, SliderThumb } from "gpui-base";
export default class S extends View {
  init(_p, cx) {
    this.input = InputState.new({ placeholder: "Name", value: "Ada" });
    this.ok = this.input.on("change", (_e, cx) => cx.notify());
    this.volume = SliderState.new({ min: 0, max: 10, value: 5 });
    this.log = [];
    this.volume.on("change", (v, cx) => { this.log.push(`c${v}`); cx.notify(); });
    this.volume.on("release", (v, cx) => { this.log.push(`r${v}`); cx.notify(); });
  }
  render() {
    return v_flex()
      .child(div().test_id("value").child(`${this.input.value()} ${this.ok}`))
      .child(Input.new(this.input).test_id("input"))
      .child(Slider.new(this.volume).test_id("slider").child(SliderTrack.new(this.volume).child(SliderIndicator.new(this.volume).child(SliderThumb.new(this.volume)))))
      .child(div().test_id("log").child(`${this.volume.value()} ${this.log.join(",")}`));
  }
}
"#);
    assert_eq!(h.text_of("value"), "Ada true");
    h.type_text("input", "Grace");
    assert_eq!(h.text_of("value"), "Grace true");
    assert_eq!(h.prop("slider", Prop::Max), Some(PropValue::F32(10.0)));
    assert!(h.native().children(h.find("slider").unwrap()).is_empty(), "the parts are drawn natively");
    h.slide("slider", &[6.0, 7.0]);
    assert_eq!(h.text_of("log"), "7 c6,c7,r7");
}

#[test]
fn headless_toggles_progress_svg_and_element_helpers() {
    let mut h = app(r##"
import { View, div, svg, uniform_list } from "gpui-kit";
import { v_flex, Checkbox, Switch, Progress, ProgressTrack, ProgressIndicator, Tabs, Tab } from "gpui-base";
export default class H extends View {
  init() { this.on = false; this.tab = 0; }
  render(cx) {
    const mapped = div().map((el) => "mapped");
    return v_flex()
      .child(Checkbox.new("c").test_id("drawn").checked(this.on).on_change((v, cx) => { this.on = v; cx.notify(); })
        .child(div().size_4().when(this.on, (el) => el.bg("primary"))).child("Agree"))
      .child(Switch.new("s").test_id("native").checked(this.on))
      .child(Progress.new().test_id("bar").value(40).child(ProgressTrack.new().child(ProgressIndicator.new().w(40))))
      .child(Progress.new().test_id("native-bar").value(25))
      .child(svg("icons/check.svg").test_id("icon").size_4().text_color("#ff0000"))
      .child(svg("icons/list.svg").test_id("plain-icon").size_4())
      .child(Tabs.new("tabs").children([0, 1].map((i) => Tab.new(i).test_id(`tab${i}`).selected(this.tab === i).child(`T${i}`).on_click((_e, cx) => { this.tab = i; cx.notify(); }))))
      .child(div().test_id("map").child(mapped))
      .child(uniform_list("u", 3, (i) => `row-${i}`, (range) => [0, 1, 2].map((i) => div().test_id(`u${i}`).child(`u${i}`))).h_24());
  }
}
"##);
    assert_eq!(h.view_type("drawn"), ViewType::View, "with children, a drawn checkbox");
    assert_eq!(h.prop("drawn", Prop::A11yRole), Some(PropValue::I32(8)));
    h.tap("drawn");
    assert_eq!(h.prop("drawn", Prop::A11yState), Some(PropValue::Json(r#"{"checked":true}"#.into())));
    assert_eq!(h.prop("native", Prop::Checked), Some(PropValue::Bool(true)));
    assert_eq!(h.view_type("native"), ViewType::Switch);
    assert_eq!(h.view_type("bar"), ViewType::View);
    assert_eq!(h.prop("bar", Prop::A11yValue), Some(PropValue::Str("40%".into())));
    assert_eq!(h.prop("native-bar", Prop::Value), Some(PropValue::F32(0.25)));
    // No such files in this program: an empty box of the icon's size, and one warning each.
    assert_eq!(h.view_type("icon"), ViewType::View);
    assert_eq!(h.frame("icon").width, 16.0);
    assert!(
        h.logs().iter().any(|l| l.contains("svg(\"icons/list.svg\"): no such file in the program")),
        "{:?}",
        h.logs()
    );
    h.tap("tab1");
    assert_eq!(h.prop("tab1", Prop::A11yState), Some(PropValue::Json(r#"{"selected":true}"#.into())));
    assert_eq!(h.text_of("map"), "mapped", ".map returns what the transform returns");
    assert!(h.find("u2").is_some());
}

#[test]
fn desktop_only_methods_are_accepted_and_ignored() {
    let h = app(r#"
import { View, div } from "gpui-kit";
export default class D extends View {
  render(cx) {
    return div().test_id("d").on_hover(() => {}).tooltip("tip").key_context("Editor").tab_index(0).on_mouse_down("left", () => {})
      .child("ok");
  }
}
"#);
    assert_eq!(h.text_of("d"), "ok");
}

#[test]
fn reserved_module_names() {
    let h = Harness::new(&[("main.js", "import x from 'nsh/clipboard';\nexport default x;")]);
    assert!(h.load_error().unwrap().to_string().contains("reserved"), "{:?}", h.load_error());
    let registry = HostRegistry {
        modules: vec![HostModuleDescriptor {
            name: "nsh/evil".into(),
            methods: vec![HostMethod { name: "x".into(), kind: MethodKind::Sync, ts: None }],
            events: vec![],
            requires_permission: false,
            dts: None,
        }],
        components: vec![],
    };
    let h = Harness::with_registry(&[("main.js", "export default 1;")], registry);
    assert!(h.load_error().unwrap().to_string().contains("reserved"), "{:?}", h.load_error());
}

#[test]
fn the_gpui_shell_specifier_loads_and_exports_nothing() {
    // As in gpui-shell: the module exists, its names live in "gpui-kit".
    let h = app(r#"
import {} from "gpui-shell";
import * as shell from "gpui-shell";
import { View, div } from "gpui-kit";
export default class S extends View {
  render() { return div().test_id("n").child(String(Object.keys(shell).length)); }
}
"#);
    assert_eq!(h.text_of("n"), "0");

    let h = Harness::new(&[(
        "main.js",
        r#"import { View } from "gpui-shell";
export default class S extends View { render() { return null; } }"#,
    )]);
    let e = h.load_error().expect("a name from gpui-shell does not link").to_string();
    assert!(e.contains("View"), "{e}");
}
