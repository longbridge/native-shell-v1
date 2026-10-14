//! The events example, driven through its main flows: discover → event →
//! booking → ticket, the calendar, cancelling with undo, and the network
//! failures its stand-in service scripts (the first refresh is offline, the
//! first payment times out).

use std::path::PathBuf;

use native_shell::core::protocol::{Command, EventType, Prop, PropValue};
use native_shell::testing::Harness;

/// First load, refresh and payment latencies of the stand-in service, with room to spare.
const LOAD_MS: f64 = 1100.0;
const REFRESH_MS: f64 = 800.0;
const PAY_MS: f64 = 1300.0;

fn data_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("nsh-events-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

fn start(dir: &PathBuf) -> Harness {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples/events");
    let h = Harness::from_dir(&path).expect("example directory").data_dir(dir).start();
    if let Some(e) = h.load_error() {
        panic!("{e}\n{:#?}", h.logs());
    }
    h
}

/// Started, with the program loaded.
fn loaded(dir: &PathBuf) -> Harness {
    let mut h = start(dir);
    h.advance(LOAD_MS);
    assert!(h.find("featured-harbor-lights").is_some(), "{:?}\n{}", h.logs(), h.dump());
    h
}

fn disabled(h: &Harness, test_id: &str) -> bool {
    match h.prop(test_id, Prop::A11yState) {
        Some(PropValue::Json(j)) => j.contains(r#""disabled":true"#),
        _ => h.prop(test_id, Prop::Enabled) == Some(PropValue::Bool(false)),
    }
}

fn selected(h: &Harness, test_id: &str) -> bool {
    matches!(h.prop(test_id, Prop::A11yState), Some(PropValue::Json(j)) if j.contains(r#""selected":true"#))
}

/// Every mounted test id starting with `prefix`, sorted (calendar days sort by date).
fn ids_with_prefix(h: &Harness, prefix: &str) -> Vec<String> {
    let mut ids: Vec<String> = h
        .native()
        .nodes
        .values()
        .filter_map(|n| match n.props.get(&Prop::TestId) {
            Some(PropValue::Str(s)) if s.starts_with(prefix) => Some(s.clone()),
            _ => None,
        })
        .collect();
    ids.sort();
    ids.dedup();
    ids
}

fn checked(h: &Harness, test_id: &str) -> bool {
    matches!(h.prop(test_id, Prop::A11yState), Some(PropValue::Json(j)) if j.contains(r#""checked":true"#))
}

fn toasts(h: &Harness) -> Vec<serde_json::Value> {
    h.commands()
        .iter()
        .filter(|(_, c, _)| *c == Command::ShowToast)
        .filter_map(|(_, _, p)| serde_json::from_str(p).ok())
        .collect()
}

/// Books `event_id` from the Discover tab: tier, quantity, attendee, then pays
/// through the scripted first timeout. Leaves the ticket page on top.
fn book(h: &mut Harness, event_id: &str, tier: &str) {
    h.tap(&format!("event-{event_id}"));
    h.tap("get-tickets");
    h.tap(&format!("tier-{tier}"));
    h.tap("continue");
    h.type_text("name", "Robin Alvarez");
    h.type_text("email", "robin@example.org");
    h.tap("continue");
    h.tap("pay");
    h.advance(PAY_MS);
    if h.find("payment-failed").is_some() {
        h.tap("pay");
        h.advance(PAY_MS);
    }
    assert!(h.find("booked").is_some(), "{}", h.dump());
}

#[test]
fn discover_event_booking_and_ticket() {
    let dir = data_dir("book");
    let mut h = start(&dir);
    // Placeholders until the program arrives, then the featured events and the list by day.
    assert!(h.find("loading").is_some());
    assert!(h.find("featured").is_none());
    h.advance(LOAD_MS);
    assert!(h.find("loading").is_none());
    assert_eq!(h.text_of("subtitle"), "14 events in Harborview");
    assert_eq!(h.text_of("day-0"), "Today");
    assert!(h.find("day-1").is_none(), "no section for a day without events");
    assert!(h.text_of("event-quiet-hours").contains("4 left"), "few seats left are called out");

    // Category filter.
    h.tap("category-theater");
    assert!(selected(&h, "category-theater") && !selected(&h, "category-all"));
    assert!(h.find("event-the-understudy").is_some() && h.find("event-night-market").is_none());
    h.tap("category-all");
    assert!(h.find("event-night-market").is_some());

    // The event page: when, where, how much, and one primary action.
    h.tap("event-harbor-lights");
    assert_eq!(h.surfaces().len(), 2);
    assert_eq!(h.text_of("event-title"), "Harbor Lights Orchestra");
    assert_eq!(h.text_of("details-time"), "7:30 – 10:00 PM");
    assert_eq!(h.text_of("details-price"), "$28 – $120");
    assert_eq!(h.text_of("from-price"), "$28");
    assert!(h.find("availability").is_none());
    assert!(h.find("map").is_some());

    // Tickets: tier radios, quantity stepper clamped to what's left.
    h.tap("get-tickets");
    assert_eq!(h.surfaces().len(), 3);
    assert!(checked(&h, "tier-lawn"));
    assert_eq!(h.text_of("total"), "$56", "two lawn tickets by default");
    h.tap("tier-box");
    assert_eq!(h.text_of("total"), "$240");
    assert!(disabled(&h, "quantity-more"), "only two boxes left");
    h.tap("tier-reserved");
    h.tap("quantity-more");
    assert_eq!(h.text_of("total"), "$162");
    h.tap("continue");

    // Details: validation under each field, Return moves through the form.
    h.tap("continue");
    assert_eq!(h.text_of("name-error"), "Enter the name on the booking.");
    assert_eq!(h.text_of("email-error"), "Enter an email address for the tickets.");
    h.type_text("name", "Robin Alvarez");
    assert!(h.find("name-error").is_none(), "an error clears once fixed");
    h.type_text("email", "robin@example");
    h.type_text("phone", "12");
    h.submit("phone");
    assert_eq!(h.text_of("email-error"), "Enter an email address like name@example.org.");
    assert!(h.find("phone-error").is_some());
    h.type_text("email", "robin@example.org");
    h.type_text("phone", "");
    h.tap("continue");

    // Review, then the scripted timeout: nothing charged, try again.
    assert!(h.text_of("review").contains("3 × Reserved seat"));
    assert_eq!(h.text_of("pay"), "Pay $162");
    h.tap("pay");
    assert!(disabled(&h, "pay"), "no second charge while paying");
    h.advance(PAY_MS);
    assert!(h.text_of("payment-failed").contains("You haven’t been charged"));
    assert_eq!(h.text_of("pay"), "Try Again");
    h.tap("pay");
    h.advance(PAY_MS);

    // The ticket replaces the booking; back returns to the event, which says you're going.
    assert_eq!(h.surfaces().len(), 3);
    assert!(h.text_of("booked").contains("robin@example.org"));
    assert!(h.text_of("ticket-code-text").starts_with("HV-"));
    assert_eq!(h.text_of("ticket-seats"), "3 × Reserved seat · Admits 3");
    assert!(h.find("ticket-code").is_some());
    h.tap("back");
    assert!(h.text_of("going").contains("3 tickets"));
    h.tap("back");
    h.tap("tab-tickets");
    assert!(selected(&h, "tab-tickets"));
    assert!(h.text_of("ticket-1").contains("Harbor Lights Orchestra"));
    assert!(h.error_logs().is_empty(), "{:?}", h.error_logs());

    // Tickets stay on the device.
    drop(h);
    let mut h = loaded(&dir);
    h.tap("tab-tickets");
    assert!(h.find("ticket-1").is_some());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn calendar_selects_days_and_shows_an_empty_day() {
    let dir = data_dir("calendar");
    let mut h = loaded(&dir);
    h.tap("tab-calendar");
    // Today is selected, with its agenda.
    let days = ids_with_prefix(&h, "day-");
    let today = days.iter().find(|d| selected(&h, d)).expect("a selected day").clone();
    assert!(h.find("event-night-market").is_some() && h.find("event-quiet-hours").is_some());

    // Tomorrow has nothing on: the empty state points at the next event.
    let tomorrow = days.iter().position(|d| *d == today).and_then(|i| days.get(i + 1)).cloned();
    if let Some(tomorrow) = tomorrow {
        h.tap(&tomorrow);
        assert!(selected(&h, &tomorrow));
        assert!(h.find("empty-day").is_some(), "{}", h.dump());
        h.tap("next-event-day");
        assert!(h.find("event-harbor-lights").is_some());
    }

    // The month moves with the arrows.
    let month = h.text_of("month");
    h.tap("next-month");
    assert_ne!(h.text_of("month"), month);
    h.tap("previous-month");
    assert_eq!(h.text_of("month"), month);

    // An event added to the calendar is marked in the agenda.
    h.tap("tab-discover");
    h.tap("event-glass-garden");
    h.tap("add-to-calendar");
    assert_eq!(h.text_of("add-to-calendar"), "In Calendar");
    assert_eq!(toasts(&h).last().unwrap()["title"], "Added to Calendar");
    h.tap("back");
    h.tap("tab-calendar");
    let index = days.iter().position(|d| *d == today).unwrap();
    if let Some(day) = days.get(index + 3) {
        h.tap(day);
        assert!(h.text_of("event-glass-garden").contains("In Calendar"));
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn cancelling_asks_naming_the_event_and_can_be_undone() {
    let dir = data_dir("cancel");
    let mut h = loaded(&dir);
    book(&mut h, "cities-water", "free");
    assert_eq!(h.text_of("ticket-seats"), "2 × Free entry · Admits 2");
    h.tap("back");
    h.tap("back");
    h.tap("tab-tickets");
    h.tap("ticket-1");

    // Keep Tickets leaves everything as it was.
    h.tap("cancel-booking");
    let alert = h.alert().expect("a confirmation");
    assert_eq!(alert["title"], "Cancel tickets for “Cities and Water”?");
    assert_eq!(alert["actions"][0]["label"], "Keep Tickets");
    assert_eq!(alert["actions"][1]["style"], "destructive");
    h.alert_action(0);
    assert!(h.find("ticket-code").is_some());

    // Cancel Booking pops back to an empty list, with Undo.
    h.clear_commands();
    h.tap("cancel-booking");
    h.alert_action(1);
    assert_eq!(h.surfaces().len(), 1);
    assert!(h.find("no-tickets").is_some());
    assert_eq!(toasts(&h).last().unwrap()["title"], "Booking cancelled");
    h.toast_action("booking-cancelled");
    assert!(h.find("ticket-1").is_some() && h.find("no-tickets").is_none(), "{:?}\n{}", h.logs(), h.dump());
    assert!(h.error_logs().is_empty(), "{:?}", h.error_logs());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn an_offline_refresh_keeps_the_program_and_retries() {
    let dir = data_dir("offline");
    let mut h = loaded(&dir);
    let discover = h.find("discover").unwrap();
    h.dispatch(discover, EventType::Refresh, "{}");
    assert_eq!(h.prop("discover", Prop::Refreshing), Some(PropValue::Bool(true)));
    h.advance(REFRESH_MS);
    assert!(h.text_of("offline").contains("You’re offline"));
    assert!(h.find("event-night-market").is_some(), "the program stays on screen");
    h.tap("retry-program");
    h.advance(REFRESH_MS);
    assert!(h.find("offline").is_none());
    assert_eq!(h.prop("discover", Prop::Refreshing), Some(PropValue::Bool(false)));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_sold_out_event_cannot_be_booked() {
    let dir = data_dir("sold-out");
    let mut h = loaded(&dir);
    h.tap("category-theater");
    assert!(h.text_of("event-the-understudy").contains("Sold out"));
    h.tap("event-the-understudy");
    assert_eq!(h.text_of("availability"), "Sold out");
    assert_eq!(h.text_of("get-tickets"), "Sold Out");
    assert!(disabled(&h, "get-tickets"));
    let _ = std::fs::remove_dir_all(&dir);
}
