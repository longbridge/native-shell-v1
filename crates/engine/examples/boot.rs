//! Instance creation and first frame of a trivial program.
//!
//! `cargo run --release -p native-shell --example boot`
use std::time::Instant;

use native_shell::testing::Harness;

fn main() {
    let mut times = Vec::new();
    for _ in 0..10 {
        let t = Instant::now();
        let h = Harness::new(&[(
            "main.js",
            r#"import { View, div } from "gpui-kit"; export default class B extends View { render() { return div().child("hi"); } }"#,
        )]);
        times.push(t.elapsed().as_secs_f64() * 1000.0);
        assert!(h.load_error().is_none());
    }
    times.sort_by(f64::total_cmp);
    println!("boot + first frame: median {:.2} ms, min {:.2} ms", times[5], times[0]);
}
