//! The portfolio example, driven through its main flows.
//!
//! Prices are simulated from seeded walks, so totals are the same on every run
//! and on every day; only date labels follow the calendar.

use std::path::{Path, PathBuf};

use native_shell::core::protocol::{Command, EventType, Mutation, Prop, PropValue};
use native_shell::testing::Harness;

/// A fresh data directory for one test.
fn data_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("nsh-portfolio-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

fn start(dir: &Path) -> Harness {
    let example = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples/portfolio");
    let h = Harness::from_dir(&example).expect("example directory").data_dir(dir).start();
    if let Some(e) = h.load_error() {
        panic!("{e}\n{:#?}", h.logs());
    }
    h
}

/// Started and past the first simulated round trip.
fn loaded(dir: &Path) -> Harness {
    let mut h = start(dir);
    assert!(h.find("loading").is_some(), "{}", h.dump());
    h.advance(500.0);
    assert!(h.find("loading").is_none(), "{}", h.dump());
    h
}

fn disabled(h: &Harness, test_id: &str) -> bool {
    match h.prop(test_id, Prop::A11yState) {
        Some(PropValue::Json(j)) => j.contains(r#""disabled":true"#),
        _ => h.prop(test_id, Prop::Enabled) == Some(PropValue::Bool(false)),
    }
}

/// `$32,614.49` → 32614.49; a leading minus sign (U+2212 or ASCII) makes it negative.
fn amount(text: &str) -> f64 {
    let negative = text.starts_with('\u{2212}') || text.starts_with('-');
    let digits: String = text.chars().filter(|c| c.is_ascii_digit() || *c == '.').collect();
    let n: f64 = digits.parse().unwrap_or_else(|_| panic!("not an amount: {text:?}"));
    if negative {
        -n
    } else {
        n
    }
}

/// The payloads of the toasts shown since the harness started, oldest first.
fn toasts(h: &Harness) -> Vec<serde_json::Value> {
    h.mutations()
        .iter()
        .filter_map(|m| match m {
            Mutation::Command { command: Command::ShowToast, payload, .. } => serde_json::from_str(payload).ok(),
            _ => None,
        })
        .collect()
}

fn assert_clean(h: &Harness) {
    assert!(h.error_logs().is_empty(), "{:?}", h.error_logs());
    // Errors in tasks are logged rather than thrown at the caller.
    assert!(!h.logs().iter().any(|l| l.contains("Error")), "{:?}", h.logs());
}

#[test]
fn overview_totals_ranges_sorting_and_live_updates() {
    let dir = data_dir("overview");
    let mut h = loaded(&dir);
    // The sample history, valued at the simulated prices.
    assert_eq!(h.text_of("total"), "$32,614.49");
    assert_eq!(h.text_of("day-change"), "−$56.91 (−0.17%)");
    assert!(h.text_of("total-gain").starts_with('+'));
    // The day's chart starts at the previous close, which its caption names.
    assert_eq!(h.text_of("value-caption"), "Previous close $32,671.40");
    assert!(h.prop("value-chart", Prop::A11yLabel).is_some());

    // Each range draws a different span and says how it went.
    let mut captions = vec![h.text_of("value-range-change")];
    for range in 1..4 {
        h.select("value-range", range);
        captions.push(h.text_of("value-range-change"));
    }
    captions.dedup();
    assert_eq!(captions.len(), 4, "{captions:?}");
    assert!(h.texts().contains(&"Past year".to_string()));

    // Sorting from the menu reorders the rows.
    let order = |h: &Harness| -> Vec<String> {
        h.texts()
            .into_iter()
            .filter(|t| ["WAVE", "ACME", "NOVA", "VRDE", "HRBR", "QUIL"].contains(&t.as_str()))
            .skip(4) // the allocation legend's symbols come first
            .collect()
    };
    assert_eq!(h.text_of("holdings").split(' ').next(), Some("WAVE"));
    h.select("sort", 3);
    // The menu's button names the order in force.
    assert!(matches!(h.prop("sort", Prop::Menu), Some(PropValue::Json(j)) if j.contains(r#""placeholder":"Sort: Symbol""#)));
    assert_eq!(order(&h).first().map(String::as_str), Some("ACME"), "{:?}", order(&h));
    h.select("sort", 1);
    assert_eq!(h.text_of("holdings").split(' ').next(), Some("QUIL"), "biggest gainer today first");

    // Prices move live every two seconds…
    let before = h.text_of("total");
    h.advance(2000.0);
    let live = h.text_of("total");
    assert_ne!(live, before);
    // …and stop while the app is in the background, as its timers do.
    h.instance_mut().set_active(false);
    h.advance(10_000.0);
    assert_eq!(h.text_of("total"), live);
    h.instance_mut().set_active(true);
    h.advance(2000.0);
    assert_ne!(h.text_of("total"), live);
    assert_clean(&h);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn adding_a_transaction_updates_totals_and_can_be_undone() {
    let dir = data_dir("add");
    let mut h = loaded(&dir);
    let total = amount(&h.text_of("total"));

    h.tap("add");
    assert_eq!(h.surfaces().len(), 2);
    assert!(disabled(&h, "form-add"), "Add waits for the shares");
    // A purchase of ORBT, a new holding, at the prefilled market price.
    h.select("form-symbol", 6);
    let price = amount(&h.native_input("form-price").text);
    // Decimal keyboards; Return on shares moves to the price.
    for id in ["form-shares", "form-price"] {
        assert_eq!(h.prop(id, Prop::KeyboardType), Some(PropValue::I32(2)), "decimal");
    }
    assert_eq!(h.prop("form-shares", Prop::ReturnKey), Some(PropValue::I32(3)), "next");
    assert_eq!(h.prop("form-price", Prop::ReturnKey), Some(PropValue::I32(1)), "done");
    h.type_text("form-shares", "12.5");
    h.clear_commands();
    h.submit("form-shares");
    let price_node = h.find("form-price").unwrap();
    assert!(h.commands().iter().any(|(n, c, _)| *n == price_node && *c == Command::Focus), "{:?}", h.commands());
    assert_eq!(amount(&h.text_of("form-total")), (12.5 * price * 100.0).round() / 100.0);
    h.submit("form-price");
    assert_eq!(h.surfaces().len(), 1, "{}", h.dump());

    // Totals include the new holding at once, and the toast confirms with Undo.
    let added = amount(&h.text_of("total"));
    assert!((added - total - 12.5 * price).abs() < 0.02, "{total} + 12.5 × {price} ≠ {added}");
    assert!(h.find("holding-ORBT").is_some());
    let toast = toasts(&h).pop().unwrap();
    assert_eq!(toast["title"], "Bought 12.5 ORBT");
    h.toast_action("transaction");
    assert!(h.find("holding-ORBT").is_none());
    assert_eq!(amount(&h.text_of("total")), total);

    // The transaction survives a restart.
    h.tap("add");
    h.type_text("form-shares", "10");
    h.tap("form-add");
    let saved = h.text_of("total");
    drop(h);
    let h = loaded(&dir);
    assert_eq!(h.text_of("total"), saved);
    assert_clean(&h);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_transaction_form_validates_and_asks_before_discarding() {
    let dir = data_dir("form");
    let mut h = loaded(&dir);
    h.tap("add");

    // Shares that aren't a number are named under the field.
    h.type_text("form-shares", "1.23456");
    h.tap("form-add");
    assert_eq!(h.text_of("form-shares-error"), "Enter a number of shares, like 10 or 2.5.");
    h.type_text("form-shares", "2");
    assert!(h.find("form-shares-error").is_none(), "editing clears the message");

    // A sale can't exceed what was held on its date.
    h.select("form-type", 1);
    h.select("form-symbol", 1); // sellable symbols only, alphabetically by holding order: NOVA is second
    h.type_text("form-shares", "500");
    h.tap("form-add");
    assert!(h.text_of("form-shares-error").starts_with("You held "), "{}", h.dump());
    // A future date is refused.
    h.type_text("form-shares", "1");
    h.pick_date("form-date", "2099-01-02");
    h.tap("form-add");
    assert_eq!(h.text_of("form-date-error"), "Choose today or an earlier date.");

    // Cancel with a draft asks; keep editing stays, discard closes.
    h.tap("form-cancel");
    assert_eq!(h.alert().unwrap()["title"], "Discard this transaction?");
    h.alert_action(0);
    assert!(h.find("transaction-form").is_some());
    h.tap("form-cancel");
    h.alert_action(1);
    assert!(h.find("transaction-form").is_none());
    assert_eq!(h.text_of("total"), "$32,614.49");
    assert_clean(&h);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn offline_shows_a_banner_or_an_error_and_retry_recovers() {
    let dir = data_dir("offline");
    let mut h = loaded(&dir);
    // Turning the simulated network off from the alerts page.
    h.tap("alerts");
    h.toggle("simulate-offline");
    h.tap("back");
    assert!(h.find("offline").is_some(), "{}", h.dump());
    let total = h.text_of("total");
    h.advance(4000.0);
    assert_eq!(h.text_of("total"), total, "no live updates while offline");

    // Retry and pull to refresh both try again, and fail while it's off.
    h.tap("offline-retry");
    assert!(disabled(&h, "offline-retry"));
    h.advance(500.0);
    assert!(h.find("offline").is_some());
    let scroll = h.find("portfolio").unwrap();
    h.dispatch(scroll, EventType::Refresh, "{}");
    assert_eq!(h.prop("portfolio", Prop::Refreshing), Some(PropValue::Bool(true)));
    h.advance(1300.0);
    assert_eq!(h.prop("portfolio", Prop::Refreshing), Some(PropValue::Bool(false)));
    assert!(h.find("offline").is_some());

    // Launched offline, there is nothing to show yet: a full-screen error.
    drop(h);
    let mut h = loaded(&dir);
    assert!(h.find("error").is_some() && h.find("total").is_none(), "{}", h.dump());
    h.tap("retry");
    assert!(h.find("loading").is_some());
    h.advance(500.0);
    assert!(h.find("error").is_some());

    // Back online, a retry loads the portfolio.
    h.tap("alerts");
    h.toggle("simulate-offline");
    h.tap("back");
    h.advance(500.0);
    assert_eq!(h.text_of("total"), "$32,614.49");
    assert!(h.find("offline").is_none());
    assert_clean(&h);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn an_empty_portfolio_invites_a_first_transaction() {
    let dir = data_dir("empty");
    let mut h = loaded(&dir);
    h.tap("alerts");
    h.tap("erase");
    assert_eq!(h.alert().unwrap()["title"], "Erase 9 transactions?");
    h.alert_action(1);
    assert!(disabled(&h, "erase"));
    assert!(h.find("alert-holdings").is_none());
    h.tap("back");
    assert!(h.find("empty").is_some() && h.find("total").is_none(), "{}", h.dump());

    h.tap("empty-add");
    h.type_text("form-shares", "3");
    h.tap("form-add");
    assert!(h.find("empty").is_none());
    assert!(h.find("holding-ACME").is_some());
    assert_eq!(h.text_of("holdings").split(' ').next(), Some("ACME"));
    assert_clean(&h);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_holding_shows_its_position_and_deletes_transactions_with_undo() {
    let dir = data_dir("holding");
    let mut h = loaded(&dir);
    h.tap("holding-NOVA");
    assert_eq!(h.surfaces().len(), 2);
    assert_eq!(h.text_of("position-shares-value"), "20");
    assert_eq!(h.text_of("position-market-value-value"), h.text_of("value-NOVA"));
    assert_eq!(h.text_of("price-caption"), "Previous close $318.42");
    h.select("price-range", 3);
    assert!(h.text_of("price-caption").starts_with("Past year +"), "{}", h.text_of("price-caption"));

    // Deleting the sale asks with the transaction named, then offers Undo.
    let sale = h.find_text("Sold 5 shares").expect("the sale");
    h.tap_node(sale);
    assert_eq!(h.alert().unwrap()["title"], "Delete “Sell 5 NOVA”?");
    h.alert_action(1);
    assert_eq!(h.text_of("position-shares-value"), "25");
    h.toast_action("transaction");
    assert_eq!(h.text_of("position-shares-value"), "20");

    // Selling everything keeps the record and says the holding is closed.
    h.tap("holding-add");
    h.select("form-type", 1);
    h.type_text("form-shares", "20");
    h.tap("form-add");
    assert!(h.find("not-held").is_some(), "{}", h.dump());
    h.tap("back");
    assert!(h.find("holding-NOVA").is_none());
    assert_clean(&h);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn alerts_announce_big_moves_once_and_respect_settings() {
    let dir = data_dir("alerts");
    let mut h = loaded(&dir);
    h.tap("alerts");
    // Dependent settings are disabled, not hidden, when move alerts are off.
    h.toggle("move-alerts");
    assert!(disabled(&h, "threshold") && disabled(&h, "alert-NOVA"));
    h.toggle("move-alerts");
    // A 2% threshold: the live feed carries some holdings across it.
    h.select("threshold", 0);
    h.tap("back");
    h.advance(60_000.0);
    let moves: Vec<String> = toasts(&h).iter().map(|t| t["title"].as_str().unwrap_or("").to_string()).collect();
    let mut unique = moves.clone();
    unique.sort();
    unique.dedup();
    assert!(!moves.is_empty());
    assert!(moves.iter().all(|m| m.contains(" today")), "{moves:?}");
    assert_eq!(unique.len(), moves.len(), "each holding is announced once: {moves:?}");
    assert_clean(&h);
    let _ = std::fs::remove_dir_all(&dir);
}

