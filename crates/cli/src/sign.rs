//! `keygen`, `bundle` and `verify`: signed `.nspkg` releases.

use std::path::Path;

use anyhow::{anyhow, bail, Context, Result};
use native_shell_services::bundle::{self, BuildOptions, TrustedKey};

use crate::program::Program;

/// Writes `<prefix>.secret` (owner-only) and `<prefix>.pub`, both base64.
pub fn keygen(prefix: &Path) -> Result<(std::path::PathBuf, std::path::PathBuf)> {
    let (secret, public) = bundle::generate_keypair();
    let with = |ext: &str| {
        let mut p = prefix.as_os_str().to_owned();
        p.push(ext);
        std::path::PathBuf::from(p)
    };
    let (secret_path, public_path) = (with(".secret"), with(".pub"));
    for p in [&secret_path, &public_path] {
        if p.exists() {
            bail!("{} already exists; refusing to overwrite a key", p.display());
        }
    }
    if let Some(parent) = prefix.parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent)?;
    }
    write_private(&secret_path, &format!("{secret}\n"))?;
    std::fs::write(&public_path, format!("{public}\n"))
        .with_context(|| format!("cannot write {}", public_path.display()))?;
    Ok((secret_path, public_path))
}

#[cfg(unix)]
fn write_private(path: &Path, content: &str) -> Result<()> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    let mut f = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)
        .with_context(|| format!("cannot write {}", path.display()))?;
    f.write_all(content.as_bytes())?;
    Ok(())
}

#[cfg(not(unix))]
fn write_private(path: &Path, content: &str) -> Result<()> {
    std::fs::write(path, content).with_context(|| format!("cannot write {}", path.display()))
}

/// A key given as a file path, or inline as base64.
fn read_key(arg: &str) -> Result<String> {
    let p = Path::new(arg);
    if p.is_file() {
        Ok(std::fs::read_to_string(p).with_context(|| format!("cannot read {}", p.display()))?.trim().to_string())
    } else {
        Ok(arg.trim().to_string())
    }
}

pub fn build(dir: &Path, key: &str, key_id: &str, out: &Path, bytecode: bool) -> Result<Vec<u8>> {
    let program = Program::read(dir)?;
    let secret = bundle::secret_key_from_base64(&read_key(key)?).map_err(|e| anyhow!("--key: {e}"))?;
    let files = &program.files;
    let opts = BuildOptions { bytecode, created_at: None };
    let bytes = bundle::build(files, &program.manifest, &secret, key_id, &opts).map_err(|e| anyhow!("{e}"))?;
    std::fs::write(out, &bytes).with_context(|| format!("cannot write {}", out.display()))?;
    Ok(bytes)
}

/// The `key_id` a bundle claims (from its unsigned header; `open` then checks it).
fn claimed_key_id(bytes: &[u8]) -> Result<String> {
    let len = bytes.get(8..12).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as usize);
    let header = len.and_then(|n| bytes.get(12..12usize.checked_add(n)?)).ok_or_else(|| anyhow!("not a bundle"))?;
    let v: serde_json::Value = serde_json::from_slice(header).context("not a bundle header")?;
    v.get("key_id").and_then(|k| k.as_str()).map(str::to_string).ok_or_else(|| anyhow!("bundle header has no key_id"))
}

pub fn verify(file: &Path, trusted: &str) -> Result<String> {
    let bytes = std::fs::read(file).with_context(|| format!("cannot read {}", file.display()))?;
    let key_id = claimed_key_id(&bytes)?;
    let key = TrustedKey::from_base64(key_id, &read_key(trusted)?).map_err(|e| anyhow!("--trusted: {e}"))?;
    let v = bundle::open(&bytes, &[key], &[], env!("CARGO_PKG_VERSION")).map_err(|e| anyhow!("{e}"))?;
    Ok(format!(
        "OK {} {} (key `{}`)\n  sha256 {}\n  abi {}\n  {} files, {} bytecode modules",
        v.manifest.id,
        v.manifest.version,
        v.key_id,
        v.sha256,
        v.abi_hash,
        v.files.len(),
        v.bytecode.len()
    ))
}
