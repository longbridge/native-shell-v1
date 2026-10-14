//! The resolved theme color tokens the core sends on the root node
//! (`Prop::ThemeTokens`), used by renderer-drawn chrome.

use std::collections::HashMap;

use native_shell_core::theme::Appearance;

#[derive(Debug, Clone, Default)]
pub struct ThemeTokens {
    pub colors: HashMap<String, u32>,
    pub appearance: Appearance,
}

impl ThemeTokens {
    pub fn parse(json: &str) -> Option<ThemeTokens> {
        let value: serde_json::Value = serde_json::from_str(json).ok()?;
        let object = value.as_object()?;
        let mut tokens = ThemeTokens::default();
        for (name, v) in object {
            if name == "appearance" {
                tokens.appearance = serde_json::from_value(v.clone()).unwrap_or_default();
            } else if let Some(c) = v.as_u64() {
                tokens.colors.insert(name.clone(), c as u32);
            }
        }
        Some(tokens)
    }

    pub fn color(&self, name: &str) -> Option<u32> {
        self.colors.get(name).copied()
    }
}
