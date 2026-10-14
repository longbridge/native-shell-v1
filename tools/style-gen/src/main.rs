//! Generates `crates/core/src/style_table.rs`: every no-argument style method
//! GPUI exposes, translated into a neutral list of effects that
//! `native_shell_core::style` applies without depending on GPUI.
//!
//! # How the table is derived
//!
//! The method names come from GPUI's inspector reflection
//! (`gpui::styled_reflection::methods`), which covers exactly the
//! `fn(self) -> Self` methods of `Styled` — the same source gpui-shell binds.
//! Each method is invoked (`FunctionReflection::invoke`) twice:
//!
//! * on `StyleRefinement::default()`, where every field is unset, and
//! * on a *saturated* refinement, where every field holds a sentinel value.
//!
//! A field is written by the method when it changed in either run. The two runs
//! must agree on the written value — a method that writes a constant — or the
//! generator stops: a value that depends on what was there before needs a
//! dedicated translation (see `underline` and `grid_location`, which GPUI fills
//! with `get_or_insert_default` and then modifies one part of). A write of
//! `None` is a reset, which is how `content_normal` and
//! `text_decoration_none` are expressed.
//!
//! Every field of `StyleRefinement` and `TextStyleRefinement` is destructured
//! without `..`, so an upstream field this generator does not know about is a
//! compile error, and a known field that has no translation panics when a
//! method touches it.
//!
//! # Methods reflection does not reach
//!
//! gpui-shell also binds the reflected methods of `gpui_base::StyledExt` and
//! the `font_*` weights that `StyledExt` generates with a macro (which the
//! reflection pass does not expand). `gpui-base` is not published on crates.io,
//! so those methods are transcribed below from its source (`h_flex`, `v_flex`,
//! `debug_*`, `font_*`) and run through the same diff. The scroll methods
//! (`overflow_*_scroll`) belong to GPUI's `StatefulInteractiveElement` rather
//! than `Styled`, and the `overflow_*_scrollbar` variants to gpui-base's
//! scrollable; on mobile both mean "this axis scrolls", so they are transcribed
//! as the overflow writes GPUI's `overflow_*_scroll` performs.
//!
//! Usage: `cargo run` (writes the file), `cargo run -- --check` (fails if the
//! checked-in file differs).

use std::fmt::Debug;
use std::fmt::Write as _;

use gpui::{
    AbsoluteLength, BoxShadow, DefiniteLength, Fill, FontWeight, GridLocation, GridPlacement,
    GridTemplate, GridTemplateMinSize, Hsla, Length, Overflow, Rgba, StyleRefinement, Styled,
    TextOverflow, TextStyleRefinement, UnderlineStyle, hsla,
};

type Method = fn(StyleRefinement) -> StyleRefinement;

/// The `fn(self) -> Self` methods of `gpui_base::StyledExt` that its reflection
/// exposes, transcribed from `crates/base/src/styled.rs` of gpui-kit.
const BASE_REFLECTED: &[(&str, Method)] = &[
    ("h_flex", |s| s.flex().flex_row().items_center()),
    ("v_flex", |s| s.flex().flex_col()),
    ("debug_red", |s| {
        s.border_1().border_color(hsl(0., 72.2, 50.6))
    }),
    ("debug_blue", |s| {
        s.border_1().border_color(hsl(217.2, 91.2, 59.8))
    }),
    ("debug_yellow", |s| {
        s.border_1().border_color(hsl(47.9, 95.8, 53.1))
    }),
    ("debug_green", |s| {
        s.border_1().border_color(hsl(142.1, 70.6, 45.3))
    }),
    ("debug_pink", |s| {
        s.border_1().border_color(hsl(330.4, 81.2, 60.4))
    }),
];

/// `debug_*` only draw in debug builds of the host (`cfg!(debug_assertions)`).
const DEBUG_ONLY: &[&str] = &[
    "debug_red",
    "debug_blue",
    "debug_yellow",
    "debug_green",
    "debug_pink",
];

/// The macro-generated `font_*` methods of `gpui_base::StyledExt`, which
/// gpui-shell binds by hand for the same reason.
const BASE_TRANSCRIBED: &[(&str, Method)] = &[
    ("font_thin", |s| s.font_weight(FontWeight::THIN)),
    ("font_extralight", |s| {
        s.font_weight(FontWeight::EXTRA_LIGHT)
    }),
    ("font_light", |s| s.font_weight(FontWeight::LIGHT)),
    ("font_normal", |s| s.font_weight(FontWeight::NORMAL)),
    ("font_medium", |s| s.font_weight(FontWeight::MEDIUM)),
    ("font_semibold", |s| s.font_weight(FontWeight::SEMIBOLD)),
    ("font_bold", |s| s.font_weight(FontWeight::BOLD)),
    ("font_extrabold", |s| s.font_weight(FontWeight::EXTRA_BOLD)),
    ("font_black", |s| s.font_weight(FontWeight::BLACK)),
];

/// Scroll methods of `StatefulInteractiveElement` (and gpui-base's scrollbar
/// variants), as the overflow writes they perform.
const ELEMENT_TRANSCRIBED: &[(&str, Method)] = &[
    ("overflow_scroll", |mut s| {
        s.overflow.x = Some(Overflow::Scroll);
        s.overflow.y = Some(Overflow::Scroll);
        s
    }),
    ("overflow_x_scroll", |mut s| {
        s.overflow.x = Some(Overflow::Scroll);
        s
    }),
    ("overflow_y_scroll", |mut s| {
        s.overflow.y = Some(Overflow::Scroll);
        s
    }),
    ("overflow_scrollbar", |mut s| {
        s.overflow.x = Some(Overflow::Scroll);
        s.overflow.y = Some(Overflow::Scroll);
        s
    }),
    ("overflow_x_scrollbar", |mut s| {
        s.overflow.x = Some(Overflow::Scroll);
        s
    }),
    ("overflow_y_scrollbar", |mut s| {
        s.overflow.y = Some(Overflow::Scroll);
        s
    }),
];

fn hsl(hue: f32, saturation: f32, lightness: f32) -> Hsla {
    hsla(hue / 360., saturation / 100., lightness / 100., 1.)
}

// GPUI gates `debug`/`debug_below` on debug_assertions, and the table must not
// depend on the build profile.
#[cfg(not(debug_assertions))]
compile_error!("build style-gen without --release");

fn main() {
    let mut rows: Vec<(String, Vec<String>)> = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let mut add =
        |name: &str, method: &dyn Fn(StyleRefinement) -> StyleRefinement, rows: &mut Vec<_>| {
            // gpui-shell resolves `StyledExt` first, then `Styled`; the first name wins.
            if !seen.insert(name.to_string()) {
                return false;
            }
            let mut effects = translate(name, method);
            if DEBUG_ONLY.contains(&name) {
                effects = vec![format!("Effect::DebugOnly(&[{}])", effects.join(", "))];
            }
            rows.push((name.to_string(), effects));
            true
        };

    let mut base_reflected = 0;
    for (name, method) in BASE_REFLECTED {
        base_reflected += usize::from(add(name, method, &mut rows));
    }
    let mut gpui_reflected = 0;
    for method in gpui::styled_reflection::methods::<StyleRefinement>() {
        gpui_reflected += usize::from(add(method.name, &|s| method.invoke(s), &mut rows));
    }
    let mut base_transcribed = 0;
    for (name, method) in BASE_TRANSCRIBED {
        base_transcribed += usize::from(add(name, method, &mut rows));
    }
    let mut element_transcribed = 0;
    for (name, method) in ELEMENT_TRANSCRIBED {
        element_transcribed += usize::from(add(name, method, &mut rows));
    }
    rows.sort();

    let mut out = String::new();
    out.push_str("// @generated by tools/style-gen; do not edit.\n");
    out.push_str(
        "//\n// Regenerate with `cargo run --manifest-path tools/style-gen/Cargo.toml`.\n",
    );
    out.push_str("// Source: gpui-pre 0.3.9 `Styled` reflection, plus the gpui-base and element\n");
    out.push_str("// methods transcribed in tools/style-gen/src/main.rs.\n\n");
    out.push_str("#![allow(clippy::approx_constant, clippy::excessive_precision, clippy::unreadable_literal)]\n\n");
    // Import only what the rows name, so the file compiles without warnings.
    let body: String = rows
        .iter()
        .flat_map(|(_, e)| e.iter().map(String::as_str))
        .collect();
    let used = |names: &[&'static str]| -> Vec<&'static str> {
        names
            .iter()
            .copied()
            .filter(|n| body.contains(&format!("{n}::")) || body.contains(&format!("{n} {{")))
            .collect()
    };
    let local = used(&[
        "Axis",
        "Corner",
        "Decoration",
        "Edge",
        "Effect",
        "GridMin",
        "GridSlot",
        "Len",
        "Placement",
        "ShadowLayer",
        "TextAlignment",
        "Truncation",
    ]);
    let taffy = used(&[
        "AlignContent",
        "AlignItems",
        "Display",
        "FlexDirection",
        "FlexWrap",
        "Overflow",
        "Position",
    ]);
    let _ = writeln!(out, "use super::{{{}}};", local.join(", "));
    let _ = writeln!(out, "use taffy::style::{{{}}};\n", taffy.join(", "));
    let _ = writeln!(
        out,
        "/// Methods reflected from GPUI's `Styled`.\n#[cfg_attr(not(test), allow(dead_code))]\npub(crate) const GPUI_REFLECTED: usize = {gpui_reflected};"
    );
    let _ = writeln!(
        out,
        "/// Methods reflected from gpui-base's `StyledExt`.\n#[cfg_attr(not(test), allow(dead_code))]\npub(crate) const BASE_REFLECTED: usize = {base_reflected};"
    );
    let _ = writeln!(
        out,
        "/// gpui-base methods its reflection does not reach (`font_*`).\n#[cfg_attr(not(test), allow(dead_code))]\npub(crate) const BASE_TRANSCRIBED: usize = {base_transcribed};"
    );
    let _ = writeln!(
        out,
        "/// Element scroll methods recorded as style on mobile.\n#[cfg_attr(not(test), allow(dead_code))]\npub(crate) const ELEMENT_TRANSCRIBED: usize = {element_transcribed};"
    );
    let _ = writeln!(
        out,
        "/// Every row of [`NULLARY`].\n#[cfg_attr(not(test), allow(dead_code))]\npub(crate) const NULLARY_COUNT: usize = {};\n",
        rows.len()
    );
    out.push_str("/// Every no-argument style method and what it writes, sorted by name.\n");
    out.push_str("pub(crate) static NULLARY: &[(&str, &[Effect])] = &[\n");
    for (name, effects) in &rows {
        let _ = writeln!(out, "    ({name:?}, &[{}]),", effects.join(", "));
    }
    out.push_str("];\n");

    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("repository root");
    let target = root.join("crates/core/src/style_table.rs");
    if std::env::args().any(|a| a == "--check") {
        let current = std::fs::read_to_string(&target).unwrap_or_default();
        if current != out {
            eprintln!("{} is out of date; run tools/style-gen", target.display());
            std::process::exit(1);
        }
        println!(
            "{} is up to date ({} methods)",
            target.display(),
            rows.len()
        );
    } else {
        std::fs::write(&target, out).expect("write style_table.rs");
        println!(
            "wrote {} methods ({gpui_reflected} gpui + {base_reflected} gpui-base reflected + \
             {base_transcribed} gpui-base transcribed + {element_transcribed} element) to {}",
            rows.len(),
            target.display()
        );
    }
}

/// A refinement with every field set to a sentinel that no method writes in
/// combination with the default run, so a write of any value is visible in at
/// least one of the two runs.
fn saturated() -> StyleRefinement {
    use gpui::*;
    let len = || Some(Length::Definite(px(777.).into()));
    let def = || Some(DefiniteLength::from(px(777.)));
    let abs = || Some(AbsoluteLength::from(px(777.)));
    let color = hsla(0.5, 0.5, 0.5, 0.5);
    StyleRefinement {
        display: Some(Display::None),
        visibility: Some(Visibility::Hidden),
        overflow: PointRefinement {
            x: Some(Overflow::Clip),
            y: Some(Overflow::Clip),
        },
        scrollbar_width: abs(),
        allow_concurrent_scroll: Some(true),
        restrict_scroll_to_axis: Some(true),
        position: Some(Position::Absolute),
        inset: EdgesRefinement {
            top: len(),
            right: len(),
            bottom: len(),
            left: len(),
        },
        size: SizeRefinement {
            width: len(),
            height: len(),
        },
        min_size: SizeRefinement {
            width: len(),
            height: len(),
        },
        max_size: SizeRefinement {
            width: len(),
            height: len(),
        },
        aspect_ratio: Some(7.77),
        margin: EdgesRefinement {
            top: len(),
            right: len(),
            bottom: len(),
            left: len(),
        },
        padding: EdgesRefinement {
            top: def(),
            right: def(),
            bottom: def(),
            left: def(),
        },
        border_widths: EdgesRefinement {
            top: abs(),
            right: abs(),
            bottom: abs(),
            left: abs(),
        },
        align_items: Some(AlignItems::End),
        align_self: Some(AlignSelf::End),
        align_content: Some(AlignContent::End),
        justify_content: Some(JustifyContent::FlexEnd),
        gap: SizeRefinement {
            width: def(),
            height: def(),
        },
        flex_direction: Some(FlexDirection::ColumnReverse),
        flex_wrap: Some(FlexWrap::WrapReverse),
        flex_basis: len(),
        flex_grow: Some(7.77),
        flex_shrink: Some(7.77),
        background: Some(Fill::from(color)),
        border_color: Some(color),
        border_style: Some(BorderStyle::Dashed),
        corner_radii: CornersRefinement {
            top_left: abs(),
            top_right: abs(),
            bottom_right: abs(),
            bottom_left: abs(),
        },
        box_shadow: Some(vec![BoxShadow::new(px(7.), px(7.), color)]),
        text: TextStyleRefinement {
            color: Some(color),
            font_family: Some("sentinel".into()),
            font_features: Some(FontFeatures::default()),
            font_fallbacks: Some(FontFallbacks::from_fonts(vec!["sentinel".into()])),
            font_size: abs(),
            line_height: def(),
            font_weight: Some(FontWeight(777.)),
            font_style: Some(FontStyle::Oblique),
            background_color: Some(color),
            underline: Some(UnderlineStyle {
                thickness: px(7.),
                color: Some(color),
                wavy: true,
            }),
            strikethrough: Some(StrikethroughStyle {
                thickness: px(7.),
                color: Some(color),
            }),
            white_space: Some(WhiteSpace::Nowrap),
            text_overflow: Some(TextOverflow::Truncate("sentinel".into())),
            text_align: Some(TextAlign::Right),
            line_clamp: Some(777),
        },
        mouse_cursor: Some(CursorStyle::Crosshair),
        opacity: Some(0.777),
        grid_cols: Some(GridTemplate {
            repeat: 77,
            min_size: GridTemplateMinSize::MaxContent,
        }),
        grid_rows: Some(GridTemplate {
            repeat: 77,
            min_size: GridTemplateMinSize::MaxContent,
        }),
        grid_location: Some(GridLocation {
            row: GridPlacement::Line(77)..GridPlacement::Line(78),
            column: GridPlacement::Line(79)..GridPlacement::Line(80),
        }),
        debug: Some(true),
        debug_below: Some(true),
    }
}

/// The two runs of one method, one field at a time.
struct Runs<'a> {
    name: &'a str,
    out: Vec<String>,
}

impl Runs<'_> {
    /// `None` when the field was not written; otherwise the written value
    /// (`Some(None)` for a reset). Panics when the two runs disagree.
    fn leaf<T: PartialEq + Debug + Clone>(
        &self,
        field: &str,
        default_out: &Option<T>,
        saturated_in: &Option<T>,
        saturated_out: &Option<T>,
    ) -> Option<Option<T>> {
        let written = default_out.is_some() || saturated_out != saturated_in;
        if !written {
            return None;
        }
        assert_eq!(
            default_out, saturated_out,
            "`{}` writes `{field}` depending on its previous value; give it a dedicated translation",
            self.name
        );
        Some(default_out.clone())
    }

    fn push(&mut self, effect: String) {
        self.out.push(effect);
    }

    /// The written value of a field that cannot be reset.
    fn set<T>(&self, field: &str, v: Option<T>) -> T {
        v.unwrap_or_else(|| panic!("`{}` resets `{field}`, which has no translation", self.name))
    }

    fn unsupported(&self, field: &str, value: &dyn Debug) -> ! {
        panic!(
            "`{}` writes `{field}` = {value:?}, which has no translation",
            self.name
        )
    }
}

fn translate(name: &str, method: &dyn Fn(StyleRefinement) -> StyleRefinement) -> Vec<String> {
    let d = method(StyleRefinement::default());
    let s_in = saturated();
    let s = method(s_in.clone());
    let mut runs = Runs {
        name,
        out: Vec::new(),
    };

    // Exhaustive: a new upstream field is a compile error here.
    let StyleRefinement {
        display,
        visibility,
        overflow,
        scrollbar_width,
        allow_concurrent_scroll,
        restrict_scroll_to_axis,
        position,
        inset,
        size,
        min_size,
        max_size,
        aspect_ratio,
        margin,
        padding,
        border_widths,
        align_items,
        align_self,
        align_content,
        justify_content,
        gap,
        flex_direction,
        flex_wrap,
        flex_basis,
        flex_grow,
        flex_shrink,
        background,
        border_color,
        border_style,
        corner_radii,
        box_shadow,
        text,
        mouse_cursor,
        opacity,
        grid_cols,
        grid_rows,
        grid_location,
        debug,
        debug_below,
    } = &d;

    macro_rules! leaf {
        ($($path:ident).+) => {
            runs.leaf(stringify!($($path).+), &d.$($path).+, &s_in.$($path).+, &s.$($path).+)
        };
    }

    if let Some(v) = leaf!(display) {
        let v = runs.set("display", v);
        runs.push(format!("Effect::Display(Display::{v:?})"));
    }
    let _ = display;
    if let Some(v) = leaf!(visibility) {
        let v = runs.set("visibility", v);
        runs.push(format!(
            "Effect::Visible({})",
            v == gpui::Visibility::Visible
        ));
    }
    let _ = visibility;
    let _ = overflow;
    for (axis, v) in [("X", leaf!(overflow.x)), ("Y", leaf!(overflow.y))] {
        if let Some(v) = v {
            let v = runs.set("overflow", v);
            runs.push(format!("Effect::Overflow{axis}(Overflow::{v:?})"));
        }
    }
    if let Some(v) = leaf!(scrollbar_width) {
        let v = runs.set("scrollbar_width", v);
        runs.push(format!("Effect::ScrollbarWidth({})", abs_len(v)));
    }
    let _ = scrollbar_width;
    if let Some(v) = leaf!(allow_concurrent_scroll) {
        runs.unsupported("allow_concurrent_scroll", &v);
    }
    let _ = allow_concurrent_scroll;
    if let Some(v) = leaf!(restrict_scroll_to_axis) {
        runs.unsupported("restrict_scroll_to_axis", &v);
    }
    let _ = restrict_scroll_to_axis;
    if let Some(v) = leaf!(position) {
        let v = runs.set("position", v);
        runs.push(format!("Effect::Position(Position::{v:?})"));
    }
    let _ = position;

    let _ = (
        inset,
        margin,
        padding,
        border_widths,
        size,
        min_size,
        max_size,
        gap,
        corner_radii,
    );
    for (edge, v) in [
        ("Top", leaf!(inset.top)),
        ("Right", leaf!(inset.right)),
        ("Bottom", leaf!(inset.bottom)),
        ("Left", leaf!(inset.left)),
    ] {
        if let Some(v) = v {
            let v = runs.set("inset", v);
            runs.push(format!("Effect::Inset(Edge::{edge}, {})", length(v)));
        }
    }
    for (field, label, w, h) in [
        ("size", "Size", leaf!(size.width), leaf!(size.height)),
        (
            "min_size",
            "MinSize",
            leaf!(min_size.width),
            leaf!(min_size.height),
        ),
        (
            "max_size",
            "MaxSize",
            leaf!(max_size.width),
            leaf!(max_size.height),
        ),
    ] {
        for (axis, v) in [("Width", w), ("Height", h)] {
            if let Some(v) = v {
                let v = v.unwrap_or_else(|| runs.unsupported(field, &v));
                runs.push(format!("Effect::{label}(Axis::{axis}, {})", length(v)));
            }
        }
    }
    if let Some(v) = leaf!(aspect_ratio) {
        let v = runs.set("aspect_ratio", v);
        runs.push(format!("Effect::AspectRatio({})", f(v)));
    }
    let _ = aspect_ratio;
    for (edge, v) in [
        ("Top", leaf!(margin.top)),
        ("Right", leaf!(margin.right)),
        ("Bottom", leaf!(margin.bottom)),
        ("Left", leaf!(margin.left)),
    ] {
        if let Some(v) = v {
            let v = runs.set("margin", v);
            runs.push(format!("Effect::Margin(Edge::{edge}, {})", length(v)));
        }
    }
    for (edge, v) in [
        ("Top", leaf!(padding.top)),
        ("Right", leaf!(padding.right)),
        ("Bottom", leaf!(padding.bottom)),
        ("Left", leaf!(padding.left)),
    ] {
        if let Some(v) = v {
            let v = runs.set("padding", v);
            runs.push(format!("Effect::Padding(Edge::{edge}, {})", definite(v)));
        }
    }
    for (edge, v) in [
        ("Top", leaf!(border_widths.top)),
        ("Right", leaf!(border_widths.right)),
        ("Bottom", leaf!(border_widths.bottom)),
        ("Left", leaf!(border_widths.left)),
    ] {
        if let Some(v) = v {
            let v = runs.set("border_widths", v);
            runs.push(format!("Effect::BorderWidth(Edge::{edge}, {})", abs_len(v)));
        }
    }

    if let Some(v) = leaf!(align_items) {
        runs.push(format!(
            "Effect::AlignItems({})",
            opt(v.map(|v| format!("AlignItems::{v:?}")))
        ));
    }
    let _ = align_items;
    if let Some(v) = leaf!(align_self) {
        runs.push(format!(
            "Effect::AlignSelf({})",
            opt(v.map(|v| format!("AlignItems::{v:?}")))
        ));
    }
    let _ = align_self;
    if let Some(v) = leaf!(align_content) {
        runs.push(format!(
            "Effect::AlignContent({})",
            opt(v.map(|v| format!("AlignContent::{v:?}")))
        ));
    }
    let _ = align_content;
    if let Some(v) = leaf!(justify_content) {
        runs.push(format!(
            "Effect::JustifyContent({})",
            opt(v.map(|v| format!("AlignContent::{v:?}")))
        ));
    }
    let _ = justify_content;
    for (axis, v) in [("Width", leaf!(gap.width)), ("Height", leaf!(gap.height))] {
        if let Some(v) = v {
            let v = runs.set("gap", v);
            runs.push(format!("Effect::Gap(Axis::{axis}, {})", definite(v)));
        }
    }

    if let Some(v) = leaf!(flex_direction) {
        let v = runs.set("flex_direction", v);
        runs.push(format!("Effect::FlexDirection(FlexDirection::{v:?})"));
    }
    let _ = flex_direction;
    if let Some(v) = leaf!(flex_wrap) {
        let v = runs.set("flex_wrap", v);
        runs.push(format!("Effect::FlexWrap(FlexWrap::{v:?})"));
    }
    let _ = flex_wrap;
    if let Some(v) = leaf!(flex_basis) {
        let v = runs.set("flex_basis", v);
        runs.push(format!("Effect::FlexBasis({})", length(v)));
    }
    let _ = flex_basis;
    if let Some(v) = leaf!(flex_grow) {
        let v = runs.set("flex_grow", v);
        runs.push(format!("Effect::FlexGrow({})", f(v)));
    }
    let _ = flex_grow;
    if let Some(v) = leaf!(flex_shrink) {
        let v = runs.set("flex_shrink", v);
        runs.push(format!("Effect::FlexShrink({})", f(v)));
    }
    let _ = flex_shrink;

    if let Some(v) = leaf!(background) {
        let v = v.map(|fill| match fill {
            Fill::Color(bg) => match bg.as_solid() {
                Some(c) => argb(c),
                None => runs.unsupported("background", &bg),
            },
        });
        runs.push(format!("Effect::Background({})", opt(v)));
    }
    let _ = background;
    if let Some(v) = leaf!(border_color) {
        runs.push(format!("Effect::BorderColor({})", opt(v.map(argb))));
    }
    let _ = border_color;
    if let Some(v) = leaf!(border_style) {
        let v = runs.set("border_style", v);
        runs.push(format!(
            "Effect::BorderDashed({})",
            v == gpui::BorderStyle::Dashed
        ));
    }
    let _ = border_style;
    for (corner, v) in [
        ("TopLeft", leaf!(corner_radii.top_left)),
        ("TopRight", leaf!(corner_radii.top_right)),
        ("BottomRight", leaf!(corner_radii.bottom_right)),
        ("BottomLeft", leaf!(corner_radii.bottom_left)),
    ] {
        if let Some(v) = v {
            let v = runs.set("corner_radii", v);
            runs.push(format!(
                "Effect::CornerRadius(Corner::{corner}, {})",
                abs_len(v)
            ));
        }
    }
    if let Some(v) = leaf!(box_shadow) {
        let v = runs.set("box_shadow", v);
        let layers: Vec<String> = v.iter().map(shadow).collect();
        runs.push(format!("Effect::BoxShadow(&[{}])", layers.join(", ")));
    }
    let _ = box_shadow;

    translate_text(&mut runs, text, &s_in.text, &s.text);

    if let Some(v) = leaf!(mouse_cursor) {
        let v = runs.set("mouse_cursor", v);
        runs.push(format!("Effect::Cursor({:?})", format!("{v:?}")));
    }
    let _ = mouse_cursor;
    if let Some(v) = leaf!(opacity) {
        let v = runs.set("opacity", v);
        runs.push(format!("Effect::Opacity({})", f(v)));
    }
    let _ = opacity;
    if let Some(v) = leaf!(grid_cols) {
        runs.push(format!("Effect::GridCols({})", opt(v.map(grid_template))));
    }
    let _ = grid_cols;
    if let Some(v) = leaf!(grid_rows) {
        runs.push(format!("Effect::GridRows({})", opt(v.map(grid_template))));
    }
    let _ = grid_rows;

    // `grid_location_mut` is `get_or_insert_default`, then one part is written:
    // an unset location reads as the default (all `Auto`), and each part is a leaf.
    let read = |l: &Option<GridLocation>| {
        let l = l.clone().unwrap_or_default();
        [l.row.start, l.row.end, l.column.start, l.column.end]
    };
    if grid_location.is_some() || s.grid_location != s_in.grid_location {
        if s.grid_location.is_none() {
            runs.unsupported("grid_location", &s.grid_location);
        }
        let (dd, si, so) = (
            read(grid_location),
            read(&s_in.grid_location),
            read(&s.grid_location),
        );
        let default = read(&None);
        for (i, slot) in ["RowStart", "RowEnd", "ColumnStart", "ColumnEnd"]
            .iter()
            .enumerate()
        {
            let written = dd[i] != default[i] || so[i] != si[i];
            if written {
                assert_eq!(
                    dd[i], so[i],
                    "`{name}` writes grid_location.{slot} depending on its previous value"
                );
                runs.push(format!(
                    "Effect::Grid(GridSlot::{slot}, {})",
                    placement(dd[i])
                ));
            }
        }
    }

    if let Some(v) = leaf!(debug) {
        runs.push(format!("Effect::Debug({:?})", v.unwrap_or(false)));
    }
    let _ = debug;
    if let Some(v) = leaf!(debug_below) {
        runs.push(format!("Effect::DebugBelow({:?})", v.unwrap_or(false)));
    }
    let _ = debug_below;

    assert!(
        !runs.out.is_empty() || name == "content_normal" || name == "text_decoration_none",
        "`{name}` writes nothing"
    );
    runs.out
}

fn translate_text(
    runs: &mut Runs<'_>,
    d: &TextStyleRefinement,
    s_in: &TextStyleRefinement,
    s: &TextStyleRefinement,
) {
    let TextStyleRefinement {
        color,
        font_family,
        font_features,
        font_fallbacks,
        font_size,
        line_height,
        font_weight,
        font_style,
        background_color,
        underline,
        strikethrough,
        white_space,
        text_overflow,
        text_align,
        line_clamp,
    } = d;

    macro_rules! leaf {
        ($field:ident) => {
            runs.leaf(
                concat!("text.", stringify!($field)),
                $field,
                &s_in.$field,
                &s.$field,
            )
        };
    }

    if let Some(v) = leaf!(color) {
        let v = runs.set("text.color", v);
        runs.push(format!("Effect::TextColor({})", argb(v)));
    }
    if let Some(v) = leaf!(font_family) {
        runs.unsupported("text.font_family", &v);
    }
    if let Some(v) = leaf!(font_features) {
        runs.unsupported("text.font_features", &v);
    }
    if let Some(v) = leaf!(font_fallbacks) {
        runs.unsupported("text.font_fallbacks", &v);
    }
    if let Some(v) = leaf!(font_size) {
        let v = runs.set("text.font_size", v);
        runs.push(format!("Effect::FontSize({})", abs_len(v)));
    }
    if let Some(v) = leaf!(line_height) {
        let v = runs.set("text.line_height", v);
        runs.push(format!("Effect::LineHeight({})", definite(v)));
    }
    if let Some(v) = leaf!(font_weight) {
        let v = runs.set("text.font_weight", v);
        runs.push(format!("Effect::FontWeight({})", f(v.0)));
    }
    if let Some(v) = leaf!(font_style) {
        let v = runs.set("text.font_style", v);
        let italic = match v {
            gpui::FontStyle::Normal => false,
            gpui::FontStyle::Italic => true,
            other => runs.unsupported("text.font_style", &other),
        };
        runs.push(format!("Effect::Italic({italic})"));
    }
    if let Some(v) = leaf!(background_color) {
        runs.push(format!("Effect::TextBackground({})", opt(v.map(argb))));
    }

    // `underline` is either replaced whole (`underline`, `text_decoration_none`)
    // or ensured with `get_or_insert_with(Default::default)` and then one part
    // changed (`text_decoration_*`); tell the two apart by whether the runs agree.
    if underline.is_some() || s.underline != s_in.underline {
        if *underline == s.underline {
            runs.push(format!(
                "Effect::Underline({})",
                opt(underline.map(|u| decoration(u.thickness.as_f32(), u.wavy, u.color)))
            ));
        } else {
            let (Some(dd), Some(si), Some(so)) = (underline, &s_in.underline, &s.underline) else {
                runs.unsupported("text.underline", &s.underline);
            };
            let default = UnderlineStyle::default();
            runs.push("Effect::UnderlineEnsure".into());
            if dd.thickness != default.thickness || so.thickness != si.thickness {
                assert_eq!(
                    dd.thickness, so.thickness,
                    "`{}`: underline thickness",
                    runs.name
                );
                runs.push(format!(
                    "Effect::UnderlineThickness({})",
                    f(dd.thickness.as_f32())
                ));
            }
            if dd.wavy != default.wavy || so.wavy != si.wavy {
                assert_eq!(dd.wavy, so.wavy, "`{}`: underline wavy", runs.name);
                runs.push(format!("Effect::UnderlineWavy({})", dd.wavy));
            }
            if dd.color != default.color || so.color != si.color {
                runs.unsupported("text.underline.color", &dd.color);
            }
        }
    }
    if let Some(v) = leaf!(strikethrough) {
        runs.push(format!(
            "Effect::Strikethrough({})",
            opt(v.map(|s| decoration(s.thickness.as_f32(), false, s.color)))
        ));
    }
    if let Some(v) = leaf!(white_space) {
        let v = runs.set("text.white_space", v);
        runs.push(format!("Effect::Nowrap({})", v == gpui::WhiteSpace::Nowrap));
    }
    if let Some(v) = leaf!(text_overflow) {
        let v = v.map(|o| {
            let (at, ellipsis) = match o {
                TextOverflow::Truncate(e) => ("End", e),
                TextOverflow::TruncateStart(e) => ("Start", e),
                TextOverflow::TruncateMiddle(e) => ("Middle", e),
            };
            assert_eq!(
                ellipsis.as_ref(),
                "…",
                "`{}`: unexpected ellipsis",
                runs.name
            );
            format!("Truncation::{at}")
        });
        runs.push(format!("Effect::TextOverflow({})", opt(v)));
    }
    if let Some(v) = leaf!(text_align) {
        let v = runs.set("text.text_align", v);
        runs.push(format!("Effect::TextAlign(TextAlignment::{v:?})"));
    }
    if let Some(v) = leaf!(line_clamp) {
        runs.push(format!(
            "Effect::LineClamp({})",
            opt(v.map(|n| n.to_string()))
        ));
    }
    let _ = (
        color,
        font_family,
        font_features,
        font_fallbacks,
        font_size,
        line_height,
        font_weight,
        font_style,
    );
    let _ = (
        background_color,
        strikethrough,
        white_space,
        text_overflow,
        text_align,
        line_clamp,
    );
}

fn opt(v: Option<impl std::fmt::Display>) -> String {
    match v {
        Some(v) => format!("Some({v})"),
        None => "None".into(),
    }
}

/// An `f32` as a Rust literal that reads back to the same value.
fn f(v: f32) -> String {
    assert!(v.is_finite(), "non-finite value {v}");
    let s = format!("{v:?}");
    if s.contains(['.', 'e']) {
        s
    } else {
        format!("{s}.0")
    }
}

fn abs_len(v: AbsoluteLength) -> String {
    match v {
        AbsoluteLength::Pixels(p) => format!("Len::Px({})", f(p.as_f32())),
        AbsoluteLength::Rems(r) => format!("Len::Rems({})", f(r.0)),
    }
}

fn definite(v: DefiniteLength) -> String {
    match v {
        DefiniteLength::Absolute(a) => abs_len(a),
        DefiniteLength::Fraction(x) => format!("Len::Fraction({})", f(x)),
    }
}

fn length(v: Length) -> String {
    match v {
        Length::Definite(d) => definite(d),
        Length::Auto => "Len::Auto".into(),
    }
}

/// `Hsla` as `0xAARRGGBB`, the colour form of the mobile protocol.
fn argb(c: Hsla) -> String {
    let c = Rgba::from(c);
    let ch = |x: f32| (x.clamp(0., 1.) * 255.).round() as u32;
    format!(
        "0x{:08X}",
        (ch(c.a) << 24) | (ch(c.r) << 16) | (ch(c.g) << 8) | ch(c.b)
    )
}

fn shadow(s: &BoxShadow) -> String {
    format!(
        "ShadowLayer {{ color: {}, x: {}, y: {}, blur: {}, spread: {}, inset: {} }}",
        argb(s.color),
        f(s.offset.x.as_f32()),
        f(s.offset.y.as_f32()),
        f(s.blur_radius.as_f32()),
        f(s.spread_radius.as_f32()),
        s.inset
    )
}

fn decoration(thickness: f32, wavy: bool, color: Option<Hsla>) -> String {
    format!(
        "Decoration {{ thickness: {}, wavy: {wavy}, color: {} }}",
        f(thickness),
        opt(color.map(argb))
    )
}

fn grid_template(t: GridTemplate) -> String {
    let min = match t.min_size {
        GridTemplateMinSize::Zero => "Zero",
        GridTemplateMinSize::MinContent => "MinContent",
        GridTemplateMinSize::MaxContent => "MaxContent",
    };
    format!("({}, GridMin::{min})", t.repeat)
}

fn placement(p: GridPlacement) -> String {
    match p {
        GridPlacement::Auto => "Placement::Auto".into(),
        GridPlacement::Line(l) => format!("Placement::Line({l})"),
        GridPlacement::Span(s) => format!("Placement::Span({s})"),
    }
}
