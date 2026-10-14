//! The program's own files as images: `svg(path)` and `image(path)` with a
//! path inside the program.
//!
//! Renderers only see what a batch carries, and a bundle-relative path means
//! nothing to them. So the engine resolves both here: an SVG is read from the
//! program's files and parsed into `Shape` path data (`svg.rs`, cached per
//! path), and any other image is written once to
//! `<data_dir>/.assets/<sha256>.<ext>` (a temp directory when the instance has
//! no data directory) and sent as that absolute path. Content addressing makes
//! the write idempotent across runs and versions, and `.assets` sits beside
//! `files/`, outside the directory `fs/promises` exposes to the program.
//!
//! Files come from the verified bundle, else from the loaded sources. Sources
//! are text; a binary file is passed in them as a `data:…;base64,` URL.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::rc::Rc;

use base64::Engine as _;
use sha2::{Digest, Sha256};

use crate::loader::ProgramRef;
use crate::svg::{self, Icon};

pub(crate) struct Assets {
    program: ProgramRef,
    dir: PathBuf,
    icons: HashMap<String, Rc<Result<Icon, String>>>,
    /// Program path → absolute path of the written copy.
    files: HashMap<String, Result<String, String>>,
    /// Paths whose failure was already reported.
    reported: HashSet<String>,
}

impl Assets {
    pub fn new(program: ProgramRef, data_dir: Option<&Path>) -> Assets {
        let dir = match data_dir {
            Some(d) => d.join(".assets"),
            None => std::env::temp_dir().join("native-shell-assets"),
        };
        Assets { program, dir, icons: HashMap::new(), files: HashMap::new(), reported: HashSet::new() }
    }

    /// The parsed SVG at `path` in the program.
    pub fn icon(&mut self, path: &str) -> Rc<Result<Icon, String>> {
        if let Some(icon) = self.icons.get(path) {
            return icon.clone();
        }
        let parsed = self
            .bytes(path)
            .and_then(|b| String::from_utf8(b).map_err(|_| "not UTF-8 text".to_string()))
            .and_then(|text| svg::parse(&text));
        let icon = Rc::new(parsed);
        self.icons.insert(path.to_string(), icon.clone());
        icon
    }

    /// The absolute path of a copy of the program file at `path`, written on first use.
    pub fn file(&mut self, path: &str) -> Result<String, String> {
        if let Some(f) = self.files.get(path) {
            return f.clone();
        }
        let written = self.bytes(path).and_then(|bytes| self.write(path, &bytes));
        self.files.insert(path.to_string(), written.clone());
        written
    }

    /// True the first time `path` fails, so each bad file is reported once.
    pub fn first_report(&mut self, path: &str) -> bool {
        self.reported.insert(path.to_string())
    }

    fn bytes(&self, path: &str) -> Result<Vec<u8>, String> {
        let program = self.program.borrow();
        if let Some(bytes) = program.bundle.as_ref().and_then(|b| b.files.get(path)) {
            return Ok(bytes.clone());
        }
        let text = program.sources.get(path).ok_or("no such file in the program")?;
        match text.strip_prefix("data:").and_then(|rest| rest.split_once(";base64,")) {
            Some((_, b64)) => base64::engine::general_purpose::STANDARD
                .decode(b64.trim())
                .map_err(|e| format!("bad base64 data URL: {e}")),
            None => Ok(text.as_bytes().to_vec()),
        }
    }

    fn write(&self, path: &str, bytes: &[u8]) -> Result<String, String> {
        let digest: String = Sha256::digest(bytes).iter().map(|b| format!("{b:02x}")).collect();
        let ext = Path::new(path)
            .extension()
            .and_then(|e| e.to_str())
            .filter(|e| e.len() <= 8 && e.bytes().all(|c| c.is_ascii_alphanumeric()))
            .map(|e| e.to_ascii_lowercase())
            .unwrap_or_else(|| "bin".into());
        let target = self.dir.join(format!("{digest}.{ext}"));
        let failed = |e: std::io::Error| format!("cannot write {}: {e}", target.display());
        if !target.is_file() {
            std::fs::create_dir_all(&self.dir).map_err(failed)?;
            // Written aside then renamed, so a renderer never reads half a file.
            let partial = self.dir.join(format!("{digest}.{}.partial", std::process::id()));
            std::fs::write(&partial, bytes).map_err(failed)?;
            std::fs::rename(&partial, &target).map_err(failed)?;
        }
        Ok(target.to_string_lossy().into_owned())
    }
}
