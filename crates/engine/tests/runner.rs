//! The threaded runner end to end.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use native_shell::core::host::{Host, LogLevel, Measurer, TextMetrics, WidthMode};
use native_shell::core::manifest::Manifest;
use native_shell::core::protocol::{decode, EventType, Prop, PropValue};
use native_shell::runner::ThreadedInstance;
use native_shell::InstanceConfig;
use native_shell_tree::testing::{FakeMeasurer, Replayer};

#[derive(Default)]
struct CountingHost {
    ready: AtomicUsize,
    logs: Mutex<Vec<String>>,
}

impl Measurer for CountingHost {
    fn measure_text(&self, json: &str, width: WidthMode) -> TextMetrics {
        FakeMeasurer.measure_text(json, width)
    }
}

impl Host for CountingHost {
    fn batch_ready(&self) {
        self.ready.fetch_add(1, Ordering::SeqCst);
    }
    fn call_sync(&self, _: &str, _: &str, _: &str) -> Result<String, String> {
        Err("none".into())
    }
    fn call_async(&self, _: u64, _: &str, _: &str, _: &str) {}
    fn log(&self, _: LogLevel, message: &str) {
        self.logs.lock().unwrap().push(message.to_string());
    }
}

const APP: &str = r#"
import { View, div } from "native-shell";
export default class T extends View {
  init(_p, cx) { this.n = 0; cx.timer.every(20, (cx) => { if (this.n < 3) { this.n += 1; cx.notify(); } }); }
  render() { return div().test_id("n").child(`n=${this.n}`).on_click((_e, cx) => { this.n = 100; cx.notify(); }); }
}
"#;

fn text_of(r: &Replayer, test_id: &str) -> Option<String> {
    let node = r.nodes.iter().find(|(_, n)| n.props.get(&Prop::TestId) == Some(&PropValue::Str(test_id.into())))?.0;
    let text = r.children(*node).first()?;
    match r.prop(*text, Prop::AttributedText)? {
        PropValue::Json(j) => {
            let v: serde_json::Value = serde_json::from_str(j).ok()?;
            Some(v["runs"][0]["text"].as_str()?.to_string())
        }
        _ => None,
    }
}

fn drain(instance: &ThreadedInstance, replay: &mut Replayer) {
    // take_batch from a thread other than the JS thread and the test thread.
    let batches: Vec<Vec<u8>> =
        std::thread::scope(|s| s.spawn(|| std::iter::from_fn(|| instance.take_batch()).collect()).join().unwrap());
    for b in batches {
        replay.apply(&decode(&b).unwrap()).unwrap();
    }
}

fn wait_for(instance: &ThreadedInstance, replay: &mut Replayer, test_id: &str, expect: &str) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        drain(instance, replay);
        if text_of(replay, test_id).as_deref() == Some(expect) {
            return;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    panic!("timed out waiting for {test_id} = {expect:?}, have {:?}", text_of(replay, test_id));
}

#[test]
fn a_threaded_instance_renders_ticks_and_takes_events() {
    let host = Arc::new(CountingHost::default());
    let instance = ThreadedInstance::spawn(InstanceConfig::new("t.runner"), host.clone()).unwrap();
    let manifest = Manifest::parse(r#"{"id":"t.runner","name":"Runner"}"#).unwrap();
    instance.load_sources(manifest, BTreeMap::from([("main.js".to_string(), APP.to_string())])).unwrap();

    let mut replay = Replayer::new();
    // Timers run on the JS thread's own clock: the first frame shows n=0, three ticks follow.
    wait_for(&instance, &mut replay, "n", "n=3");
    assert!(host.ready.load(Ordering::SeqCst) >= 1, "batch_ready was called");

    let node =
        *replay.nodes.iter().find(|(_, n)| n.props.get(&Prop::TestId) == Some(&PropValue::Str("n".into()))).unwrap().0;
    instance.dispatch_event(node, EventType::Click, r#"{"count":1}"#, 1);
    wait_for(&instance, &mut replay, "n", "n=100");
    let snapshot = instance.metrics_snapshot();
    assert!(
        snapshot.batches >= 1 && snapshot.tick_ns > 0 && snapshot.handoff_ns > 0 && snapshot.native_nodes > 0,
        "{snapshot:?}"
    );
    let metrics = instance.metrics().unwrap();
    assert!(metrics.batches >= 2 && metrics.timers_fired >= 3, "{metrics:?}");
    instance.shutdown();
    assert!(instance.metrics().is_none(), "the thread is gone");
}

#[test]
fn load_errors_come_back_to_the_caller() {
    let host = Arc::new(CountingHost::default());
    let instance = ThreadedInstance::spawn(InstanceConfig::new("t.runner"), host).unwrap();
    let manifest = Manifest::parse(r#"{"id":"t.runner","name":"Runner"}"#).unwrap();
    let err =
        instance.load_sources(manifest, BTreeMap::from([("main.js".to_string(), "syntax error here".to_string())]));
    assert!(matches!(err, Err(native_shell::Error::Module(_))), "{err:?}");
}

#[test]
fn the_handle_is_send_and_sync() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<ThreadedInstance>();
}
