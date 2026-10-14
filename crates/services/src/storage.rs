//! `localStorage` and `sessionStorage`.
//!
//! Both are the same store; `sessionStorage` simply has no file. Reads answer
//! from memory (scripts call `getItem` from `render`), and writes reach the
//! disk later on a writer thread so a `setItem` never blocks a frame.
//!
//! Persistence is ordered by revision rather than a dirty flag: every mutation
//! bumps `revision`, and a write records the revision it landed. Two writers
//! (the background one and an explicit `flush`) serialize on `write_lock`, and
//! a writer whose snapshot is older than what already reached the disk skips,
//! so an older snapshot can never land after a newer one.
//!
//! A file that does not parse is moved aside to `localStorage.json.corrupt` and
//! the store starts empty: on a phone there is no one to repair it by hand, and
//! keeping the bytes lets support recover them.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Mutex;

use crate::Limits;

pub const LOCAL_FILE: &str = "localStorage.json";

pub struct Store {
    path: Option<PathBuf>,
    values: Option<BTreeMap<String, String>>,
    bytes: usize,
    revision: u64,
    /// A writer thread is running and will pick up any newer revision.
    writing: bool,
    /// The revision the last writer was started for.
    queued: u64,
}

/// Why a `setItem` was refused; scripts see a `QuotaExceededError`.
#[derive(Debug, PartialEq, Eq)]
pub struct Quota(pub String);

impl Store {
    pub fn new(path: Option<PathBuf>) -> Store {
        Store { path, values: None, bytes: 0, revision: 0, writing: false, queued: 0 }
    }

    pub fn in_memory() -> Store {
        Store::new(None)
    }

    fn values(&mut self) -> &mut BTreeMap<String, String> {
        if self.values.is_none() {
            let values = self.path.as_ref().map(load).unwrap_or_default();
            self.bytes = values.iter().map(|(k, v)| k.len() + v.len()).sum();
            self.values = Some(values);
        }
        self.values.get_or_insert_with(BTreeMap::new)
    }

    pub fn get(&mut self, key: &str) -> Option<String> {
        self.values().get(key).cloned()
    }

    pub fn set(&mut self, key: &str, value: &str, limits: &Limits) -> Result<(), Quota> {
        if value.len() > limits.storage_value_bytes {
            return Err(Quota(format!(
                "Storage.setItem(\"{key}\"): value is {} bytes, over the {}-byte limit per value",
                value.len(),
                limits.storage_value_bytes
            )));
        }
        let bytes = self.bytes;
        let values = self.values();
        let old = values.get(key).map(|v| key.len() + v.len());
        if old.is_none() && values.len() >= limits.storage_keys {
            return Err(Quota(format!(
                "Storage.setItem(\"{key}\"): the store already holds {} keys, the limit",
                limits.storage_keys
            )));
        }
        let next = bytes - old.unwrap_or(0) + key.len() + value.len();
        if next > limits.storage_total_bytes {
            return Err(Quota(format!(
                "Storage.setItem(\"{key}\"): the store would hold {next} bytes, over the {}-byte limit",
                limits.storage_total_bytes
            )));
        }
        if values.get(key).map(String::as_str) == Some(value) {
            return Ok(());
        }
        values.insert(key.to_string(), value.to_string());
        self.bytes = next;
        self.revision += 1;
        Ok(())
    }

    pub fn remove(&mut self, key: &str) {
        if let Some(v) = self.values().remove(key) {
            self.bytes -= key.len() + v.len();
            self.revision += 1;
        }
    }

    pub fn clear(&mut self) {
        if !self.values().is_empty() {
            self.values().clear();
            self.bytes = 0;
            self.revision += 1;
        }
    }

    pub fn key(&mut self, index: usize) -> Option<String> {
        self.values().keys().nth(index).cloned()
    }

    pub fn len(&mut self) -> usize {
        self.values().len()
    }

    /// The bytes to write if memory is ahead of `written`.
    fn snapshot(&self, written: u64) -> Option<(u64, PathBuf, Vec<u8>)> {
        let path = self.path.clone()?;
        let values = self.values.as_ref()?;
        if self.revision <= written {
            return None;
        }
        let body = serde_json::to_vec(values).ok()?;
        Some((self.revision, path, body))
    }
}

fn load(path: &PathBuf) -> BTreeMap<String, String> {
    let Ok(bytes) = std::fs::read(path) else {
        return BTreeMap::new();
    };
    match serde_json::from_slice(&bytes) {
        Ok(values) => values,
        Err(_) => {
            let _ = std::fs::rename(path, path.with_extension("json.corrupt"));
            BTreeMap::new()
        }
    }
}

/// A persisted store plus the bookkeeping that orders its writes.
pub struct Persisted {
    pub store: Mutex<Store>,
    /// The revision on disk; held for the whole write so writers serialize.
    write_lock: Mutex<u64>,
}

impl Persisted {
    pub fn new(store: Store) -> Persisted {
        Persisted { store: Mutex::new(store), write_lock: Mutex::new(0) }
    }

    /// Writes the current revision synchronously (`flush`).
    pub fn flush(&self) -> std::io::Result<()> {
        let mut written = lock(&self.write_lock);
        let snapshot = lock(&self.store).snapshot(*written);
        if let Some((revision, path, body)) = snapshot {
            write_atomic(&path, &body)?;
            *written = revision;
        }
        Ok(())
    }

    /// Starts a background writer if memory is ahead of the disk and none is
    /// running. The writer loops until the disk has caught up.
    pub fn persist_in_background(self: &std::sync::Arc<Self>) {
        {
            let mut store = lock(&self.store);
            // Lock order is `write_lock` then `store`, so the disk revision is
            // not consulted here; a writer with nothing to do exits at once.
            if store.writing || store.path.is_none() || store.revision == store.queued {
                return;
            }
            store.writing = true;
            store.queued = store.revision;
        }
        let this = self.clone();
        std::thread::spawn(move || loop {
            let mut written = lock(&this.write_lock);
            let snapshot = {
                let mut store = lock(&this.store);
                let snapshot = store.snapshot(*written);
                if snapshot.is_none() {
                    store.writing = false;
                }
                snapshot
            };
            let Some((revision, path, body)) = snapshot else { return };
            if write_atomic(&path, &body).is_err() {
                // Leave it dirty; the next mutation or `flush` tries again.
                lock(&this.store).writing = false;
                return;
            }
            *written = revision;
        });
    }
}

pub(crate) fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

/// Writes `<path>.tmp` then renames it over `path`, so a crash leaves either
/// the old file or the new one, never half of one.
pub fn write_atomic(path: &std::path::Path, body: &[u8]) -> std::io::Result<()> {
    use std::io::Write as _;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("json.tmp");
    let mut file = std::fs::File::create(&tmp)?;
    file.write_all(body)?;
    file.sync_all()?;
    drop(file);
    std::fs::rename(&tmp, path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_and_limits() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(LOCAL_FILE);
        let limits = Limits { storage_total_bytes: 16, storage_keys: 2, storage_value_bytes: 8, ..Limits::default() };
        let p = Persisted::new(Store::new(Some(path.clone())));
        {
            let mut s = lock(&p.store);
            s.set("a", "1", &limits).unwrap();
            s.set("b", "22", &limits).unwrap();
            assert!(s.set("c", "3", &limits).unwrap_err().0.contains("keys"));
            assert!(s.set("a", "123456789", &limits).unwrap_err().0.contains("per value"));
            assert!(s.set("a", "12345678", &limits).is_ok());
            assert!(s.set("b", "12345678", &limits).unwrap_err().0.contains("bytes"));
            assert_eq!(s.key(1).as_deref(), Some("b"));
            s.remove("b");
            assert_eq!(s.len(), 1);
        }
        p.flush().unwrap();
        let mut again = Store::new(Some(path.clone()));
        assert_eq!(again.get("a").as_deref(), Some("12345678"));
        assert_eq!(again.len(), 1);

        std::fs::write(&path, b"{not json").unwrap();
        let mut broken = Store::new(Some(path.clone()));
        assert_eq!(broken.len(), 0);
        assert!(path.with_extension("json.corrupt").exists());
    }
}
