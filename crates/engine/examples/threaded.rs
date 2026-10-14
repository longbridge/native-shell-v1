//! Click-to-batch latency through the threaded runner, with a measurer that
//! costs ~0.3 ms per text (a phone's text layout), compared with the core's own
//! `tick_ns`.
//!
//! `cargo run --release -p native-shell --example threaded`

use std::collections::BTreeMap;
use std::sync::mpsc::{channel, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use native_shell::core::host::{Host, LogLevel, Measurer, TextMetrics, WidthMode};
use native_shell::core::manifest::Manifest;
use native_shell::core::protocol::{decode, EventType, Prop, PropValue};
use native_shell::runner::ThreadedInstance;
use native_shell::InstanceConfig;
use native_shell_tree::testing::{FakeMeasurer, Replayer};

struct SlowHost {
    ready: Mutex<Sender<Instant>>,
}

impl Measurer for SlowHost {
    fn measure_text(&self, json: &str, width: WidthMode) -> TextMetrics {
        let until = Instant::now() + Duration::from_micros(300);
        while Instant::now() < until {}
        FakeMeasurer.measure_text(json, width)
    }
}

impl Host for SlowHost {
    fn batch_ready(&self) {
        let _ = self.ready.lock().unwrap().send(Instant::now());
    }
    fn call_sync(&self, _: &str, _: &str, _: &str) -> Result<String, String> {
        Err("none".into())
    }
    fn call_async(&self, _: u64, _: &str, _: &str, _: &str) {}
    fn log(&self, _: LogLevel, m: &str) {
        eprintln!("{m}");
    }
}

fn program(rows: usize) -> String {
    format!(
        r#"
import {{ View, div }} from "gpui-kit";
import {{ h_flex, v_flex, Button }} from "gpui-base";
const row = (i) => h_flex().gap_2().px_2().py_1().items_center()
  .child(div().w(20).h(20).rounded_md().bg("primary"))
  .child(div().flex_1().text_sm().child(`row ${{i}}`))
  .child(div().text_xs().text_color("muted_foreground").child("detail"))
  .child(Button.new(`b${{i}}`).px_2().on_click(() => {{}}).child("go"));
export default class B extends View {{
  init() {{ this.n = 0; }}
  render(cx) {{
    const r = []; for (let i = 0; i < {rows}; i++) r.push(row(i));
    return v_flex().size_full()
      .child(div().test_id("counter").child(`n=${{this.n}}`).on_click((_e, cx) => {{ this.n += 1; cx.notify(); }}))
      .child(v_flex().children(r));
  }}
}}
"#
    )
}

fn main() {
    println!("{:>6} {:>8} {:>9} {:>9} {:>9} {:>9}", "rows", "natives", "wall ms", "tick ms", "measures", "gap ms");
    for rows in [170usize, 500, 1700] {
        let (tx, rx) = channel();
        let host = Arc::new(SlowHost { ready: Mutex::new(tx) });
        let instance = ThreadedInstance::spawn(InstanceConfig::new("bench"), host).unwrap();
        let manifest = Manifest::parse(r#"{"id":"bench","name":"Bench"}"#).unwrap();
        instance.load_sources(manifest, BTreeMap::from([("main.js".to_string(), program(rows))])).unwrap();
        let mut replay = Replayer::new();
        let drain = |replay: &mut Replayer| {
            while let Some(b) = instance.take_batch() {
                replay.apply(&decode(&b).unwrap()).unwrap();
            }
        };
        let _ = rx.recv_timeout(Duration::from_secs(10));
        std::thread::sleep(Duration::from_millis(50));
        drain(&mut replay);
        while rx.try_recv().is_ok() {}
        let counter = *replay
            .nodes
            .iter()
            .find(|(_, n)| n.props.get(&Prop::TestId) == Some(&PropValue::Str("counter".into())))
            .unwrap()
            .0;
        let clicks = 10;
        let mut wall = Duration::ZERO;
        let before = instance.metrics().unwrap();
        for _ in 0..clicks {
            let t = Instant::now();
            instance.dispatch_event(counter, EventType::Click, r#"{"count":1}"#, 1);
            let ready = rx.recv_timeout(Duration::from_secs(10)).expect("batch_ready");
            wall += ready - t;
            drain(&mut replay);
            std::thread::sleep(Duration::from_millis(5));
            while rx.try_recv().is_ok() {}
        }
        let after = instance.metrics().unwrap();
        let tick = (after.tick_ns - before.tick_ns) as f64 / 1e6 / clicks as f64;
        let wall = wall.as_secs_f64() * 1000.0 / clicks as f64;
        let measures = (after.measure_text_calls - before.measure_text_calls) as f64 / clicks as f64;
        println!(
            "{:>6} {:>8} {:>9.2} {:>9.2} {:>9.1} {:>9.2}",
            rows,
            replay.nodes.len(),
            wall,
            tick,
            measures,
            wall - tick
        );
    }
}
