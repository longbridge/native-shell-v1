//! `native-shell.d.ts`: the script API as TypeScript declarations.
//!
//! Scripts are plain JavaScript, so an editor's checker (`// @ts-check`, or
//! `checkJs` in the generated `jsconfig.json`) is the only checking they get
//! before running. The module surface (`typings.d.ts`) mirrors the engine's
//! prelude and reuses gpui-shell's declarations where the API is shared, so
//! gpui-shell programs type-check unchanged. The style vocabulary is generated
//! from the same tables the runtime resolves through (`nullary_names()` and
//! `PARAM`), and theme tokens from `COLOR_TOKENS`, so a name that type-checks
//! is a name the runtime accepts. Host modules and components come from the registry the app
//! exports, one `declare module` per module, so imports of app services are
//! checked too.

use std::fmt::Write as _;

use native_shell_core::host::{HostRegistry, MethodKind};
use native_shell_core::style::{nullary_names, PARAM};
use native_shell_core::theme::COLOR_TOKENS;

/// The hand-written part: the module surface of the engine's prelude, with
/// gpui-shell's declarations and documentation where the API is shared.
const TEMPLATE: &str = include_str!("typings.d.ts");

/// Accessibility roles `.role(...)` accepts (the engine's `A11Y_ROLES`).
pub const ROLES: &[&str] = &[
    "none", "button", "link", "header", "image", "text", "text_field", "switch", "checkbox", "radio", "slider",
    "progress_bar", "tab", "tab_list", "list", "list_item", "menu", "alert", "search",
];

/// Signature and documentation of each method that takes arguments. The
/// gpui-shell ones are its declarations verbatim: which length type a method
/// accepts follows its Rust signature, so `.p("auto")` and `.rounded("50%")`
/// are type errors here for the same reason they throw at run time.
fn param(name: &str) -> (&'static str, &'static str) {
    match name {
        "bg" => ("value: Color", "Sets the background colour."),
        "border" => ("value: AbsoluteLength", "Sets the border width on all four sides. Draws nothing without a colour."),
        "border_b" => ("value: AbsoluteLength", "Sets the border width on the bottom."),
        "border_color" => ("value: Color", "Sets the border colour. Draws nothing without a width."),
        "border_l" => ("value: AbsoluteLength", "Sets the border width on the left."),
        "border_r" => ("value: AbsoluteLength", "Sets the border width on the right."),
        "border_t" => ("value: AbsoluteLength", "Sets the border width on the top."),
        "border_x" => ("value: AbsoluteLength", "Sets the border width on the left and right."),
        "border_y" => ("value: AbsoluteLength", "Sets the border width on the top and bottom."),
        "bottom" => ("value: Length", "Sets the bottom offset of a positioned element."),
        "flex_basis" => ("value: Length", "Sets the size this child starts from before growing or shrinking."),
        "flex_grow" => ("value: number", "Sets how much of the free space this child takes."),
        "flex_shrink" => ("value: number", "Sets how readily this child gives space back."),
        "font_family" => ("value: string", "Sets the font family."),
        "font_weight" => ("value: number", "Sets the font weight to a number between 100 and 900."),
        "gap" => ("value: DefiniteLength", "Sets the gap between children on both axes."),
        "gap_x" => ("value: DefiniteLength", "Sets the gap between children along the main axis."),
        "gap_y" => ("value: DefiniteLength", "Sets the gap between children along the cross axis."),
        "h" => ("value: Length", "Sets the height."),
        "inset" => ("value: Length", "Sets all four offsets of a positioned element."),
        "left" => ("value: Length", "Sets the left offset of a positioned element."),
        "line_height" => (
            "value: DefiniteLength",
            "Sets the line height. A bare number is a multiplier (`1.45`), not pixels; a string is a length.",
        ),
        "m" => ("value: Length", "Sets the margin on all four sides."),
        "max_h" => ("value: Length", "Sets the maximum height."),
        "max_size" => ("value: Length", "Sets the maximum width and height together."),
        "max_w" => ("value: Length", "Sets the maximum width."),
        "mb" => ("value: Length", "Sets the margin on the bottom."),
        "min_h" => ("value: Length", "Sets the minimum height."),
        "min_size" => ("value: Length", "Sets the minimum width and height together."),
        "min_w" => ("value: Length", "Sets the minimum width."),
        "ml" => ("value: Length", "Sets the margin on the left."),
        "mr" => ("value: Length", "Sets the margin on the right."),
        "mt" => ("value: Length", "Sets the margin on the top."),
        "mx" => ("value: Length", "Sets the margin on the left and right."),
        "my" => ("value: Length", "Sets the margin on the top and bottom."),
        "opacity" => ("value: number", "Sets the opacity of the element and everything in it, from 0 to 1."),
        "p" => ("value: DefiniteLength", "Sets the padding on all four sides."),
        "pb" => ("value: DefiniteLength", "Sets the padding on the bottom."),
        "pl" => ("value: DefiniteLength", "Sets the padding on the left."),
        "pr" => ("value: DefiniteLength", "Sets the padding on the right."),
        "pt" => ("value: DefiniteLength", "Sets the padding on the top."),
        "px" => ("value: DefiniteLength", "Sets the padding on the left and right."),
        "py" => ("value: DefiniteLength", "Sets the padding on the top and bottom."),
        "right" => ("value: Length", "Sets the right offset of a positioned element."),
        "rounded" => ("value: AbsoluteLength", "Sets the corner radius on all four corners."),
        "rounded_b" => ("value: AbsoluteLength", "Sets the corner radius on the two bottom corners."),
        "rounded_bl" => ("value: AbsoluteLength", "Sets the corner radius on the bottom-left corner."),
        "rounded_br" => ("value: AbsoluteLength", "Sets the corner radius on the bottom-right corner."),
        "rounded_l" => ("value: AbsoluteLength", "Sets the corner radius on the two left corners."),
        "rounded_r" => ("value: AbsoluteLength", "Sets the corner radius on the two right corners."),
        "rounded_t" => ("value: AbsoluteLength", "Sets the corner radius on the two top corners."),
        "rounded_tl" => ("value: AbsoluteLength", "Sets the corner radius on the top-left corner."),
        "rounded_tr" => ("value: AbsoluteLength", "Sets the corner radius on the top-right corner."),
        "size" => ("value: Length", "Sets the width and the height together."),
        "text_bg" => ("value: Color", "Sets the background painted behind the text itself."),
        "text_color" => ("value: Color", "Sets the text colour, which children inherit."),
        "text_size" => ("value: AbsoluteLength", "Sets the font size."),
        "top" => ("value: Length", "Sets the top offset of a positioned element."),
        "w" => ("value: Length", "Sets the width."),
        // GPUI `Styled` methods gpui-shell does not bind.
        "aspect_ratio" => ("ratio: number", "Sets width / height."),
        "line_clamp" => ("lines: number", "Truncates the text after this many lines."),
        "grid_cols" => ("count: number", "A grid with this many equal columns."),
        "grid_rows" => ("count: number", "A grid with this many equal rows."),
        "col_span" => ("count: number", "Spans this many grid columns."),
        "row_span" => ("count: number", "Spans this many grid rows."),
        // Mobile additions.
        "shadow" => ("color: Color, x: number, y: number, blur: number, spread?: number", "Mobile: one drop shadow."),
        "letter_spacing" => ("value: AbsoluteLength", "Mobile: extra space between letters."),
        "bg_gradient" => ("angle: number, ...colors: Color[]", "Mobile: a linear gradient, evenly spaced stops."),
        "transition" => (
            "property: AnimatableProperty, options?: number | TransitionOptions",
            "Mobile: animates changes of `property` natively (a number is the duration in ms).",
        ),
        "spring" => ("property: AnimatableProperty, options?: SpringOptions", "Mobile: animates changes of `property` with a spring."),
        "translate" => ("x: number, y?: number", "Mobile: moves the element after layout, in points."),
        "scale" => ("value: number", "Mobile: scales the element after layout."),
        "rotate" => ("degrees: number", "Mobile: rotates the element after layout."),
        _ => ("value: Length", ""),
    }
}

const STYLE_TYPES: &str = r#"  /** What `transition` and `spring` animate. */
  export type AnimatableProperty = "opacity" | "width" | "height" | "left" | "top" | "frame" | "transform" | "background";
  export interface TransitionOptions {
    /** Milliseconds. */
    duration?: number;
    /** Milliseconds. */
    delay?: number;
    easing?: "linear" | "ease_in" | "ease_out" | "ease_in_out";
  }
  export interface SpringOptions {
    /** Seconds: the period of the undamped spring. */
    response?: number;
    /** Ratio; 1 has no overshoot. */
    damping?: number;
    /** Milliseconds. */
    delay?: number;
  }
"#;

fn style_methods() -> String {
    let mut out = String::from(STYLE_TYPES);
    out.push_str("\n  /** The style surface every element and `active(...)` block shares. */\n  export interface StyleMethods {\n");
    out.push_str(
        "    // Style methods that take an argument.\n",
    );
    for name in PARAM {
        let (args, doc) = param(name);
        if !doc.is_empty() {
            let _ = writeln!(out, "    /** {doc} */");
        }
        let _ = writeln!(out, "    {name}({args}): this;");
    }
    let nullary = nullary_names();
    let _ = writeln!(
        out,
        "\n    // The {} no-argument style methods, generated from GPUI's reflection table.",
        nullary.len()
    );
    for name in nullary {
        let _ = writeln!(out, "    {name}(): this;");
    }
    out.push_str("  }\n");
    out
}

fn host_components(registry: &HostRegistry) -> String {
    let mut out = String::from("\n  /** Props of the host components the app registered. */\n  export interface HostComponents {\n");
    for c in &registry.components {
        let props = c.props_ts.as_deref().unwrap_or("Props");
        let _ = writeln!(out, "    {}: {props};", ts_string(&c.name));
    }
    out.push_str("  }\n");
    out
}

pub fn generate(registry: &HostRegistry) -> String {
    let colors = COLOR_TOKENS.iter().map(|t| ts_string(t)).collect::<Vec<_>>().join(" | ");
    let roles = ROLES.iter().map(|t| ts_string(t)).collect::<Vec<_>>().join(" | ");
    let module_end = "\ndeclare module \"gpui-kit\"";
    let mut out = TEMPLATE.replace("/*@THEME_COLORS@*/", &colors).replace("/*@ROLES@*/", &roles);
    // Generated members go at the end of the "native-shell" module.
    let close = out.find(module_end).and_then(|i| out[..i].rfind('}')).expect("template has the native-shell module");
    out.insert_str(close, &(style_methods() + &host_components(registry)));

    for m in &registry.modules {
        let _ = writeln!(out, "\ndeclare module {} {{", ts_string(&m.name));
        out.push_str("  import { Context, HostValue } from \"native-shell\";\n");
        if let Some(dts) = &m.dts {
            for line in dts.lines() {
                let _ = writeln!(out, "  {line}");
            }
        }
        for method in &m.methods {
            let sig = match (&method.ts, method.kind) {
                (Some(ts), _) => ts.clone(),
                (None, MethodKind::Sync) => "(...args: HostValue[]) => any".into(),
                (None, MethodKind::Async) => "(...args: HostValue[]) => Promise<any>".into(),
            };
            let _ = writeln!(out, "  export const {}: {sig};", method.name);
        }
        let events = if m.events.is_empty() {
            "string".to_string()
        } else {
            m.events.iter().map(|e| ts_string(e)).collect::<Vec<_>>().join(" | ")
        };
        let _ = writeln!(
            out,
            "  /** Listens to an event the host emits; `cancel()` stops listening. */\n  export function on(event: {events}, handler: (payload: any, cx: Context) => void): {{ cancel(): void }};"
        );
        out.push_str("}\n");
    }
    out
}

fn ts_string(s: &str) -> String {
    serde_json::Value::String(s.to_string()).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use native_shell_core::host::{HostComponentDescriptor, HostMethod, HostModuleDescriptor};

    #[test]
    fn covers_vocabulary_and_registry() {
        let registry = HostRegistry {
            modules: vec![HostModuleDescriptor {
                name: "demo/quote".into(),
                methods: vec![
                    HostMethod { name: "quote".into(), kind: MethodKind::Async, ts: Some("(symbol: string) => Promise<Quote>".into()) },
                    HostMethod { name: "current".into(), kind: MethodKind::Sync, ts: None },
                ],
                events: vec!["tick".into()],
                requires_permission: true,
                dts: Some("export interface Quote { last: number }".into()),
            }],
            components: vec![HostComponentDescriptor {
                name: "demo-stock-cell".into(),
                events: vec![],
                measurable: true,
                requires_permission: true,
                props_ts: Some("{ symbol: string }".into()),
            }],
        };
        let d = generate(&registry);
        for name in nullary_names() {
            assert!(d.contains(&format!("  {name}(): this;")), "missing {name}");
        }
        for name in PARAM {
            assert!(d.contains(&format!("  {name}(")), "missing {name}");
        }
        assert!(d.contains("  p_4(): this;"));
        assert!(d.contains("  bg(value: Color): this;"));
        assert!(d.contains("  border(value: AbsoluteLength): this;"));
        assert!(d.contains("\"primary_foreground\""));
        assert!(d.contains("declare module \"demo/quote\" {"));
        assert!(d.contains("export const quote: (symbol: string) => Promise<Quote>;"));
        assert!(d.contains("export const current: (...args: HostValue[]) => any;"));
        assert!(d.contains("export interface Quote { last: number }"));
        assert!(d.contains("on(event: \"tick\""));
        assert!(d.contains("\"demo-stock-cell\": { symbol: string };"));
        assert!(!d.contains("/*@"), "a template placeholder was left in");
    }

    /// The declarations follow the engine's prelude: every name its public
    /// modules export is declared, and `.role(...)` takes exactly its roles.
    #[test]
    fn matches_the_engine_prelude() {
        let d = generate(&HostRegistry::default());
        let index = include_str!("../../engine/js/index.js");
        let exported: Vec<&str> = index
            .split("export {")
            .skip(1)
            .flat_map(|block| block.split('}').next().unwrap_or("").split(','))
            .map(str::trim)
            .filter(|n| !n.is_empty())
            .collect();
        assert!(exported.len() > 20, "could not read the prelude's exports");
        for name in exported {
            let declared = [format!("export function {name}("), format!("export const {name}:"), format!("export class {name} ")]
                .iter()
                .any(|decl| d.contains(decl.as_str()))
                || d.contains(&format!("export abstract class {name} "));
            assert!(declared, "`{name}` is exported by the engine but not declared");
        }
        assert!(include_str!("../../engine/js/component.js").contains("export class TitleBar"));
        assert!(d.contains("export class TitleBar"));

        let elements = include_str!("../../engine/js/elements.js");
        let roles = elements.split("const A11Y_ROLES = new Set([").nth(1).and_then(|r| r.split("]);").next()).unwrap_or("");
        let mut engine_roles: Vec<&str> = roles.split(',').map(|r| r.trim().trim_matches('"')).filter(|r| !r.is_empty()).collect();
        let mut ours = ROLES.to_vec();
        engine_roles.sort_unstable();
        ours.sort_unstable();
        assert_eq!(ours, engine_roles, "ROLES must match the engine's A11Y_ROLES");
    }
}
