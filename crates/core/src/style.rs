//! Style methods: GPUI's `Styled` vocabulary and behaviour, resolved into a
//! taffy layout style plus the visual and text properties a native view needs.
//!
//! Scripts record a chain (`div().flex().flex_col().gap_2().bg("primary")`) as a
//! list of [`StyleOp`]s. The chain's canonical text is its intern key: the same
//! chain on the next render costs one hash lookup, not a parse. Parsing happens
//! once per distinct chain per theme.
//!
//! The surface is gpui-shell's, and it splits the same way:
//!
//! * **No-argument methods** (`flex_col`, `p_4`, `rounded_md`, `text_sm`, …) are
//!   the generated table in `style_table.rs`. `tools/style-gen` invokes every
//!   method GPUI's inspector reflection exposes on a `StyleRefinement` and
//!   records which fields it writes, as a list of [`Effect`]s; this module only
//!   applies effects. So `p_4` is exactly GPUI's `p_4` (1 rem), `flex_1` sets a
//!   0% basis, `truncate` hides overflow, and so on, without hand-kept tables.
//! * **Methods that take arguments** are gpui-shell's 59 ([`GPUI_PARAM`]), with
//!   its argument rules: a bare number is pixels, a string is `"12px"`,
//!   `"1.5rem"`, `"50%"` or `"auto"` where the GPUI type allows it, and
//!   `line_height(1.4)` is a multiple of the font size. After them come a few
//!   mobile additions ([`MOBILE_PARAM`]); where GPUI has a method of the same
//!   name, it keeps GPUI's meaning.
//!
//! Lengths are converted the way GPUI hands them to taffy: rems at 16 px,
//! fractions as percentages. GPUI also rounds layout lengths to device pixels;
//! the tree lays out in points and leaves that to the renderer.

use std::collections::HashMap;
use std::rc::Rc;

use serde::Serialize;
use taffy::prelude::*;
use taffy::style::{Overflow, Position as TaffyPosition};

use crate::protocol::CursorStyle;
use crate::theme::Theme;

#[rustfmt::skip]
#[path = "style_table.rs"]
mod table;

pub const REM: f32 = 16.0;

/// Upper bound of the layout scale: how far rem lengths grow with the user's text size
/// (Dynamic Type on iOS, font scale on Android). Rem is the zoom axis, as in GPUI: padding,
/// gaps, control and icon sizes given in rems grow with their labels so a layout keeps its
/// proportions. Text itself scales fully (renderers apply the whole font scale); layout is
/// damped at 1.6 so screens still fit at the accessibility sizes, which reach 3×.
pub const MAX_LAYOUT_SCALE: f32 = 1.6;

/// The effective rem size for a font scale: [`REM`] × the font scale clamped to
/// `1.0 ..= MAX_LAYOUT_SCALE` (layouts never shrink below their designed size).
pub fn rem_size(font_scale: f32) -> f32 {
    let scale = if font_scale.is_finite() { font_scale.clamp(1.0, MAX_LAYOUT_SCALE) } else { 1.0 };
    REM * scale
}

/// The layout lengths a chain gave in rems (their value in rems, by field), so the tree can
/// lay them out at the effective rem size: `layout` and `visual` hold them at [`REM`].
/// Text sizes are not here: they always resolve at [`REM`] and renderers scale them.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct RemLengths {
    /// `[top, right, bottom, left]`, as the `border_widths` order.
    pub padding: [Option<f32>; 4],
    pub margin: [Option<f32>; 4],
    pub inset: [Option<f32>; 4],
    pub border: [Option<f32>; 4],
    /// `[top-left, top-right, bottom-right, bottom-left]`, as [`Visual::radii`].
    pub radii: [Option<f32>; 4],
    /// `[width, height]`.
    pub gap: [Option<f32>; 2],
    pub size: [Option<f32>; 2],
    pub min_size: [Option<f32>; 2],
    pub max_size: [Option<f32>; 2],
    pub flex_basis: Option<f32>,
    pub scrollbar_width: Option<f32>,
}

impl RemLengths {
    pub fn is_empty(&self) -> bool {
        *self == RemLengths::default()
    }
}

/// GPUI's default font size (`rems(1.)`), which gpui-shell's root view also sets.
pub const DEFAULT_TEXT_SIZE: f32 = 16.0;

/// GPUI's default line height: φ times the font size (`gpui::phi()`).
pub const DEFAULT_LINE_HEIGHT: LineHeight = LineHeight::Multiple(1.618_034);

#[derive(Debug, Clone, PartialEq)]
pub enum Arg {
    Num(f64),
    Str(String),
    Bool(bool),
}

impl Arg {
    fn canonical(&self) -> String {
        match self {
            Arg::Num(n) => format!("{n}"),
            Arg::Str(s) => format!("{s:?}"),
            Arg::Bool(b) => format!("{b}"),
        }
    }

    /// How gpui-shell names a value in a type error.
    fn describe(&self) -> String {
        match self {
            Arg::Num(n) => format!("number ({n})"),
            Arg::Str(s) => format!("string (\"{s}\")"),
            Arg::Bool(b) => format!("boolean ({b})"),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct StyleOp {
    pub name: String,
    pub args: Vec<Arg>,
    /// `.active(s => …)`: ops that apply while pressed.
    pub nested: Option<(String, Vec<StyleOp>)>,
}

impl StyleOp {
    pub fn new(name: &str, args: Vec<Arg>) -> Self {
        StyleOp { name: name.to_string(), args, nested: None }
    }
}

pub fn chain_key(ops: &[StyleOp]) -> String {
    let mut key = String::new();
    for op in ops {
        if !key.is_empty() {
            key.push('|');
        }
        key.push_str(&op.name);
        if !op.args.is_empty() {
            key.push('(');
            key.push_str(&op.args.iter().map(Arg::canonical).collect::<Vec<_>>().join(","));
            key.push(')');
        }
        if let Some((state, inner)) = &op.nested {
            key.push_str(&format!("{state}{{{}}}", chain_key(inner)));
        }
    }
    key
}

/// One box-shadow layer. Colours are `0xAARRGGBB`.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize)]
pub struct Shadow {
    pub color: u32,
    pub x: f32,
    pub y: f32,
    pub blur: f32,
    pub spread: f32,
    /// Drawn inside the bounds. No GPUI preset uses it.
    #[serde(skip_serializing_if = "is_false")]
    pub inset: bool,
}

fn is_false(b: &bool) -> bool {
    !*b
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ScrollAxis {
    #[default]
    None,
    X,
    Y,
    Both,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Transition {
    pub property: String,
    pub kind: &'static str,
    pub duration: f32,
    pub delay: f32,
    pub easing: String,
    pub response: f32,
    pub damping: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum LineHeight {
    #[default]
    Normal,
    /// A multiple of the font size (GPUI's `DefiniteLength::Fraction`).
    Multiple(f32),
    Px(f32),
}

/// Inherited text properties. `None` means inherit from the parent.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct TextStyle {
    pub size: Option<f32>,
    pub weight: Option<u16>,
    pub color: Option<u32>,
    pub family: Option<String>,
    pub italic: Option<bool>,
    /// Whether an underline is drawn: set with a thickness above zero.
    pub underline: Option<bool>,
    pub underline_thickness: Option<f32>,
    pub underline_wavy: Option<bool>,
    /// Whether a strikethrough is drawn.
    pub strike: Option<bool>,
    pub line_height: Option<LineHeight>,
    pub align: Option<&'static str>,
    pub letter_spacing: Option<f32>,
    /// Overflowing text ends in an ellipsis (GPUI's `text_overflow`).
    pub truncate: Option<bool>,
    /// Where the ellipsis goes when [`TextStyle::truncate`] is set.
    pub truncate_at: Option<Truncation>,
    /// GPUI's `line_clamp`.
    pub lines: Option<u32>,
    /// GPUI's `white_space: Nowrap`.
    pub nowrap: Option<bool>,
    /// The background painted behind the text itself (`text_bg`).
    pub background: Option<u32>,
}

impl TextStyle {
    pub fn inherit(&self, parent: &TextStyle) -> TextStyle {
        TextStyle {
            size: self.size.or(parent.size),
            weight: self.weight.or(parent.weight),
            color: self.color.or(parent.color),
            family: self.family.clone().or_else(|| parent.family.clone()),
            italic: self.italic.or(parent.italic),
            underline: self.underline.or(parent.underline),
            underline_thickness: self.underline_thickness.or(parent.underline_thickness),
            underline_wavy: self.underline_wavy.or(parent.underline_wavy),
            strike: self.strike.or(parent.strike),
            line_height: self.line_height.or(parent.line_height),
            align: self.align.or(parent.align),
            letter_spacing: self.letter_spacing.or(parent.letter_spacing),
            truncate: self.truncate.or(parent.truncate),
            truncate_at: self.truncate_at.or(parent.truncate_at),
            lines: self.lines.or(parent.lines),
            nowrap: self.nowrap.or(parent.nowrap),
            background: self.background.or(parent.background),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Visual {
    pub background: Option<u32>,
    pub gradient: Option<(f32, Vec<(u32, f32)>)>,
    /// `[top-left, top-right, bottom-right, bottom-left]`.
    pub radii: [f32; 4],
    pub border_color: Option<u32>,
    pub border_dashed: bool,
    pub opacity: f32,
    /// The first of [`Visual::shadows`], for renderers that draw one layer.
    pub shadow: Option<Shadow>,
    /// Every box-shadow layer, bottom first (GPUI's `box_shadow`).
    pub shadows: Vec<Shadow>,
    /// Overflow is hidden on at least one axis.
    pub clip: bool,
    pub invisible: bool,
    pub transform: Option<[f32; 4]>,
}

impl Default for Visual {
    fn default() -> Self {
        Visual {
            background: None,
            gradient: None,
            radii: [0.0; 4],
            border_color: None,
            border_dashed: false,
            opacity: 1.0,
            shadow: None,
            shadows: Vec::new(),
            clip: false,
            invisible: false,
            transform: None,
        }
    }
}

impl Visual {
    /// Whether a view with this style draws anything of its own.
    pub fn paints(&self) -> bool {
        self.background.is_some_and(|c| c >> 24 != 0)
            || self.gradient.is_some()
            || self.shadow.is_some()
            || self.opacity < 1.0
            || self.clip
            || self.invisible
            || self.transform.is_some()
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Resolved {
    pub layout: Style,
    pub visual: Visual,
    pub text: TextStyle,
    pub scroll: ScrollAxis,
    pub transitions: Vec<Transition>,
    pub active_opacity: Option<f32>,
    pub active_background: Option<u32>,
    /// `.hover(…)`: applied natively while a pointer is over the node (pointer platforms).
    pub hover_opacity: Option<f32>,
    pub hover_background: Option<u32>,
    /// The pointer's shape over the node (`cursor_pointer` …).
    pub cursor: Option<CursorStyle>,
    pub border_widths: [f32; 4],
    /// Which of the lengths above were given in rems.
    pub rems: RemLengths,
}

impl Resolved {
    /// The layout style with rem lengths at `rem_size` (see [`rem_size`]) instead of [`REM`].
    pub fn scaled_layout(&self, rem_size: f32) -> Style {
        let mut l = self.layout.clone();
        let r = &self.rems;
        if r.is_empty() || rem_size == REM {
            return l;
        }
        let len = |v: f32| v * rem_size;
        let edges = |rect: &mut taffy::geometry::Rect<LengthPercentage>, rems: &[Option<f32>; 4]| {
            for (i, v) in rems.iter().enumerate() {
                if let Some(v) = v {
                    *edge_mut(rect, i) = LengthPercentage::length(len(*v));
                }
            }
        };
        edges(&mut l.padding, &r.padding);
        edges(&mut l.border, &r.border);
        for (i, v) in r.margin.iter().enumerate() {
            if let Some(v) = v {
                *edge_mut(&mut l.margin, i) = LengthPercentageAuto::length(len(*v));
            }
        }
        for (i, v) in r.inset.iter().enumerate() {
            if let Some(v) = v {
                *edge_mut(&mut l.inset, i) = LengthPercentageAuto::length(len(*v));
            }
        }
        for (size, rems) in [(&mut l.size, &r.size), (&mut l.min_size, &r.min_size), (&mut l.max_size, &r.max_size)] {
            if let Some(v) = rems[0] {
                size.width = Dimension::length(len(v));
            }
            if let Some(v) = rems[1] {
                size.height = Dimension::length(len(v));
            }
        }
        if let Some(v) = r.gap[0] {
            l.gap.width = LengthPercentage::length(len(v));
        }
        if let Some(v) = r.gap[1] {
            l.gap.height = LengthPercentage::length(len(v));
        }
        if let Some(v) = r.flex_basis {
            l.flex_basis = Dimension::length(len(v));
        }
        if let Some(v) = r.scrollbar_width {
            l.scrollbar_width = len(v);
        }
        l
    }

    /// Corner radii with rem radii at `rem_size`.
    pub fn scaled_radii(&self, rem_size: f32) -> [f32; 4] {
        scale4(self.visual.radii, &self.rems.radii, rem_size)
    }

    /// Border widths with rem widths at `rem_size`.
    pub fn scaled_border_widths(&self, rem_size: f32) -> [f32; 4] {
        scale4(self.border_widths, &self.rems.border, rem_size)
    }

    /// GPUI's default `Style`, as GPUI hands it to taffy: block display, a row
    /// once `flex` is set, items that shrink (1) but do not grow, auto basis.
    pub fn base() -> Resolved {
        let layout = Style { display: Display::Block, ..Style::default() };
        Resolved { layout, ..Resolved::default() }
    }

    pub fn has_border(&self) -> bool {
        self.border_widths.iter().any(|w| *w > 0.0)
    }

    pub fn is_hidden(&self) -> bool {
        self.layout.display == Display::None
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct StyleError(pub String);

impl std::fmt::Display for StyleError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// Parsed chains by key. Cleared on theme change.
#[derive(Default)]
pub struct StyleCache {
    map: HashMap<String, Rc<Resolved>>,
    pub hits: u64,
    pub misses: u64,
}

impl StyleCache {
    pub fn resolve(&mut self, key: &str, ops: &[StyleOp], theme: &Theme) -> Result<Rc<Resolved>, StyleError> {
        if let Some(r) = self.map.get(key) {
            self.hits += 1;
            return Ok(r.clone());
        }
        self.misses += 1;
        let r = Rc::new(resolve(ops, theme)?);
        self.map.insert(key.to_string(), r.clone());
        Ok(r)
    }

    /// A chain resolved before, without needing its ops (counts a hit).
    pub fn get(&mut self, key: &str) -> Option<Rc<Resolved>> {
        let r = self.map.get(key).cloned();
        if r.is_some() {
            self.hits += 1;
        }
        r
    }

    pub fn clear(&mut self) {
        self.map.clear();
    }

    pub fn len(&self) -> usize {
        self.map.len()
    }

    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }
}

pub fn resolve(ops: &[StyleOp], theme: &Theme) -> Result<Resolved, StyleError> {
    let mut r = Resolved::base();
    for op in ops {
        apply(&mut r, op, theme)?;
    }
    finish(&mut r);
    Ok(r)
}

/// Properties derived from several GPUI fields at once.
fn finish(r: &mut Resolved) {
    let o = r.layout.overflow;
    r.scroll = match (o.x == Overflow::Scroll, o.y == Overflow::Scroll) {
        (true, true) => ScrollAxis::Both,
        (true, false) => ScrollAxis::X,
        (false, true) => ScrollAxis::Y,
        (false, false) => ScrollAxis::None,
    };
    let clips = |o: Overflow| matches!(o, Overflow::Hidden | Overflow::Clip);
    r.visual.clip = clips(o.x) || clips(o.y);
}

fn apply(r: &mut Resolved, op: &StyleOp, theme: &Theme) -> Result<(), StyleError> {
    if let Some((state, inner)) = &op.nested {
        return apply_state(r, state, inner, theme);
    }
    if let Some(effects) = nullary(&op.name) {
        if !op.args.is_empty() {
            return Err(StyleError(format!("`{}` takes no arguments", op.name)));
        }
        apply_effects(r, effects);
        return Ok(());
    }
    if PARAM.contains(&op.name.as_str()) {
        return apply_param(r, &op.name, &op.args, theme);
    }
    Err(unknown(&op.name))
}

fn apply_state(r: &mut Resolved, state: &str, inner: &[StyleOp], theme: &Theme) -> Result<(), StyleError> {
    let s = resolve(inner, theme)?;
    match state {
        "active" => {
            if s.visual.opacity < 1.0 {
                r.active_opacity = Some(s.visual.opacity);
            }
            if let Some(bg) = s.visual.background {
                r.active_background = Some(bg);
            }
            Ok(())
        }
        // Like `active`, applied by the renderer without a round trip; touch screens ignore it.
        "hover" => {
            if s.visual.opacity < 1.0 {
                r.hover_opacity = Some(s.visual.opacity);
            }
            if let Some(bg) = s.visual.background {
                r.hover_background = Some(bg);
            }
            Ok(())
        }
        // Focus rings are drawn natively.
        "focus" | "focus_visible" | "group_hover" => Ok(()),
        other => Err(StyleError(format!("unknown state style `{other}`"))),
    }
}

fn unknown(name: &str) -> StyleError {
    match suggest(name) {
        Some(s) => StyleError(format!("unknown style method `{name}` (did you mean: {s}?)")),
        None => StyleError(format!("unknown style method `{name}`")),
    }
}

/// gpui-shell's methods that take arguments, in its documented order.
pub const GPUI_PARAM: &[&str] = &[
    "w", "h", "size", "min_w", "min_h", "min_size", "max_w", "max_h", "max_size", // Length
    "p", "px", "py", "pt", "pb", "pl", "pr", // DefiniteLength
    "m", "mx", "my", "mt", "mb", "ml", "mr", // Length
    "inset", "top", "bottom", "left", "right", // Length
    "gap", "gap_x", "gap_y", "flex_grow", "flex_shrink", "flex_basis", // flex
    "bg", "text_color", "text_bg", "text_size", "font_family", "font_weight", "line_height", "opacity", // paint
    "border", "border_t", "border_b", "border_l", "border_r", "border_x", "border_y", "border_color", // AbsoluteLength
    "rounded", "rounded_t", "rounded_b", "rounded_l", "rounded_r", "rounded_tl", "rounded_tr", "rounded_bl",
    "rounded_br",
];

/// Methods that take arguments beyond gpui-shell's. `aspect_ratio`,
/// `line_clamp`, `grid_cols`, `grid_rows`, `col_span` and `row_span` are GPUI
/// `Styled` methods and behave as GPUI's; `shadow` replaces the shadow list as
/// GPUI's does, from one layer given inline; the rest are mobile-only.
pub const MOBILE_PARAM: &[&str] = &[
    "aspect_ratio", "line_clamp", "grid_cols", "grid_rows", "col_span", "row_span", "shadow", "letter_spacing",
    "bg_gradient", "transition", "spring", "translate", "scale", "rotate",
];

/// Every method that takes arguments: [`GPUI_PARAM`] then [`MOBILE_PARAM`].
pub const PARAM: &[&str] = &[
    "w", "h", "size", "min_w", "min_h", "min_size", "max_w", "max_h", "max_size", "p", "px", "py", "pt", "pb", "pl",
    "pr", "m", "mx", "my", "mt", "mb", "ml", "mr", "inset", "top", "bottom", "left", "right", "gap", "gap_x", "gap_y",
    "flex_grow", "flex_shrink", "flex_basis", "bg", "text_color", "text_bg", "text_size", "font_family", "font_weight",
    "line_height", "opacity", "border", "border_t", "border_b", "border_l", "border_r", "border_x", "border_y",
    "border_color", "rounded", "rounded_t", "rounded_b", "rounded_l", "rounded_r", "rounded_tl", "rounded_tr",
    "rounded_bl", "rounded_br", // mobile
    "aspect_ratio", "line_clamp", "grid_cols", "grid_rows", "col_span", "row_span", "shadow", "letter_spacing",
    "bg_gradient", "transition", "spring", "translate", "scale", "rotate",
];

/// The effects of a no-argument method, if `name` is one.
fn nullary(name: &str) -> Option<&'static [Effect]> {
    table::NULLARY.binary_search_by(|(n, _)| (*n).cmp(name)).ok().map(|i| table::NULLARY[i].1)
}

/// Every nullary name, sorted, for suggestions and the generated `.d.ts`.
pub fn nullary_names() -> Vec<String> {
    table::NULLARY.iter().map(|(n, _)| n.to_string()).collect()
}

/// The closest known method name, within two edits (a third of the name for
/// long names), as gpui-shell suggests.
pub fn suggest(name: &str) -> Option<String> {
    let budget = 2.max(name.chars().count() / 3);
    let mut best: Option<(usize, &str)> = None;
    let candidates = table::NULLARY.iter().map(|(n, _)| *n).chain(PARAM.iter().copied());
    for c in candidates {
        let d = edit_distance(name, c);
        if d <= budget && best.is_none_or(|(bd, bc)| d < bd || (d == bd && c < bc)) {
            best = Some((d, c));
        }
    }
    best.map(|(_, c)| c.to_string())
}

fn edit_distance(a: &str, b: &str) -> usize {
    let b: Vec<char> = b.chars().collect();
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    let mut cur = vec![0; b.len() + 1];
    for (i, ca) in a.chars().enumerate() {
        cur[0] = i + 1;
        for (j, cb) in b.iter().enumerate() {
            cur[j + 1] = (prev[j] + usize::from(ca != *cb)).min(prev[j + 1] + 1).min(cur[j] + 1);
        }
        std::mem::swap(&mut prev, &mut cur);
    }
    prev[b.len()]
}

// ---------------------------------------------------------------------------
// Effects: the neutral form of a write to GPUI's `StyleRefinement`.
// ---------------------------------------------------------------------------

/// A GPUI length before conversion: `AbsoluteLength` is `Px`/`Rems`,
/// `DefiniteLength` adds `Fraction`, `Length` adds `Auto`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Len {
    Px(f32),
    Rems(f32),
    /// A fraction of the parent (`0.5` is 50 %).
    Fraction(f32),
    Auto,
}

impl Len {
    /// GPUI's `ToTaffy`: rems at [`REM`], fractions as percentages.
    fn dimension(self) -> Dimension {
        match self {
            Len::Px(v) => Dimension::length(v),
            Len::Rems(v) => Dimension::length(v * REM),
            Len::Fraction(f) => Dimension::percent(f),
            Len::Auto => Dimension::auto(),
        }
    }

    fn lpa(self) -> LengthPercentageAuto {
        match self {
            Len::Px(v) => LengthPercentageAuto::length(v),
            Len::Rems(v) => LengthPercentageAuto::length(v * REM),
            Len::Fraction(f) => LengthPercentageAuto::percent(f),
            Len::Auto => LengthPercentageAuto::auto(),
        }
    }

    /// Only reached with a `DefiniteLength`, which has no `Auto`.
    fn lp(self) -> LengthPercentage {
        match self {
            Len::Px(v) => LengthPercentage::length(v),
            Len::Rems(v) => LengthPercentage::length(v * REM),
            Len::Fraction(f) => LengthPercentage::percent(f),
            Len::Auto => LengthPercentage::length(0.0),
        }
    }

    /// The value in rems, when the length was given in rems.
    fn rems(self) -> Option<f32> {
        match self {
            Len::Rems(v) => Some(v),
            _ => None,
        }
    }

    /// Only reached with an `AbsoluteLength`.
    fn px(self) -> f32 {
        match self {
            Len::Px(v) => v,
            Len::Rems(v) => v * REM,
            Len::Fraction(_) | Len::Auto => 0.0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Edge {
    Top,
    Right,
    Bottom,
    Left,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Axis {
    Width,
    Height,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Corner {
    TopLeft,
    TopRight,
    BottomRight,
    BottomLeft,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ShadowLayer {
    pub color: u32,
    pub x: f32,
    pub y: f32,
    pub blur: f32,
    pub spread: f32,
    pub inset: bool,
}

/// An underline or strikethrough.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Decoration {
    pub thickness: f32,
    pub wavy: bool,
    pub color: Option<u32>,
}

/// The track minimum of GPUI's `grid_cols`/`grid_rows` template.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GridMin {
    /// `repeat(n, minmax(0, 1fr))`.
    Zero,
    /// `repeat(n, minmax(min-content, 1fr))`.
    MinContent,
    /// `repeat(n, minmax(0, max-content))`.
    MaxContent,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GridSlot {
    RowStart,
    RowEnd,
    ColumnStart,
    ColumnEnd,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Placement {
    Auto,
    Line(i16),
    Span(u16),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextAlignment {
    Left,
    Center,
    Right,
}

/// Where an ellipsis replaces overflowing text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Truncation {
    Start,
    Middle,
    End,
}

impl Truncation {
    pub fn as_str(self) -> &'static str {
        match self {
            Truncation::Start => "start",
            Truncation::Middle => "middle",
            Truncation::End => "end",
        }
    }
}

/// One write a style method makes. A `None` payload resets the field to its
/// default (for text, to inheriting), as GPUI's `Option` fields do.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Effect {
    Display(Display),
    Visible(bool),
    OverflowX(Overflow),
    OverflowY(Overflow),
    ScrollbarWidth(Len),
    Position(TaffyPosition),
    Inset(Edge, Len),
    Size(Axis, Len),
    MinSize(Axis, Len),
    MaxSize(Axis, Len),
    AspectRatio(f32),
    Margin(Edge, Len),
    Padding(Edge, Len),
    BorderWidth(Edge, Len),
    AlignItems(Option<AlignItems>),
    AlignSelf(Option<AlignItems>),
    AlignContent(Option<AlignContent>),
    JustifyContent(Option<AlignContent>),
    Gap(Axis, Len),
    FlexDirection(FlexDirection),
    FlexWrap(FlexWrap),
    FlexBasis(Len),
    FlexGrow(f32),
    FlexShrink(f32),
    Background(Option<u32>),
    BorderColor(Option<u32>),
    BorderDashed(bool),
    CornerRadius(Corner, Len),
    /// Replaces every layer.
    BoxShadow(&'static [ShadowLayer]),
    /// The pointer cursor; recorded, no meaning on touch screens.
    Cursor(&'static str),
    Opacity(f32),
    GridCols(Option<(u16, GridMin)>),
    GridRows(Option<(u16, GridMin)>),
    Grid(GridSlot, Placement),
    TextColor(u32),
    TextBackground(Option<u32>),
    FontSize(Len),
    LineHeight(Len),
    FontWeight(f32),
    Italic(bool),
    Underline(Option<Decoration>),
    /// GPUI's `underline.get_or_insert_with(Default::default)`: a zero-width,
    /// straight underline unless this element already set one.
    UnderlineEnsure,
    UnderlineThickness(f32),
    UnderlineWavy(bool),
    Strikethrough(Option<Decoration>),
    Nowrap(bool),
    TextOverflow(Option<Truncation>),
    TextAlign(TextAlignment),
    LineClamp(Option<u32>),
    /// GPUI's debug outlines; ignored.
    Debug(bool),
    DebugBelow(bool),
    /// Applied only in debug builds, as gpui-base's `debug_*` colours are.
    DebugOnly(&'static [Effect]),
}

fn axis_index(a: Axis) -> usize {
    match a {
        Axis::Width => 0,
        Axis::Height => 1,
    }
}

fn edge_index(e: Edge) -> usize {
    match e {
        Edge::Top => 0,
        Edge::Right => 1,
        Edge::Bottom => 2,
        Edge::Left => 3,
    }
}

fn scale4(base: [f32; 4], rems: &[Option<f32>; 4], rem_size: f32) -> [f32; 4] {
    let mut out = base;
    for (o, r) in out.iter_mut().zip(rems) {
        if let Some(r) = r {
            *o = r * rem_size;
        }
    }
    out
}

/// The edge of a rect at a `[top, right, bottom, left]` index.
fn edge_mut<T>(rect: &mut taffy::geometry::Rect<T>, i: usize) -> &mut T {
    match i {
        0 => &mut rect.top,
        1 => &mut rect.right,
        2 => &mut rect.bottom,
        _ => &mut rect.left,
    }
}

fn set_edge<T>(rect: &mut taffy::geometry::Rect<T>, e: Edge, v: T) {
    match e {
        Edge::Top => rect.top = v,
        Edge::Right => rect.right = v,
        Edge::Bottom => rect.bottom = v,
        Edge::Left => rect.left = v,
    }
}

fn set_axis<T>(size: &mut Size<T>, a: Axis, v: T) {
    match a {
        Axis::Width => size.width = v,
        Axis::Height => size.height = v,
    }
}

fn grid_template(t: Option<(u16, GridMin)>) -> Vec<taffy::style::GridTemplateComponent<String>> {
    use taffy::style::{MaxTrackSizingFunction, MinTrackSizingFunction};
    use taffy::style_helpers::{minmax, repeat};
    let Some((count, min)) = t else { return Vec::new() };
    let track = match min {
        GridMin::Zero => minmax(MinTrackSizingFunction::length(0.0), fr(1.0_f32)),
        GridMin::MinContent => minmax(MinTrackSizingFunction::min_content(), fr(1.0_f32)),
        GridMin::MaxContent => minmax(MinTrackSizingFunction::length(0.0), MaxTrackSizingFunction::max_content()),
    };
    vec![repeat(count, vec![track])]
}

fn placement(p: Placement) -> GridPlacement {
    match p {
        Placement::Auto => GridPlacement::Auto,
        Placement::Line(l) => line(l),
        Placement::Span(s) => span(s),
    }
}

fn apply_effects(r: &mut Resolved, effects: &[Effect]) {
    for e in effects {
        apply_effect(r, *e);
    }
}

fn apply_effect(r: &mut Resolved, e: Effect) {
    match e {
        Effect::Display(d) => r.layout.display = d,
        Effect::Visible(v) => r.visual.invisible = !v,
        Effect::OverflowX(o) => r.layout.overflow.x = o,
        Effect::OverflowY(o) => r.layout.overflow.y = o,
        Effect::ScrollbarWidth(w) => {
            r.rems.scrollbar_width = w.rems();
            r.layout.scrollbar_width = w.px()
        }
        Effect::Position(p) => r.layout.position = p,
        Effect::Inset(edge, v) => {
            r.rems.inset[edge_index(edge)] = v.rems();
            set_edge(&mut r.layout.inset, edge, v.lpa())
        }
        Effect::Size(a, v) => {
            r.rems.size[axis_index(a)] = v.rems();
            set_axis(&mut r.layout.size, a, v.dimension())
        }
        Effect::MinSize(a, v) => {
            r.rems.min_size[axis_index(a)] = v.rems();
            set_axis(&mut r.layout.min_size, a, v.dimension())
        }
        Effect::MaxSize(a, v) => {
            r.rems.max_size[axis_index(a)] = v.rems();
            set_axis(&mut r.layout.max_size, a, v.dimension())
        }
        Effect::AspectRatio(v) => r.layout.aspect_ratio = Some(v),
        Effect::Margin(edge, v) => {
            r.rems.margin[edge_index(edge)] = v.rems();
            set_edge(&mut r.layout.margin, edge, v.lpa())
        }
        Effect::Padding(edge, v) => {
            r.rems.padding[edge_index(edge)] = v.rems();
            set_edge(&mut r.layout.padding, edge, v.lp())
        }
        Effect::BorderWidth(edge, v) => {
            r.rems.border[edge_index(edge)] = v.rems();
            r.border_widths[edge_index(edge)] = v.px();
            set_edge(&mut r.layout.border, edge, LengthPercentage::length(v.px()));
        }
        Effect::AlignItems(v) => r.layout.align_items = v,
        Effect::AlignSelf(v) => r.layout.align_self = v,
        Effect::AlignContent(v) => r.layout.align_content = v,
        Effect::JustifyContent(v) => r.layout.justify_content = v,
        Effect::Gap(a, v) => {
            r.rems.gap[axis_index(a)] = v.rems();
            set_axis(&mut r.layout.gap, a, v.lp())
        }
        Effect::FlexDirection(v) => r.layout.flex_direction = v,
        Effect::FlexWrap(v) => r.layout.flex_wrap = v,
        Effect::FlexBasis(v) => {
            r.rems.flex_basis = v.rems();
            r.layout.flex_basis = v.dimension()
        }
        Effect::FlexGrow(v) => r.layout.flex_grow = v,
        Effect::FlexShrink(v) => r.layout.flex_shrink = v,
        Effect::Background(c) => r.visual.background = c,
        Effect::BorderColor(c) => r.visual.border_color = c,
        Effect::BorderDashed(d) => r.visual.border_dashed = d,
        Effect::CornerRadius(c, v) => {
            let i = match c {
                Corner::TopLeft => 0,
                Corner::TopRight => 1,
                Corner::BottomRight => 2,
                Corner::BottomLeft => 3,
            };
            r.rems.radii[i] = v.rems();
            r.visual.radii[i] = v.px();
        }
        Effect::BoxShadow(layers) => {
            let shadows: Vec<Shadow> = layers
                .iter()
                .map(|s| Shadow { color: s.color, x: s.x, y: s.y, blur: s.blur, spread: s.spread, inset: s.inset })
                .collect();
            set_shadows(r, shadows);
        }
        // The generated table names GPUI's `CursorStyle` variants, which the protocol mirrors.
        Effect::Cursor(name) => r.cursor = CursorStyle::ALL.iter().copied().find(|c| c.name() == name),
        Effect::Debug(_) | Effect::DebugBelow(_) => {}
        Effect::Opacity(v) => r.visual.opacity = v.clamp(0.0, 1.0),
        Effect::GridCols(g) => r.layout.grid_template_columns = grid_template(g),
        Effect::GridRows(g) => r.layout.grid_template_rows = grid_template(g),
        Effect::Grid(slot, p) => {
            let p = placement(p);
            match slot {
                GridSlot::RowStart => r.layout.grid_row.start = p,
                GridSlot::RowEnd => r.layout.grid_row.end = p,
                GridSlot::ColumnStart => r.layout.grid_column.start = p,
                GridSlot::ColumnEnd => r.layout.grid_column.end = p,
            }
        }
        Effect::TextColor(c) => r.text.color = Some(c),
        Effect::TextBackground(c) => r.text.background = c,
        Effect::FontSize(v) => r.text.size = Some(v.px()),
        Effect::LineHeight(v) => {
            r.text.line_height = Some(match v {
                Len::Fraction(m) => LineHeight::Multiple(m),
                other => LineHeight::Px(other.px()),
            })
        }
        Effect::FontWeight(w) => r.text.weight = Some(w.round() as u16),
        Effect::Italic(i) => r.text.italic = Some(i),
        Effect::Underline(d) => {
            r.text.underline = d.map(|d| d.thickness > 0.0);
            r.text.underline_thickness = d.map(|d| d.thickness);
            r.text.underline_wavy = d.map(|d| d.wavy);
        }
        Effect::UnderlineEnsure => {
            if r.text.underline_thickness.is_none() {
                r.text.underline = Some(false);
                r.text.underline_thickness = Some(0.0);
                r.text.underline_wavy = Some(false);
            }
        }
        Effect::UnderlineThickness(w) => {
            r.text.underline = Some(w > 0.0);
            r.text.underline_thickness = Some(w);
        }
        Effect::UnderlineWavy(w) => r.text.underline_wavy = Some(w),
        Effect::Strikethrough(d) => r.text.strike = d.map(|d| d.thickness > 0.0),
        Effect::Nowrap(n) => r.text.nowrap = Some(n),
        Effect::TextOverflow(o) => {
            r.text.truncate = o.map(|_| true);
            r.text.truncate_at = o;
        }
        Effect::TextAlign(a) => {
            r.text.align = Some(match a {
                TextAlignment::Left => "left",
                TextAlignment::Center => "center",
                TextAlignment::Right => "right",
            })
        }
        Effect::LineClamp(n) => r.text.lines = n,
        Effect::DebugOnly(inner) => {
            if cfg!(debug_assertions) {
                apply_effects(r, inner);
            }
        }
    }
}

fn set_shadows(r: &mut Resolved, shadows: Vec<Shadow>) {
    r.visual.shadow = shadows.first().copied();
    r.visual.shadows = shadows;
}

// ---------------------------------------------------------------------------
// Methods that take arguments.
// ---------------------------------------------------------------------------

/// gpui-shell's `arg`: the method name and how many arguments it needed.
fn arg<'a>(args: &'a [Arg], i: usize, method: &str) -> Result<&'a Arg, StyleError> {
    args.get(i)
        .ok_or_else(|| StyleError(format!("`{method}` expects at least {} argument(s), got {}", i + 1, args.len())))
}

fn num(a: &Arg) -> Result<f32, StyleError> {
    match a {
        Arg::Num(n) => Ok(*n as f32),
        other => Err(StyleError(format!("expected a number, got {}", other.describe()))),
    }
}

fn string(a: &Arg) -> Result<&str, StyleError> {
    match a {
        Arg::Str(s) => Ok(s),
        other => Err(StyleError(format!("expected a string, got {}", other.describe()))),
    }
}

/// A length as written, before it is narrowed to the GPUI type a method takes.
/// A bare number is pixels; a string carries its unit.
fn parse_length(a: &Arg, method: &str) -> Result<Len, StyleError> {
    let Arg::Str(text) = a else {
        return Ok(Len::Px(num(a)?));
    };
    let text = text.trim();
    let number = |n: &str| {
        n.trim()
            .parse::<f32>()
            .map_err(|_| StyleError(format!("`{method}` could not read a number in the length \"{text}\"")))
    };
    if text == "auto" {
        return Ok(Len::Auto);
    }
    if let Some(n) = text.strip_suffix('%') {
        return Ok(Len::Fraction(number(n)? / 100.0));
    }
    if let Some(n) = text.strip_suffix("rem") {
        return Ok(Len::Rems(number(n)?));
    }
    if let Some(n) = text.strip_suffix("px") {
        return Ok(Len::Px(number(n)?));
    }
    Err(StyleError(format!(
        "`{method}` expects a length: a number of pixels, or a string like \"50%\", \"12px\", \"1rem\" or \"auto\"; got \"{text}\""
    )))
}

/// GPUI's `Length`: anything, `"auto"` included.
fn length(args: &[Arg], method: &str) -> Result<Len, StyleError> {
    parse_length(arg(args, 0, method)?, method)
}

/// GPUI's `DefiniteLength`: no `"auto"`.
fn definite(a: &Arg, method: &str) -> Result<Len, StyleError> {
    match parse_length(a, method)? {
        Len::Auto => Err(StyleError(format!(
            "`{method}` cannot be \"auto\"; it expects a definite length such as 12 or \"50%\""
        ))),
        v => Ok(v),
    }
}

/// GPUI's `AbsoluteLength`: pixels or rems only.
fn absolute(a: &Arg, method: &str) -> Result<Len, StyleError> {
    match parse_length(a, method)? {
        Len::Fraction(_) | Len::Auto => Err(StyleError(format!(
            "`{method}` expects an absolute length such as 8 or \"0.5rem\"; percentages and \"auto\" are not allowed here"
        ))),
        v => Ok(v),
    }
}

/// A theme token or a `#rgb`/`#rrggbb`/`#rrggbbaa` literal. gpui-shell takes
/// literals only; mobile scripts may also name the token directly.
fn color(a: &Arg, theme: &Theme) -> Result<u32, StyleError> {
    let text = string(a)?;
    theme.color(text).ok_or_else(|| {
        if text.starts_with('#') {
            StyleError(format!("`{text}` is not a valid color literal (expected #rgb, #rrggbb or #rrggbbaa)"))
        } else {
            StyleError(format!(
                "`{text}` is not a color value; pass a theme token like \"primary\" or a #rgb, #rrggbb, or #rrggbbaa literal"
            ))
        }
    })
}

fn edges(sides: &str) -> &'static [Edge] {
    match sides {
        "" => &[Edge::Top, Edge::Right, Edge::Bottom, Edge::Left],
        "x" => &[Edge::Right, Edge::Left],
        "y" => &[Edge::Top, Edge::Bottom],
        "t" => &[Edge::Top],
        "b" => &[Edge::Bottom],
        "l" => &[Edge::Left],
        "r" => &[Edge::Right],
        _ => &[],
    }
}

fn corners(sides: &str) -> &'static [Corner] {
    match sides {
        "" => &[Corner::TopLeft, Corner::TopRight, Corner::BottomRight, Corner::BottomLeft],
        "t" => &[Corner::TopLeft, Corner::TopRight],
        "b" => &[Corner::BottomRight, Corner::BottomLeft],
        "l" => &[Corner::TopLeft, Corner::BottomLeft],
        "r" => &[Corner::TopRight, Corner::BottomRight],
        "tl" => &[Corner::TopLeft],
        "tr" => &[Corner::TopRight],
        "bl" => &[Corner::BottomLeft],
        "br" => &[Corner::BottomRight],
        _ => &[],
    }
}

fn anim(args: &[Arg], method: &str, kind: &'static str) -> Result<Transition, StyleError> {
    let property = string(arg(args, 0, method)?)?.to_string();
    if !["opacity", "width", "height", "left", "top", "frame", "transform", "background"].contains(&property.as_str()) {
        return Err(StyleError(format!(
            "`{method}`: `{property}` cannot be animated (opacity, width, height, left, top, transform, background)"
        )));
    }
    let opts: serde_json::Value = match args.get(1) {
        Some(Arg::Str(s)) => serde_json::from_str(s).map_err(|e| StyleError(format!("`{method}` options: {e}")))?,
        _ => serde_json::Value::Null,
    };
    let f = |k: &str, d: f32| opts.get(k).and_then(|v| v.as_f64()).map(|v| v as f32).unwrap_or(d);
    Ok(Transition {
        property: if matches!(property.as_str(), "width" | "height" | "left" | "top") { "frame".into() } else { property },
        kind,
        duration: f("duration", 250.0),
        delay: f("delay", 0.0),
        easing: opts.get("easing").and_then(|v| v.as_str()).unwrap_or("ease_in_out").to_string(),
        response: f("response", 0.35),
        damping: f("damping", 0.825),
    })
}

/// A count argument (`u16`/`usize` in GPUI).
fn count(args: &[Arg], method: &str) -> Result<u32, StyleError> {
    let n = num(arg(args, 0, method)?)?;
    if !(n.is_finite() && n >= 0.0 && n.fract() == 0.0) {
        return Err(StyleError(format!("`{method}` expects a whole number, got {n}")));
    }
    Ok(n as u32)
}

fn apply_param(r: &mut Resolved, name: &str, args: &[Arg], theme: &Theme) -> Result<(), StyleError> {
    let a0 = || arg(args, 0, name);
    let mut fx: Vec<Effect> = Vec::new();
    match name {
        "w" => fx.push(Effect::Size(Axis::Width, length(args, name)?)),
        "h" => fx.push(Effect::Size(Axis::Height, length(args, name)?)),
        "size" => {
            let v = length(args, name)?;
            fx.extend([Effect::Size(Axis::Width, v), Effect::Size(Axis::Height, v)]);
        }
        "min_w" => fx.push(Effect::MinSize(Axis::Width, length(args, name)?)),
        "min_h" => fx.push(Effect::MinSize(Axis::Height, length(args, name)?)),
        "min_size" => {
            let v = length(args, name)?;
            fx.extend([Effect::MinSize(Axis::Width, v), Effect::MinSize(Axis::Height, v)]);
        }
        "max_w" => fx.push(Effect::MaxSize(Axis::Width, length(args, name)?)),
        "max_h" => fx.push(Effect::MaxSize(Axis::Height, length(args, name)?)),
        "max_size" => {
            let v = length(args, name)?;
            fx.extend([Effect::MaxSize(Axis::Width, v), Effect::MaxSize(Axis::Height, v)]);
        }
        "p" | "px" | "py" | "pt" | "pb" | "pl" | "pr" => {
            let v = definite(a0()?, name)?;
            fx.extend(edges(&name[1..]).iter().map(|e| Effect::Padding(*e, v)));
        }
        "m" | "mx" | "my" | "mt" | "mb" | "ml" | "mr" => {
            let v = length(args, name)?;
            fx.extend(edges(&name[1..]).iter().map(|e| Effect::Margin(*e, v)));
        }
        "inset" => {
            let v = length(args, name)?;
            fx.extend(edges("").iter().map(|e| Effect::Inset(*e, v)));
        }
        "top" => fx.push(Effect::Inset(Edge::Top, length(args, name)?)),
        "bottom" => fx.push(Effect::Inset(Edge::Bottom, length(args, name)?)),
        "left" => fx.push(Effect::Inset(Edge::Left, length(args, name)?)),
        "right" => fx.push(Effect::Inset(Edge::Right, length(args, name)?)),
        "gap" => {
            let v = definite(a0()?, name)?;
            fx.extend([Effect::Gap(Axis::Width, v), Effect::Gap(Axis::Height, v)]);
        }
        "gap_x" => fx.push(Effect::Gap(Axis::Width, definite(a0()?, name)?)),
        "gap_y" => fx.push(Effect::Gap(Axis::Height, definite(a0()?, name)?)),
        "flex_grow" => fx.push(Effect::FlexGrow(num(a0()?)?)),
        "flex_shrink" => fx.push(Effect::FlexShrink(num(a0()?)?)),
        "flex_basis" => fx.push(Effect::FlexBasis(length(args, name)?)),
        "bg" => fx.push(Effect::Background(Some(color(a0()?, theme)?))),
        "text_color" => fx.push(Effect::TextColor(color(a0()?, theme)?)),
        "text_bg" => fx.push(Effect::TextBackground(Some(color(a0()?, theme)?))),
        "text_size" => fx.push(Effect::FontSize(absolute(a0()?, name)?)),
        "font_family" => r.text.family = Some(string(a0()?)?.to_string()),
        "font_weight" => {
            let w = num(a0()?)?;
            if !(w.is_finite() && (100.0..=900.0).contains(&w)) {
                return Err(StyleError(format!("`{name}` expects a finite number between 100 and 900; got {w}")));
            }
            fx.push(Effect::FontWeight(w));
        }
        // A bare number is a multiple of the font size, not pixels; a string
        // is a definite length, so "150%" is also a multiple.
        "line_height" => fx.push(Effect::LineHeight(match a0()? {
            Arg::Num(m) => Len::Fraction(*m as f32),
            other => definite(other, name)?,
        })),
        "opacity" => fx.push(Effect::Opacity(num(a0()?)?)),
        "border" | "border_t" | "border_b" | "border_l" | "border_r" | "border_x" | "border_y" => {
            let v = absolute(a0()?, name)?;
            let sides = name.strip_prefix("border").unwrap_or_default().trim_start_matches('_');
            fx.extend(edges(sides).iter().map(|e| Effect::BorderWidth(*e, v)));
        }
        "border_color" => fx.push(Effect::BorderColor(Some(color(a0()?, theme)?))),
        "rounded" | "rounded_t" | "rounded_b" | "rounded_l" | "rounded_r" | "rounded_tl" | "rounded_tr"
        | "rounded_bl" | "rounded_br" => {
            let v = absolute(a0()?, name)?;
            let sides = name.strip_prefix("rounded").unwrap_or_default().trim_start_matches('_');
            fx.extend(corners(sides).iter().map(|c| Effect::CornerRadius(*c, v)));
        }

        // GPUI `Styled` methods gpui-shell does not bind.
        "aspect_ratio" => fx.push(Effect::AspectRatio(num(a0()?)?)),
        "line_clamp" => fx.extend([
            Effect::LineClamp(Some(count(args, name)?)),
            Effect::OverflowX(Overflow::Hidden),
            Effect::OverflowY(Overflow::Hidden),
        ]),
        "grid_cols" => fx.push(Effect::GridCols(Some((count(args, name)? as u16, GridMin::Zero)))),
        "grid_rows" => fx.push(Effect::GridRows(Some((count(args, name)? as u16, GridMin::Zero)))),
        "col_span" => {
            let n = count(args, name)? as u16;
            fx.extend([
                Effect::Grid(GridSlot::ColumnStart, Placement::Span(n)),
                Effect::Grid(GridSlot::ColumnEnd, Placement::Span(n)),
            ]);
        }
        "row_span" => {
            let n = count(args, name)? as u16;
            fx.extend([
                Effect::Grid(GridSlot::RowStart, Placement::Span(n)),
                Effect::Grid(GridSlot::RowEnd, Placement::Span(n)),
            ]);
        }

        // Mobile additions.
        "shadow" => {
            // shadow(color, x, y, blur, spread?)
            let layer = Shadow {
                color: color(a0()?, theme)?,
                x: num(arg(args, 1, name)?)?,
                y: num(arg(args, 2, name)?)?,
                blur: num(arg(args, 3, name)?)?,
                spread: args.get(4).map(num).transpose()?.unwrap_or(0.0),
                inset: false,
            };
            set_shadows(r, vec![layer]);
        }
        "letter_spacing" => r.text.letter_spacing = Some(absolute(a0()?, name)?.px()),
        "bg_gradient" => {
            // bg_gradient(angle, color, color, …)
            let angle = num(a0()?)?;
            let n = args.len().saturating_sub(1);
            if n < 2 {
                return Err(StyleError("`bg_gradient` needs an angle and at least two colors".into()));
            }
            let stops = (0..n)
                .map(|i| Ok((color(&args[i + 1], theme)?, i as f32 / (n - 1) as f32)))
                .collect::<Result<Vec<_>, StyleError>>()?;
            r.visual.gradient = Some((angle, stops));
        }
        "transition" => r.transitions.push(anim(args, name, "timing")?),
        "spring" => r.transitions.push(anim(args, name, "spring")?),
        "translate" => {
            let x = num(a0()?)?;
            let y = args.get(1).map(num).transpose()?.unwrap_or(0.0);
            let t = r.visual.transform.get_or_insert([0.0, 0.0, 1.0, 0.0]);
            t[0] = x;
            t[1] = y;
        }
        "scale" => {
            let s = num(a0()?)?;
            r.visual.transform.get_or_insert([0.0, 0.0, 1.0, 0.0])[2] = s;
        }
        "rotate" => {
            let d = num(a0()?)?;
            r.visual.transform.get_or_insert([0.0, 0.0, 1.0, 0.0])[3] = d;
        }
        other => return Err(unknown(other)),
    }
    apply_effects(r, &fx);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::Appearance;

    fn theme() -> Theme {
        Theme::default_for(Appearance::Light)
    }

    fn op(name: &str) -> StyleOp {
        StyleOp::new(name, vec![])
    }

    fn opa(name: &str, args: Vec<Arg>) -> StyleOp {
        StyleOp::new(name, args)
    }

    fn one(name: &str) -> Resolved {
        resolve(&[op(name)], &theme()).unwrap()
    }

    fn err(ops: &[StyleOp]) -> String {
        resolve(ops, &theme()).unwrap_err().0
    }

    fn s(v: &str) -> Arg {
        Arg::Str(v.into())
    }

    #[test]
    fn the_table_is_the_generated_reflection() {
        assert_eq!(table::NULLARY.len(), table::NULLARY_COUNT);
        assert_eq!(
            table::NULLARY_COUNT,
            table::GPUI_REFLECTED + table::BASE_REFLECTED + table::BASE_TRANSCRIBED + table::ELEMENT_TRANSCRIBED
        );
        const { assert!(table::GPUI_REFLECTED > 3000) };
        assert!(table::NULLARY.windows(2).all(|w| w[0].0 < w[1].0), "sorted and unique");
        assert_eq!(nullary_names().len(), table::NULLARY_COUNT);
    }

    #[test]
    fn every_nullary_name_resolves() {
        let t = theme();
        for (name, _) in table::NULLARY {
            assert!(resolve(&[op(name)], &t).is_ok(), "{name}");
        }
    }

    #[test]
    fn parametric_names_are_gpui_shells_59_plus_extras_and_disjoint_from_nullary() {
        assert_eq!(GPUI_PARAM.len(), 59);
        let joined: Vec<&str> = GPUI_PARAM.iter().chain(MOBILE_PARAM).copied().collect();
        assert_eq!(PARAM, joined.as_slice());
        for name in PARAM {
            assert!(nullary(name).is_none(), "`{name}` is both nullary and parametric");
            let e = err(&[op(name)]);
            assert!(e.contains(&format!("`{name}` expects at least 1 argument(s), got 0")), "{name}: {e}");
        }
    }

    #[test]
    fn base_is_gpuis_default_style() {
        let r = Resolved::base();
        assert_eq!(r.layout.display, Display::Block);
        assert_eq!(r.layout.flex_direction, FlexDirection::Row);
        assert_eq!(r.layout.flex_shrink, 1.0);
        assert_eq!(r.layout.flex_grow, 0.0);
        assert_eq!(r.layout.flex_basis, Dimension::auto());
        assert_eq!(r.layout.position, TaffyPosition::Relative);
        assert_eq!(r.layout.inset.top, LengthPercentageAuto::auto());
        assert_eq!(DEFAULT_TEXT_SIZE, 16.0);
        assert_eq!(DEFAULT_LINE_HEIGHT, LineHeight::Multiple(1.618_034));
    }

    #[test]
    fn spacing_follows_gpui_rems_and_fractions() {
        let r = resolve(&[op("p_4"), op("gap_0p5"), op("w_1_2"), op("h_px"), op("mx_auto")], &theme()).unwrap();
        assert_eq!(r.layout.padding.top, LengthPercentage::length(16.0));
        assert_eq!(r.layout.padding.left, LengthPercentage::length(16.0));
        assert_eq!(r.layout.gap.width, LengthPercentage::length(2.0));
        assert_eq!(r.layout.gap.height, LengthPercentage::length(2.0));
        assert_eq!(r.layout.size.width, Dimension::percent(0.5));
        assert_eq!(r.layout.size.height, Dimension::length(1.0));
        assert_eq!(r.layout.margin.left, LengthPercentageAuto::auto());
        assert_eq!(r.layout.margin.top, LengthPercentageAuto::length(0.0));
        assert_eq!(one("w_full").layout.size.width, Dimension::percent(1.0));
        let full = one("size_full").layout.size;
        assert_eq!((full.width, full.height), (Dimension::percent(1.0), Dimension::percent(1.0)));
        assert_eq!(one("m_neg_4").layout.margin.right, LengthPercentageAuto::length(-16.0));
        assert_eq!(one("min_w_0").layout.min_size.width, Dimension::length(0.0));
        let inset = one("inset_0").layout.inset;
        assert_eq!([inset.top, inset.right, inset.bottom, inset.left], [LengthPercentageAuto::length(0.0); 4]);
    }

    #[test]
    fn flex_presets() {
        let f1 = one("flex_1").layout;
        assert_eq!((f1.flex_grow, f1.flex_shrink, f1.flex_basis), (1.0, 1.0, Dimension::percent(0.0)));
        let none = one("flex_none").layout;
        assert_eq!((none.flex_grow, none.flex_shrink, none.flex_basis), (0.0, 0.0, Dimension::auto()));
        assert_eq!(one("items_center").layout.align_items, Some(AlignItems::Center));
        assert_eq!(one("justify_between").layout.justify_content, Some(JustifyContent::SpaceBetween));
        let r = resolve(&[op("content_center"), op("content_normal")], &theme()).unwrap();
        assert_eq!(r.layout.align_content, None);
    }

    #[test]
    fn gpui_base_flex_shorthands() {
        let h = one("h_flex").layout;
        assert_eq!((h.display, h.flex_direction, h.align_items), (Display::Flex, FlexDirection::Row, Some(AlignItems::Center)));
        let v = one("v_flex").layout;
        assert_eq!((v.display, v.flex_direction, v.align_items), (Display::Flex, FlexDirection::Column, None));
    }

    #[test]
    fn radii_borders_and_shadows() {
        assert_eq!(one("rounded_md").visual.radii, [6.0; 4]);
        assert_eq!(one("rounded_full").visual.radii, [9999.0; 4]);
        let r = resolve(&[op("rounded_lg"), op("rounded_tl_none")], &theme()).unwrap();
        assert_eq!(r.visual.radii, [0.0, 8.0, 8.0, 8.0]);
        let b = one("border_2");
        assert_eq!(b.border_widths, [2.0; 4]);
        assert_eq!(b.layout.border.top, LengthPercentage::length(2.0));
        assert_eq!(one("border_x_1").border_widths, [0.0, 1.0, 0.0, 1.0]);

        // Tailwind's two-layer `shadow-md`: 0 4px 6px -1px and 0 2px 4px -2px at 10% black.
        let md = one("shadow_md").visual;
        assert_eq!(
            md.shadows,
            vec![
                Shadow { color: 0x1A00_0000, x: 0.0, y: 4.0, blur: 6.0, spread: -1.0, inset: false },
                Shadow { color: 0x1A00_0000, x: 0.0, y: 2.0, blur: 4.0, spread: -2.0, inset: false },
            ]
        );
        assert_eq!(md.shadow, Some(md.shadows[0]));
        let none = resolve(&[op("shadow_md"), op("shadow_none")], &theme()).unwrap().visual;
        assert_eq!((none.shadow, none.shadows.len()), (None, 0));
    }

    #[test]
    fn text_presets() {
        for (name, px) in [("text_xs", 12.0), ("text_sm", 14.0), ("text_base", 16.0), ("text_xl", 20.0), ("text_3xl", 30.0)] {
            let t = one(name).text;
            assert_eq!(t.size, Some(px), "{name}");
            // GPUI's text sizes leave the line height alone (φ by default).
            assert_eq!(t.line_height, None, "{name}");
        }
        assert_eq!(one("font_bold").text.weight, Some(700));
        assert_eq!(one("font_medium").text.weight, Some(500));
        assert_eq!(one("italic").text.italic, Some(true));
        assert_eq!(one("text_center").text.align, Some("center"));
        let u = one("underline").text;
        assert_eq!((u.underline, u.underline_thickness), (Some(true), Some(1.0)));
        let wavy = one("text_decoration_wavy").text;
        assert_eq!((wavy.underline, wavy.underline_wavy), (Some(false), Some(true)), "ensured, zero thickness");
        let wavy2 = resolve(&[op("underline"), op("text_decoration_wavy")], &theme()).unwrap().text;
        assert_eq!((wavy2.underline, wavy2.underline_wavy), (Some(true), Some(true)));
        assert_eq!(resolve(&[op("underline"), op("text_decoration_none")], &theme()).unwrap().text.underline, None);
        assert_eq!(one("line_through").text.strike, Some(true));
    }

    #[test]
    fn truncate_and_clamp_follow_gpui() {
        let t = one("truncate");
        assert_eq!((t.text.truncate, t.text.truncate_at, t.text.nowrap), (Some(true), Some(Truncation::End), Some(true)));
        assert_eq!(t.text.lines, None);
        assert_eq!(t.layout.overflow.x, Overflow::Hidden);
        assert!(t.visual.clip);
        assert_eq!(one("text_ellipsis_middle").text.truncate_at, Some(Truncation::Middle));

        let c = resolve(&[opa("line_clamp", vec![Arg::Num(3.0)])], &theme()).unwrap();
        assert_eq!((c.text.lines, c.text.truncate), (Some(3), None));
        assert_eq!(c.layout.overflow.y, Overflow::Hidden);
    }

    #[test]
    fn overflow_axes_and_scroll() {
        let h = one("overflow_hidden");
        assert_eq!((h.layout.overflow.x, h.layout.overflow.y), (Overflow::Hidden, Overflow::Hidden));
        assert!(h.visual.clip);
        let x = one("overflow_x_hidden").layout.overflow;
        assert_eq!((x.x, x.y), (Overflow::Hidden, Overflow::Visible));
        assert_eq!(one("overflow_y_scroll").scroll, ScrollAxis::Y);
        assert_eq!(resolve(&[op("overflow_x_scroll"), op("overflow_y_scroll")], &theme()).unwrap().scroll, ScrollAxis::Both);
        assert!(!one("overflow_y_scroll").visual.clip);
    }

    #[test]
    fn grid_methods_follow_gpui() {
        use taffy::style::GridTemplateComponent;
        let r = resolve(&[op("grid"), opa("grid_cols", vec![Arg::Num(3.0)]), opa("col_span", vec![Arg::Num(2.0)])], &theme())
            .unwrap();
        assert_eq!(r.layout.display, Display::Grid);
        assert!(matches!(r.layout.grid_template_columns.as_slice(), [GridTemplateComponent::Repeat(_)]));
        assert_eq!(r.layout.grid_column, taffy::geometry::Line { start: span(2), end: span(2) });
        // `grid_cols` alone does not switch the display, as in GPUI.
        assert_eq!(resolve(&[opa("grid_cols", vec![Arg::Num(3.0)])], &theme()).unwrap().layout.display, Display::Block);
        let full = one("col_span_full").layout.grid_column;
        assert_eq!(full, taffy::geometry::Line { start: line(1), end: line(-1) });
    }

    #[test]
    fn visibility_and_display() {
        assert!(one("invisible").visual.invisible);
        assert!(one("hidden").is_hidden());
        assert_eq!(one("block").layout.display, Display::Block);
        assert_eq!(one("absolute").layout.position, TaffyPosition::Absolute);
        // A cursor sets only the pointer shape (pointer platforms; touch screens ignore it).
        assert_eq!(one("cursor_pointer"), Resolved { cursor: Some(CursorStyle::PointingHand), ..Resolved::base() });
    }

    #[test]
    fn parametric_lengths() {
        let r = resolve(
            &[
                opa("w", vec![Arg::Num(100.0)]),
                opa("h", vec![s("1.5rem")]),
                opa("p", vec![s("10%")]),
                opa("min_h", vec![s("auto")]),
                opa("text_size", vec![s("0.75rem")]),
                opa("rounded", vec![Arg::Num(4.0)]),
            ],
            &theme(),
        )
        .unwrap();
        assert_eq!(r.layout.size.width, Dimension::length(100.0));
        assert_eq!(r.layout.size.height, Dimension::length(24.0));
        assert_eq!(r.layout.padding.left, LengthPercentage::percent(0.1));
        assert_eq!(r.layout.min_size.height, Dimension::auto());
        assert_eq!(r.text.size, Some(12.0));
        assert_eq!(r.visual.radii, [4.0; 4]);
    }

    #[test]
    fn line_height_number_is_a_multiple() {
        let lh = |a: Arg| resolve(&[opa("line_height", vec![a])], &theme()).unwrap().text.line_height;
        assert_eq!(lh(Arg::Num(1.4)), Some(LineHeight::Multiple(1.4)));
        assert_eq!(lh(s("150%")), Some(LineHeight::Multiple(1.5)));
        assert_eq!(lh(s("18px")), Some(LineHeight::Px(18.0)));
        assert_eq!(lh(s("1.5rem")), Some(LineHeight::Px(24.0)));
    }

    #[test]
    fn argument_errors_mirror_gpui_shell() {
        assert!(err(&[opa("p", vec![s("auto")])]).contains("cannot be \"auto\"; it expects a definite length"));
        let e = err(&[opa("rounded", vec![s("50%")])]);
        assert!(e.contains("expects an absolute length such as 8 or \"0.5rem\""), "{e}");
        let e = err(&[opa("w", vec![s("wide")])]);
        assert!(e.contains("`w` expects a length: a number of pixels"), "{e}");
        assert!(err(&[opa("w", vec![s("1xrem")])]).contains("could not read a number in the length \"1xrem\""));
        assert!(err(&[opa("bg", vec![Arg::Num(1.0)])]).contains("expected a string, got number (1)"));
        assert!(err(&[opa("opacity", vec![s("0.5")])]).contains("expected a number, got string (\"0.5\")"));
        for w in [99.0, 901.0] {
            let e = err(&[opa("font_weight", vec![Arg::Num(w)])]);
            assert!(e.contains("expects a finite number between 100 and 900"), "{e}");
        }
        assert!(err(&[opa("bg", vec![s("nope")])]).contains("is not a color value"));
        assert!(err(&[opa("bg", vec![s("#12")])]).contains("is not a valid color literal"));
        assert!(err(&[op("p")]).contains("`p` expects at least 1 argument(s), got 0"));
    }

    #[test]
    fn colors_tokens_and_visuals() {
        // Tailwind's `opacity_50` is not a GPUI method.
        assert!(err(&[op("opacity_50")]).contains("unknown style method `opacity_50`"));
        let r = resolve(&[opa("bg", vec![s("primary")]), opa("opacity", vec![Arg::Num(0.5)])], &theme()).unwrap();
        assert_eq!(r.visual.background, theme().color("primary"));
        assert_eq!(r.visual.opacity, 0.5);
        assert!(r.visual.paints());
    }

    #[test]
    fn unknown_names_get_suggestions() {
        assert!(err(&[op("flex_coll")]).contains("did you mean: flex_col"));
        assert_eq!(suggest("items_centre").as_deref(), Some("items_center"));
        assert_eq!(suggest("text_colour").as_deref(), Some("text_color"));
        assert_eq!(suggest("rounde").as_deref(), Some("rounded"));
        assert_eq!(suggest("on_click"), None);
        assert!(err(&[opa("flex_col", vec![Arg::Num(1.0)])]).contains("`flex_col` takes no arguments"));
    }

    #[test]
    fn active_state_maps_to_press_feedback() {
        let mut o = op("active");
        o.nested = Some(("active".into(), vec![opa("opacity", vec![Arg::Num(0.6)]), opa("bg", vec![s("muted")])]));
        let r = resolve(&[o], &theme()).unwrap();
        assert_eq!(r.active_opacity, Some(0.6));
        assert_eq!(r.active_background, theme().color("muted"));
    }

    #[test]
    fn cache_interns_by_chain_key() {
        let ops = vec![op("p_2"), opa("bg", vec![s("#fff")])];
        let key = chain_key(&ops);
        assert_eq!(key, "p_2|bg(\"#fff\")");
        let mut c = StyleCache::default();
        let a = c.resolve(&key, &ops, &theme()).unwrap();
        let b = c.resolve(&key, &ops, &theme()).unwrap();
        assert!(Rc::ptr_eq(&a, &b));
        assert_eq!((c.hits, c.misses), (1, 1));
        assert!(c.get(&key).is_some());
        assert_eq!((c.hits, c.len()), (2, 1));
        c.clear();
        assert!(c.is_empty());
    }

    #[test]
    fn transitions_validate_properties() {
        let r = resolve(&[opa("transition", vec![s("opacity"), s("{\"duration\":120}")])], &theme()).unwrap();
        assert_eq!(r.transitions[0].duration, 120.0);
        assert!(resolve(&[opa("spring", vec![s("color")])], &theme()).is_err());
    }

    #[test]
    fn inline_shadow_replaces_the_layers() {
        let r = resolve(
            &[op("shadow_lg"), opa("shadow", vec![s("#000"), Arg::Num(0.0), Arg::Num(2.0), Arg::Num(4.0)])],
            &theme(),
        )
        .unwrap();
        assert_eq!(r.visual.shadows.len(), 1);
        assert_eq!(r.visual.shadow.map(|s| (s.y, s.blur)), Some((2.0, 4.0)));
    }

    #[test]
    fn rem_size_clamps_the_font_scale() {
        assert_eq!(rem_size(1.0), 16.0);
        assert_eq!(rem_size(1.3), 16.0 * 1.3);
        assert_eq!(rem_size(2.0), 16.0 * MAX_LAYOUT_SCALE);
        assert_eq!(rem_size(0.85), 16.0);
        assert_eq!(rem_size(f32::NAN), 16.0);
    }

    #[test]
    fn rem_lengths_scale_and_px_lengths_do_not() {
        let r = resolve(
            &[
                op("p_4"),
                opa("w", vec![Arg::Str("2.5rem".into())]),
                opa("h", vec![Arg::Num(40.0)]),
                op("w_1_2"),
                op("gap_2"),
                op("rounded_md"),
                op("border_1"),
                op("text_sm"),
            ],
            &theme(),
        )
        .unwrap();
        // `w_1_2` came after the rem width: the fraction wins and is not scaled.
        assert_eq!(r.rems.size, [None, None]);
        let l = r.scaled_layout(20.0);
        assert_eq!(l.padding.top, LengthPercentage::length(20.0));
        assert_eq!(l.gap.width, LengthPercentage::length(10.0));
        assert_eq!(l.size.width, Dimension::percent(0.5));
        assert_eq!(l.size.height, Dimension::length(40.0));
        assert_eq!(l.border.top, LengthPercentage::length(1.0));
        assert_eq!(r.scaled_radii(20.0), [7.5; 4]);
        assert_eq!(r.scaled_border_widths(20.0), [1.0; 4]);
        // Text resolves at the base rem; renderers apply the font scale.
        assert_eq!(r.text.size, Some(14.0));
        // At the base rem nothing changes.
        assert_eq!(r.scaled_layout(REM), r.layout);
    }

    #[test]
    fn hover_state_and_cursors() {
        let mut o = op("hover");
        o.nested = Some(("hover".into(), vec![opa("opacity", vec![Arg::Num(0.8)]), opa("bg", vec![s("muted")])]));
        let r = resolve(&[o, op("cursor_pointer")], &theme()).unwrap();
        assert_eq!(r.hover_opacity, Some(0.8));
        assert_eq!(r.hover_background, theme().color("muted"));
        assert_eq!(r.cursor, Some(CursorStyle::PointingHand));
        assert_eq!(r.active_opacity, None);
        // Every generated cursor method names a protocol cursor.
        let cursors: Vec<String> = nullary_names().into_iter().filter(|n| n.starts_with("cursor_")).collect();
        assert!(cursors.len() >= 21, "{cursors:?}");
        for name in &cursors {
            assert!(one(name).cursor.is_some(), "{name}");
        }
    }
}
