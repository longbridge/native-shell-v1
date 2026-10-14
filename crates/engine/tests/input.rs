//! The controlled text input protocol: echo suppression, stale writes, IME.

use native_shell::core::protocol::{Command, Mutation, Prop, PropValue, ViewType};
use native_shell::testing::Harness;

const FORM: &str = r#"
import { View, div, v_flex, Input, InputState, Textarea, TextareaState, Button } from "native-shell";
export default class F extends View {
  init(_p, cx) {
    this.events = [];
    this.name = InputState.new({ placeholder: "Name", max_length: 20, keyboard: "email", return_key: "done" });
    this.name.on("change", (e, cx) => { this.events.push(`change:${e.text}:${e.composing}`); cx.notify(); });
    this.name.on("submit", (e, cx) => { this.events.push(`submit:${e.text}`); cx.notify(); });
    this.name.on("focus", (_e, cx) => { this.events.push("focus"); cx.notify(); });
    this.upper = InputState.new({ value: "seed" });
    this.upper.on("change", (e, cx) => {
      if (!e.composing && e.text !== e.text.toUpperCase()) this.upper.set_value(e.text.toUpperCase());
    });
    this.notes = TextareaState.new({ placeholder: "Notes" });
  }
  render(cx) {
    return v_flex().size_full().gap_2()
      .child(div().test_id("events").child(this.events.join("|")))
      .child(div().test_id("echo").child(this.name.value()))
      .child(Input.new(this.name).test_id("name"))
      .child(Input.new(this.upper).test_id("upper"))
      .child(Textarea.new(this.notes).test_id("notes"))
      .child(Button.new("set").test_id("set").label("set").on_click(() => this.name.set_value("from script")))
      .child(Button.new("focus").test_id("focus").label("focus").on_click(() => this.name.focus()));
  }
}
"#;

fn form() -> Harness {
    let h = Harness::new(&[("main.js", FORM)]);
    assert!(h.load_error().is_none(), "{:?}", h.load_error());
    h
}

#[test]
fn inputs_carry_their_configuration() {
    let h = form();
    assert_eq!(h.view_type("name"), ViewType::TextInput);
    assert_eq!(h.prop("name", Prop::Placeholder), Some(PropValue::Str("Name".into())));
    assert_eq!(h.prop("name", Prop::MaxLength), Some(PropValue::I32(20)));
    assert_eq!(h.prop("name", Prop::KeyboardType), Some(PropValue::I32(3)));
    assert_eq!(h.prop("name", Prop::ReturnKey), Some(PropValue::I32(1)));
    assert_eq!(h.prop("notes", Prop::Multiline), Some(PropValue::Bool(true)));
    assert_eq!(h.prop("upper", Prop::TextValue), Some(PropValue::Json(r#"{"text":"seed","count":0}"#.into())));
    assert_eq!(h.native_input("upper").text, "seed");
}

#[test]
fn typing_updates_state_and_is_not_echoed_back() {
    let mut h = form();
    let batches = h.batch_count();
    h.type_text("name", "Ada");
    assert_eq!(h.text_of("events"), "change:Ada:false");
    assert_eq!(h.text_of("echo"), "Ada");
    // The re-render that followed did not send the field its own text back.
    assert!(h.batch_count() > batches);
    assert!(!h.mutations().iter().any(
        |m| matches!(m, Mutation::SetProp { prop: Prop::TextValue, value: PropValue::Json(j), .. } if j.contains("Ada"))
    ));
    assert_eq!(h.native_input("name").text, "Ada");
    h.submit("name");
    assert_eq!(h.text_of("events"), "change:Ada:false|submit:Ada");
}

#[test]
fn script_writes_reach_the_field_with_the_last_seen_count() {
    let mut h = form();
    h.type_text("upper", "abc");
    // The listener rewrote the value; the write carries count 1 (the change it answered).
    assert_eq!(h.prop("upper", Prop::TextValue), Some(PropValue::Json(r#"{"text":"ABC","count":1}"#.into())));
    assert_eq!(h.native_input("upper").text, "ABC");
}

#[test]
fn a_script_write_older_than_native_typing_is_rejected_by_the_count() {
    let mut h = form();
    h.type_text("name", "a");
    // A write the core makes after seeing every change is applied.
    h.tap("set");
    assert_eq!(h.prop("name", Prop::TextValue), Some(PropValue::Json(r#"{"text":"from script","count":1}"#.into())));
    assert_eq!(h.native_input("name").text, "from script");

    // The user types (native count 2) while the core still believes count 1.
    let typed = h.type_in_flight("name", "from scriptX");
    h.tap("set");
    // The write carries count 1, older than the field's 2: the renderer keeps the user's text.
    assert_eq!(h.native_input("name").text, "from scriptX");
    // The late change then arrives and the script state follows the user.
    h.deliver(typed);
    assert_eq!(h.text_of("echo"), "from scriptX");
    assert_eq!(h.native_input("name").text, "from scriptX");
}

#[test]
fn composition_never_triggers_script_writes() {
    let mut h = form();
    h.clear_mutations();
    h.compose("upper", &["n", "ni", "nih"], "你好");
    assert_eq!(h.native_input("upper").text, "你好");
    let writes: Vec<_> =
        h.mutations().iter().filter(|m| matches!(m, Mutation::SetProp { prop: Prop::TextValue, .. })).collect();
    assert!(writes.is_empty(), "no TextValue echo while composing or after: {writes:?}");

    let mut h = form();
    h.compose("name", &["k", "ka"], "か");
    assert_eq!(h.text_of("events"), "change:k:true|change:ka:true|change:か:false");
    // The view re-rendered on every change, yet the field was never written.
    assert!(!h
        .mutations()
        .iter()
        .skip_while(|m| !matches!(m, Mutation::BeginBatch { seq: 2, .. }))
        .any(|m| matches!(m, Mutation::SetProp { prop: Prop::TextValue, .. })));
}

#[test]
fn focus_goes_out_as_a_command_on_the_field() {
    let mut h = form();
    h.clear_commands();
    h.tap("focus");
    let node = h.find("name").unwrap();
    assert!(h.commands().iter().any(|(n, c, _)| *n == node && *c == Command::Focus), "{:?}", h.commands());
    h.focus("name");
    assert!(h.text_of("events").ends_with("focus"));
}
