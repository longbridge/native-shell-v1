//! The root node's platform commands: `OpenUrl`, `SetClipboard` and
//! `Haptic`. The root component (`components::overlays`) routes them here;
//! toasts stay with it.

use gtk::gio;
use gtk::prelude::*;
use native_shell_core::protocol::Command;
use serde_json::Value;

/// Runs `command` for the surface `view` belongs to; other commands are ignored.
pub fn run(view: &gtk::Widget, command: Command, payload: &Value) {
    let text = |key: &str| payload.get(key).and_then(Value::as_str).map(str::to_owned);
    match command {
        // Sent only after the core's capability check.
        Command::OpenUrl => {
            let Some(url) = text("url") else { return };
            let window = view.root().and_downcast::<gtk::Window>();
            gtk::UriLauncher::new(&url).launch(window.as_ref(), None::<&gio::Cancellable>, move |result| {
                if let Err(e) = result {
                    gtk::glib::g_warning!("native-shell", "cannot open {url}: {e}");
                }
            });
        }
        Command::SetClipboard => {
            if let Some(t) = text("text") {
                view.clipboard().set_text(&t);
                // A read right after the write sees it, before the clipboard's change notification.
                crate::clipboard::set_text(Some(t));
            }
        }
        // Desktops have no haptic engine to drive.
        Command::Haptic => {}
        _ => {}
    }
}
