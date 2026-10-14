//! The host API: host modules (sync, async, events) and host components.

mod support;

use std::process::ExitCode;
use std::sync::{Arc, Mutex};

use native_shell_gtk::{
    CallContext, EventSink, HostComponent, HostComponentType, HostMethod, HostModule, HostModuleDescriptor, MethodKind,
    Responder, Runtime,
};
use serde_json::{json, Value};
use support::*;

fn main() -> ExitCode {
    run(&[
        ("sync and async module calls", sync_and_async_module_calls),
        ("module events reach listening scripts", module_events_reach_listening_scripts),
        ("host components mount, update, measure and emit", host_components_mount_update_measure_and_emit),
        ("reserved names are refused", reserved_names_are_refused),
    ])
}

const MANIFEST: &str = r#"{
  "id": "dev.native-shell.host-test", "name": "Host", "version": "1.0.0", "entry": "main.js",
  "capabilities": { "host_modules": ["demo/greeter"], "host_components": ["demo-badge"] }
}"#;

/// `demo/greeter`: `name()` (sync), `greet(name)` (async, answered later from
/// another thread), `ping()` (async, emits `pong` to the caller), event `pong`.
#[derive(Default)]
struct Greeter {
    listeners: Arc<Mutex<Vec<(String, i64)>>>,
}

impl HostModule for Greeter {
    fn descriptor(&self) -> HostModuleDescriptor {
        let method = |name: &str, kind| HostMethod { name: name.into(), kind, ts: None };
        HostModuleDescriptor {
            name: "demo/greeter".into(),
            methods: vec![
                method("name", MethodKind::Sync),
                method("greet", MethodKind::Async),
                method("ping", MethodKind::Async),
                method("forget", MethodKind::Async),
            ],
            events: vec!["pong".into()],
            requires_permission: true,
            dts: None,
        }
    }

    fn call_sync(&self, method: &str, args: &[Value], _cx: &CallContext) -> Result<Value, String> {
        match method {
            "name" => Ok(json!("ada")),
            "__listeners" => {
                let event = args.first().and_then(Value::as_str).unwrap_or_default().to_string();
                let count = args.get(1).and_then(Value::as_i64).unwrap_or_default();
                self.listeners.lock().unwrap().push((event, count));
                Ok(Value::Null)
            }
            _ => Err(format!("no sync method {method}")),
        }
    }

    fn call(&self, method: &str, args: Vec<Value>, cx: CallContext, respond: Responder) {
        match method {
            "greet" => {
                let name = args.first().and_then(Value::as_str).unwrap_or_default().to_string();
                std::thread::spawn(move || {
                    if name.is_empty() {
                        respond.reject("nobody to greet");
                    } else {
                        respond.resolve(&json!(format!("hello-{name}")));
                    }
                });
            }
            "ping" => {
                cx.emit("pong", &json!({ "n": 1 }));
                respond.resolve(&Value::Null);
            }
            // Dropped unanswered: the script still gets an answer.
            "forget" => {}
            _ => respond.reject(format!("no method {method}")),
        }
    }
}

fn sync_and_async_module_calls() {
    let runtime = Runtime::new();
    runtime.register_module(Greeter::default()).unwrap();
    let f = Fixture::with_runtime(
        &runtime,
        MANIFEST,
        &[(
            "main.js",
            r#"
import { View, div, v_flex } from "native-shell";
import { name, greet, forget } from "demo/greeter";
export default class App extends View {
  init(_p, cx) {
    this.log = [`sync-${name()}`];
    cx.spawn(async (cx) => {
      this.log.push(await greet("bob"));
      try { await greet(""); } catch (e) { this.log.push(e.message.includes("nobody to greet") ? "rejected" : e.message); }
      try { await forget(); } catch (e) { this.log.push(e.message.includes("without answering") ? "dropped" : e.message); }
      cx.notify();
    });
  }
  render() { return v_flex().child(div().test_id(this.log.join("|")).h(10)); }
}"#,
        )],
    );
    f.wait_for("sync-ada|hello-bob|rejected|dropped");
}

fn module_events_reach_listening_scripts() {
    let runtime = Runtime::new();
    let greeter = Greeter::default();
    let listeners = greeter.listeners.clone();
    runtime.register_module(greeter).unwrap();
    let f = Fixture::with_runtime(
        &runtime,
        MANIFEST,
        &[(
            "main.js",
            r#"
import { View, div } from "native-shell";
import { on, ping } from "demo/greeter";
export default class App extends View {
  init(_p, cx) {
    this.got = [];
    this.sub = on("pong", (p, cx) => { this.got.push(p.n); cx.notify(); });
    cx.spawn(async () => { await ping(); });
  }
  render() {
    return div().test_id(`got-${this.got.join(",")}`).h(10).on_click(() => this.sub.cancel());
  }
}"#,
        )],
    );
    // `cx.emit` answers the caller; `Runtime::emit` broadcasts to every live instance.
    f.wait_for("got-1");
    runtime.emit("demo/greeter", "pong", &json!({ "n": 2 }));
    f.wait_for("got-1,2");
    assert_eq!(listeners.lock().unwrap().first(), Some(&("pong".to_string(), 1)));
    f.tap("got-1,2");
    pump_until(std::time::Duration::from_secs(2), || listeners.lock().unwrap().last() == Some(&("pong".to_string(), 0)));
    assert_eq!(listeners.lock().unwrap().last(), Some(&("pong".to_string(), 0)));
}

/// `demo-badge`: a button showing `props.label`; clicking emits `select`.
struct Badge {
    button: gtk::Button,
}

impl HostComponent for Badge {
    fn widget(&self) -> gtk::Widget {
        self.button.clone().upcast()
    }

    fn update(&mut self, props: &Value) {
        self.button.set_label(props["label"].as_str().unwrap_or_default());
    }
}

fn badge_type() -> HostComponentType {
    HostComponentType::new("demo-badge", |events: EventSink| {
        let button = gtk::Button::new();
        button.connect_clicked(move |b| events.emit("select", json!({ "label": b.label().map(|l| l.to_string()) })));
        Box::new(Badge { button })
    })
    .events(["select"])
    .measure(|_props, _width| (120.0, 56.0))
}

fn host_components_mount_update_measure_and_emit() {
    let runtime = Runtime::new();
    runtime.register_component(badge_type()).unwrap();
    let f = Fixture::with_runtime(
        &runtime,
        MANIFEST,
        &[(
            "main.js",
            r#"
import { View, div, v_flex, host_component } from "native-shell";
export default class App extends View {
  init() { this.selected = []; this.n = 1; }
  render() {
    return v_flex().items_start()
      .child(host_component("demo-badge", { label: `Badge ${this.n}` }).test_id("badge")
        .on("select", (p, cx) => { this.selected.push(p.label); this.n += 1; cx.notify(); }))
      .child(div().test_id(`selected-${this.selected.join(",")}`).h(10));
  }
}"#,
        )],
    );
    let badge = f.expect("badge");
    let bounds = f.bounds(&badge);
    assert_eq!((bounds.width(), bounds.height()), (120.0, 56.0), "the measured size lays the component out");
    let button = f.find_where(|w| w.is::<gtk::Button>()).expect("the component's widget is mounted");
    assert_eq!(button.downcast_ref::<gtk::Button>().unwrap().label().as_deref(), Some("Badge 1"));
    f.click("badge");
    f.expect("selected-Badge 1");
    assert_eq!(button.downcast_ref::<gtk::Button>().unwrap().label().as_deref(), Some("Badge 2"), "new props reach the same widget");
}

fn reserved_names_are_refused() {
    struct Clipboard;
    impl HostModule for Clipboard {
        fn descriptor(&self) -> HostModuleDescriptor {
            HostModuleDescriptor { name: "nsh/clipboard".into(), methods: vec![], events: vec![], requires_permission: false, dts: None }
        }
    }
    let runtime = Runtime::new();
    assert!(runtime.register_module(Clipboard).unwrap_err().contains("reserved"));
    assert!(runtime.register_component(HostComponentType::new("nsh/select", |_| unreachable!())).is_err());
    assert!(runtime.registry().modules.is_empty());
}
