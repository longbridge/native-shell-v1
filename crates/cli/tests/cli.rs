//! End-to-end tests of the `native-shell` binary.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::path::Path;
use std::process::{Child, Command, Output, Stdio};
use std::time::{Duration, Instant};

fn bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_native-shell"))
}

fn run(args: &[&str], cwd: &Path) -> Output {
    bin().args(args).current_dir(cwd).output().expect("run native-shell")
}

fn stdout(o: &Output) -> String {
    String::from_utf8_lossy(&o.stdout).into_owned()
}

fn stderr(o: &Output) -> String {
    String::from_utf8_lossy(&o.stderr).into_owned()
}

fn assert_ok(o: &Output) {
    assert!(o.status.success(), "failed: {}\nstdout:\n{}\nstderr:\n{}", o.status, stdout(o), stderr(o));
}

#[test]
fn new_check_types_bundle_verify() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();

    assert_ok(&run(&["new", "app"], root));
    for f in ["native-shell.json", "main.js", "jsconfig.json", "native-shell.d.ts", "AGENTS.md"] {
        assert!(root.join("app").join(f).is_file(), "{f} missing");
    }
    // A second `new` into a non-empty directory is refused.
    assert!(!run(&["new", "app"], root).status.success());

    let check = run(&["check", "app", "--print-tree"], root);
    assert_ok(&check);
    assert!(stdout(&check).contains("Count: 0"), "tree dump should show the text:\n{}", stdout(&check));

    std::fs::write(
        root.join("registry.json"),
        r#"{"modules":[{"name":"demo/quote","methods":[{"name":"quote","kind":"async","ts":"(symbol: string) => Promise<number>"}],"events":["tick"]}],
            "components":[{"name":"demo-stock-cell","props_ts":"{ symbol: string }"}]}"#,
    )
    .unwrap();
    assert_ok(&run(&["types", "app", "--registry", "registry.json"], root));
    let dts = std::fs::read_to_string(root.join("app/native-shell.d.ts")).unwrap();
    assert!(dts.contains("declare module \"demo/quote\""));
    assert!(dts.contains("export const quote: (symbol: string) => Promise<number>;"));
    assert!(dts.contains("\"demo-stock-cell\": { symbol: string };"));
    assert!(dts.contains("  flex_col(): this;"));

    assert_ok(&run(&["keygen", "--out", "keys/release"], root));
    assert!(root.join("keys/release.secret").is_file() && root.join("keys/release.pub").is_file());
    // Keys are never overwritten.
    assert!(!run(&["keygen", "--out", "keys/release"], root).status.success());

    for (out, extra) in [("app.nspkg", None), ("app-src.nspkg", Some("--no-bytecode"))] {
        let mut args = vec!["bundle", "app", "--key", "keys/release.secret", "--key-id", "release-1", "-o", out];
        args.extend(extra);
        assert_ok(&run(&args, root));
        let verify = run(&["verify", out, "--trusted", "keys/release.pub"], root);
        assert_ok(&verify);
        assert!(stdout(&verify).contains("OK org.example.app 0.1.0 (key `release-1`)"), "{}", stdout(&verify));
        let modules = if extra.is_some() { "0 bytecode modules" } else { "1 bytecode modules" };
        assert!(stdout(&verify).contains(modules), "{}", stdout(&verify));
    }

    // A different key does not verify; a tampered bundle does not either.
    assert_ok(&run(&["keygen", "--out", "keys/other"], root));
    let wrong = run(&["verify", "app.nspkg", "--trusted", "keys/other.pub"], root);
    assert!(!wrong.status.success());
    assert!(stderr(&wrong).contains("signature"), "{}", stderr(&wrong));
    let mut bytes = std::fs::read(root.join("app.nspkg")).unwrap();
    let n = bytes.len();
    bytes[n - 100] ^= 1;
    std::fs::write(root.join("tampered.nspkg"), bytes).unwrap();
    assert!(!run(&["verify", "tampered.nspkg", "--trusted", "keys/release.pub"], root).status.success());
}

/// A 1×1 PNG: not UTF-8.
const PNG: &[u8] = &[
    0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44, 0x52, 0x00, 0x00, 0x00,
    0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1f, 0x15, 0xc4, 0x89, 0x00, 0x00, 0x00, 0x0d, 0x49,
    0x44, 0x41, 0x54, 0x78, 0x9c, 0x63, 0xf8, 0xcf, 0xc0, 0xf0, 0x1f, 0x00, 0x05, 0x00, 0x01, 0xff, 0x89, 0x99, 0x3d,
    0x1d, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4e, 0x44, 0xae, 0x42, 0x60, 0x82,
];
const SVG: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><path d="M2 2h20v20H2z"/></svg>"#;

#[test]
fn check_loads_binary_files() {
    let tmp = tempfile::tempdir().unwrap();
    write_program(
        tmp.path(),
        "import { View, div, image, svg } from \"native-shell\";\nexport default class A extends View {\n  render(_cx) { return div().child(image(\"logo.png\").w(10).h(10)).child(svg(\"icon.svg\").w(10).h(10)); }\n}\n",
    );
    std::fs::write(tmp.path().join("logo.png"), PNG).unwrap();
    std::fs::write(tmp.path().join("icon.svg"), SVG).unwrap();
    let o = run(&["check", ".", "--print-tree"], tmp.path());
    assert_ok(&o);
    assert!(stdout(&o).contains("Image"), "the PNG should render as an image:\n{}", stdout(&o));
}

fn write_program(dir: &Path, main_js: &str) {
    std::fs::create_dir_all(dir).unwrap();
    std::fs::write(dir.join("native-shell.json"), r#"{"id":"org.example.t","name":"T"}"#).unwrap();
    std::fs::write(dir.join("main.js"), main_js).unwrap();
}

#[test]
fn check_reports_syntax_errors_with_location() {
    let tmp = tempfile::tempdir().unwrap();
    write_program(
        tmp.path(),
        "import { View, div } from \"native-shell\";\n\nexport default class A extends View {\n  render(cx) { return div(.child(\"x\"); }\n}\n",
    );
    let o = run(&["check", "."], tmp.path());
    assert_eq!(o.status.code(), Some(1), "stdout:\n{}\nstderr:\n{}", stdout(&o), stderr(&o));
    let err = stderr(&o);
    assert!(err.contains("main.js:4"), "expected file:line in:\n{err}");
}

#[test]
fn check_suggests_style_methods() {
    let tmp = tempfile::tempdir().unwrap();
    write_program(
        tmp.path(),
        "import { View, div } from \"native-shell\";\nexport default class A extends View {\n  render(cx) { return div().flex_coll().child(\"x\"); }\n}\n",
    );
    let o = run(&["check", "."], tmp.path());
    assert_eq!(o.status.code(), Some(1));
    let err = stderr(&o);
    assert!(err.contains("did you mean"), "expected a suggestion in:\n{err}");
    assert!(err.contains("flex_col"), "{err}");
}

/// Kills the dev server when the test ends, pass or fail.
struct Server(Child);

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn http(port: u16, method: &str, path: &str, body: &str) -> (u16, String) {
    let mut s = TcpStream::connect(("127.0.0.1", port)).unwrap();
    s.set_read_timeout(Some(Duration::from_secs(30))).unwrap();
    write!(s, "{method} {path} HTTP/1.1\r\nHost: localhost\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len())
        .unwrap();
    let mut resp = String::new();
    s.read_to_string(&mut resp).unwrap();
    let status = resp.split_whitespace().nth(1).and_then(|c| c.parse().ok()).unwrap_or(0);
    let body = resp.split_once("\r\n\r\n").map(|(_, b)| b.to_string()).unwrap_or_default();
    (status, body)
}

fn json(s: &str) -> serde_json::Value {
    serde_json::from_str(s).unwrap_or_else(|e| panic!("not JSON ({e}): {s}"))
}

#[test]
fn dev_server_serves_reloads_and_logs() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("app");
    write_program(&dir, "export default 1;\n");
    std::fs::write(dir.join("logo.png"), PNG).unwrap();
    std::fs::write(dir.join("icon.svg"), SVG).unwrap();

    let mut child = bin()
        .args(["dev", dir.to_str().unwrap(), "--port", "0"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut lines = BufReader::new(child.stdout.take().unwrap()).lines();
    let server = Server(child);
    let port: u16 = loop {
        let line = lines.next().expect("dev server exited").unwrap();
        if let Some(url) = line.split("http://").nth(1) {
            break url.rsplit(':').next().unwrap().trim().parse().unwrap();
        }
    };

    let (status, manifest) = http(port, "GET", "/manifest", "");
    assert_eq!(status, 200);
    assert_eq!(json(&manifest)["id"], "org.example.t");

    let (status, bundle) = http(port, "GET", "/bundle", "");
    assert_eq!(status, 200);
    let bundle = json(&bundle);
    assert_eq!(bundle["manifest"]["name"], "T");
    assert_eq!(bundle["files"]["main.js"], "export default 1;\n");
    assert!(bundle["files"].get("native-shell.json").is_none());
    // Binary files travel as data URLs; text ones as text.
    let png = bundle["files"]["logo.png"].as_str().unwrap();
    let b64 = png.strip_prefix("data:image/png;base64,").expect("a PNG data URL");
    use base64::Engine as _;
    assert_eq!(base64::engine::general_purpose::STANDARD.decode(b64).unwrap(), PNG);
    assert_eq!(bundle["files"]["icon.svg"], SVG);

    let (_, v) = http(port, "GET", "/version", "");
    let v1 = json(&v)["version"].as_str().unwrap().to_string();

    // A long poll with the current version is held until the program changes.
    let poll = std::thread::spawn(move || {
        let start = Instant::now();
        let (_, v) = http(port, "GET", &format!("/version?since={v1}"), "");
        (json(&v)["version"].as_str().unwrap().to_string(), start.elapsed(), v1)
    });
    std::thread::sleep(Duration::from_millis(600));
    std::fs::write(dir.join("main.js"), "export default 2;\n").unwrap();
    let (v2, waited, v1) = poll.join().unwrap();
    assert_ne!(v1, v2);
    assert!(waited >= Duration::from_millis(500), "long poll returned early ({waited:?})");
    assert!(waited < Duration::from_secs(10), "reload took {waited:?}");
    let (_, bundle) = http(port, "GET", "/bundle", "");
    assert_eq!(json(&bundle)["files"]["main.js"], "export default 2;\n");

    let (status, _) = http(port, "POST", "/log", r#"{"level":"warn","message":"hello from device"}"#);
    assert_eq!(status, 200);
    let logged = lines.by_ref().map_while(Result::ok).find(|l| l.contains("hello from device"));
    assert_eq!(logged.as_deref(), Some("[device warn] hello from device"));

    assert_eq!(http(port, "GET", "/nope", "").0, 404);
    drop(server);
}
