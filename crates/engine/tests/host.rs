//! Host modules (sync, async, events, permissions) and host components.

use native_shell::core::host::{HostComponentDescriptor, HostMethod, HostModuleDescriptor, HostRegistry, MethodKind};
use native_shell::core::protocol::{event_mask, Prop, PropValue, ViewType};
use native_shell::testing::Harness;
use native_shell::Error;

fn registry() -> HostRegistry {
    let method = |name: &str, kind| HostMethod { name: name.into(), kind, ts: None };
    HostRegistry {
        modules: vec![
            HostModuleDescriptor {
                name: "demo/quote".into(),
                methods: vec![method("quote", MethodKind::Async), method("subscribe", MethodKind::Async)],
                events: vec!["tick".into()],
                requires_permission: true,
                dts: None,
            },
            HostModuleDescriptor {
                name: "demo/user".into(),
                methods: vec![method("current_user", MethodKind::Sync), method("fail", MethodKind::Sync)],
                events: vec![],
                requires_permission: false,
                dts: None,
            },
        ],
        components: vec![
            HostComponentDescriptor {
                name: "demo-stock-cell".into(),
                events: vec!["select".into()],
                measurable: true,
                requires_permission: true,
                props_ts: None,
            },
            HostComponentDescriptor {
                name: "demo-chart".into(),
                events: vec![],
                measurable: false,
                requires_permission: true,
                props_ts: None,
            },
        ],
    }
}

const QUOTE_APP: &str = r#"
import { View, div, v_flex, host_component } from "native-shell";
import { quote, on } from "demo/quote";
import demoQuote from "demo/quote";
import { current_user, fail } from "demo/user";
export default class Q extends View {
  init(_p, cx) {
    this.user = current_user();
    this.price = "?";
    this.ticks = [];
    this.error = "";
    try { fail(); } catch (e) { this.error = e.message; }
    cx.spawn(async (cx) => {
      const q = await quote("700.HK");
      this.price = q.last;
      cx.notify();
      try { await demoQuote.quote("BAD"); } catch (e) { this.error += ` | ${e.message}`; cx.notify(); }
    });
    this.sub = on("tick", (p, cx) => { this.ticks.push(p.price); cx.notify(); });
  }
  render(cx) {
    return v_flex().size_full()
      .child(div().test_id("user").child(this.user.name))
      .child(div().test_id("price").child(String(this.price)))
      .child(div().test_id("ticks").child(this.ticks.join(",")))
      .child(div().test_id("error").child(this.error))
      .child(div().test_id("stop").child("stop").on_click(() => this.sub.cancel()))
      .child(host_component("demo-stock-cell", { symbol: "700.HK", intrinsic_height: 56 }).test_id("cell")
        .on("select", (p, cx) => { this.ticks.push(`selected ${p.symbol}`); cx.notify(); }))
      .child(host_component("demo-chart", { symbol: "700.HK" }).test_id("kline").w_full().h_40());
  }
}
"#;

fn quote_app(manifest: &str) -> Harness {
    Harness::build(&[("main.js", QUOTE_APP)])
        .manifest(manifest)
        .registry(registry())
        .sync_module("demo/user", |method, _args| match method {
            "current_user" => Ok(r#"{"name":"Ada","id":7}"#.into()),
            "__listeners" => Ok("null".into()),
            _ => Err("no such user method".into()),
        })
        .sync_module("demo/quote", |method, _| match method {
            "__listeners" => Ok("null".into()),
            _ => Err("sync quote".into()),
        })
        .async_module("demo/quote", |method, args| match (method, args) {
            ("quote", r#"["700.HK"]"#) => Some(Ok(r#"{"symbol":"700.HK","last":412.6}"#.into())),
            ("quote", _) => Some(Err("unknown symbol".into())),
            _ => None,
        })
        .start()
}

const GRANTED: &str = r#"{"id":"t.quote","name":"Q","capabilities":{"host_modules":["demo/quote"],"host_components":["demo-stock-cell","demo-chart"]}}"#;

#[test]
fn sync_and_async_methods_answer_through_the_host() {
    let h = quote_app(GRANTED);
    assert!(h.load_error().is_none(), "{:?} {:?}", h.load_error(), h.logs());
    assert_eq!(h.text_of("user"), "Ada");
    assert_eq!(h.text_of("price"), "412.6");
    assert_eq!(h.text_of("error"), "demo/user.fail: no such user method | unknown symbol");
    let calls = h.calls();
    let quote = calls.iter().find(|c| c.method == "quote").unwrap();
    assert_eq!((quote.module.as_str(), quote.args.as_str()), ("demo/quote", r#"["700.HK"]"#));
    assert!(quote.request.is_some());
}

#[test]
fn host_events_reach_listeners_until_cancelled() {
    let mut h = quote_app(GRANTED);
    // Listener counts are reported so the module can manage its upstream subscription.
    assert!(h
        .calls()
        .iter()
        .any(|c| c.module == "demo/quote" && c.method == "__listeners" && c.args == r#"["tick",1]"#));
    h.emit("demo/quote", "tick", r#"{"price":413}"#);
    h.emit("demo/quote", "tick", r#"{"price":414}"#);
    assert_eq!(h.text_of("ticks"), "413,414");
    h.tap("stop");
    assert!(h.calls().iter().any(|c| c.method == "__listeners" && c.args == r#"["tick",0]"#));
    h.emit("demo/quote", "tick", r#"{"price":415}"#);
    assert_eq!(h.text_of("ticks"), "413,414");
    // Unknown events and modules are ignored.
    h.emit("demo/quote", "nope", "{}");
    h.emit("demo/other", "tick", "{}");
}

#[test]
fn pending_async_calls_resolve_when_the_host_responds() {
    let mut h = Harness::build(&[(
        "main.js",
        r#"
import { View, div } from "native-shell";
import { subscribe } from "demo/quote";
export default class S extends View {
  init(_p, cx) { this.state = "waiting"; cx.spawn(async (cx) => { this.state = await subscribe("AAPL.US"); cx.notify(); }); }
  render() { return div().test_id("state").child(this.state); }
}
"#,
    )])
    .registry(registry())
    .start();
    assert_eq!(h.text_of("state"), "waiting");
    let id = h.calls().iter().find(|c| c.method == "subscribe").and_then(|c| c.request).unwrap();
    h.respond(id, true, r#""subscribed""#);
    assert_eq!(h.text_of("state"), "subscribed");
}

#[test]
fn importing_an_ungranted_module_fails_at_link_time() {
    let h =
        quote_app(r#"{"id":"t.quote","name":"Q","capabilities":{"host_components":["demo-stock-cell","demo-chart"]}}"#);
    let Some(Error::Capability(e)) = h.load_error() else { panic!("{:?}", h.load_error()) };
    assert!(e.contains("demo/quote") && e.contains("capabilities.host_modules"), "{e}");
    // demo/user does not require permission: only demo/quote was refused.
    assert!(!h.calls().iter().any(|c| c.method == "quote"));
}

#[test]
fn host_components_lay_out_and_emit_events() {
    let mut h = quote_app(GRANTED);
    assert_eq!(h.view_type("cell"), ViewType::HostComponent);
    let cell = h.find("cell").unwrap();
    assert_eq!(h.native().prop(cell, Prop::HostComponent), Some(&PropValue::Str("demo-stock-cell".into())));
    assert_eq!(
        h.native().prop(cell, Prop::HostProps),
        Some(&PropValue::Json(r#"{"symbol":"700.HK","intrinsic_height":56}"#.into()))
    );
    let frame = h.native().nodes[&cell].frame.unwrap();
    assert!(
        (frame.width - 390.0).abs() < 0.01 && (frame.height - 56.0).abs() < 0.01,
        "measured by the host: {frame:?}"
    );
    let kline = h.native().nodes[&h.find("kline").unwrap()].frame.unwrap();
    assert!((kline.width - 390.0).abs() < 0.01 && (kline.height - 160.0).abs() < 0.01, "sized by style: {kline:?}");
    assert_ne!(h.native().nodes[&cell].mask & event_mask::HOST_EVENT, 0);
    h.host_event("cell", "select", r#"{"symbol":"700.HK"}"#);
    assert_eq!(h.text_of("ticks"), "selected 700.HK");
}

#[test]
fn host_components_need_registration_and_permission() {
    let h = quote_app(
        r#"{"id":"t.quote","name":"Q","capabilities":{"host_modules":["demo/quote"],"host_components":["demo-stock-cell"]}}"#,
    );
    let Some(Error::FirstRender(e)) = h.load_error() else { panic!("{:?}", h.load_error()) };
    assert!(
        e.message.contains("`demo-chart` is not granted") && e.message.contains("capabilities.host_components"),
        "{e}"
    );

    let h = Harness::build(&[(
        "main.js",
        r#"
import { View, host_component } from "native-shell";
export default class S extends View { render() { return host_component("nope", {}); } }
"#,
    )])
    .registry(registry())
    .start();
    let Some(Error::FirstRender(e)) = h.load_error() else { panic!() };
    assert!(e.message.contains("unknown host component `nope`") && e.message.contains("demo-stock-cell"), "{e}");

    let h = Harness::build(&[(
        "main.js",
        r#"
import { View, host_component } from "native-shell";
export default class S extends View { render() { return host_component("demo-stock-cell", {}).on("tap", () => {}); } }
"#,
    )])
    .registry(registry())
    .start();
    let Some(Error::FirstRender(e)) = h.load_error() else { panic!() };
    assert!(e.message.contains("has no event \"tap\""), "{e}");
}

#[test]
fn the_capability_override_wins_over_the_manifest() {
    let h = Harness::build(&[("main.js", QUOTE_APP)])
        .manifest(r#"{"id":"t.quote","name":"Q"}"#)
        .registry(registry())
        .capabilities(native_shell::core::manifest::Capabilities::all())
        .sync_module("demo/user", |_, _| Ok(r#"{"name":"Root"}"#.into()))
        .start();
    assert!(h.load_error().is_none(), "{:?}", h.load_error());
    assert_eq!(h.text_of("user"), "Root");
}
