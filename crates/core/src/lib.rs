//! Shared contracts of native-shell.
//!
//! The pipeline, all on the instance's JS thread:
//!
//! ```text
//! render(cx) ─▶ Desc ─▶ keyed diff + flattening ─▶ taffy layout ─▶ mutation batch
//!   engine       desc        tree                      tree          protocol
//! ```
//!
//! The platform renderer applies each batch on its main thread and sends events
//! back. This crate holds what every other crate and both renderers agree on.

pub mod desc;
pub mod host;
pub mod manifest;
pub mod protocol;
pub mod style;
pub mod theme;

/// Engine identity baked into the ABI hash a signed bundle declares.
pub const ENGINE_VERSION: &str = concat!("native-shell/", env!("CARGO_PKG_VERSION"), "+quickjs-jit/0.12.6");
