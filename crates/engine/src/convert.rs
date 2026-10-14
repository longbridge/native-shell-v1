//! Render output (the prelude's element records) → [`Desc`].
//!
//! The walk reads only fields every element sets in its constructor, so it
//! never trips the prelude's unknown-name guard. Style chains are interned by
//! the key the prelude computed while recording: a chain seen before costs one
//! hash lookup and its ops are never read out of JS again; theme changes clear
//! the cache. Consecutive text children (strings, numbers, `span`, `link`)
//! merge into one text leaf, the way gpui lays out inline text.

use std::rc::Rc;

use native_shell_core::desc::{Desc, ElementKind, ListSpec, TextRun};
use native_shell_core::protocol::{A11yRole, EventType, Prop, PropValue};
use native_shell_core::style::{Arg, Resolved, StyleCache, StyleOp, TextStyle};
use native_shell_core::theme::Theme;
use rquickjs::{Array, Object, Value};

use crate::assets::Assets;

type Result<T> = std::result::Result<T, String>;

pub(crate) struct Converter<'a> {
    pub styles: &'a mut StyleCache,
    /// The program's own files, for `svg(path)` and `image(path)`.
    pub assets: &'a mut Assets,
    /// Problems worth a warning that do not fail the render (a missing icon).
    pub warnings: Vec<String>,
    pub theme: &'a Theme,
    pub base: Rc<Resolved>,
    /// Ids of the lists met, so their windows can be tracked and their owner re-rendered.
    pub lists: Vec<String>,
    /// Description nodes built.
    pub nodes: u64,
    /// Keyed controls with a native and a drawn form, and how each came out.
    pub dual: Vec<DualForm>,
}

/// What every text leaf of a text view carries.
struct TextLeaf {
    selectable: bool,
    link: Option<u32>,
}

enum Child {
    Run(TextRun),
    Node(Desc),
}

#[derive(Clone, Copy)]
enum Kind {
    Str,
    Bool,
    I32,
    F32,
    Color,
    Json,
    F32Array,
}

/// Script prop name → protocol prop and value encoding.
fn prop_spec(name: &str) -> Option<(Prop, Kind)> {
    use Kind::*;
    Some(match name {
        "test_id" => (Prop::TestId, Str),
        "a11y_label" => (Prop::A11yLabel, Str),
        "a11y_hint" => (Prop::A11yHint, Str),
        "a11y_value" => (Prop::A11yValue, Str),
        "a11y_hidden" => (Prop::A11yHidden, Bool),
        "a11y_state" => (Prop::A11yState, Json),
        "role" => (Prop::A11yRole, I32),
        "hit_slop" => (Prop::HitSlop, F32Array),
        "placeholder" => (Prop::Placeholder, Str),
        "placeholder_color" => (Prop::PlaceholderColor, Color),
        "secure" => (Prop::Secure, Bool),
        "keyboard" => (Prop::KeyboardType, I32),
        "max_length" => (Prop::MaxLength, I32),
        "allowed_chars" => (Prop::AllowedChars, Str),
        "return_key" => (Prop::ReturnKey, I32),
        "content_type" => (Prop::ContentType, I32),
        "auto_capitalize" => (Prop::AutoCapitalize, I32),
        "editable" => (Prop::Editable, Bool),
        "multiline" => (Prop::Multiline, Bool),
        "text_value" => (Prop::TextValue, Json),
        "checked" => (Prop::Checked, Bool),
        "enabled" => (Prop::Enabled, Bool),
        "check_style" => (Prop::CheckStyle, I32),
        "value" => (Prop::Value, F32),
        "min" => (Prop::Min, F32),
        "max" => (Prop::Max, F32),
        "step" => (Prop::Step, F32),
        "indeterminate" => (Prop::Indeterminate, Bool),
        "items" => (Prop::Items, Json),
        "selected_index" => (Prop::SelectedIndex, I32),
        "date" => (Prop::Date, Str),
        "date_mode" => (Prop::DateMode, I32),
        "menu" => (Prop::Menu, Json),
        "path" => (Prop::PathData, Str),
        "view_box" => (Prop::ViewBox, F32Array),
        "fill" => (Prop::FillColor, Color),
        "stroke" => (Prop::StrokeColor, Color),
        "stroke_width" => (Prop::StrokeWidth, F32),
        "tooltip" => (Prop::Tooltip, Str),
        "list_alignment" => (Prop::ListAlignment, I32),
        "fill_rule" => (Prop::FillRule, I32),
        "src" => (Prop::ImageSource, Str),
        "content_mode" => (Prop::ContentMode, I32),
        "tint" => (Prop::TintColor, Color),
        "refreshing" => (Prop::Refreshing, Bool),
        "refresh_enabled" => (Prop::RefreshEnabled, Bool),
        "host_props" => (Prop::HostProps, Json),
        "selectable" => (Prop::Selectable, Bool),
        _ => return None,
    })
}

/// Props any view may carry (accessibility, testing, touch area).
fn is_generic(p: Prop) -> bool {
    matches!(
        p,
        Prop::TestId
            | Prop::A11yLabel
            | Prop::A11yHint
            | Prop::A11yValue
            | Prop::A11yHidden
            | Prop::A11yState
            | Prop::A11yRole
            | Prop::HitSlop
    )
}

/// `TextField` → `text_field`.
fn snake(name: &str) -> String {
    let mut out = String::new();
    for (i, c) in name.chars().enumerate() {
        if c.is_ascii_uppercase() {
            if i > 0 {
                out.push('_');
            }
            out.push(c.to_ascii_lowercase());
        } else {
            out.push(c);
        }
    }
    out
}

fn role(name: &str) -> Option<i32> {
    A11yRole::ALL.iter().find(|r| snake(r.name()) == name).map(|r| *r as i32)
}

fn field<'js>(obj: &Object<'js>, name: &str) -> Result<Value<'js>> {
    obj.get(name).map_err(|e| format!("reading `{name}` of an element: {e}"))
}

/// A switch, checkbox, radio or progress with a key, as converted.
pub(crate) struct DualForm {
    pub key: String,
    pub drawn: bool,
    /// Drawn but with no children to show its state.
    pub empty: bool,
}

/// Whether a style draws a box of its own (background, border, radius,
/// shadow, padding, text style) rather than only placing the element.
/// Opacity does not count: dimming a native control is not drawing it.
fn draws_a_box(style: &Resolved, base: &Resolved) -> bool {
    let (v, b) = (&style.visual, &base.visual);
    v.background != b.background
        || v.gradient != b.gradient
        || v.radii != b.radii
        || v.border_color != b.border_color
        || !v.shadows.is_empty()
        || style.border_widths != base.border_widths
        || style.layout.padding != base.layout.padding
        || style.text != base.text
}

/// A remote image (or a data URL), passed to renderers as is.
fn is_url(src: &str) -> bool {
    src.contains("://") || src.starts_with("data:")
}

fn string_field(obj: &Object<'_>, name: &str) -> Result<Option<String>> {
    let v = field(obj, name)?;
    if v.is_null() || v.is_undefined() {
        return Ok(None);
    }
    js_string(&v).map(Some).ok_or_else(|| format!("element field `{name}` must be a string"))
}

fn js_string(v: &Value<'_>) -> Option<String> {
    v.as_string().and_then(|s| s.to_string().ok())
}

fn number(v: &Value<'_>) -> Option<f64> {
    v.as_number().or_else(|| v.as_int().map(f64::from))
}

/// An array field; `None` when absent (`null`, the prototype default).
fn array<'js>(obj: &Object<'js>, name: &str) -> Result<Option<Array<'js>>> {
    let v = field(obj, name)?;
    if v.is_null() || v.is_undefined() {
        return Ok(None);
    }
    v.into_array().map(Some).ok_or_else(|| format!("element field `{name}` must be an array"))
}

/// The items of an optional array field.
fn items<'js, T: rquickjs::FromJs<'js>>(obj: &Object<'js>, name: &str) -> Result<Vec<T>> {
    match array(obj, name)? {
        Some(a) => {
            a.iter::<T>().collect::<rquickjs::Result<Vec<T>>>().map_err(|e| format!("element field `{name}`: {e}"))
        }
        None => Ok(Vec::new()),
    }
}

const RS: char = '\u{1e}';
const US: char = '\u{1f}';
const GS: char = '\u{1d}';
const FS: char = '\u{1c}';

/// Decodes a style chain as `js/elements.js` records it: each op is `RS name`
/// followed by `US type value` per argument (`n` number, `s` string, `b`
/// boolean); `.active(...)` is `RS @active GS inner FS`, `.hover(...)` likewise.
fn parse_chain(key: &str) -> Result<Vec<StyleOp>> {
    let mut ops = Vec::new();
    let mut rest = key;
    while let Some(after) = rest.strip_prefix(RS) {
        // `.active(...)` / `.hover(...)`: `@state GS inner FS`.
        let state = after.strip_prefix('@').and_then(|r| r.split_once(GS));
        if let Some((state, inner)) = state {
            // Find the matching FS, counting nested states.
            let mut depth = 1;
            let mut end = None;
            for (i, c) in inner.char_indices() {
                match c {
                    GS => depth += 1,
                    FS => {
                        depth -= 1;
                        if depth == 0 {
                            end = Some(i);
                            break;
                        }
                    }
                    _ => {}
                }
            }
            let end = end.ok_or_else(|| format!("malformed `.{state}(...)` style chain"))?;
            let mut op = StyleOp::new(state, Vec::new());
            op.nested = Some((state.into(), parse_chain(&inner[..end])?));
            ops.push(op);
            rest = &inner[end + FS.len_utf8()..];
            continue;
        }
        let end = after.find([RS, FS, GS]).unwrap_or(after.len());
        let mut parts = after[..end].split(US);
        let name = parts.next().unwrap_or_default();
        let mut args = Vec::new();
        for part in parts {
            let mut chars = part.chars();
            let tag = chars.next();
            let value = chars.as_str();
            args.push(match tag {
                Some('n') => Arg::Num(value.parse().map_err(|_| format!("`{name}`: bad number `{value}`"))?),
                Some('s') => Arg::Str(value.to_string()),
                Some('b') => Arg::Bool(value == "1"),
                _ => return Err(format!("`{name}`: malformed style argument")),
            });
        }
        ops.push(StyleOp::new(name, args));
        rest = &after[end..];
    }
    if !rest.is_empty() {
        return Err("malformed style chain".into());
    }
    Ok(ops)
}

impl Converter<'_> {
    /// What a `render(cx)` (or overlay content function) returned.
    pub fn root(&mut self, v: Value<'_>) -> Result<Desc> {
        if v.is_null() || v.is_undefined() {
            return Err("render() must return an element".into());
        }
        let mut children = Vec::new();
        self.child(v, &mut children)?;
        let mut out = self.flush(children)?;
        match out.len() {
            1 => Ok(out.remove(0)),
            _ => Err("render() must return a single element".into()),
        }
    }

    fn style(&mut self, obj: &Object<'_>) -> Result<Rc<Resolved>> {
        let sk = string_field(obj, "_sk")?.unwrap_or_default();
        let pk = string_field(obj, "_pk")?.unwrap_or_default();
        if sk.is_empty() && pk.is_empty() {
            return Ok(self.base.clone());
        }
        // The preset applies first; the encoded chains concatenate.
        let key = if pk.is_empty() { sk } else { pk + &sk };
        if let Some(r) = self.styles.get(&key) {
            return Ok(r);
        }
        let ops = parse_chain(&key)?;
        self.styles.resolve(&key, &ops, self.theme).map_err(|e| e.0)
    }

    fn props(&self, obj: &Object<'_>, desc: &mut Desc) -> Result<()> {
        let p = field(obj, "_p")?;
        if p.is_null() || p.is_undefined() {
            return Ok(());
        }
        let p: Object<'_> = p.into_object().ok_or("element field `p` must be an object")?;
        for entry in p.props::<String, Value<'_>>() {
            let (name, v) = entry.map_err(|e| e.to_string())?;
            if v.is_undefined() || v.is_null() {
                continue;
            }
            let (prop, kind) = prop_spec(&name).ok_or_else(|| format!("unknown element prop `{name}`"))?;
            let bad = || format!("prop `{name}` has the wrong type");
            let value = match kind {
                Kind::Str => PropValue::Str(js_string(&v).ok_or_else(bad)?),
                Kind::Json => PropValue::Json(js_string(&v).ok_or_else(bad)?),
                Kind::Bool => PropValue::Bool(v.as_bool().ok_or_else(bad)?),
                Kind::F32 => PropValue::F32(number(&v).ok_or_else(bad)? as f32),
                Kind::I32 if name == "role" => {
                    let r = js_string(&v).ok_or_else(bad)?;
                    PropValue::I32(role(&r).ok_or_else(|| format!("unknown role `{r}`"))?)
                }
                Kind::I32 => PropValue::I32(number(&v).ok_or_else(bad)? as i32),
                Kind::Color => {
                    let s = js_string(&v).ok_or_else(bad)?;
                    PropValue::Color(self.theme.color(&s).ok_or_else(|| format!("`{name}`: unknown color `{s}`"))?)
                }
                Kind::F32Array => {
                    let a = v.into_array().ok_or_else(bad)?;
                    let mut out = Vec::with_capacity(a.len());
                    for x in a.iter::<f64>() {
                        out.push(x.map_err(|_| bad())? as f32);
                    }
                    PropValue::F32Array(out)
                }
            };
            desc.props.push((prop, value));
        }
        desc.props.sort_by_key(|(p, _)| *p as u16);
        Ok(())
    }

    fn handlers(obj: &Object<'_>, desc: &mut Desc) -> Result<()> {
        if field(obj, "_dis")?.as_bool() == Some(true) {
            return Ok(());
        }
        let Some(h) = array(obj, "_h")? else { return Ok(()) };
        for i in (0..h.len()).step_by(2) {
            let name: String = h.get(i).map_err(|e| e.to_string())?;
            let id: u32 = h.get(i + 1).map_err(|e| e.to_string())?;
            let event = EventType::from_script_name(&name).ok_or_else(|| format!("unknown event `{name}`"))?;
            // The last handler registered for an event wins.
            match desc.handlers.iter_mut().find(|(e, _)| *e == event) {
                Some(slot) => slot.1 = id,
                None => desc.handlers.push((event, id)),
            }
        }
        Ok(())
    }

    fn node(&mut self, kind: ElementKind, obj: &Object<'_>) -> Result<Desc> {
        let mut d = Desc::new(kind, self.style(obj)?);
        d.key = string_field(obj, "_k")?;
        d.ref_id = string_field(obj, "_rid")?;
        self.props(obj, &mut d)?;
        Self::handlers(obj, &mut d)?;
        self.nodes += 1;
        Ok(d)
    }

    fn element(&mut self, obj: &Object<'_>, el: &str) -> Result<Desc> {
        // Native leaves draw everything themselves: their children (gpui-base's
        // slider parts, for one) are not converted.
        let leaf = match el {
            "view" => {
                let mut d = self.node(ElementKind::View, obj)?;
                d.children = self.children(obj)?;
                return Ok(d);
            }
            "text" => {
                let runs = self.runs(obj)?;
                let mut d = self.node(ElementKind::Text(runs), obj)?;
                links_enabled(&mut d);
                return Ok(d);
            }
            "switch" | "checkbox" | "progress" => {
                // Decided by how the element was built — a `.child`/`.children`
                // call, `.headless()`, or a style that draws a box — not by this
                // render's children, so the view kind stays put while the
                // content changes. Layout-only styles (margin, size, flex,
                // position) lay out the native control instead.
                let children = array(obj, "_c")?.is_some_and(|c| !c.is_empty());
                let asked = children || field(obj, "_hl")?.as_bool().unwrap_or(false);
                let drawn = asked || draws_a_box(&*self.style(obj)?, &self.base);
                if let Some(key) = string_field(obj, "_k")? {
                    // A drawn control with nothing inside shows no state (a
                    // drawn checkbox's mark is its children).
                    let checked = match field(obj, "_p")?.into_object() {
                        Some(p) => p.get::<_, Option<bool>>("checked").ok().flatten().unwrap_or(false),
                        None => false,
                    };
                    let empty = drawn && !children && (el == "progress" || checked);
                    self.dual.push(DualForm { key, drawn, empty });
                }
                return match (drawn, el) {
                    (true, _) => self.drawn(obj, el),
                    (false, "progress") => self.node(ElementKind::Progress, obj).map(|mut d| {
                        d.handlers.retain(|(e, _)| *e != EventType::Click);
                        d
                    }),
                    (false, _) => self.toggle(obj, el),
                };
            }
            "svg" => return self.svg(obj),
            "image" => return self.image(obj),
            "textview" => return self.textview(obj),
            "input" => ElementKind::TextInput,
            "slider" => ElementKind::Slider,
            "spinner" => ElementKind::Spinner,
            "segmented" => ElementKind::Segmented,
            "select" => ElementKind::Select,
            "date" => ElementKind::DatePicker,
            "shape" => ElementKind::Shape,
            "host" => ElementKind::HostComponent(string_field(obj, "_hn")?.ok_or("host component without a name")?),
            "list" => return self.list(obj),
            "row" => return Err("list rows can only appear inside a list".into()),
            "style" => return Err("the element passed to .active()/.hover() cannot be rendered".into()),
            other => return Err(format!("unknown element kind `{other}`")),
        };
        let mut d = self.node(leaf, obj)?;
        // A native control reports through its own events; taps go to the control.
        d.handlers.retain(|(e, _)| {
            *e != EventType::Click || matches!(d.kind, ElementKind::Image | ElementKind::HostComponent(_))
        });
        Ok(d)
    }

    /// A native switch or checkbox; with `.label(s)` it becomes a row of the control and the label.
    fn toggle(&mut self, obj: &Object<'_>, el: &str) -> Result<Desc> {
        let kind = if el == "switch" { ElementKind::Switch } else { ElementKind::Checkbox };
        let mut control = self.node(kind, obj)?;
        control.handlers.retain(|(e, _)| *e != EventType::Click);
        let Some(label) = string_field(obj, "_lbl")? else {
            return Ok(control);
        };
        // The element's style (the label preset plus the script's chain) sizes the row.
        let mut row = Desc::new(ElementKind::View, std::mem::replace(&mut control.style, self.base.clone()));
        row.key = control.key.take();
        let text = Desc::new(ElementKind::Text(vec![run(label)]), self.base.clone());
        row.children = vec![control, text];
        self.nodes += 2;
        Ok(row)
    }

    /// A toggle or progress the script draws itself (gpui-base's headless form): a
    /// view that announces the control's role and state; toggles flip on a tap.
    fn drawn(&mut self, obj: &Object<'_>, el: &str) -> Result<Desc> {
        let mut d = self.node(ElementKind::View, obj)?;
        let checked = d.prop(Prop::Checked) == Some(&PropValue::Bool(true));
        let role = match (el, d.prop(Prop::CheckStyle)) {
            ("switch", _) => Some(A11yRole::Switch),
            ("checkbox", Some(PropValue::I32(1))) => Some(A11yRole::Radio),
            ("checkbox", _) => Some(A11yRole::Checkbox),
            _ => None,
        };
        d.props.retain(|(p, _)| is_generic(*p));
        if let Some(role) = role {
            d.props.retain(|(p, _)| *p != Prop::A11yRole);
            d.props.push((Prop::A11yRole, PropValue::I32(role as i32)));
            let mut state: serde_json::Map<String, serde_json::Value> = d
                .prop(Prop::A11yState)
                .and_then(|v| match v {
                    PropValue::Json(j) => serde_json::from_str(j).ok(),
                    _ => None,
                })
                .unwrap_or_default();
            if checked {
                state.insert("checked".into(), serde_json::Value::Bool(true));
            }
            d.props.retain(|(p, _)| *p != Prop::A11yState);
            if !state.is_empty() {
                d.props.push((Prop::A11yState, PropValue::Json(serde_json::Value::Object(state).to_string())));
            }
            d.handlers.retain(|(e, _)| *e != EventType::Change);
        }
        d.props.sort_by_key(|(p, _)| *p as u16);
        d.children = self.children(obj)?;
        Ok(d)
    }

    /// `svg(path)`: the program's SVG drawn as `Shape` path data in one color, as
    /// gpui draws an svg: the element's own text color, else the theme's
    /// foreground. Paths with different paints (filled, stroked) become sibling
    /// shapes over the element's box. A remote SVG stays an image, tinted.
    fn svg(&mut self, obj: &Object<'_>) -> Result<Desc> {
        let mut d = self.node(ElementKind::Image, obj)?;
        let color = match d.prop(Prop::TintColor) {
            Some(PropValue::Color(c)) => *c,
            _ => d.style.text.color.or_else(|| self.theme.color("foreground")).unwrap_or(0xFF00_0000),
        };
        let src = match d.prop(Prop::ImageSource) {
            Some(PropValue::Str(s)) => s.clone(),
            _ => String::new(),
        };
        if is_url(&src) {
            d.props.retain(|(p, _)| *p != Prop::TintColor);
            d.props.push((Prop::TintColor, PropValue::Color(color)));
            d.props.sort_by_key(|(p, _)| *p as u16);
            return Ok(d);
        }
        d.props.retain(|(p, _)| !matches!(p, Prop::ImageSource | Prop::TintColor | Prop::ContentMode));
        d.kind = ElementKind::View;
        let icon = self.assets.icon(&src);
        let icon = match &*icon {
            Ok(icon) => icon,
            Err(e) => {
                if self.assets.first_report(&src) {
                    self.warnings.push(format!("svg(\"{src}\"): {e}; nothing is drawn"));
                }
                return Ok(d);
            }
        };
        let paint = |layer: &crate::svg::Layer, props: &mut Vec<(Prop, PropValue)>| {
            props.push((Prop::PathData, PropValue::Str(layer.path.clone())));
            if layer.fill {
                props.push((Prop::FillColor, PropValue::Color(color)));
                if layer.even_odd {
                    props.push((Prop::FillRule, PropValue::I32(1)));
                }
            }
            if let Some(width) = layer.stroke {
                props.push((Prop::StrokeColor, PropValue::Color(color)));
                props.push((Prop::StrokeWidth, PropValue::F32(width)));
            }
            props.push((Prop::ViewBox, PropValue::F32Array(icon.view_box.to_vec())));
            props.sort_by_key(|(p, _)| *p as u16);
        };
        match icon.layers.as_slice() {
            [] => {}
            [layer] => {
                d.kind = ElementKind::Shape;
                paint(layer, &mut d.props);
            }
            layers => {
                let overlay = self.overlay_style()?;
                for layer in layers {
                    let mut shape = Desc::new(ElementKind::Shape, overlay.clone());
                    paint(layer, &mut shape.props);
                    d.children.push(shape);
                    self.nodes += 1;
                }
            }
        }
        Ok(d)
    }

    /// Fills the parent's box: the style of each layer of a multi-paint svg.
    fn overlay_style(&mut self) -> Result<Rc<Resolved>> {
        const KEY: &str = "\u{1e}absolute\u{1e}inset_0";
        if let Some(r) = self.styles.get(KEY) {
            return Ok(r);
        }
        let ops = parse_chain(KEY)?;
        self.styles.resolve(KEY, &ops, self.theme).map_err(|e| e.0)
    }

    /// `image(src)`: a URL as is; a program file as the absolute path of its copy on disk.
    fn image(&mut self, obj: &Object<'_>) -> Result<Desc> {
        let mut d = self.node(ElementKind::Image, obj)?;
        let Some(i) =
            d.props.iter().position(|(p, v)| *p == Prop::ImageSource && matches!(v, PropValue::Str(s) if !is_url(s)))
        else {
            return Ok(d);
        };
        let PropValue::Str(src) = d.props.remove(i).1 else { return Ok(d) };
        match self.assets.file(&src) {
            Ok(path) => d.props.insert(i, (Prop::ImageSource, PropValue::Str(path))),
            Err(e) => {
                if self.assets.first_report(&src) {
                    self.warnings.push(format!("image(\"{src}\"): {e}; nothing is drawn"));
                }
            }
        }
        Ok(d)
    }

    fn list(&mut self, obj: &Object<'_>) -> Result<Desc> {
        let id = string_field(obj, "_lid")?.ok_or("list without an id")?;
        let count = number(&field(obj, "_count")?).unwrap_or(0.0).max(0.0) as u32;
        let est = number(&field(obj, "_est")?).unwrap_or(44.0) as f32;
        let uniform = field(obj, "_uniform")?.as_bool().unwrap_or(false);
        self.lists.push(id.clone());
        let mut d = self.node(ElementKind::List(ListSpec { id, count, estimated_row_height: est, uniform }), obj)?;
        for row in items::<Object<'_>>(obj, "_c")? {
            let index = number(&field(&row, "_idx")?).ok_or("list row without an index")? as u32;
            let mut r = Desc::new(ElementKind::ListRow(index), self.base.clone());
            r.key = string_field(&row, "_k")?;
            r.children = self.children(&row)?;
            self.nodes += 1;
            d.children.push(r);
        }
        Ok(d)
    }

    /// The runs of a `text(…)` element: strings, numbers and spans only.
    fn runs(&mut self, obj: &Object<'_>) -> Result<Vec<TextRun>> {
        let mut runs = Vec::new();
        for v in items::<Value<'_>>(obj, "_c")? {
            match self.classify(v)? {
                Child::Run(r) => runs.push(r),
                Child::Node(_) => return Err("text() can only contain strings, numbers and span()s".into()),
            }
        }
        Ok(runs)
    }

    fn span(&mut self, obj: &Object<'_>) -> Result<TextRun> {
        let style = self.style(obj)?;
        let mut text = String::new();
        for v in items::<Value<'_>>(obj, "_c")? {
            text.push_str(&js_string(&v).ok_or("span() content must be text")?);
        }
        Ok(TextRun { text, style: style.text.clone(), url: string_field(obj, "_url")? })
    }

    fn classify(&mut self, v: Value<'_>) -> Result<Child> {
        if let Some(s) = js_string(&v) {
            return Ok(Child::Run(run(s)));
        }
        if let Some(n) = number(&v) {
            return Ok(Child::Run(run(n.to_string())));
        }
        let obj = v.into_object().ok_or("a child must be an element, an entity, a string or a number")?;
        let el = field(&obj, "__el")?;
        if let Some(el) = js_string(&el) {
            if el == "span" {
                return Ok(Child::Run(self.span(&obj)?));
            }
            return Ok(Child::Node(self.element(&obj, &el)?));
        }
        let entity = field(&obj, "__entity")?;
        if let Some(id) = number(&entity) {
            self.nodes += 1;
            return Ok(Child::Node(Desc::new(ElementKind::Entity(id as u32), self.base.clone())));
        }
        Err("a child must be an element, an entity, a string or a number".into())
    }

    fn child(&mut self, v: Value<'_>, out: &mut Vec<Child>) -> Result<()> {
        out.push(self.classify(v)?);
        Ok(())
    }

    /// Children in order, consecutive runs merged into one text leaf.
    fn flush(&mut self, children: Vec<Child>) -> Result<Vec<Desc>> {
        let mut out = Vec::with_capacity(children.len());
        let mut runs: Vec<TextRun> = Vec::new();
        for c in children {
            match c {
                Child::Run(r) => runs.push(r),
                Child::Node(d) => {
                    if !runs.is_empty() {
                        out.push(self.text_leaf(std::mem::take(&mut runs)));
                    }
                    out.push(d);
                }
            }
        }
        if !runs.is_empty() {
            out.push(self.text_leaf(runs));
        }
        Ok(out)
    }

    fn text_leaf(&mut self, runs: Vec<TextRun>) -> Desc {
        self.nodes += 1;
        let mut d = Desc::new(ElementKind::Text(runs), self.base.clone());
        links_enabled(&mut d);
        d
    }

    /// Children in order: the `_before` slots, `_c`, then the `_after` slots
    /// (gpui-base's `header`, `content`, `panel`… placed around ordinary children).
    fn children(&mut self, obj: &Object<'_>) -> Result<Vec<Desc>> {
        let mut children = Vec::new();
        for field in ["_before", "_c", "_after"] {
            let Some(list) = array(obj, field)? else { continue };
            children.reserve(list.len());
            for v in list.iter::<Value<'_>>() {
                let v = v.map_err(|e| e.to_string())?;
                if v.is_null() || v.is_undefined() {
                    continue;
                }
                self.child(v, &mut children)?;
            }
        }
        self.flush(children)
    }

    /// Resolves an engine-made chain (text view layout), interned like a script's.
    fn engine_style(&mut self, ops: &[(&str, Option<Arg>)]) -> Result<Rc<Resolved>> {
        let ops: Vec<StyleOp> = ops.iter().map(|(n, a)| StyleOp::new(n, a.iter().cloned().collect())).collect();
        let key = format!("tv:{}", native_shell_core::style::chain_key(&ops));
        if let Some(r) = self.styles.get(&key) {
            return Ok(r);
        }
        self.styles.resolve(&key, &ops, self.theme).map_err(|e| e.0)
    }

    /// `TextView.markdown(...)` / `.html(...)`: a column of blocks.
    fn textview(&mut self, obj: &Object<'_>) -> Result<Desc> {
        let source = string_field(obj, "_src")?.unwrap_or_default();
        let blocks = match string_field(obj, "_tv")?.as_deref() {
            Some("html") => crate::textview::html(&source),
            _ => crate::textview::markdown(&source),
        };
        let mut d = self.node(ElementKind::View, obj)?;
        // Text props and the link handler go to the text leaves, not the column.
        let selectable = d.prop(Prop::Selectable) == Some(&PropValue::Bool(true));
        d.props.retain(|(p, _)| *p != Prop::Selectable);
        let link = d.handler(EventType::Link);
        d.handlers.retain(|(e, _)| *e != EventType::Link);
        let column = self.engine_style(&[("flex", None), ("flex_col", None), ("gap", Some(Arg::Num(8.0)))])?;
        let mut inner = Desc::new(ElementKind::View, column);
        let leaf = TextLeaf { selectable, link };
        for b in &blocks {
            let block = self.block(b, &leaf)?;
            inner.children.push(block);
        }
        d.children = vec![inner];
        Ok(d)
    }

    fn block(&mut self, b: &crate::textview::Block, leaf: &TextLeaf) -> Result<Desc> {
        use crate::textview::Block;
        self.nodes += 1;
        Ok(match b {
            Block::Paragraph(spans) => self.text_block(spans, None, leaf),
            Block::Heading(level, spans) => {
                let size = [28.0, 24.0, 20.0, 18.0, 16.0, 16.0][usize::from(level.saturating_sub(1)).min(5)];
                self.text_block(spans, Some(size), leaf)
            }
            Block::Code(code) => {
                let style = self.engine_style(&[
                    ("p", Some(Arg::Num(12.0))),
                    ("rounded", Some(Arg::Num(6.0))),
                    ("bg", Some(Arg::Str("muted".into()))),
                ])?;
                let mut d = Desc::new(ElementKind::View, style);
                let span = crate::textview::Span {
                    text: code.trim_end_matches('\n').to_string(),
                    code: true,
                    ..Default::default()
                };
                d.children.push(self.text_block(std::slice::from_ref(&span), None, leaf));
                d
            }
            Block::Quote(inner) => {
                let style = self.engine_style(&[
                    ("flex", None),
                    ("flex_col", None),
                    ("gap", Some(Arg::Num(8.0))),
                    ("pl", Some(Arg::Num(12.0))),
                    ("border_l", Some(Arg::Num(3.0))),
                    ("border_color", Some(Arg::Str("border".into()))),
                    ("text_color", Some(Arg::Str("muted_foreground".into()))),
                ])?;
                let mut d = Desc::new(ElementKind::View, style);
                for b in inner {
                    d.children.push(self.block(b, leaf)?);
                }
                d
            }
            Block::List { ordered, start, items } => {
                let column = self.engine_style(&[("flex", None), ("flex_col", None), ("gap", Some(Arg::Num(4.0)))])?;
                let row = self.engine_style(&[("flex", None), ("flex_row", None), ("gap", Some(Arg::Num(8.0)))])?;
                // The item's text takes what the marker leaves and wraps there, rather than overflowing the view.
                let body_style = self.engine_style(&[
                    ("flex", None),
                    ("flex_col", None),
                    ("gap", Some(Arg::Num(4.0))),
                    ("flex_1", None),
                    ("min_w", Some(Arg::Num(0.0))),
                ])?;
                let mut d = Desc::new(ElementKind::View, column);
                for (i, item) in items.iter().enumerate() {
                    let marker = if *ordered { format!("{}.", start + i as u64) } else { "•".to_string() };
                    let mut r = Desc::new(ElementKind::View, row.clone());
                    r.children.push(Desc::new(ElementKind::Text(vec![run(marker)]), self.base.clone()));
                    let mut body = Desc::new(ElementKind::View, body_style.clone());
                    for b in item {
                        body.children.push(self.block(b, leaf)?);
                    }
                    r.children.push(body);
                    d.children.push(r);
                }
                d
            }
            Block::Rule => {
                let style =
                    self.engine_style(&[("h", Some(Arg::Num(1.0))), ("bg", Some(Arg::Str("border".into())))])?;
                Desc::new(ElementKind::View, style)
            }
        })
    }

    fn text_block(&mut self, spans: &[crate::textview::Span], size: Option<f32>, leaf: &TextLeaf) -> Desc {
        let runs = spans
            .iter()
            .map(|s| TextRun {
                text: s.text.clone(),
                style: TextStyle {
                    size: size.or(s.code.then_some(14.0)),
                    weight: (s.bold || size.is_some()).then_some(700),
                    italic: s.italic.then_some(true),
                    strike: s.strike.then_some(true),
                    underline: s.url.is_some().then_some(true),
                    color: s.url.as_ref().and_then(|_| self.theme.color("primary")),
                    family: s.code.then(|| "monospace".to_string()),
                    ..TextStyle::default()
                },
                url: s.url.clone(),
            })
            .collect();
        let mut d = Desc::new(ElementKind::Text(runs), self.base.clone());
        if leaf.selectable {
            d.props.push((Prop::Selectable, PropValue::Bool(true)));
        }
        if let Some(h) = leaf.link {
            d.handlers.push((EventType::Link, h));
        }
        links_enabled(&mut d);
        d
    }
}

fn run(text: String) -> TextRun {
    TextRun { text, style: TextStyle::default(), url: None }
}

fn links_enabled(d: &mut Desc) {
    if let ElementKind::Text(runs) = &d.kind {
        if runs.iter().any(|r| r.url.is_some()) && d.prop(Prop::LinksEnabled).is_none() {
            d.props.push((Prop::LinksEnabled, PropValue::Bool(true)));
            d.props.sort_by_key(|(p, _)| *p as u16);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chains_decode() {
        let key = "\u{1e}flex\u{1e}px\u{1f}n8\u{1e}bg\u{1f}sprimary\u{1e}@active\u{1d}\u{1e}opacity\u{1f}n0.5\u{1c}\u{1e}shadow\u{1f}s#000\u{1f}n0\u{1f}n1\u{1f}n2";
        let ops = parse_chain(key).unwrap();
        assert_eq!(ops.len(), 5);
        assert_eq!(ops[1], StyleOp::new("px", vec![Arg::Num(8.0)]));
        assert_eq!(ops[2], StyleOp::new("bg", vec![Arg::Str("primary".into())]));
        assert_eq!(ops[3].nested.as_ref().unwrap().1, vec![StyleOp::new("opacity", vec![Arg::Num(0.5)])]);
        assert_eq!(ops[4].args.len(), 4);
        assert!(parse_chain("junk").is_err());
    }

    #[test]
    fn roles_map_from_snake_case() {
        assert_eq!(role("button"), Some(A11yRole::Button as i32));
        assert_eq!(role("text_field"), Some(A11yRole::TextField as i32));
        assert_eq!(role("progress_bar"), Some(A11yRole::ProgressBar as i32));
        assert_eq!(role("nope"), None);
    }
}
