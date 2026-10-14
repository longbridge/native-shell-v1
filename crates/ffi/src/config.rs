//! The `config_json` of `nsh_instance_create` (documented in `native_shell.h`).
//!
//! Unknown keys are errors, like in the manifest: a misspelt `trusted_keys`
//! must fail loudly rather than silently trust nothing.

use std::path::PathBuf;
use std::time::Duration;

use native_shell::{InstanceConfig, Limits, ThemeConfig};
use native_shell_core::host::{Environment, HostRegistry};
use native_shell_core::manifest::Capabilities;
use native_shell_services::bundle::TrustedKey;
use serde::Deserialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Threading {
    Dedicated,
    Caller,
}

pub struct Config {
    pub threading: Threading,
    pub instance: InstanceConfig,
    pub trusted_keys: Vec<TrustedKey>,
    pub revoked: Vec<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawConfig {
    #[serde(default)]
    app_id: String,
    data_dir: Option<PathBuf>,
    #[serde(default)]
    threading: u32,
    #[serde(default)]
    dev: bool,
    #[serde(default)]
    limits: RawLimits,
    #[serde(default)]
    capabilities: Option<Capabilities>,
    #[serde(default)]
    registry: HostRegistry,
    #[serde(default)]
    environment: Option<Environment>,
    /// `{"light": {token: color}, "dark": {…}}`: the app's design tokens mapped onto the shell's.
    #[serde(default)]
    theme: Option<ThemeConfig>,
    #[serde(default)]
    trusted_keys: Vec<RawKey>,
    #[serde(default)]
    revoked: Vec<String>,
}

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct RawLimits {
    memory_mb: Option<u32>,
    stack_kb: Option<u32>,
    render_ms: Option<u32>,
    event_ms: Option<u32>,
    module_ms: Option<u32>,
}

/// Host modules under this prefix (`nsh/clipboard`) are built-ins the platform
/// answers itself; an app may not register its own.
const RESERVED_PREFIX: &str = "nsh/";

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawKey {
    id: String,
    public_key: String,
}

impl Config {
    pub fn parse(json: &[u8]) -> Result<Config, String> {
        let raw: RawConfig = serde_json::from_slice(json).map_err(|e| format!("config JSON: {e}"))?;
        let threading = match raw.threading {
            0 => Threading::Dedicated,
            1 => Threading::Caller,
            n => return Err(format!("config: `threading` must be 0 (dedicated) or 1 (caller), got {n}")),
        };
        if let Some(m) = raw.registry.modules.iter().find(|m| m.name.starts_with(RESERVED_PREFIX)) {
            return Err(format!("config: registry module `{}`: names starting with `{RESERVED_PREFIX}` are reserved for built-ins", m.name));
        }
        if let Some(c) = raw.registry.components.iter().find(|c| c.name.starts_with(RESERVED_PREFIX)) {
            return Err(format!("config: registry component `{}`: names starting with `{RESERVED_PREFIX}` are reserved for built-ins", c.name));
        }
        // Unknown tokens and malformed colors fail at create, not on first render.
        if let Some(theme) = &raw.theme {
            theme.validate().map_err(|e| format!("config: {e}"))?;
        }
        let trusted_keys = raw
            .trusted_keys
            .into_iter()
            .map(|k| {
                TrustedKey::from_base64(k.id.clone(), &k.public_key)
                    .map_err(|e| format!("config: trusted key `{}`: {e}", k.id))
            })
            .collect::<Result<Vec<_>, _>>()?;
        let mut limits = Limits::default();
        let l = raw.limits;
        if let Some(v) = l.memory_mb {
            limits.memory_bytes = (v as usize) << 20;
        }
        if let Some(v) = l.stack_kb {
            limits.stack_bytes = (v as usize) << 10;
        }
        if let Some(v) = l.render_ms {
            limits.render_budget = Duration::from_millis(v.into());
        }
        if let Some(v) = l.event_ms {
            limits.event_budget = Duration::from_millis(v.into());
        }
        if let Some(v) = l.module_ms {
            limits.module_budget = Duration::from_millis(v.into());
        }
        let instance = InstanceConfig {
            app_id: raw.app_id,
            data_dir: raw.data_dir,
            limits,
            capabilities: raw.capabilities,
            registry: raw.registry,
            environment: raw.environment.unwrap_or_default(),
            dev: raw.dev,
            theme: raw.theme,
        };
        Ok(Config { threading, instance, trusted_keys, revoked: raw.revoked })
    }
}
