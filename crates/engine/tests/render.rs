//! Rendering, invalidation, conversion and failure handling.

use native_shell::core::protocol::{Prop, PropValue};
use native_shell::testing::Harness;
use native_shell::Error;

fn app(main: &str) -> Harness {
    let h = Harness::new(&[("main.js", main)]);
    if let Some(e) = h.load_error() {
        panic!("load failed: {e}\nlogs: {:#?}", h.logs());
    }
    h
}

#[test]
fn render_notify_cycle() {
    let mut h = app(r#"
import { View, div, v_flex, Button } from "native-shell";
export default class Counter extends View {
  init() { this.n = 0; }
  render(cx) {
    return v_flex().size_full().p_4().gap_2()
      .child(div().test_id("label").child("Count: ").child(this.n))
      .child(Button.new("inc").test_id("inc").label("+1").on_click((_e, cx) => { this.n += 1; cx.notify(); cx.notify(); }));
  }
}
"#);
    assert_eq!(h.text_of("label"), "Count: 0");
    let renders = h.instance().metrics().renders;
    h.tap("inc");
    assert_eq!(h.text_of("label"), "Count: 1");
    // Repeated notify in one handler coalesces into one render.
    assert_eq!(h.instance().metrics().renders, renders + 1);
    h.tap("inc");
    h.tap("inc");
    assert_eq!(h.text_of("label"), "Count: 3");
}

#[test]
fn nested_entity_invalidation_only_reconverts_the_dirty_entity() {
    let mut h = app(r#"
import { View, div, v_flex } from "native-shell";
class Child extends View {
  init(props) { this.n = 0; this.name = props.name; }
  render(cx) {
    return div().test_id(this.name).on_click((_e, cx) => { this.n += 1; cx.notify(); }).child(`${this.name}: ${this.n}`);
  }
}
export default class Parent extends View {
  init(_p, cx) {
    this.a = cx.new(Child, { name: "a" });
    this.b = cx.new(Child, { name: "b" });
    this.renders = 0;
  }
  render(cx) {
    this.renders += 1;
    const filler = [];
    for (let i = 0; i < 50; i++) filler.push(div().child(`row ${i}`));
    return v_flex().child(div().test_id("parent").child(`parent ${this.renders}`)).child(this.a).child(this.b).children(filler);
  }
}
"#);
    assert_eq!(h.text_of("a"), "a: 0");
    let before = h.instance().metrics();
    h.tap("a");
    let after = h.instance().metrics();
    assert_eq!(h.text_of("a"), "a: 1");
    assert_eq!(h.text_of("b"), "b: 0");
    assert_eq!(h.text_of("parent"), "parent 1", "the parent did not re-render");
    assert_eq!(after.renders - before.renders, 1);
    assert!(
        after.converted_nodes - before.converted_nodes <= 2,
        "only the child's two nodes were converted: {}",
        after.converted_nodes - before.converted_nodes
    );
}

#[test]
fn text_children_merge_and_spans_keep_their_style() {
    let h = app(r#"
import { View, div, text, span, link } from "native-shell";
export default class T extends View {
  render() {
    return div().child(
      div().test_id("merged").child("a").child(1).child(span("b").font_bold()).child(link("https://x.dev", "x")),
    ).child(text(["t1 ", span("t2").italic()]).test_id("text"));
  }
}
"#);
    assert_eq!(h.text_of("merged"), "a1bx");
    let node = h.find("merged").unwrap();
    let text = h.native().children(node)[0];
    let Some(PropValue::Json(json)) = h.native().prop(text, Prop::AttributedText) else { panic!("no text") };
    let v: serde_json::Value = serde_json::from_str(json).unwrap();
    let runs = v["runs"].as_array().unwrap();
    assert_eq!(runs.len(), 4, "{json}");
    assert_eq!(runs[2]["weight"], 700);
    assert_eq!(runs[3]["url"], "https://x.dev");
    assert_eq!(h.native().prop(text, Prop::LinksEnabled), Some(&PropValue::Bool(true)));
    assert_eq!(h.text_of("text"), "t1 t2");
}

#[test]
fn unknown_style_methods_throw_at_the_call_site_with_a_hint() {
    let h = Harness::new(&[(
        "main.js",
        r#"
import { View, div } from "native-shell";
export default class T extends View {
  render() {
    return div().flex_coll();
  }
}
"#,
    )]);
    let Some(Error::FirstRender(e)) = h.load_error() else {
        panic!("expected a first render error, got {:?}", h.load_error())
    };
    assert!(e.message.contains("unknown style method `flex_coll`"), "{e}");
    assert!(e.message.contains("did you mean: flex_col"), "{e}");
    assert_eq!(e.file.as_deref(), Some("main.js"));
    assert_eq!(e.line, Some(5), "{e}");
    assert!(h.find("nsh-error-page").is_some(), "{}", h.dump());
    assert!(h.texts().iter().any(|t| t.contains("flex_coll")));
    assert!(!h.host().errors().is_empty());
}

#[test]
fn style_arity_and_values_are_checked() {
    let h = Harness::new(&[(
        "main.js",
        r#"
import { View, div } from "native-shell";
export default class T extends View { render() { return div().flex_col(1); } }
"#,
    )]);
    let Some(Error::FirstRender(e)) = h.load_error() else { panic!() };
    assert!(e.message.contains("`flex_col` takes no arguments"), "{e}");

    let h = Harness::new(&[(
        "main.js",
        r#"
import { View, div } from "native-shell";
export default class T extends View { render() { return div().bg("no-such-color"); } }
"#,
    )]);
    let Some(Error::FirstRender(e)) = h.load_error() else { panic!() };
    assert!(e.message.contains("no-such-color"), "{e}");
}

#[test]
fn a_failing_render_keeps_the_last_frame_and_shows_a_banner() {
    let mut h = app(r#"
import { View, div, v_flex } from "native-shell";
export default class T extends View {
  init() { this.broken = false; this.n = 0; }
  render(cx) {
    if (this.broken) throw new Error("boom");
    return v_flex().size_full()
      .child(div().test_id("break").child(`n=${this.n}`).on_click((_e, cx) => { this.broken = true; cx.notify(); }))
      .child(div().test_id("fix").child("fix").on_click((_e, cx) => { this.broken = false; this.n += 1; cx.notify(); }));
  }
}
"#);
    h.tap("break");
    assert_eq!(h.text_of("break"), "n=0", "the last good frame stays");
    let banner = h.text_of("nsh-error-banner");
    assert!(banner.contains("boom") && banner.contains("main.js:6"), "{banner}");
    // Handlers of the last good frame still work.
    h.tap("fix");
    assert_eq!(h.text_of("break"), "n=1");
    assert!(h.find("nsh-error-banner").is_none());
}

#[test]
fn notify_during_render_throws_naming_the_rule() {
    let h = Harness::new(&[(
        "main.js",
        r#"
import { View, div } from "native-shell";
export default class T extends View { render(cx) { cx.notify(); return div(); } }
"#,
    )]);
    let Some(Error::FirstRender(e)) = h.load_error() else { panic!() };
    assert!(e.message.contains("cx.notify() is not allowed during the `render` phase"), "{e}");
}

#[test]
fn state_and_overlays_are_refused_during_render() {
    for (code, expect) in [
        ("InputState.new({})", "InputState.new(...) cannot run during render; create state in init()"),
        ("cx.new(T, {})", "cx.new(Class, props) cannot run during render"),
        ("cx.open_sheet(() => div())", "is not allowed during the `render` phase; overlays may only be opened"),
        ("cx.navigator.push(T, {})", "is not allowed during the `render` phase"),
        ("window.push_toast({ title: 'x' })", "window.push_toast(options) is not allowed during the `render` phase"),
    ] {
        let src = format!(
            r#"
import {{ View, div, InputState }} from "native-shell";
export default class T extends View {{ render(cx) {{ {code}; return div(); }} }}
"#
        );
        let h = Harness::new(&[("main.js", &src)]);
        let Some(Error::FirstRender(e)) = h.load_error() else { panic!("{code} was allowed") };
        assert!(e.message.contains(expect), "{code}: {e}");
    }
}

#[test]
fn a_captured_cx_is_refused_later() {
    let mut h = app(r#"
import { View, div } from "native-shell";
export default class T extends View {
  render(cx) {
    return div().size_full()
      .child(div().test_id("keep").child("keep").on_click((_e, cx) => { this.kept = cx; }))
      .child(div().test_id("use").child("use").on_click(() => { this.kept.notify(); }));
  }
}
"#);
    h.tap("keep");
    h.tap("use");
    let logs = h.error_logs().join("\n");
    assert!(logs.contains("cx is no longer valid"), "{logs}");
}

#[test]
fn first_render_failure_shows_an_error_page_and_init_errors_too() {
    let h = Harness::new(&[(
        "main.js",
        r#"
import { View } from "native-shell";
export default class T extends View {
  init() { throw new TypeError("bad init"); }
}
"#,
    )]);
    let Some(Error::FirstRender(e)) = h.load_error() else { panic!("{:?}", h.load_error()) };
    assert!(e.message.contains("bad init") && e.line == Some(4), "{e}");
    assert!(h.text_of("nsh-error-page").contains("bad init"));
}

#[test]
fn the_entry_must_export_a_view() {
    let h = Harness::new(&[("main.js", "export default 42;")]);
    let Some(Error::FirstRender(e)) = h.load_error() else { panic!("{:?}", h.load_error()) };
    assert!(e.message.contains("export default"), "{e}");
}

#[test]
fn compile_errors_carry_file_and_line() {
    let h = Harness::new(&[
        ("main.js", "import { helper } from './lib/util.js';\nexport default helper;\n"),
        ("lib/util.js", "export const helper = 1;\nconst = ;\n"),
    ]);
    let Some(Error::Module(e)) = h.load_error() else { panic!("{:?}", h.load_error()) };
    assert_eq!(e.file.as_deref(), Some("lib/util.js"), "{e}");
    assert_eq!(e.line, Some(2), "{e}");
    assert!(e.message.contains("SyntaxError"), "{e}");
}

#[test]
fn entity_props_update_and_release() {
    let mut h = app(r#"
import { View, div, v_flex } from "native-shell";
class Label extends View {
  init(props) { this.text = props.text; }
  update(props) { this.text = props.text; }
  render() { return div().test_id("label").child(this.text); }
}
export default class T extends View {
  init(_p, cx) { this.label = cx.new(Label, { text: "one" }); this.shown = true; }
  render(cx) {
    return v_flex()
      .child(div().test_id("set").child("set").on_click(() => this.label.set_props({ text: "two" })))
      .child(div().test_id("drop").child("drop").on_click((_e, cx) => { this.label.release(); this.shown = false; cx.notify(); }))
      .when(this.shown, (el) => el.child(this.label));
  }
}
"#);
    assert_eq!(h.text_of("label"), "one");
    h.tap("set");
    assert_eq!(h.text_of("label"), "two");
    let entities = h.instance().metrics().entities;
    h.tap("drop");
    assert!(h.find("label").is_none());
    assert_eq!(h.instance().metrics().entities, entities - 1);
}
