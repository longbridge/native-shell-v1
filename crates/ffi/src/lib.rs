//! C ABI and JNI entry points of native-shell.
//!
//! `include/native_shell.h` is the contract; [`capi`] implements it one to one
//! and the JNI layer (`dev.nativeshell.NativeBridge`) maps onto the same
//! [`instance`] operations, so both platforms share one set of semantics:
//! integer instance ids, Rust-owned buffers freed by handle, per-thread last
//! error, and panics contained per instance.

mod buffer;
pub mod capi;
mod config;
mod error;
mod gate;
pub mod host;
mod instance;
#[cfg(any(target_os = "android", feature = "jni"))]
pub mod jni;

pub use capi::*;
pub use host::{NSHBuffer, NSHHostCallbacks, NSHInstance, NSHMeasureRequest, NSHSize, NSHTextMetrics};

pub const NSH_THREAD_DEDICATED: u32 = 0;
pub const NSH_THREAD_CALLER: u32 = 1;
pub const NSH_ABI_VERSION: u32 = 1;
