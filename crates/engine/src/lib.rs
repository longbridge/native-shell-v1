//! A QuickJS-scripted, natively rendered UI runtime for iOS (UIKit) and Android
//! (View): the mobile sibling of gpui-shell.
//!
//! One [`Instance`] runs one program on one thread:
//!
//! ```text
//! dispatch_event / tick ─▶ JS (views, handlers, timers) ─▶ render(cx) ─▶ convert ─▶ Desc
//!                                                                          │
//!            take_batch ◀── encode ◀── commands ◀── Tree::commit ◀────────┘
//! ```
//!
//! Most of the programming model lives in JavaScript (`js/`, the prelude):
//! views, `cx` and its call scopes, the handler registry, overlays, input
//! state. Rust owns what must not be scriptable or must be fast: the sandbox
//! and resource budgets, the module loader, conversion of render output into
//! [`Desc`](native_shell_core::desc::Desc) with interned styles, the tree,
//! batches, event routing and coalescing, timers, and the host bridge.
//!
//! [`runner::ThreadedInstance`] puts an instance on its own thread for the
//! platform bindings; [`testing::Harness`] drives one headlessly for tests and
//! `native-shell check`.

mod assets;
mod convert;
mod error;
mod instance;
mod loader;
mod natives;
pub mod runner;
mod sandbox;
mod svg;
pub mod testing;
mod textview;

pub use error::{Error, ScriptError};
pub use instance::{Instance, InstanceConfig, Limits, Metrics};
pub use native_shell_core as core;
pub use native_shell_services::bundle::VerifiedBundle;
pub use native_shell_tree::Tree;
pub use natives::{Clock, ThemeConfig};
