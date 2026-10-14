//! The events example on GTK: its main flow from the first load to a ticket,
//! with screenshots of every screen and state in both appearances.

mod support;

use std::process::ExitCode;
use std::time::Duration;

use native_shell_core::protocol::EventType;
use support::*;

fn main() -> ExitCode {
    run(&[
        ("events: discover, calendar, event, booking, ticket", events_flow),
        ("events: the largest accessibility text size", events_large_text),
    ])
}

fn shot(f: &Fixture, name: &str, dark: bool) {
    f.screenshot(format!("screenshots/events-{name}-{}.png", if dark { "dark" } else { "light" }));
}

/// Types into the native field of `test_id`, as the keyboard would.
fn type_into(f: &Fixture, test_id: &str, text: &str) {
    let root = f.expect(test_id);
    let editable = descendants(&root)
        .into_iter()
        .find_map(|w| w.downcast::<gtk::Text>().ok())
        .unwrap_or_else(|| panic!("no text field in {test_id:?}"));
    editable.set_text(text);
    f.settle();
}

fn descendants(w: &gtk::Widget) -> Vec<gtk::Widget> {
    let mut out = vec![w.clone()];
    let mut child = w.first_child();
    while let Some(c) = child {
        out.extend(descendants(&c));
        child = c.next_sibling();
    }
    out
}

/// `cal-YYYY-MM-DD` for today plus `offset` days, as the calendar names its days.
fn day_id(offset: i32) -> String {
    let date = gtk::glib::DateTime::now_local().expect("local time").add_days(offset).expect("date");
    format!("cal-{}", date.format("%Y-%m-%d").expect("format"))
}

fn events_flow() {
    if let Some(settings) = gtk::Settings::default() {
        settings.set_gtk_enable_animations(false);
    }
    for dark in [false, true] {
        set_dark(dark);
        let f = Fixture::open_dir("../../examples/events");
        // The first load shows the screen's shape in placeholders.
        f.expect("loading");
        shot(&f, "loading", dark);
        f.wait_for("featured-harbor-lights");
        shot(&f, "discover", dark);

        // A refresh that finds no connection keeps the program and says so.
        f.send("discover", EventType::Refresh, "{}");
        f.wait_for("offline");
        shot(&f, "offline", dark);
        f.tap("retry-program");
        assert!(pump_until(Duration::from_secs(3), || {
            f.settle();
            f.find("offline").is_none()
        }));

        // No tickets yet: what the tab is for.
        f.tap("tab-tickets");
        f.expect("no-tickets");
        shot(&f, "tickets-empty", dark);

        // The calendar: today's agenda, then a day with nothing on.
        f.tap("tab-calendar");
        f.expect("agenda");
        shot(&f, "calendar", dark);
        f.tap(&day_id(1));
        f.expect("empty-day");
        shot(&f, "calendar-empty", dark);

        // An event, then the booking flow.
        f.tap("tab-discover");
        f.tap("event-harbor-lights");
        f.expect("event-title");
        shot(&f, "event", dark);
        f.tap("get-tickets");
        f.expect("tier-reserved");
        f.tap("tier-reserved");
        shot(&f, "booking-tickets", dark);
        f.tap("continue");
        f.tap("continue");
        f.expect("field-email-error");
        shot(&f, "booking-errors", dark);
        type_into(&f, "name", "Robin Alvarez");
        type_into(&f, "email", "robin@example.org");
        f.tap("continue");
        f.expect("review");
        shot(&f, "booking-review", dark);

        // The first payment times out; trying again books the tickets.
        f.tap("pay");
        f.wait_for("payment-failed");
        shot(&f, "booking-failed", dark);
        f.tap("pay");
        f.wait_for("booked");
        shot(&f, "ticket", dark);

        // The tickets tab lists it.
        assert!(f.surface.go_back());
        f.settle();
        assert!(f.surface.go_back());
        f.settle();
        f.tap("tab-tickets");
        f.expect("ticket-1");
        shot(&f, "tickets", dark);
    }
}

fn events_large_text() {
    let settings = gtk::Settings::default().expect("settings");
    settings.set_gtk_enable_animations(false);
    let dpi = settings.gtk_xft_dpi();
    // The largest accessibility size scales body text about 2.3×; the layout must hold at 2×.
    settings.set_gtk_xft_dpi(2 * 96 * 1024);
    set_dark(false);
    let f = Fixture::open_dir("../../examples/events");
    f.wait_for("featured-harbor-lights");
    shot(&f, "large-text-discover", false);
    f.tap("tab-calendar");
    shot(&f, "large-text-calendar", false);
    f.tap("event-quiet-hours");
    f.expect("event-title");
    shot(&f, "large-text-event", false);
    f.tap("get-tickets");
    f.expect("tier-seat");
    shot(&f, "large-text-booking", false);
    drop(f);
    settings.set_gtk_xft_dpi(dpi);
}
