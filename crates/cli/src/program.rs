//! A program directory on disk: `native-shell.json` plus its sources.
//!
//! `check`, `bundle` and `dev` all read a program the same way, so what a
//! developer checks is exactly what ships and what the device hot-reloads.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use base64::Engine as _;
use native_shell_core::host::HostRegistry;
use native_shell_core::manifest::{Manifest, MANIFEST_FILE};
use sha2::{Digest, Sha256};

/// Files a program directory holds for editors and agents, never shipped.
const TOOLING: &[&str] = &[MANIFEST_FILE, "jsconfig.json", "tsconfig.json", "package.json", "package-lock.json"];

pub struct Program {
    pub manifest: Manifest,
    /// The manifest exactly as written, for bundles.
    pub manifest_json: String,
    /// Every program file (sources and assets) keyed by `/`-separated relative path.
    pub files: BTreeMap<String, Vec<u8>>,
}

impl Program {
    pub fn read(dir: &Path) -> Result<Program> {
        let manifest_path = dir.join(MANIFEST_FILE);
        let manifest_json = std::fs::read_to_string(&manifest_path)
            .with_context(|| format!("cannot read {}", manifest_path.display()))?;
        let manifest = Manifest::parse(&manifest_json).map_err(|e| anyhow::anyhow!("{}: {e}", dir.display()))?;
        let mut files = BTreeMap::new();
        for (rel, path) in list_files(dir)? {
            if TOOLING.contains(&rel.as_str()) || rel.ends_with(".d.ts") || rel.ends_with(".md") {
                continue;
            }
            let bytes = std::fs::read(&path).with_context(|| format!("cannot read {}", path.display()))?;
            files.insert(rel, bytes);
        }
        if !files.contains_key(&manifest.entry) {
            bail!("{}: entry `{}` not found", dir.display(), manifest.entry);
        }
        Ok(Program { manifest, manifest_json, files })
    }

    /// The files as the engine loads plain sources: text as is, and anything
    /// that is not UTF-8 (images, fonts) as a `data:<mime>;base64,…` URL,
    /// which the engine turns back into bytes.
    pub fn sources(&self) -> BTreeMap<String, String> {
        self.files
            .iter()
            .map(|(path, bytes)| {
                let text = match std::str::from_utf8(bytes) {
                    Ok(text) => text.to_string(),
                    Err(_) => format!("data:{};base64,{}", mime(path), base64::engine::general_purpose::STANDARD.encode(bytes)),
                };
                (path.clone(), text)
            })
            .collect()
    }

    /// The `nsh_instance_load_sources` JSON: `{"manifest": {…}, "files": {…}}`.
    pub fn sources_json(&self) -> String {
        let manifest: serde_json::Value = serde_json::from_str(&self.manifest_json).unwrap_or(serde_json::Value::Null);
        serde_json::json!({ "manifest": manifest, "files": self.sources() }).to_string()
    }

    /// Content hash of everything the device loads; changes on every edit.
    pub fn version(&self) -> String {
        let mut h = Sha256::new();
        h.update(self.manifest_json.as_bytes());
        for (k, v) in &self.files {
            h.update((k.len() as u64).to_le_bytes());
            h.update(k.as_bytes());
            h.update((v.len() as u64).to_le_bytes());
            h.update(v);
        }
        h.finalize()[..12].iter().map(|b| format!("{b:02x}")).collect()
    }
}

fn mime(path: &str) -> &'static str {
    match path.rsplit('.').next().unwrap_or("").to_ascii_lowercase().as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "svg" => "image/svg+xml",
        "ttf" => "font/ttf",
        "otf" => "font/otf",
        "woff2" => "font/woff2",
        "json" => "application/json",
        _ => "application/octet-stream",
    }
}

/// Every regular file under `dir`, skipping hidden entries and `node_modules`,
/// sorted by relative path.
pub fn list_files(dir: &Path) -> Result<Vec<(String, PathBuf)>> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        let entries = std::fs::read_dir(&d).with_context(|| format!("cannot list {}", d.display()))?;
        for entry in entries {
            let entry = entry?;
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.starts_with('.') || name == "node_modules" {
                continue;
            }
            let path = entry.path();
            let ty = entry.file_type()?;
            if ty.is_dir() {
                stack.push(path);
            } else if ty.is_file() {
                let rel = path.strip_prefix(dir).unwrap_or(&path);
                let rel = rel.components().map(|c| c.as_os_str().to_string_lossy()).collect::<Vec<_>>().join("/");
                out.push((rel, path));
            }
        }
    }
    out.sort();
    Ok(out)
}

pub fn read_registry(path: Option<&Path>) -> Result<HostRegistry> {
    match path {
        None => Ok(HostRegistry::default()),
        Some(p) => {
            let text = std::fs::read_to_string(p).with_context(|| format!("cannot read {}", p.display()))?;
            let registry: HostRegistry =
                serde_json::from_str(&text).with_context(|| format!("{}: not a host registry", p.display()))?;
            // `nsh/…` names are built-ins the platform answers; apps cannot register them.
            let names = registry.modules.iter().map(|m| &m.name).chain(registry.components.iter().map(|c| &c.name));
            if let Some(name) = names.into_iter().find(|n| n.starts_with("nsh/")) {
                bail!("{}: `{name}`: names starting with `nsh/` are reserved for built-ins", p.display());
            }
            Ok(registry)
        }
    }
}
