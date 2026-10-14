//! The measurement matrix: scripts × fonts × line heights × font scales.
//! Pango needs no display, so these run as plain unit tests on any thread.

use native_shell_core::host::WidthMode;
use serde_json::json;

use super::*;

const LATIN: &str = "The quick brown fox jumps over the lazy dog";
const CJK: &str = "敏捷的棕色狐狸跳过了懒狗，日本語のテキストも折り返す";
const EMOJI: &str = "Party time 🎉🎂 with family 👨‍👩‍👧‍👦 and flags 🇯🇵";
const MIXED: &str = "Order #42 已发货 🚚 — arriving Tuesday";

fn text(s: &str, extra: serde_json::Value) -> String {
    let mut run = json!({"text": s, "size": 17, "weight": 400, "color": 4278190080u32});
    if let (Some(run), Some(extra)) = (run.as_object_mut(), extra.as_object()) {
        for (k, v) in extra {
            run.insert(k.clone(), v.clone());
        }
    }
    json!({"runs": [run]}).to_string()
}

fn with(json: &str, key: &str, value: serde_json::Value) -> String {
    let mut v: serde_json::Value = serde_json::from_str(json).unwrap_or_default();
    v[key] = value;
    v.to_string()
}

fn lines_of(json: &str, width: Option<f32>, scale: f32) -> i32 {
    build(&parse(json), width, scale).layout.line_count()
}

#[test]
fn unconstrained_text_is_one_line_and_measures_its_width() {
    for s in [LATIN, CJK, EMOJI, MIXED] {
        let m = measure_at(&text(s, json!({})), WidthMode::Undefined, 1.0, 1.0);
        assert!(m.width > 100.0, "{s}: {m:?}");
        assert_eq!(lines_of(&text(s, json!({})), None, 1.0), 1, "{s}");
        assert!(m.baseline > 0.0 && m.baseline < m.height, "{s}: {m:?}");
    }
}

#[test]
fn width_modes_follow_the_protocol() {
    let json = text(LATIN, json!({}));
    let full = measure_at(&json, WidthMode::Undefined, 1.0, 1.0);
    let wide = measure_at(&json, WidthMode::AtMost(full.width + 50.0), 1.0, 1.0);
    assert_eq!(wide.width, full.width);
    assert_eq!(wide.height, full.height);
    let exact = measure_at(&json, WidthMode::Exactly(full.width + 50.0), 1.0, 1.0);
    assert_eq!(exact.width, full.width + 50.0);
    let narrow = measure_at(&json, WidthMode::AtMost(120.0), 1.0, 1.0);
    assert!(narrow.width <= 120.0, "{narrow:?}");
    assert!(narrow.height > full.height * 2.5, "wraps: {narrow:?} vs {full:?}");
    // Laid out again at exactly its own width, the text keeps its lines.
    let again = measure_at(&json, WidthMode::Exactly(full.width), 1.0, 1.0);
    assert_eq!(again.height, full.height);
    // An infinite or negative width is an unconstrained probe.
    assert_eq!(measure_at(&json, WidthMode::AtMost(f32::INFINITY), 1.0, 1.0), full);
}

#[test]
fn wrapping_breaks_words_and_cjk_within_the_width() {
    for s in [LATIN, CJK, EMOJI, MIXED, "Supercalifragilisticexpialidocious"] {
        let json = text(s, json!({}));
        for w in [60.0, 97.5, 150.0, 233.3] {
            let laid = build(&parse(&json), Some(w), 1.0);
            assert!(laid.width <= w + 0.01, "{s} at {w}: {}", laid.width);
            assert!(laid.layout.line_count() >= 2, "{s} at {w}");
        }
    }
}

#[test]
fn empty_string_is_zero_wide_and_one_line_high() {
    let empty = measure_at(&text("", json!({})), WidthMode::Undefined, 1.0, 1.0);
    let one = measure_at(&text("A", json!({})), WidthMode::Undefined, 1.0, 1.0);
    assert_eq!(empty.width, 0.0);
    assert_eq!(empty.height, one.height);
    assert_eq!(empty.baseline, one.baseline);
    let lh = measure_at(&text("", json!({"line_height": 30})), WidthMode::Undefined, 1.0, 1.0);
    assert_eq!(lh.height, 30.0);
    let none = measure_at(r#"{"runs":[]}"#, WidthMode::Undefined, 1.0, 1.0);
    assert!(none.width == 0.0 && none.height > 0.0, "{none:?}");
}

#[test]
fn fixed_line_height_is_exact_per_line_and_centers_glyphs() {
    for s in [LATIN, CJK, EMOJI, MIXED] {
        for lh in [14.0f32, 22.0, 28.5, 40.0] {
            for scale in [1.0f32, 1.3, 2.0] {
                let json = text(s, json!({"line_height": lh}));
                let laid = build(&parse(&json), Some(110.0), scale);
                let lines = laid.layout.line_count() as f32;
                let expected = lines * lh * scale;
                assert!((laid.height - expected).abs() <= 1.0, "{s} lh {lh} × {scale}: {} vs {expected}", laid.height);
                // The first baseline sits inside the first line box, below its middle.
                assert!(laid.baseline > lh * scale * 0.4 && laid.baseline < lh * scale, "{s} lh {lh}: {}", laid.baseline);
            }
        }
    }
}

#[test]
fn font_scale_scales_every_size() {
    for s in [LATIN, CJK, EMOJI] {
        let json = text(s, json!({"line_height": 22, "letter_spacing": 0.5}));
        let base = measure_at(&json, WidthMode::Undefined, 1.0, 1.0);
        for scale in [1.3f32, 2.0] {
            let m = measure_at(&json, WidthMode::Undefined, scale, 1.0);
            // Less than linear: the UI face has an optical size axis that
            // tightens larger text, as on iOS.
            let ratio = m.width / base.width / scale;
            assert!((0.85..=1.02).contains(&ratio), "{s} × {scale}: width ratio {ratio}");
            assert!((m.height - (22.0 * scale).ceil()).abs() <= 1.0, "{s} × {scale}: {m:?}");
        }
    }
}

#[test]
fn heights_round_up_to_device_pixels() {
    let json = text(LATIN, json!({"line_height": 20.3}));
    for device in [1.0f32, 1.5, 2.0, 3.0] {
        let m = measure_at(&json, WidthMode::Undefined, 1.0, device);
        let px = m.height * device;
        assert!((px - px.round()).abs() < 1e-3, "{device}: {m:?}");
        assert!(m.height >= 20.3 - 1e-3 && m.height < 20.3 + 1.0 / device + 1e-3, "{device}: {m:?}");
    }
}

#[test]
fn line_clamp_caps_height_and_truncates_with_an_ellipsis() {
    let long = format!("{LATIN} {LATIN} {LATIN}");
    let json = text(&long, json!({"line_height": 20}));
    let free = measure_at(&json, WidthMode::Exactly(150.0), 1.0, 1.0);
    let clamped = with(&json, "lines", json!(2));
    let clip = measure_at(&clamped, WidthMode::Exactly(150.0), 1.0, 1.0);
    assert!(free.height > 60.0);
    assert_eq!(clip.height, 40.0);
    let laid = build(&parse(&clamped), Some(150.0), 1.0);
    assert_eq!(laid.clip, Some(40.0));

    let truncated = with(&clamped, "truncate", json!(true));
    let laid = build(&parse(&truncated), Some(150.0), 1.0);
    assert_eq!(laid.layout.line_count(), 2);
    assert!(laid.layout.is_ellipsized());
    assert_eq!(laid.clip, None);
    assert_eq!(measure_at(&truncated, WidthMode::Exactly(150.0), 1.0, 1.0).height, 40.0);

    // `lines: 1` without truncate is nowrap: one line, clipped.
    let nowrap = with(&json, "lines", json!(1));
    assert_eq!(measure_at(&nowrap, WidthMode::Exactly(150.0), 1.0, 1.0).height, 20.0);
}

#[test]
fn truncation_counts_lines_across_hard_breaks() {
    let json = with(&text("First paragraph\nSecond paragraph\nThird paragraph", json!({})), "lines", json!(2));
    let json = with(&json, "truncate", json!(true));
    let laid = build(&parse(&json), Some(300.0), 1.0);
    assert_eq!(laid.layout.line_count(), 2);
    assert!(laid.layout.text().ends_with('\u{2026}'), "{}", laid.layout.text());
}

#[test]
fn alignment_keeps_the_measured_size() {
    let base = text(LATIN, json!({}));
    let left = measure_at(&base, WidthMode::AtMost(180.0), 1.0, 1.0);
    for align in ["center", "right", "justify"] {
        let m = measure_at(&with(&base, "align", json!(align)), WidthMode::AtMost(180.0), 1.0, 1.0);
        assert_eq!(m.height, left.height, "{align}");
        assert!(m.width <= 180.0, "{align}: {m:?}");
    }
}

#[test]
fn measured_layout_equals_drawn_layout() {
    // The renderer redraws at the pixel-snapped width the core settles on; the
    // layout it builds there must have the measured size (error ≤ 1 px).
    for s in [LATIN, CJK, EMOJI, MIXED, "Line one\nLine two that is a little longer"] {
        for (extra, scale) in [(json!({}), 1.0), (json!({"line_height": 24}), 1.3), (json!({"weight": 700, "italic": true}), 2.0)] {
            let json = text(s, extra);
            for probe in [WidthMode::Undefined, WidthMode::AtMost(140.0), WidthMode::AtMost(260.0)] {
                let measured = measure_at(&json, probe, scale, 2.0);
                let frame_width = (measured.width * 2.0).ceil() / 2.0;
                let drawn = build(&parse(&json), Some(frame_width), scale);
                let at_frame = measure_at(&json, WidthMode::Exactly(frame_width), scale, 2.0);
                assert!((drawn.height - measured.height).abs() <= 1.0, "{s} {probe:?}: {} vs {:?}", drawn.height, measured);
                assert_eq!(at_frame.height, measured.height, "{s} {probe:?}");
                assert!(drawn.width <= frame_width + 0.01, "{s} {probe:?}");
            }
        }
    }
}

#[test]
fn runs_keep_their_own_fonts() {
    let mixed = json!({"runs": [
        {"text": "Total ", "size": 15, "weight": 400, "color": 4278190080u32},
        {"text": "$1,234.56", "size": 28, "weight": 700, "color": 4281545523u32, "family": "monospace"}
    ]})
    .to_string();
    let small = measure_at(&text("Total ", json!({"size": 15})), WidthMode::Undefined, 1.0, 1.0);
    let m = measure_at(&mixed, WidthMode::Undefined, 1.0, 1.0);
    assert!(m.height > small.height * 1.5, "{m:?} vs {small:?}");
    assert_eq!(layout::families(Some("Menlo")), layout::MONO_FAMILIES);
    assert!(layout::families(Some("Georgia")).starts_with("Georgia, "));
}

#[test]
fn scales_are_recorded_for_the_js_thread() {
    // The only test touching the globals; it restores them.
    let (device, font) = (device_scale(), font_scale());
    assert!(set_scales(2.0, 1.5));
    assert_eq!((device_scale(), font_scale()), (2.0, 1.5));
    assert!(!set_scales(2.0, 1.5));
    set_scales(f32::NAN, -1.0);
    assert_eq!((device_scale(), font_scale()), (1.0, 1.0));
    set_scales(device, font);
}
