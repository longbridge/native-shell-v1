//! Description → native properties.
//!
//! Visual properties are derived from the resolved style for every native view;
//! kind-specific ones (attributed text, list counts…) from the element; then the
//! description's explicit props pass through and win. The result is kept sorted
//! by property id so two commits diff with one merge.

use std::fmt::Write;

use native_shell_core::desc::{Desc, ElementKind};
use native_shell_core::protocol::{Prop, PropValue, ViewType};
use native_shell_core::style::{LineHeight, ScrollAxis, TextStyle, DEFAULT_TEXT_SIZE};

use crate::DEFAULT_PREFETCH;

pub(crate) const DEFAULT_WEIGHT: u16 = 400;

/// What text JSON depends on besides the description. Sizes are never multiplied by the
/// user's font scale: renderers apply it the same way when measuring and drawing.
pub(crate) struct TextEnv {
    pub color: u32,
}

pub(crate) fn derive(
    desc: &Desc,
    native: ViewType,
    text: &TextStyle,
    env: &TextEnv,
    native_count: u32,
) -> Vec<(Prop, PropValue)> {
    let s = &desc.style;
    let v = &s.visual;
    let mut out: Vec<(Prop, PropValue)> = Vec::new();
    if let Some(c) = v.background {
        out.push((Prop::BackgroundColor, PropValue::Color(c)));
    }
    if v.radii != [0.0; 4] {
        out.push((Prop::CornerRadii, PropValue::F32Array(v.radii.to_vec())));
    }
    if s.has_border() {
        out.push((
            Prop::BorderWidths,
            PropValue::F32Array(s.border_widths.to_vec()),
        ));
    }
    if let Some(c) = v.border_color {
        out.push((Prop::BorderColor, PropValue::Color(c)));
    }
    if v.opacity < 1.0 {
        out.push((Prop::Opacity, PropValue::F32(v.opacity)));
    }
    // The protocol carries one shadow: the first outer layer. Further layers and inset
    // shadows are not drawn yet.
    if let Some(sh) = v
        .shadows
        .iter()
        .find(|s| !s.inset)
        .or(v.shadow.as_ref().filter(|s| !s.inset))
    {
        out.push((
            Prop::Shadow,
            PropValue::Json(serde_json::to_string(sh).unwrap_or_default()),
        ));
    }
    if let Some((angle, stops)) = &v.gradient {
        let stops: Vec<(u32, f32)> = stops.clone();
        let json = serde_json::json!({ "angle": angle, "stops": stops });
        out.push((Prop::Gradient, PropValue::Json(json.to_string())));
    }
    if v.clip {
        out.push((Prop::ClipsToBounds, PropValue::Bool(true)));
    }
    if s.is_hidden() || v.invisible {
        out.push((Prop::Hidden, PropValue::Bool(true)));
    }
    if let Some([tx, ty, scale, rotate]) = v.transform {
        let json = serde_json::json!({ "translate_x": tx, "translate_y": ty, "scale": scale, "rotate": rotate });
        out.push((Prop::Transform, PropValue::Json(json.to_string())));
    }
    if !s.transitions.is_empty() {
        let mut map = serde_json::Map::new();
        for t in &s.transitions {
            let anim = if t.kind == "spring" {
                serde_json::json!({ "kind": t.kind, "response": t.response, "damping": t.damping, "delay": t.delay })
            } else {
                serde_json::json!({ "kind": t.kind, "duration": t.duration, "delay": t.delay, "easing": t.easing })
            };
            map.insert(t.property.clone(), anim);
        }
        out.push((
            Prop::Animation,
            PropValue::Json(serde_json::Value::Object(map).to_string()),
        ));
    }
    if let Some(o) = s.active_opacity {
        out.push((Prop::ActiveOpacity, PropValue::F32(o)));
    }
    if let Some(c) = s.active_background {
        out.push((Prop::ActiveBackground, PropValue::Color(c)));
    }

    match (&desc.kind, native) {
        (_, ViewType::Scroll) if s.scroll == ScrollAxis::X => {
            out.push((Prop::Horizontal, PropValue::Bool(true)))
        }
        (ElementKind::Select, _) => {
            out.push((Prop::SelectedIndex, PropValue::I32(selected_index(desc))))
        }
        (ElementKind::Text(runs), _) => {
            out.push((
                Prop::AttributedText,
                PropValue::Json(attributed_text(runs, text, env)),
            ));
        }
        (ElementKind::TextInput, _) => out.push((
            Prop::TextStyle,
            PropValue::Json(input_text_style(text, env)),
        )),
        (ElementKind::List(spec), _) => {
            out.push((
                Prop::ItemCount,
                PropValue::I32(spec.count.min(i32::MAX as u32) as i32),
            ));
            out.push((Prop::Prefetch, PropValue::F32(DEFAULT_PREFETCH)));
        }
        (ElementKind::ListRow(i), _) => out.push((
            Prop::RowIndex,
            PropValue::I32((*i).min(i32::MAX as u32) as i32),
        )),
        _ => {}
    }

    for (p, value) in &desc.props {
        // Sent once after Create, never diffed: a host view cannot change what it is.
        if *p == Prop::HostComponent {
            continue;
        }
        // A select's selected value travels as the index of its menu item.
        if native == ViewType::Select && *p == Prop::Value && matches!(value, PropValue::Str(_)) {
            continue;
        }
        if *p == Prop::TextValue {
            if let Some(t) = text_of(value) {
                out.push((
                    Prop::TextValue,
                    PropValue::Json(text_value_json(&t, native_count)),
                ));
                continue;
            }
        }
        out.push((*p, value.clone()));
    }
    sort_last_wins(&mut out);
    out
}

/// Sorts by prop id; for duplicates the later entry wins (explicit props override derived ones).
fn sort_last_wins(v: &mut Vec<(Prop, PropValue)>) {
    v.reverse();
    v.sort_by_key(|(p, _)| *p as u16);
    v.dedup_by_key(|(p, _)| *p as u16);
}

/// The script's controlled value, given as the plain text or as `{text}` JSON.
fn text_of(v: &PropValue) -> Option<String> {
    match v {
        PropValue::Str(s) => Some(s.clone()),
        PropValue::Json(j) => serde_json::from_str::<serde_json::Value>(j)
            .ok()?
            .get("text")?
            .as_str()
            .map(str::to_string),
        _ => None,
    }
}

pub(crate) fn text_value_json(text: &str, count: u32) -> String {
    format!("{{\"text\":{},\"count\":{count}}}", json_str(text))
}

fn json_str(s: &str) -> String {
    serde_json::to_string(s).unwrap_or_else(|_| "\"\"".into())
}

fn num(out: &mut String, key: &str, v: f32) {
    let v = if v.is_finite() { v } else { 0.0 };
    let _ = write!(out, ",\"{key}\":{v}");
}

/// Index of the `Menu` item whose `value` is the selected one (`Prop::Value` as a string),
/// `-1` when none. An explicit `SelectedIndex` in the description overrides it.
fn selected_index(desc: &Desc) -> i32 {
    let (Some(PropValue::Json(menu)), Some(PropValue::Str(selected))) =
        (desc.prop(Prop::Menu), desc.prop(Prop::Value))
    else {
        return -1;
    };
    let Ok(menu) = serde_json::from_str::<serde_json::Value>(menu) else {
        return -1;
    };
    menu.get("items")
        .and_then(|i| i.as_array())
        .and_then(|items| {
            items
                .iter()
                .position(|i| i.get("value").and_then(|v| v.as_str()) == Some(selected.as_str()))
        })
        .map_or(-1, |i| i as i32)
}

fn resolve_line_height(lh: Option<LineHeight>, size: f32) -> Option<f32> {
    match lh? {
        LineHeight::Normal => None,
        LineHeight::Multiple(m) => Some(m * size),
        LineHeight::Px(px) => Some(px),
    }
}

/// The `AttributedText` shape of `docs/protocol-json.md`: `size`, `weight` and `color` always
/// present, booleans only when true, line height as absolute `line_height`, no nulls.
/// `whitespace_nowrap` without a line limit is `lines: 1` without truncation.
pub(crate) fn attributed_text(
    runs: &[native_shell_core::desc::TextRun],
    node: &TextStyle,
    env: &TextEnv,
) -> String {
    let mut out = String::from("{\"runs\":[");
    for (i, run) in runs.iter().enumerate() {
        let st = run.style.inherit(node);
        if i > 0 {
            out.push(',');
        }
        let size = st.size.unwrap_or(DEFAULT_TEXT_SIZE);
        let _ = write!(out, "{{\"text\":{}", json_str(&run.text));
        num(&mut out, "size", size);
        let _ = write!(out, ",\"weight\":{}", st.weight.unwrap_or(DEFAULT_WEIGHT));
        let _ = write!(out, ",\"color\":{}", st.color.unwrap_or(env.color));
        if let Some(f) = &st.family {
            let _ = write!(out, ",\"family\":{}", json_str(f));
        }
        for (key, on) in [
            ("italic", st.italic),
            ("underline", st.underline),
            ("strike", st.strike),
        ] {
            if on == Some(true) {
                let _ = write!(out, ",\"{key}\":true");
            }
        }
        if let Some(lh) = resolve_line_height(st.line_height, size) {
            num(&mut out, "line_height", lh);
        }
        if let Some(ls) = st.letter_spacing {
            num(&mut out, "letter_spacing", ls);
        }
        if let Some(url) = &run.url {
            let _ = write!(out, ",\"url\":{}", json_str(url));
        }
        out.push('}');
    }
    out.push(']');
    if let Some(a) = node.align {
        let _ = write!(out, ",\"align\":\"{a}\"");
    }
    let lines = node
        .lines
        .filter(|l| *l > 0)
        .or(if node.nowrap == Some(true) {
            Some(1)
        } else {
            None
        });
    if let Some(l) = lines {
        let _ = write!(out, ",\"lines\":{l}");
    }
    if node.truncate == Some(true) {
        out.push_str(",\"truncate\":true");
    }
    out.push('}');
    out
}

/// `{size, weight, color, family, align, line_height}` of a text field.
fn input_text_style(st: &TextStyle, env: &TextEnv) -> String {
    let mut out = String::from("{");
    let size = st.size.unwrap_or(DEFAULT_TEXT_SIZE);
    let _ = write!(out, "\"size\":{size}");
    let _ = write!(out, ",\"weight\":{}", st.weight.unwrap_or(DEFAULT_WEIGHT));
    let _ = write!(out, ",\"color\":{}", st.color.unwrap_or(env.color));
    if let Some(f) = &st.family {
        let _ = write!(out, ",\"family\":{}", json_str(f));
    }
    if let Some(a) = st.align {
        let _ = write!(out, ",\"align\":\"{a}\"");
    }
    if let Some(lh) = resolve_line_height(st.line_height, size) {
        num(&mut out, "line_height", lh);
    }
    out.push('}');
    out
}

/// The concatenated run texts of an attributed text, for dumps.
pub(crate) fn plain_text(json: &str) -> String {
    let Ok(v) = serde_json::from_str::<serde_json::Value>(json) else {
        return String::new();
    };
    v.get("runs")
        .and_then(|r| r.as_array())
        .map(|runs| {
            runs.iter()
                .filter_map(|r| r.get("text")?.as_str())
                .collect()
        })
        .unwrap_or_default()
}

/// Merges two prop lists sorted by id: calls `emit` for every value that changed and with
/// `Null` for every prop that disappeared (the renderer's reset contract). Leaves `old == new`.
pub(crate) fn diff(
    old: &mut Vec<(Prop, PropValue)>,
    new: Vec<(Prop, PropValue)>,
    mut emit: impl FnMut(Prop, PropValue),
) {
    if *old == new {
        return;
    }
    let (mut i, mut j) = (0, 0);
    while i < old.len() || j < new.len() {
        let a = old.get(i).map(|(p, _)| *p as u16);
        let b = new.get(j).map(|(p, _)| *p as u16);
        match (a, b) {
            (Some(x), Some(y)) if x == y => {
                if old[i].1 != new[j].1 {
                    emit(new[j].0, new[j].1.clone());
                }
                i += 1;
                j += 1;
            }
            (Some(x), Some(y)) if x < y => {
                emit(old[i].0, PropValue::Null);
                i += 1;
            }
            (Some(_), None) => {
                emit(old[i].0, PropValue::Null);
                i += 1;
            }
            _ => {
                emit(new[j].0, new[j].1.clone());
                j += 1;
            }
        }
    }
    *old = new;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diff_emits_changes_and_resets() {
        let mut old = vec![
            (Prop::BackgroundColor, PropValue::Color(1)),
            (Prop::Opacity, PropValue::F32(0.5)),
        ];
        let new = vec![
            (Prop::BackgroundColor, PropValue::Color(2)),
            (Prop::A11yLabel, PropValue::Str("x".into())),
        ];
        let mut seen = Vec::new();
        diff(&mut old, new.clone(), |p, v| seen.push((p, v)));
        assert_eq!(
            seen,
            vec![
                (Prop::BackgroundColor, PropValue::Color(2)),
                (Prop::Opacity, PropValue::Null),
                (Prop::A11yLabel, PropValue::Str("x".into())),
            ]
        );
        assert_eq!(old, new);
    }

    #[test]
    fn explicit_props_override_derived_ones() {
        let mut v = vec![
            (Prop::Opacity, PropValue::F32(0.5)),
            (Prop::BackgroundColor, PropValue::Color(1)),
            (Prop::Opacity, PropValue::F32(0.2)),
        ];
        sort_last_wins(&mut v);
        assert_eq!(
            v,
            vec![
                (Prop::BackgroundColor, PropValue::Color(1)),
                (Prop::Opacity, PropValue::F32(0.2))
            ]
        );
    }

    #[test]
    fn text_value_round_trips() {
        assert_eq!(
            text_of(&PropValue::Json(text_value_json("a\"b", 3))),
            Some("a\"b".into())
        );
        assert_eq!(text_value_json("hi", 2), "{\"text\":\"hi\",\"count\":2}");
    }
}
