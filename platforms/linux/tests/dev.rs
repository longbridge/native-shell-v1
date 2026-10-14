//! The `nshell dev` client against a small in-process server speaking the
//! same HTTP endpoints (crates/cli/src/dev.rs).

mod support;

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::process::ExitCode;
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

use native_shell_gtk::{DevClient, Runtime};
use serde_json::json;
use support::*;

fn main() -> ExitCode {
    run(&[
        ("loads and reloads in place", loads_and_reloads_in_place),
        ("server errors show the error page until fixed", server_errors_show_the_error_page_until_fixed),
        ("an unreachable server shows the error page", an_unreachable_server_shows_the_error_page),
        ("logs reach the server", logs_reach_the_server),
    ])
}

#[derive(Default)]
struct State {
    version: u32,
    main_js: String,
    error: Option<String>,
    logs: Vec<String>,
}

/// Serves `/version` (long poll), `/bundle` and `/log` like `nshell dev`.
#[derive(Clone)]
struct DevServer {
    state: Arc<(Mutex<State>, Condvar)>,
    url: String,
}

impl DevServer {
    fn start(main_js: &str) -> DevServer {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://127.0.0.1:{}", listener.local_addr().unwrap().port());
        let server = DevServer { state: Arc::default(), url };
        server.publish(Some(main_js), None);
        let s = server.clone();
        std::thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                let s = s.clone();
                std::thread::spawn(move || s.serve(stream));
            }
        });
        server
    }

    /// A new version: a program, or the error reading it.
    fn publish(&self, main_js: Option<&str>, error: Option<&str>) {
        let (lock, changed) = &*self.state;
        let mut state = lock.lock().unwrap();
        state.version += 1;
        if let Some(js) = main_js {
            state.main_js = js.to_string();
        }
        state.error = error.map(str::to_string);
        changed.notify_all();
    }

    fn logs(&self) -> Vec<String> {
        self.state.0.lock().unwrap().logs.clone()
    }

    fn serve(&self, stream: TcpStream) {
        let mut reader = BufReader::new(stream.try_clone().unwrap());
        let mut line = String::new();
        reader.read_line(&mut line).unwrap();
        let target = line.split_whitespace().nth(1).unwrap_or("/").to_string();
        let mut length = 0;
        loop {
            let mut header = String::new();
            reader.read_line(&mut header).unwrap();
            if header.trim().is_empty() {
                break;
            }
            if let Some(v) = header.to_ascii_lowercase().strip_prefix("content-length:") {
                length = v.trim().parse().unwrap_or(0);
            }
        }
        let mut body = vec![0; length];
        reader.read_exact(&mut body).unwrap();
        let (lock, changed) = &*self.state;
        let (status, answer) = match target.split_once('?').map_or((target.as_str(), ""), |(p, q)| (p, q)) {
            ("/version", query) => {
                let since: Option<u32> = query.strip_prefix("since=").and_then(|v| v.parse().ok());
                let mut state = lock.lock().unwrap();
                if since == Some(state.version) {
                    state = changed.wait_timeout(state, Duration::from_secs(2)).unwrap().0;
                }
                (200, json!({ "version": state.version.to_string(), "error": state.error }).to_string())
            }
            ("/bundle", _) => {
                let state = lock.lock().unwrap();
                match &state.error {
                    Some(e) => (500, e.clone()),
                    None => (200, json!({ "manifest": manifest_json(), "files": { "main.js": state.main_js } }).to_string()),
                }
            }
            ("/log", _) => {
                lock.lock().unwrap().logs.push(String::from_utf8_lossy(&body).into_owned());
                (200, "{}".to_string())
            }
            _ => (404, String::new()),
        };
        let mut stream = stream;
        let _ = write!(stream, "HTTP/1.1 {status} X\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{answer}", answer.len());
    }
}

fn manifest_json() -> serde_json::Value {
    serde_json::from_str(MANIFEST).unwrap()
}

fn program(id: &str) -> String {
    format!(
        r#"import {{ View, div }} from "native-shell";
export default class App extends View {{
  init() {{ console.log("started {id}"); }}
  render() {{ return div().size_full().test_id("{id}"); }}
}}"#
    )
}

/// A window showing the client's program.
fn show(client: &DevClient) -> gtk::Window {
    let window = gtk::Window::builder().decorated(false).default_width(WIDTH).default_height(HEIGHT).child(&client.widget()).build();
    window.present();
    window
}

fn wait_for(window: &gtk::Window, test_id: &str) -> gtk::Widget {
    let mut found = None;
    pump_until(Duration::from_secs(10), || {
        found = find_test_id(window.upcast_ref(), test_id);
        found.is_some()
    });
    found.unwrap_or_else(|| panic!("{test_id:?} never appeared"))
}

fn loads_and_reloads_in_place() {
    let server = DevServer::start(&program("first"));
    let client = DevClient::connect(&server.url, &Runtime::new()).unwrap();
    let window = show(&client);
    wait_for(&window, "first");
    let first = client.surface().unwrap();
    let first_version = client.version().unwrap();

    server.publish(Some(&program("second")), None);
    wait_for(&window, "second");
    assert!(find_test_id(window.upcast_ref(), "first").is_none(), "the old program is gone");
    assert_ne!(client.surface().unwrap(), first, "a fresh surface");
    assert_ne!(client.version().unwrap(), first_version);
    assert_eq!(client.widget().root().and_downcast::<gtk::Window>().as_ref(), Some(&window), "in the same window");
    window.destroy();
}

fn server_errors_show_the_error_page_until_fixed() {
    let server = DevServer::start(&program("good"));
    let client = DevClient::connect(&server.url, &Runtime::new()).unwrap();
    let window = show(&client);
    wait_for(&window, "good");
    server.publish(None, Some("main.js: Unexpected token '}' at 3:1"));
    wait_for(&window, "nsh-error-page");
    let surface = client.surface().unwrap();
    next_frame(surface.upcast_ref());
    screenshot(surface.upcast_ref(), std::path::Path::new("target/agents/linux-app/dev-error.png"));
    server.publish(Some(&program("fixed")), None);
    wait_for(&window, "fixed");
    window.destroy();
}

fn an_unreachable_server_shows_the_error_page() {
    // A port nothing listens on: bound, then released.
    let port = TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
    let client = DevClient::connect(&format!("http://127.0.0.1:{port}"), &Runtime::new()).unwrap();
    let window = show(&client);
    wait_for(&window, "nsh-error-page");
    assert!(DevClient::connect("https://example.org:1", &Runtime::new()).is_err());
    window.destroy();
}

fn logs_reach_the_server() {
    let server = DevServer::start(&program("logging"));
    let client = DevClient::connect(&server.url, &Runtime::new()).unwrap();
    let window = show(&client);
    wait_for(&window, "logging");
    assert!(pump_until(Duration::from_secs(5), || server.logs().iter().any(|l| l.contains("started logging"))), "logs: {:?}", server.logs());
    let line: serde_json::Value = serde_json::from_str(server.logs().iter().find(|l| l.contains("started logging")).unwrap()).unwrap();
    assert_eq!(line["level"], "info");
    window.destroy();
}
