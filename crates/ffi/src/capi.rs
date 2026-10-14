//! The `extern "C"` functions of `include/native_shell.h`.
//!
//! Each one validates its pointers, converts to Rust types and calls
//! [`crate::instance`], which does the panic containment per instance. The
//! functions that return nothing report failures through `nsh_last_error`
//! only. Byte arguments are `(ptr, len)`; a NULL pointer with a zero length is
//! the empty string.

use std::ffi::c_char;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::{Arc, OnceLock};

use crate::buffer;
use crate::error;
use crate::host::{CallbacksHost, NSHBuffer, NSHHostCallbacks, NSHInstance};
use crate::instance;

/// # Safety
/// `ptr` must be NULL or point to `len` readable bytes that outlive the call.
unsafe fn bytes<'a>(ptr: *const u8, len: usize) -> Result<&'a [u8], String> {
    if len == 0 {
        Ok(&[])
    } else if ptr.is_null() {
        Err("NULL pointer with a non-zero length".into())
    } else {
        Ok(std::slice::from_raw_parts(ptr, len))
    }
}

/// # Safety
/// As [`bytes`].
unsafe fn text<'a>(ptr: *const u8, len: usize) -> Result<&'a str, String> {
    std::str::from_utf8(bytes(ptr, len)?).map_err(|e| format!("invalid UTF-8: {e}"))
}

/// Runs an entry point: clears the last error, catches panics outside any
/// instance (argument conversion), records a failure.
fn entry<R>(fallback: R, f: impl FnOnce() -> Result<R, String>) -> R {
    error::clear();
    match catch_unwind(AssertUnwindSafe(f)) {
        Ok(Ok(r)) => r,
        Ok(Err(e)) => {
            error::set(e);
            fallback
        }
        Err(p) => {
            error::set(format!("panic: {}", error::panic_message(p.as_ref())));
            fallback
        }
    }
}

/// # Safety
/// `callbacks` must point to a valid `NSHHostCallbacks` whose functions and
/// `context` stay valid until `nsh_instance_destroy` returns; `config_json`
/// must point to `len` bytes.
#[no_mangle]
pub unsafe extern "C" fn nsh_instance_create(
    callbacks: *const NSHHostCallbacks,
    config_json: *const u8,
    len: usize,
) -> NSHInstance {
    entry(0, || {
        if callbacks.is_null() {
            return Err("callbacks is NULL".into());
        }
        let callbacks = *callbacks;
        let config = bytes(config_json, len)?;
        instance::create(
            config,
            Box::new(move |id, poisoned, gate| Ok(Arc::new(CallbacksHost::new(callbacks, id, poisoned, gate)) as _)),
        )
    })
}

/// # Safety
/// `json` must point to `len` bytes.
#[no_mangle]
pub unsafe extern "C" fn nsh_instance_load_sources(instance: NSHInstance, json: *const u8, len: usize) -> i32 {
    entry(1, || instance::load_sources(instance, bytes(json, len)?).map(|_| 0))
}

/// # Safety
/// `data` must point to `len` bytes.
#[no_mangle]
pub unsafe extern "C" fn nsh_instance_load_bundle(instance: NSHInstance, data: *const u8, len: usize) -> i32 {
    entry(1, || instance::load_bundle(instance, bytes(data, len)?).map(|_| 0))
}

/// # Safety
/// `environment_json` must point to `len` bytes.
#[no_mangle]
pub unsafe extern "C" fn nsh_instance_set_environment(instance: NSHInstance, environment_json: *const u8, len: usize) {
    entry((), || instance::set_environment(instance, bytes(environment_json, len)?))
}

/// # Safety
/// `payload_json` must point to `len` bytes.
#[no_mangle]
pub unsafe extern "C" fn nsh_instance_dispatch_event(
    instance: NSHInstance,
    node: u32,
    event_type: u32,
    payload_json: *const u8,
    len: usize,
    seq: u32,
) {
    entry((), || instance::dispatch_event(instance, node, event_type, text(payload_json, len)?, seq))
}

/// # Safety
/// `json` must point to `len` bytes.
#[no_mangle]
pub unsafe extern "C" fn nsh_instance_host_respond(
    instance: NSHInstance,
    request_id: u64,
    ok: i32,
    json: *const u8,
    len: usize,
) {
    entry((), || instance::host_respond(instance, request_id, ok != 0, text(json, len)?))
}

/// # Safety
/// Each pointer must point to its length in bytes.
#[no_mangle]
pub unsafe extern "C" fn nsh_instance_host_emit(
    instance: NSHInstance,
    module: *const u8,
    module_len: usize,
    event: *const u8,
    event_len: usize,
    payload_json: *const u8,
    payload_len: usize,
) {
    entry((), || {
        instance::host_emit(
            instance,
            text(module, module_len)?,
            text(event, event_len)?,
            text(payload_json, payload_len)?,
        )
    })
}

#[no_mangle]
pub extern "C" fn nsh_instance_tick(instance: NSHInstance, now_ms: f64) -> i64 {
    entry(-1, || instance::tick(instance, now_ms))
}

#[no_mangle]
pub extern "C" fn nsh_instance_take_batch(instance: NSHInstance) -> NSHBuffer {
    entry(NSHBuffer::EMPTY, || Ok(instance::take_batch(instance)?.map(NSHBuffer::from_vec).unwrap_or(NSHBuffer::EMPTY)))
}

#[no_mangle]
pub extern "C" fn nsh_instance_set_active(instance: NSHInstance, active: i32) {
    entry((), || instance::set_active(instance, active != 0))
}

#[no_mangle]
pub extern "C" fn nsh_instance_trim_memory(instance: NSHInstance) {
    entry((), || instance::trim_memory(instance))
}

#[no_mangle]
pub extern "C" fn nsh_instance_metrics(instance: NSHInstance) -> NSHBuffer {
    entry(NSHBuffer::EMPTY, || Ok(NSHBuffer::from_vec(instance::metrics(instance)?.into_bytes())))
}

#[no_mangle]
pub extern "C" fn nsh_instance_destroy(instance: NSHInstance) {
    entry((), || instance::destroy(instance))
}

#[no_mangle]
pub extern "C" fn nsh_buffer_free(buffer: NSHBuffer) {
    entry((), || {
        buffer::free(buffer.handle);
        Ok(())
    })
}

/// # Safety
/// `data` must point to `len` bytes.
#[no_mangle]
pub unsafe extern "C" fn nsh_string_new(data: *const u8, len: usize) -> NSHBuffer {
    entry(NSHBuffer::EMPTY, || Ok(NSHBuffer::from_vec(bytes(data, len)?.to_vec())))
}

#[no_mangle]
pub extern "C" fn nsh_last_error() -> *const c_char {
    error::as_ptr()
}

#[no_mangle]
pub extern "C" fn nsh_abi_hash() -> *const c_char {
    static HASH: OnceLock<std::ffi::CString> = OnceLock::new();
    HASH.get_or_init(|| std::ffi::CString::new(native_shell_core::protocol::abi_hash()).unwrap_or_default()).as_ptr()
}

#[no_mangle]
pub extern "C" fn nsh_protocol_version() -> u32 {
    native_shell_core::protocol::PROTOCOL_VERSION
}
