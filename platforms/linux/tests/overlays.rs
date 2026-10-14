//! Pages, sheets, dialogs, alerts and toasts in the example programs, with
//! screenshots in both appearances.

mod support;

use std::process::ExitCode;
use std::time::Duration;

use adw::prelude::{AlertDialogExt, NavigationPageExt};
use support::*;

fn main() -> ExitCode {
    run(&[
        ("components: overlays page, sheet, toast, dialog, alert", components_overlays),
        ("settings: a dirty page refuses back until discarded", settings_back),
        ("tasks: the edit sheet", tasks_sheet),
    ])
}

/// Broadway never ticks the frame clock (see `support`), so transitions are
/// turned off and every presentation lands at once.
fn no_animations() {
    if let Some(settings) = gtk::Settings::default() {
        settings.set_gtk_enable_animations(false);
    }
}

fn nav(f: &Fixture) -> adw::NavigationView {
    f.find_where(|w| w.is::<adw::NavigationView>()).and_downcast().expect("the navigation view")
}

fn visible_title(f: &Fixture) -> String {
    nav(f).visible_page().map(|p| p.title().to_string()).unwrap_or_default()
}

fn modal_hosts(f: &Fixture) -> usize {
    let mut n = 0;
    let stage = nav(f).parent().expect("the stage");
    let mut child = stage.first_child();
    while let Some(w) = child {
        if w.type_().name() == "NshModalHost" {
            n += 1;
        }
        child = w.next_sibling();
    }
    n
}

/// The alert on screen: libadwaita presents it in its own window here (the
/// fixture's window is a plain `GtkWindow`).
fn alert() -> Option<adw::AlertDialog> {
    fn find(w: &gtk::Widget) -> Option<adw::AlertDialog> {
        if let Some(a) = w.downcast_ref::<adw::AlertDialog>() {
            return Some(a.clone());
        }
        let mut child = w.first_child();
        while let Some(c) = child {
            if let Some(a) = find(&c) {
                return Some(a);
            }
            child = c.next_sibling();
        }
        None
    }
    gtk::Window::list_toplevels().iter().find_map(find)
}

/// Clicks the alert button labelled `label`, as the user would.
fn choose(alert: &adw::AlertDialog, label: &str) {
    fn button(w: &gtk::Widget, label: &str) -> Option<gtk::Button> {
        if let Some(b) = w.downcast_ref::<gtk::Button>() {
            if b.label().as_deref() == Some(label) {
                return Some(b.clone());
            }
        }
        let mut child = w.first_child();
        while let Some(c) = child {
            if let Some(b) = button(&c, label) {
                return Some(b);
            }
            child = c.next_sibling();
        }
        None
    }
    button(alert.upcast_ref(), label).unwrap_or_else(|| panic!("no {label:?} button")).emit_clicked();
}

fn shoot(f: &Fixture, name: &str) {
    f.set_dark(false);
    f.screenshot(format!("screenshots/{name}-light.png"));
    f.set_dark(true);
    f.screenshot(format!("screenshots/{name}-dark.png"));
    f.set_dark(false);
}

fn components_overlays() {
    no_animations();
    let f = Fixture::open_dir("../../examples/components");
    f.tap("open-overlays");
    assert_eq!(visible_title(&f), "Overlays");
    shoot(&f, "overlays-page");

    // Sheet: medium detent, dismissed by the program after a choice.
    f.tap("share");
    f.wait_for("share-sheet");
    assert_eq!(modal_hosts(&f), 1);
    shoot(&f, "overlays-sheet");
    f.tap("share-message");
    assert_eq!(modal_hosts(&f), 0, "the sheet closed");

    // The toast it raised.
    assert!(f.find("nsh-toast").is_some_and(|t| t.is_visible()));
    shoot(&f, "overlays-toast");
    assert!(pump_until(Duration::from_secs(5), || f.find("nsh-toast").is_none()), "the toast retracts after its timeout");

    // Dialog: a one-field form.
    f.tap("rename");
    f.wait_for("rename-dialog");
    shoot(&f, "overlays-dialog");
    f.tap("rename-cancel");
    assert_eq!(modal_hosts(&f), 0);

    // Alert: native, destructive action in red; choosing reports the action.
    f.tap("delete");
    let a = alert().expect("an alert");
    assert_eq!(a.response_appearance("1"), adw::ResponseAppearance::Destructive);
    choose(&a, "Delete");
    f.settle();
    f.expect("deleted");
    assert!(alert().is_none_or(|a| !a.is_mapped()));

    // The user goes back (edge swipe, Escape, Alt+Left all pop the view).
    nav(&f).pop();
    f.settle();
    assert_eq!(visible_title(&f), "Home");
    f.expect("index");
}

fn settings_back() {
    no_animations();
    let f = Fixture::open_dir("../../examples/settings");
    f.tap("account");
    let title = visible_title(&f);
    assert!(title == "Account" || title == "Sign in", "{title}");
    shoot(&f, "settings-account");
    nav(&f).pop();
    f.settle();
    assert_eq!(visible_title(&f), "Home");
}

fn tasks_sheet() {
    no_animations();
    let f = Fixture::open_dir("../../examples/tasks");
    let task = f
        .find_where(|w| w.widget_name().starts_with("task-"))
        .map(|w| w.widget_name().to_string())
        .expect("a task row");
    f.tap(&task);
    f.wait_for("editor");
    assert_eq!(modal_hosts(&f), 1);
    shoot(&f, "tasks-editor-sheet");
}
