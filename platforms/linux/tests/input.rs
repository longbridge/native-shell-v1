//! `TextInput` end to end: native edits reach the script, the script's values
//! come back under the event-count protocol, IME composition is never
//! clobbered, filters, submit and focus.

mod support;

use std::process::ExitCode;

use native_shell_gtk::InstanceOptions;

use support::*;

fn main() -> ExitCode {
    run(&[
        ("typing reaches the script", typing_reaches_the_script),
        ("script values replace the text", script_values_replace_the_text),
        ("composition defers script values", composition_defers_script_values),
        ("filters limit what is typed", filters_limit_what_is_typed),
        ("submit, done and focus", submit_done_and_focus),
        ("multiline editor", multiline_editor),
        ("multiline placeholder sits on the first line", multiline_placeholder_sits_on_the_first_line),
        ("selection uses the theme's selection color", selection_uses_the_theme_token),
        ("chat composer sends and clears", chat_composer_sends_and_clears),
        ("address form: return moves on, validation", address_form),
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

fn multiline_placeholder_sits_on_the_first_line() {
    let f = Fixture::new(
        r##"
import { View, v_flex, Textarea, TextareaState } from "native-shell";
export default class App extends View {
  init() { this.notes = TextareaState.new({ placeholder: "Gate code, a safe place to leave it", rows: 4 }); }
  render() {
    return v_flex().size_full().bg("#ffffff").p(16)
      .child(Textarea.new(this.notes).test_id("notes").px(12).py(10).text_base().text_color("#000000"));
  }
}"##,
    );
    let node = f.expect("notes");
    let field = f.bounds(&node);
    let label = find_in(&node, &|w| w.is::<gtk::Label>()).expect("the placeholder");
    assert!(label.is_visible());
    assert!(f.bounds(&label).height() < field.height() / 2.0, "one line, not stretched over the field");
    // No (blinking) caret in the way of the ink.
    area(&f, "notes").set_cursor_visible(false);
    let placeholder = ink(&f, &field);
    // The same words typed: the placeholder must sit exactly where they land.
    area(&f, "notes").buffer().set_text("Gate code, a safe place to leave it");
    f.settle();
    let typed = ink(&f, &field);
    assert!(!label.is_visible());
    assert!(placeholder.0.abs_diff(typed.0) <= 1 && placeholder.1.abs_diff(typed.1) <= 1, "placeholder ink at {placeholder:?}, typed text at {typed:?}");
    // At the insets, at the top: not centered in the four rows.
    let (left, top) = (typed.0 as f32 - field.x(), typed.1 as f32 - field.y());
    assert!((12.0..16.0).contains(&left), "leading inset: ink {left} from the edge");
    assert!((10.0..20.0).contains(&top), "top inset: ink {top} from the top");
}

/// The top-left of the ink (non-white pixels) inside `rect` of the surface.
fn ink(f: &Fixture, rect: &gtk::graphene::Rect) -> (u32, u32) {
    let texture = render(f.surface.upcast_ref());
    let mut downloader = gtk::gdk::TextureDownloader::new(&texture);
    downloader.set_format(gtk::gdk::MemoryFormat::B8g8r8a8);
    let (bytes, stride) = downloader.download_bytes();
    let (x0, y0) = (rect.x() as usize + 2, rect.y() as usize + 2);
    let (x1, y1) = ((rect.x() + rect.width()) as usize - 2, (rect.y() + rect.height()) as usize - 2);
    let dark = |x: usize, y: usize| bytes[y * stride + x * 4 + 1] < 200;
    let top = (y0..y1).find(|&y| (x0..x1).any(|x| dark(x, y))).expect("ink in the field");
    let left = (x0..x1).find(|&x| (y0..y1).any(|y| dark(x, y))).expect("ink in the field");
    (left as u32, top as u32)
}

fn selection_uses_the_theme_token() {
    let f = Fixture::new(
        r##"
import { View, v_flex, Input, InputState } from "native-shell";
export default class App extends View {
  init(_p, cx) {
    cx.set_theme({ tokens: { selection: "#ff000080", background: "#ffffff" } });
    this.state = InputState.new({ value: "                " });
  }
  render(cx) {
    return v_flex().size_full().bg(cx.theme().background).p(16)
      .child(Input.new(this.state).test_id("field").h(44).bg(cx.theme().background));
  }
}"##,
    );
    let field = line(&f, "field");
    let bounds = f.bounds(&f.expect("field"));
    let (x, y) = ((bounds.x() + 20.0) as i32, (bounds.y() + bounds.height() / 2.0) as i32);
    assert_eq!(f.pixel(x, y), 0xffffffff);
    f.click("field");
    field.select_region(0, -1);
    // Spaces only, so no glyph covers the sampled pixel: white under 50% red.
    let [_, r, g, b] = f.pixel(x, y).to_be_bytes();
    assert!(r == 0xff && (0x70..=0x90).contains(&g) && g == b, "selection color {:#x}", f.pixel(x, y));
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

/// Screenshots `screenshots/<name>-<light|dark>.png`, switching the theme
/// live (the dark one also checks every editor restyles).
fn shoot(f: &Fixture, name: &str) {
    for dark in [false, true] {
        f.set_dark(dark);
        f.screenshot(format!("screenshots/{name}-{}.png", if dark { "dark" } else { "light" }));
    }
    f.set_dark(false);
}

/// The components example's shipping address form, as a user fills it in.
fn address_form() {
    // The accent the iOS and Android example apps give the examples.
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/components");
    let f = Fixture::start_with(&example_runtime(), InstanceOptions::default(), |i| i.load_dir(&dir));
    // Without animations the page shows at once, never mid-transition.
    if let Some(settings) = gtk::Settings::default() {
        settings.set_gtk_enable_animations(false);
    }
    f.tap("open-fields");
    f.wait_for("name");
    shoot(&f, "input-address-empty");

    let fields: Vec<gtk::Text> = ["name", "street", "city", "postal", "phone"].iter().map(|id| line(&f, id)).collect();
    f.click("name");
    assert!(fields[0].has_focus());
    for (i, text) in ["Ada Lovelace", "12 Harbour Street", "Springfield", "123"].iter().enumerate() {
        type_text(&f, &fields[i], text);
        // Return moves to the next field (the script focuses it).
        fields[i].emit_by_name::<()>("activate", &[]);
        f.settle();
        assert!(fields[i + 1].has_focus(), "Return in field {i} moves on");
    }
    // The postal code asks for a number keyboard.
    assert_eq!(fields[3].text(), "123");
    assert_eq!(fields[3].input_purpose(), gtk::InputPurpose::Digits);
    assert_eq!(fields[1].input_hints(), gtk::InputHints::UPPERCASE_WORDS);
    type_text(&f, &fields[4], "555");
    let notes = area(&f, "notes");
    notes.buffer().insert_at_cursor("Leave it with the neighbour at number 14.");
    f.settle();
    // Save validates and explains under the field.
    f.tap("save-address");
    f.expect("postal-error");
    f.expect("phone-error");
    shoot(&f, "input-address-errors");
    // Editing clears the errors; six digits do not fit a five-digit code.
    type_text(&f, &fields[3], "456");
    assert_eq!(fields[3].text(), "12345");
    assert!(f.find("postal-error").is_none());
}
