//! `native-shell.json`: what a mini program is and what it may do.
//!
//! Everything is denied unless listed. Unknown fields are rejected, so a typo
//! in a permission is an error at load, never a silently missing grant.

use serde::{Deserialize, Serialize};

pub const MANIFEST_FILE: &str = "native-shell.json";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    /// Reverse-DNS id; also the data directory name.
    pub id: String,
    pub name: String,
    #[serde(default = "default_version")]
    pub version: String,
    #[serde(default = "default_entry")]
    pub entry: String,
    /// Lowest runtime version the program needs, e.g. `"0.1.0"`.
    #[serde(default)]
    pub min_shell_version: Option<String>,
    /// App Store 4.7.5: age rating of the program's content.
    #[serde(default)]
    pub age_rating: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub capabilities: Capabilities,
}

fn default_version() -> String {
    "0.0.0".into()
}
fn default_entry() -> String {
    "main.js".into()
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Capabilities {
    /// `localStorage` (on by default, as in gpui-shell).
    #[serde(default = "yes")]
    pub storage: bool,
    /// `fs/promises` inside the program's data directory.
    #[serde(default)]
    pub fs: bool,
    #[serde(default)]
    pub network: Network,
    #[serde(default)]
    pub clipboard: Clipboard,
    /// `cx.open_url` for http(s) links.
    #[serde(default)]
    pub open_url: bool,
    /// Host modules the program imports (`"demo/quote"`). Only checked for modules that require permission.
    #[serde(default)]
    pub host_modules: Vec<String>,
    /// Host components the program places (`"demo-stock-cell"`).
    #[serde(default)]
    pub host_components: Vec<String>,
}

fn yes() -> bool {
    true
}

impl Default for Capabilities {
    fn default() -> Self {
        Capabilities {
            storage: true,
            fs: false,
            network: Network::default(),
            clipboard: Clipboard::default(),
            open_url: false,
            host_modules: Vec::new(),
            host_components: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Network {
    /// Hosts reachable by `fetch` and `WebSocket` with any method. `*.example.com` matches subdomains.
    #[serde(default)]
    pub hosts: Vec<String>,
    /// Finer HTTP grants.
    #[serde(default)]
    pub http: Vec<HttpGrant>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HttpGrant {
    pub host: String,
    #[serde(default)]
    pub schemes: Vec<String>,
    #[serde(default)]
    pub methods: Vec<String>,
    #[serde(default)]
    pub path_prefix: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Clipboard {
    #[serde(default)]
    pub read: bool,
    #[serde(default)]
    pub write: bool,
}

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum ManifestError {
    #[error("{MANIFEST_FILE}: {0}")]
    Parse(String),
    #[error("{MANIFEST_FILE}: `id` must be reverse-DNS letters, digits, `.`, `-`, `_` (got `{0}`)")]
    Id(String),
    #[error("{MANIFEST_FILE}: `entry` must be a relative path inside the program (got `{0}`)")]
    Entry(String),
}

impl Manifest {
    pub fn parse(json: &str) -> Result<Manifest, ManifestError> {
        let m: Manifest = serde_json::from_str(json).map_err(|e| ManifestError::Parse(e.to_string()))?;
        if m.id.is_empty() || !m.id.chars().all(|c| c.is_ascii_alphanumeric() || ".-_".contains(c)) || m.id.starts_with('.') {
            return Err(ManifestError::Id(m.id));
        }
        if m.entry.starts_with('/') || m.entry.split('/').any(|p| p == "..") || m.entry.is_empty() {
            return Err(ManifestError::Entry(m.entry));
        }
        Ok(m)
    }
}

impl Capabilities {
    /// Everything granted: the headless `check` and tests.
    pub fn all() -> Capabilities {
        Capabilities {
            storage: true,
            fs: true,
            network: Network { hosts: vec!["*".into()], http: Vec::new() },
            clipboard: Clipboard { read: true, write: true },
            open_url: true,
            host_modules: vec!["*".into()],
            host_components: vec!["*".into()],
        }
    }

    pub fn allows_host_module(&self, name: &str) -> bool {
        self.host_modules.iter().any(|m| m == "*" || m == name)
    }

    pub fn allows_host_component(&self, name: &str) -> bool {
        self.host_components.iter().any(|m| m == "*" || m == name)
    }

    /// Whether `fetch(method, url)` is granted. Redirects must be re-checked with this too.
    pub fn allows_http(&self, method: &str, scheme: &str, host: &str, path: &str) -> bool {
        let host = host.to_ascii_lowercase();
        if !matches!(scheme, "http" | "https" | "ws" | "wss") {
            return false;
        }
        if self.network.hosts.iter().any(|h| host_matches(h, &host)) {
            return true;
        }
        self.network.http.iter().any(|g| {
            host_matches(&g.host, &host)
                && (g.schemes.is_empty() || g.schemes.iter().any(|s| s.eq_ignore_ascii_case(scheme)))
                && (g.methods.is_empty() || g.methods.iter().any(|m| m.eq_ignore_ascii_case(method)))
                && g.path_prefix.as_ref().is_none_or(|p| path.starts_with(p.as_str()))
        })
    }
}

fn host_matches(pattern: &str, host: &str) -> bool {
    let pattern = pattern.to_ascii_lowercase();
    if pattern == "*" {
        return true;
    }
    match pattern.strip_prefix("*.") {
        Some(suffix) => host.ends_with(&format!(".{suffix}")),
        None => pattern == host,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn minimal_manifest_defaults_to_deny() {
        let m = Manifest::parse(r#"{"id":"com.example.todo","name":"Todo"}"#).unwrap();
        assert_eq!(m.entry, "main.js");
        assert!(m.capabilities.storage);
        assert!(!m.capabilities.fs);
        assert!(!m.capabilities.allows_http("GET", "https", "example.com", "/"));
        assert!(!m.capabilities.allows_host_module("demo/quote"));
    }

    #[test]
    fn unknown_fields_and_bad_values_are_errors() {
        assert!(Manifest::parse(r#"{"id":"a","name":"b","capabilites":{}}"#).is_err());
        assert!(Manifest::parse(r#"{"id":"a","name":"b","capabilities":{"netwrok":{}}}"#).is_err());
        assert!(matches!(Manifest::parse(r#"{"id":"../x","name":"b"}"#), Err(ManifestError::Id(_))));
        assert!(matches!(Manifest::parse(r#"{"id":"a","name":"b","entry":"../main.js"}"#), Err(ManifestError::Entry(_))));
    }

    #[test]
    fn network_grants() {
        let m = Manifest::parse(
            r#"{"id":"a","name":"b","capabilities":{"network":{"hosts":["*.example.org"],
            "http":[{"host":"api.example.com","methods":["GET"],"path_prefix":"/v1/"}]},
            "host_modules":["demo/quote"]}}"#,
        )
        .unwrap();
        let c = &m.capabilities;
        assert!(c.allows_http("POST", "https", "api.example.org", "/x"));
        assert!(!c.allows_http("GET", "https", "example.org.evil.io", "/"));
        assert!(c.allows_http("GET", "https", "API.example.com", "/v1/quote"));
        assert!(!c.allows_http("POST", "https", "api.example.com", "/v1/quote"));
        assert!(!c.allows_http("GET", "https", "api.example.com", "/v2/"));
        assert!(!c.allows_http("GET", "file", "api.example.com", "/v1/"));
        assert!(c.allows_host_module("demo/quote"));
        assert!(!c.allows_host_module("demo/trade"));
    }
}
