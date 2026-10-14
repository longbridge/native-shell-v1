//! `fetch` and `WebSocket` against local servers.

// tungstenite's handshake callback returns its large `Response` in `Err`.
#![allow(clippy::result_large_err)]

mod common;

use std::io::{BufRead as _, BufReader, Read as _, Write as _};
use std::net::{TcpListener, TcpStream};

use common::{network, Harness};
use native_shell_core::manifest::Capabilities;
use native_shell_services::Limits;
use serde_json::json;

/// A minimal HTTP/1.1 server: one request per connection.
fn http_server() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            std::thread::spawn(move || serve(stream, port));
        }
    });
    port
}

fn serve(stream: TcpStream, port: u16) {
    let mut reader = BufReader::new(stream.try_clone().unwrap());
    let mut line = String::new();
    reader.read_line(&mut line).unwrap();
    let mut parts = line.split_whitespace();
    let method = parts.next().unwrap_or_default().to_string();
    let path = parts.next().unwrap_or_default().to_string();
    let mut headers = Vec::new();
    let mut length = 0;
    loop {
        let mut h = String::new();
        reader.read_line(&mut h).unwrap();
        let h = h.trim_end().to_string();
        if h.is_empty() {
            break;
        }
        let (k, v) = h.split_once(':').unwrap();
        if k.eq_ignore_ascii_case("content-length") {
            length = v.trim().parse().unwrap();
        }
        headers.push((k.to_ascii_lowercase(), v.trim().to_string()));
    }
    let mut body = vec![0; length];
    reader.read_exact(&mut body).unwrap();
    let header = |name: &str| headers.iter().find(|(k, _)| k == name).map(|(_, v)| v.clone()).unwrap_or_default();

    let (status, extra, out): (&str, Vec<String>, Vec<u8>) = match path.as_str() {
        "/hello" => ("200 OK", vec!["X-Test: a".into(), "X-Test: b".into(), "Content-Type: text/plain".into()], b"hello".to_vec()),
        "/json" => ("200 OK", vec!["Content-Type: application/json".into()], br#"{"a":1,"b":[true,null]}"#.to_vec()),
        "/echo" => (
            "200 OK",
            vec![],
            serde_json::to_vec(&json!({
                "method": method,
                "type": header("content-type"),
                "auth": header("authorization"),
                "custom": header("x-custom"),
                "body": String::from_utf8_lossy(&body),
            }))
            .unwrap(),
        ),
        "/redirect" => ("302 Found", vec!["Location: /echo".into()], vec![]),
        "/redirect-post" => ("307 Temporary Redirect", vec!["Location: /echo".into()], vec![]),
        "/see-other" => ("303 See Other", vec!["Location: /echo".into()], vec![]),
        "/away" => ("302 Found", vec![format!("Location: http://localhost:{port}/hello")], vec![]),
        "/loop" => ("302 Found", vec!["Location: /loop".into()], vec![]),
        "/big" => ("200 OK", vec![], vec![b'x'; 2048]),
        "/error" => ("500 Internal Server Error", vec![], b"boom".to_vec()),
        _ => ("404 Not Found", vec![], b"nope".to_vec()),
    };
    let mut response = format!("HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n", out.len());
    for h in extra {
        response.push_str(&h);
        response.push_str("\r\n");
    }
    response.push_str("\r\n");
    let mut stream = stream;
    stream.write_all(response.as_bytes()).unwrap();
    stream.write_all(&out).unwrap();
}

#[test]
fn fetch_get_post_json_status_headers() {
    let port = http_server();
    let h = Harness::new(network(&["127.0.0.1"]));
    let base = format!("http://127.0.0.1:{port}");
    let r = h
        .run(&format!(
            r#"
            const base = "{base}";
            const hello = await fetch(base + "/hello");
            const text = await hello.text();
            const json = await (await fetch(new URL("/json", base))).json();
            const post = await (await fetch(base + "/echo", {{
                method: "post", body: JSON.stringify({{ x: 1 }}),
                headers: {{ "Content-Type": "application/json", "X-Custom": "c" }},
            }})).json();
            const bytes = await (await fetch(base + "/echo", {{ method: "PUT", body: new Uint8Array([104, 105]) }})).json();
            const missing = await fetch(base + "/missing");
            const error = await fetch(base + "/error");
            const buf = await (await fetch(base + "/hello")).arrayBuffer();
            return {{
                status: hello.status, ok: hello.ok, statusText: hello.statusText, url: hello.url, text,
                xtest: hello.headers.get("x-test"), has: hello.headers.has("Content-Type"),
                names: [...hello.headers.keys()].filter(k => k.startsWith("x") || k === "content-type"),
                json, post, bytes: bytes.body + " " + bytes.method,
                missing: [missing.status, missing.ok, await missing.text()],
                error: [error.status, error.statusText, await error.text()],
                buf: buf.byteLength,
            }};
            "#
        ))
        .unwrap();
    assert_eq!(
        r,
        json!({
            "status": 200, "ok": true, "statusText": "OK", "url": format!("{base}/hello"), "text": "hello",
            "xtest": "a, b", "has": true, "names": ["content-type", "x-test"],
            "json": {"a": 1, "b": [true, null]},
            "post": {"method": "POST", "type": "application/json", "auth": "", "custom": "c", "body": "{\"x\":1}"},
            "bytes": "hi PUT",
            "missing": [404, false, "nope"],
            "error": [500, "Internal Server Error", "boom"],
            "buf": 5,
        })
    );
}

#[test]
fn fetch_redirects() {
    let port = http_server();
    let limits = Limits { fetch_redirects: 3, fetch_body_bytes: 1024, ..Limits::default() };
    let h = Harness::with(network(&["127.0.0.1"]), limits, tempfile::tempdir().unwrap());
    let base = format!("http://127.0.0.1:{port}");
    let r = h
        .run(&format!(
            r#"
            const base = "{base}";
            const r = await fetch(base + "/redirect", {{ headers: {{ Authorization: "Bearer t" }} }});
            const kept = await r.json();
            const post = await (await fetch(base + "/see-other", {{ method: "POST", body: "x" }})).json();
            const replay = await (await fetch(base + "/redirect-post", {{ method: "POST", body: "y" }})).json();
            return {{ redirected: r.redirected, url: r.url, auth: kept.auth, post: post.method, replay: [replay.method, replay.body] }};
            "#
        ))
        .unwrap();
    assert_eq!(
        r,
        json!({ "redirected": true, "url": format!("{base}/echo"), "auth": "Bearer t", "post": "GET", "replay": ["POST", "y"] })
    );

    let away = h.run(&format!(r#"await fetch("{base}/away");"#)).unwrap_err();
    assert_eq!(
        away,
        format!(
            "TypeError: fetch redirect to http://localhost:{port}/hello: not granted — add \"localhost\" to capabilities.network.hosts in native-shell.json"
        )
    );
    let looped = h.run(&format!(r#"await fetch("{base}/loop");"#)).unwrap_err();
    assert!(looped.contains("more than 3 redirects"), "{looped}");
    let big = h.run(&format!(r#"await fetch("{base}/big");"#)).unwrap_err();
    assert!(big.contains("over the 1024-byte limit"), "{big}");
    let upload = h.run(&format!(r#"await fetch("{base}/echo", {{ method: "POST", body: "x".repeat(2000) }});"#)).unwrap_err();
    assert!(upload.starts_with("RangeError"), "{upload}");
    assert!(!h.services.has_pending());
}

#[test]
fn fetch_denied_and_invalid() {
    let h = Harness::new(Capabilities::default());
    let err = h.run(r#"await fetch("https://x.com/");"#).unwrap_err();
    assert_eq!(
        err,
        "TypeError: fetch https://x.com/: not granted — add \"x.com\" to capabilities.network.hosts in native-shell.json"
    );
    let h = Harness::new(network(&["*"]));
    assert!(h.run(r#"await fetch("file:///etc/passwd");"#).unwrap_err().contains("only http and https"));
    assert!(h.run(r#"await fetch("nope");"#).unwrap_err().contains("invalid URL"));
    assert!(h.run(r#"await fetch("http://127.0.0.1:1/", { headers: { Host: "x" } });"#).unwrap_err().contains("may not be set"));
    assert!(h.run(r#"await fetch("http://127.0.0.1:1/", { body: "x" });"#).unwrap_err().contains("cannot have a body"));
    // Nothing listens on port 1: a transport error rejects with a TypeError.
    assert!(h.run(r#"await fetch("http://127.0.0.1:1/");"#).unwrap_err().starts_with("TypeError: fetch http://127.0.0.1:1/ failed"));
}

/// Echoes text and binary messages; closes when the client does.
fn ws_server() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            std::thread::spawn(move || {
                let callback = |req: &tungstenite::handshake::server::Request,
                                mut res: tungstenite::handshake::server::Response| {
                    if let Some(p) = req.headers().get("sec-websocket-protocol") {
                        let first = p.to_str().unwrap().split(',').next().unwrap().trim().to_string();
                        res.headers_mut().insert("sec-websocket-protocol", first.parse().unwrap());
                    }
                    Ok(res)
                };
                let mut ws = tungstenite::accept_hdr(stream, callback).unwrap();
                loop {
                    match ws.read() {
                        Ok(m @ (tungstenite::Message::Text(_) | tungstenite::Message::Binary(_))) => {
                            if m.to_text().map(|t| t == "bye").unwrap_or(false) {
                                ws.close(Some(tungstenite::protocol::CloseFrame {
                                    code: 4000.into(),
                                    reason: "server says bye".into(),
                                }))
                                .unwrap();
                            } else {
                                ws.send(m).unwrap();
                            }
                        }
                        Ok(_) => {}
                        Err(_) => break,
                    }
                }
            });
        }
    });
    port
}

#[test]
fn websocket_echo_and_close() {
    let port = ws_server();
    let h = Harness::new(network(&["127.0.0.1"]));
    let r = h
        .run(&format!(
            r#"
            const ws = new WebSocket("ws://127.0.0.1:{port}/chat", ["chat.v1", "chat.v0"]);
            const states = [ws.readyState];
            let early; try {{ ws.send("too soon"); }} catch (e) {{ early = e.name; }}
            const got = [];
            const closed = new Promise((resolve) => {{
                ws.addEventListener("close", (e) => resolve([e.code, e.reason, e.wasClean]));
            }});
            await new Promise((resolve, reject) => {{ ws.onopen = resolve; ws.onerror = (e) => reject(new Error(e.message)); }});
            states.push(ws.readyState);
            ws.binaryType = "arraybuffer";
            const next = () => new Promise((resolve) => {{ ws.onmessage = (e) => resolve(e.data); }});
            let m = next(); ws.send("hello"); got.push(await m);
            m = next(); ws.send(new Uint8Array([1, 2, 3])); const bin = await m;
            got.push(bin instanceof ArrayBuffer, [...new Uint8Array(bin)]);
            m = next(); ws.send(new Uint16Array([0x0102]).buffer); got.push(new Uint8Array(await m).length);
            ws.close(1000, "done");
            states.push(ws.readyState);
            const close = await closed;
            states.push(ws.readyState);
            return {{ states, early, got, protocol: ws.protocol, close }};
            "#
        ))
        .unwrap();
    assert_eq!(
        r,
        json!({
            "states": [0, 1, 2, 3], "early": "InvalidStateError",
            "got": ["hello", true, [1, 2, 3], 2], "protocol": "chat.v1",
            "close": [1000, "done", true],
        })
    );
    // The server closing first.
    let r = h
        .run(&format!(
            r#"
            const ws = new WebSocket("ws://127.0.0.1:{port}/");
            await new Promise((r) => ws.onopen = r);
            ws.send("bye");
            return await new Promise((r) => ws.onclose = (e) => r([e.code, e.reason, e.wasClean, ws.readyState]));
            "#
        ))
        .unwrap();
    assert_eq!(r, json!([4000, "server says bye", true, 3]));
    assert!(!h.services.has_pending());
}

#[test]
fn websocket_denied_and_unreachable() {
    let h = Harness::new(Capabilities::default());
    let err = h.run(r#"new WebSocket("wss://stream.example.com/q");"#).unwrap_err();
    assert_eq!(
        err,
        "TypeError: WebSocket wss://stream.example.com/q: not granted — add \"stream.example.com\" to capabilities.network.hosts in native-shell.json"
    );
    let h = Harness::new(network(&["127.0.0.1"]));
    assert!(h.run(r#"new WebSocket("ftp://127.0.0.1/");"#).unwrap_err().starts_with("SyntaxError"));
    let r = h
        .run(
            r#"
            const ws = new WebSocket("ws://127.0.0.1:1/");
            const events = [];
            ws.onerror = () => events.push("error");
            return await new Promise((r) => ws.onclose = (e) => r([...events, e.code, e.wasClean]));
            "#,
        )
        .unwrap();
    assert_eq!(r, json!(["error", 1006, false]));
}
