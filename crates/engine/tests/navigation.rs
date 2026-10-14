//! Pages, sheets, dialogs, alerts, toasts, theme.

use native_shell::core::host::Environment;
use native_shell::core::protocol::{Command, Mutation, Prop, PropValue, ViewType, ROOT_NODE};
use native_shell::core::theme::{Appearance, Theme, COLOR_TOKENS};
use native_shell::testing::Harness;

const APP: &str = r##"
import { View, div, v_flex, Button } from "native-shell";
class Detail extends View {
  init(props, cx) { this.id = props.id; this.n = 0; }
  render(cx) {
    return v_flex().size_full()
      .child(div().test_id("detail").child(`detail ${this.id} n=${this.n}`))
      .child(Button.new("inc").test_id("detail-inc").label("+").on_click((_e, cx) => { this.n += 1; cx.notify(); }))
      .child(Button.new("title").test_id("rename").label("rename").on_click((_e, cx) => cx.set_title("Renamed")))
      .child(Button.new("lock").test_id("lock").label("lock").on_click((_e, cx) => {
        cx.set_can_go_back(false);
        cx.on_back((cx) => { this.n = 100; cx.notify(); });
      }))
      .child(Button.new("pop").test_id("pop").label("pop").on_click((_e, cx) => cx.navigator.pop()));
  }
}
export default class App extends View {
  init() { this.log = []; this.count = 0; }
  note(s, cx) { this.log.push(s); cx.notify(); }
  render(cx) {
    const t = cx.theme();
    return v_flex().size_full().bg(t.colors.background)
      .child(div().test_id("log").child(this.log.join(",")))
      .child(div().test_id("swatch").size_4().bg("primary"))
      .child(Button.new("push").test_id("push").label("push").on_click((_e, cx) => cx.navigator.push(Detail, { id: 7 }, { title: "Detail" })))
      .child(Button.new("sheet").test_id("sheet").label("sheet").on_click((_e, cx) => {
        cx.open_sheet(() => v_flex().p_4().child(div().test_id("sheet-count").child(`count ${this.count}`))
          .child(Button.new("more").test_id("more").label("more").on_click((_e, cx) => { this.count += 1; cx.notify(); }))
          .child(Button.new("close").test_id("close-sheet").label("close").on_click((_e, cx) => cx.close_sheet())),
          { detents: ["medium"], on_dismiss: (cx) => this.note("sheet dismissed", cx) });
      }))
      .child(Button.new("dialog").test_id("dialog").label("dialog").on_click((_e, cx) => {
        cx.open_dialog(() => div().p_4().test_id("dialog-body").child("Are you sure?"), { dismissable: false });
      }))
      .child(Button.new("alert").test_id("alert").label("alert").on_click((_e, cx) => {
        cx.alert({ title: "Delete?", message: "Gone for good", actions: [
          { label: "Cancel", style: "cancel", on_press: (cx) => this.note("cancel", cx) },
          { label: "Delete", style: "destructive", on_press: (cx) => this.note("deleted", cx) },
        ] });
      }))
      .child(Button.new("toast").test_id("toast").label("toast").on_click((_e, cx) => { cx.push_toast({ title: "Saved", level: "success" }); cx.haptic("success"); }))
      .child(Button.new("dark").test_id("dark").label("dark").on_click((_e, cx) => cx.set_theme({ appearance: "dark" })))
      .child(Button.new("brand").test_id("brand").label("brand").on_click((_e, cx) => cx.set_theme({ tokens: { primary: "#ff0000" } })))
      .child(Button.new("open").test_id("open").label("open").on_click((_e, cx) => cx.open_url("https://example.com")))
      .child(div().test_id("window").child(`${cx.window().width}x${cx.window().height} ${cx.window().appearance}`));
  }
}
"##;

fn app() -> Harness {
    let h = Harness::new(&[("main.js", APP)]);
    assert!(h.load_error().is_none(), "{:?} {:?}", h.load_error(), h.logs());
    h
}

#[test]
fn pages_push_present_and_pop() {
    let mut h = app();
    h.clear_commands();
    h.tap("push");
    let surfaces = h.surfaces();
    assert_eq!(surfaces.len(), 2);
    let (page, kind) = surfaces[1];
    assert_eq!(kind, ViewType::Page);
    assert_eq!(h.native().prop(page, Prop::Title), Some(&PropValue::Str("Detail".into())));
    assert_eq!(h.native().prop(page, Prop::CanGoBack), Some(&PropValue::Bool(true)));
    assert!(h.commands().contains(&(page, Command::PresentPage, r#"{"animated":true}"#.into())));
    assert_eq!(h.text_of("detail"), "detail 7 n=0");
    h.tap("detail-inc");
    assert_eq!(h.text_of("detail"), "detail 7 n=1");
    h.tap("rename");
    assert_eq!(h.native().prop(page, Prop::Title), Some(&PropValue::Str("Renamed".into())));

    h.tap("pop");
    // DismissPage, then the Delete, in one batch.
    assert!(h.commands().contains(&(page, Command::DismissPage, r#"{"animated":true}"#.into())));
    assert_eq!(h.surfaces().len(), 1);
    assert!(h.find("detail").is_none());
}

#[test]
fn back_pops_unless_the_page_takes_it() {
    let mut h = app();
    h.tap("push");
    h.back();
    assert_eq!(h.surfaces().len(), 1, "system back popped the page");

    h.tap("push");
    h.tap("lock");
    let page = h.surfaces()[1].0;
    assert_eq!(h.native().prop(page, Prop::CanGoBack), Some(&PropValue::Bool(false)));
    h.back();
    assert_eq!(h.surfaces().len(), 2);
    assert_eq!(h.text_of("detail"), "detail 7 n=100");

    // A native swipe-back (Dismiss) always removes it.
    h.dismiss();
    assert_eq!(h.surfaces().len(), 1);
}

#[test]
fn a_page_can_take_back_from_its_init() {
    let mut h = Harness::new(&[(
        "main.js",
        r##"
import { View, div, Button } from "native-shell";
class Editor extends View {
  init(_props, cx) {
    this.asked = false;
    cx.set_can_go_back(false);
    cx.on_back((cx) => { this.asked = true; cx.notify(); });
  }
  render() { return div().test_id("editor").child(this.asked ? "asked" : "editing"); }
}
export default class App extends View {
  render() {
    return div().child(Button.new("push").test_id("push").label("push")
      .on_click((_e, cx) => cx.navigator.push(Editor, {}, { title: "Edit" })));
  }
}
"##,
    )]);
    assert!(h.load_error().is_none(), "{:?}", h.load_error());
    h.tap("push");
    let page = h.surfaces()[1].0;
    assert_eq!(h.native().prop(page, Prop::CanGoBack), Some(&PropValue::Bool(false)));
    h.back();
    assert_eq!(h.surfaces().len(), 2, "the page kept back");
    assert_eq!(h.text_of("editor"), "asked");
    // The root surface was not affected.
    assert_eq!(h.native().prop(ROOT_NODE, Prop::CanGoBack), None);
}

#[test]
fn sheets_re_evaluate_with_their_opener_and_dismiss() {
    let mut h = app();
    h.tap("sheet");
    let (sheet, kind) = h.surfaces()[1];
    assert_eq!(kind, ViewType::Sheet);
    assert_eq!(h.native().prop(sheet, Prop::Detents), Some(&PropValue::Json(r#"["medium"]"#.into())));
    assert_eq!(h.text_of("sheet-count"), "count 0");
    h.tap("more");
    assert_eq!(h.text_of("sheet-count"), "count 1", "the content function ran again when the opener re-rendered");
    h.dismiss();
    assert_eq!(h.surfaces().len(), 1);
    assert_eq!(h.text_of("log"), "sheet dismissed");

    h.tap("sheet");
    h.tap("close-sheet");
    assert_eq!(h.surfaces().len(), 1);
}

#[test]
fn dialogs_and_alerts() {
    let mut h = app();
    h.tap("dialog");
    let (dialog, kind) = h.surfaces()[1];
    assert_eq!(kind, ViewType::Dialog);
    assert_eq!(h.native().prop(dialog, Prop::Dismissable), Some(&PropValue::Bool(false)));
    assert_eq!(h.text_of("dialog-body"), "Are you sure?");
    h.dismiss();

    h.tap("alert");
    let spec = h.alert().expect("an alert");
    assert_eq!(spec["title"], "Delete?");
    assert_eq!(spec["actions"][1]["style"], "destructive");
    h.alert_action(1);
    assert_eq!(h.text_of("log"), "deleted");
    assert_eq!(h.surfaces().len(), 1);

    h.tap("alert");
    h.dismiss();
    assert_eq!(h.text_of("log"), "deleted,cancel", "dismissing runs the cancel action");
}

#[test]
fn toast_haptic_and_open_url_are_root_commands() {
    let mut h =
        Harness::with_manifest(&[("main.js", APP)], r#"{"id":"t.nav","name":"Nav","capabilities":{"open_url":false}}"#);
    h.clear_commands();
    h.tap("toast");
    let cmds: Vec<_> = h.commands().iter().map(|(n, c, _)| (*n, *c)).collect();
    assert_eq!(cmds, vec![(ROOT_NODE, Command::ShowToast), (ROOT_NODE, Command::Haptic)]);
    assert!(h.commands()[0].2.contains("\"level\":\"success\""));
    h.clear_commands();
    h.tap("open");
    assert!(h.commands().is_empty(), "open_url without the capability is refused");
    assert!(h.error_logs().join("\n").contains("capabilities.open_url"));

    let mut h = app();
    h.clear_commands();
    h.tap("open");
    assert_eq!(h.commands(), &[(ROOT_NODE, Command::OpenUrl, r#"{"url":"https://example.com"}"#.to_string())]);
}

const UNDO: &str = r#"
import { View, div, v_flex, Button } from "native-shell";
export default class App extends View {
  init() { this.items = ["a", "b"]; this.removed = null; }
  render(cx) {
    return v_flex()
      .child(div().test_id("items").child(this.items.join(",")))
      .child(Button.new("del").test_id("del").label("Delete").on_click((_e, cx) => {
        this.removed = this.items.pop();
        cx.notify();
        window.push_toast({
          id: "undo",
          title: `Deleted "${this.removed}"`,
          timeout: 4000,
          action: { label: "Undo", on_click: (cx) => { this.items.push(this.removed); cx.notify(); } },
        });
      }))
      .child(Button.new("forever").test_id("forever").label("Forever").on_click((_e, cx) => {
        cx.push_toast({ id: "sticky", title: "Kept", timeout: null, action: { label: "Restore", on_click: (cx) => {
          this.items = ["a", "b"];
          cx.notify();
        } } });
      }))
      .child(Button.new("drop").test_id("drop").label("Drop").on_click(() => window.remove_toast("sticky")));
  }
}
"#;

#[test]
fn a_toast_action_runs_its_handler_with_an_event_cx() {
    let mut h = Harness::new(&[("main.js", UNDO)]);
    h.clear_commands();
    h.tap("del");
    assert_eq!(h.text_of("items"), "a");
    let (node, command, payload) = h.commands()[0].clone();
    assert_eq!((node, command), (ROOT_NODE, Command::ShowToast));
    let payload: serde_json::Value = serde_json::from_str(&payload).unwrap();
    assert_eq!(payload["action"], serde_json::json!({ "label": "Undo" }));
    assert_eq!(payload["duration"], 4000);

    h.advance(3000.0);
    h.toast_action("undo");
    assert_eq!(h.text_of("items"), "a,b", "the handler ran and could notify");
    assert!(h.error_logs().is_empty(), "{:?}", h.error_logs());

    let dropped = h.instance().metrics().events_dropped;
    h.toast_action("undo");
    assert_eq!(h.instance().metrics().events_dropped, dropped + 1, "a toast acts once: the tap dismissed it");
}

#[test]
fn an_expired_or_retracted_toast_drops_its_action() {
    let mut h = Harness::new(&[("main.js", UNDO)]);
    h.tap("del");
    h.advance(4000.0);
    let dropped = h.instance().metrics().events_dropped;
    h.toast_action("undo");
    assert_eq!(h.text_of("items"), "a", "the expired toast's handler is gone");
    assert_eq!(h.instance().metrics().events_dropped, dropped + 1);

    // Without a timeout the handler lives until the toast is retracted.
    h.tap("del");
    h.tap("forever");
    h.advance(60_000.0);
    h.toast_action("sticky");
    assert_eq!(h.text_of("items"), "a,b");
    h.tap("del");
    h.tap("forever");
    h.tap("drop");
    h.toast_action("sticky");
    assert_eq!(h.text_of("items"), "a", "remove_toast released the handler");
    assert_eq!(h.instance().metrics().events_dropped, dropped + 2);
    assert!(h.error_logs().is_empty(), "{:?}", h.error_logs());
}

#[test]
fn a_bad_toast_action_is_a_type_error() {
    let mut h = Harness::new(&[(
        "main.js",
        r#"
import { View, Button } from "native-shell";
export default class App extends View {
  render(cx) {
    return Button.new("t").test_id("t").label("t").on_click(() => window.push_toast({ title: "x", action: { label: "Undo" } }));
  }
}
"#,
    )]);
    h.clear_commands();
    h.tap("t");
    assert!(h.commands().is_empty());
    assert!(h.error_logs().join("\n").contains("`action.on_click` must be a function"), "{:?}", h.error_logs());
}

const SETTINGS: &str = r#"
import { View, div, v_flex, Button } from "native-shell";
class Account extends View {
  render(cx) {
    return v_flex()
      .child(Button.new("save").test_id("save").label("Save").on_click((_e, cx) => this.props.save("bob", cx)))
      .child(Button.new("done").test_id("done").label("Done").on_click((_e, cx) => cx.navigator.pop()));
  }
}
export default class Settings extends View {
  init() { this.account = "alice"; this.renders = 0; }
  render(cx) {
    this.renders += 1;
    const me = cx.entity();
    return v_flex()
      .child(div().test_id("account").child(`${this.account} r=${this.renders} same=${me === cx.entity()}`))
      .child(Button.new("open").test_id("open").label("Account").on_click((_e, cx) => {
        cx.navigator.push(Account, { save: (account, cx) => { this.account = account; cx.notify(me); } });
      }))
      .child(Button.new("open2").test_id("open2").label("Account").on_click((_e, cx) => {
        this.child = cx.new(Account, { save: () => {} });
        this.kept = cx.entity();
        this.child.release();
        cx.notify(this.child);
      }));
  }
}
"#;

#[test]
fn cx_entity_lets_a_pushed_page_redraw_the_view_below() {
    let mut h = Harness::new(&[("main.js", SETTINGS)]);
    assert_eq!(h.text_of("account"), "alice r=1 same=true");
    h.tap("open");
    h.tap("save");
    assert_eq!(h.text_of("account"), "bob r=2 same=true", "cx.notify(entity) from the page re-rendered Settings");
    h.tap("done");
    assert_eq!(h.text_of("account"), "bob r=3 same=true", "the surface a pop reveals draws again");
    h.tap("open2");
    assert!(h.error_logs().is_empty(), "notifying a released entity is a no-op: {:?}", h.error_logs());
}

/// A default theme color, so tests follow palette changes.
fn color(appearance: Appearance, token: &str) -> PropValue {
    PropValue::Color(Theme::default_for(appearance).color(token).expect("a default token"))
}

#[test]
fn theme_switch_re_renders_with_new_colors() {
    let mut h = app();
    assert_eq!(h.prop("swatch", Prop::BackgroundColor), Some(color(Appearance::Light, "primary")));
    h.tap("dark");
    assert_eq!(h.prop("swatch", Prop::BackgroundColor), Some(color(Appearance::Dark, "primary")));
    h.tap("brand");
    assert_eq!(h.prop("swatch", Prop::BackgroundColor), Some(PropValue::Color(0xFFFF_0000)));
}

#[test]
fn environment_changes_re_render_and_follow_the_system_appearance() {
    let mut h = app();
    assert_eq!(h.text_of("window"), "390x844 light");
    h.set_environment(Environment {
        width: 400.0,
        height: 800.0,
        appearance: Appearance::Dark,
        ..Environment::default()
    });
    assert_eq!(h.text_of("window"), "400x800 dark");
    assert_eq!(h.prop("swatch", Prop::BackgroundColor), Some(color(Appearance::Dark, "primary")));
}

#[test]
fn a_popped_page_gets_dismiss_page_then_delete_in_one_batch() {
    let mut h = app();
    h.tap("push");
    let page = h.surfaces()[1].0;
    h.clear_mutations();
    h.tap("pop");
    let m = h.mutations();
    let begin = m.iter().position(|x| matches!(x, Mutation::BeginBatch { .. })).unwrap();
    assert!(
        matches!(&m[begin + 1], Mutation::Command { node, command: Command::DismissPage, .. } if *node == page),
        "DismissPage comes right after BeginBatch: {m:#?}"
    );
    let delete =
        m.iter().position(|x| matches!(x, Mutation::Delete { node } if *node == page)).expect("the page is deleted");
    let end = m.iter().position(|x| matches!(x, Mutation::EndBatch)).unwrap();
    assert!(delete < end, "in the same batch");

    // A page the user swiped away gets no DismissPage.
    h.tap("push");
    h.clear_mutations();
    h.dismiss();
    assert!(!h.mutations().iter().any(|x| matches!(x, Mutation::Command { command: Command::DismissPage, .. })));
    assert_eq!(h.surfaces().len(), 1);
}

#[test]
fn host_theme_tokens_apply_per_appearance_under_script_overrides() {
    use native_shell::ThemeConfig;
    let theme = ThemeConfig {
        light: [("primary".to_string(), "#2563eb".to_string())].into(),
        dark: [("primary".to_string(), "#60a5fa".to_string())].into(),
    };
    let mut h = Harness::build(&[("main.js", APP)]).theme(theme).start();
    assert_eq!(h.prop("swatch", Prop::BackgroundColor), Some(PropValue::Color(0xFF25_63EB)));
    h.tap("dark");
    assert_eq!(h.prop("swatch", Prop::BackgroundColor), Some(PropValue::Color(0xFF60_A5FA)));
    h.tap("brand");
    assert_eq!(h.prop("swatch", Prop::BackgroundColor), Some(PropValue::Color(0xFFFF_0000)), "the script's tokens win");

    let bad = ThemeConfig { light: [("nope".to_string(), "#fff".to_string())].into(), ..ThemeConfig::default() };
    let h = Harness::build(&[("main.js", APP)]).theme(bad).start();
    assert!(h.load_error().unwrap().to_string().contains("nope"));
}

#[test]
fn the_root_carries_the_resolved_theme_tokens_when_they_change() {
    let main = r##"
import { View, div, v_flex, Button } from "native-shell";
export default class T extends View {
  render(cx) {
    return v_flex()
      .child(Button.new("brand").test_id("brand").label("brand").on_click((_e, cx) => cx.set_theme({ tokens: { colors: { primary: "#ff0000" } } })))
      .child(Button.new("same").test_id("same").label("same").on_click((_e, cx) => cx.set_theme({ tokens: { colors: { primary: "#ff0000" } } })))
      .child(Button.new("dark").test_id("dark").label("dark").on_click((_e, cx) => cx.set_theme({ appearance: "dark", tokens: { colors: { primary: "#ff0000" } } })));
  }
}
"##;
    let mut h = Harness::new(&[("main.js", main)]);
    let first = h.theme_tokens().to_vec();
    assert_eq!(first.len(), 1, "the first batch carries the theme");
    assert_eq!(first[0]["appearance"], "light");
    let light = Theme::default_for(Appearance::Light);
    assert_eq!(first[0]["background"], light.color("background").unwrap());
    assert_eq!(first[0].as_object().unwrap().len(), COLOR_TOKENS.len() + 1, "every color and the appearance");

    h.tap("brand");
    assert_eq!(h.theme_tokens().len(), 2);
    assert_eq!(h.theme_tokens()[1]["primary"], 0xFFFF_0000u32);
    h.tap("same");
    assert_eq!(h.theme_tokens().len(), 2, "an unchanged theme is not sent again");
    h.tap("dark");
    let dark = h.theme_tokens().last().unwrap().clone();
    assert_eq!(dark["appearance"], "dark");
    assert_eq!(dark["primary"], 0xFFFF_0000u32);
    assert_ne!(dark["background"], first[0]["background"]);
    assert_eq!(
        h.native().root_props().get(&Prop::ThemeTokens),
        Some(&PropValue::Json(dark.to_string())),
        "the renderer holds the latest tokens on the root"
    );

    // The system appearance changes under a script that does not pin one.
    let mut h = Harness::new(&[("main.js", main)]);
    h.set_environment(Environment { appearance: Appearance::Dark, ..Environment::default() });
    assert_eq!(h.theme_tokens().last().unwrap()["appearance"], "dark");
}

#[test]
fn the_window_says_whether_the_system_confirms_copies() {
    let main = r#"
import { View, div } from "native-shell";
export default class C extends View {
  render(cx) { return div().test_id("c").child(`${cx.window().clipboard_feedback}`); }
}
"#;
    let mut h = Harness::new(&[("main.js", main)]);
    assert_eq!(h.text_of("c"), "false");
    h.set_environment(Environment { clipboard_feedback: true, ..Environment::default() });
    assert_eq!(h.text_of("c"), "true", "a change re-renders with the new value");
}

#[test]
fn rem_follows_the_clamped_font_scale_and_line_height_the_full_one() {
    let main = r#"
import { View, div, v_flex } from "native-shell";
export default class R extends View {
  render(cx) {
    return v_flex().items_start()
      .child(div().test_id("metrics").child(`${window.rem_size()} ${window.line_height().toFixed(2)} ${"rem_size" in cx.window()}`))
      .child(div().test_id("padded").p_4().child(div().w(10).h(10)));
  }
}
"#;
    let mut h = Harness::new(&[("main.js", main)]);
    assert_eq!(h.text_of("metrics"), "16 25.89 false", "window.rem_size() is the one way to read rem");
    assert_eq!(h.frame("padded").width, 10.0 + 2.0 * 16.0);
    h.set_environment(Environment { font_scale: 2.0, ..Environment::default() });
    assert_eq!(h.text_of("metrics"), "25.6 51.78 false", "rem stops at 1.6×; text lines take the full 2×");
    let width = h.frame("padded").width;
    // Frames snap to device pixels.
    assert!((width - (10.0 + 2.0 * 25.6)).abs() < 0.5, "p_4 is 1rem a side: {width}");
}

#[test]
fn typography_keeps_gpui_shell_keys_and_adds_the_ios_text_styles() {
    let main = r#"
import { View, div } from "native-shell";
export default class T extends View {
  render(cx) {
    const t = cx.theme().typography;
    const s = (r) => `${r.size}/${r.line_height}/${r.weight}`;
    return div().test_id("t").child([t.sans, s(t.md), t.mono, s(t.large_title), s(t.headline), s(t.body), s(t.caption2)].join(" "));
  }
}
"#;
    let h = Harness::new(&[("main.js", main)]);
    assert_eq!(h.text_of("t"), "system-ui 16/24/400 monospace 34/41/700 17/22/600 17/22/400 11/13/400");
}

#[test]
fn the_settings_example_needs_no_window_refresh_after_its_account_page() {
    // examples/settings with its `window.refresh()` calls removed: the page's
    // pop redraws the settings below it (and `cx.notify(cx.entity())` captured
    // before the push would too).
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/settings");
    let mut files = Vec::new();
    for entry in std::fs::read_dir(&dir).unwrap() {
        let path = entry.unwrap().path();
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        if let Ok(text) = std::fs::read_to_string(&path) {
            files.push((name, text.replace("window.refresh();", "")));
        }
    }
    assert!(
        files.iter().any(|(n, t)| n == "main.js" && t.contains("save: (account, cx)")),
        "the scenario still exists"
    );
    let refs: Vec<(&str, &str)> = files.iter().map(|(n, t)| (n.as_str(), t.as_str())).collect();
    let data = std::env::temp_dir().join(format!("nsh-refresh-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&data);
    let mut h = Harness::build(&refs).data_dir(&data).start();
    assert!(h.load_error().is_none(), "{:?}", h.load_error());
    h.tap("account");
    h.type_text("email", "ada@example.net");
    h.tap("save");
    assert_eq!(h.surfaces().len(), 1);
    assert!(h.texts().contains(&"ada@example.net".to_string()), "{:?}", h.texts());
    h.tap("account");
    h.tap("sign-out");
    h.alert_action(1);
    assert_eq!(h.text_of("account-name"), "Sign in");
    let _ = std::fs::remove_dir_all(&data);
}

/// Toasts belong to the window: a page that deletes and closes leaves an Undo that still works.
#[test]
fn a_toast_action_outlives_the_page_that_pushed_it() {
    let mut h = Harness::new(&[(
        "main.js",
        r#"
import { View, div, v_flex, Button } from "native-shell";
const items = ["a", "b"];
let list = null;
class Detail extends View {
  render() {
    return Button.new("delete").test_id("delete").label("Delete").on_click((_e, cx) => {
      const gone = items.pop();
      cx.navigator.pop();
      cx.push_toast({ id: "undo", title: "Deleted", action: { label: "Undo", on_click: (cx) => { items.push(gone); cx.notify(list); } } });
    });
  }
}
export default class App extends View {
  init(_p, cx) { list = cx.entity(); }
  render() {
    return v_flex()
      .child(div().test_id("items").child(items.join(",")))
      .child(Button.new("open").test_id("open").label("Open").on_click((_e, cx) => cx.navigator.push(Detail)));
  }
}
"#,
    )]);
    h.tap("open");
    h.tap("delete");
    assert_eq!(h.surfaces().len(), 1);
    assert_eq!(h.text_of("items"), "a");
    h.toast_action("undo");
    assert_eq!(h.text_of("items"), "a,b");
    assert!(h.error_logs().is_empty(), "{:?}", h.error_logs());
}

