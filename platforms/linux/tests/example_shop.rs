//! The shop example on GTK, screen by screen: each step is driven through the
//! renderer as a user would, and each main screen is captured in both
//! appearances into `screenshots/shop-<screen>-{light,dark}.png`.

#[path = "../examples/common/mod.rs"]
mod common;
mod support;

use std::process::ExitCode;
use std::time::Duration;


use native_shell_core::protocol::EventType;
use native_shell_gtk::{InstanceOptions, Runtime};
use support::*;

fn main() -> ExitCode {
    run(&[
        ("shop from browsing to a placed order", browse_to_order),
        ("shop's empty, no-result and error states", empty_and_error_states),
    ])
}

fn open() -> Fixture {
    // Pages push without the slide, so a pushed page is in place for the next step.
    gtk::Settings::default().expect("settings").set_gtk_enable_animations(false);
    let dir = common::examples_dir().join("shop");
    let f = Fixture::start_with(&Runtime::new(), InstanceOptions::default(), |instance| instance.load_dir(&dir));
    f.wait_for("grid");
    f
}

/// Captures the screen as it is in light and in dark, once earlier toasts have gone.
fn shoot(f: &Fixture, name: &str) {
    pump_until(Duration::from_secs(8), || f.find("nsh-toast").is_none());
    for dark in [false, true] {
        f.set_dark(dark);
        f.screenshot(format!("screenshots/shop-{name}-{}.png", if dark { "dark" } else { "light" }));
    }
    f.set_dark(false);
}

/// Replaces a field's text in its GTK editable, as typing would.
fn type_text(f: &Fixture, test_id: &str, text: &str) {
    let field = f.expect(test_id);
    let editable = find_editable(&field).unwrap_or_else(|| panic!("{test_id:?} holds no editable"));
    editable.set_text(text);
    f.settle();
}

fn find_editable(widget: &gtk::Widget) -> Option<gtk::Editable> {
    if let Some(e) = widget.downcast_ref::<gtk::Text>() {
        return Some(e.clone().upcast());
    }
    let mut child = widget.first_child();
    while let Some(w) = child {
        if let Some(found) = find_editable(&w) {
            return Some(found);
        }
        child = w.next_sibling();
    }
    None
}

fn select(f: &Fixture, test_id: &str, index: u32) {
    f.send(test_id, EventType::Change, &serde_json::json!({ "index": index }).to_string());
}

fn add(f: &Fixture, product: &str, color: Option<u32>) {
    f.click(&format!("product-{product}"));
    f.wait_for("add-to-cart");
    if let Some(i) = color {
        select(f, "color", i);
    }
    f.click("add-to-cart");
    f.click("back");
}

fn browse_to_order() {
    let f = open();

    // A protocol tap: the first GTK click into the catalog's scroll view is
    // swallowed (reported to the Linux team).
    f.tap("product-mug");
    f.wait_for("add-to-cart");
    select(&f, "color", 1);
    f.click("quantity-plus");
    shoot(&f, "product");
    f.click("add-to-cart");
    f.click("back");
    add(&f, "throw", Some(1));
    add(&f, "planter", None);

    f.click("tab-search");
    type_text(&f, "search", "terracotta");
    f.wait_for("results");
    shoot(&f, "search");

    f.click("tab-cart");
    f.wait_for("cart");
    type_text(&f, "promo", "SUMMER24");
    f.click("promo-apply");
    f.wait_for("promo-error");
    shoot(&f, "cart");

    type_text(&f, "promo", "WELCOME10");
    f.click("promo-apply");
    f.click("checkout");
    f.wait_for("checkout-page");
    f.click("place-order");
    f.wait_for("error-street");
    shoot(&f, "checkout-errors");
    type_text(&f, "field-street", "12 Harbour Street");
    type_text(&f, "field-city", "Springfield");
    type_text(&f, "field-postal", "12345");
    f.click("place-order");
    f.wait_for("order-placed");
    shoot(&f, "order-placed");

    f.click("view-orders");
    f.wait_for("orders");
    shoot(&f, "account");
    f.click("order-LS-10481");
    f.wait_for("order");
    shoot(&f, "order");
}

fn empty_and_error_states() {
    let f = open();
    f.click("tab-cart");
    f.wait_for("cart-empty");
    shoot(&f, "cart-empty");

    f.click("tab-search");
    shoot(&f, "search-suggestions");
    type_text(&f, "search", "xylophone");
    f.wait_for("no-results");
    shoot(&f, "search-empty");

    // The order fails while the network is down, keeping what was typed.
    f.click("tab-shop");
    add(&f, "daypack", Some(1));
    f.click("tab-account");
    f.send("fail-network", EventType::Change, r#"{"checked":true}"#);
    f.click("tab-cart");
    f.click("checkout");
    f.wait_for("checkout-page");
    type_text(&f, "field-street", "12 Harbour Street");
    type_text(&f, "field-city", "Springfield");
    type_text(&f, "field-postal", "12345");
    f.click("place-order");
    f.wait_for("order-failed");
    shoot(&f, "checkout-failed");
    f.click("back");

    // Started again offline, the shop can't load its catalog.
    let options = InstanceOptions { data_dir: Some(f.data_dir.path().to_path_buf()), ..Default::default() };
    let dir = common::examples_dir().join("shop");
    let again = Fixture::start_with(&Runtime::new(), options, |instance| instance.load_dir(&dir));
    again.wait_for("catalog-error");
    shoot(&again, "catalog-error");
}
