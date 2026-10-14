//! Renders example programs headlessly into `screenshots/<example>-{light,dark}.png`,
//! at phone size, with empty storage and the example app's host modules.
//!
//! `cargo run --example screenshot` renders every example; name some to
//! render only those (`-- tasks chat`). Runs on a private Broadway display,
//! like the tests, so no window ever opens on the desktop.

mod common;
#[path = "../tests/support/mod.rs"]
mod support;

use std::path::Path;
use std::process::ExitCode;
use std::time::Duration;

use native_shell_gtk::{InstanceOptions, Runtime};
use support::{pump_for, Fixture};

fn main() -> ExitCode {
    let wanted: Vec<String> = std::env::args().skip(1).collect();
    let _display = support::init();
    common::host_demo::register(Runtime::shared());
    let out = Path::new(env!("CARGO_MANIFEST_DIR")).join("screenshots");
    let mut failed = false;
    for example in common::examples().filter(|e| wanted.is_empty() || wanted.iter().any(|w| w == e.name)) {
        let dir = common::examples_dir().join(example.name);
        for dark in [false, true] {
            support::set_dark(dark);
            let f = Fixture::start_with(Runtime::shared(), InstanceOptions::default(), |instance| instance.load_dir(&dir));
            // Host answers and first ticks arrive on timers.
            pump_for(Duration::from_millis(800));
            f.settle();
            let path = out.join(format!("{}-{}.png", example.name, if dark { "dark" } else { "light" }));
            f.screenshot(&path);
            println!("{}", path.display());
            failed |= !path.exists();
        }
    }
    if failed {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}
