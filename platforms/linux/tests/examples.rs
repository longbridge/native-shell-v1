//! The repository's example programs on GTK, with the example app's host
//! modules, and the engine's own failure surfaces.

#[path = "../examples/common/mod.rs"]
mod common;
mod support;

use std::process::ExitCode;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use native_shell_gtk::{InstanceOptions, Runtime};
use support::*;

fn main() -> ExitCode {
    run(&[
        ("every example loads with its assets", every_example_loads_with_its_assets),
        ("watchlist shows live quotes in the host cell", watchlist_shows_live_quotes_in_the_host_cell),
        ("screenshots repeat byte for byte", screenshots_repeat_byte_for_byte),
        ("a failing first render shows the error page", a_failing_first_render_shows_the_error_page),
        ("a failing later render shows the error banner", a_failing_later_render_shows_the_error_banner),
    ])
}

fn runtime() -> Runtime {
    let runtime = Runtime::new();
    common::host_demo::register(&runtime);
    runtime
}

fn open(runtime: &Runtime, name: &str) -> Fixture {
    let dir = common::examples_dir().join(name);
    Fixture::start_with(runtime, InstanceOptions::default(), |instance| instance.load_dir(&dir))
}

fn every_example_loads_with_its_assets() {
    let runtime = runtime();
    let errors = Arc::new(Mutex::new(Vec::new()));
    let log = errors.clone();
    runtime.set_logger(move |level, message| {
        if matches!(level, native_shell_gtk::LogLevel::Warn | native_shell_gtk::LogLevel::Error) {
            log.lock().unwrap().push(message.to_string());
        }
    });
    for example in common::examples() {
        let f = open(&runtime, example.name);
        assert!(f.find("nsh-error-page").is_none() && f.find("nsh-error-banner").is_none(), "{} failed to render", example.name);
        assert!(f.surface.mounted_node_count() > 10, "{} rendered almost nothing", example.name);
    }
    let errors = errors.lock().unwrap();
    assert!(errors.is_empty(), "examples logged warnings: {errors:#?}");
}

fn watchlist_shows_live_quotes_in_the_host_cell() {
    let f = open(&runtime(), "watchlist");
    f.wait_for("cell-HALO.US");
    let priced = || {
        f.find_where(|w| w.has_css_class("demo-stock-cell")).is_some_and(|cell| {
            let mut texts = Vec::new();
            let mut child = cell.first_child().and_then(|grid| grid.first_child());
            while let Some(w) = child {
                if let Some(label) = w.downcast_ref::<gtk::Label>() {
                    texts.push(label.text().to_string());
                }
                child = w.next_sibling();
            }
            texts.iter().any(|t| t.contains('%'))
        })
    };
    assert!(pump_until(Duration::from_secs(5), priced), "quotes never reached the cells");
    let cell = f.expect("cell-HALO.US");
    assert!(f.bounds(&cell).height() >= 64.0, "the host measures its cell");
}

fn a_failing_first_render_shows_the_error_page() {
    let f = Fixture::start_with(&Runtime::new(), InstanceOptions::default(), |instance| {
        let main = r#"
import { View } from "native-shell";
export default class App extends View { render() { throw new Error("no data"); } }"#;
        let result = instance.load_sources(MANIFEST, [("main.js".to_string(), main.to_string())].into());
        assert!(result.is_err(), "a failing first render fails the load");
        Ok(())
    });
    f.expect("nsh-error-page");
    f.screenshot("target/agents/linux-app/error-page.png");
}

fn a_failing_later_render_shows_the_error_banner() {
    let f = Fixture::new(
        r#"
import { View, div } from "native-shell";
export default class App extends View {
  init() { this.broken = false; }
  render() {
    if (this.broken) throw new Error("lost the connection");
    return div().size_full().test_id("ok").on_click((_e, cx) => { this.broken = true; cx.notify(); });
  }
}"#,
    );
    f.tap("ok");
    f.expect("nsh-error-banner");
    f.expect("ok");
    f.screenshot("target/agents/linux-app/error-banner.png");
}

/// Two fresh runs of an example whose screen shows dates (tasks: due dates
/// relative to today) give identical images: the fixture pins the wall clock,
/// time zone, language and text size, and captures hold animations still.
fn screenshots_repeat_byte_for_byte() {
    let shot = |n: u32| {
        let f = open(&runtime(), "tasks");
        pump_for(Duration::from_millis(200));
        let path = f.screenshot(format!("target/agents/linux-app/repeat-{n}.png"));
        std::fs::read(path).unwrap()
    };
    let (first, second) = (shot(1), shot(2));
    assert!(first == second, "two runs of the tasks example differ");
}
