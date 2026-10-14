//! `TextInput` at the component's own level, where a test controls exactly
//! what the core sends: stale and equal values, writes during composition,
//! `Null`, editor rebuilds. The program-level flows are in `tests/input.rs`.

use std::cell::RefCell;
use std::rc::Rc;

use gtk::prelude::*;
use native_shell_core::protocol::{event_mask, Command, EventType, Frame, Mutation, Prop, PropValue, ViewType};

use super::{input_hints, input_purpose, stylesheet, Config};
use crate::component::Shared;
use crate::gtk_test::{self, flush};
use crate::mount::MountingManager;
use crate::node_view::NodeView;
use crate::theme::ThemeTokens;

#[test]
fn stylesheet_scales_text_and_uses_the_text_faces() {
    let config = Config {
        style: serde_json::json!({ "size": 17, "weight": 600, "family": "Menlo", "line_height": 22, "color": 0xff11_2233u32 }),
        placeholder_color: Some(0x8000_0000),
        ..Default::default()
    };
    let css = stylesheet(&config, 2.0, None, None, Some(0xff00_7aff));
    assert!(css.contains("font-size: 34.00px; font-weight: 600;"), "text scales fully: {css}");
    assert!(css.contains("line-height: 44.00px"), "{css}");
    assert!(css.contains("font-family: \"Adwaita Mono\""), "the same faces as Text: {css}");
    assert!(css.contains("color: rgba(17,34,51,1.000); caret-color: rgba(0,122,255,1.000)"), "{css}");
    assert!(css.contains("label.nsh-placeholder { color: rgba(0,0,0,0.502)"), "{css}");
}

#[test]
fn stylesheet_defaults_follow_the_theme() {
    let css = stylesheet(&Config::default(), 1.0, Some(0xff0a_0a0a), Some(0xff73_7373), None);
    assert!(css.contains("font-size: 16.00px"), "{css}");
    assert!(css.contains("color: rgba(10,10,10,1.000); caret-color: rgba(10,10,10,1.000)"), "{css}");
    assert!(css.contains("label.nsh-placeholder { color: rgba(115,115,115,1.000)"), "{css}");
}

#[test]
fn keyboard_and_content_types_map_to_input_purposes() {
    use gtk::InputPurpose as P;
    assert_eq!(input_purpose(false, 0, 0), P::FreeForm);
    assert_eq!(input_purpose(false, 1, 0), P::Digits);
    assert_eq!(input_purpose(false, 2, 0), P::Number);
    assert_eq!(input_purpose(false, 3, 0), P::Email);
    assert_eq!(input_purpose(false, 4, 0), P::Phone);
    assert_eq!(input_purpose(false, 5, 0), P::Url);
    assert_eq!(input_purpose(false, 1, 1), P::Pin);
    assert_eq!(input_purpose(true, 3, 0), P::Password);
    assert_eq!(input_purpose(false, 0, 3), P::Password);
    assert_eq!(input_hints(1, P::FreeForm), gtk::InputHints::UPPERCASE_WORDS);
    assert_eq!(input_hints(2, P::Email), gtk::InputHints::NO_SPELLCHECK);
}

// MARK: Widgets

type Events = Rc<RefCell<Vec<(EventType, serde_json::Value)>>>;

const INPUT: u32 = 1;

struct Fixture {
    manager: MountingManager,
    events: Events,
    window: gtk::Window,
}

impl Fixture {
    fn new(mask: u32) -> Fixture {
        let events: Events = Rc::default();
        let sink = events.clone();
        let root = NodeView::new();
        let window = gtk::Window::builder().default_width(390).default_height(200).child(&root).build();
        window.present();
        let shared = Rc::new(Shared {
            events: Rc::new(move |_node, event, payload: String| {
                sink.borrow_mut().push((event, serde_json::from_str(&payload).unwrap_or_default()));
            }),
            theme: RefCell::new(ThemeTokens::default()),
            root,
            runtime: Default::default(),
            overlays: Default::default(),
        });
        let mut manager = MountingManager::new(shared);
        manager.apply(&[
            Mutation::Create { node: INPUT, view_type: ViewType::TextInput },
            Mutation::Insert { node: INPUT, parent: 0, index: 0 },
            Mutation::SetFrame { node: INPUT, frame: Frame { x: 16.0, y: 16.0, width: 358.0, height: 44.0 } },
            Mutation::SetEventMask { node: INPUT, mask },
            Mutation::SetProp { node: INPUT, prop: Prop::Placeholder, value: PropValue::Str("Task name".into()) },
        ]);
        flush();
        Fixture { manager, events, window }
    }

    fn set(&mut self, prop: Prop, value: PropValue) {
        self.manager.apply(&[Mutation::SetProp { node: INPUT, prop, value }]);
        flush();
    }

    fn value(&mut self, text: &str, count: u32) {
        self.set(Prop::TextValue, PropValue::Json(serde_json::json!({ "text": text, "count": count }).to_string()));
    }

    fn command(&mut self, command: Command) {
        self.manager.apply(&[Mutation::Command { node: INPUT, command, payload: "{}".into() }]);
        flush();
    }

    fn overlay(&self) -> gtk::Overlay {
        let view = self.manager.view(INPUT).cloned().unwrap_or_default();
        view.content().and_then(|c| c.downcast().ok()).unwrap_or_default()
    }

    fn line(&self) -> gtk::Text {
        self.overlay().child().and_then(|c| c.downcast().ok()).unwrap_or_default()
    }

    fn area(&self) -> gtk::TextView {
        let scroller = self.overlay().child().and_then(|c| c.downcast::<gtk::ScrolledWindow>().ok());
        scroller.and_then(|s| s.child()).and_then(|c| c.downcast().ok()).unwrap_or_default()
    }

    fn placeholder_visible(&self) -> bool {
        let mut child = self.overlay().first_child();
        while let Some(w) = child {
            if w.is::<gtk::Label>() {
                return w.is_visible();
            }
            child = w.next_sibling();
        }
        false
    }

    /// Types the way the keyboard does: one insertion at the caret.
    fn type_text(&self, text: &str) {
        self.line().emit_by_name::<()>("insert-at-cursor", &[&text]);
        flush();
    }

    fn preedit(&self, preedit: &str) {
        self.line().emit_by_name::<()>("preedit-changed", &[&preedit]);
        flush();
    }

    fn take(&self) -> Vec<(EventType, serde_json::Value)> {
        self.events.borrow_mut().drain(..).collect()
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        self.window.destroy();
        flush();
    }
}

#[test]
fn edits_are_counted_with_utf16_selections() {
    gtk_test::run(|| {
        let f = Fixture::new(event_mask::CHANGE);
        assert!(f.placeholder_visible());
        f.type_text("Buy");
        f.type_text(" 😀");
        let events = f.take();
        assert_eq!(events[0], (EventType::Change, serde_json::json!({ "text": "Buy", "selection": [3, 3], "composing": false, "count": 1 })));
        assert_eq!(events[1].1["selection"], serde_json::json!([6, 6]), "the emoji is two UTF-16 units");
        assert_eq!(events[1].1["count"], 2);
        assert!(!f.placeholder_visible());
    });
}

#[test]
fn stale_and_equal_values_are_not_written() {
    gtk_test::run(|| {
        let mut f = Fixture::new(event_mask::CHANGE);
        f.type_text("abc");
        f.value("ABC", 1);
        assert_eq!(f.line().text(), "ABC");
        assert_eq!(f.line().position(), 3, "the caret keeps its distance from the end");
        f.type_text("d");
        f.value("zzz", 1);
        assert_eq!(f.line().text(), "ABCd", "a value older than the latest edit is dropped");
        f.line().set_position(1);
        f.value("ABCd", 2);
        assert_eq!(f.line().position(), 1, "equal text is not rewritten (the caret would jump)");
        f.value("done", 2);
        assert_eq!(f.line().text(), "done");
        assert_eq!(f.take().len(), 2, "only the two typed edits are reported, never the writes");
    });
}

#[test]
fn values_wait_for_the_composition() {
    gtk_test::run(|| {
        let mut f = Fixture::new(event_mask::CHANGE);
        f.type_text("I ");
        f.preedit("ni");
        let events = f.take();
        assert_eq!(events[1].1, serde_json::json!({ "text": "I ni", "selection": [4, 4], "composing": true, "count": 2 }));
        assert!(!f.placeholder_visible());
        // Sent while composing: kept until the composition ends, then dropped
        // because the end of the composition is a newer edit.
        f.value("I NI", 2);
        assert_eq!(f.line().text(), "I ");
        f.preedit("");
        f.type_text("你");
        assert_eq!(f.line().text(), "I 你");
        let last = f.take().pop().map(|e| e.1).unwrap_or_default();
        assert_eq!((last["composing"].clone(), last["count"].clone()), (serde_json::json!(false), serde_json::json!(4)));
    });
}

#[test]
fn null_resets_text_and_count() {
    gtk_test::run(|| {
        let mut f = Fixture::new(event_mask::CHANGE);
        f.type_text("abc");
        f.set(Prop::TextValue, PropValue::Null);
        assert_eq!(f.line().text(), "");
        assert!(f.placeholder_visible());
        f.take();
        // A recycled node accepts the script's first value again.
        f.value("fresh", 0);
        assert_eq!(f.line().text(), "fresh");
        f.type_text("!");
        assert_eq!(f.take()[0].1["count"], 1);
    });
}

#[test]
fn filters_trim_or_reject_insertions() {
    gtk_test::run(|| {
        let mut f = Fixture::new(event_mask::CHANGE);
        f.set(Prop::MaxLength, PropValue::I32(4));
        f.set(Prop::AllowedChars, PropValue::Str("0123456789".into()));
        f.type_text("1a2b3c4d5");
        assert_eq!(f.line().text(), "1234");
        assert_eq!(f.line().position(), 4);
        assert_eq!(f.take().len(), 1, "a trimmed insertion is one edit");
        f.type_text("9");
        assert!(f.take().is_empty(), "a rejected one is none");
        f.set(Prop::MaxLength, PropValue::Null);
        f.set(Prop::AllowedChars, PropValue::Null);
        f.type_text("x");
        assert_eq!(f.line().text(), "1234x");
    });
}

#[test]
fn focus_commands_and_disabled_fields() {
    gtk_test::run(|| {
        let mut f = Fixture::new(event_mask::FOCUS | event_mask::BLUR);
        f.type_text("abc");
        f.command(Command::Focus);
        assert!(f.line().has_focus());
        assert_eq!(f.line().selection_bounds(), None, "focusing does not select all");
        f.command(Command::Blur);
        assert!(!f.line().has_focus());
        let kinds: Vec<EventType> = f.take().into_iter().map(|e| e.0).collect();
        assert_eq!(kinds, vec![EventType::Focus, EventType::Blur]);
        f.set(Prop::Editable, PropValue::Bool(false));
        f.command(Command::Focus);
        assert!(!f.line().has_focus(), "a disabled field cannot take focus");
    });
}

#[test]
fn switching_to_multiline_keeps_text_and_rules() {
    gtk_test::run(|| {
        let mut f = Fixture::new(event_mask::CHANGE);
        f.type_text("Gate code");
        f.command(Command::Focus);
        f.set(Prop::Multiline, PropValue::Bool(true));
        let buf = f.area().buffer();
        let text = |b: &gtk::TextBuffer| b.text(&b.start_iter(), &b.end_iter(), false).to_string();
        assert_eq!(text(&buf), "Gate code");
        assert!(f.area().has_focus(), "focus moves to the new editor");
        buf.insert_at_cursor("\n1234");
        flush();
        let count = f.take().last().and_then(|e| e.1["count"].as_u64()).unwrap_or(0) as u32;
        assert_eq!(count, 2, "the count carries over");
        f.value("Leave at door", count);
        assert_eq!(text(&buf), "Leave at door");
        f.set(Prop::MaxLength, PropValue::I32(16));
        buf.insert_at_cursor(" please");
        flush();
        assert_eq!(text(&buf), "Leave at door pl", "filters apply to the text view too");
        f.set(Prop::Multiline, PropValue::Bool(false));
        assert_eq!(f.line().text(), "Leave at door pl");
    });
}

#[test]
fn secure_fields_draw_dots() {
    gtk_test::run(|| {
        let mut f = Fixture::new(0);
        f.set(Prop::Secure, PropValue::Bool(true));
        assert!(!gtk::Text::is_visible(&f.line()), "secure text is drawn as dots");
        assert_eq!(f.line().input_purpose(), gtk::InputPurpose::Password);
    });
}
