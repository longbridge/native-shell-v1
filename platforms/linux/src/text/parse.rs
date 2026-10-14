//! `AttributedText` JSON (docs/protocol-json.md) to a plain, thread-safe
//! description. Pango objects are not `Send`, so the JS thread (measuring) and
//! the main thread (drawing) each build their own layout from the same
//! [`ParsedText`] with the same configuration.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

use serde_json::Value;

/// Size of a run without one, as on the other renderers.
pub const DEFAULT_SIZE: f32 = 16.0;
/// Text color when a run has none (opaque black; the core always sends one).
const DEFAULT_COLOR: u32 = 0xff00_0000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Align {
    #[default]
    Left,
    Center,
    Right,
    Justify,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Run {
    /// Byte range in [`ParsedText::text`].
    pub start: usize,
    pub end: usize,
    /// Unscaled points; the font scale is applied when laying out.
    pub size: f32,
    pub weight: i32,
    pub color: u32,
    pub family: Option<String>,
    pub italic: bool,
    pub underline: bool,
    pub strike: bool,
    pub letter_spacing: f32,
    pub url: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct ParsedText {
    pub text: String,
    pub runs: Vec<Run>,
    pub align: Align,
    /// 0: unlimited.
    pub max_lines: usize,
    pub truncate: bool,
    /// The line height every line is forced to (the largest of the runs'),
    /// unscaled; `None` keeps the fonts' natural line heights.
    pub line_height: Option<f32>,
}

impl ParsedText {
    pub fn has_links(&self) -> bool {
        self.runs.iter().any(|r| r.url.is_some())
    }

    /// The link under a byte index of the text.
    pub fn url_at(&self, index: usize) -> Option<&str> {
        self.runs.iter().find(|r| r.start <= index && index < r.end).and_then(|r| r.url.as_deref())
    }

    /// Size, weight, family and style of an empty string's line: the first run's.
    pub fn first_run(&self) -> Option<&Run> {
        self.runs.first()
    }
}

/// Large enough that text measured on the JS thread is still parsed when it is drawn.
const CACHE_LIMIT: usize = 20_000;

fn cache() -> &'static Mutex<HashMap<String, Arc<ParsedText>>> {
    static CACHE: OnceLock<Mutex<HashMap<String, Arc<ParsedText>>>> = OnceLock::new();
    CACHE.get_or_init(Default::default)
}

/// Parses (or returns the cached parse of) one `AttributedText` value.
pub fn parse(json: &str) -> Arc<ParsedText> {
    if let Some(hit) = cache().lock().ok().and_then(|c| c.get(json).cloned()) {
        return hit;
    }
    let parsed = Arc::new(build(&serde_json::from_str(json).unwrap_or(Value::Null)));
    if let Ok(mut c) = cache().lock() {
        if c.len() >= CACHE_LIMIT {
            c.clear();
        }
        c.insert(json.to_owned(), parsed.clone());
    }
    parsed
}

pub fn build(v: &Value) -> ParsedText {
    let mut out = ParsedText::default();
    for run in v.get("runs").and_then(Value::as_array).into_iter().flatten() {
        let text = run.get("text").and_then(Value::as_str).unwrap_or("");
        let start = out.text.len();
        out.text.push_str(text);
        let flag = |key: &str| run.get(key).and_then(Value::as_bool).unwrap_or(false);
        let float = |key: &str| run.get(key).and_then(Value::as_f64).map(|f| f as f32).filter(|f| f.is_finite());
        if let Some(lh) = float("line_height").filter(|lh| *lh > 0.0) {
            out.line_height = Some(out.line_height.map_or(lh, |cur| cur.max(lh)));
        }
        out.runs.push(Run {
            start,
            end: out.text.len(),
            size: float("size").filter(|s| *s > 0.0).unwrap_or(DEFAULT_SIZE),
            weight: run.get("weight").and_then(Value::as_i64).map_or(400, |w| w.clamp(1, 1000) as i32),
            color: run.get("color").and_then(Value::as_u64).map_or(DEFAULT_COLOR, |c| c as u32),
            family: run.get("family").and_then(Value::as_str).filter(|f| !f.is_empty()).map(str::to_owned),
            italic: flag("italic"),
            underline: flag("underline"),
            strike: flag("strike"),
            letter_spacing: float("letter_spacing").unwrap_or(0.0),
            url: run.get("url").and_then(Value::as_str).map(str::to_owned),
        });
    }
    out.align = match v.get("align").and_then(Value::as_str) {
        Some("center") => Align::Center,
        Some("right") => Align::Right,
        Some("justify") => Align::Justify,
        _ => Align::Left,
    };
    out.max_lines = v.get("lines").and_then(Value::as_u64).map_or(0, |l| l as usize);
    out.truncate = v.get("truncate").and_then(Value::as_bool).unwrap_or(false);
    out
}

/// Drops every cached parse (tests).
#[cfg(test)]
pub fn clear_cache() {
    if let Ok(mut c) = cache().lock() {
        c.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_runs_and_paragraph_options() {
        let p = parse(
            r#"{"runs":[{"text":"Hi ","size":17,"weight":600,"color":4278190080,"line_height":22},
                {"text":"there","size":15,"weight":400,"color":4294901760,"italic":true,"underline":true,
                 "strike":true,"letter_spacing":0.5,"family":"monospace","url":"https://example.org","line_height":24}],
               "align":"center","lines":2,"truncate":true}"#,
        );
        assert_eq!(p.text, "Hi there");
        assert_eq!((p.runs[0].start, p.runs[0].end, p.runs[1].start, p.runs[1].end), (0, 3, 3, 8));
        assert_eq!(p.runs[0].weight, 600);
        assert!(p.runs[1].italic && p.runs[1].underline && p.runs[1].strike);
        assert_eq!(p.runs[1].family.as_deref(), Some("monospace"));
        assert_eq!(p.line_height, Some(24.0));
        assert_eq!((p.align, p.max_lines, p.truncate), (Align::Center, 2, true));
        assert_eq!(p.url_at(4), Some("https://example.org"));
        assert_eq!(p.url_at(1), None);
    }

    #[test]
    fn malformed_json_is_empty_text() {
        let p = parse("{not json");
        assert!(p.text.is_empty() && p.runs.is_empty());
        clear_cache();
    }
}
