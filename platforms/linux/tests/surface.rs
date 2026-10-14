//! The surface end to end: batches applied, events back, environment, screenshots.

mod support;

use std::process::ExitCode;
use std::time::Duration;

use support::*;

fn main() -> ExitCode {
    run(&[
        ("renders and handles a tap", renders_and_handles_a_tap),
        ("environment follows size and appearance", environment_follows_size_and_appearance),
        ("paints backgrounds at their frames", paints_backgrounds_at_their_frames),
        ("timers pause while the surface is hidden", timers_pause_while_the_surface_is_hidden),
    ])
}

fn renders_and_handles_a_tap() {
    let f = Fixture::new(
        r#"
import { View, div, v_flex } from "native-shell";
export default class App extends View {
  init() { this.n = 0; }
  render() {
    return v_flex().size_full()
      .child(div().test_id("inc").h(44).on_click((_e, cx) => { this.n += 1; cx.notify(); }))
      .child(div().test_id(`count-${this.n}`).h(20));
  }
}"#,
    );
    f.expect("count-0");
    f.tap("inc");
    f.expect("count-1");
    assert!(f.find("count-0").is_none());
}

fn environment_follows_size_and_appearance() {
    let f = Fixture::new(
        r#"
import { View, div } from "native-shell";
export default class App extends View {
  render() {
    const { width, height } = window.viewport_size();
    return div().size_full().test_id(`env-${window.appearance()}-${width}x${height}`);
  }
}"#,
    );
    f.expect(&format!("env-light-{WIDTH}x{HEIGHT}"));
    f.set_dark(true);
    f.expect(&format!("env-dark-{WIDTH}x{HEIGHT}"));
    f.resize(320, 600);
    f.expect("env-dark-320x600");
}

fn paints_backgrounds_at_their_frames() {
    let f = Fixture::new(
        r##"
import { View, div, v_flex } from "native-shell";
export default class App extends View {
  render() {
    return v_flex().size_full().bg("#ffffff")
      .child(div().test_id("red").w(100).h(50).bg("#ff0000"));
  }
}"##,
    );
    assert_eq!(f.pixel(10, 10), 0xffff0000);
    assert_eq!(f.pixel(150, 10), 0xffffffff);
    assert_eq!(f.pixel(10, 70), 0xffffffff);
    let path = f.screenshot("target/agents/linux-app/surface.png");
    assert!(path.exists());
}

fn timers_pause_while_the_surface_is_hidden() {
    let f = Fixture::new(
        r#"
import { View, div } from "native-shell";
export default class App extends View {
  init(_p, cx) { this.ticks = 0; setInterval(() => { this.ticks += 1; cx.notify(); }, 20); }
  render() { return div().size_full().test_id(this.ticks > 0 ? "ticking" : "idle"); }
}"#,
    );
    f.wait_for("ticking");
    f.window.set_visible(false);
    assert!(!f.surface.is_mapped());
    f.settle();
    let paused = f.instance.metrics().unwrap().timers_fired;
    pump_for(Duration::from_millis(200));
    assert_eq!(f.instance.metrics().unwrap().timers_fired, paused, "no timer fires while hidden");
    f.window.set_visible(true);
    pump_until(Duration::from_secs(2), || f.instance.metrics().unwrap().timers_fired > paused);
    assert!(f.instance.metrics().unwrap().timers_fired > paused, "timers resume when shown");
}
