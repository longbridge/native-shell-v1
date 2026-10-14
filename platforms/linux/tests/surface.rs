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
        ("the frame clock runs at full rate", the_frame_clock_runs_at_full_rate),
        ("the clipboard round-trips", the_clipboard_round_trips),
        ("click and long press reach the right gesture", click_and_long_press_reach_the_right_gesture),
        ("a node that appears later can be clicked at once", a_node_that_appears_later_can_be_clicked_at_once),
        ("a tall surface settles while something animates", a_tall_surface_settles_while_something_animates),
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

/// The display's client acknowledges frames, so GTK is not throttled to 1 fps.
fn the_frame_clock_runs_at_full_rate() {
    let f = Fixture::new(r#"import { View, div } from "native-shell"; export default class A extends View { render() { return div(); } }"#);
    let ticks = Counter::default();
    let counter = ticks.clone();
    f.surface.add_tick_callback(move |_, _| {
        counter.bump();
        gtk::glib::ControlFlow::Continue
    });
    pump_for(Duration::from_millis(500));
    assert!(ticks.get() >= 10, "{} frames in 0.5 s", ticks.get());
}

fn the_clipboard_round_trips() {
    let f = Fixture::new(
        r#"
import { View, div, v_flex } from "native-shell";
export default class App extends View {
  init() { this.read = "none"; }
  render() {
    return v_flex()
      .child(div().test_id("copy").h(40).on_click((_e, cx) => cx.write_to_clipboard("Gate code 4521")))
      .child(div().test_id("paste").h(40).on_click((_e, cx) => { this.read = cx.read_from_clipboard() ?? "empty"; cx.notify(); }))
      .child(div().test_id(`read-${this.read}`).h(10));
  }
}"#,
    );
    f.tap("copy");
    f.tap("paste");
    f.expect("read-Gate code 4521");
    let clipboard = f.surface.clipboard();
    let text = std::rc::Rc::new(std::cell::RefCell::new(None));
    let slot = text.clone();
    clipboard.read_text_async(None::<&gtk::gio::Cancellable>, move |r| *slot.borrow_mut() = r.ok().flatten().map(|s| s.to_string()));
    pump_until(Duration::from_secs(2), || text.borrow().is_some());
    assert_eq!(text.borrow().as_deref(), Some("Gate code 4521"), "the desktop clipboard has it too");
}

/// A row with both a click and a long press (a secondary click on desktops):
/// the click helper must not take the secondary-button gesture.
fn click_and_long_press_reach_the_right_gesture() {
    let f = Fixture::new(
        r#"
import { View, div } from "native-shell";
export default class App extends View {
  init() { this.log = []; }
  render() {
    return div().size_full()
      .child(div().test_id("row").h(44)
        .on_click((_e, cx) => { this.log.push("click"); cx.notify(); })
        .on_long_press((_e, cx) => { this.log.push("long"); cx.notify(); }))
      .child(div().test_id(`log-${this.log.join("-")}`).h(10));
  }
}"#,
    );
    f.click("row");
    f.expect("log-click");
    f.long_press("row");
    f.expect("log-click-long");
}

/// `wait_for` hands back a laid-out widget even when its batch arrived during the wait.
fn a_node_that_appears_later_can_be_clicked_at_once() {
    let f = Fixture::new(
        r#"
import { View, div, Button } from "native-shell";
export default class App extends View {
  init(_p, cx) { this.ready = false; this.clicked = false; setTimeout(() => { this.ready = true; cx.notify(); }, 700); }
  render() {
    return div().size_full()
      .child(this.ready ? Button.new("late").test_id("late").label("Continue").on_click((_e, cx) => { this.clicked = true; cx.notify(); }) : null)
      .child(div().test_id(this.clicked ? "clicked" : "waiting").h(10));
  }
}"#,
    );
    let late = f.wait_for("late");
    assert!(late.width() > 0 && late.height() > 0, "allocated: {}x{}", late.width(), late.height());
    f.click("late");
    f.expect("clicked");
}

/// Taller than a small test screen, with a spinner keeping GTK laying out
/// every frame: the window must not be clamped below the fixture's size, or
/// the environment flips between two heights and batches never stop.
fn a_tall_surface_settles_while_something_animates() {
    let f = Fixture::open_dir("../../examples/components");
    f.resize(390, 2600);
    pump_for(Duration::from_millis(300));
    f.tap("open-status");
    f.settle();
    assert_eq!(f.surface.environment().height, 2600.0);
}
