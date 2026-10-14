//! "gpui-component" data and navigation components: DescriptionList, Tree,
//! List, TabBar, Breadcrumb, Settings, GroupBox, Calendar, Carousel, menus,
//! Toolbar and Icon.

use native_shell::core::host::Environment;
use native_shell::core::protocol::{Command, EventType, Prop, PropValue, ViewType};
use native_shell::core::theme::{Appearance, Theme};
use native_shell::testing::Harness;

const IMPORTS: &str = r#"
import { View, div, v_flex, h_flex, text, Switch } from "native-shell";
import { DescriptionList, DescriptionItem, Tree, TreeItem, List, TabBar, Tabs, Tab, Breadcrumb, Settings, SettingPage,
  SettingGroup, SettingItem, GroupBox, Calendar, CalendarState, Carousel, CarouselState, CarouselContent, CarouselItem,
  CarouselPrevious, CarouselNext, CarouselPagination, CarouselPaginationItem, Menu, MenuItem, MenuSeparator, DropdownMenu,
  DropdownButton, Toolbar, Icon } from "gpui-component";
"#;

fn program(body: &str) -> String {
    format!("{IMPORTS}\n{body}")
}

fn start(body: &str) -> Harness {
    started(Harness::new(&[("main.js", &program(body))]))
}

fn started(h: Harness) -> Harness {
    assert!(h.load_error().is_none(), "{:?} {:?}", h.load_error(), h.logs());
    assert!(h.error_logs().is_empty(), "{:?}", h.error_logs());
    h
}

/// The error a program's first render reports.
fn render_error(body: &str) -> String {
    let h = Harness::new(&[("main.js", &program(body))]);
    let mut all = h.error_logs();
    if let Some(e) = h.load_error() {
        all.push(format!("{e:?}"));
    }
    all.join("\n")
}

fn str_prop(h: &Harness, node: u32, prop: Prop) -> Option<String> {
    match h.native().prop(node, prop) {
        Some(PropValue::Str(s)) => Some(s.clone()),
        Some(PropValue::Json(s)) => Some(s.clone()),
        _ => None,
    }
}

/// The node showing `text`, or the nearest ancestor of it that carries an accessibility label.
fn labeled(h: &Harness, text: &str) -> u32 {
    let mut node = h.find_text(text).unwrap_or_else(|| panic!("no text {text:?}\n{}", h.dump()));
    while str_prop(h, node, Prop::A11yLabel).is_none() {
        node = h.native().nodes[&node].parent.unwrap_or_else(|| panic!("no labeled ancestor of {text:?}"));
    }
    node
}

fn a11y_state(h: &Harness, node: u32) -> serde_json::Value {
    str_prop(h, node, Prop::A11yState).map(|s| serde_json::from_str(&s).unwrap()).unwrap_or_default()
}

fn log(h: &Harness) -> String {
    h.text_of("log")
}

// Description list ---------------------------------------------------------------

const ORDER: &str = r#"
export default class C extends View {
  render() {
    return v_flex()
      .child(new DescriptionList().test_id("dl")
        .child(new DescriptionItem("Order number").value("W4471-2289"))
        .child(DescriptionItem.new("Placed").value("Oct 12, 2026"))
        .item("Total", "$84.20"))
      .child(new DescriptionList().vertical().columns(2).bordered(false).test_id("grid")
        .child(new DescriptionItem("Carrier").value("Royal Mail"))
        .child(new DescriptionItem("Tracking").value("RM 22 334"))
        .child(new DescriptionItem("Note").value("Leave with the neighbour").span(2)));
  }
}
"#;

#[test]
fn description_list_lays_out_label_value_rows_on_a_card() {
    let h = start(ORDER);
    assert_eq!(h.text_of("dl"), "Order number W4471-2289 Placed Oct 12, 2026 Total $84.20");
    let row = labeled(&h, "W4471-2289");
    assert_eq!(str_prop(&h, row, Prop::A11yLabel).as_deref(), Some("Order number, W4471-2289"));
    // The card is the surface color; rows are separated by hairlines (2 between 3 rows).
    let surface = Theme::default_for(Appearance::Light).color("surface").unwrap();
    assert_eq!(h.prop("dl", Prop::BackgroundColor), Some(PropValue::Color(surface)));
    assert_eq!(h.native().children(h.find("dl").unwrap()).len(), 5);
    // Rows keep iOS's 44 pt minimum.
    assert!(h.native().nodes[&row].frame.unwrap().height >= 44.0);
}

#[test]
fn description_list_vertical_layout_is_a_grid_with_spans() {
    let h = start(ORDER);
    assert_eq!(h.prop("grid", Prop::BackgroundColor), None, "not bordered: no card");
    // Carrier and Tracking side by side, then the note across both columns.
    let frame = |text: &str| h.native().nodes[&labeled(&h, text)].frame.unwrap();
    let (carrier, tracking, note) = (frame("Royal Mail"), frame("RM 22 334"), frame("Leave with the neighbour"));
    assert_eq!(carrier.y, tracking.y);
    assert!(tracking.x > carrier.x);
    assert!(note.y > carrier.y);
    assert!(note.width > carrier.width * 1.9, "{} vs {}", note.width, carrier.width);
}

#[test]
fn description_list_refuses_other_children_and_bad_columns() {
    let err = render_error("export default class C extends View { render() { return new DescriptionList().child(div()); } }");
    assert!(err.contains("DescriptionList accepts only DescriptionItem children"), "{err}");
    let err = render_error("export default class C extends View { render() { return new DescriptionList().columns(11); } }");
    assert!(err.contains("from 1 through 10"), "{err}");
}

// Tree ------------------------------------------------------------------------

const FILES: &str = r#"
export default class C extends View {
  init() { this.log = []; }
  render() {
    return v_flex()
      .child(text(this.log.join(",")).test_id("log"))
      .child(new Tree("files").test_id("tree")
        .on_select((id, cx) => { this.log.push(id); cx.notify(); })
        .child(new TreeItem("docs", "Documents").icon("folder").expanded(true)
          .child(new TreeItem("plan", "Garden plan.pdf"))
          .child(new TreeItem("tax", "Taxes").child(new TreeItem("receipts", "Receipts.pdf"))))
        .child(new TreeItem("photos", "Photos").child(new TreeItem("lisbon", "Lisbon.jpg")))
        .child(new TreeItem("locked", "Archive").disabled(true)));
  }
}
"#;

#[test]
fn tree_shows_expanded_folders_and_toggles_on_tap() {
    let mut h = start(FILES);
    assert_eq!(h.text_of("tree"), "Documents Garden plan.pdf Taxes Photos Archive");
    let docs = labeled(&h, "Documents");
    assert_eq!(a11y_state(&h, docs)["expanded"], true);
    assert_eq!(str_prop(&h, docs, Prop::A11yValue).as_deref(), Some("level 1, 2 items"));
    assert_eq!(a11y_state(&h, labeled(&h, "Photos")).get("expanded"), None, "collapsed folders announce no expanded state");

    h.tap_text("Photos");
    assert_eq!(h.text_of("tree"), "Documents Garden plan.pdf Taxes Photos Lisbon.jpg Archive");
    assert_eq!(log(&h), "photos");
    assert_eq!(a11y_state(&h, labeled(&h, "Photos"))["selected"], true);
    assert_eq!(str_prop(&h, labeled(&h, "Lisbon.jpg"), Prop::A11yValue).as_deref(), Some("level 2"));

    // Collapsing keeps the choice across renders.
    h.tap_text("Documents");
    assert_eq!(h.text_of("tree"), "Documents Photos Lisbon.jpg Archive");
    assert_eq!(log(&h), "photos,docs");
}

#[test]
fn tree_disabled_items_cannot_be_selected() {
    let mut h = start(FILES);
    let archive = labeled(&h, "Archive");
    assert_eq!(a11y_state(&h, archive)["disabled"], true);
    h.tap_text("Archive");
    assert_eq!(log(&h), "");
}

#[test]
fn tree_arrow_keys_move_the_selection_and_open_folders() {
    let mut h = start(FILES);
    let key = |h: &mut Harness, text: &str, key: &str| {
        let node = labeled(h, text);
        h.dispatch(node, EventType::Key, &format!(r#"{{"key":"{key}","phase":"down"}}"#));
    };
    key(&mut h, "Garden plan.pdf", "down");
    assert_eq!(log(&h), "tax");
    key(&mut h, "Taxes", "right");
    assert_eq!(h.text_of("tree"), "Documents Garden plan.pdf Taxes Receipts.pdf Photos Archive");
    key(&mut h, "Taxes", "right");
    assert_eq!(log(&h), "tax,receipts", "right on an open folder moves into it");
    key(&mut h, "Receipts.pdf", "left");
    assert_eq!(log(&h), "tax,receipts,tax", "left on a leaf moves to its folder");
    key(&mut h, "Taxes", "left");
    assert_eq!(h.text_of("tree"), "Documents Garden plan.pdf Taxes Photos Archive");
    key(&mut h, "Photos", "down");
    assert_eq!(log(&h), "tax,receipts,tax", "disabled rows are skipped: nothing below Photos");
}

#[test]
fn tree_selected_item_reveals_a_selection_made_elsewhere_once() {
    let mut h = start(
        r#"
export default class C extends View {
  init() { this.sel = "receipts"; }
  render() {
    return v_flex()
      .child(div().test_id("pick").child("Pick").on_click((_e, cx) => { this.sel = "lisbon"; cx.notify(); }))
      .child(new Tree("files").test_id("tree").selected_item(this.sel).on_select((id, cx) => { this.sel = id; cx.notify(); })
        .child(new TreeItem("docs", "Documents").child(new TreeItem("tax", "Taxes").child(new TreeItem("receipts", "Receipts.pdf"))))
        .child(new TreeItem("photos", "Photos").child(new TreeItem("lisbon", "Lisbon.jpg"))));
  }
}
"#,
    );
    assert_eq!(h.text_of("tree"), "Documents Taxes Receipts.pdf Photos");
    assert_eq!(a11y_state(&h, labeled(&h, "Receipts.pdf"))["selected"], true);
    // The reader may close the folder around the selection; it stays closed.
    h.tap_text("Documents");
    assert_eq!(h.text_of("tree"), "Documents Photos");
    h.tap("pick");
    assert_eq!(h.text_of("tree"), "Documents Photos Lisbon.jpg");
    assert_eq!(a11y_state(&h, labeled(&h, "Lisbon.jpg"))["selected"], true);
}

// List ------------------------------------------------------------------------------

#[test]
fn list_renders_a_snapshot_of_rows_keyed_by_id() {
    let mut h = start(
        r#"
export default class C extends View {
  init() { this.rows = [{ id: "a", name: "Apples" }, { id: "b", name: "Bread" }]; this.log = []; }
  render() {
    return v_flex().size_full()
      .child(text(this.log.join(",")).test_id("log"))
      .child(new List("groceries", () => this.rows, (row) => div().child(row.name)).test_id("list").flex_1()
        .on_item_click((key, cx) => { this.log.push(key); cx.notify(); }));
  }
}
"#,
    );
    assert_eq!(h.view_type("list"), ViewType::List);
    assert_eq!(h.prop("list", Prop::ItemCount), Some(PropValue::I32(2)));
    assert!(h.texts().contains(&"Bread".to_string()));
    h.tap_text("Bread");
    assert_eq!(log(&h), "b");

    let err = render_error(r#"export default class C extends View { render() { return new List("l", [], () => div()); } }"#);
    assert!(err.contains("rows must be a function"), "{err}");
}

// Tab bar ---------------------------------------------------------------------------

#[test]
fn tab_bar_marks_the_selected_tab_and_reports_taps() {
    let mut h = start(
        r#"
export default class C extends View {
  init() { this.tab = 1; this.log = []; }
  render() {
    return v_flex()
      .child(text(this.log.join(",")).test_id("log"))
      .child(new TabBar("sections").test_id("bar").selected_index(this.tab)
        .on_change((i, cx) => { this.tab = i; this.log.push(i); cx.notify(); })
        .child(new Tab().label("Overview").test_id("t0"))
        .child(new Tab().label("Items").test_id("t1"))
        .child(new Tab().label("Returns").disabled(true).test_id("t2")));
  }
}
"#,
    );
    assert_eq!(a11y_state(&h, h.find("t1").unwrap())["selected"], true);
    assert_eq!(a11y_state(&h, h.find("t0").unwrap()).get("selected"), None);
    let primary = Theme::default_for(Appearance::Light).color("primary").unwrap();
    assert_eq!(h.prop("t1", Prop::BorderColor), Some(PropValue::Color(primary)), "the tinted underline");
    h.tap("t0");
    assert_eq!(log(&h), "0");
    assert_eq!(a11y_state(&h, h.find("t0").unwrap())["selected"], true);
    h.tap("t2");
    assert_eq!(log(&h), "0", "a disabled tab does nothing");
}

#[test]
fn segmented_tab_bar_is_the_native_segmented_control() {
    let mut h = start(
        r#"
export default class C extends View {
  init() { this.tab = 0; }
  render() {
    return v_flex()
      .child(text(`tab ${this.tab}`).test_id("log"))
      .child(Tabs.new("range").variant("segmented").test_id("seg").selected_index(this.tab)
        .on_change((i, cx) => { this.tab = i; cx.notify(); })
        .child(new Tab().label("Day")).child(new Tab().label("Week")).child(new Tab().label("Month")));
  }
}
"#,
    );
    assert_eq!(h.view_type("seg"), ViewType::Segmented);
    assert_eq!(h.prop("seg", Prop::Items), Some(PropValue::Json(r#"["Day","Week","Month"]"#.into())));
    let seg = h.find("seg").unwrap();
    h.dispatch(seg, EventType::Change, r#"{"index":2,"value":"Month"}"#);
    assert_eq!(log(&h), "tab 2");
    assert_eq!(h.prop("seg", Prop::SelectedIndex), Some(PropValue::I32(2)));

    let err = render_error(r#"export default class C extends View { render() { return new TabBar("t").child(div()); } }"#);
    assert!(err.contains("TabBar accepts only Tab children"), "{err}");
}

// Breadcrumb -------------------------------------------------------------------------

#[test]
fn breadcrumb_goes_back_to_an_earlier_place() {
    let mut h = start(
        r#"
export default class C extends View {
  init() { this.log = []; }
  render() {
    return v_flex()
      .child(text(this.log.join(",")).test_id("log"))
      .child(new Breadcrumb(["Files", "Projects", "Garden plan"]).test_id("path").on_item_click((i, cx) => { this.log.push(i); cx.notify(); }));
  }
}
"#,
    );
    assert_eq!(h.text_of("path"), "Files Projects Garden plan");
    assert_eq!(str_prop(&h, labeled(&h, "Garden plan"), Prop::A11yLabel).as_deref(), Some("Garden plan, current"));
    h.tap_text("Projects");
    h.tap_text("Garden plan");
    assert_eq!(log(&h), "1", "the current place is not a button");

    let err = render_error(r#"export default class C extends View { render() { return new Breadcrumb("Files"); } }"#);
    assert!(err.contains("Breadcrumb(labels) expects an array of strings"), "{err}");
}

// Settings ---------------------------------------------------------------------------

const SETTINGS: &str = r#"
export default class C extends View {
  init() { this.wifi = true; }
  render() {
    const general = new SettingPage("General").description("Applies to this device.")
      .child(new SettingGroup().title("Connections").description("Off saves battery.")
        .child(new SettingItem("Wi-Fi").content(Switch.new("wifi").test_id("wifi").checked(this.wifi)
          .on_change((v, cx) => { this.wifi = v; cx.notify(); })))
        .child(new SettingItem("Bluetooth").description("Not available on this model").disabled(true)
          .content(Switch.new("bt").test_id("bt"))));
    const privacy = new SettingPage("Privacy")
      .child(new SettingGroup().title("Sharing").child(new SettingItem("Share analytics").content(Switch.new("an").test_id("analytics"))));
    return new Settings("prefs").test_id("settings").default_selected_page(0).child(general).child(privacy);
  }
}
"#;

#[test]
fn settings_draws_groups_as_inset_cards_and_switches_pages() {
    let mut h = start(SETTINGS);
    let texts = h.texts();
    for t in ["Applies to this device.", "CONNECTIONS", "Wi-Fi", "Bluetooth", "Not available on this model", "Off saves battery."] {
        assert!(texts.contains(&t.to_string()), "{t}: {texts:?}");
    }
    assert!(!texts.contains(&"Share analytics".to_string()));
    // The pages are peers on a segmented control.
    let switcher = h.nodes_of(ViewType::Segmented)[0];
    assert_eq!(h.native().prop(switcher, Prop::Items), Some(&PropValue::Json(r#"["General","Privacy"]"#.into())));
    // A disabled item disables its control.
    assert_eq!(h.prop("bt", Prop::Enabled), Some(PropValue::Bool(false)));
    assert_eq!(h.prop("wifi", Prop::Enabled), Some(PropValue::Bool(true)));

    h.dispatch(switcher, EventType::Change, r#"{"index":1,"value":"Privacy"}"#);
    assert!(h.texts().contains(&"Share analytics".to_string()));
    assert!(h.find("wifi").is_none());
    // The page stays chosen across renders.
    h.toggle("analytics");
    assert!(h.find("analytics").is_some());
}

#[test]
fn a_single_settings_page_shows_its_title() {
    let h = start(
        r#"
export default class C extends View {
  render() {
    return new Settings("s").child(new SettingPage("Notifications")
      .child(new SettingGroup().child(new SettingItem("Previews").layout("vertical").content(text("When unlocked")))));
  }
}
"#,
    );
    assert_eq!(h.texts(), ["Notifications", "Previews", "When unlocked"]);
    assert!(h.nodes_of(ViewType::Segmented).is_empty());
    let err = render_error(
        r#"export default class C extends View { render() { return new Settings("s").child(new SettingPage("P").child(new SettingGroup().child(new SettingItem("x")))); } }"#,
    );
    assert!(err.contains("SettingItem requires content(element)"), "{err}");
}

// Group box and toolbar ------------------------------------------------------------------

#[test]
fn group_box_titles_its_content_and_toolbar_spaces_its_items() {
    let h = start(
        r#"
export default class C extends View {
  render() {
    return v_flex().w(390)
      .child(new GroupBox().title("Delivery").variant("fill").test_id("box").child(text("Arrives Thursday")))
      .child(new Toolbar("actions").test_id("toolbar").child(div().test_id("a").size(20)).child(div().test_id("b").size(20)));
  }
}
"#,
    );
    assert_eq!(h.text_of("box"), "DELIVERY Arrives Thursday");
    let content = h.native().children(h.find("box").unwrap())[1];
    let surface = Theme::default_for(Appearance::Light).color("surface").unwrap();
    assert_eq!(h.native().prop(content, Prop::BackgroundColor), Some(&PropValue::Color(surface)));
    assert!(h.frame("toolbar").height >= 44.0);
    assert_eq!(h.frame("a").x, 16.0);
    assert_eq!(h.frame("b").x, 390.0 - 16.0 - 20.0, "items at both ends");
}

// Calendar --------------------------------------------------------------------------------

fn calendar(setup: &str, calendar: &str) -> String {
    format!(
        r#"
export default class C extends View {{
  init() {{ this.state = CalendarState(); {setup} this.log = []; this.state.on("change", (d, cx) => {{ this.log.push(JSON.stringify(d)); cx.notify(); }}); }}
  render() {{
    return v_flex().w(390)
      .child(text(this.log.join(" ")).test_id("log"))
      .child({calendar}.test_id("cal"));
  }}
}}
"#
    )
}

fn labeled_node(h: &Harness, label: &str) -> u32 {
    h.native()
        .nodes
        .iter()
        .find(|(_, n)| matches!(n.props.get(&Prop::A11yLabel), Some(PropValue::Str(s)) if s == label))
        .map(|(id, _)| *id)
        .unwrap_or_else(|| panic!("nothing labeled {label:?}"))
}

#[test]
fn calendar_shows_a_month_and_selects_a_day() {
    let mut h = start(&calendar(r#"this.state.set_month(2020, 10);"#, "new Calendar(this.state)"));
    let texts = h.texts();
    assert_eq!(texts[1], "October 2020");
    assert_eq!(&texts[2..9], ["SUN", "MON", "TUE", "WED", "THU", "FRI", "SAT"], "en starts on Sunday");
    // October 2020 starts on a Thursday.
    assert_eq!(&texts[9..12], ["1", "2", "3"]);
    assert_eq!(str_prop(&h, labeled(&h, "14"), Prop::A11yLabel).as_deref(), Some("Wednesday, October 14, 2020"));
    h.tap_text("14");
    assert_eq!(log(&h), r#""2020-10-14""#);
    assert_eq!(a11y_state(&h, labeled(&h, "14"))["selected"], true);
    let primary = Theme::default_for(Appearance::Light).color("primary").unwrap();
    let disc = h.native().nodes[&h.find_text("14").unwrap()].parent.unwrap();
    assert_eq!(h.native().prop(disc, Prop::BackgroundColor), Some(&PropValue::Color(primary)));

    h.tap_node(labeled_node(&h, "Next month"));
    assert_eq!(h.texts()[1], "November 2020");
    h.tap_node(labeled_node(&h, "Previous month"));
    h.tap_node(labeled_node(&h, "Previous month"));
    assert_eq!(h.texts()[1], "September 2020");
}

#[test]
fn calendar_selects_a_range_and_skips_disabled_days() {
    let mut h = start(&calendar(
        r#"this.state.set_date(["2026-10-05", null]); this.state.set_disabled_matcher((d) => d === "2026-10-20");"#,
        "new Calendar(this.state).first_day_of_week(1)",
    ));
    assert_eq!(&h.texts()[2..4], ["MON", "TUE"]);
    h.tap_text("12");
    assert_eq!(log(&h), r#"["2026-10-05","2026-10-12"]"#);
    // The days between are in the range's band.
    assert_eq!(a11y_state(&h, labeled(&h, "8"))["selected"], true);
    assert_eq!(a11y_state(&h, labeled(&h, "13")).get("selected"), None);
    // A disabled day is announced and ignores taps.
    assert_eq!(a11y_state(&h, labeled(&h, "20"))["disabled"], true);
    h.tap_text("20");
    // A tap after a complete range starts a new one.
    h.tap_text("3");
    assert_eq!(log(&h), r#"["2026-10-05","2026-10-12"] ["2026-10-03",null]"#);
}

#[test]
fn calendar_week_starts_on_the_locales_first_day() {
    let src = program(&calendar("this.state.set_month(2020, 10);", "new Calendar(this.state)"));
    for (locale, first) in [("en-US", "SUN"), ("de-DE", "MON"), ("en-GB", "MON"), ("ar-EG", "SAT")] {
        let env = Environment { locale: locale.into(), ..Environment::default() };
        let h = started(Harness::build(&[("main.js", &src)]).environment(env).start());
        assert_eq!(h.texts()[2], first, "{locale}");
    }
}

#[test]
fn calendar_marks_days_and_names_its_parts() {
    let mut h = start(&calendar(
        r#"this.state.set_month(2020, 10); this.state.set_marker_matcher((d) => d === "2020-10-16" ? "tinted" : d === "2020-10-20" ? "default" : null);"#,
        "new Calendar(this.state)",
    ));
    assert_eq!(h.prop("cal-2020-10-16", Prop::A11yLabel), Some(PropValue::Str("Friday, October 16, 2020, has events".into())));
    let dot = |h: &Harness, id: &str| {
        let disc = h.native().children(h.find(id).unwrap())[0];
        let kids = h.native().children(disc).to_vec();
        kids.iter().find_map(|k| match h.native().prop(*k, Prop::BackgroundColor) {
            Some(PropValue::Color(c)) => Some(*c),
            _ => None,
        })
    };
    let theme = Theme::default_for(Appearance::Light);
    assert_eq!(dot(&h, "cal-2020-10-16"), theme.color("primary"));
    assert_eq!(dot(&h, "cal-2020-10-20"), theme.color("muted_foreground"));
    assert_eq!(dot(&h, "cal-2020-10-17"), None);
    // Day and month-button ids make taps scriptable.
    h.tap("cal-2020-10-16");
    assert_eq!(log(&h), r#""2020-10-16""#);
    assert_eq!(dot(&h, "cal-2020-10-16"), theme.color("primary_foreground"), "white on the selected disc");
    h.tap("cal-next");
    assert!(h.find("cal-2020-11-02").is_some());
    h.tap("cal-prev");
    assert!(h.find("cal-2020-10-31").is_some());
}

#[test]
fn calendar_columns_fit_at_twice_the_text_size() {
    let src = program(&calendar("this.state.set_month(2020, 10);", "new Calendar(this.state)"));
    let env = Environment { font_scale: 2.0, ..Environment::default() };
    let h = started(Harness::build(&[("main.js", &src)]).environment(env).start());
    assert_eq!(&h.texts()[2..4], ["S", "M"], "narrow weekday symbols");
    // Saturday's column ends inside the 390 pt screen, and its disc inside the column.
    let sat = h.find("cal-2020-10-31").unwrap();
    let cell = h.native().nodes[&sat].frame.unwrap();
    assert!(cell.x + cell.width <= 390.5, "{cell:?}");
    let disc = h.native().nodes[&h.native().children(sat)[0]].frame.unwrap();
    assert!(disc.x >= 0.0 && disc.x + disc.width <= cell.width + 0.5, "{disc:?} in {cell:?}");
}

#[test]
fn calendar_state_is_refused_in_render() {
    let err = render_error("export default class C extends View { render() { return new Calendar(CalendarState()); } }");
    assert!(err.contains("CalendarState() cannot run during render"), "{err}");
}

// Carousel ---------------------------------------------------------------------------------

const PHOTOS: &str = r#"
export default class C extends View {
  init() { this.state = CarouselState(3); this.log = []; }
  render() {
    const s = this.state;
    return v_flex().w(390)
      .child(text(this.log.join(",")).test_id("log"))
      .child(new Carousel("photos", s).test_id("carousel").accessibility_label("Cabin photos")
        .on_change((i, cx) => { this.log.push(i); cx.notify(); })
        .child(new CarouselContent(s).test_id("content")
          .children([0, 1, 2].map((i) => new CarouselItem(`p${i}`, i, s).test_id(`slide${i}`).child(div().h(200).child(`Photo ${i + 1}`)))))
        .child(new CarouselPrevious(s).test_id("prev"))
        .child(new CarouselNext(s).test_id("next"))
        .child(new CarouselPagination().test_id("dots").children([0, 1, 2].map((i) => new CarouselPaginationItem(`d${i}`, i, s).test_id(`dot${i}`)))));
  }
}
"#;

#[test]
fn carousel_pages_with_buttons_and_dots() {
    let mut h = start(PHOTOS);
    assert_eq!(h.prop("carousel", Prop::A11yLabel), Some(PropValue::Str("Cabin photos".into())));
    assert_eq!(h.prop("carousel", Prop::A11yValue), Some(PropValue::Str("Item 1 of 3".into())));
    assert_eq!(h.prop("slide1", Prop::A11yHidden), Some(PropValue::Bool(true)), "slides off screen are not announced");
    assert_eq!(a11y_state(&h, h.find("prev").unwrap())["disabled"], true, "no previous item without looping");
    // Every slide is as wide as the viewport.
    assert_eq!(h.frame("slide1").width, 390.0);

    h.tap("next");
    assert_eq!(log(&h), "1");
    assert_eq!(h.prop("carousel", Prop::A11yValue), Some(PropValue::Str("Item 2 of 3".into())));
    assert_eq!(h.prop("slide1", Prop::A11yHidden), None);
    assert_eq!(a11y_state(&h, h.find("dot1").unwrap())["selected"], true);
    h.tap("dot2");
    assert_eq!(log(&h), "1,2");
    assert_eq!(a11y_state(&h, h.find("next").unwrap())["disabled"], true);
}

/// The `ScrollTo` commands sent to the node with `test_id`, as JSON.
fn scrolls(h: &Harness, test_id: &str) -> Vec<serde_json::Value> {
    let node = h.find(test_id).unwrap();
    h.commands()
        .iter()
        .filter(|(n, c, _)| *n == node && *c == Command::ScrollTo)
        .map(|(_, _, p)| serde_json::from_str(p).unwrap())
        .collect()
}

#[test]
fn carousel_is_a_paged_horizontal_scroll() {
    let mut h = start(PHOTOS);
    // A native paged scroll: never a pan that would take the page's vertical drags.
    assert_eq!(h.view_type("content"), ViewType::Scroll);
    assert_eq!(h.prop("content", Prop::Horizontal), Some(PropValue::Bool(true)));
    assert_eq!(h.prop("content", Prop::Paging), Some(PropValue::Bool(true)));
    assert!(scrolls(&h, "content").is_empty(), "the first slide needs no scroll");

    // A swipe: the slide nearest the offset becomes the selection.
    let content = h.find("content").unwrap();
    let scroll = |h: &mut Harness, x: f32| {
        h.dispatch(content, EventType::Scroll, &format!(r#"{{"x":{x},"y":0,"content_width":1170,"content_height":200,"viewport_width":390,"viewport_height":200}}"#));
    };
    scroll(&mut h, 150.0);
    assert_eq!(log(&h), "");
    scroll(&mut h, 390.0);
    assert_eq!(log(&h), "1");
    assert_eq!(h.prop("carousel", Prop::A11yValue), Some(PropValue::Str("Item 2 of 3".into())));

    // The buttons and dots scroll there, animated, a viewport width per slide.
    h.clear_commands();
    h.tap("next");
    assert_eq!(log(&h), "1,2");
    assert_eq!(scrolls(&h, "content"), [serde_json::json!({"x": 780.0, "y": 0, "animated": true})]);
    // The slides it passes on the way back are not selections.
    h.clear_commands();
    h.tap("dot0");
    scroll(&mut h, 400.0);
    scroll(&mut h, 0.0);
    assert_eq!(log(&h), "1,2,0");
    assert_eq!(scrolls(&h, "content")[0]["x"], 0.0);
}

#[test]
fn carousel_shows_a_scripts_selection_without_animating_the_first_time() {
    let h = start(
        r#"
export default class C extends View {
  init() { this.state = CarouselState(3, 2); }
  render() {
    const s = this.state;
    return v_flex().w(390).child(new Carousel("c", s).child(new CarouselContent(s).test_id("content")
      .children([0, 1, 2].map((i) => new CarouselItem(`p${i}`, i, s).child(div().h(100))))));
  }
}
"#,
    );
    assert_eq!(scrolls(&h, "content"), [serde_json::json!({"x": 780.0, "y": 0, "animated": false})]);
}

// Menus -----------------------------------------------------------------------------------

fn menu_json(h: &Harness, id: &str) -> serde_json::Value {
    match h.prop(id, Prop::Menu) {
        Some(PropValue::Json(s)) => serde_json::from_str(&s).unwrap(),
        other => panic!("{other:?}"),
    }
}

#[test]
fn dropdown_menu_is_a_native_menu_that_keeps_its_label() {
    let mut h = start(
        r#"
export default class C extends View {
  init() { this.log = []; }
  note(s, cx) { this.log.push(s); cx.notify(); }
  render() {
    return v_flex()
      .child(text(this.log.join(",")).test_id("log"))
      .child(new DropdownMenu("sort", "Sort by").test_id("sort").item("Name", (cx) => this.note("name", cx)).item("Date", (cx) => this.note("date", cx)))
      .child(new DropdownButton("track", "Track package").variant("primary").test_id("split")
        .on_click((_e, cx) => this.note("track", cx)).menu_item("Contact carrier", (cx) => this.note("carrier", cx)));
  }
}
"#,
    );
    assert_eq!(h.view_type("sort"), ViewType::Select);
    let menu = menu_json(&h, "sort");
    assert_eq!(menu["placeholder"], "Sort by");
    assert_eq!(menu["items"][1]["label"], "Date");
    assert_eq!(h.prop("sort", Prop::SelectedIndex), Some(PropValue::I32(-1)));
    h.select("sort", 1);
    assert_eq!(log(&h), "date");
    // The next render clears the renderer's mark: the button keeps its name.
    assert_eq!(h.prop("sort", Prop::SelectedIndex), Some(PropValue::I32(-2)));
    h.select("sort", 0);
    assert_eq!(log(&h), "date,name");
    assert_eq!(h.prop("sort", Prop::SelectedIndex), Some(PropValue::I32(-1)));

    // The split button: its label half acts, its chevron half opens the menu.
    h.tap_text("Track package");
    let half = h.native().children(h.find("split").unwrap())[1];
    assert_eq!(h.native().nodes[&half].view_type, ViewType::Select);
    assert_eq!(str_prop(&h, half, Prop::A11yLabel).as_deref(), Some("More Track package options"));
    h.dispatch(half, EventType::Change, r#"{"index":0,"value":0}"#);
    assert_eq!(log(&h), "date,name,track,carrier");
    let primary = Theme::default_for(Appearance::Light).color("primary").unwrap();
    assert_eq!(h.prop("split", Prop::BackgroundColor), Some(PropValue::Color(primary)));
}

#[test]
fn menu_items_dispatch_their_action_to_the_nearest_handler() {
    let mut h = start(
        r#"
export default class C extends View {
  init() { this.log = []; }
  render() {
    return v_flex()
      .on_action("archive", (e, cx) => { this.log.push(e.action); cx.notify(); })
      .child(text(this.log.join(",")).test_id("log"))
      .child(h_flex().child(new Menu("Message").test_id("menu")
        .child(new MenuItem("Archive", "archive"))
        .child(new MenuSeparator())
        .child(new MenuItem("Delete", "delete").disabled(true))));
  }
}
"#,
    );
    let menu = menu_json(&h, "menu");
    assert_eq!(menu["items"].as_array().unwrap().len(), 2, "separators are not menu items");
    assert_eq!(menu["items"][1]["disabled"], true);
    h.select("menu", 0);
    assert_eq!(log(&h), "archive");
    h.select("menu", 1);
    assert_eq!(log(&h), "archive", "a disabled item does nothing");
}

// Icon -----------------------------------------------------------------------------------

#[test]
fn icon_draws_the_built_in_set_and_the_programs_own_svgs() {
    let main = program(
        r#"
export default class C extends View {
  render() {
    return v_flex()
      .child(new Icon("icons/chevron-right.svg").test_id("chevron"))
      .child(Icon.new("Inbox").size("large").color("primary").test_id("inbox"))
      .child(new Icon("star-fill").size(16).test_id("star"))
      .child(new Icon("art/logo.svg").test_id("own"))
      .child(new Icon("icons/heart.svg").accessibility_label("Favorite").test_id("fav"));
  }
}
"#,
    );
    let logo = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><path d="M2 2h20v20H2z"/></svg>"#;
    let h = started(Harness::new(&[("main.js", &main), ("art/logo.svg", logo)]));
    assert_eq!(h.view_type("chevron"), ViewType::Shape);
    assert_eq!(h.prop("chevron", Prop::PathData), Some(PropValue::Str("M9 18l6-6-6-6".into())));
    assert_eq!((h.frame("chevron").width, h.frame("chevron").height), (20.0, 20.0));
    assert_eq!(h.frame("inbox").width, 24.0);
    let primary = Theme::default_for(Appearance::Light).color("primary").unwrap();
    assert_eq!(h.prop("inbox", Prop::StrokeColor), Some(PropValue::Color(primary)));
    assert!(h.prop("star", Prop::FillColor).is_some(), "star-fill is filled");
    assert_eq!(h.prop("chevron", Prop::FillColor), None);
    assert_eq!(h.view_type("own"), ViewType::Shape, "the program's svg, parsed into a shape");
    // Icons beside a label are decoration; one that stands alone is named.
    assert_eq!(h.prop("chevron", Prop::A11yHidden), Some(PropValue::Bool(true)));
    assert_eq!(h.prop("fav", Prop::A11yHidden), Some(PropValue::Bool(false)));

    let err = render_error(r#"export default class C extends View { render() { return new Icon("../secret.svg"); } }"#);
    assert!(err.contains("Icon path must stay inside the application asset root"), "{err}");
}
