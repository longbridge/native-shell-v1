//! The notes example, driven through its main flows: folders, writing,
//! checklists, search, deleting with undo, persistence and its error states.

use std::path::{Path, PathBuf};

use native_shell::core::protocol::{Command, EventType, Prop, PropValue, ROOT_NODE};
use native_shell::testing::Harness;

fn example() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples/notes")
}

/// A fresh data directory for one test.
fn data_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("nsh-notes-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

/// The example on `dir`, with its library loaded.
fn open(dir: &Path) -> Harness {
    let mut h = Harness::from_dir(&example()).expect("example directory").data_dir(dir).start();
    if let Some(e) = h.load_error() {
        panic!("{e}\n{:#?}", h.logs());
    }
    h.advance(10.0);
    h
}

fn disabled(h: &Harness, test_id: &str) -> bool {
    match h.prop(test_id, Prop::A11yState) {
        Some(PropValue::Json(j)) => j.contains(r#""disabled":true"#),
        _ => h.prop(test_id, Prop::Enabled) == Some(PropValue::Bool(false)),
    }
}

fn assert_clean(h: &Harness) {
    assert!(h.error_logs().is_empty(), "{:?}", h.error_logs());
}

fn toasts(h: &Harness) -> Vec<String> {
    h.commands()
        .iter()
        .filter(|(n, c, _)| *n == ROOT_NODE && *c == Command::ShowToast)
        .map(|(_, _, p)| p.clone())
        .collect()
}

fn key(h: &mut Harness, node: u32, keystroke: &str) {
    let (mods, key) = keystroke.rsplit_once('-').unwrap_or(("", keystroke));
    let modifiers: serde_json::Map<String, serde_json::Value> =
        mods.split('-').filter(|m| !m.is_empty()).map(|m| (m.to_string(), true.into())).collect();
    let payload = serde_json::json!({ "key": key, "modifiers": modifiers, "phase": "down" });
    h.dispatch(node, EventType::Key, &payload.to_string());
}

#[test]
fn notes_folders_writing_search_and_undo() {
    let dir = data_dir("flow");
    let mut h = open(&dir);

    // A first run shows the sample library: every note, the default folder and three more.
    assert_eq!(h.text_of("count-all"), "10");
    assert_eq!(h.text_of("count-notes"), "4");
    assert!(h.find("folder-f-recipes").is_some() && h.find("tag-travel").is_some());
    assert_eq!(h.text_of("folders-status"), "10 Notes");

    // New folder: Save waits for a name no other folder has.
    h.tap("new-folder");
    assert!(h.find("folder-dialog").is_some());
    assert!(disabled(&h, "folder-save"));
    h.type_text("folder-name", "recipes");
    assert_eq!(h.text_of("folder-name-error"), "A folder named “recipes” already exists.");
    assert!(disabled(&h, "folder-save"));
    h.type_text("folder-name", "Garden");
    assert!(h.find("folder-name-error").is_none());
    h.submit("folder-name");
    assert!(h.find("folder-dialog").is_none());
    assert_eq!(h.text_of("folder-f11"), "Garden 0");

    // An empty folder says so; writing a note there files it there.
    h.tap("folder-f11");
    assert_eq!(h.surfaces().len(), 2);
    assert!(h.find("empty").is_some() && h.find("empty-compose").is_some());
    assert!(disabled(&h, "select"));
    h.tap("notes-compose");
    assert_eq!(h.surfaces().len(), 3);
    assert!(h.find("body").is_some(), "a new note opens for writing\n{}", h.dump());
    assert!(disabled(&h, "toggle-preview") && disabled(&h, "more"));
    h.type_text("body", "Seeds to order\n- [ ] Tomatoes\n- [ ] Basil\n\n#garden");
    h.tap("editor-back");
    assert_eq!(h.text_of("title-n12"), "Seeds to order");
    assert_eq!(h.text_of("notes-status"), "1 Note");
    assert!(h.find("group-today").is_some());

    // The note opens rendered: its checklist ticks off in place.
    h.tap("note-n12");
    assert!(h.find("body").is_none() && h.find("rendered").is_some());
    h.tap("item-1");
    assert_eq!(h.prop("item-1", Prop::A11yState), Some(PropValue::Json(r#"{"checked":true}"#.into())));
    h.tap("toggle-preview");
    assert_eq!(h.native_input("body").text, "Seeds to order\n- [x] Tomatoes\n- [ ] Basil\n\n#garden");
    // The checklist button starts an item under the line the cursor is on.
    h.tap("add-item");
    assert!(h.native_input("body").text.ends_with("#garden\n- [ ] "), "{:?}", h.native_input("body").text);
    h.tap("toggle-preview");
    assert!(h.find("item-5").is_some());

    // Pin from the editor's menu.
    h.tap("more");
    h.tap("menu-pin");
    h.back();
    assert!(h.find("group-pinned").is_some() && h.find("group-today").is_none());
    h.back();
    assert_eq!(h.surfaces().len(), 1);
    assert_eq!(h.text_of("count-f11"), "1");
    assert!(h.find("tag-garden").is_some());

    // Search across every note marks what matched, and says when nothing does.
    h.type_text("folders-search", "basil");
    assert_eq!(h.text_of("results"), h.text_of("note-n12"));
    assert!(h.find("folders").is_none());
    h.type_text("folders-search", "basil lisbon");
    assert!(h.find("no-results").is_some());
    h.type_text("folders-search", "tram");
    assert!(h.text_of("detail-n2").contains("Tram 28"), "{}", h.text_of("detail-n2"));
    h.tap("folders-search-clear");
    assert!(h.find("folders").is_some());

    // Delete from a note's menu (a long press), then undo from the toast.
    h.tap("folder-f11");
    h.long_press("note-n12");
    assert!(h.find("note-menu").is_some());
    h.clear_commands();
    h.tap("menu-delete");
    assert!(h.find("note-n12").is_none() && h.find("empty").is_some());
    assert!(
        toasts(&h).iter().any(|t| t.contains("Deleted “Seeds to order”") && t.contains("Undo")),
        "{:?}",
        toasts(&h)
    );
    h.toast_action("deleted");
    assert_eq!(h.text_of("title-n12"), "Seeds to order");
    h.back();

    // Deleting a folder names it and asks first; its notes go with it.
    h.long_press("folder-f11");
    h.tap("folder-delete");
    assert_eq!(h.alert().unwrap()["title"], "Delete “Garden”?");
    h.alert_action(1);
    assert!(h.find("folder-f11").is_none());
    assert_eq!(h.text_of("count-all"), "10");
    assert_clean(&h);

    // The library was written; a new instance on the same directory reads it back.
    h.advance(500.0);
    drop(h);
    let h = open(&dir);
    assert!(h.find("folder-f11").is_none());
    assert_eq!(h.text_of("count-all"), "10");
    assert!(h.find("tag-garden").is_none());
    assert_clean(&h);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn notes_select_rename_move_and_shortcuts() {
    let dir = data_dir("select");
    let mut h = open(&dir);

    // Rename from Edit mode, where every user folder opens its menu.
    h.tap("edit");
    h.tap("folder-f-work");
    h.tap("folder-rename");
    assert_eq!(h.native_input("folder-name").text, "Work");
    h.type_text("folder-name", "Office");
    h.tap("folder-save");
    assert_eq!(h.text_of("folder-f-work"), "Office 2");
    h.tap("edit");

    // Select two notes, move them, then select and delete one with undo.
    h.tap("folder-notes");
    h.tap("select");
    assert!(disabled(&h, "delete-selected"));
    h.tap("note-n3");
    h.tap("note-n9");
    assert_eq!(h.text_of("notes-status"), "2 Selected");
    h.tap("move-selected");
    h.tap("move-to-f-work");
    assert!(h.find("note-n3").is_none() && h.find("note-n9").is_none());
    assert_eq!(h.text_of("notes-status"), "2 Notes");
    h.tap("select");
    h.tap("note-n10");
    h.clear_commands();
    h.tap("delete-selected");
    assert!(h.find("note-n10").is_none());
    assert!(toasts(&h).iter().any(|t| t.contains("Deleted “Bike maintenance”")));
    h.toast_action("deleted");
    assert!(h.find("note-n10").is_some());

    // Share copies the note.
    h.tap("note-n1");
    h.tap("share");
    assert!(
        h.text_of("share-preview").starts_with("Groceries\n○ Oat milk\n✓ Sourdough loaf"),
        "{}",
        h.text_of("share-preview")
    );
    h.tap("copy-markdown");
    assert!(h.find("share-sheet").is_none());

    // Delete from the editor's menu goes back to the list, with undo.
    h.tap("more");
    h.tap("menu-delete");
    assert_eq!(h.surfaces().len(), 2);
    assert!(h.find("note-n1").is_none());
    h.toast_action("deleted");
    assert!(h.find("note-n1").is_some());

    // A hardware keyboard: ⌘N writes a new note from the list; ⌘F searches.
    let page = h.find("notes-page").unwrap();
    key(&mut h, page, "cmd-n");
    assert_eq!(h.surfaces().len(), 3);
    assert!(h.find("body").is_some());
    // An untouched new note is never stored.
    h.back();
    assert_eq!(h.text_of("notes-status"), "2 Notes");
    h.back();
    key(&mut h, ROOT_NODE, "shift-cmd-n");
    assert!(h.find("folder-dialog").is_some());
    h.tap("folder-cancel");
    assert_clean(&h);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn notes_shows_long_content_and_its_error_state() {
    // An unreadable library is reported and left as it is.
    let dir = data_dir("corrupt");
    std::fs::create_dir_all(dir.join("files")).unwrap();
    std::fs::write(dir.join("files/notes.json"), "{ not json").unwrap();
    let mut h = open(&dir);
    assert!(h.find("load-error").is_some());
    assert!(disabled(&h, "folders-compose") && disabled(&h, "new-folder"));
    std::fs::write(dir.join("files/notes.json"), r#"{"version":1,"nextId":3,"folders":[],"notes":[{"id":"n1","folder":"gone","body":"Kept","pinned":false,"updated":0}]}"#).unwrap();
    h.tap("retry");
    h.advance(10.0);
    // A note whose folder is missing lands in the default folder.
    assert_eq!(h.text_of("count-notes"), "1");
    drop(h);

    // The long planning note renders its sections and checklist.
    let dir2 = data_dir("long");
    let mut h = open(&dir2);
    h.tap("folder-f-work");
    h.tap("note-n6");
    assert!(h.find("checklist-22").is_some(), "{}", h.dump());
    assert!(h.find("text-0").is_some());
    h.scroll("note-scroll", 2000.0);
    assert_clean(&h);
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::remove_dir_all(&dir2);
}
