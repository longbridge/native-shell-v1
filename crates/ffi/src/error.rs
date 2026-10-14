//! `nsh_last_error`: the reason the last call on this thread failed.
//!
//! Thread-local so that two hosts driving instances from different threads
//! never read each other's errors, and a `CString` so the pointer handed out
//! stays valid until this thread makes its next call.

use std::cell::RefCell;
use std::ffi::CString;

thread_local! {
    static LAST_ERROR: RefCell<CString> = RefCell::new(CString::default());
}

pub fn set(message: impl Into<String>) {
    let mut bytes = message.into().into_bytes();
    bytes.retain(|b| *b != 0);
    // Interior NULs were just removed, so this cannot fail.
    let c = CString::new(bytes).unwrap_or_default();
    LAST_ERROR.with(|e| *e.borrow_mut() = c);
}

pub fn clear() {
    LAST_ERROR.with(|e| *e.borrow_mut() = CString::default());
}

#[cfg(any(target_os = "android", feature = "jni"))]
pub fn get() -> String {
    LAST_ERROR.with(|e| e.borrow().to_string_lossy().into_owned())
}

/// Pointer to this thread's message; valid until the next `set`/`clear` here.
pub fn as_ptr() -> *const std::ffi::c_char {
    LAST_ERROR.with(|e| e.borrow().as_ptr())
}

pub fn panic_message(payload: &(dyn std::any::Any + Send)) -> String {
    if let Some(s) = payload.downcast_ref::<&str>() {
        (*s).to_string()
    } else if let Some(s) = payload.downcast_ref::<String>() {
        s.clone()
    } else {
        "unknown panic".to_string()
    }
}
