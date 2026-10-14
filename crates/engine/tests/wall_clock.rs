//! `InstanceConfig::wall_clock`: a pinned `Date` for tests and screenshots.

use native_shell::testing::{Harness, WALL_CLOCK};

const APP: &str = r#"
import { View, div, v_flex } from "native-shell";
export default class T extends View {
  init(_p, cx) {
    this.log = [];
    const start = Date.now();
    this.log.push(`start=${start}`, `ctor=${new Date().getTime() === start}`);
    setTimeout(() => { this.log.push(`after=${Date.now() - start}`, `new=${new Date().getTime() - start}`); cx.notify(); }, 250);
  }
  render() {
    const d = new Date(2001, 0, 2, 3, 4, 5);
    class Later extends Date {}
    const checks = [
      `args=${d.getFullYear()}-${d.getMonth()}-${d.getDate()}T${d.getHours()}:${d.getMinutes()}`,
      `parse=${Date.parse("2001-01-02T00:00:00Z")}`,
      `utc=${Date.UTC(2001, 0, 2)}`,
      `instanceof=${new Date() instanceof Date && d instanceof Date}`,
      `proto=${Object.getPrototypeOf(new Date()) === Date.prototype && d.constructor === Date}`,
      `sub=${new Later() instanceof Later && new Later() instanceof Date}`,
      `string=${typeof Date() === "string"}`,
      `iso=${new Date().toISOString().slice(0, 16)}`,
    ];
    return v_flex()
      .child(div().test_id("log").child(this.log.join(",")))
      .child(div().test_id("checks").child(checks.join(",")));
  }
}
"#;

#[test]
fn date_starts_at_the_pinned_instant_and_follows_timers() {
    let mut h = Harness::new(&[("main.js", APP)]);
    assert!(h.load_error().is_none(), "{:?}", h.load_error());
    assert_eq!(h.text_of("log"), format!("start={},ctor=true", WALL_CLOCK as u64));
    h.advance(250.0);
    assert_eq!(h.text_of("log"), format!("start={},ctor=true,after=250,new=250", WALL_CLOCK as u64));
}

#[test]
fn the_rest_of_date_is_the_builtin() {
    let h = Harness::new(&[("main.js", APP)]);
    let checks = h.text_of("checks");
    for expected in [
        "args=2001-0-2T3:4",
        "parse=978393600000",
        "utc=978393600000",
        "instanceof=true",
        "proto=true",
        "sub=true",
        "string=true",
    ] {
        assert!(checks.contains(expected), "{expected} missing in {checks}");
    }
    assert!(checks.contains("iso=2026-10-14T09:4"), "{checks}");
}

#[test]
fn without_a_pin_date_is_the_system_clock() {
    let h = Harness::build(&[("main.js", APP)]).wall_clock(None).start();
    let log = h.text_of("log");
    let start: f64 = log.trim_start_matches("start=").split(',').next().unwrap().parse().unwrap();
    let system = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() as f64;
    assert!((system - start).abs() < 60_000.0, "{start} vs {system}");
}
