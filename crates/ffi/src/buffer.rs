//! Byte buffers handed across the ABI, owned by Rust and freed by handle.
//!
//! The platform gets `(ptr, len, handle)` and gives back only the handle, so a
//! double free, a free of `0` or a free of a forged handle is a no-op instead of
//! heap corruption. The bytes live in a `Vec` whose heap allocation never moves
//! while it sits in the table, so `ptr` stays valid until the handle is freed.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, MutexGuard, OnceLock};

static NEXT: AtomicU64 = AtomicU64::new(1);

fn table() -> MutexGuard<'static, HashMap<u64, Vec<u8>>> {
    static TABLE: OnceLock<Mutex<HashMap<u64, Vec<u8>>>> = OnceLock::new();
    // A panic while holding this lock cannot leave the map inconsistent (all
    // operations are single inserts/removes), so poisoning is ignored.
    TABLE.get_or_init(Default::default).lock().unwrap_or_else(|p| p.into_inner())
}

/// Stores `bytes` and returns `(ptr, len, handle)`. Empty input gives `(null, 0, 0)`.
pub fn insert(bytes: Vec<u8>) -> (*const u8, usize, u64) {
    if bytes.is_empty() {
        return (std::ptr::null(), 0, 0);
    }
    let handle = NEXT.fetch_add(1, Ordering::Relaxed);
    let ptr = bytes.as_ptr();
    let len = bytes.len();
    table().insert(handle, bytes);
    (ptr, len, handle)
}

/// Frees a buffer; unknown handles (including 0 and already freed ones) are ignored.
pub fn free(handle: u64) {
    if handle != 0 {
        table().remove(&handle);
    }
}

/// Moves the bytes out of the table (used for `call_sync` answers).
pub fn take(handle: u64) -> Option<Vec<u8>> {
    table().remove(&handle)
}

/// `(ptr, len)` of a live buffer.
#[cfg(any(target_os = "android", feature = "jni", test))]
pub fn get(handle: u64) -> Option<(*const u8, usize)> {
    table().get(&handle).map(|v| (v.as_ptr(), v.len()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn handles_free_once() {
        let (ptr, len, h) = insert(b"abc".to_vec());
        assert_eq!(len, 3);
        assert_eq!(unsafe { std::slice::from_raw_parts(ptr, len) }, b"abc");
        assert_eq!(get(h).map(|(_, l)| l), Some(3));
        free(h);
        free(h);
        free(0);
        free(u64::MAX);
        assert!(get(h).is_none());
        assert_eq!(insert(Vec::new()).2, 0);
    }
}
