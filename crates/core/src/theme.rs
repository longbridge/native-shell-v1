//! Theme tokens: the 17 colors of gpui-shell plus spacing and radius scales.
//!
//! Colors are resolved to ARGB in Rust when a style is interned, so a theme
//! change clears the style cache and re-renders every view; renderers never see
//! a token name.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const COLOR_TOKENS: [&str; 17] = [
    "background",
    "foreground",
    "surface",
    "surface_foreground",
    "primary",
    "primary_foreground",
    "secondary",
    "secondary_foreground",
    "muted",
    "muted_foreground",
    "accent",
    "accent_foreground",
    "destructive",
    "destructive_foreground",
    "border",
    "input",
    "ring",
];

const LIGHT: [&str; 17] = [
    "#ffffff", "#0a0a0a", "#ffffff", "#0a0a0a", "#171717", "#fafafa", "#e5e5e5", "#171717", "#f5f5f5", "#737373",
    "#f5f5f5", "#171717", "#ef4444", "#fafafa", "#e5e5e5", "#e5e5e5", "#a3a3a3",
];
const DARK: [&str; 17] = [
    "#0a0a0a", "#fafafa", "#0a0a0a", "#fafafa", "#fafafa", "#171717", "#262626", "#fafafa", "#262626", "#a3a3a3",
    "#262626", "#fafafa", "#f87171", "#dc2626", "#262626", "#2f2f2f", "#737373",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Appearance {
    #[default]
    Light,
    Dark,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Theme {
    pub appearance: Appearance,
    pub colors: BTreeMap<String, u32>,
    pub spacing: BTreeMap<String, f32>,
    pub radius: BTreeMap<String, f32>,
}

impl Theme {
    pub fn default_for(appearance: Appearance) -> Self {
        let table = match appearance {
            Appearance::Light => LIGHT,
            Appearance::Dark => DARK,
        };
        let colors = COLOR_TOKENS.iter().zip(table).map(|(k, v)| (k.to_string(), parse_hex(v).unwrap())).collect();
        let spacing = [("xxs", 2.0), ("xs", 4.0), ("sm", 8.0), ("md", 12.0), ("lg", 16.0), ("xl", 24.0), ("xxl", 32.0)]
            .into_iter()
            .map(|(k, v)| (k.to_string(), v))
            .collect();
        let radius = [("none", 0.0), ("sm", 4.0), ("md", 6.0), ("lg", 8.0), ("xl", 12.0), ("full", 9999.0)]
            .into_iter()
            .map(|(k, v)| (k.to_string(), v))
            .collect();
        Theme { appearance, colors, spacing, radius }
    }

    /// `set_theme({appearance, tokens})`: overrides on top of the defaults for that appearance.
    pub fn with_overrides(appearance: Appearance, tokens: &BTreeMap<String, String>) -> Result<Self, String> {
        let mut theme = Theme::default_for(appearance);
        for (k, v) in tokens {
            if !COLOR_TOKENS.contains(&k.as_str()) {
                return Err(format!("unknown theme token `{k}`"));
            }
            let c = parse_hex(v).ok_or_else(|| format!("theme token `{k}`: `{v}` is not a #rgb/#rrggbb/#rrggbbaa color"))?;
            theme.colors.insert(k.clone(), c);
        }
        Ok(theme)
    }

    /// A token name, `transparent`, or a hex literal.
    pub fn color(&self, value: &str) -> Option<u32> {
        if let Some(c) = self.colors.get(value) {
            return Some(*c);
        }
        match value {
            "transparent" => Some(0),
            "white" => Some(0xFFFF_FFFF),
            "black" => Some(0xFF00_0000),
            _ => parse_hex(value).or_else(|| parse_rgb_fn(value)),
        }
    }

    pub fn is_dark(&self) -> bool {
        self.appearance == Appearance::Dark
    }

    /// The deep-frozen snapshot `cx.theme()` returns.
    pub fn to_json(&self) -> serde_json::Value {
        let colors: serde_json::Map<_, _> =
            self.colors.iter().map(|(k, v)| (k.clone(), serde_json::Value::String(to_hex(*v)))).collect();
        serde_json::json!({
            "appearance": self.appearance,
            "is_dark": self.is_dark(),
            "colors": colors,
            "spacing": self.spacing,
            "radius": self.radius,
        })
    }
}

/// `#rgb`, `#rgba`, `#rrggbb`, `#rrggbbaa` to `0xAARRGGBB`.
pub fn parse_hex(s: &str) -> Option<u32> {
    let h = s.strip_prefix('#')?;
    if !h.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let n = |i: usize, len: usize| u32::from_str_radix(&h[i..i + len], 16).ok();
    let (r, g, b, a) = match h.len() {
        3 => (n(0, 1)? * 17, n(1, 1)? * 17, n(2, 1)? * 17, 255),
        4 => (n(0, 1)? * 17, n(1, 1)? * 17, n(2, 1)? * 17, n(3, 1)? * 17),
        6 => (n(0, 2)?, n(2, 2)?, n(4, 2)?, 255),
        8 => (n(0, 2)?, n(2, 2)?, n(4, 2)?, n(6, 2)?),
        _ => return None,
    };
    Some((a << 24) | (r << 16) | (g << 8) | b)
}

/// `rgb(1, 2, 3)` / `rgba(1, 2, 3, 0.5)`.
fn parse_rgb_fn(s: &str) -> Option<u32> {
    let inner = s.strip_prefix("rgba(").or_else(|| s.strip_prefix("rgb("))?.strip_suffix(')')?;
    let parts: Vec<f32> = inner.split(',').map(|p| p.trim().parse().ok()).collect::<Option<_>>()?;
    let (r, g, b, a) = match parts.as_slice() {
        [r, g, b] => (*r, *g, *b, 1.0),
        [r, g, b, a] => (*r, *g, *b, *a),
        _ => return None,
    };
    let c = |v: f32| v.clamp(0.0, 255.0).round() as u32;
    Some((c(a * 255.0) << 24) | (c(r) << 16) | (c(g) << 8) | c(b))
}

pub fn to_hex(c: u32) -> String {
    let a = c >> 24;
    if a == 255 {
        format!("#{:06x}", c & 0xFF_FFFF)
    } else {
        format!("#{:06x}{:02x}", c & 0xFF_FFFF, a)
    }
}

/// Multiply a color's alpha.
pub fn with_alpha(c: u32, alpha: f32) -> u32 {
    let a = ((c >> 24) as f32 * alpha.clamp(0.0, 1.0)).round() as u32;
    (a << 24) | (c & 0xFF_FFFF)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_forms() {
        assert_eq!(parse_hex("#fff"), Some(0xFFFF_FFFF));
        assert_eq!(parse_hex("#112233"), Some(0xFF11_2233));
        assert_eq!(parse_hex("#11223380"), Some(0x8011_2233));
        assert_eq!(parse_hex("#12"), None);
        assert_eq!(parse_hex("123456"), None);
        assert_eq!(to_hex(0x8011_2233), "#11223380");
    }

    #[test]
    fn tokens_resolve_per_appearance() {
        let light = Theme::default_for(Appearance::Light);
        let dark = Theme::default_for(Appearance::Dark);
        assert_eq!(light.color("background"), Some(0xFFFF_FFFF));
        assert_eq!(dark.color("background"), Some(0xFF0A_0A0A));
        assert_eq!(light.colors.len(), 17);
        assert_eq!(light.color("rgba(255, 0, 0, 0.5)"), Some(0x80FF_0000));
        assert_eq!(light.color("nope"), None);
    }

    #[test]
    fn overrides_validate_names_and_values() {
        let mut t = BTreeMap::new();
        t.insert("primary".to_string(), "#ff0000".to_string());
        assert_eq!(Theme::with_overrides(Appearance::Light, &t).unwrap().color("primary"), Some(0xFFFF_0000));
        t.insert("primay".to_string(), "#ff0000".to_string());
        assert!(Theme::with_overrides(Appearance::Light, &t).is_err());
    }
}
