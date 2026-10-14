//! One Pango configuration for measuring and drawing. Each thread owns a
//! PangoCairo font map and context (Pango objects are not shared across
//! threads); both are set up identically, with unhinted metrics and
//! unrounded glyph positions so a layout has the same geometry on the JS
//! thread, on the main thread and at any device scale.

use gtk::pango;
use gtk::pango::prelude::{FontExt, FontMapExt, IsAttribute};
use pangocairo::cairo;

use super::parse::{Align, ParsedText, Run, DEFAULT_SIZE};

/// The system UI face. GNOME's UI font (Adwaita Sans, derived from Inter) is
/// the closest Linux counterpart of the iOS system font; the rest are the
/// usual fallbacks on other desktops. Fontconfig picks the first installed
/// family and falls back per glyph (CJK, emoji) from there.
pub const SANS_FAMILIES: &str = "Adwaita Sans, Inter, Cantarell, Noto Sans, sans-serif";
pub const MONO_FAMILIES: &str = "Adwaita Mono, JetBrains Mono, DejaVu Sans Mono, monospace";

const ELLIPSIS: &str = "\u{2026}";

thread_local! {
    static CONTEXT: pango::Context = new_context();
}

fn new_context() -> pango::Context {
    let context = pangocairo::FontMap::new().create_context();
    if let Ok(mut options) = cairo::FontOptions::new() {
        options.set_hint_style(cairo::HintStyle::None);
        options.set_hint_metrics(cairo::HintMetrics::Off);
        options.set_antialias(cairo::Antialias::Gray);
        pangocairo::functions::context_set_font_options(&context, Some(&options));
    }
    context.set_round_glyph_positions(false);
    context
}

/// A laid out text and its geometry in points.
pub struct TextLayout {
    pub layout: pango::Layout,
    /// Widest shown line, unrounded.
    pub width: f32,
    /// Bottom of the last shown line, unrounded.
    pub height: f32,
    /// First line's baseline from the top.
    pub baseline: f32,
    /// Set when `lines` caps the text without an ellipsis: draw clipped to this height.
    pub clip: Option<f32>,
}

/// Lays `text` out, wrapping within `width` (unwrapped when `None`), with
/// every size multiplied by `font_scale`.
pub fn build(text: &ParsedText, width: Option<f32>, font_scale: f32) -> TextLayout {
    CONTEXT.with(|context| {
        let layout = pango::Layout::new(context);
        let base = configure(&layout, text, width, font_scale);
        let mut attrs = apply_text(&layout, text, text.text.len(), font_scale);
        let lines = layout.line_count().max(0) as usize;
        if text.truncate && text.max_lines > 0 && lines > text.max_lines {
            // Pango limits lines per paragraph, so text with hard breaks can
            // still run past the limit: cut it after the last allowed line
            // and let the ellipsis go there.
            if let Some(line) = layout.line_readonly(text.max_lines as i32 - 1) {
                let end = (line.start_index() + line.length()).max(0) as usize;
                attrs = apply_text(&layout, text, end.min(text.text.len()), font_scale);
            }
        }
        if text.line_height.is_some() {
            center_fallback_runs(context, &layout, &attrs, &base);
        }
        geometry(layout, text, font_scale)
    })
}

/// Returns the layout's base font.
fn configure(layout: &pango::Layout, text: &ParsedText, width: Option<f32>, font_scale: f32) -> pango::FontDescription {
    let base = text.first_run().map_or_else(|| font(None, DEFAULT_SIZE, 400, false, font_scale), |r| run_font(r, font_scale));
    layout.set_font_description(Some(&base));
    layout.set_wrap(pango::WrapMode::WordChar);
    layout.set_width(match width {
        Some(w) if w.is_finite() && w >= 0.0 => (w * pango::SCALE as f32).ceil() as i32,
        _ => -1,
    });
    layout.set_alignment(match text.align {
        Align::Center => pango::Alignment::Center,
        Align::Right => pango::Alignment::Right,
        Align::Left | Align::Justify => pango::Alignment::Left,
    });
    layout.set_justify(text.align == Align::Justify);
    if text.truncate && text.max_lines > 0 {
        layout.set_ellipsize(pango::EllipsizeMode::End);
        layout.set_height(-(text.max_lines as i32));
    }
    base
}

/// Sets the text cut at byte `cut` (an ellipsis appended when it is cut) and
/// the runs' attributes.
fn apply_text(layout: &pango::Layout, text: &ParsedText, cut: usize, font_scale: f32) -> pango::AttrList {
    let mut shown = text.text[..cut].to_owned();
    if cut < text.text.len() {
        shown.truncate(shown.trim_end_matches(['\n', '\r', '\u{2028}', '\u{2029}']).len());
        shown.push_str(ELLIPSIS);
    }
    let attrs = pango::AttrList::new();
    let scale = pango::SCALE as f32;
    let mut runs = text.runs.iter().filter(|r| r.start < shown.len() || r.start == 0).peekable();
    while let Some(run) = runs.next() {
        // The last shown run also covers an appended ellipsis.
        let end = if runs.peek().is_none() { shown.len() } else { run.end.min(shown.len()) };
        let (start, end) = (run.start as u32, end as u32);
        let insert = |mut attr: pango::Attribute| {
            attr.set_start_index(start);
            attr.set_end_index(end);
            attrs.insert(attr);
        };
        insert(pango::AttrFontDesc::new(&run_font(run, font_scale)).upcast());
        let [a, r, g, b] = run.color.to_be_bytes().map(|c| u16::from(c) * 257);
        insert(pango::AttrColor::new_foreground(r, g, b).upcast());
        insert(pango::AttrInt::new_foreground_alpha(a).upcast());
        if run.underline {
            insert(pango::AttrInt::new_underline(pango::Underline::Single).upcast());
        }
        if run.strike {
            insert(pango::AttrInt::new_strikethrough(true).upcast());
        }
        if run.letter_spacing != 0.0 {
            insert(pango::AttrInt::new_letter_spacing((run.letter_spacing * font_scale * scale).round() as i32).upcast());
        }
    }
    if let Some(lh) = text.line_height {
        // A fixed line height on every line; Pango splits the difference to
        // the font's height evenly above and below, centering the glyphs.
        attrs.insert(pango::AttrInt::new_line_height_absolute((lh * font_scale * scale).round() as i32).upcast());
    }
    layout.set_text(&shown);
    layout.set_attributes(Some(&attrs));
    attrs
}

/// With a fixed line height Pango centers each font's box in the line on its
/// own, so a line mixing faces (CJK or emoji fallback, a monospace run…)
/// would be the union of boxes offset from each other and grow past the line
/// height. Raising or lowering each run to the base font's box keeps every
/// line exactly the line height, like the forced line heights on iOS and
/// Android.
fn center_fallback_runs(context: &pango::Context, layout: &pango::Layout, attrs: &pango::AttrList, base: &pango::FontDescription) {
    let Some(metrics) = context.load_font(base).map(|f| f.metrics(None)) else { return };
    let reference = metrics.descent() - metrics.ascent();
    let rises = attrs.copy().unwrap_or_default();
    let mut changed = false;
    let mut iter = layout.iter();
    loop {
        if let Some(run) = iter.run_readonly() {
            let item = run.item();
            let (_, logical) = run.glyph_string().extents(&item.analysis().font());
            // Logical y is -ascent, height ascent + descent.
            let own = (logical.height() + logical.y()) + logical.y();
            let rise = (own - reference) / 2;
            if rise != 0 {
                let mut attr = pango::AttrInt::new_rise(rise);
                attr.set_start_index(item.offset() as u32);
                attr.set_end_index((item.offset() + item.length()) as u32);
                rises.insert(attr);
                changed = true;
            }
        }
        if !iter.next_run() {
            break;
        }
    }
    if changed {
        layout.set_attributes(Some(&rises));
    }
}

fn geometry(layout: pango::Layout, text: &ParsedText, font_scale: f32) -> TextLayout {
    let scale = pango::SCALE as f32;
    let lines = layout.line_count().max(1) as usize;
    let shown = if text.max_lines > 0 { lines.min(text.max_lines) } else { lines };
    let mut iter = layout.iter();
    let (mut widest, mut bottom) = (0, 0);
    for i in 0..shown {
        let (_, logical) = iter.line_extents();
        widest = widest.max(logical.width());
        bottom = logical.y() + logical.height();
        if i + 1 < shown && !iter.next_line() {
            break;
        }
    }
    // A fixed line height is exact; Pango's line boxes can still exceed it
    // by a fraction where it adds an ellipsis run without the attribute.
    let height = match text.line_height {
        Some(lh) => shown as f32 * (lh * font_scale * scale).round() / scale,
        None => bottom as f32 / scale,
    };
    TextLayout {
        width: if text.text.is_empty() { 0.0 } else { widest as f32 / scale },
        height,
        baseline: layout.baseline() as f32 / scale,
        clip: (shown < lines).then_some(height),
        layout,
    }
}

fn run_font(run: &Run, font_scale: f32) -> pango::FontDescription {
    font(run.family.as_deref(), run.size, run.weight, run.italic, font_scale)
}

fn font(family: Option<&str>, size: f32, weight: i32, italic: bool, font_scale: f32) -> pango::FontDescription {
    let mut desc = pango::FontDescription::new();
    desc.set_family(&families(family));
    desc.set_absolute_size(f64::from(size * font_scale) * f64::from(pango::SCALE));
    desc.set_weight(pango::Weight::__Unknown(weight.clamp(100, 1000)));
    desc.set_style(if italic { pango::Style::Italic } else { pango::Style::Normal });
    desc
}

/// A run's `family` as a fontconfig family list. Generic names and the Apple
/// system faces programs name map to the Linux UI faces; any other family is
/// tried first and falls back to the UI face when it is not installed.
pub fn families(family: Option<&str>) -> String {
    match family {
        None | Some("" | "system" | "system-ui" | "sans-serif" | "-apple-system" | ".SystemUIFont") => SANS_FAMILIES.into(),
        Some("monospace" | "mono" | "ui-monospace" | "Menlo" | "Monaco" | "SF Mono") => MONO_FAMILIES.into(),
        Some("serif") => "serif".into(),
        Some(name) => format!("{name}, {SANS_FAMILIES}"),
    }
}
