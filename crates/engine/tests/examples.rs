//! The example programs, driven end to end through their main flows.

use std::path::PathBuf;

use native_shell::core::host::{Environment, HostRegistry};
use native_shell::core::protocol::{Command, Prop, PropValue, ViewType, ROOT_NODE};
use native_shell::testing::{Harness, HarnessBuilder};

fn example(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples").join(name)
}

fn build(name: &str) -> HarnessBuilder {
    Harness::from_dir(&example(name)).expect("example directory")
}

fn started(b: HarnessBuilder) -> Harness {
    let h = b.start();
    if let Some(e) = h.load_error() {
        panic!("{e}\n{:#?}", h.logs());
    }
    h
}

/// A fresh data directory for one test.
fn data_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("nsh-{name}-{}", std::process::id()));
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

fn assert_clean(h: &Harness) {
    assert!(h.error_logs().is_empty(), "{:?}", h.error_logs());
}

// Tasks ---------------------------------------------------------------------

#[test]
fn tasks_add_complete_edit_delete_undo_and_persist() {
    let dir = data_dir("tasks");
    let mut h = started(build("tasks").data_dir(&dir));
    // A first run shows sample tasks: five open (one overdue), one completed behind the disclosure.
    assert_eq!(h.text_of("subtitle"), "5 to do · 1 overdue");
    assert_eq!(h.view_type("tasks"), ViewType::List);
    assert!(h.find("task-1").is_some() && h.find("task-6").is_none());
    assert!(h.text_of("due-1").starts_with("Overdue"));

    // Add from the composer: Return adds and clears; the button is disabled while empty.
    assert!(disabled(&h, "add"));
    h.type_text("draft", "Water the plants");
    assert!(!disabled(&h, "add"), "{}", h.dump());
    h.submit("draft");
    assert_eq!(h.text_of("title-7"), "Water the plants");
    assert_eq!(h.native_input("draft").text, "");
    assert_eq!(h.text_of("subtitle"), "6 to do · 1 overdue");

    // Complete it; it moves behind the disclosure.
    h.tap("check-7");
    assert_eq!(h.text_of("subtitle"), "5 to do · 1 overdue");
    assert!(h.find("task-7").is_none());
    h.tap("toggle-completed");
    assert!(h.find("task-7").is_some() && h.find("task-6").is_some());

    // Edit: rename and clear the due date in the sheet.
    h.tap("task-3");
    assert_eq!(h.surfaces().len(), 2);
    h.type_text("editor-title", "Book the dentist for a cleaning");
    h.toggle("editor-due");
    assert!(h.find("editor-date").is_none());
    h.tap("editor-save");
    assert_eq!(h.surfaces().len(), 1);
    assert_eq!(h.text_of("title-3"), "Book the dentist for a cleaning");
    assert!(h.find("due-3").is_none());

    // Save is disabled while the name is empty.
    h.tap("task-3");
    h.type_text("editor-title", "  ");
    assert!(disabled(&h, "editor-save"));
    h.tap("editor-cancel");
    assert_eq!(h.text_of("title-3"), "Book the dentist for a cleaning");

    // Delete from the sheet, then undo; the bar goes away on its own.
    h.tap("task-4");
    h.tap("editor-delete");
    assert!(h.find("task-4").is_none());
    assert_eq!(h.text_of("undo-label"), "Deleted “Return the library books”");
    h.tap("undo");
    assert!(h.find("task-4").is_some() && h.find("undo-bar").is_none());
    h.tap("task-4");
    h.tap("editor-delete");
    h.advance(5100.0);
    assert!(h.find("undo-bar").is_none());

    // Clearing completed tasks names the count and asks first.
    h.tap("clear-completed");
    assert_eq!(h.alert().unwrap()["title"], "Delete 2 completed tasks?");
    h.alert_action(1);
    assert!(h.find("toggle-completed").is_none() && h.find("task-7").is_none());
    assert_clean(&h);

    // A new instance on the same data directory sees the saved list.
    drop(h);
    let h = started(build("tasks").data_dir(&dir));
    assert_eq!(h.text_of("title-3"), "Book the dentist for a cleaning");
    assert!(h.find("task-4").is_none() && h.find("toggle-completed").is_none());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn tasks_shows_an_empty_state() {
    let dir = data_dir("tasks-empty");
    let mut h = started(build("tasks").data_dir(&dir));
    for id in 1..=5 {
        h.tap(&format!("task-{id}"));
        h.tap("editor-delete");
    }
    h.tap("clear-completed");
    h.alert_action(1);
    assert!(h.find("empty").is_some(), "{}", h.dump());
    assert!(h.find("subtitle").is_none());
    let _ = std::fs::remove_dir_all(&dir);
}

/// The renderer's exact sequence, without the harness's own range reports: rows
/// appended to a short list appear before the renderer reports the new range.
#[test]
fn tasks_rows_survive_reported_list_ranges() {
    let dir = data_dir("tasks-range");
    let mut h = started(build("tasks").data_dir(&dir).emulate_list_ranges(false));
    h.scroll_list("tasks", 0, 6);
    let rows = |h: &Harness| h.native().children(h.find("tasks").unwrap()).len();
    assert_eq!(rows(&h), 6, "{}", h.dump());
    h.type_text("draft", "Water the plants");
    h.tap("add");
    assert!(h.texts().contains(&"Water the plants".to_string()), "{}", h.dump());
    h.scroll_list("tasks", 2, 7);
    assert_eq!(rows(&h), 5);
    assert!(h.find("task-1").is_none() && h.find("task-7").is_some());
    let _ = std::fs::remove_dir_all(&dir);
}

// Settings ------------------------------------------------------------------

#[test]
fn settings_toggles_pickers_account_and_confirmations() {
    let dir = data_dir("settings");
    let mut h = started(build("settings").data_dir(&dir));
    assert_eq!(h.view_type("settings"), ViewType::Scroll);
    assert_eq!(h.text_of("account-name"), "Ada Lovelace");

    // The daily summary depends on notifications: disabled, not hidden.
    assert!(h.find("summary-time").is_some());
    h.toggle("notifications");
    assert!(disabled(&h, "summary"));
    assert!(h.find("summary-time").is_none());
    h.toggle("notifications");
    h.pick_date("summary-time", "2026-10-14T07:45");

    // The theme applies at once and persists.
    let light = h.prop("account", Prop::BackgroundColor);
    h.select("theme", 2);
    assert_ne!(h.prop("account", Prop::BackgroundColor), light);

    // Clearing the cache waits for the measurement, then asks with the size.
    assert!(disabled(&h, "clear-cache"));
    h.advance(700.0);
    h.tap("clear-cache");
    assert_eq!(h.alert().unwrap()["title"], "Clear 184 MB of cached files?");
    h.alert_action(1);
    assert!(h.texts().contains(&"Zero KB".to_string()));

    // The account page: validation under the field, unsaved changes ask before leaving.
    h.tap("account");
    assert_eq!(h.surfaces().len(), 2);
    assert!(disabled(&h, "save"));
    h.type_text("email", "ada@example");
    h.submit("email");
    assert_eq!(h.text_of("email-error"), "Enter an email address like name@example.org.");
    h.tap("back");
    assert_eq!(h.alert().unwrap()["title"], "Discard changes?");
    h.alert_action(0);
    assert_eq!(h.surfaces().len(), 2, "Keep editing stays");
    h.type_text("email", "ada@example.net");
    h.tap("save");
    assert_eq!(h.surfaces().len(), 1);
    assert!(h.texts().contains(&"ada@example.net".to_string()));

    // Sign out confirms with the account named, and the card offers sign-in.
    h.tap("account");
    h.tap("sign-out");
    assert_eq!(h.alert().unwrap()["title"], "Sign out of ada@example.net?");
    h.alert_action(1);
    assert_eq!(h.surfaces().len(), 1);
    assert_eq!(h.text_of("account-name"), "Sign in");
    assert_clean(&h);

    // Everything above persisted.
    drop(h);
    let h = started(build("settings").data_dir(&dir));
    assert_eq!(h.text_of("account-name"), "Sign in");
    assert!(h.texts().iter().any(|t| t.contains("Delivered at")));
    let _ = std::fs::remove_dir_all(&dir);
}

// Chat ----------------------------------------------------------------------

#[test]
fn chat_sends_retries_loads_history_and_deletes_with_undo() {
    let mut h = started(build("chat"));
    assert_eq!(h.view_type("messages"), ViewType::List);
    assert!(h.find("failed-m8").is_some());

    // Return sends; delivery is confirmed after the round trip.
    assert!(disabled(&h, "send"));
    h.type_text("draft", "See you at the station");
    h.submit("draft");
    assert_eq!(h.native_input("draft").text, "");
    assert_eq!(h.text_of("delivery"), "Sending…");
    h.advance(800.0);
    assert_eq!(h.text_of("delivery"), "Delivered");

    // A failed message can be sent again.
    h.tap("retry-m8");
    assert!(h.find("failed-m8").is_none());
    h.advance(800.0);

    // Older messages load on request, then the start of the conversation is marked.
    h.scroll_list("messages", 0, 4);
    h.tap("load-earlier");
    assert!(h.find("history-loading").is_some());
    h.advance(1000.0);
    h.scroll_list("messages", 0, 4);
    assert!(h.find("history-start").is_some(), "{}", h.dump());

    // Message actions: copy, and delete with undo.
    h.scroll_list("messages", 0, 30);
    h.clear_commands();
    h.tap("bubble-m7");
    assert_eq!(h.surfaces().len(), 2);
    h.tap("action-copy");
    assert!(h.commands().iter().any(|(_, c, p)| *c == Command::SetClipboard && p.contains("first-aid kit")));
    h.long_press("bubble-m7");
    h.tap("action-delete");
    assert!(h.find("bubble-m7").is_none() && h.find("undo-bar").is_some());
    h.tap("undo");
    assert!(h.find("bubble-m7").is_some());
    assert_clean(&h);
}

// Watchlist -----------------------------------------------------------------

fn watchlist(b: HarnessBuilder, fail: bool) -> Harness {
    let registry: HostRegistry =
        serde_json::from_str(&std::fs::read_to_string(example("watchlist").join("host.json")).unwrap()).unwrap();
    started(
        b.registry(registry)
            .sync_module("demo/user", |method, _| match method {
                "current_user" => Ok(r#"{"id":"u1","name":"Ada Lovelace"}"#.into()),
                _ => Ok("null".into()),
            })
            .sync_module("demo/quote", |_, _| Ok("null".into()))
            .async_module("demo/quote", move |method, args| match method {
                "quote" if fail => Some(Err("offline".into())),
                "quote" => {
                    let symbol: Vec<String> = serde_json::from_str(args).ok()?;
                    let last = match symbol[0].as_str() {
                        "ORCA.US" => 112.0,
                        "KITE.US" => 107.0,
                        "HALO.US" => 103.0,
                        "MAPL.US" => 99.0,
                        "FERN.US" => 95.0,
                        _ => 101.0,
                    };
                    Some(Ok(serde_json::json!({ "symbol": symbol[0], "last": last, "prev_close": 100.0, "change_percent": last - 100.0 }).to_string()))
                }
                _ => Some(Ok("null".into())),
            }),
    )
}

/// The list index of the row holding `test_id`.
fn row_index(h: &Harness, list: &str, test_id: &str) -> i32 {
    let list = h.find(list).expect("list");
    let mut node = h.find(test_id).expect("row content");
    loop {
        let parent = h.native().nodes[&node].parent.expect("inside the list");
        if parent == list {
            return match h.native().prop(node, Prop::RowIndex) {
                Some(PropValue::I32(i)) => *i,
                other => panic!("a list row carries its index: {other:?}"),
            };
        }
        node = parent;
    }
}

#[test]
fn watchlist_uses_host_modules_and_components() {
    let dir = data_dir("watchlist");
    let mut h = watchlist(build("watchlist").data_dir(&dir), false);
    assert_eq!(h.text_of("subtitle"), "Ada Lovelace · 6 symbols");
    assert_eq!(h.text_of("status"), "Live");
    assert_eq!(h.view_type("cell-HALO.US"), ViewType::HostComponent);
    assert_eq!(
        h.prop("cell-HALO.US", Prop::HostProps),
        Some(PropValue::Json(r#"{"symbol":"HALO.US","name":"Halo Systems","last":103,"change_percent":3}"#.into()))
    );

    // Pushed ticks update the native cell.
    h.emit("demo/quote", "tick", r#"{"symbol":"HALO.US","last":101.5,"prev_close":100,"change_percent":1.5}"#);
    assert_eq!(
        h.prop("cell-HALO.US", Prop::HostProps),
        Some(PropValue::Json(r#"{"symbol":"HALO.US","name":"Halo Systems","last":101.5,"change_percent":1.5}"#.into()))
    );

    // Sorting by change puts the largest gain first.
    h.select("sort", 1);
    assert_eq!(row_index(&h, "watchlist", "cell-ORCA.US"), 0, "{}", h.dump());
    assert_eq!(row_index(&h, "watchlist", "cell-FERN.US"), 5);

    // Pausing drops the listener, so the host can stop the feed.
    h.tap("live");
    assert_eq!(h.text_of("status"), "Paused");
    assert!(h.calls().iter().any(|c| c.method == "__listeners" && c.args == r#"["tick",0]"#));
    h.tap("live");
    assert_eq!(h.text_of("status"), "Live");

    // A row opens its detail; removing it offers undo.
    h.host_event("cell-KITE.US", "select", "{}");
    assert_eq!(h.text_of("detail-price"), "107.00");
    h.tap("remove");
    assert!(h.find("cell-KITE.US").is_none() && h.find("undo-bar").is_some());
    h.tap("undo");
    assert!(h.find("cell-KITE.US").is_some());

    // Adding from the sheet subscribes the new symbol.
    h.tap("add");
    h.tap("add-LUMA.US");
    assert_eq!(h.text_of("subtitle"), "Ada Lovelace · 7 symbols");
    assert!(h.calls().iter().any(|c| c.method == "subscribe" && c.args.contains("LUMA.US")));
    assert_clean(&h);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn watchlist_shows_offline_and_empty_states() {
    let dir = data_dir("watchlist-states");
    let h = watchlist(build("watchlist").data_dir(&dir), true);
    assert!(h.find("error").is_some(), "{}", h.dump());
    assert_eq!(h.text_of("status"), "Offline");
    drop(h);

    let mut h = watchlist(build("watchlist").data_dir(&dir), false);
    for symbol in ["HALO.US", "MAPL.US", "KITE.US", "ORCA.US", "FERN.US", "PINE.HK"] {
        h.host_event(&format!("cell-{symbol}"), "select", "{}");
        h.tap("remove");
    }
    assert!(h.find("empty").is_some(), "{}", h.dump());
    assert!(h.find("sort").is_none());
    h.tap("empty-add");
    assert_eq!(h.surfaces().len(), 2);
    let _ = std::fs::remove_dir_all(&dir);
}

/// Row windows reported by the renderer keep the right host cells mounted.
#[test]
fn watchlist_rows_survive_reported_list_ranges() {
    let dir = data_dir("watchlist-range");
    let mut h = watchlist(build("watchlist").data_dir(&dir).emulate_list_ranges(false), false);
    let list = h.find("watchlist").unwrap();
    let rows = |h: &Harness| h.native().children(list).len();
    h.scroll_list("watchlist", 0, 6);
    assert_eq!(rows(&h), 6, "{}", h.dump());
    h.scroll_list("watchlist", 0, 2);
    assert_eq!(rows(&h), 2);
    assert!(h.find("cell-FERN.US").is_some() && h.find("cell-KITE.US").is_none());
    h.scroll_list("watchlist", 2, 6);
    assert_eq!(rows(&h), 4);
    assert!(h.find("cell-FERN.US").is_none() && h.find("cell-PINE.HK").is_some());
    let _ = std::fs::remove_dir_all(&dir);
}

// Components ----------------------------------------------------------------

#[test]
fn components_pages_show_their_states() {
    let mut h = started(build("components"));
    assert_eq!(h.view_type("index"), ViewType::Scroll);

    // Buttons: publish is disabled until there is text, then shows its wait.
    h.tap("open-buttons");
    assert_eq!(h.surfaces().len(), 2);
    assert!(disabled(&h, "publish"));
    h.type_text("post", "The tomatoes finally came in.");
    h.tap("publish");
    assert!(h.texts().contains(&"Publishing…".to_string()));
    h.advance(1300.0);
    assert!(h.find("published").is_some());
    h.tap("delete-account");
    assert_eq!(h.alert().unwrap()["title"], "Delete your account?");
    h.alert_action(0);
    h.tap("back");
    assert_eq!(h.surfaces().len(), 1);

    // Text fields: errors under the field on save, then success pops the page.
    h.tap("open-fields");
    for (id, text) in
        [("name", "Ada Lovelace"), ("street", "12 Harbour Street"), ("city", "Springfield"), ("postal", "123")]
    {
        h.type_text(id, text);
    }
    h.tap("save-address");
    assert_eq!(h.text_of("postal-error"), "Enter a 5-digit postal code.");
    h.type_text("postal", "12345");
    assert!(h.find("postal-error").is_none());
    h.tap("save-address");
    assert_eq!(h.surfaces().len(), 1);

    // Selection: rows are the controls; an unavailable option stays put.
    h.tap("open-selection");
    h.tap("email-digest");
    assert_eq!(h.prop("email-digest", Prop::A11yState), Some(PropValue::Json(r#"{"checked":true}"#.into())));
    h.tap("speed-same-day");
    h.tap("speed-express");
    assert_eq!(h.prop("speed-express", Prop::A11yState), Some(PropValue::Json(r#"{"checked":true}"#.into())));
    h.tap("back");

    // Sliders and progress: the value is written next to the slider; the upload counts up.
    h.tap("open-progress");
    h.slide("volume", &[55.0, 70.0]);
    assert_eq!(h.text_of("volume-value"), "70%");
    h.tap("upload-start");
    assert_eq!(h.text_of("upload-status"), "Preparing photos…");
    h.advance(910.0);
    h.advance(350.0 * 4.0);
    assert_eq!(h.text_of("upload-status"), "4 of 12 uploaded");
    h.tap("upload-cancel");
    assert_eq!(h.text_of("upload-status"), "12 photos · 48.6 MB");
    h.tap("back");

    // Pickers read the booking back.
    h.tap("open-pickers");
    h.select("sitting", 0);
    h.select("guests", 3);
    h.pick_date("date", "2026-12-24");
    assert_eq!(h.text_of("summary"), "Table for 4 on Thu, Dec 24 at 1:00 PM.");
    h.tap("request");
    assert!(h.find("requested").is_some());
    h.tap("back");

    // Overlays: sheet, dialog, toast, alert.
    h.tap("open-overlays");
    h.tap("share");
    assert!(h.find("share-sheet").is_some());
    h.dismiss();
    h.tap("rename");
    assert!(h.find("rename-dialog").is_some());
    h.type_text("rename-input", "Coast trip plan");
    h.tap("rename-save");
    assert_eq!(h.text_of("doc-name"), "Coast trip plan.pdf");
    h.clear_commands();
    h.tap("copy-link");
    assert!(h
        .commands()
        .iter()
        .any(|(n, c, p)| *n == ROOT_NODE && *c == Command::ShowToast && p.contains("Link copied")));
    h.tap("delete");
    assert_eq!(h.alert().unwrap()["title"], "Delete “Coast trip plan.pdf”?");
    h.alert_action(1);
    assert!(h.find("deleted").is_some());
    assert_clean(&h);
}

/// Commit buttons in page footers stay above the keyboard, on pushed pages too.
#[test]
fn components_footer_rises_with_the_keyboard() {
    let mut h = started(build("components"));
    h.tap("open-fields");
    let page = h.surfaces()[1].0;
    let bottom = |h: &Harness| {
        let f = h.frame("save-address");
        let mut y = f.y + f.height;
        let mut node = h.find("save-address").unwrap();
        while let Some(parent) = h.native().nodes[&node].parent.filter(|p| *p != page) {
            y += h.native().nodes[&parent].frame.map_or(0.0, |f| f.y);
            node = parent;
        }
        y
    };
    let resting = bottom(&h);
    h.set_environment(Environment { keyboard_height: 300.0, ..Environment::default() });
    assert!((resting - bottom(&h) - 300.0).abs() < 40.0, "{resting} -> {}\n{}", bottom(&h), h.dump());
}

// gpui-shell's own program --------------------------------------------------

#[test]
fn gpui_shell_todolist_runs_unchanged() {
    let dir = data_dir("gpui-todo");
    let mut h = started(build("gpui-todolist").data_dir(&dir));
    let texts = h.texts();
    assert!(texts.contains(&"Todo".to_string()) && texts.contains(&"Nothing yet".to_string()), "{texts:?}");
    let input = h.nodes_of(ViewType::TextInput)[0];
    h.type_text_node(input, "Buy milk");
    h.tap_text("Add");
    h.type_text_node(input, "Write tests");
    h.submit_node(input);
    assert!(h.texts().contains(&"2 of 2 remaining".to_string()), "{:?}", h.texts());
    assert_eq!(h.native_input_node(input).text, "", "the script cleared the field");

    // The row is gpui-base's headless Checkbox: a drawn indicator, toggled by a tap.
    h.tap_text("Buy milk");
    assert!(h.texts().contains(&"1 of 2 remaining".to_string()), "{:?}", h.texts());

    // Its icons are the program's SVG files, drawn as shapes in the text color.
    let paths: Vec<String> = h
        .nodes_of(ViewType::Shape)
        .into_iter()
        .filter_map(|n| match (h.native().prop(n, Prop::PathData), h.native().prop(n, Prop::ViewBox)) {
            (Some(PropValue::Str(d)), Some(PropValue::F32Array(b))) if b == &vec![0.0, 0.0, 24.0, 24.0] => {
                Some(d.clone())
            }
            _ => None,
        })
        .collect();
    assert!(paths.iter().any(|d| d.starts_with("M8 6L21 6")), "the list icon in the title: {paths:?}");
    assert!(paths.iter().any(|d| d == "M20 6L9 17L4 12"), "the check mark of the done row: {paths:?}");
    assert!(paths.iter().any(|d| d == "M12 5L12 19M5 12L19 12"), "the plus of the Add button: {paths:?}");
    let check = h
        .nodes_of(ViewType::Shape)
        .into_iter()
        .find(|&n| h.native().prop(n, Prop::PathData) == Some(&PropValue::Str("M20 6L9 17L4 12".into())));
    let check = check.expect("check icon");
    assert_eq!(h.native().prop(check, Prop::StrokeWidth), Some(&PropValue::F32(3.0)));
    assert!(h.native().prop(check, Prop::StrokeColor).is_some() && h.native().prop(check, Prop::FillColor).is_none());
    assert!(!h.logs().iter().any(|l| l.contains("nothing is drawn")), "{:?}", h.logs());
    h.tap_text("Done");
    assert!(h.find_text("Buy milk").is_some() && h.find_text("Write tests").is_none());
    h.tap_text("All");

    // Clear completed opens window.open_dialog; Delete runs the callback, closes it and toasts.
    h.clear_commands();
    h.tap_text("Clear completed…");
    assert_eq!(h.surfaces().len(), 2);
    assert!(h.find_text("Delete 1 completed item?").is_some(), "{}", h.dump());
    h.tap_text("Delete");
    assert_eq!(h.surfaces().len(), 1);
    assert!(h.find_text("Buy milk").is_none());
    assert!(
        h.commands().iter().any(|(_, c, p)| *c == Command::ShowToast && p.contains("Deleted 1 item")),
        "{:?}",
        h.commands()
    );
    assert_clean(&h);
    let _ = std::fs::remove_dir_all(&dir);
}
