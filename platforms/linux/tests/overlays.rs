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
        ("settings: an edited page refuses back until discarded", settings_back),
        ("tasks: the edit sheet", tasks_sheet),
        ("an alert follows the program's appearance", alert_appearance),
        ("transitions run: push, sheet spring, pop", animated_transitions),
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

/// `name-light.png` or `name-dark.png`.
fn shot(f: &Fixture, name: &str, dark: bool) {
    f.screenshot(format!("screenshots/{name}-{}.png", if dark { "dark" } else { "light" }));
}

fn shot_alert(alert: &adw::AlertDialog, name: &str, dark: bool) {
    let path = format!("{}/screenshots/{name}-{}.png", env!("CARGO_MANIFEST_DIR"), if dark { "dark" } else { "light" });
    // The alert's own window paints its background.
    let window = alert.root().and_downcast::<gtk::Window>().expect("the alert's window");
    screenshot(window.upcast_ref(), std::path::Path::new(&path));
}

fn components_overlays() {
    no_animations();
    for dark in [false, true] {
        // Each appearance from a fresh start, as a user would see it.
        set_dark(dark);
        let f = Fixture::open_dir("../../examples/components");
        f.tap("open-overlays");
        assert_eq!(visible_title(&f), "Overlays");
        shot(&f, "overlays-page", dark);

        // Sheet: medium detent, closed by the program after a choice.
        f.tap("share");
        f.wait_for("share-sheet");
        assert_eq!(modal_hosts(&f), 1);
        shot(&f, "overlays-sheet", dark);
        f.tap("share-message");
        assert_eq!(modal_hosts(&f), 0, "the sheet closed");

        // The toast it raised.
        assert!(f.find("nsh-toast").is_some());
        shot(&f, "overlays-toast", dark);
        assert!(pump_until(Duration::from_secs(5), || f.find("nsh-toast").is_none()), "the toast retracts after its timeout");

        // Dialog: a one-field form; back (Escape) dismisses it.
        f.tap("rename");
        f.wait_for("rename-dialog");
        shot(&f, "overlays-dialog", dark);
        assert!(f.surface.go_back());
        f.settle();
        assert_eq!(modal_hosts(&f), 0);
        assert!(f.find("rename-dialog").is_none());

        // Alert: native, the destructive action styled; choosing reports it.
        f.tap("delete");
        let a = alert().expect("an alert");
        assert_eq!(a.response_appearance("1"), adw::ResponseAppearance::Destructive);
        shot_alert(&a, "overlays-alert", dark);
        choose(&a, "Delete");
        f.settle();
        f.expect("deleted");
        assert!(alert().is_none_or(|a| !a.is_mapped()));

        // Back pops the page.
        assert!(f.surface.go_back());
        f.settle();
        assert_eq!(visible_title(&f), "Home");
        f.expect("index");
        assert!(!f.surface.go_back(), "nothing left to go back from");
    }
    set_dark(false);
}

/// Types into the native text field of the input `test_id`.
fn type_into(f: &Fixture, test_id: &str, text: &str) {
    let field = f.expect(test_id);
    let editable = f
        .find_where(|w| w.is::<gtk::Text>() && w.is_ancestor(&field))
        .and_downcast::<gtk::Text>()
        .expect("a native text field");
    editable.set_text(text);
    f.settle();
}

fn settings_back() {
    no_animations();
    for dark in [false, true] {
        set_dark(dark);
        let f = Fixture::open_dir("../../examples/settings");
        f.tap("account");
        let title = visible_title(&f);
        assert!(title == "Account" || title == "Sign in", "{title}");
        shot(&f, "settings-account", dark);

        // An edit makes the page refuse back: BackPressed, and the page asks first.
        type_into(&f, "name", "Alex Kim");
        assert!(!nav(&f).visible_page().expect("the account page").can_pop(), "gestures and shortcuts are off");
        assert!(f.surface.go_back());
        f.settle();
        assert_eq!(visible_title(&f), title, "the page stays");
        let a = alert().expect("the discard alert");
        assert_eq!(a.heading().as_deref(), Some("Discard changes?"));
        shot_alert(&a, "settings-discard-alert", dark);
        // Keep editing: the alert goes, the page stays.
        choose(&a, "Keep Editing");
        f.settle();
        assert_eq!(visible_title(&f), title);
        // Back again, discard: the page pops.
        assert!(f.surface.go_back());
        f.settle();
        choose(&alert().expect("the discard alert again"), "Discard Changes");
        f.settle();
        assert_eq!(visible_title(&f), "Home");
    }
    set_dark(false);
}

fn tasks_sheet() {
    no_animations();
    for dark in [false, true] {
        set_dark(dark);
        let f = Fixture::open_dir("../../examples/tasks");
        let task = f
            .find_where(|w| w.widget_name().starts_with("task-"))
            .map(|w| w.widget_name().to_string())
            .expect("a task row");
        f.tap(&task);
        f.wait_for("editor");
        assert_eq!(modal_hosts(&f), 1);
        shot(&f, "tasks-editor-sheet", dark);
    }
    set_dark(false);
}

fn alert_appearance() {
    no_animations();
    set_dark(false);
    let f = Fixture::new(
        r#"
import { View, div, Button } from "native-shell";
export default class App extends View {
  render(cx) {
    return div().size_full().child(Button.new("ask").test_id("ask").label("Ask").on_click((_e, cx) => {
      cx.set_theme({ appearance: "dark" });
      cx.alert({ title: "Delete “Groceries”?", message: "This can’t be undone.",
        actions: [{ label: "Cancel", style: "cancel" }, { label: "Delete", style: "destructive" }] });
    }));
  }
}"#,
    );
    f.tap("ask");
    let a = alert().expect("an alert");
    let window = a.root().and_downcast::<gtk::Window>().expect("the alert's window");
    let w = window.width();
    let path = format!("{}/screenshots/alert-program-dark.png", env!("CARGO_MANIFEST_DIR"));
    screenshot(window.upcast_ref(), std::path::Path::new(&path));
    let bg = pixel(window.upcast_ref(), w / 2, 8);
    let luma = ((bg >> 16) & 0xff) + ((bg >> 8) & 0xff) + (bg & 0xff);
    assert!(luma < 3 * 0x60, "a dark alert on a light desktop, got {bg:08x}");
    choose(&a, "Cancel");
}

/// With the frame clock running, the springs reach their end states.
fn animated_transitions() {
    if let Some(settings) = gtk::Settings::default() {
        settings.set_gtk_enable_animations(true);
    }
    set_dark(false);
    let f = Fixture::open_dir("../../examples/components");
    let index = f.expect("index");
    f.tap("open-overlays");
    assert!(pump_until(Duration::from_secs(3), || !index.is_mapped()), "the push transition ends and hides the list");

    f.tap("share");
    let sheet = f.wait_for("share-sheet");
    // Medium detent: the card rests with its top at half the surface height.
    let rest = f.surface.height() as f32 / 2.0;
    assert!(
        pump_until(Duration::from_secs(3), || {
            f.layout();
            (f.bounds(&sheet).y() - rest).abs() < 1.0
        }),
        "the sheet springs up to its detent, at {}",
        f.bounds(&sheet).y()
    );
    assert!(f.surface.go_back());
    assert!(pump_until(Duration::from_secs(3), || f.find("share-sheet").is_none_or(|w| !w.is_mapped())), "the sheet slides away");
    f.settle();

    assert!(f.surface.go_back());
    assert!(pump_until(Duration::from_secs(3), || index.is_mapped()), "the pop reveals the list");
    f.settle();
    no_animations();
}
