//! [`Host`] backed by the C callback table (`NSHHostCallbacks`).
//!
//! The engine calls these on the instance's JS thread. A callback that panics
//! (possible when the host is Rust, or a language whose unwinding Rust can
//! catch) must not unwind into the engine: the call is caught here, the
//! instance is marked poisoned, `error` is reported, and a neutral value is
//! returned so the engine can finish the step. The next entry point that
//! touches a poisoned instance destroys it.
//!
//! Every callback also passes the instance's [`Gate`]: after destroy, none
//! runs (see [`crate::gate`]).

use std::ffi::c_void;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use native_shell_core::host::{Host, LogLevel, Measurer, TextMetrics, WidthMode};

use crate::buffer;
use crate::gate::Gate;
use crate::error::panic_message;

pub type NSHInstance = u64;

#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct NSHTextMetrics {
    pub width: f32,
    pub height: f32,
    pub baseline: f32,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct NSHSize {
    pub width: f32,
    pub height: f32,
    pub has_value: i32,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct NSHBuffer {
    pub ptr: *const u8,
    pub len: usize,
    pub handle: u64,
}

impl NSHBuffer {
    pub const EMPTY: NSHBuffer = NSHBuffer { ptr: std::ptr::null(), len: 0, handle: 0 };

    pub fn from_vec(bytes: Vec<u8>) -> NSHBuffer {
        let (ptr, len, handle) = buffer::insert(bytes);
        NSHBuffer { ptr, len, handle }
    }
}

/// `NSHMeasureRequest`: one text of a `measure_text_batch` call.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct NSHMeasureRequest {
    pub attributed_json: *const u8,
    pub len: usize,
    pub width: f32,
    pub width_mode: i32,
}

// The pointer types are declared `C-unwind` so a callback written in Rust may
// panic without aborting the process: the panic is caught in `CallbacksHost`.
pub type MeasureTextFn = unsafe extern "C-unwind" fn(*mut c_void, *const u8, usize, f32, i32) -> NSHTextMetrics;
pub type MeasureTextBatchFn = unsafe extern "C-unwind" fn(*mut c_void, *const NSHMeasureRequest, usize, *mut NSHTextMetrics);
pub type MeasureComponentFn =
    unsafe extern "C-unwind" fn(*mut c_void, *const u8, usize, *const u8, usize, f32, i32) -> NSHSize;
pub type BatchReadyFn = unsafe extern "C-unwind" fn(*mut c_void, NSHInstance);
pub type CallSyncFn = unsafe extern "C-unwind" fn(
    *mut c_void,
    NSHInstance,
    *const u8,
    usize,
    *const u8,
    usize,
    *const u8,
    usize,
    *mut NSHBuffer,
) -> i32;
pub type CallAsyncFn =
    unsafe extern "C-unwind" fn(*mut c_void, NSHInstance, u64, *const u8, usize, *const u8, usize, *const u8, usize);
pub type LogFn = unsafe extern "C-unwind" fn(*mut c_void, NSHInstance, i32, *const u8, usize);
pub type ErrorFn = unsafe extern "C-unwind" fn(*mut c_void, NSHInstance, *const u8, usize);

/// `NSHHostCallbacks`. Every pointer may be NULL; a missing callback behaves as
/// "not supported" (zero metrics, no intrinsic size, rejected calls, dropped logs).
#[repr(C)]
#[derive(Clone, Copy)]
pub struct NSHHostCallbacks {
    pub context: *mut c_void,
    pub measure_text: Option<MeasureTextFn>,
    /// Optional: when NULL a batch is measured one `measure_text` at a time.
    pub measure_text_batch: Option<MeasureTextBatchFn>,
    pub measure_component: Option<MeasureComponentFn>,
    pub batch_ready: Option<BatchReadyFn>,
    pub call_sync: Option<CallSyncFn>,
    pub call_async: Option<CallAsyncFn>,
    pub log: Option<LogFn>,
    pub error: Option<ErrorFn>,
}

pub struct CallbacksHost {
    callbacks: NSHHostCallbacks,
    instance: NSHInstance,
    poisoned: Arc<AtomicBool>,
    gate: Arc<Gate>,
}

// SAFETY: the C contract (native_shell.h) requires the callbacks to be
// thread-safe and `context` to stay valid until `nsh_instance_destroy` returns;
// the engine calls them only on the instance's JS thread. Under that contract
// sharing the table across threads is sound.
unsafe impl Send for CallbacksHost {}
unsafe impl Sync for CallbacksHost {}

impl CallbacksHost {
    pub fn new(callbacks: NSHHostCallbacks, instance: NSHInstance, poisoned: Arc<AtomicBool>, gate: Arc<Gate>) -> CallbacksHost {
        CallbacksHost { callbacks, instance, poisoned, gate }
    }

    /// Runs a callback, turning a panic into a poisoned instance and a reported error.
    fn guard<R>(&self, what: &str, fallback: R, f: impl FnOnce() -> R) -> R {
        let Some(_pass) = self.gate.enter() else { return fallback };
        match catch_unwind(AssertUnwindSafe(f)) {
            Ok(r) => r,
            Err(payload) => {
                self.poisoned.store(true, Ordering::SeqCst);
                let message = format!("host callback `{what}` panicked: {}", panic_message(payload.as_ref()));
                self.report_error(&message);
                fallback
            }
        }
    }

    /// `error`, itself guarded: a panicking error callback is swallowed.
    pub fn report_error(&self, message: &str) {
        let Some(_pass) = self.gate.enter() else { return };
        if let Some(f) = self.callbacks.error {
            let _ = catch_unwind(AssertUnwindSafe(|| unsafe {
                f(self.callbacks.context, self.instance, message.as_ptr(), message.len())
            }));
        }
    }
}

impl Measurer for CallbacksHost {
    fn measure_text(&self, attributed_json: &str, width: WidthMode) -> TextMetrics {
        let Some(f) = self.callbacks.measure_text else { return TextMetrics::default() };
        let (w, mode) = width.to_raw();
        self.guard("measure_text", TextMetrics::default(), || {
            let m = unsafe { f(self.callbacks.context, attributed_json.as_ptr(), attributed_json.len(), w, mode) };
            TextMetrics { width: m.width, height: m.height, baseline: m.baseline }
        })
    }

    fn measure_text_batch(&self, requests: &[(&str, WidthMode)]) -> Vec<TextMetrics> {
        let Some(f) = self.callbacks.measure_text_batch else {
            return requests.iter().map(|(json, width)| self.measure_text(json, *width)).collect();
        };
        let raw: Vec<NSHMeasureRequest> = requests
            .iter()
            .map(|(json, width)| {
                let (w, mode) = width.to_raw();
                NSHMeasureRequest { attributed_json: json.as_ptr(), len: json.len(), width: w, width_mode: mode }
            })
            .collect();
        let mut out = vec![NSHTextMetrics::default(); raw.len()];
        self.guard("measure_text_batch", (), || unsafe {
            f(self.callbacks.context, raw.as_ptr(), raw.len(), out.as_mut_ptr())
        });
        out.into_iter().map(|m| TextMetrics { width: m.width, height: m.height, baseline: m.baseline }).collect()
    }

    fn measure_component(&self, name: &str, props_json: &str, width: WidthMode) -> Option<(f32, f32)> {
        let f = self.callbacks.measure_component?;
        let (w, mode) = width.to_raw();
        self.guard("measure_component", None, || {
            let s = unsafe {
                f(self.callbacks.context, name.as_ptr(), name.len(), props_json.as_ptr(), props_json.len(), w, mode)
            };
            (s.has_value != 0).then_some((s.width, s.height))
        })
    }
}

impl Host for CallbacksHost {
    fn batch_ready(&self) {
        if let Some(f) = self.callbacks.batch_ready {
            self.guard("batch_ready", (), || unsafe { f(self.callbacks.context, self.instance) });
        }
    }

    fn call_sync(&self, module: &str, method: &str, args_json: &str) -> Result<String, String> {
        let Some(f) = self.callbacks.call_sync else {
            return Err(format!("{module}.{method}: the host has no call_sync callback"));
        };
        let failed = Err(format!("{module}.{method}: host callback panicked"));
        self.guard("call_sync", failed, || {
            let mut out = NSHBuffer::EMPTY;
            let rc = unsafe {
                f(
                    self.callbacks.context,
                    self.instance,
                    module.as_ptr(),
                    module.len(),
                    method.as_ptr(),
                    method.len(),
                    args_json.as_ptr(),
                    args_json.len(),
                    &mut out,
                )
            };
            let text = buffer::take(out.handle).map(|b| String::from_utf8_lossy(&b).into_owned()).unwrap_or_default();
            if rc == 0 {
                Ok(if text.is_empty() { "null".into() } else { text })
            } else if text.is_empty() {
                Err(format!("{module}.{method} failed"))
            } else {
                Err(text)
            }
        })
    }

    fn call_async(&self, request_id: u64, module: &str, method: &str, args_json: &str) {
        let Some(f) = self.callbacks.call_async else {
            self.log(LogLevel::Error, &format!("{module}.{method}: the host has no call_async callback"));
            return;
        };
        self.guard("call_async", (), || unsafe {
            f(
                self.callbacks.context,
                self.instance,
                request_id,
                module.as_ptr(),
                module.len(),
                method.as_ptr(),
                method.len(),
                args_json.as_ptr(),
                args_json.len(),
            )
        });
    }

    fn log(&self, level: LogLevel, message: &str) {
        let Some(f) = self.callbacks.log else { return };
        let level = match level {
            LogLevel::Debug => 0,
            LogLevel::Info => 1,
            LogLevel::Warn => 2,
            LogLevel::Error => 3,
        };
        self.guard("log", (), || unsafe {
            f(self.callbacks.context, self.instance, level, message.as_ptr(), message.len())
        });
    }

    fn error(&self, message: &str) {
        self.log(LogLevel::Error, message);
        self.report_error(message);
    }
}
