//! The non-network standard library as scripts see it: crypto, encoding, URL,
//! storage and fs.

mod common;

use common::Harness;
use native_shell_core::manifest::Capabilities;
use native_shell_services::Limits;
use serde_json::json;

#[test]
fn crypto() {
    let h = Harness::new(Capabilities::default());
    let r = h
        .run(
            r#"
            const id = crypto.randomUUID();
            const a = crypto.getRandomValues(new Uint8Array(32));
            const b = crypto.getRandomValues(new Uint32Array(4));
            let mismatch; try { crypto.getRandomValues(new Float32Array(2)); } catch (e) { mismatch = e.name; }
            let quota; try { crypto.getRandomValues(new Uint8Array(65537)); } catch (e) { quota = e.name; }
            const hex = (buf) => [...new Uint8Array(buf)].map(x => x.toString(16).padStart(2, "0")).join("");
            const d256 = await crypto.subtle.digest("SHA-256", new TextEncoder().encode("abc"));
            const d512 = await crypto.subtle.digest({ name: "sha-512" }, new Uint8Array(0));
            let unsupported; try { await crypto.subtle.digest("MD5", new Uint8Array(0)); } catch (e) { unsupported = e.message; }
            return {
                uuid: /^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/.test(id),
                distinct: crypto.randomUUID() !== id,
                filled: a.some(x => x !== 0) && b.some(x => x !== 0),
                mismatch, quota,
                d256: hex(d256), d512: hex(d512).slice(0, 16), isBuffer: d256 instanceof ArrayBuffer,
                unsupported: unsupported.includes("NotSupportedError"),
            };
            "#,
        )
        .unwrap();
    assert_eq!(
        r,
        json!({
            "uuid": true, "distinct": true, "filled": true,
            "mismatch": "TypeMismatchError", "quota": "QuotaExceededError",
            "d256": "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
            "d512": "cf83e1357eefb8bd", "isBuffer": true, "unsupported": true,
        })
    );
}

#[test]
fn encoding() {
    let h = Harness::new(Capabilities::default());
    let r = h
        .run(
            r#"
            const enc = new TextEncoder().encode("héllo 世界 😀");
            const dec = new TextDecoder();
            let fatal; try { new TextDecoder("utf-8", { fatal: true }).decode(new Uint8Array([0xff])); } catch (e) { fatal = e.name; }
            let label; try { new TextDecoder("latin1"); } catch (e) { label = e.name; }
            let bad; try { atob("a"); } catch (e) { bad = e.name; }
            let wide; try { btoa("世"); } catch (e) { wide = e.name; }
            return {
                len: enc.length,
                round: dec.decode(enc),
                bom: dec.decode(new Uint8Array([0xef, 0xbb, 0xbf, 0x41])),
                invalid: dec.decode(new Uint8Array([0x41, 0xff, 0xc3, 0x42])),
                buffer: dec.decode(new TextEncoder().encode("ab").buffer),
                fatal, label,
                btoa: btoa("hello\xff"), atob: atob(" aGVs bG8 "), bad, wide,
            };
            "#,
        )
        .unwrap();
    assert_eq!(
        r,
        json!({
            "len": 18, "round": "héllo 世界 😀", "bom": "A", "invalid": "A\u{fffd}\u{fffd}B", "buffer": "ab",
            "fatal": "TypeError", "label": "RangeError",
            "btoa": "aGVsbG//", "atob": "hello", "bad": "InvalidCharacterError", "wide": "InvalidCharacterError",
        })
    );
}

#[test]
fn url() {
    let h = Harness::new(Capabilities::default());
    let r = h
        .run(
            r#"
            const u = new URL("https://user:pw@Example.com:8443/a/b?x=1&y=two%20words#frag");
            const rel = new URL("../c?q", "https://example.com/a/b/");
            const p = new URLSearchParams("?a=1&b=2&a=3");
            p.append("c", "x y");
            const s = new URL("https://example.com/?a=1");
            s.searchParams.set("b", "2");
            s.searchParams.delete("a");
            const t = new URL("https://example.com/");
            t.search = "?k=v"; t.hash = "h"; t.pathname = "/p q";
            let invalid; try { new URL("not a url"); } catch (e) { invalid = e.name; }
            return {
                href: u.href, protocol: u.protocol, host: u.host, hostname: u.hostname, port: u.port,
                pathname: u.pathname, search: u.search, hash: u.hash, origin: u.origin,
                username: u.username, y: u.searchParams.get("y"),
                rel: rel.toString(), json: JSON.stringify({ u: rel }),
                all: p.getAll("a"), params: p.toString(), has: p.has("b"),
                s: s.href, t: t.href, invalid,
                can: [URL.canParse("https://x"), URL.canParse("x")],
                entries: [...new URLSearchParams({ k: "v", n: 1 })],
            };
            "#,
        )
        .unwrap();
    assert_eq!(
        r,
        json!({
            "href": "https://user:pw@example.com:8443/a/b?x=1&y=two%20words#frag",
            "protocol": "https:", "host": "example.com:8443", "hostname": "example.com", "port": "8443",
            "pathname": "/a/b", "search": "?x=1&y=two%20words", "hash": "#frag", "origin": "https://example.com:8443",
            "username": "user", "y": "two words",
            "rel": "https://example.com/a/c?q", "json": "{\"u\":\"https://example.com/a/c?q\"}",
            "all": ["1", "3"], "params": "a=1&b=2&a=3&c=x+y", "has": true,
            "s": "https://example.com/?b=2", "t": "https://example.com/p%20q?k=v#h", "invalid": "TypeError",
            "can": [true, false], "entries": [["k", "v"], ["n", "1"]],
        })
    );
}

#[test]
fn storage_persists_and_enforces_limits() {
    let limits = Limits { storage_value_bytes: 16, storage_keys: 3, ..Limits::default() };
    let h = Harness::with(Capabilities::default(), limits.clone(), tempfile::tempdir().unwrap());
    let r = h
        .run(
            r#"
            localStorage.setItem("a", "1");
            localStorage.setItem("b", { toString() { return "two"; } });
            sessionStorage.setItem("s", "session");
            let quota; try { localStorage.setItem("big", "x".repeat(17)); } catch (e) { quota = e.name; }
            localStorage.setItem("c", "3");
            let keys; try { localStorage.setItem("d", "4"); } catch (e) { keys = e.name + " " + e.message; }
            localStorage.removeItem("c");
            localStorage.flush();
            return { a: localStorage.getItem("a"), b: localStorage.getItem("b"), none: localStorage.getItem("zz"),
                     len: localStorage.length, k0: localStorage.key(0), k9: localStorage.key(9), quota, keys,
                     isStorage: localStorage instanceof Storage };
            "#,
        )
        .unwrap();
    assert_eq!(r["a"], "1");
    assert_eq!(r["b"], "two");
    assert_eq!(r["none"], serde_json::Value::Null);
    assert_eq!(r["len"], 2);
    assert_eq!(r["k0"], "a");
    assert_eq!(r["k9"], serde_json::Value::Null);
    assert_eq!(r["quota"], "QuotaExceededError");
    assert!(r["keys"].as_str().unwrap().starts_with("QuotaExceededError"), "{r}");
    assert_eq!(r["isStorage"], true);
    let file = std::fs::read_to_string(h.dir.path().join("localStorage.json")).unwrap();
    assert_eq!(serde_json::from_str::<serde_json::Value>(&file).unwrap(), json!({"a": "1", "b": "two"}));

    // A new instance on the same data dir sees localStorage but not sessionStorage.
    let dir = tempfile::tempdir().unwrap();
    std::fs::copy(h.dir.path().join("localStorage.json"), dir.path().join("localStorage.json")).unwrap();
    let again = Harness::with(Capabilities::default(), limits, dir);
    let r = again.run(r#"return [localStorage.getItem("b"), sessionStorage.getItem("s"), sessionStorage.length];"#).unwrap();
    assert_eq!(r, json!(["two", null, 0]));
}

#[test]
fn storage_is_written_in_the_background() {
    let h = Harness::new(Capabilities::default());
    h.run(r#"localStorage.setItem("k", "v");"#).unwrap();
    let path = h.dir.path().join("localStorage.json");
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while !path.exists() {
        assert!(std::time::Instant::now() < deadline, "background write never landed");
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
}

#[test]
fn storage_denied() {
    let caps = Capabilities { storage: false, ..Capabilities::default() };
    let h = Harness::new(caps);
    let err = h.run("localStorage.getItem('a');").unwrap_err();
    assert_eq!(
        err,
        "TypeError: localStorage: not granted — set \"storage\": true in capabilities in native-shell.json"
    );
    assert!(h.run("sessionStorage.length;").unwrap_err().contains("sessionStorage: not granted"));
}

#[test]
fn fs_round_trip() {
    let caps = Capabilities { fs: true, ..Capabilities::default() };
    let h = Harness::new(caps);
    let r = h
        .run(
            r#"
            const fs = globalThis.__nsh_services.fs;
            await fs.mkdir("notes/2026", { recursive: true });
            await fs.writeFile("notes/2026/a.txt", "héllo");
            await fs.writeFile("bin", new Uint8Array([1, 2, 3]));
            const text = await fs.readFile("notes/2026/a.txt", "utf8");
            const bytes = await fs.readFile("bin");
            const st = await fs.stat("notes/2026/a.txt");
            await fs.rename("bin", "notes/bin2");
            const list = await fs.readdir("notes");
            const exists = [await fs.exists("bin"), await fs.exists("notes/bin2")];
            await fs.unlink("notes/bin2");
            let missing; try { await fs.readFile("nope"); } catch (e) { missing = e.code; }
            let escape; try { await fs.readFile("../localStorage.json"); } catch (e) { escape = e.message; }
            let absolute; try { await fs.writeFile("/tmp/x", "x"); } catch (e) { absolute = e.message; }
            let notEmpty; try { await fs.rmdir("notes"); } catch (e) { notEmpty = !!e.message; }
            const promise = fs.exists("x") instanceof Promise;
            return { text, bytes: [...bytes], isU8: bytes instanceof Uint8Array, size: st.size, is_file: st.is_file,
                     is_dir: st.is_dir, mtime: st.mtime_ms > 0, list, exists, missing, escape, absolute, notEmpty, promise };
            "#,
        )
        .unwrap();
    assert_eq!(
        r,
        json!({
            "text": "héllo", "bytes": [1, 2, 3], "isU8": true, "size": 6, "is_file": true, "is_dir": false,
            "mtime": true, "list": ["2026", "bin2"], "exists": [false, true], "missing": "ENOENT",
            "escape": "fs: `../localStorage.json` leaves the program's data directory",
            "absolute": "fs: `/tmp/x` must be relative to the program's data directory",
            "notEmpty": true, "promise": true,
        })
    );
    assert!(h.dir.path().join("files/notes/2026/a.txt").exists());
}

#[cfg(unix)]
#[test]
fn fs_symlink_escape() {
    let caps = Capabilities { fs: true, ..Capabilities::default() };
    let h = Harness::new(caps);
    std::fs::create_dir_all(h.dir.path().join("files")).unwrap();
    std::fs::write(h.dir.path().join("secret"), "s").unwrap();
    std::os::unix::fs::symlink(h.dir.path(), h.dir.path().join("files/up")).unwrap();
    let err = h.run(r#"return await __nsh_services.fs.readFile("up/secret", "utf8");"#).unwrap_err();
    assert!(err.contains("leaves the program's data directory"), "{err}");
}

#[test]
fn fs_denied() {
    let h = Harness::new(Capabilities::default());
    let err = h.run(r#"await __nsh_services.fs.readdir(".");"#).unwrap_err();
    assert_eq!(err, "TypeError: fs/promises: not granted — set \"fs\": true in capabilities in native-shell.json");
}

#[test]
fn hidden_services_object_is_not_enumerable() {
    let h = Harness::new(Capabilities::default());
    let r = h.run("return [Object.keys(globalThis).includes('__nsh_services'), typeof __nsh_services.fs.readFile];").unwrap();
    assert_eq!(r, json!([false, "function"]));
}
