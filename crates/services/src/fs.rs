//! `fs/promises` confined to `<data_dir>/files`.
//!
//! Paths from scripts are relative to that root. A path is refused before it
//! touches the disk if it is absolute or has a `..` component, and refused
//! after resolution if the deepest existing ancestor canonicalizes outside the
//! root, which is how a symlink planted inside the root is kept from leading
//! out of it.

use std::path::{Component, Path, PathBuf};

use crate::Limits;

pub struct Fs {
    root: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Stat {
    pub size: u64,
    pub is_file: bool,
    pub is_dir: bool,
    pub mtime_ms: f64,
}

type R<T> = Result<T, String>;

impl Fs {
    pub fn new(data_dir: Option<&Path>) -> Fs {
        Fs { root: data_dir.map(|d| d.join("files")) }
    }

    fn root(&self) -> R<PathBuf> {
        let root = self.root.as_ref().ok_or("fs: this instance has no data directory")?;
        std::fs::create_dir_all(root).map_err(|e| format!("fs: cannot create {}: {e}", root.display()))?;
        root.canonicalize().map_err(|e| format!("fs: {e}"))
    }

    /// Resolves a script path inside the root, refusing escapes.
    pub fn resolve(&self, path: &str) -> R<PathBuf> {
        let root = self.root()?;
        if path.starts_with('/') || path.starts_with('\\') || path.contains('\0') {
            return Err(format!("fs: `{path}` must be relative to the program's data directory"));
        }
        let mut full = root.clone();
        for part in Path::new(path).components() {
            match part {
                Component::Normal(p) => full.push(p),
                Component::CurDir => {}
                _ => return Err(format!("fs: `{path}` leaves the program's data directory")),
            }
        }
        // The deepest ancestor that exists decides where the path really goes.
        let mut probe = full.as_path();
        loop {
            if std::fs::symlink_metadata(probe).is_ok() {
                let real = probe.canonicalize().map_err(|e| format!("fs: `{path}`: {e}"))?;
                if !real.starts_with(&root) {
                    return Err(format!("fs: `{path}` leaves the program's data directory"));
                }
                break;
            }
            match probe.parent() {
                Some(parent) => probe = parent,
                None => break,
            }
        }
        Ok(full)
    }

    pub fn read(&self, path: &str, limits: &Limits) -> R<Vec<u8>> {
        let full = self.resolve(path)?;
        let len = std::fs::metadata(&full).map_err(|e| io(path, e))?.len();
        if len > limits.fs_read_bytes as u64 {
            return Err(format!("fs.readFile(`{path}`): {len} bytes, over the {}-byte limit", limits.fs_read_bytes));
        }
        std::fs::read(&full).map_err(|e| io(path, e))
    }

    pub fn write(&self, path: &str, data: &[u8], limits: &Limits) -> R<()> {
        if data.len() > limits.fs_write_bytes {
            return Err(format!(
                "fs.writeFile(`{path}`): {} bytes, over the {}-byte limit",
                data.len(),
                limits.fs_write_bytes
            ));
        }
        let full = self.resolve(path)?;
        std::fs::write(&full, data).map_err(|e| io(path, e))
    }

    pub fn readdir(&self, path: &str, limits: &Limits) -> R<Vec<String>> {
        let full = self.resolve(path)?;
        let mut names = Vec::new();
        for entry in std::fs::read_dir(&full).map_err(|e| io(path, e))? {
            let entry = entry.map_err(|e| io(path, e))?;
            if names.len() >= limits.fs_readdir_entries {
                return Err(format!("fs.readdir(`{path}`): more than {} entries", limits.fs_readdir_entries));
            }
            names.push(entry.file_name().to_string_lossy().into_owned());
        }
        names.sort();
        Ok(names)
    }

    pub fn exists(&self, path: &str) -> R<bool> {
        Ok(std::fs::symlink_metadata(self.resolve(path)?).is_ok())
    }

    pub fn unlink(&self, path: &str) -> R<()> {
        std::fs::remove_file(self.resolve(path)?).map_err(|e| io(path, e))
    }

    pub fn mkdir(&self, path: &str, recursive: bool) -> R<()> {
        let full = self.resolve(path)?;
        if recursive {
            std::fs::create_dir_all(full)
        } else {
            std::fs::create_dir(full)
        }
        .map_err(|e| io(path, e))
    }

    pub fn rmdir(&self, path: &str) -> R<()> {
        let full = self.resolve(path)?;
        if full == self.root()? {
            return Err("fs.rmdir: cannot remove the data directory itself".into());
        }
        std::fs::remove_dir(full).map_err(|e| io(path, e))
    }

    pub fn stat(&self, path: &str) -> R<Stat> {
        let meta = std::fs::metadata(self.resolve(path)?).map_err(|e| io(path, e))?;
        let mtime_ms = meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map_or(0.0, |d| d.as_secs_f64() * 1000.0);
        Ok(Stat { size: meta.len(), is_file: meta.is_file(), is_dir: meta.is_dir(), mtime_ms })
    }

    pub fn rename(&self, from: &str, to: &str) -> R<()> {
        std::fs::rename(self.resolve(from)?, self.resolve(to)?).map_err(|e| io(from, e))
    }
}

fn io(path: &str, e: std::io::Error) -> String {
    let code = match e.kind() {
        std::io::ErrorKind::NotFound => "ENOENT",
        std::io::ErrorKind::AlreadyExists => "EEXIST",
        std::io::ErrorKind::PermissionDenied => "EACCES",
        _ => "EIO",
    };
    format!("{code}: `{path}`: {e}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escapes_are_refused() {
        let dir = tempfile::tempdir().unwrap();
        let fs = Fs::new(Some(dir.path()));
        let limits = Limits::default();
        fs.write("a.txt", b"hi", &limits).unwrap();
        assert_eq!(fs.read("./a.txt", &limits).unwrap(), b"hi");
        for bad in ["../x", "a/../../x", "/etc/passwd", "\\x", "a/../a.txt"] {
            assert!(fs.resolve(bad).is_err(), "{bad}");
        }
        std::fs::write(dir.path().join("secret"), b"s").unwrap();
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(dir.path(), dir.path().join("files/out")).unwrap();
            assert!(fs.read("out/secret", &limits).unwrap_err().contains("leaves"));
            assert!(fs.write("out/new", b"x", &limits).unwrap_err().contains("leaves"));
            std::os::unix::fs::symlink(dir.path().join("secret"), dir.path().join("files/link")).unwrap();
            assert!(fs.read("link", &limits).unwrap_err().contains("leaves"));
        }
        fs.mkdir("d/e", true).unwrap();
        fs.write("d/e/f", b"123", &limits).unwrap();
        assert_eq!(fs.readdir("d", &limits).unwrap(), vec!["e"]);
        assert_eq!(fs.stat("d/e/f").unwrap().size, 3);
        fs.rename("d/e/f", "g").unwrap();
        assert!(fs.exists("g").unwrap() && !fs.exists("d/e/f").unwrap());
        assert!(fs.read("missing", &limits).unwrap_err().starts_with("ENOENT"));
        assert!(Fs::new(None).read("a", &limits).unwrap_err().contains("no data directory"));
    }
}
