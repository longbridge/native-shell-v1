//! Batch flags, background/foreground, memory trimming, clipboard reads, stale events.

use native_shell::core::protocol::{batch_flags, EventType, Mutation};
use native_shell::testing::Harness;

const APP: &str = r#"
import { View, div, v_flex } from "native-shell";
export default class A extends View {
  init(_p, cx) { this.open = false; this.ticks = 0; this.clip = ""; cx.timer.every(100, (cx) => { this.ticks += 1; cx.notify(); }); }
  render(cx) {
    return v_flex().size_full()
      .child(div().test_id("ticks").child(`${this.ticks}`))
      .child(div().test_id("toggle").child("toggle").on_click((_e, cx) => { this.open = !this.open; cx.notify(); }))
      .child(div().test_id("panel").bg("primary").transition("frame", { duration: 200 }).h(this.open ? 200 : 40))
      .child(div().test_id("paste").child(this.clip || "paste").on_click((_e, cx) => { this.clip = cx.read_from_clipboard() ?? "empty"; cx.notify(); }));
  }
}
"#;

fn flags_of_last_batch(h: &Harness) -> u32 {
    h.mutations()
        .iter()
        .rev()
        .find_map(|m| match m {
            Mutation::BeginBatch { flags, .. } => Some(*flags),
            _ => None,
        })
        .unwrap()
}

#[test]
fn transitions_mark_batches_animated() {
    let mut h = Harness::new(&[("main.js", APP)]);
    assert!(h.load_error().is_none(), "{:?}", h.load_error());
    assert_eq!(flags_of_last_batch(&h) & batch_flags::INITIAL, batch_flags::INITIAL);
    h.tap("toggle");
    assert_eq!(flags_of_last_batch(&h), batch_flags::ANIMATED, "the panel with a frame transition moved");
    h.advance(100.0);
    assert_eq!(flags_of_last_batch(&h) & batch_flags::ANIMATED, 0, "a text change is not animated");
}

#[test]
fn inactive_instances_pause_timers_and_frames() {
    let mut h = Harness::new(&[("main.js", APP)]);
    h.advance(100.0);
    assert_eq!(h.text_of("ticks"), "1");
    h.instance_mut().set_active(false);
    let batches = h.batch_count();
    h.advance(1000.0);
    assert_eq!(h.batch_count(), batches);
    h.instance_mut().set_active(true);
    h.advance(0.0);
    assert!(h.text_of("ticks").parse::<u32>().unwrap() >= 2);
    h.instance_mut().trim_memory();
    h.advance(100.0);
    assert!(h.instance().metrics().memory_used_bytes > 0);
}

#[test]
fn clipboard_reads_go_through_the_host_and_are_gated() {
    let mut h =
        Harness::build(&[("main.js", APP)]).sync_module("nsh/clipboard", |_, _| Ok(r#""copied text""#.into())).start();
    h.tap("paste");
    assert_eq!(h.text_of("paste"), "copied text");

    let mut h = Harness::with_manifest(&[("main.js", APP)], r#"{"id":"t.clip","name":"Clip"}"#);
    h.tap("paste");
    assert!(h.error_logs().join("\n").contains("capabilities.clipboard.read"), "{:?}", h.error_logs());
}

#[test]
fn events_for_deleted_nodes_are_dropped() {
    let mut h = Harness::new(&[(
        "main.js",
        r#"
import { View, div, v_flex } from "native-shell";
export default class A extends View {
  init() { this.shown = true; this.hits = 0; }
  render() {
    return v_flex()
      .child(div().test_id("hits").child(`${this.hits}`))
      .when(this.shown, (el) => el.child(div().test_id("gone").child("x").on_click((_e, cx) => { this.hits += 1; this.shown = false; cx.notify(); })));
  }
}
"#,
    )]);
    let node = h.find("gone").unwrap();
    h.tap("gone");
    assert!(h.find("gone").is_none());
    let dropped = h.instance().metrics().events_dropped;
    // A tap the renderer sent before it applied the batch that deleted the node.
    h.dispatch(node, EventType::Click, r#"{"count":1}"#);
    assert_eq!(h.text_of("hits"), "1");
    assert_eq!(h.instance().metrics().events_dropped, dropped + 1);
}

#[test]
fn metrics_count_measurement_and_serialize() {
    let mut h = Harness::new(&[("main.js", APP)]);
    h.advance(100.0);
    let m = h.instance().metrics();
    assert!(m.measure_text_calls > 0 && m.tick_ns > 0 && m.batch_bytes > 0 && m.boot_ns > 0, "{m:?}");
    assert!(m.measure_text_batches > 0, "the tree's batched measurement goes through the timed measurer: {m:?}");
    assert!(m.prelude_bytecode, "this host's build embeds the prelude as bytecode");
    let fast = h.instance().metrics_fast();
    assert_eq!(fast.memory_used_bytes, 0, "the cheap read skips the heap walk");
    assert!(fast.native_nodes > 0 && fast.native_nodes == m.native_nodes, "{fast:?}");
    assert_eq!(fast.renders, m.renders);
    let json = serde_json::to_value(m).unwrap();
    for key in [
        "renders",
        "render_ns",
        "convert_ns",
        "commit_ns",
        "encode_ns",
        "measure_text_calls",
        "measure_text_ns",
        "measure_text_batches",
        "measure_component_calls",
        "measure_component_ns",
        "tick_ns",
        "batch_bytes",
        "prelude_bytecode",
        "boot_ns",
    ] {
        assert!(json.get(key).is_some(), "{key} missing in {json}");
    }
}
