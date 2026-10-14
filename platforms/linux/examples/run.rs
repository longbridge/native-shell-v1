//! Opens a program directory in a window: `cargo run --example run -- ../../examples/tasks`.
//! The example app's host modules are registered, so `watchlist` runs too.

mod common;

use std::path::PathBuf;
use std::rc::Rc;

use adw::prelude::*;
use native_shell_gtk::{Instance, InstanceOptions, Runtime, SurfaceView};

fn main() -> gtk::glib::ExitCode {
    let dir = std::env::args().nth(1).map(PathBuf::from).unwrap_or_else(|| common::examples_dir().join("tasks"));
    let app = adw::Application::builder().application_id("dev.native_shell.Run").build();
    app.connect_startup(|_| common::host_demo::register(Runtime::shared()));
    app.connect_activate(move |app| {
        let surface = SurfaceView::new();
        let window = adw::ApplicationWindow::builder().application(app).default_width(390).default_height(844).content(&surface).build();
        window.present();
        let instance = match Instance::new(&common::app_id(&dir), InstanceOptions::default()) {
            Ok(i) => Rc::new(i),
            Err(e) => return eprintln!("{e}"),
        };
        surface.attach(instance.clone());
        if let Err(e) = instance.load_dir(&dir) {
            eprintln!("{e}");
        }
    });
    app.run_with_args::<&str>(&[])
}
