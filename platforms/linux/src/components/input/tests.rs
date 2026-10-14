//! The GTK-free parts of `TextInput`; the widgets are tested end to end in
//! `tests/input.rs`.

use super::{input_hints, input_purpose, stylesheet, Config};

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
