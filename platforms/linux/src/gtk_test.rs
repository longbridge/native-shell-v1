//! Runs GTK unit tests on one thread.
//!
//! GTK is initialized once per process and only that thread may use it, while
//! the test runner calls tests from a pool of threads. Every GTK test sends its
//! body here and waits for it.
//!
//! Unless `GDK_BACKEND=broadway` is set, a private Broadway display is started
//! (`tests/support/display.rs`, stopped when the test process exits), so no
//! window ever opens on the desktop. Without `gtk4-broadwayd` the tests are skipped.

use std::panic::{catch_unwind, resume_unwind, AssertUnwindSafe};
use std::sync::mpsc;
use std::sync::{Mutex, OnceLock};

type Job = Box<dyn FnOnce() + Send>;
type Outcome = Result<(), Box<dyn std::any::Any + Send>>;

struct Runner {
    jobs: mpsc::Sender<(Job, mpsc::Sender<Outcome>)>,
}

fn runner() -> Option<&'static Mutex<Runner>> {
    static RUNNER: OnceLock<Option<Mutex<Runner>>> = OnceLock::new();
    RUNNER
        .get_or_init(|| {
            let (ready_tx, ready_rx) = mpsc::channel();
            let (jobs_tx, jobs_rx) = mpsc::channel::<(Job, mpsc::Sender<Outcome>)>();
            std::thread::spawn(move || {
                if !private_display() || gtk::init().is_err() {
                    let _ = ready_tx.send(false);
                    return;
                }
                adw::init().ok();
                let _ = ready_tx.send(true);
                while let Ok((job, done)) = jobs_rx.recv() {
                    let _ = done.send(catch_unwind(AssertUnwindSafe(job)));
                }
            });
            ready_rx.recv().unwrap_or(false).then(|| Mutex::new(Runner { jobs: jobs_tx }))
        })
        .as_ref()
}

#[path = "../tests/support/display.rs"]
mod display;

/// Points GDK at a Broadway display; `false` when none can be started. The
/// display lives as long as the process (its watchdog stops it then).
fn private_display() -> bool {
    match display::start() {
        Some(Some(d)) => {
            std::mem::forget(d);
            true
        }
        Some(None) => true,
        None => false,
    }
}

/// Runs `f` on the GTK thread; returns `false` (and runs nothing) without a display.
pub fn run(f: impl FnOnce() + Send + 'static) -> bool {
    let Some(runner) = runner() else {
        eprintln!("skipped: GTK cannot start (is gtk4-broadwayd installed?)");
        return false;
    };
    let (done_tx, done_rx) = mpsc::channel();
    // One test at a time: they share the GTK settings and the main context.
    let runner = runner.lock().unwrap_or_else(|e| e.into_inner());
    if runner.jobs.send((Box::new(f), done_tx)).is_err() {
        panic!("the GTK test thread is gone");
    }
    match done_rx.recv() {
        Ok(Ok(())) => true,
        Ok(Err(panic)) => resume_unwind(panic),
        Err(_) => panic!("the GTK test thread is gone"),
    }
}

/// Runs the main context until `done` holds or `timeout_ms` passes.
pub fn wait_until(timeout_ms: u64, mut done: impl FnMut() -> bool) -> bool {
    let deadline = std::time::Instant::now() + std::time::Duration::from_millis(timeout_ms);
    let context = gtk::glib::MainContext::default();
    while !done() {
        if std::time::Instant::now() > deadline {
            return false;
        }
        while context.iteration(false) {}
        std::thread::sleep(std::time::Duration::from_millis(4));
    }
    true
}

/// Runs everything pending on the main context.
pub fn flush() {
    let context = gtk::glib::MainContext::default();
    while context.iteration(false) {}
}
