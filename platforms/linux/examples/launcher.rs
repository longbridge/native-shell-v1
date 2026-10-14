//! The example app: a launcher listing the repository's example programs. A
//! program opens on its own page, pushed on the navigation stack; the back
//! button, Escape, Alt+Left or a swipe returns to the list.
//!
//! `cargo run --example launcher` (in `platforms/linux`); `-- --example tasks`
//! opens one directly and `--reset` starts it with empty storage.
//! `--screenshot <dir>` renders the list headlessly into
//! `<dir>/launcher-{light,dark}.png` and exits.

mod common;
#[path = "../tests/support/mod.rs"]
mod support;

use std::path::Path;
use std::rc::Rc;

use adw::prelude::*;
use common::{Example, SECTIONS};
use gtk::glib;
use native_shell_gtk::{default_data_dir, Instance, InstanceOptions, Runtime, SurfaceView};

fn main() -> glib::ExitCode {
    let args: Vec<String> = std::env::args().collect();
    let open = args.iter().position(|a| a == "--example").and_then(|i| args.get(i + 1)).cloned();
    let reset = args.iter().any(|a| a == "--reset");
    if let Some(i) = args.iter().position(|a| a == "--screenshot") {
        return screenshot(args.get(i + 1).map_or(Path::new("screenshots"), |d| Path::new(d)));
    }

    let app = adw::Application::builder().application_id("dev.native_shell.Examples").build();
    app.connect_startup(|_| {
        common::host_demo::register(Runtime::shared());
        install_css();
    });
    app.connect_activate(move |app| {
        let navigation = adw::NavigationView::new();
        navigation.add(&list_page(&navigation, reset));
        let window = adw::ApplicationWindow::builder()
            .application(app)
            .title("Examples")
            .default_width(420)
            .default_height(860)
            .content(&navigation)
            .build();
        window.present();
        if let Some(name) = &open {
            if let Some(example) = common::examples().find(|e| e.name == name) {
                open_example(&navigation, example, reset);
            }
        }
    });
    app.run_with_args::<&str>(&[])
}

/// Inset grouped sections of examples, as Settings lists its panes.
fn list_page(navigation: &adw::NavigationView, reset: bool) -> adw::NavigationPage {
    let page = adw::PreferencesPage::new();
    for (title, examples) in SECTIONS {
        let group = adw::PreferencesGroup::builder().title(title).build();
        for example in examples {
            let row = adw::ActionRow::builder().title(example.title).subtitle(example.summary).activatable(true).build();
            row.add_prefix(&tile(example));
            row.add_suffix(&gtk::Image::from_icon_name("go-next-symbolic"));
            row.set_widget_name(&format!("example-{}", example.name));
            let navigation = navigation.clone();
            row.connect_activated(move |_| open_example(&navigation, example, reset));
            group.add(&row);
        }
        page.add(&group);
    }
    let toolbar = adw::ToolbarView::new();
    toolbar.add_top_bar(&adw::HeaderBar::new());
    toolbar.set_content(Some(&page));
    adw::NavigationPage::builder().title("Examples").child(&toolbar).build()
}

/// The example's symbol, white on a rounded tile of its color.
fn tile(example: &Example) -> gtk::Widget {
    let tile = gtk::Image::from_icon_name(example.icon);
    tile.set_pixel_size(18);
    tile.set_size_request(32, 32);
    tile.set_valign(gtk::Align::Center);
    tile.add_css_class("example-tile");
    tile.add_css_class(&format!("tile-{}", example.name));
    tile.upcast()
}

fn install_css() {
    let mut css = String::from(".example-tile { border-radius: 8px; color: white; }\n");
    for example in common::examples() {
        css.push_str(&format!(".tile-{} {{ background: {}; }}\n", example.name, example.tint));
    }
    let provider = gtk::CssProvider::new();
    provider.load_from_string(&css);
    if let Some(display) = gtk::gdk::Display::default() {
        gtk::style_context_add_provider_for_display(&display, &provider, gtk::STYLE_PROVIDER_PRIORITY_APPLICATION);
    }
}

/// Runs the example on a new page; the instance lives as long as the page.
fn open_example(navigation: &adw::NavigationView, example: &Example, reset: bool) {
    let dir = common::examples_dir().join(example.name);
    let app_id = common::app_id(&dir);
    let data_dir = default_data_dir(&app_id);
    if reset {
        let _ = std::fs::remove_dir_all(&data_dir);
    }
    let options = InstanceOptions { data_dir: Some(data_dir), ..InstanceOptions::default() };
    let instance = match Instance::new(&app_id, options) {
        Ok(i) => Rc::new(i),
        Err(e) => return show_error(navigation, example, &e),
    };
    let title = example.title;
    instance.connect_error(move |message| glib::g_warning!("examples", "{title}: {message}"));
    let surface = SurfaceView::new();
    surface.attach(instance.clone());
    // The engine shows load failures in the surface itself; the dialog adds nothing then.
    if let Err(e) = instance.load_dir(&dir) {
        glib::g_warning!("examples", "{}: {e}", example.title);
    }
    let toolbar = adw::ToolbarView::new();
    toolbar.add_top_bar(&adw::HeaderBar::new());
    toolbar.set_content(Some(&surface));
    let page = adw::NavigationPage::builder().title(example.title).tag(example.name).child(&toolbar).build();
    navigation.push(&page);
}

fn show_error(navigation: &adw::NavigationView, example: &Example, message: &str) {
    let dialog = adw::AlertDialog::new(Some(&format!("Couldn’t Open “{}”", example.title)), Some(message));
    dialog.add_response("ok", "OK");
    dialog.present(Some(navigation));
}

/// Renders the list page in both appearances, at the launcher's default size.
fn screenshot(dir: &Path) -> glib::ExitCode {
    let _display = support::init();
    install_css();
    for appearance in ["light", "dark"] {
        support::set_dark(appearance == "dark");
        let navigation = adw::NavigationView::new();
        navigation.add(&list_page(&navigation, false));
        // The window's background, which a window paints behind its content.
        let page = adw::Bin::builder().child(&navigation).css_classes(["background"]).build();
        let window = gtk::Window::builder().decorated(false).child(&page).build();
        window.present();
        support::pump_for(std::time::Duration::from_millis(300));
        // Broadway lays the window out once, within its 768-point-high screen;
        // lay the page out at the launcher's size like the tests do.
        page.measure(gtk::Orientation::Horizontal, -1);
        page.measure(gtk::Orientation::Vertical, 420);
        page.allocate(420, 860, -1, None);
        let path = dir.join(format!("launcher-{appearance}.png"));
        support::screenshot(page.upcast_ref(), &path);
        println!("{}", path.display());
        window.destroy();
    }
    glib::ExitCode::SUCCESS
}
