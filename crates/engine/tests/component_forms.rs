//! "gpui-component" form components: Form / VForm / HForm / Field, InputGroup
//! (+ parts), Rating, Stepper (+ StepperItem), TimeField, Clipboard and
//! ColorPicker.

use native_shell::core::protocol::{A11yRole, EventType, Prop, PropValue, ViewType};
use native_shell::testing::Harness;
use native_shell::Error;

fn load(src: &str) -> Harness {
    let h = Harness::new(&[("main.js", src)]);
    assert!(h.load_error().is_none(), "{:?} {:?}", h.load_error(), h.logs());
    h
}


/// A node's frame in window coordinates (frames are relative to the parent).
fn at(h: &Harness, id: &str) -> (f32, f32) {
    let mut node = h.find(id).unwrap_or_else(|| panic!("no {id}\n{}", h.dump()));
    let (mut x, mut y) = (0.0, 0.0);
    loop {
        let n = &h.native().nodes[&node];
        if let Some(f) = n.frame {
            x += f.x;
            y += f.y;
        }
        match n.parent {
            Some(p) if h.native().nodes.contains_key(&p) => node = p,
            _ => return (x, y),
        }
    }
}

fn string(h: &Harness, id: &str, prop: Prop) -> String {
    match h.prop(id, prop) {
        Some(PropValue::Str(s)) => s,
        other => panic!("{id}: {prop:?} is {other:?}\n{}", h.dump()),
    }
}

const SIGN_UP: &str = r#"
import { View, InputState, Switch, text } from "native-shell";
import { VForm, HForm, Field, InputGroup, InputGroupInput } from "gpui-component";
export default class C extends View {
  init() {
    this.email = InputState.new({ placeholder: "you@example.org" });
    this.email.on("change", (_e, cx) => { this.error = null; cx.notify(); });
    this.error = null;
    this.business = false;
  }
  render() {
    return VForm.new().test_id("form")
      .child(Field.new().test_id("email").label("Email").required(true).description("We send the receipt here.")
        .error(this.error)
        .child(InputGroup.new("email").test_id("email-group").invalid(this.error !== null)
          .input(InputGroupInput.new(this.email).test_id("email-input").content_type("email_address"))))
      .child(Field.new().test_id("company").label("Company").visible(this.business).child(text("Acme")))
      .child(Field.new().test_id("terms").child(Switch.new("terms").test_id("terms-switch")
        .on_change((v, cx) => { this.business = v; this.error = v ? "Use a work address." : null; cx.notify(); })));
  }
}
"#;

#[test]
fn a_vertical_form_stacks_labeled_fields_with_descriptions_and_errors() {
    let mut h = load(SIGN_UP);
    assert!(h.texts().iter().any(|t| t.contains("Email")), "{}", h.dump());
    assert_eq!(h.text_of("email-description"), "We send the receipt here.");
    assert!(h.find("email-error").is_none());
    // The required marker is announced in words, and the lone control is named after the label.
    assert_eq!(string(&h, "email-label", Prop::A11yLabel), "Email, required");
    assert_eq!(string(&h, "email-input", Prop::A11yLabel), "Email, required");
    assert_eq!(h.view_type("email-input"), ViewType::TextInput);
    // A hidden field is laid out as nothing.
    assert!(h.find_text("Acme").is_none(), "{}", h.dump());

    h.tap("terms-switch");
    h.dispatch(h.find("terms-switch").unwrap(), EventType::Change, r#"{"checked":true}"#);
    assert_eq!(h.text_of("email-error"), "Use a work address.");
    assert_eq!(h.prop("email-error", Prop::A11yRole), Some(PropValue::I32(A11yRole::Alert as i32)));
    assert!(h.find("email-description").is_none(), "the error replaces the description");
    assert!(h.find_text("Acme").is_some(), "{}", h.dump());
}

fn first_render_error(src: &str) -> String {
    let h = Harness::new(&[("main.js", src)]);
    match h.load_error() {
        Some(Error::FirstRender(e)) => e.message.clone(),
        other => panic!("expected a render error, got {other:?}"),
    }
}

#[test]
fn forms_accept_only_fields_and_steppers_only_steps() {
    for (code, expect) in [
        ("VForm.new().child(div())", "Form accepts only registered Field children; received an ordinary element"),
        ("Stepper.new('s').child(div())", "Stepper accepts only registered StepperItem children; received an ordinary element"),
        ("VForm.new().columns(1.5)", "Form.columns expects an exactly representable positive integer"),
        ("Field.new().align('top')", "unsupported Field alignment `top`"),
        ("Rating.new('r').max(-1)", "Rating.max(max) expects a non-negative integer, got -1"),
        ("Rating.new('r').size('huge')", "unsupported Rating size `huge`"),
        ("Rating.new('')", "Rating id must not be empty"),
        ("InputGroup.new('g').input(div())", "input expects one registered InputGroup part"),
        ("InputGroupAddon.new('a').align('left')", "unsupported align `left`"),
        ("InputGroupInput.new(null)", "InputGroupInput expects one InputState entity"),
    ] {
        let src = format!(
            r#"
import {{ View, div }} from "native-shell";
import {{ VForm, Field, Stepper, Rating, InputGroup, InputGroupAddon, InputGroupInput }} from "gpui-component";
export default class T extends View {{ render() {{ return {code}; }} }}
"#
        );
        let message = first_render_error(&src);
        assert!(message.contains(expect), "{code}: {message}");
    }
}

const PROFILE: &str = r#"
import { View, InputState, Switch, div } from "native-shell";
import { HForm, Field, InputGroup, InputGroupInput } from "gpui-component";
export default class C extends View {
  init() { this.name = InputState.new({ value: "Ada Lovelace" }); }
  render() {
    return HForm.new().test_id("form").label_width(110)
      .child(Field.new().test_id("name").label("Name")
        .child(InputGroup.new("name").test_id("name-group").input(InputGroupInput.new(this.name).test_id("name-input"))))
      .child(Field.new().test_id("alerts").label("Alerts").description("Delivery updates only.")
        .child(Switch.new("alerts").test_id("alerts-switch")))
      .child(Field.new().test_id("plain").label_indent(false).child(div().test_id("signout").child("Sign out")));
  }
}
"#;

#[test]
fn a_horizontal_form_is_an_inset_grouped_list_of_rows() {
    let h = load(PROFILE);
    let surface = native_shell::core::theme::Theme::default_for(native_shell::core::theme::Appearance::Light).color("surface").unwrap();
    assert_eq!(h.prop("form", Prop::BackgroundColor), Some(PropValue::Color(surface)));
    // Label beside the control: same row, the control to the trailing side.
    let label = at(&h, "name-label");
    let input = at(&h, "name-input");
    assert!(input.0 >= label.0 + 110.0 - 1.0, "label {label:?} input {input:?}");
    assert!((input.1 - label.1).abs() < 20.0, "label {label:?} input {input:?}");
    // A field inside a row loses its own frame.
    assert_eq!(h.prop("name-group", Prop::BorderWidths), None, "{}", h.dump());
    assert!(at(&h, "alerts-switch").0 > at(&h, "alerts-label").0 + 110.0);
    assert_eq!(h.text_of("alerts-description"), "Delivery updates only.");
    // An unindented field starts at the row's inset.
    assert!(at(&h, "signout").0 < 40.0, "{:?}", at(&h, "signout"));
}

const SEARCH: &str = r##"
import { View, InputState, v_flex, div } from "native-shell";
import { InputGroup, InputGroupInput, InputGroupAddon, InputGroupButton, InputGroupText } from "gpui-component";
export default class C extends View {
  init() {
    this.code = InputState.new({ placeholder: "Gift card code" });
    this.query = "";
    this.log = [];
    this.locked = false;
  }
  render() {
    return v_flex()
      .child(div().test_id("log").child(this.log.join(",")))
      .child(div().test_id("lock").child("lock").on_click((_e, cx) => { this.locked = true; cx.notify(); }))
      .child(InputGroup.new("code").test_id("group").aria_label("Gift card").disabled(this.locked).invalid(this.query === "BAD")
        .input(InputGroupInput.new(this.code).test_id("input").value(this.query).content_type("one_time_code")
          .on_change((v, cx) => { this.query = v; this.log.push(`change:${v}`); cx.notify(); }))
        .addon(InputGroupAddon.new("count").test_id("count").align("inline-end")
          .child(InputGroupText.new().child(`${this.query.length}/16`))
          .child(InputGroupButton.new("apply").test_id("apply").label("Apply").disabled(this.query.length === 0)
            .on_click((_e, cx) => { this.log.push(`apply:${this.query}`); this.query = ""; cx.notify(); })))
        .addon(InputGroupAddon.new("prefix").test_id("prefix").child(InputGroupText.new().child("#"))));
  }
}
"##;

#[test]
fn an_input_group_frames_its_field_and_addons_and_reports_edits() {
    let mut h = load(SEARCH);
    // Inline-start addons come before the field, inline-end ones after it.
    assert!(at(&h, "prefix").0 < at(&h, "input").0 && at(&h, "input").0 < at(&h, "count").0, "{}", h.dump());
    assert_eq!(string(&h, "input", Prop::A11yLabel), "Gift card", "the group's name goes to its field");
    assert_eq!(h.prop("input", Prop::KeyboardType), Some(PropValue::I32(1)));
    assert_eq!(h.prop("apply", Prop::A11yState), Some(PropValue::Json(r#"{"disabled":true}"#.into())));
    let input_border = native_shell::core::theme::Theme::default_for(native_shell::core::theme::Appearance::Light).color("input").unwrap();
    assert_eq!(h.prop("group", Prop::BorderColor), Some(PropValue::Color(input_border)));

    h.type_text("input", "SPRING");
    assert_eq!(h.text_of("log"), "change:SPRING");
    assert!(h.texts().contains(&"6/16".to_string()), "{}", h.dump());
    h.tap("apply");
    assert_eq!(h.text_of("log"), "change:SPRING,apply:SPRING");
    assert_eq!(h.native_input("input").text, "", "a controlled value is written back to the field");

    h.type_text("input", "BAD");
    let destructive = native_shell::core::theme::Theme::default_for(native_shell::core::theme::Appearance::Light).color("destructive").unwrap();
    assert_eq!(h.prop("group", Prop::BorderColor), Some(PropValue::Color(destructive)));

    // Disabling the group stops edits and its buttons.
    h.tap("lock");
    assert_eq!(h.prop("input", Prop::Editable), Some(PropValue::Bool(false)));
    h.tap("apply");
    assert!(!h.text_of("log").contains("apply:BAD"), "{}", h.text_of("log"));
}

const DELIVERY: &str = r#"
import { View, v_flex, div } from "native-shell";
import { Rating } from "gpui-component";
export default class C extends View {
  init() { this.stars = 0; this.log = []; }
  render() {
    return v_flex()
      .child(div().test_id("log").child(this.log.join(",")))
      .child(Rating.new("delivery").test_id("rate").value(this.stars).on_change((v, cx) => { this.stars = v; this.log.push(v); cx.notify(); }))
      .child(Rating.new("average").test_id("average").value(4.5).disabled(true))
      .child(Rating.new("locked").test_id("locked").value(2).max(3).disabled(true).on_change((v, cx) => { this.log.push(`locked${v}`); cx.notify(); }));
  }
}
"#;

#[test]
fn a_rating_sets_lowers_and_reads_out_its_stars() {
    let mut h = load(DELIVERY);
    for ix in 1..=5 {
        assert!(h.find(&format!("rate-star-{ix}")).is_some(), "{}", h.dump());
    }
    assert_eq!(string(&h, "rate-star-1", Prop::A11yLabel), "1 star");
    assert_eq!(string(&h, "rate-star-4", Prop::A11yLabel), "4 stars");
    h.tap("rate-star-4");
    assert_eq!(h.text_of("log"), "4");
    assert_eq!(h.prop("rate-star-4", Prop::A11yState), Some(PropValue::Json(r#"{"selected":true}"#.into())));
    assert_eq!(h.prop("rate-star-5", Prop::A11yState), None, "unselected stars carry no state");
    // A filled star lowers the rating to the one before it, as gpui-component's does.
    h.tap("rate-star-4");
    assert_eq!(h.text_of("log"), "4,3");
    h.tap("rate-star-1");
    assert_eq!(h.text_of("log"), "4,3,0");
    assert!(h.commands().iter().any(|(_, c, p)| *c == native_shell::core::protocol::Command::Haptic && p.contains("selection")));

    // Hardware keys step it and digits set it.
    let node = h.find("rate").unwrap();
    h.dispatch(node, EventType::Key, r#"{"key":"right","modifiers":{},"phase":"down"}"#);
    h.dispatch(node, EventType::Key, r#"{"key":"right","modifiers":{},"phase":"down"}"#);
    h.dispatch(node, EventType::Key, r#"{"key":"5","key_char":"5","modifiers":{},"phase":"down"}"#);
    h.dispatch(node, EventType::Key, r#"{"key":"left","modifiers":{},"phase":"down"}"#);
    assert_eq!(h.text_of("log"), "4,3,0,1,2,5,4");

    // Read-only ratings are one image announcing the value, halves included, and take no taps.
    assert_eq!(h.prop("average", Prop::A11yRole), Some(PropValue::I32(A11yRole::Image as i32)));
    assert_eq!(string(&h, "average", Prop::A11yLabel), "4.5 out of 5 stars");
    assert!(h.find("average-star-1").is_none());
    assert_eq!(string(&h, "locked", Prop::A11yLabel), "2 out of 3 stars");
    assert!(h.find("locked-star-1").is_none(), "disabled ratings have no star buttons");
}

const CHECKOUT: &str = r#"
import { View, v_flex, div } from "native-shell";
import { Stepper, StepperItem } from "gpui-component";
export default class C extends View {
  init() { this.step = 1; this.log = []; }
  render() {
    return v_flex()
      .child(div().test_id("log").child(this.log.join(",")))
      .child(Stepper.new("checkout").test_id("steps").selected_index(this.step)
        .on_change((i, cx) => { this.step = i; this.log.push(i); cx.notify(); })
        .child(StepperItem.new().child("Cart"))
        .child(StepperItem.new().child("Shipping"))
        .child(StepperItem.new().disabled(this.step < 2).child("Payment")))
      .child(Stepper.new("history").test_id("vertical").vertical(true).selected_index(2).disabled(true)
        .items([StepperItem.new().child("Ordered"), StepperItem.new().child("Shipped"), StepperItem.new().child("Delivered")]));
  }
}
"#;

#[test]
fn a_stepper_marks_done_and_current_steps_and_reports_taps() {
    let mut h = load(CHECKOUT);
    assert_eq!(string(&h, "steps-step-0", Prop::A11yLabel), "Step 1 of 3, Cart");
    assert_eq!(string(&h, "steps-step-0", Prop::A11yValue), "Completed");
    assert_eq!(h.prop("steps-step-1", Prop::A11yState), Some(PropValue::Json(r#"{"selected":true}"#.into())));
    assert_eq!(h.prop("steps-step-2", Prop::A11yState), Some(PropValue::Json(r#"{"disabled":true}"#.into())));
    // Horizontal: steps side by side, number badges for the steps not done yet.
    assert!(h.frame("steps-step-0").x < h.frame("steps-step-1").x && h.frame("steps-step-1").x < h.frame("steps-step-2").x);
    assert!(h.texts().contains(&"3".to_string()), "{}", h.dump());

    h.tap("steps-step-2");
    assert_eq!(h.text_of("log"), "", "a disabled step is not chosen");
    h.tap("steps-step-1");
    assert_eq!(h.text_of("log"), "", "the current step is not reported again");
    h.tap("steps-step-0");
    assert_eq!(h.text_of("log"), "0");
    assert_eq!(h.prop("steps-step-0", Prop::A11yState), Some(PropValue::Json(r#"{"selected":true}"#.into())));

    // Vertical: stacked, every step done or current, and a disabled stepper takes no taps.
    assert!(h.frame("vertical-step-0").y < h.frame("vertical-step-1").y && h.frame("vertical-step-1").y < h.frame("vertical-step-2").y);
    h.tap("vertical-step-0");
    assert_eq!(h.text_of("log"), "0");
}

const REMINDER: &str = r#"
import { View, v_flex, div } from "native-shell";
import { TimeField, TimeFieldState } from "gpui-component";
export default class C extends View {
  init() {
    this.time = TimeFieldState.new({ value: "09:30" });
    this.seconds = TimeFieldState.new({ value: "07:05:42", precision: "second" });
    this.log = [];
    this.time.on("change", (t, cx) => { this.log.push(t); cx.notify(); });
  }
  render() {
    return v_flex()
      .child(div().test_id("log").child(`${this.log.join(",")} now=${this.time.value()} s=${this.seconds.value()}`))
      .child(TimeField.new(this.time).test_id("time"))
      .child(TimeField.new(this.seconds).test_id("seconds").disabled(true))
      .child(div().test_id("reset").child("reset").on_click((_e, cx) => { this.time.set_value("18:00"); }));
  }
}
"#;

#[test]
fn a_time_field_is_the_native_time_picker_bound_to_its_state() {
    let mut h = load(REMINDER);
    assert_eq!(h.view_type("time"), ViewType::DatePicker);
    assert_eq!(string(&h, "time", Prop::Date), "09:30");
    assert_eq!(h.prop("time", Prop::DateMode), Some(PropValue::I32(1)));
    assert_eq!(h.text_of("log"), " now=09:30 s=07:05:42");
    h.dispatch(h.find("time").unwrap(), EventType::Change, r#"{"date":"10:15"}"#);
    assert_eq!(h.text_of("log"), "10:15 now=10:15 s=07:05:42");
    // Seconds survive an edit of the hour and minute.
    h.dispatch(h.find("seconds").unwrap(), EventType::Change, r#"{"date":"08:00"}"#);
    assert_eq!(h.prop("seconds", Prop::Enabled), Some(PropValue::Bool(false)));
    // set_value redraws without reporting a change.
    h.tap("reset");
    assert_eq!(string(&h, "time", Prop::Date), "18:00");
    assert_eq!(h.text_of("log"), "10:15 now=18:00 s=07:05:42");
}

const SHARE: &str = r#"
import { View, h_flex, div } from "native-shell";
import { Clipboard } from "gpui-component";
export default class C extends View {
  init() { this.log = []; }
  render() {
    return h_flex()
      .child(div().test_id("log").child(this.log.join(",")))
      .child(Clipboard.new("link").test_id("copy").value("https://example.org/r/4521")
        .accessibility_label("Copy invite link")
        .on_copied((v, cx) => { this.log.push(v); cx.notify(); }));
  }
}
"#;

const CLIPBOARD_MANIFEST: &str =
    r#"{"id":"dev.test.forms","name":"Forms","version":"1.0.0","entry":"main.js","capabilities":{"clipboard":{"write":true}}}"#;

#[test]
fn a_clipboard_button_copies_and_confirms_unless_the_platform_does() {
    let mut h = Harness::build(&[("main.js", SHARE)]).manifest(CLIPBOARD_MANIFEST).start();
    assert!(h.load_error().is_none(), "{:?}", h.load_error());
    assert_eq!(string(&h, "copy", Prop::A11yLabel), "Copy invite link");
    h.tap("copy");
    assert!(h.commands().iter().any(|(_, c, p)| *c == native_shell::core::protocol::Command::SetClipboard && p.contains("example.org/r/4521")));
    assert_eq!(h.text_of("log"), "https://example.org/r/4521");
    assert_eq!(string(&h, "copy", Prop::A11yValue), "Copied");
    h.advance(2100.0);
    assert_eq!(h.prop("copy", Prop::A11yValue), None, "the confirmation goes after two seconds");

    h.set_environment(native_shell::core::host::Environment { clipboard_feedback: true, ..Default::default() });
    h.tap("copy");
    assert_eq!(h.text_of("log"), "https://example.org/r/4521,https://example.org/r/4521");
    assert_eq!(h.prop("copy", Prop::A11yValue), None, "the platform confirms the copy itself");
}

#[test]
fn copying_needs_the_clipboard_capability() {
    let mut h = Harness::with_manifest(&[("main.js", SHARE)], r#"{"id":"dev.test.forms","name":"Forms"}"#);
    h.tap("copy");
    assert!(h.error_logs().join("\n").contains("capabilities.clipboard.write"), "{:?}", h.error_logs());
}

const THEME: &str = r##"
import { View, v_flex, div } from "native-shell";
import { ColorPicker, ColorPickerState } from "gpui-component";
export default class C extends View {
  init() {
    this.accent = ColorPickerState.new({ value: "#007aff" });
    this.border = ColorPickerState.new();
    this.log = [];
    this.accent.on("change", (hex, cx) => { this.log.push(hex); cx.notify(); });
  }
  render() {
    return v_flex()
      .child(div().test_id("log").child(`${this.log.join(",")} value=${this.accent.value()}`))
      .child(ColorPicker.new(this.accent).test_id("accent").label("Accent"))
      .child(ColorPicker.new(this.border).test_id("off").disabled(true));
  }
}
"##;

#[test]
fn a_color_picker_opens_a_sheet_of_colors_and_reports_choices() {
    let mut h = load(THEME);
    assert_eq!(h.text_of("log"), " value=#007AFF");
    assert_eq!(string(&h, "accent-swatch", Prop::A11yLabel), "Accent");
    assert_eq!(string(&h, "accent-swatch", Prop::A11yValue), "#007AFF");
    assert_eq!(h.surfaces().len(), 1);
    h.tap("off-swatch");
    assert_eq!(h.surfaces().len(), 1, "a disabled picker does not open");

    h.tap("accent-swatch");
    assert_eq!(h.surfaces().len(), 2, "{}", h.dump());
    assert_eq!(h.prop("accent-swatch-007AFF", Prop::A11yState), Some(PropValue::Json(r#"{"selected":true}"#.into())));
    h.tap("accent-swatch-FF9500");
    assert_eq!(h.text_of("log"), "#FF9500 value=#FF9500");
    assert_eq!(h.native_input("accent-hex").text, "FF9500", "the hex field follows the choice");
    assert_eq!(h.surfaces().len(), 2, "a choice applies without closing the sheet");

    // A complete hex applies as it is typed; Done on an incomplete one explains.
    h.type_text("accent-hex", "34C759");
    assert_eq!(h.text_of("log"), "#FF9500,#34C759 value=#34C759");
    h.type_text("accent-hex", "34C");
    h.submit("accent-hex");
    assert!(h.find("accent-hex-error").is_some(), "{}", h.dump());

    h.tap("accent-done");
    assert_eq!(h.surfaces().len(), 1);
    assert_eq!(string(&h, "accent-swatch", Prop::A11yValue), "#34C759");
}

const IN_OVERLAYS: &str = r#"
import { View, v_flex, div, Popover } from "native-shell";
import { VForm, Field, Rating } from "gpui-component";
export default class C extends View {
  init() { this.sheet = 0; this.pop = 0; }
  form(id, value, set) {
    return VForm.new().child(Field.new().label("Rate").child(
      Rating.new(id).test_id(id).value(value).on_change((v, cx) => { set(v); cx.notify(); })));
  }
  render() {
    return v_flex()
      .child(div().test_id("log").child(`sheet=${this.sheet} pop=${this.pop}`))
      .child(div().test_id("open").child("open").on_click((_e, cx) =>
        cx.open_sheet(() => this.form("in-sheet", this.sheet, (v) => (this.sheet = v)))))
      .child(Popover.new("p").trigger(div().test_id("pop").child("pop"))
        .content(this.form("in-popover", this.pop, (v) => (this.pop = v))));
  }
}
"#;

#[test]
fn composite_components_work_inside_sheets_and_anchored_surfaces() {
    let mut h = load(IN_OVERLAYS);
    h.tap("open");
    h.tap("in-sheet-star-3");
    assert_eq!(h.text_of("log"), "sheet=3 pop=0");
    h.tap("pop");
    h.tap("in-popover-star-2");
    assert_eq!(h.text_of("log"), "sheet=3 pop=2");
    h.tap("in-popover-star-4");
    assert_eq!(h.text_of("log"), "sheet=3 pop=4", "{:?}", h.error_logs());
}
