//! The components example's data and navigation pages on GTK (an order's
//! details, a project's files, a cabin booking, a photo album, notification
//! settings), opened from the index as a reader would, with their light and
//! dark screenshots.

mod support;

use std::process::ExitCode;
use std::time::Duration;

use support::{example_runtime, pump_for, run, Fixture};

fn main() -> ExitCode {
    run(&[
        ("components example: description list page", order_page),
        ("components example: tree page", files_page),
        ("components example: calendar page", booking_page),
        ("components example: carousel page", photos_page),
        ("components example: settings page", settings_page),
    ])
}

/// The components example at its index, with the example apps' theme.
fn components() -> Fixture {
    gtk::Settings::default().expect("settings").set_gtk_enable_animations(false);
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/components");
    let f = Fixture::start_with(&example_runtime(), Default::default(), |i| i.load_dir(&dir));
    f.wait_for("index");
    f
}

/// Pushes the page behind `open-<id>` and waits for `ready` on it.
fn open(id: &str, ready: &str) -> Fixture {
    let f = components();
    f.tap(&format!("open-{id}"));
    pump_for(Duration::from_millis(400));
    f.wait_for(ready);
    f
}

/// Light and dark screenshots of what `f` shows, into `screenshots/`.
fn screenshots(f: &Fixture, name: &str) {
    f.set_dark(false);
    f.screenshot(format!("screenshots/components-{name}-light.png"));
    f.set_dark(true);
    f.screenshot(format!("screenshots/components-{name}-dark.png"));
    f.set_dark(false);
}

fn order_page() {
    // The order loads first, then reads like a receipt.
    let f = open("order", "total");
    f.expect("order-details");
    f.expect("delivery");
    assert!(f.find("order-loading").is_none());
    screenshots(&f, "order");
}

fn files_page() {
    let f = open("files", "tree");
    f.expect("details");
    f.expect("path");
    screenshots(&f, "files");
}

/// `YYYY-MM-DD`, `days` from today.
fn date(days: i32) -> String {
    let d = gtk::glib::DateTime::now_local().expect("now").add_days(days).expect("date");
    format!("{:04}-{:02}-{:02}", d.year(), d.month(), d.day_of_month())
}

fn booking_page() {
    let f = open("booking", "calendar");
    f.expect("summary-prompt");
    // A stay a week out, clear of the booked nights (3–5 and 12–13 days out),
    // stepping to the next month when a date falls there.
    for day in [date(7), date(10)] {
        let cell = format!("calendar-{day}");
        if f.find(&cell).is_none() {
            f.tap("calendar-next");
        }
        f.tap(&cell);
    }
    f.expect("total");
    screenshots(&f, "booking");
}

fn photos_page() {
    let f = open("photos", "carousel");
    f.expect("caption");
    screenshots(&f, "photos");
    // Favorites holds one photo; unstarring it leaves the empty state.
    f.tap("album-favorites");
    f.tap("favorite");
    f.expect("no-favorites");
    screenshots(&f, "photos-empty");
}

fn settings_page() {
    let f = open("notifications", "settings");
    f.expect("allow");
    f.expect("previews");
    screenshots(&f, "settings");
}
