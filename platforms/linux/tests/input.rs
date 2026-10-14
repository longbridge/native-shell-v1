//! `TextInput` end to end: native edits reach the script, the script's values
//! come back under the event-count protocol, IME composition is never
//! clobbered, filters, submit and focus.

mod support;

use std::process::ExitCode;

use support::*;

fn main() -> ExitCode {
    run(&[
        ("typing reaches the script", typing_reaches_the_script),
        ("script values replace the text", script_values_replace_the_text),
        ("composition defers script values", composition_defers_script_values),
        ("filters limit what is typed", filters_limit_what_is_typed),
        ("submit, done and focus", submit_done_and_focus),
        ("multiline editor", multiline_editor),
        ("chat composer sends and clears", chat_composer_sends_and_clears),
        ("example screenshots", example_screenshots),
    ])
}

/// A small form: the title echoes into a test id, `shout` uppercases
/// everything the user types, `code` takes four digits, `notes` is a
/// textarea, and every input event is logged.
const FORM: &str = r#"
import { View, div, v_flex, Input, InputState, Textarea, TextareaState } from "native-shell";
export default class App extends View {
  init() {
    this.log = [];
    this.title = InputState.new({ placeholder: "Task name", return_key: "done" });
    this.shout = InputState.new({ placeholder: "Shout" });
    this.code = InputState.new({ placeholder: "Code", max_length: 4, allowed_chars: "0123456789", keyboard: "number" });
    this.notes = TextareaState.new({ placeholder: "Notes", rows: 3 });
    for (const [name, state] of [["title", this.title], ["shout", this.shout], ["code", this.code], ["notes", this.notes]]) {
      for (const event of ["change", "submit", "focus", "blur"]) {
        state.on(event, (e, cx) => {
          this.log.push(`${name}.${event}` + (event === "change" && e.composing ? "*" : ""));
          cx.notify();
        });
      }
    }
    this.shout.on("change", (e) => this.shout.set_value(e.text.toUpperCase()));
  }
  render() {
    const value = (name, state) => div().h(1).test_id(`${name}=${state.value()}`);
    return v_flex().size_full().p(16).gap(8)
      .child(Input.new(this.title).test_id("title").h(44))
      .child(Input.new(this.shout).test_id("shout").h(44))
      .child(Input.new(this.code).test_id("code").h(44))
      .child(Textarea.new(this.notes).test_id("notes"))
      .child(div().test_id("focus-title").h(44).on_click(() => this.title.focus()))
      .child(div().test_id("blur-title").h(44).on_click(() => this.title.blur()))
      .child(value("title", this.title))
      .child(value("shout", this.shout))
      .child(value("code", this.code))
      .child(value("notes", this.notes))
      .child(div().h(1).test_id(`log:${this.log.join(",")}`));
  }
}"#;

fn line(f: &Fixture, test_id: &str) -> gtk::Text {
    let node = f.expect(test_id);
    let found = find_in(&node, &|w| w.is::<gtk::Text>());
    found.and_then(|w| w.downcast().ok()).unwrap_or_else(|| panic!("{test_id:?} has no gtk::Text"))
}

fn area(f: &Fixture, test_id: &str) -> gtk::TextView {
    let node = f.expect(test_id);
    let found = find_in(&node, &|w| w.is::<gtk::TextView>());
    found.and_then(|w| w.downcast().ok()).unwrap_or_else(|| panic!("{test_id:?} has no gtk::TextView"))
}

fn find_in(root: &gtk::Widget, matches: &dyn Fn(&gtk::Widget) -> bool) -> Option<gtk::Widget> {
    if matches(root) {
        return Some(root.clone());
    }
    let mut child = root.first_child();
    while let Some(w) = child {
        if let Some(found) = find_in(&w, matches) {
            return Some(found);
        }
        child = w.next_sibling();
    }
    None
}

/// Types the way the keyboard does: one insertion at the caret.
fn type_text(f: &Fixture, text: &gtk::Text, s: &str) {
    text.emit_by_name::<()>("insert-at-cursor", &[&s]);
    f.settle();
}

fn log(f: &Fixture) -> String {
    let w = f.find_where(|w| w.widget_name().starts_with("log:")).expect("the log node");
    w.widget_name().trim_start_matches("log:").to_owned()
}

fn typing_reaches_the_script() {
    let f = Fixture::new(FORM);
    let title = line(&f, "title");
    type_text(&f, &title, "Buy milk");
    f.expect("title=Buy milk");
    type_text(&f, &title, " 😀");
    f.expect("title=Buy milk 😀");
    assert_eq!(log(&f), "title.change,title.change");
}

fn script_values_replace_the_text() {
    let f = Fixture::new(FORM);
    let shout = line(&f, "shout");
    type_text(&f, &shout, "hello");
    assert_eq!(shout.text(), "HELLO", "the script's value replaces what was typed");
    assert_eq!(shout.position(), 5, "the caret stays at the end");
    type_text(&f, &shout, " there");
    assert_eq!(shout.text(), "HELLO THERE");
    f.expect("shout=HELLO THERE");
    // The script's write is not echoed back as a change.
    assert_eq!(log(&f), "shout.change,shout.change");
}

fn composition_defers_script_values() {
    let f = Fixture::new(FORM);
    let shout = line(&f, "shout");
    type_text(&f, &shout, "a ");
    assert_eq!(shout.text(), "A ");
    // Pinyin in progress: the script uppercases "A ni" but must not write it
    // into the field while the input method composes.
    shout.emit_by_name::<()>("preedit-changed", &[&"ni"]);
    f.settle();
    assert!(log(&f).ends_with("shout.change*"), "{}", log(&f));
    f.expect("shout=A NI");
    assert_eq!(shout.text(), "A ", "the field keeps the composition");
    shout.emit_by_name::<()>("preedit-changed", &[&"ni hao"]);
    f.settle();
    assert_eq!(shout.text(), "A ");
    // The commit: the preedit ends and the characters land.
    shout.emit_by_name::<()>("preedit-changed", &[&""]);
    type_text(&f, &shout, "你好");
    assert_eq!(shout.text(), "A 你好");
    f.expect("shout=A 你好");
    assert!(log(&f).ends_with("shout.change,shout.change"), "{}", log(&f));
}

fn filters_limit_what_is_typed() {
    let f = Fixture::new(FORM);
    let code = line(&f, "code");
    assert_eq!(code.input_purpose(), gtk::InputPurpose::Digits);
    type_text(&f, &code, "1a2b3c4d5");
    assert_eq!(code.text(), "1234");
    f.expect("code=1234");
    type_text(&f, &code, "9");
    assert_eq!(code.text(), "1234");
    assert_eq!(log(&f), "code.change", "a filtered insertion is one edit, a rejected one none");
}

fn submit_done_and_focus() {
    let f = Fixture::new(FORM);
    let title = line(&f, "title");
    f.tap("focus-title");
    assert!(title.has_focus());
    type_text(&f, &title, "Call Ana");
    title.emit_by_name::<()>("activate", &[]);
    f.settle();
    assert!(!title.has_focus(), "the `done` return key ends editing");
    assert_eq!(log(&f), "title.focus,title.change,title.submit,title.blur");
    f.tap("focus-title");
    f.tap("blur-title");
    assert!(!title.has_focus());
    assert!(log(&f).ends_with("title.blur,title.focus,title.blur"), "{}", log(&f));
}

fn multiline_editor() {
    let f = Fixture::new(FORM);
    let notes = area(&f, "notes");
    let buffer = notes.buffer();
    buffer.insert_at_cursor("Gate code 1234");
    f.settle();
    buffer.insert_at_cursor("\nLeave at the door");
    f.settle();
    f.expect("notes=Gate code 1234\nLeave at the door");
    assert_eq!(log(&f), "notes.change,notes.change");
    assert!(notes.wrap_mode() != gtk::WrapMode::None, "long notes wrap");
}

/// Broadway never ticks the frame clock here (see tests/support), so a style
/// change (the dark theme) is not applied until something reads the style,
/// and the cached render nodes are not redrawn: do both by hand.
fn restyle(widget: &gtk::Widget) {
    widget.color();
    widget.queue_draw();
    let mut child = widget.first_child();
    while let Some(w) = child {
        restyle(&w);
        child = w.next_sibling();
    }
}

fn shoot(f: &Fixture, name: &str) {
    for (dark, appearance) in [(false, "light"), (true, "dark")] {
        f.set_dark(dark);
        restyle(f.surface.upcast_ref());
        f.screenshot(format!("target/agents/linux-input/{name}-{appearance}.png"));
    }
}

fn chat_composer_sends_and_clears() {
    let f = Fixture::open_dir("../../examples/chat");
    let draft = line(&f, "draft");
    // A tap anywhere in the field (its padding included) starts editing.
    f.click("draft");
    assert!(draft.has_focus());
    type_text(&f, &draft, "On my way");
    draft.emit_by_name::<()>("activate", &[]);
    f.settle();
    let bubble = f.find_where(|w| w.widget_name().starts_with("bubble-new-"));
    assert!(bubble.is_some(), "the message was sent");
    assert_eq!(draft.text(), "", "the script cleared the composer");
    assert!(draft.has_focus(), "`send` keeps the keyboard up");
    type_text(&f, &draft, "!");
    assert_eq!(draft.text(), "!", "typing goes on after the script's write");
}

fn example_screenshots() {
    for example in ["chat", "gpui-todolist", "settings", "tasks", "components"] {
        let f = Fixture::open_dir(format!("../../examples/{example}"));
        shoot(&f, example);
    }
}
