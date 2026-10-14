//! Opens a program in a window: `cargo run --example run -- ../../examples/tasks`,
//! or the program `nshell dev` serves, reloaded on every change:
//! `cargo run --example run -- --dev http://192.168.1.20:8080`.
//! The example app's host modules are registered, so `watchlist` runs too.

mod common;

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

use adw::prelude::*;
use native_shell_gtk::{DevClient, Instance, InstanceOptions, Runtime, SurfaceView};

fn main() -> gtk::glib::ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let dev = args.iter().position(|a| a == "--dev").and_then(|i| args.get(i + 1)).cloned();
    let dir = args.first().filter(|a| !a.starts_with("--")).map(PathBuf::from).unwrap_or_else(|| common::examples_dir().join("tasks"));
    let app = adw::Application::builder().application_id("dev.native_shell.Run").build();
    app.connect_startup(|_| common::host_demo::register(Runtime::shared()));
    app.connect_activate(move |app| {
        let window = adw::ApplicationWindow::builder().application(app).default_width(390).default_height(844).build();
        if let Some(url) = &dev {
            match DevClient::connect(url, Runtime::shared()) {
                Ok(client) => {
                    window.set_content(Some(&client.widget()));
                    // Polling stops with the window.
                    let client = RefCell::new(Some(client));
                    window.connect_destroy(move |_| drop(client.take()));
                }
                Err(e) => return eprintln!("{e}"),
            }
            window.present();
            return;
        }
        let surface = SurfaceView::new();
        window.set_content(Some(&surface));
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
