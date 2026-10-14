//! Opens one program in a window.
//!
//! * `cargo linux-run examples/tasks` (from the repository root; relative
//!   paths resolve from the current directory)
//! * `cargo linux-run examples/tasks --watch`: reloads in place whenever a
//!   file of the program changes
//! * `cargo linux-run --dev http://192.168.1.20:8080`: the program
//!   `nshell dev` serves, reloaded on every change
//!
//! The example app's host modules are registered, so `watchlist` runs too.

mod common;

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::rc::Rc;

use adw::prelude::*;
use native_shell_gtk::{DevClient, Instance, InstanceOptions, Runtime, SurfaceView};

/// What to run.
enum Source {
    Dir { dir: PathBuf, watch: bool },
    Dev(String),
}

fn parse_args() -> Result<Source, String> {
    let mut args = std::env::args().skip(1);
    let (mut dir, mut watch, mut dev) = (None, false, None);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--watch" => watch = true,
            "--dev" => dev = Some(args.next().ok_or("--dev needs the URL `nshell dev` printed")?),
            flag if flag.starts_with("--") => return Err(format!("unknown option {flag}")),
            path => dir = Some(PathBuf::from(path)),
        }
    }
    if let Some(url) = dev {
        return Ok(Source::Dev(url));
    }
    let dir = dir.unwrap_or_else(|| common::examples_dir().join("tasks"));
    let dir = std::path::absolute(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    if !dir.join("native-shell.json").is_file() {
        return Err(format!("{} is not a program: it has no native-shell.json", dir.display()));
    }
    Ok(Source::Dir { dir, watch })
}

fn main() -> ExitCode {
    let source = match parse_args() {
        Ok(s) => s,
        Err(e) => {
            eprintln!("error: {e}\nusage: run [<program dir>] [--watch] | run --dev http://HOST:PORT");
            return ExitCode::from(2);
        }
    };
    let app = adw::Application::builder().application_id("dev.native_shell.Run").build();
    app.connect_startup(|_| common::host_demo::register(Runtime::shared()));
    app.connect_activate(move |app| {
        let window = adw::ApplicationWindow::builder().application(app).default_width(390).default_height(844).build();
        let client = match &source {
            Source::Dev(url) => DevClient::connect(url, Runtime::shared()),
            Source::Dir { dir, watch: true } => DevClient::watch(dir, Runtime::shared()),
            Source::Dir { dir, watch: false } => return open(&window, dir),
        };
        match client {
            Ok(client) => {
                window.set_content(Some(&client.widget()));
                // Polling stops with the window.
                let client = RefCell::new(Some(client));
                window.connect_destroy(move |_| drop(client.take()));
                window.present();
            }
            Err(e) => eprintln!("error: {e}"),
        }
    });
    if app.run_with_args::<&str>(&[]) == gtk::glib::ExitCode::SUCCESS {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

fn open(window: &adw::ApplicationWindow, dir: &Path) {
    let surface = SurfaceView::new();
    window.set_content(Some(&surface));
    window.present();
    let instance = match Instance::new(&common::app_id(dir), InstanceOptions::default()) {
        Ok(i) => Rc::new(i),
        Err(e) => return eprintln!("error: {e}"),
    };
    surface.attach(instance.clone());
    // A program that fails to start shows the engine's error page.
    if let Err(e) = instance.load_dir(dir) {
        eprintln!("error: {e}");
    }
}
