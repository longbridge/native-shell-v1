//! The notes example on GTK: its main screens driven through their flows,
//! their key states checked, and `screenshots/notes-<screen>-{light,dark}.png`
//! written under `NSH_SCREENSHOTS=1`.

mod support;

use std::path::Path;
use std::process::ExitCode;
use std::time::Duration;

use native_shell_gtk::InstanceOptions;
use support::*;

fn main() -> ExitCode {
    run(&[
        ("notes: folders and the new folder dialog", folders_and_new_folder),
        ("notes: a folder's notes and Select", list_and_select),
        ("notes: a note's long-press menu", note_menu),
        ("notes: a rendered checklist and its text", editor_checklist_and_text),
        ("notes: a long note", long_note),
        ("notes: search and no results", search_and_no_results),
        ("notes: an empty folder", empty_folder),
        ("notes: sharing a note", share),
        ("notes: large text", large_text),
    ])
}

/// The example on a fresh data directory, its sample library loaded.
fn open(dark: bool) -> Fixture {
    no_animations();
    set_dark(dark);
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/notes");
    let f = Fixture::start_with(&example_runtime(), InstanceOptions::default(), |i| i.load_dir(&dir));
    f.wait_for("folder-notes");
    f
}

fn no_animations() {
    if let Some(settings) = gtk::Settings::default() {
        settings.set_gtk_enable_animations(false);
    }
}

fn appearances() -> [bool; 2] {
    [false, true]
}

fn shot(f: &Fixture, name: &str, dark: bool) {
    // Sheets and dialogs finish presenting on the frame clock.
    pump_for(Duration::from_millis(300));
    f.settle();
    f.screenshot(format!("screenshots/notes-{name}-{}.png", if dark { "dark" } else { "light" }));
}

/// Types into the native text of the input or text area `test_id`.
fn type_into(f: &Fixture, test_id: &str, text: &str) {
    let field = f.expect(test_id);
    if let Some(entry) = f.find_where(|w| w.is::<gtk::Text>() && w.is_ancestor(&field)).and_downcast::<gtk::Text>() {
        entry.set_text(text);
    } else {
        let view = f
            .find_where(|w| w.is::<gtk::TextView>() && (w == &field || w.is_ancestor(&field)))
            .and_downcast::<gtk::TextView>()
            .unwrap_or_else(|| panic!("no native text in {test_id:?}"));
        view.buffer().set_text(text);
    }
    f.settle();
}

fn folders_and_new_folder() {
    for dark in appearances() {
        let f = open(dark);
        // Text and counts are checked by the engine's harness test; here, that every part is on screen.
        assert!(f.find("folder-all").is_some() && f.find("folder-f-work").is_some() && f.find("tag-groceries").is_some());
        shot(&f, "folders", dark);
        f.click("new-folder");
        type_into(&f, "folder-name", "Recipes");
        assert!(f.find("folder-name-error").is_some());
        shot(&f, "new-folder", dark);
        f.click("folder-save");
        assert!(f.find("folder-dialog").is_some(), "Save waits for a name no other folder has");
    }
}

fn list_and_select() {
    for dark in appearances() {
        let f = open(dark);
        f.click("folder-notes");
        assert!(f.find("group-pinned").is_some());
        assert!(f.find("note-n1").is_some() && f.find("note-n3").is_some());
        shot(&f, "list", dark);
        f.click("select");
        f.click("note-n3");
        assert!(f.find("note-menu").is_none(), "a tap in Select mode selects; it opens no menu");
        assert!(f.find("select-n3").is_some(), "rows show their selection marks");
        shot(&f, "select", dark);
    }
}

fn note_menu() {
    for dark in appearances() {
        let f = open(dark);
        f.click("folder-notes");
        f.long_press("note-n3");
        f.wait_for("note-menu");
        assert!(f.find("menu-delete").is_some());
        shot(&f, "note-menu", dark);
    }
}

fn editor_checklist_and_text() {
    for dark in appearances() {
        let f = open(dark);
        f.click("folder-notes");
        f.click("note-n1");
        assert!(f.find("note-menu").is_none(), "a tap opens the note, not its menu");
        assert!(f.find("rendered").is_some() && f.find("item-1").is_some());
        shot(&f, "editor-checklist", dark);
        f.click("toggle-preview");
        assert!(f.find("body").is_some());
        shot(&f, "editor-edit", dark);
    }
}

fn long_note() {
    for dark in appearances() {
        let f = open(dark);
        f.click("folder-f-work");
        f.click("note-n6");
        assert!(f.find("checklist-22").is_some());
        // List items wrap inside the page rather than running past its edge.
        let text = f.expect("text-0");
        assert!(f.bounds(&text).width() <= f.surface.width() as f32);
        shot(&f, "editor-long", dark);
    }
}

fn search_and_no_results() {
    for dark in appearances() {
        let f = open(dark);
        type_into(&f, "folders-search", "tram");
        assert!(f.find("results").is_some() && f.find("folders").is_none());
        assert!(f.find("note-n2").is_some() && f.find("note-n1").is_none());
        shot(&f, "search", dark);
        type_into(&f, "folders-search", "zebra");
        assert!(f.find("no-results").is_some());
        shot(&f, "no-results", dark);
    }
}

fn empty_folder() {
    for dark in appearances() {
        let f = open(dark);
        f.click("new-folder");
        type_into(&f, "folder-name", "Garden");
        f.click("folder-save");
        f.click("folder-f11");
        assert!(f.find("empty").is_some() && f.find("empty-compose").is_some());
        assert!(f.find("empty-compose-shortcut").is_some(), "a desktop shows the shortcut");
        shot(&f, "empty-folder", dark);
    }
}

fn share() {
    for dark in appearances() {
        let f = open(dark);
        f.click("folder-notes");
        f.click("note-n1");
        f.click("share");
        f.wait_for("share-sheet");
        assert!(f.find("share-preview").is_some() && f.find("copy-markdown").is_some());
        shot(&f, "share", dark);
    }
}

/// Twice the default text size: rows grow, the date stacks over the preview.
fn large_text() {
    let settings = gtk::Settings::default().expect("GTK settings");
    let dpi = settings.gtk_xft_dpi();
    settings.set_gtk_xft_dpi(96 * 1024 * 2);
    let outcome = std::panic::catch_unwind(|| {
        let f = open(false);
        f.click("folder-notes");
        shot(&f, "list-large", false);
        let f = open(true);
        f.click("folder-notes");
        f.click("note-n1");
        shot(&f, "editor-checklist-large", true);
    });
    settings.set_gtk_xft_dpi(dpi);
    if let Err(e) = outcome {
        std::panic::resume_unwind(e);
    }
}
