//! What can go wrong while creating and loading an instance.
//!
//! Script failures after the first frame are not errors of this API: a render
//! that throws keeps the last good frame and shows a banner, a handler that
//! throws is logged. Only failures that leave nothing to show come back here.

use std::fmt;

use native_shell_core::manifest::ManifestError;
use rquickjs::Value;

/// Where in the program a script error happened, when the engine could tell.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScriptError {
    pub file: Option<String>,
    pub line: Option<u32>,
    pub message: String,
}

impl fmt::Display for ScriptError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match (&self.file, self.line) {
            (Some(file), Some(line)) => write!(f, "{file}:{line}: {}", self.message),
            (Some(file), None) => write!(f, "{file}: {}", self.message),
            _ => f.write_str(&self.message),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Manifest(#[from] ManifestError),
    /// The JS runtime could not be created, or the configuration is unusable.
    #[error("instance setup: {0}")]
    Setup(String),
    /// A module failed to resolve, compile or evaluate.
    #[error("{0}")]
    Module(ScriptError),
    /// An import or a placement the capabilities do not grant.
    #[error("{0}")]
    Capability(String),
    /// The entry view's `init` or first `render` failed; the error page is showing.
    #[error("first render failed: {0}")]
    FirstRender(ScriptError),
    #[error("bundle: {0}")]
    Bundle(String),
    /// `load_sources` / `load_bundle` called on an instance that already runs a program.
    #[error("this instance already runs a program; create a new instance to load another")]
    AlreadyLoaded,
}

/// Turns a caught JS value into a message with the program location it was thrown from.
pub(crate) fn script_error(value: &Value<'_>, trip: Option<String>) -> ScriptError {
    let (mut message, stack) = match value.as_object() {
        Some(obj) => {
            let msg: Option<String> = obj.get("message").ok();
            let name: Option<String> = obj.get("name").ok();
            let stack: Option<String> = obj.get("stack").ok();
            let text = match (name, msg) {
                (Some(n), Some(m)) => format!("{n}: {m}"),
                (None, Some(m)) => m,
                _ => value.as_string().and_then(|s| s.to_string().ok()).unwrap_or_else(|| "unknown error".into()),
            };
            (text, stack)
        }
        None => (
            value
                .as_string()
                .and_then(|s| s.to_string().ok())
                .or_else(|| value.as_number().map(|n| n.to_string()))
                .unwrap_or_else(|| "a non-Error value was thrown".into()),
            None,
        ),
    };
    if let Some(trip) = trip {
        message = trip;
    }
    let (file, line) = stack.as_deref().and_then(location).map_or((None, None), |(f, l)| (Some(f), Some(l)));
    ScriptError { file, line, message }
}

/// The first program frame of a QuickJS stack: `    at f (main.js:12:5)` → `("main.js", 12)`.
fn location(stack: &str) -> Option<(String, u32)> {
    stack.lines().find_map(|line| {
        let line = line.trim().trim_end_matches(')');
        let at = line.rfind(['(', ' ']).map_or(line, |i| &line[i + 1..]);
        let mut parts = at.rsplitn(3, ':');
        let (col, row, file) = (parts.next()?, parts.next()?, parts.next()?);
        col.parse::<u32>().ok()?;
        let row = row.parse::<u32>().ok()?;
        (!file.starts_with("nsh:") && !file.is_empty() && file != "<native>").then(|| (file.to_string(), row))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn locations_skip_prelude_frames() {
        let stack = "    at render (nsh:elements:12:3)\n    at row (lib/ui.js:7:15)\n    at main.js:3:1\n";
        assert_eq!(location(stack), Some(("lib/ui.js".into(), 7)));
        assert_eq!(location("    at <native>\n"), None);
    }
}
