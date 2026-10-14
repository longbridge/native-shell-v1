//! Timers, tasks and microtasks on the manual clock.

use native_shell::testing::Harness;

const APP: &str = r#"
import { View, div, v_flex } from "native-shell";
export default class T extends View {
  init(_p, cx) {
    this.log = [];
    setTimeout((a, b) => { this.log.push(`timeout${a}${b}`); }, 100, 1, 2);
    this.interval = setInterval(() => { this.log.push("tick"); if (this.log.filter((x) => x === "tick").length === 3) clearInterval(this.interval); }, 50);
    const cancelled = setTimeout(() => this.log.push("never"), 10);
    clearTimeout(cancelled);
    cx.timer.after(200, (cx) => { this.log.push("after"); cx.notify(); });
    this.every = cx.timer.every(1000, (cx) => { this.log.push("every"); cx.notify(); });
    cx.spawn(async (cx) => {
      this.log.push("spawned");
      await cx.sleep(300);
      this.log.push("slept");
      cx.notify();
    });
    queueMicrotask(() => this.log.push("micro"));
  }
  render(cx) {
    return v_flex()
      .child(div().test_id("log").child(this.log.join(",")))
      .child(div().test_id("stop").child("stop").on_click((_e, cx) => { this.every.cancel(); this.log.push(`done=${this.every.is_done()}`); cx.notify(); }));
  }
}
"#;

#[test]
fn timers_and_tasks_follow_the_manual_clock() {
    let mut h = Harness::new(&[("main.js", APP)]);
    assert!(h.load_error().is_none(), "{:?}", h.load_error());
    assert_eq!(h.text_of("log"), "spawned,micro");
    h.advance(100.0);
    h.advance(100.0);
    assert_eq!(h.text_of("log"), "spawned,micro,tick,timeout12,tick,tick,after");
    h.advance(100.0);
    assert_eq!(h.text_of("log"), "spawned,micro,tick,timeout12,tick,tick,after,slept");
    h.advance(700.0);
    assert!(h.text_of("log").ends_with("slept,every"));
    h.tap("stop");
    h.advance(5000.0);
    assert!(h.text_of("log").ends_with("every,done=true"), "{}", h.text_of("log"));
    let now = h.now();
    assert_eq!(h.instance_mut().tick(now), None, "no timer left");
}

#[test]
fn tick_reports_the_delay_to_the_next_timer() {
    let mut h = Harness::new(&[(
        "main.js",
        r#"
import { View, div } from "native-shell";
export default class T extends View { init() { setTimeout(() => {}, 250); } render() { return div(); } }
"#,
    )]);
    let now = h.now();
    assert_eq!(h.instance_mut().tick(now + 50.0), Some(200.0));
}

#[test]
fn timers_of_a_released_view_do_not_fire() {
    let mut h = Harness::new(&[(
        "main.js",
        r#"
import { View, div, v_flex } from "native-shell";
globalThis.fired = 0;
class Child extends View {
  init(_p, cx) { cx.timer.every(10, () => { globalThis.fired += 1; }); }
  render() { return div(); }
}
export default class T extends View {
  init(_p, cx) { this.child = cx.new(Child, {}); }
  render(cx) {
    return v_flex()
      .child(div().test_id("count").child(`${globalThis.fired}`))
      .child(div().test_id("release").child("release").on_click((_e, cx) => { this.child.release(); cx.notify(); }))
      .child(div().test_id("refresh").child("refresh").on_click((_e, cx) => cx.notify()));
  }
}
"#,
    )]);
    h.advance(30.0);
    h.tap("release");
    let before = h.text_of("count");
    h.advance(100.0);
    h.tap("refresh");
    assert_eq!(h.text_of("count"), before);
    assert!(before != "0");
}

#[test]
fn uncaught_async_errors_are_logged_not_fatal() {
    let mut h = Harness::new(&[(
        "main.js",
        r#"
import { View, div } from "native-shell";
export default class T extends View {
  init(_p, cx) { cx.spawn(async () => { throw new Error("task failed"); }); setTimeout(() => { throw new Error("timer failed"); }, 5); }
  render() { return div().test_id("ok").child("ok"); }
}
"#,
    )]);
    h.advance(10.0);
    let logs = h.error_logs().join("\n");
    assert!(logs.contains("task failed") && logs.contains("timer failed"), "{logs}");
    assert_eq!(h.text_of("ok"), "ok");
}

const NESTED: &str = r#"
import { View, div } from "native-shell";
export default class T extends View {
  init(_p, cx) {
    this.ticks = 0;
    this.steps = [];
    cx.timer.after(910, (cx) => {
      cx.timer.every(350, (cx) => { this.ticks += 1; cx.notify(); });
    });
    cx.spawn(async (cx) => {
      for (let i = 0; i < 3; i++) {
        await cx.sleep(100);
        this.steps.push(Date.now());
      }
      cx.notify();
    });
  }
  render() { return div().test_id("out").child(`${this.ticks} ${this.steps.length}`); }
}
"#;

#[test]
fn timers_started_by_timers_fire_within_one_advance() {
    // One jump past several due times sees the same timeline as small steps.
    let mut h = Harness::new(&[("main.js", NESTED)]);
    h.advance(2310.0);
    assert_eq!(h.text_of("out"), "4 3", "every at 1260, 1610, 1960, 2310; sleeps at 100, 200, 300");

    let mut h = Harness::new(&[("main.js", NESTED)]);
    h.advance(910.0);
    h.advance(1400.0);
    assert_eq!(h.text_of("out"), "4 3");
}

const IDLE: &str = r#"
import { View, div, Button } from "native-shell";
export default class T extends View {
  init() { this.fired = false; this.slept = false; }
  render() {
    return div()
      .child(div().test_id("out").child(`${this.fired} ${this.slept}`))
      .child(Button.new("go").test_id("go").label("go").on_click((_e, cx) => {
        cx.timer.after(5000, (cx) => { this.fired = true; cx.notify(); });
        cx.spawn(async (cx) => { await cx.sleep(5000); this.slept = true; cx.notify(); });
      }));
  }
}
"#;

#[test]
fn timers_started_after_an_idle_spell_count_from_when_they_start() {
    let mut h = Harness::new(&[("main.js", IDLE)]);
    // The instance idles for a minute: no ticks, but time passes on the device.
    h.advance_clock(60_000.0);
    h.tap("go");
    h.advance(1000.0);
    assert_eq!(h.text_of("out"), "false false", "5 s timers started by the tap are not due 1 s later");
    h.advance(3999.0);
    assert_eq!(h.text_of("out"), "false false");
    h.advance(1.0);
    assert_eq!(h.text_of("out"), "true true");
}

#[test]
fn a_host_tick_never_moves_time_back() {
    let mut h = Harness::new(&[("main.js", IDLE)]);
    h.advance(1000.0);
    h.tap("go");
    let instance = h.instance_mut();
    assert_eq!(instance.tick(0.0), Some(5000.0), "a tick from the past is taken as now");
    assert_eq!(instance.tick(5999.0), Some(1.0));
}
