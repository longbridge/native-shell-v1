//! The shop example, driven end to end: browse, add, cart, checkout, order
//! history, and the loading, empty and error states along the way.

use std::path::PathBuf;

use native_shell::core::protocol::{Command, Prop, PropValue, ROOT_NODE};
use native_shell::testing::{Harness, HarnessBuilder};

fn build() -> HarnessBuilder {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples/shop");
    Harness::from_dir(&dir).expect("example directory")
}

fn started(dir: &PathBuf) -> Harness {
    let h = build().data_dir(dir).start();
    if let Some(e) = h.load_error() {
        panic!("{e}\n{:#?}", h.logs());
    }
    h
}

/// A fresh data directory for one test.
fn data_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("nsh-shop-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

/// Whether the control announces itself disabled (native controls also stop taking input).
fn disabled(h: &Harness, test_id: &str) -> bool {
    match h.prop(test_id, Prop::A11yState) {
        Some(PropValue::Json(j)) => j.contains(r#""disabled":true"#),
        _ => h.prop(test_id, Prop::Enabled) == Some(PropValue::Bool(false)),
    }
}

fn label(h: &Harness, test_id: &str) -> String {
    match h.prop(test_id, Prop::A11yLabel) {
        Some(PropValue::Str(s)) => s,
        other => panic!("{test_id} has no accessibility label: {other:?}"),
    }
}

/// The last toast shown, as its JSON payload.
fn last_toast(h: &Harness) -> String {
    h.commands()
        .iter()
        .rev()
        .find(|(n, c, _)| *n == ROOT_NODE && *c == Command::ShowToast)
        .map(|(_, _, p)| p.clone())
        .unwrap_or_default()
}

/// Waits out the simulated catalog request.
fn loaded(dir: &PathBuf) -> Harness {
    let mut h = started(dir);
    assert!(h.find("catalog-loading").is_some(), "{}", h.dump());
    h.advance(800.0);
    assert!(h.find("grid").is_some(), "{}", h.dump());
    h
}

fn add_to_cart(h: &mut Harness, product: &str) {
    h.tap("tab-shop");
    h.tap(&format!("product-{product}"));
    h.tap("add-to-cart");
    h.tap("back");
}

fn fill_address(h: &mut Harness) {
    h.type_text("field-street", "12 Harbour Street");
    h.type_text("field-city", "Springfield");
    h.type_text("field-postal", "12345");
}

#[test]
fn shop_browse_add_to_cart_check_out_and_see_the_order() {
    let dir = data_dir("main");
    let mut h = loaded(&dir);
    assert_eq!(label(&h, "tab-cart"), "Cart");

    // Categories filter the grid in place.
    h.tap("category-bags");
    assert!(h.find("product-mug").is_none() && h.find("product-daypack").is_some());
    assert!(h.find("featured").is_none());
    h.tap("category-all");

    // The product page: the color moves the gallery, the stepper the price on the button.
    h.tap("product-mug");
    assert_eq!(h.surfaces().len(), 2);
    assert_eq!(h.text_of("price"), "$18.00");
    assert_eq!(h.text_of("add-to-cart"), "Add to Cart · $18.00");
    h.select("color", 1);
    assert_eq!(h.text_of("color-label"), "Color: Sage");
    assert_eq!(label(&h, "gallery"), "Stoneware Mug in Sage");
    h.tap("dot-3");
    assert_eq!(label(&h, "gallery"), "Stoneware Mug, close-up");
    assert!(disabled(&h, "quantity-minus"));
    h.tap("quantity-plus");
    assert_eq!(h.text_of("quantity-value"), "2");
    assert_eq!(h.text_of("add-to-cart"), "Add to Cart · $36.00");

    // Adding confirms with a toast that can undo it; the badge counts it.
    h.tap("add-to-cart");
    assert!(last_toast(&h).contains("2 × Stoneware Mug, Sage"), "{}", last_toast(&h));
    assert_eq!(label(&h, "tab-cart"), "Cart, 2 items");
    assert_eq!(h.text_of("in-cart"), "2 already in your cart");
    h.toast_action("added");
    assert_eq!(label(&h, "tab-cart"), "Cart");
    h.tap("quantity-plus");
    h.tap("add-to-cart");
    h.tap("back");
    assert_eq!(h.surfaces().len(), 1);

    // A sized product: the size is part of the line.
    h.tap("product-planter");
    h.select("size", 2);
    h.tap("add-to-cart");
    h.tap("back");
    assert_eq!(label(&h, "tab-cart"), "Cart, 3 items");

    // The cart: totals, the free-delivery nudge, quantities.
    h.tap("tab-cart");
    assert_eq!(h.text_of("subtitle"), "3 items");
    assert_eq!(h.text_of("line-total-0"), "$36.00");
    assert_eq!(h.text_of("subtotal"), "$64.00");
    assert!(h.text_of("free-delivery").contains("ships free"), "{}", h.text_of("free-delivery"));
    h.tap("line-0-quantity-minus");
    assert_eq!(h.text_of("line-total-0"), "$18.00");
    assert!(h.text_of("free-delivery").contains("Add $4.00 more"), "{}", h.text_of("free-delivery"));
    h.tap("line-0-quantity-plus");

    // Promo codes explain why they don't apply, under the field.
    assert!(disabled(&h, "promo-apply"));
    h.type_text("promo", "summer24");
    h.submit("promo");
    assert_eq!(h.text_of("promo-error"), "SUMMER24 expired on Sep 30.");
    h.type_text("promo", "SAVE15");
    h.tap("promo-apply");
    assert_eq!(h.text_of("promo-error"), "SAVE15 needs an order of $100.00 or more.");
    h.type_text("promo", "NOPE");
    h.tap("promo-apply");
    assert!(h.text_of("promo-error").starts_with("“NOPE” isn’t a valid code"));
    h.type_text("promo", "welcome10");
    h.tap("promo-apply");
    assert!(h.find("promo-error").is_none());
    assert_eq!(h.text_of("promo-applied"), "WELCOME10");
    assert_eq!(h.text_of("discount"), "−$6.40");
    // Subtotal $64.00 − $6.40, plus 8% tax on $57.60.
    assert_eq!(h.text_of("estimated-total"), "$62.21");

    // Checkout validates when the order is placed, under each field.
    h.tap("checkout");
    assert_eq!(h.surfaces().len(), 2);
    assert_eq!(h.native_input("field-name").text, "Ada Lovelace");
    h.tap("place-order");
    assert_eq!(h.text_of("address-street-error"), "Enter a street address.");
    assert_eq!(h.text_of("address-postal-error"), "Enter a postal code.");
    assert!(h.find("address-apartment-error").is_none() && h.find("address-phone-error").is_none());
    fill_address(&mut h);
    assert!(h.find("address-street-error").is_none(), "typing clears the field's error");
    h.type_text("field-postal", "123");
    h.type_text("field-phone", "call me");
    h.tap("place-order");
    assert_eq!(h.text_of("address-postal-error"), "Enter a 5-digit postal code.");
    assert_eq!(h.text_of("address-phone-error"), "Enter a phone number with 7 to 15 digits.");
    h.type_text("field-postal", "12345");
    h.type_text("field-phone", "");

    // Standard ships free over $50; express costs extra.
    assert_eq!(h.text_of("summary-delivery"), "Free");
    h.tap("delivery-express");
    assert_eq!(h.text_of("summary-delivery"), "$12.00");
    assert_eq!(h.text_of("total"), "$74.21");

    // Placing: the button and the back button lock until the store answers.
    h.tap("place-order");
    assert_eq!(h.text_of("place-order"), "Placing Order…");
    assert!(disabled(&h, "place-order") && disabled(&h, "back"));
    h.advance(1300.0);
    assert!(h.find("order-placed").is_some(), "{}", h.dump());
    assert_eq!(h.text_of("order-id"), "LS-10481");

    // The cart is empty, and the order leads the history.
    h.tap("view-orders");
    assert_eq!(h.surfaces().len(), 1);
    assert!(h.find("order-LS-10481").is_some() && h.find("order-LS-10480").is_some());
    assert_eq!(label(&h, "tab-cart"), "Cart");
    h.tap("order-LS-10481");
    assert!(h.texts().contains(&"Stoneware Mug".to_string()));
    assert!(h.texts().contains(&"Discount".to_string()));
    h.tap("back");
    h.tap("tab-cart");
    assert!(h.find("cart-empty").is_some());
    h.tap("browse");
    assert!(h.find("grid").is_some());
    assert!(h.error_logs().is_empty(), "{:?}", h.error_logs());

    // A new instance on the same data directory sees the orders.
    drop(h);
    let mut h = loaded(&dir);
    h.tap("tab-account");
    assert!(h.find("order-LS-10481").is_some());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn shop_cart_removes_with_undo_by_button_and_long_press_and_persists() {
    let dir = data_dir("cart");
    let mut h = loaded(&dir);
    add_to_cart(&mut h, "pen");
    add_to_cart(&mut h, "tote");
    h.tap("tab-cart");
    assert_eq!(h.text_of("subtitle"), "2 items");

    // The visible Remove, then Undo puts the line back where it was.
    h.tap("remove-0");
    assert!(last_toast(&h).contains("Removed Brass Pen"));
    assert_eq!(h.text_of("subtitle"), "1 item");
    h.toast_action("removed");
    assert_eq!(h.text_of("line-total-0"), "$36.00");

    // A long press offers the same Remove, and the way to the product.
    h.long_press("line-1");
    assert!(h.find("line-menu").is_some(), "{}", h.dump());
    h.tap("line-menu-view");
    assert_eq!(h.surfaces().len(), 2);
    h.tap("back");
    h.long_press("line-1");
    h.tap("line-menu-remove");
    assert!(h.find("line-menu").is_none());
    assert!(last_toast(&h).contains("Removed Market Tote"));
    assert_eq!(h.text_of("subtitle"), "1 item");

    // The cart survives a restart.
    drop(h);
    let mut h = loaded(&dir);
    assert_eq!(label(&h, "tab-cart"), "Cart, 1 item");
    h.tap("tab-cart");
    h.tap("remove-0");
    assert!(h.find("cart-empty").is_some());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn shop_search_suggests_finds_and_says_when_nothing_matches() {
    let dir = data_dir("search");
    let mut h = loaded(&dir);
    h.tap("tab-search");
    assert!(h.find("suggestions").is_some());
    h.tap("suggest-wool");
    assert_eq!(h.native_input("search").text, "Wool");
    assert_eq!(h.text_of("result-count"), "1 result");
    h.type_text("search", "terracotta");
    assert_eq!(h.text_of("result-count"), "2 results");
    assert!(h.find("result-lamp").is_some() && h.find("result-planter").is_some());
    h.tap("result-lamp");
    assert_eq!(h.surfaces().len(), 2);
    h.tap("back");

    // The query is still there after visiting another tab.
    h.tap("tab-shop");
    h.tap("tab-search");
    assert_eq!(h.native_input("search").text, "terracotta");

    h.type_text("search", "xylophone");
    assert!(h.find("no-results").is_some());
    assert!(h.texts().contains(&"No results for “xylophone”".to_string()));
    h.tap("no-results-clear");
    assert!(h.find("suggestions").is_some());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn shop_network_errors_show_retry_and_keep_the_order() {
    let dir = data_dir("errors");
    let mut h = loaded(&dir);
    add_to_cart(&mut h, "notebook");
    add_to_cart(&mut h, "lamp");
    h.tap("tab-account");
    h.toggle("fail-network");

    // The catalog fails to load on the next start; retrying while offline fails again.
    drop(h);
    let mut h = started(&dir);
    h.advance(800.0);
    assert!(h.find("catalog-error").is_some(), "{}", h.dump());
    h.tap("catalog-retry");
    assert!(h.find("catalog-loading").is_some());
    h.advance(800.0);
    assert!(h.find("catalog-error").is_some());

    // Placing an order fails, says nothing was charged, and keeps the cart and address.
    h.tap("tab-cart");
    h.tap("checkout");
    fill_address(&mut h);
    h.tap("place-order");
    h.advance(1300.0);
    assert!(h.find("order-failed").is_some(), "{}", h.dump());
    assert!(h.text_of("order-failed").contains("wasn’t charged"));
    assert_eq!(h.text_of("place-order"), "Try Again");
    assert!(!disabled(&h, "back"));
    assert_eq!(h.native_input("field-street").text, "12 Harbour Street");

    // Trying again while still offline fails the same way.
    h.tap("place-order");
    assert_eq!(h.text_of("place-order"), "Placing Order…");
    h.advance(1300.0);
    assert!(h.find("order-failed").is_some());

    // The connection comes back (the demo switch, on the tab under this page): the retry places it.
    h.tap("tab-account");
    h.toggle("fail-network");
    h.tap("place-order");
    h.advance(1300.0);
    assert!(h.find("order-placed").is_some(), "{}", h.dump());
    h.tap("continue-shopping");
    assert_eq!(h.surfaces().len(), 1);
    assert!(h.find("catalog-error").is_some() || h.find("grid").is_some());
    assert_eq!(label(&h, "tab-cart"), "Cart");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn shop_order_history_has_an_empty_state() {
    let dir = data_dir("orders");
    let h = loaded(&dir);
    drop(h);
    // A saved state whose history is empty (the first run shows two sample orders).
    std::fs::write(dir.join("localStorage.json"), r#"{"shop.v1":"{\"cart\":[],\"orders\":[]}"}"#).unwrap();
    let mut h = loaded(&dir);
    h.tap("tab-account");
    assert!(h.find("orders-empty").is_some(), "{}", h.dump());
    assert!(h.find("order-LS-10480").is_none());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn shop_back_button_drops_its_title_when_it_would_crowd_the_page_title() {
    use native_shell::core::host::Environment;
    let dir = data_dir("back");
    let mut h = loaded(&dir);
    h.tap("product-mug");
    assert_eq!(h.text_of("back"), "Shop");
    h.tap("back");
    // A long product name leaves no room for "Shop" beside it.
    h.tap("product-towels");
    assert_eq!(h.text_of("back"), "");
    assert_eq!(label(&h, "back"), "Back to Shop");
    h.tap("back");
    // At accessibility text sizes the chevron stands alone.
    h.set_environment(Environment { width: 390.0, height: 844.0, font_scale: 2.0, ..Environment::default() });
    h.tap("product-mug");
    assert_eq!(h.text_of("back"), "");
    assert_eq!(label(&h, "back"), "Back to Shop");
    let _ = std::fs::remove_dir_all(&dir);
}
