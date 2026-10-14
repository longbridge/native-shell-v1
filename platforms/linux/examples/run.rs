//! Opens a program directory in a window: `cargo run --example run -- ../../examples/tasks`.

use std::path::PathBuf;
use std::rc::Rc;

use adw::prelude::*;
use native_shell_gtk::{Instance, InstanceOptions, SurfaceView};

fn main() -> gtk::glib::ExitCode {
    let dir = std::env::args().nth(1).map(PathBuf::from).unwrap_or_else(|| PathBuf::from("../../examples/tasks"));
    let app = adw::Application::builder().application_id("org.example.NativeShell").build();
    app.connect_activate(move |app| {
        let surface = SurfaceView::new();
        let window = adw::ApplicationWindow::builder().application(app).default_width(390).default_height(844).content(&surface).build();
        window.present();
        let instance = match Instance::new("example", InstanceOptions::default()) {
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
