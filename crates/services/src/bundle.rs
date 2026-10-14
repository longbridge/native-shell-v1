//! Signed program bundles (`.nspkg`) and QuickJS bytecode.
//!
//! # Byte layout
//!
//! All integers are little-endian.
//!
//! ```text
//! offset        size   field
//! 0             4      magic "NSPK"
//! 4             2      format version (1)
//! 6             2      flags (0; reserved)
//! 8             4      H = header length in bytes
//! 12            H      header: UTF-8 JSON, see below
//! 12+H          B      blobs: every file's bytes, concatenated in file-table order
//! 12+H+B        64     Ed25519 signature over SHA-256(bytes[0 .. 12+H+B])
//! ```
//!
//! The header:
//!
//! ```json
//! {
//!   "manifest": { …native-shell.json… },
//!   "abi_hash": "3f1c…",             // native_shell_core::protocol::abi_hash() at build time
//!   "min_runtime_version": "0.1.0",  // the manifest's min_shell_version, or "0.0.0"
//!   "created_at": 1760400000,        // unix seconds
//!   "key_id": "release-2026",
//!   "files": [
//!     { "path": "main.js", "kind": "source",   "offset": 0,   "size": 812, "sha256": "…" },
//!     { "path": "main.js", "kind": "bytecode", "offset": 812, "size": 1630, "sha256": "…" },
//!     { "path": "logo.png", "kind": "asset",   "offset": 2442, "size": 4096, "sha256": "…" }
//!   ]
//! }
//! ```
//!
//! The signature covers everything before it, magic and version included, so
//! no byte of a bundle can change without invalidating it. A bundle's identity
//! (for revocation lists) is the SHA-256 of the whole file, signature included;
//! Ed25519 signatures are deterministic, so rebuilding the same inputs with the
//! same key reproduces it.
//!
//! # Keys
//!
//! A runtime trusts a set of [`TrustedKey`]s; the header names the one that
//! signed. Shipping the next key alongside the current one before switching
//! signers is how keys rotate, and dropping a key from the set retires it.
//!
//! # Bytecode
//!
//! A bundle always carries every module's source; bytecode is an optional
//! extra, compiled by [`compile_module`]. Bytecode is only valid for the exact
//! engine it was compiled with, which the ABI hash identifies: when the hash
//! does not match the runtime's, [`open`] drops the bytecode and the program
//! runs from source.
//!
//! **Invariant:** QuickJS bytecode is trusted input. Loading crafted bytecode
//! is memory corruption (CVE-2025-46687 and its kin), so bytecode is only ever
//! loaded from a [`VerifiedBundle`], which only [`open`] can construct, after
//! the signature, the per-file hashes and the ABI hash have all checked out.
//! [`VerifiedBundle::load_module`] is the one place that calls `Module::load`.

use std::collections::BTreeMap;

use base64::Engine as _;
use ed25519_dalek::{Signature, Signer as _, SigningKey, Verifier as _, VerifyingKey};
use native_shell_core::manifest::Manifest;
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};

pub const MAGIC: &[u8; 4] = b"NSPK";
pub const FORMAT_VERSION: u16 = 1;
const PREFIX: usize = 12;
const SIGNATURE: usize = 64;
const MAX_HEADER: usize = 16 << 20;

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum BundleError {
    #[error("not a native-shell bundle: {0}")]
    Format(String),
    #[error("bundle is signed with key `{0}`, which this runtime does not trust")]
    UnknownKey(String),
    #[error("bundle signature does not verify")]
    BadSignature,
    #[error("bundle {0} has been revoked")]
    Revoked(String),
    #[error("bundle needs runtime {needs}, this is {runtime}")]
    TooOld { needs: String, runtime: String },
    #[error("bundle file `{0}` does not match its hash")]
    Integrity(String),
    #[error("bundle manifest: {0}")]
    Manifest(String),
    #[error("compiling `{0}`: {1}")]
    Compile(String, String),
    #[error("key: {0}")]
    Key(String),
}

type Result<T> = std::result::Result<T, BundleError>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FileKind {
    Source,
    Bytecode,
    Asset,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct FileEntry {
    path: String,
    kind: FileKind,
    offset: u64,
    size: u64,
    sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Header {
    manifest: Manifest,
    abi_hash: String,
    min_runtime_version: String,
    created_at: u64,
    key_id: String,
    files: Vec<FileEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrustedKey {
    pub id: String,
    pub public_key: [u8; 32],
}

impl TrustedKey {
    pub fn from_base64(id: impl Into<String>, public_b64: &str) -> Result<TrustedKey> {
        Ok(TrustedKey { id: id.into(), public_key: decode_key(public_b64)? })
    }
}

fn decode_key(b64: &str) -> Result<[u8; 32]> {
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(b64.trim())
        .map_err(|e| BundleError::Key(format!("not base64: {e}")))?;
    bytes.try_into().map_err(|_| BundleError::Key("an Ed25519 key is 32 bytes".into()))
}

/// A fresh Ed25519 key pair as `(secret_b64, public_b64)`.
pub fn generate_keypair() -> (String, String) {
    let mut secret = [0u8; 32];
    getrandom::getrandom(&mut secret).unwrap_or_else(|e| panic!("getrandom failed: {e}"));
    let key = SigningKey::from_bytes(&secret);
    let b64 = base64::engine::general_purpose::STANDARD;
    (b64.encode(secret), b64.encode(key.verifying_key().to_bytes()))
}

pub fn secret_key_from_base64(secret_b64: &str) -> Result<[u8; 32]> {
    decode_key(secret_b64)
}

pub fn public_key_of(secret: &[u8; 32]) -> [u8; 32] {
    SigningKey::from_bytes(secret).verifying_key().to_bytes()
}

#[derive(Debug, Clone)]
pub struct BuildOptions {
    /// Compile every `.js`/`.mjs` module to bytecode alongside its source.
    pub bytecode: bool,
    /// Unix seconds; defaults to now.
    pub created_at: Option<u64>,
}

impl Default for BuildOptions {
    fn default() -> BuildOptions {
        BuildOptions { bytecode: true, created_at: None }
    }
}

fn is_source(path: &str) -> bool {
    path.ends_with(".js") || path.ends_with(".mjs")
}

fn check_path(path: &str) -> Result<()> {
    let ok = !path.is_empty()
        && !path.starts_with('/')
        && !path.contains(['\\', '\0'])
        && path.split('/').all(|p| !p.is_empty() && p != "." && p != "..");
    if ok {
        Ok(())
    } else {
        Err(BundleError::Format(format!("invalid file path `{path}`")))
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Builds a signed `.nspkg` from a program's files (path → bytes, paths relative
/// to the program root). Sources are always included; with
/// `opts.bytecode` each module is also compiled.
pub fn build(
    files: &BTreeMap<String, Vec<u8>>,
    manifest: &Manifest,
    signing_key: &[u8; 32],
    key_id: &str,
    opts: &BuildOptions,
) -> Result<Vec<u8>> {
    build_for_abi(files, manifest, signing_key, key_id, opts, &native_shell_core::protocol::abi_hash())
}

fn build_for_abi(
    files: &BTreeMap<String, Vec<u8>>,
    manifest: &Manifest,
    signing_key: &[u8; 32],
    key_id: &str,
    opts: &BuildOptions,
    abi_hash: &str,
) -> Result<Vec<u8>> {
    if !files.contains_key(&manifest.entry) {
        return Err(BundleError::Manifest(format!("entry `{}` is not among the files", manifest.entry)));
    }
    let mut blobs: Vec<(String, FileKind, Vec<u8>)> = Vec::new();
    for (path, data) in files {
        check_path(path)?;
        if is_source(path) {
            blobs.push((path.clone(), FileKind::Source, data.clone()));
            if opts.bytecode {
                let source = std::str::from_utf8(data)
                    .map_err(|_| BundleError::Compile(path.clone(), "source is not UTF-8".into()))?;
                blobs.push((path.clone(), FileKind::Bytecode, compile_module(path, source)?));
            }
        } else {
            blobs.push((path.clone(), FileKind::Asset, data.clone()));
        }
    }
    let mut entries = Vec::with_capacity(blobs.len());
    let mut offset = 0u64;
    for (path, kind, data) in &blobs {
        entries.push(FileEntry {
            path: path.clone(),
            kind: *kind,
            offset,
            size: data.len() as u64,
            sha256: hex(&Sha256::digest(data)),
        });
        offset += data.len() as u64;
    }
    let created_at = opts.created_at.unwrap_or_else(|| {
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs())
    });
    let header = Header {
        manifest: manifest.clone(),
        abi_hash: abi_hash.to_string(),
        min_runtime_version: manifest.min_shell_version.clone().unwrap_or_else(|| "0.0.0".into()),
        created_at,
        key_id: key_id.to_string(),
        files: entries,
    };
    let header = serde_json::to_vec(&header).map_err(|e| BundleError::Format(e.to_string()))?;

    let mut out = Vec::with_capacity(PREFIX + header.len() + offset as usize + SIGNATURE);
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&FORMAT_VERSION.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(&(header.len() as u32).to_le_bytes());
    out.extend_from_slice(&header);
    for (_, _, data) in &blobs {
        out.extend_from_slice(data);
    }
    let signature = SigningKey::from_bytes(signing_key).sign(&Sha256::digest(&out));
    out.extend_from_slice(&signature.to_bytes());
    Ok(out)
}

/// A bundle whose signature, hashes and versions have been checked. Only
/// [`open`] creates one (`non_exhaustive` forbids struct literals elsewhere).
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct VerifiedBundle {
    pub manifest: Manifest,
    /// Sources and assets.
    pub files: BTreeMap<String, Vec<u8>>,
    /// Module path → bytecode; empty when the bundle's ABI hash is not this runtime's.
    pub bytecode: BTreeMap<String, Vec<u8>>,
    pub abi_hash: String,
    /// SHA-256 of the whole bundle, the identity revocation lists use.
    pub sha256: String,
    pub key_id: String,
    pub created_at: u64,
}

impl VerifiedBundle {
    /// Declares the module at `path` from the bundle's bytecode, or `None`
    /// when there is none for it (the caller compiles the source instead).
    pub fn load_module<'js>(
        &self,
        ctx: &rquickjs::Ctx<'js>,
        path: &str,
    ) -> rquickjs::Result<Option<rquickjs::Module<'js, rquickjs::module::Declared>>> {
        let Some(code) = self.bytecode.get(path) else { return Ok(None) };
        // SAFETY: `self` was produced by `open`, which verified the signature,
        // the bytecode's hash and that it was compiled by this engine.
        unsafe { rquickjs::Module::load(ctx.clone(), code) }.map(Some)
    }
}

/// Verifies and unpacks a `.nspkg`. `revoked` holds bundle SHA-256s (hex);
/// `runtime_version` is this runtime's semver.
pub fn open(bytes: &[u8], trusted: &[TrustedKey], revoked: &[String], runtime_version: &str) -> Result<VerifiedBundle> {
    if bytes.len() < PREFIX + SIGNATURE || &bytes[..4] != MAGIC {
        return Err(BundleError::Format("bad magic".into()));
    }
    let version = u16::from_le_bytes([bytes[4], bytes[5]]);
    if version != FORMAT_VERSION {
        return Err(BundleError::Format(format!("format version {version}, this runtime reads {FORMAT_VERSION}")));
    }
    let header_len = u32::from_le_bytes([bytes[8], bytes[9], bytes[10], bytes[11]]) as usize;
    let signed_len = bytes.len() - SIGNATURE;
    if header_len > MAX_HEADER || PREFIX + header_len > signed_len {
        return Err(BundleError::Format("header length out of range".into()));
    }
    let header: Header = serde_json::from_slice(&bytes[PREFIX..PREFIX + header_len])
        .map_err(|e| BundleError::Format(format!("header: {e}")))?;

    // Nothing in the header is believed until the signature checks out.
    let key = trusted
        .iter()
        .find(|k| k.id == header.key_id)
        .ok_or_else(|| BundleError::UnknownKey(header.key_id.clone()))?;
    let verifying = VerifyingKey::from_bytes(&key.public_key).map_err(|e| BundleError::Key(e.to_string()))?;
    let signature = Signature::from_slice(&bytes[signed_len..]).map_err(|_| BundleError::BadSignature)?;
    verifying
        .verify(&Sha256::digest(&bytes[..signed_len]), &signature)
        .map_err(|_| BundleError::BadSignature)?;

    let sha256 = hex(&Sha256::digest(bytes));
    if revoked.iter().any(|r| r.trim().eq_ignore_ascii_case(&sha256)) {
        return Err(BundleError::Revoked(sha256));
    }
    let manifest = Manifest::parse(&serde_json::to_string(&header.manifest).map_err(|e| BundleError::Manifest(e.to_string()))?)
        .map_err(|e| BundleError::Manifest(e.to_string()))?;
    for needs in [Some(&header.min_runtime_version), manifest.min_shell_version.as_ref()].into_iter().flatten() {
        if compare_versions(needs, runtime_version).ok_or_else(|| BundleError::Manifest(format!("bad version `{needs}`")))?
            == std::cmp::Ordering::Greater
        {
            return Err(BundleError::TooOld { needs: needs.clone(), runtime: runtime_version.to_string() });
        }
    }

    let blobs = &bytes[PREFIX + header_len..signed_len];
    let abi_matches = header.abi_hash == native_shell_core::protocol::abi_hash();
    let mut files = BTreeMap::new();
    let mut bytecode = BTreeMap::new();
    for entry in &header.files {
        check_path(&entry.path)?;
        let range = usize::try_from(entry.offset)
            .ok()
            .zip(usize::try_from(entry.size).ok())
            .and_then(|(start, size)| Some(start..start.checked_add(size)?))
            .filter(|r| r.end <= blobs.len())
            .ok_or_else(|| BundleError::Format(format!("`{}` lies outside the bundle", entry.path)))?;
        let data = &blobs[range];
        if hex(&Sha256::digest(data)) != entry.sha256 {
            return Err(BundleError::Integrity(entry.path.clone()));
        }
        let target = match entry.kind {
            FileKind::Bytecode if !abi_matches => continue,
            FileKind::Bytecode => &mut bytecode,
            FileKind::Source | FileKind::Asset => &mut files,
        };
        if target.insert(entry.path.clone(), data.to_vec()).is_some() {
            return Err(BundleError::Format(format!("`{}` appears twice", entry.path)));
        }
    }
    if !files.contains_key(&manifest.entry) {
        return Err(BundleError::Manifest(format!("entry `{}` is not in the bundle", manifest.entry)));
    }
    Ok(VerifiedBundle {
        manifest,
        files,
        bytecode,
        abi_hash: header.abi_hash,
        sha256,
        key_id: header.key_id,
        created_at: header.created_at,
    })
}

/// Compiles an ES module to QuickJS bytecode (little-endian, which every
/// supported device is) in a throwaway runtime. The bytecode records import
/// specifiers, not the modules they name, so imports are stubbed here and
/// resolved for real when the engine evaluates the module.
pub fn compile_module(name: &str, source: &str) -> Result<Vec<u8>> {
    let fail = |e: &dyn std::fmt::Display| BundleError::Compile(name.to_string(), e.to_string());
    let rt = rquickjs::Runtime::new().map_err(|e| fail(&e))?;
    rt.set_loader(StubImports, StubImports);
    let ctx = rquickjs::Context::full(&rt).map_err(|e| fail(&e))?;
    ctx.with(|ctx| {
        let options = rquickjs::module::WriteOptions {
            endianness: rquickjs::module::WriteOptionsEndianness::Little,
            ..Default::default()
        };
        rquickjs::Module::declare(ctx.clone(), name, source)
            .and_then(|m| m.write(options))
            .map_err(|e| match e {
                rquickjs::Error::Exception => {
                    let thrown = ctx.catch();
                    let message = thrown
                        .as_exception()
                        .and_then(|x| x.message())
                        .or_else(|| thrown.as_string().and_then(|s| s.to_string().ok()))
                        .unwrap_or_else(|| "compilation failed".into());
                    fail(&message)
                }
                e => fail(&e),
            })
    })
}

struct StubImports;

impl rquickjs::loader::Resolver for StubImports {
    fn resolve<'js>(
        &mut self,
        _ctx: &rquickjs::Ctx<'js>,
        _base: &str,
        name: &str,
        _attributes: Option<rquickjs::loader::ImportAttributes<'js>>,
    ) -> rquickjs::Result<String> {
        Ok(name.to_string())
    }
}

impl rquickjs::loader::Loader for StubImports {
    fn load<'js>(
        &mut self,
        ctx: &rquickjs::Ctx<'js>,
        name: &str,
        _attributes: Option<rquickjs::loader::ImportAttributes<'js>>,
    ) -> rquickjs::Result<rquickjs::Module<'js, rquickjs::module::Declared>> {
        rquickjs::Module::declare(ctx.clone(), name, "")
    }
}

/// Compares `major.minor.patch[-pre][+build]`; a pre-release sorts before its release.
pub fn compare_versions(a: &str, b: &str) -> Option<std::cmp::Ordering> {
    fn parse(v: &str) -> Option<([u64; 3], Option<&str>)> {
        let v = v.trim().trim_start_matches('v');
        let v = v.split('+').next()?;
        let (core, pre) = match v.split_once('-') {
            Some((c, p)) => (c, Some(p)),
            None => (v, None),
        };
        let mut parts = core.split('.');
        let mut out = [0u64; 3];
        for slot in &mut out {
            *slot = match parts.next() {
                Some(p) => p.parse().ok()?,
                None => 0,
            };
        }
        if parts.next().is_some() {
            return None;
        }
        Some((out, pre))
    }
    let (a, ap) = parse(a)?;
    let (b, bp) = parse(b)?;
    Some(a.cmp(&b).then(match (ap, bp) {
        (None, None) => std::cmp::Ordering::Equal,
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (Some(x), Some(y)) => x.cmp(y),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    const RUNTIME: &str = "0.1.0";

    fn program() -> (BTreeMap<String, Vec<u8>>, Manifest) {
        let manifest = Manifest::parse(r#"{"id":"com.example.app","name":"App","min_shell_version":"0.1.0"}"#).unwrap();
        let files = BTreeMap::from([
            ("main.js".to_string(), b"export const answer = 40 + 2; globalThis.ran = answer;".to_vec()),
            ("img/logo.png".to_string(), vec![0x89, b'P', b'N', b'G']),
        ]);
        (files, manifest)
    }

    fn keys() -> ([u8; 32], TrustedKey) {
        let (secret, public) = generate_keypair();
        (secret_key_from_base64(&secret).unwrap(), TrustedKey::from_base64("k1", &public).unwrap())
    }

    #[test]
    fn happy_path_and_bytecode_runs_in_a_fresh_runtime() {
        let (files, manifest) = program();
        let (secret, key) = keys();
        let msp = build(&files, &manifest, &secret, "k1", &BuildOptions::default()).unwrap();
        let b = open(&msp, &[key], &[], RUNTIME).unwrap();
        assert_eq!(b.manifest, manifest);
        assert_eq!(b.files, files);
        assert_eq!(b.bytecode.keys().collect::<Vec<_>>(), ["main.js"]);
        assert_eq!(b.abi_hash, native_shell_core::protocol::abi_hash());
        assert_eq!(b.sha256, hex(&Sha256::digest(&msp)));

        let rt = rquickjs::Runtime::new().unwrap();
        let ctx = rquickjs::Context::full(&rt).unwrap();
        ctx.with(|ctx| {
            let module = b.load_module(&ctx, "main.js").unwrap().unwrap();
            let (module, promise) = module.eval().unwrap();
            promise.finish::<()>().unwrap();
            assert_eq!(module.get::<_, i32>("answer").unwrap(), 42);
            assert_eq!(ctx.globals().get::<_, i32>("ran").unwrap(), 42);
            assert!(b.load_module(&ctx, "other.js").unwrap().is_none());
        });
    }

    #[test]
    fn every_byte_is_covered() {
        let (files, manifest) = program();
        let (secret, key) = keys();
        let msp = build(&files, &manifest, &secret, "k1", &BuildOptions::default()).unwrap();
        // A byte in the header, in a blob, in the signature.
        for at in [20, msp.len() - 70, msp.len() - 1] {
            let mut bad = msp.clone();
            bad[at] ^= 1;
            assert!(open(&bad, std::slice::from_ref(&key), &[], RUNTIME).is_err(), "byte {at}");
        }
        let mut bad = msp.clone();
        bad[0] = b'X';
        assert!(matches!(open(&bad, std::slice::from_ref(&key), &[], RUNTIME), Err(BundleError::Format(_))));
        let mut bad = msp.clone();
        bad[4] = 9;
        assert!(matches!(open(&bad, &[key], &[], RUNTIME), Err(BundleError::Format(_))));
    }

    #[test]
    fn keys_rotation_and_revocation() {
        let (files, manifest) = program();
        let (secret, key) = keys();
        let (_, other) = keys();
        let msp = build(&files, &manifest, &secret, "k1", &BuildOptions { bytecode: false, created_at: Some(1) }).unwrap();
        // Same id, different key: the signature fails.
        assert_eq!(open(&msp, std::slice::from_ref(&other), &[], RUNTIME).unwrap_err(), BundleError::BadSignature);
        let unknown = TrustedKey { id: "k0".into(), ..other.clone() };
        assert_eq!(open(&msp, std::slice::from_ref(&unknown), &[], RUNTIME).unwrap_err(), BundleError::UnknownKey("k1".into()));
        // Rotation: both the old and the new key are trusted.
        let (secret2, key2) = keys();
        let key2 = TrustedKey { id: "k2".into(), ..key2 };
        let msp2 = build(&files, &manifest, &secret2, "k2", &BuildOptions::default()).unwrap();
        let ring = [key.clone(), key2.clone()];
        assert!(open(&msp, &ring, &[], RUNTIME).is_ok());
        assert!(open(&msp2, &ring, &[], RUNTIME).is_ok());
        assert!(open(&msp, &[key2], &[], RUNTIME).is_err());

        let id = open(&msp, &ring, &[], RUNTIME).unwrap().sha256;
        assert_eq!(open(&msp, &ring, &[id.to_uppercase()], RUNTIME).unwrap_err(), BundleError::Revoked(id));
        assert!(open(&msp2, &ring, &[hex(&Sha256::digest(&msp))], RUNTIME).is_ok());
    }

    #[test]
    fn minimum_version() {
        let (files, mut manifest) = program();
        manifest.min_shell_version = Some("0.2.0".into());
        let (secret, key) = keys();
        let msp = build(&files, &manifest, &secret, "k1", &BuildOptions::default()).unwrap();
        assert!(matches!(open(&msp, std::slice::from_ref(&key), &[], "0.1.9"), Err(BundleError::TooOld { .. })));
        assert!(matches!(open(&msp, std::slice::from_ref(&key), &[], "0.2.0-beta.1"), Err(BundleError::TooOld { .. })));
        assert!(open(&msp, std::slice::from_ref(&key), &[], "0.2.0").is_ok());
        assert!(open(&msp, &[key], &[], "1.0.0").is_ok());
    }

    #[test]
    fn abi_mismatch_drops_bytecode_but_keeps_sources() {
        let (files, manifest) = program();
        let (secret, key) = keys();
        let msp = build_for_abi(&files, &manifest, &secret, "k1", &BuildOptions::default(), "0000000000000000").unwrap();
        let b = open(&msp, &[key], &[], RUNTIME).unwrap();
        assert!(b.bytecode.is_empty());
        assert_eq!(b.files, files);
        assert_eq!(b.abi_hash, "0000000000000000");
    }

    #[test]
    fn build_rejects_bad_input() {
        let (mut files, manifest) = program();
        let (secret, _) = keys();
        files.insert("bad.js".into(), b"export const = ;".to_vec());
        assert!(matches!(build(&files, &manifest, &secret, "k", &BuildOptions::default()), Err(BundleError::Compile(p, _)) if p == "bad.js"));
        files.remove("bad.js");
        files.insert("../x".into(), vec![]);
        assert!(matches!(build(&files, &manifest, &secret, "k", &BuildOptions::default()), Err(BundleError::Format(_))));
        files.remove("../x");
        files.remove("main.js");
        assert!(matches!(build(&files, &manifest, &secret, "k", &BuildOptions::default()), Err(BundleError::Manifest(_))));
    }

    #[test]
    fn imports_are_resolved_at_evaluation_not_compilation() {
        let code = compile_module("a.js", "import { x } from 'dep'; globalThis.got = x * 2;").unwrap();

        struct Dep;
        impl rquickjs::loader::Loader for Dep {
            fn load<'js>(
                &mut self,
                ctx: &rquickjs::Ctx<'js>,
                name: &str,
                _: Option<rquickjs::loader::ImportAttributes<'js>>,
            ) -> rquickjs::Result<rquickjs::Module<'js, rquickjs::module::Declared>> {
                rquickjs::Module::declare(ctx.clone(), name, "export const x = 21;")
            }
        }
        let rt = rquickjs::Runtime::new().unwrap();
        rt.set_loader(StubImports, Dep);
        let ctx = rquickjs::Context::full(&rt).unwrap();
        ctx.with(|ctx| {
            // SAFETY: produced by `compile_module` in this test, not from outside.
            let module = unsafe { rquickjs::Module::load(ctx.clone(), &code) }.unwrap();
            module.eval().unwrap().1.finish::<()>().unwrap();
            assert_eq!(ctx.globals().get::<_, i32>("got").unwrap(), 42);
        });
    }

    #[test]
    fn versions() {
        use std::cmp::Ordering::*;
        assert_eq!(compare_versions("0.1.0", "0.1"), Some(Equal));
        assert_eq!(compare_versions("1.0.0-rc.1", "1.0.0"), Some(Less));
        assert_eq!(compare_versions("1.10.0", "1.9.9"), Some(Greater));
        assert_eq!(compare_versions("x", "1"), None);
    }
}
