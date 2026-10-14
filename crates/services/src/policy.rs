//! Capability checks and the messages that explain a refusal.
//!
//! Every refusal names the exact manifest change that would grant it, because
//! the person reading it is a program author who has to edit
//! `native-shell.json`, and "permission denied" alone sends them hunting.

use native_shell_core::manifest::{Capabilities, MANIFEST_FILE};
use rquickjs::{Ctx, Exception};
use url::Url;

/// What a refused operation would need.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Grant<'a> {
    /// A boolean capability, e.g. `"fs"` → `capabilities.fs`.
    Flag(&'a str),
    /// A host for `capabilities.network.hosts`.
    Host(&'a str),
}

/// `"fetch https://x.com/: not granted — add \"x.com\" to capabilities.network.hosts in native-shell.json"`.
pub fn denial(what: &str, grant: Grant<'_>) -> String {
    match grant {
        Grant::Flag(key) => {
            format!("{what}: not granted — set \"{key}\": true in capabilities in {MANIFEST_FILE}")
        }
        Grant::Host(host) => {
            format!("{what}: not granted — add \"{host}\" to capabilities.network.hosts in {MANIFEST_FILE}")
        }
    }
}

/// Throws a `TypeError` carrying [`denial`].
pub fn deny(ctx: &Ctx<'_>, what: &str, grant: Grant<'_>) -> rquickjs::Error {
    Exception::throw_type(ctx, &denial(what, grant))
}

/// Checks a network request (HTTP or WebSocket) against the grants; the error
/// is the denial message. Called for the first request and every redirect hop.
pub fn check_network(caps: &Capabilities, what: &str, method: &str, url: &Url) -> Result<(), String> {
    let host = url.host_str().unwrap_or_default();
    // `allows_http` matches by host name only, so a granted host is reachable on
    // any port; ports are part of the grant only through `http` path rules.
    if caps.allows_http(method, url.scheme(), host, url.path()) {
        Ok(())
    } else {
        Err(denial(&format!("{what} {url}"), Grant::Host(host)))
    }
}

/// Whether two URLs share an origin (scheme, host, effective port).
pub fn same_origin(a: &Url, b: &Url) -> bool {
    a.scheme() == b.scheme()
        && a.host_str().map(str::to_ascii_lowercase) == b.host_str().map(str::to_ascii_lowercase)
        && a.port_or_known_default() == b.port_or_known_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn messages_name_the_grant() {
        let caps = Capabilities::default();
        let url = Url::parse("https://x.com/").unwrap();
        assert_eq!(
            check_network(&caps, "fetch", "GET", &url).unwrap_err(),
            "fetch https://x.com/: not granted — add \"x.com\" to capabilities.network.hosts in native-shell.json"
        );
        assert_eq!(
            denial("fs/promises", Grant::Flag("fs")),
            "fs/promises: not granted — set \"fs\": true in capabilities in native-shell.json"
        );
    }

    #[test]
    fn origins() {
        let a = Url::parse("https://a.com/x").unwrap();
        assert!(same_origin(&a, &Url::parse("https://A.com:443/y").unwrap()));
        assert!(!same_origin(&a, &Url::parse("http://a.com/").unwrap()));
        assert!(!same_origin(&a, &Url::parse("https://a.com:8443/").unwrap()));
    }
}
