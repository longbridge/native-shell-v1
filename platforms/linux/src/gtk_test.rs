//! Runs GTK unit tests on one thread.
//!
//! GTK is initialized once per process and only that thread may use it, while
//! the test runner calls tests from a pool of threads. Every GTK test sends its
//! body here and waits for it.
//!
//! Unless `GDK_BACKEND=broadway` is set, a private `gtk4-broadwayd` is started
//! (and stopped when the test process exits), so no window ever opens on the
//! desktop. Without `gtk4-broadwayd` the tests are skipped.

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

/// Points GDK at a Broadway display; `false` when none can be started.
fn private_display() -> bool {
    if std::env::var("GDK_BACKEND").as_deref() == Ok("broadway") {
        return true;
    }
    let pid = std::process::id();
    // Clear of the displays the integration tests (100+) and people pick.
    let display = 1000 + pid % 800;
    let runtime_dir = std::env::var("XDG_RUNTIME_DIR").unwrap_or_else(|_| std::env::temp_dir().display().to_string());
    let socket = std::path::PathBuf::from(runtime_dir).join(format!("broadway{}.socket", display + 1));
    // A socket left by a killed display would pass for the new one.
    let _ = std::fs::remove_file(&socket);
    // A watchdog stops the display once this process is gone.
    let script = format!(
        "gtk4-broadwayd :{display} --port {port} --address 127.0.0.1 >/dev/null 2>&1 & b=$!; \
         while kill -0 {pid} 2>/dev/null; do sleep 0.5; done; kill $b; rm -f '{socket}'",
        port = 20000 + display,
        socket = socket.display()
    );
    if std::process::Command::new("sh").arg("-c").arg(script).spawn().is_err() {
        return false;
    }
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while !socket.exists() {
        if std::time::Instant::now() > deadline {
            return false;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    std::env::set_var("GDK_BACKEND", "broadway");
    std::env::set_var("BROADWAY_DISPLAY", format!(":{display}"));
    std::env::remove_var("WAYLAND_DISPLAY");
    std::env::remove_var("DISPLAY");
    true
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
