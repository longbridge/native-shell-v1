//! The clipboard text scripts read synchronously (`nsh/clipboard` `read`).
//!
//! GTK reads the clipboard asynchronously and only on the main thread, while
//! the core asks on the JS thread and needs the answer before the call
//! returns. So the main thread keeps a copy of the text current: read once
//! when the first instance starts, and again whenever the clipboard changes.

use std::sync::{Mutex, OnceLock};

use gtk::gdk;
use gtk::prelude::*;

static TEXT: OnceLock<Mutex<Option<String>>> = OnceLock::new();

fn cache() -> &'static Mutex<Option<String>> {
    TEXT.get_or_init(Mutex::default)
}

/// The clipboard's text as of its last change; any thread.
pub fn text() -> Option<String> {
    cache().lock().unwrap_or_else(|p| p.into_inner()).clone()
}

/// Replaces the copy right away (the clipboard reports its own change later).
pub fn set_text(text: Option<String>) {
    *cache().lock().unwrap_or_else(|p| p.into_inner()) = text;
}

/// Starts following the default display's clipboard; main thread, idempotent.
pub fn watch() {
    thread_local! {
        static WATCHING: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    }
    if WATCHING.with(|w| w.replace(true)) {
        return;
    }
    let Some(display) = gdk::Display::default() else { return };
    let clipboard = display.clipboard();
    clipboard.connect_changed(refresh);
    refresh(&clipboard);
}

fn refresh(clipboard: &gdk::Clipboard) {
    clipboard.read_text_async(None::<&gtk::gio::Cancellable>, |result| {
        set_text(result.ok().flatten().map(|s| s.to_string()));
    });
}
