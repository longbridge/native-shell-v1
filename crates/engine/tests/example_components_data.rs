//! The components example's data and navigation pages (order, files,
//! booking, photos, settings), driven through their main flows.

use std::path::PathBuf;

use native_shell::core::protocol::{Command, EventType, Prop, PropValue, ViewType, ROOT_NODE};
use native_shell::testing::Harness;

fn components() -> Harness {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples/components");
    let h = Harness::from_dir(&dir).expect("example directory").start();
    if let Some(e) = h.load_error() {
        panic!("{e}\n{:#?}", h.logs());
    }
    h
}

fn assert_clean(h: &Harness) {
    assert!(h.error_logs().is_empty(), "{:?}", h.error_logs());
}

fn has_text(h: &Harness, text: &str) -> bool {
    h.texts().iter().any(|t| t == text)
}

fn disabled(h: &Harness, test_id: &str) -> bool {
    match h.prop(test_id, Prop::A11yState) {
        Some(PropValue::Json(j)) if j.contains(r#""disabled":true"#) => true,
        _ => h.prop(test_id, Prop::Enabled) == Some(PropValue::Bool(false)),
    }
}

fn last_toast(h: &Harness) -> String {
    h.commands()
        .iter()
        .rev()
        .find(|(n, c, _)| *n == ROOT_NODE && *c == Command::ShowToast)
        .map(|(_, _, p)| p.clone())
        .unwrap_or_else(|| panic!("no toast in {:?}", h.commands()))
}

/// The node announced as `label`.
fn labeled(h: &Harness, label: &str) -> u32 {
    h.native()
        .nodes
        .iter()
        .find(|(_, n)| matches!(n.props.get(&Prop::A11yLabel), Some(PropValue::Str(s)) if s == label))
        .map(|(id, _)| *id)
        .unwrap_or_else(|| panic!("nothing announced as {label:?}\n{}", h.dump()))
}

#[test]
fn order_page_loads_then_reads_like_a_receipt() {
    let mut h = components();
    h.tap("open-order");
    assert!(h.find("order-loading").is_some());
    h.advance(600.0);
    assert!(h.find("order-loading").is_none());
    assert!(has_text(&h, "W4471-2289"));
    assert_eq!(h.text_of("total"), "$89.73");
    assert_eq!(h.text_of("delivery"), "Carrier Royal Mail Tracking RM 22 334 556 GB Instructions Leave with the neighbour at number 14 if no one answers.");

    // The split button: the label tracks, the menu picks another delivery option.
    h.tap_text("Track Package");
    assert!(last_toast(&h).contains("Tracking opens in Royal Mail"));
    let options = h.native().children(h.find("track").unwrap())[1];
    h.dispatch(options, EventType::Change, r#"{"index":2,"value":2}"#);
    assert_eq!(h.text_of("delivery-note"), "Held at Bristol Central from tomorrow, 9 AM.");

    // Less frequent actions wait in the More menu; returns are not open yet.
    h.select("more", 0);
    assert!(last_toast(&h).contains("Invoice saved to Files"));
    h.select("more", 1);
    assert_eq!(h.alert().unwrap()["title"], "Report a problem with this order?");
    h.alert_action(0);
    assert_clean(&h);
}

#[test]
fn files_page_selects_sorts_and_adds_folders() {
    let mut h = components();
    h.tap("open-files");
    assert_eq!(h.text_of("path"), "Garden Project Photos October Harvest.jpg");
    assert_eq!(h.text_of("details"), "Name Harvest.jpg Kind JPEG image Size 3.1 MB Modified Oct 12, 2026");
    assert_eq!(h.text_of("tree"), "Design Notes Photos October Harvest.jpg After.jpg Before.jpg Budget.csv");

    // A folder opens in place; a file shows its details.
    h.tap_text("Notes");
    assert!(h.text_of("tree").contains("Notes Seed order.md Soil test results.txt"));
    h.tap_text("Seed order.md");
    assert_eq!(h.text_of("path"), "Garden Project Notes Seed order.md");
    assert!(h.text_of("details").contains("Markdown"));

    // The path goes back to the project, which has no details of its own.
    h.tap_text("Garden Project");
    assert!(h.find("no-selection").is_some());

    // Sorting by size puts the biggest folder first.
    h.select("sort", 2);
    assert!(h.text_of("tree").starts_with("Photos"), "{}", h.text_of("tree"));
    assert_eq!(h.prop("sort", Prop::Placeholder), Some(PropValue::Str("Sort by Size".into())));

    // A new folder is added, selected and counted.
    assert_eq!(h.text_of("count"), "4 folders, 8 files");
    h.tap("new-folder");
    assert_eq!(h.text_of("count"), "5 folders, 8 files");
    assert!(h.text_of("details").starts_with("Name Untitled folder Kind Folder Size Empty Modified"), "{}", h.text_of("details"));
    assert_clean(&h);
}

#[test]
fn booking_page_waits_for_valid_dates() {
    let mut h = components();
    h.tap("open-booking");
    assert_eq!(h.text_of("summary-prompt"), "Choose your check-in date.");
    assert!(disabled(&h, "request"));

    // Two months ahead, clear of every booked night.
    h.tap_node(labeled(&h, "Next month"));
    h.tap_node(labeled(&h, "Next month"));
    h.tap_text("6");
    assert!(h.text_of("summary-prompt").ends_with("Now choose your check-out date."));
    h.tap_text("9");
    assert_eq!(h.text_of("total"), "$475.00");
    assert!(h.text_of("summary").contains("3 × $145.00"));
    assert!(!disabled(&h, "request"));
    h.tap("request");
    assert!(h.find("requested").is_some());
    assert!(disabled(&h, "request"));

    // A day visit is one date.
    let stay = h.find("stay").unwrap();
    h.dispatch(stay, EventType::Change, r#"{"index":1,"value":"Day Visit"}"#);
    assert_eq!(h.text_of("summary-prompt"), "Choose the day of your visit.");
    h.tap_text("6");
    assert_eq!(h.text_of("total"), "$60.00");
    assert_clean(&h);
}

#[test]
fn photos_page_pages_and_keeps_favorites() {
    let mut h = components();
    h.tap("open-photos");
    assert_eq!(h.text_of("caption"), "First light on the lake");
    h.tap("next");
    assert_eq!(h.text_of("caption"), "Out in the canoe");
    assert_eq!(h.prop("carousel", Prop::A11yValue), Some(PropValue::Str("Item 2 of 5".into())));

    // Star this one; Favorites shows the starred photos in album order.
    h.tap("favorite");
    assert_eq!(h.prop("favorite", Prop::A11yLabel), Some(PropValue::Str("Remove from Favorites".into())));
    h.tap_text("Favorites 2");
    assert_eq!(h.text_of("caption"), "Out in the canoe");

    // With nothing starred, Favorites says how to fill it.
    h.tap("favorite");
    h.tap("favorite");
    assert!(h.find("no-favorites").is_some(), "{}", h.dump());
    h.tap("show-all");
    assert_eq!(h.text_of("caption"), "Out in the canoe");
    assert_clean(&h);
}

#[test]
fn settings_page_disables_what_depends_on_notifications() {
    let mut h = components();
    h.tap("open-notifications");
    assert!(!disabled(&h, "sounds"));
    h.toggle("allow");
    assert!(disabled(&h, "sounds"));
    assert!(disabled(&h, "previews"));
    assert!(has_text(&h, "Turn on notifications to use this."));

    let pages = h.nodes_of(ViewType::Segmented)[0];
    h.dispatch(pages, EventType::Change, r#"{"index":1,"value":"Calendar"}"#);
    assert!(h.find("invitations").is_some());
    h.select("alert", 3);
    assert_eq!(h.prop("alert", Prop::SelectedIndex), Some(PropValue::I32(3)));
    assert_clean(&h);
}
