//! The gpui-component form components on GTK, through the components
//! example's Forms page (a flower delivery checkout): text typed into framed
//! fields, the ribbon color sheet, the inset grouped payment form, and the
//! confirmation with its rating, in both appearances.

mod support;

use std::process::ExitCode;
use std::time::Duration;

use gtk::prelude::*;
use support::{example_runtime, pump_for, run, Fixture};

fn main() -> ExitCode {
    run(&[("components example: forms checkout", forms_checkout)])
}

fn forms_page() -> Fixture {
    gtk::Settings::default().expect("settings").set_gtk_enable_animations(false);
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/components");
    let read = |p: &str| std::fs::read_to_string(dir.join(p)).unwrap_or_else(|e| panic!("{p}: {e}"));
    let mut files: Vec<(String, String)> = vec![("main.js".into(), "export { default } from \"./pages/forms.js\";".into())];
    files.push(("ui.js".into(), read("ui.js")));
    for entry in std::fs::read_dir(dir.join("pages")).expect("pages").flatten() {
        let name = format!("pages/{}", entry.file_name().to_string_lossy());
        let text = read(&name);
        files.push((name, text));
    }
    let files: Vec<(&str, &str)> = files.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
    Fixture::with_runtime(&example_runtime(), &read("native-shell.json"), &files)
}

/// Types into the native field of `test_id` as the user would.
fn type_into(f: &Fixture, test_id: &str, text: &str) {
    let mut stack = vec![f.expect(test_id)];
    while let Some(w) = stack.pop() {
        if let Some(e) = w.dynamic_cast_ref::<gtk::Editable>() {
            e.set_text(text);
            f.settle();
            return;
        }
        let mut child = w.first_child();
        while let Some(c) = child {
            child = c.next_sibling();
            stack.push(c);
        }
    }
    panic!("no editable text in {test_id:?}");
}

fn screenshots(f: &Fixture, name: &str) {
    f.set_dark(false);
    f.screenshot(format!("screenshots/components-forms-{name}-light.png"));
    f.set_dark(true);
    f.screenshot(format!("screenshots/components-forms-{name}-dark.png"));
    f.set_dark(false);
}

fn forms_checkout() {
    let f = forms_page();
    f.click("continue");
    f.expect("name-error");
    screenshots(&f, "recipient-errors");

    type_into(&f, "name-input", "Jordan Lee");
    type_into(&f, "phone-input", "555 010 0199");
    assert!(f.find("name-error").is_none(), "an edit clears its field's error");
    f.click("ribbon-picker-swatch");
    f.wait_for("ribbon-picker-sheet");
    f.click("ribbon-picker-swatch-F6BF26");
    screenshots(&f, "ribbon");
    f.click("ribbon-picker-done");
    pump_for(Duration::from_millis(400));
    f.settle();

    f.click("continue");
    f.expect("payment");
    type_into(&f, "card-input", "4242 4242 4242 4242");
    type_into(&f, "expiry-input", "09/49");
    type_into(&f, "cvc-input", "123");
    type_into(&f, "promo-input", "SPRING10");
    f.click("apply");
    f.wait_for("promo-description");
    pump_for(Duration::from_millis(700));
    f.settle();
    screenshots(&f, "payment");

    f.click("continue");
    f.wait_for("confirmation");
    f.click("rating-star-4");
    let star = f.expect("rating-star-4");
    assert_eq!(star.accessible_role(), gtk::AccessibleRole::Button);
    f.expect("thanks");
    screenshots(&f, "done");
}
