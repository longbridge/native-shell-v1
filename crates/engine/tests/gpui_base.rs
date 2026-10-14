//! gpui-base's headless catalog on mobile.

use native_shell::core::protocol::{Command, EventType, Prop, PropValue, ViewType};
use native_shell::testing::Harness;

fn app(main: &str) -> Harness {
    let h = Harness::new(&[("main.js", main)]);
    if let Some(e) = h.load_error() {
        panic!("{e}\n{:#?}", h.logs());
    }
    h
}

fn state(h: &Harness, id: &str) -> serde_json::Value {
    match h.prop(id, Prop::A11yState) {
        Some(PropValue::Json(j)) => serde_json::from_str(&j).unwrap(),
        _ => serde_json::json!({}),
    }
}

#[test]
fn toggles_groups_collapsible_accordion_avatar_pagination() {
    let mut h = app(r#"
import { View, div } from "gpui-kit";
import { v_flex, Toggle, ToggleGroup, RadioGroup, Radio, Collapsible, Accordion, AccordionItem, AccordionHeader,
  AccordionPanel, AccordionTrigger, Avatar, AvatarImage, AvatarFallback, Pagination, pagination_items } from "gpui-base";
export default class B extends View {
  init() { this.bold = false; this.open = false; this.faq = null; this.choice = "a"; }
  render(cx) {
    return v_flex()
      .child(ToggleGroup.new("fmt").child(Toggle.new("bold").test_id("bold").pressed(this.bold)
        .on_change((v, cx) => { this.bold = v; cx.notify(); }).child("B")))
      .child(RadioGroup.new("g").test_id("group").children(["a", "b"].map((v) =>
        Radio.new(v).test_id(`radio-${v}`).checked(this.choice === v).on_change((_c, cx) => { this.choice = v; cx.notify(); })
          .child(div().size_4()))))
      .child(Collapsible.new().test_id("coll").open(this.open)
        .child(div().test_id("coll-header").child("Header").on_click((_e, cx) => { this.open = !this.open; cx.notify(); }))
        .content(div().test_id("coll-body").child("Body")))
      .child(Accordion.new("faq").child(AccordionItem.new().open(this.faq === "ship")
        .header(AccordionHeader.new(AccordionTrigger.new("ship-t").test_id("ship")
          .on_change((open, cx) => { this.faq = open ? "ship" : null; cx.notify(); }).child("Shipping")).aria_level(3))
        .panel(AccordionPanel.new().test_id("ship-panel").child("Two to five days."))))
      .child(Avatar.new().test_id("with-image").size_10().image(AvatarImage.new("a.png").size_full())
        .fallback(AvatarFallback.new().test_id("fallback-1").child("AL")))
      .child(Avatar.new().test_id("no-image").fallback(AvatarFallback.new().test_id("fallback-2").child("GH")))
      .child(Pagination.new("pages").test_id("pages").children(pagination_items(5, 20).map((i) =>
        div().child(i.ellipsis ? `…${i.ellipsis[0]}-${i.ellipsis[1]}` : String(i.page)))));
  }
}
"#);
    h.tap("bold");
    assert_eq!(state(&h, "bold")["checked"], true);
    h.tap("radio-b");
    assert_eq!(state(&h, "radio-b")["checked"], true);
    assert_eq!(h.prop("radio-b", Prop::A11yRole), Some(PropValue::I32(9)));
    assert!(h.find("coll-body").is_none());
    h.tap("coll-header");
    assert_eq!(h.text_of("coll-body"), "Body");
    assert!(h.find("ship-panel").is_none());
    assert_eq!(state(&h, "ship")["expanded"], serde_json::Value::Null);
    h.tap("ship");
    assert_eq!(h.text_of("ship-panel"), "Two to five days.");
    assert_eq!(state(&h, "ship")["expanded"], true);
    assert!(h.find("fallback-1").is_none() && h.find("fallback-2").is_some());
    assert_eq!(h.text_of("pages"), "1 …2-3 4 5 6 …7-19 20");
}

#[test]
fn number_and_otp_inputs() {
    let mut h = app(r#"
import { View, div } from "gpui-kit";
import { v_flex, NumberInput, InputState, OtpState, OtpInput } from "gpui-base";
export default class N extends View {
  init(_p, cx) {
    this.qty = InputState.new({ value: "1" });
    this.qty.set_step(1); this.qty.set_min(0); this.qty.set_max(3);
    this.steps = [];
    this.code = OtpState.new(4);
    this.done = false;
    this.code.on("complete", (_e, cx) => { this.done = true; cx.notify(); });
    this.code.on("change", (_e, cx) => cx.notify());
  }
  render(cx) {
    return v_flex()
      .child(NumberInput.new(this.qty).test_id("qty")
        .decrement_button(div().test_id("dec").child("-"))
        .increment_button(div().test_id("inc").child("+"))
        .on_step((a, cx) => { this.steps.push(a); cx.notify(); }))
      .child(div().test_id("value").child(`${this.qty.value()} ${this.steps.join(",")}`))
      .child(OtpInput.new(this.code).test_id("otp"))
      .child(div().test_id("otp-state").child(`${this.code.value()} ${this.done} ${this.code.len()}`));
  }
}
"#);
    for _ in 0..4 {
        h.tap("inc");
    }
    assert_eq!(h.text_of("value"), "3 increment,increment,increment,increment", "clamped to max");
    h.tap("dec");
    assert_eq!(h.text_of("value"), "2 increment,increment,increment,increment,decrement");
    let field = h.nodes_of(ViewType::TextInput)[0];
    assert_eq!(h.native().prop(field, Prop::KeyboardType), Some(&PropValue::I32(2)));
    assert_eq!(h.prop("otp", Prop::ContentType), Some(PropValue::I32(1)));
    assert_eq!(h.prop("otp", Prop::MaxLength), Some(PropValue::I32(4)));
    h.type_text("otp", "12");
    assert_eq!(h.text_of("otp-state"), "12 false 4");
    h.type_text("otp", "1234");
    assert_eq!(h.text_of("otp-state"), "1234 true 4");
}

#[test]
fn virtual_lists_and_scroll_handles() {
    let mut h = app(r#"
import { View, div } from "gpui-kit";
import { v_flex, v_virtual_list, h_virtual_list, VirtualListScrollHandle, Scrollbar } from "gpui-base";
export default class L extends View {
  init() { this.scroll = VirtualListScrollHandle.new(); this.picked = ""; }
  render(cx) {
    return v_flex().size_full()
      .child(div().test_id("picked").child(this.picked))
      .child(div().test_id("bottom").child("bottom").on_click(() => this.scroll.scroll_to_bottom()))
      .child(v_flex().relative().h(400)
        .child(v_virtual_list("rows", 1000, 28, (i) => `r${i}`, (range) => {
          const out = [];
          for (let i = range.start; i < range.end; i++) out.push(div().test_id(`r${i}`).child(`row ${i}`));
          return out;
        }).test_id("rows").track_scroll(this.scroll).on_item_click((key, cx) => { this.picked = key; cx.notify(); }))
        .child(Scrollbar.vertical("rows").absolute().inset_0()))
      .child(h_virtual_list("chips", 3, 60, (i) => `c${i}`, (range) => [0, 1, 2].map((i) => div().test_id(`c${i}`).child(`chip ${i}`))).test_id("chips"));
  }
}
"#);
    assert_eq!(h.view_type("rows"), ViewType::List);
    assert_eq!(h.prop("rows", Prop::ItemCount), Some(PropValue::I32(1000)));
    assert!(h.find("r0").is_some() && h.find("r999").is_none());
    h.tap("r2");
    assert_eq!(h.text_of("picked"), "r2");
    h.clear_commands();
    h.tap("bottom");
    let list = h.find("rows").unwrap();
    assert!(
        h.commands().iter().any(|(n, c, p)| *n == list && *c == Command::ScrollToIndex && p.contains("999")),
        "{:?}",
        h.commands()
    );
    assert_eq!(h.view_type("chips"), ViewType::Scroll);
    assert!(h.find("c2").is_some());
}

#[test]
fn anchored_surfaces_present_as_sheets() {
    let mut h = app(r#"
import { View, div } from "gpui-kit";
import { v_flex, Popover, HoverCard, Popup, Button } from "gpui-base";
export default class P extends View {
  init() { this.open = false; this.log = []; this.popup = false; }
  render(cx) {
    return v_flex()
      .child(div().test_id("log").child(this.log.join(",")))
      .child(Popover.new("menu").open(this.open)
        .on_open_change((open, cx) => { this.open = open; this.log.push(`open${open}`); cx.notify(); })
        .trigger(Button.new("t").test_id("trigger").child("Menu"))
        .content(v_flex().test_id("menu-body").child(Button.new("x").test_id("item").child("Item")
          .on_click((_e, cx) => { this.log.push("item"); this.open = false; cx.notify(); }))))
      .child(HoverCard.new("card").trigger(div().test_id("card-trigger").child("hold me"))
        .content(div().test_id("card").child("Details")))
      .child(Popup.new("p", Button.new("pt").test_id("popup-trigger").child("P").on_click((_e, cx) => { this.popup = !this.popup; cx.notify(); }))
        .when(this.popup, (el) => el.content(div().test_id("popup-body").child("Popup"))));
  }
}
"#);
    assert_eq!(h.surfaces().len(), 1);
    h.tap("trigger");
    assert_eq!(h.surfaces().len(), 2);
    assert_eq!(h.surfaces()[1].1, ViewType::Sheet);
    assert_eq!(h.text_of("menu-body"), "Item");
    h.tap("item");
    assert_eq!(h.surfaces().len(), 1, "closing from inside the content");
    h.tap("trigger");
    h.dismiss();
    assert_eq!(h.surfaces().len(), 1);
    assert_eq!(h.text_of("log"), "opentrue,item,opentrue,openfalse");
    h.long_press("card-trigger");
    assert_eq!(h.text_of("card"), "Details", "a long press opens a hover card");
    h.dismiss();
    h.tap("popup-trigger");
    assert_eq!(h.text_of("popup-body"), "Popup");
    h.tap("popup-trigger");
    assert!(h.find("popup-body").is_none());
}

#[test]
fn select_combobox_and_date_picker_take_both_shapes() {
    let mut h = app(r#"
import { View, div } from "gpui-kit";
import { v_flex, Select, Combobox, DatePicker, Popup } from "gpui-base";
export default class S extends View {
  init(_p, cx) { this.open = false; this.focus = cx.focus_handle(); this.v = "b"; }
  render(cx) {
    return v_flex()
      .child(Select.new("native").test_id("native").items([{ label: "A", value: "a" }, { label: "B", value: "b" }]).value(this.v))
      .child(Select.new("headless").test_id("headless").open(this.open)
        .on_open_change((open, cx) => { this.open = open; cx.notify(); })
        .child(div().child("Pick"))
        .child(Popup.new("list", div()).when(this.open, (el) => el.content(div().test_id("options").child("A B")))))
      .child(Combobox.new("combo").test_id("combo").child("type"))
      .child(DatePicker.new("native-date").test_id("date").value("2026-01-01"))
      .child(DatePicker.new("headless-date", this.focus).test_id("hdate").child("pick a day"));
  }
}
"#);
    assert_eq!(h.view_type("native"), ViewType::Select);
    assert_eq!(h.view_type("headless"), ViewType::View);
    h.tap("headless");
    assert_eq!(state(&h, "headless")["expanded"], true);
    assert_eq!(h.text_of("options"), "A B");
    assert_eq!(h.view_type("combo"), ViewType::View);
    assert_eq!(h.view_type("date"), ViewType::DatePicker);
    assert_eq!(h.view_type("hdate"), ViewType::View);
}

#[test]
fn text_views_render_markdown_and_html() {
    let mut h = app(r##"
import { View, div } from "gpui-kit";
import { v_flex, TextView } from "gpui-base";
export default class T extends View {
  init() { this.clicked = ""; }
  render(cx) {
    return v_flex()
      .child(div().test_id("clicked").child(this.clicked))
      .child(TextView.markdown("md", "# Title\n\nSome **bold** text and [a link](https://example.com).\n\n- one\n- two")
        .test_id("md").selectable(true).on_link_click((url, cx) => { this.clicked = url; cx.notify(); }))
      .child(TextView.html("html", "<p>Hello <b>world</b></p><ul><li>x</li></ul>").test_id("html"));
  }
}
"##);
    let md = h.text_of("md");
    assert!(
        md.contains("Title") && md.contains("Some bold text and a link.") && md.contains("•") && md.contains("two"),
        "{md}"
    );
    assert_eq!(h.text_of("html"), "Hello world • x");
    let link = h.find_text("Some bold text and a link.").unwrap();
    assert_eq!(h.native().prop(link, Prop::Selectable), Some(&PropValue::Bool(true)));
    h.dispatch(link, EventType::Link, r#"{"url":"https://example.com"}"#);
    assert_eq!(h.text_of("clicked"), "https://example.com");
}

#[test]
fn text_view_list_items_wrap_inside_the_view() {
    let h = app(r##"
import { View } from "gpui-kit";
import { v_flex, TextView } from "gpui-base";
export default class T extends View {
  render() {
    return v_flex().w(200).child(TextView.markdown("md", "- An item long enough to need more than one line here").test_id("md"));
  }
}
"##);
    let item = h.find_text("An item long enough to need more than one line here").unwrap();
    let frame = h.instance().tree().frame(item).unwrap();
    assert!(frame.width <= 200.0, "the item's text is laid out within the view: {frame:?}");
    assert!(frame.height > 30.0, "and wraps onto more lines: {frame:?}");
}

#[test]
fn paths_paint_as_shapes() {
    let h = app(r##"
import { View, div, PathBuilder, Background } from "gpui-kit";
export default class P extends View {
  render(cx) {
    const tri = PathBuilder.fill().move_to(0, 0).line_to(20, 0).line_to(10, 10).close().build();
    const line = PathBuilder.stroke(2).move_to("0%", "50%").line_to("100%", "50%").build();
    return div()
      .child(window.paint_path(tri, Background.solid("#ff0000")).test_id("tri").size_8())
      .child(window.paint_path(line, "primary").test_id("line").w_full().h_4());
  }
}
"##);
    assert_eq!(h.view_type("tri"), ViewType::Shape);
    assert_eq!(h.prop("tri", Prop::PathData), Some(PropValue::Str("M 0 0 L 20 0 L 10 10 Z".into())));
    assert_eq!(h.prop("tri", Prop::FillColor), Some(PropValue::Color(0xFFFF_0000)));
    assert_eq!(h.prop("line", Prop::StrokeWidth), Some(PropValue::F32(2.0)));
    assert_eq!(h.prop("line", Prop::ViewBox), Some(PropValue::F32Array(vec![0.0, 0.0, 100.0, 100.0])));
}

#[test]
fn key_bindings_route_actions() {
    let mut h = app(r#"
import { View, div } from "gpui-kit";
export default class K extends View {
  init(_p, cx) { this.log = []; this.bound = cx.bind_keys([{ keystroke: "cmd-s", action: "save", context: "Editor" }, { keystroke: "escape", action: "close" }]); }
  render(cx) {
    return div().test_id("editor").key_context("Editor")
      .on_action("save", (e, cx) => { this.log.push(e.action); cx.notify(); })
      .on_key_down((e, cx) => { this.log.push(`down:${e.keystroke}`); cx.notify(); })
      .on_key_up((e, cx) => { this.log.push(`up:${e.key}`); cx.notify(); })
      .child(div().test_id("log").child(`${this.bound} ${this.log.join(",")}`));
  }
}
"#);
    let node = h.find("editor").unwrap();
    h.dispatch(node, EventType::Key, r#"{"key":"s","key_char":"s","modifiers":{"cmd":true},"phase":"down"}"#);
    h.dispatch(node, EventType::Key, r#"{"key":"a","key_char":"a","modifiers":{"shift":true},"phase":"down"}"#);
    h.dispatch(node, EventType::Key, r#"{"key":"a","modifiers":{},"phase":"up"}"#);
    assert_eq!(h.text_of("log"), "2 save,down:shift-a,up:a");
}

#[test]
fn key_bindings_match_whatever_order_their_modifiers_are_written_in() {
    let mut h = app(r#"
import { View, div } from "gpui-kit";
export default class K extends View {
  init(_p, cx) {
    this.log = [];
    cx.bind_keys([{ keystroke: "cmd-shift-n", action: "folder" }, { keystroke: "Shift-Ctrl-N", action: "folder" },
      { keystroke: "alt-cmd-k", action: "kill" }]);
  }
  render() {
    return div().test_id("root")
      .on_action("folder", (e, cx) => { this.log.push(e.action); cx.notify(); })
      .on_action("kill", (e, cx) => { this.log.push(e.action); cx.notify(); })
      .on_key_down((e, cx) => { this.log.push(e.keystroke); cx.notify(); })
      .child(div().test_id("log").child(this.log.join(",")));
  }
}
"#);
    let node = h.find("root").unwrap();
    h.dispatch(node, EventType::Key, r#"{"key":"n","modifiers":{"cmd":true,"shift":true},"phase":"down"}"#);
    h.dispatch(node, EventType::Key, r#"{"key":"n","modifiers":{"ctrl":true,"shift":true},"phase":"down"}"#);
    h.dispatch(node, EventType::Key, r#"{"key":"k","modifiers":{"cmd":true,"alt":true},"phase":"down"}"#);
    // An event is spelled in gpui's order: cmd before shift.
    h.dispatch(node, EventType::Key, r#"{"key":"x","modifiers":{"shift":true,"cmd":true},"phase":"down"}"#);
    assert_eq!(h.text_of("log"), "folder,folder,kill,cmd-shift-x");
}

#[test]
fn tables_are_headless_semantic_views() {
    let h = app(r#"
import { View, div } from "gpui-kit";
import { Table, TableHeader, TableBody, TableRow, TableHead, TableCell, TableCaption } from "gpui-base";
const rows = [["AAPL", "190"], ["MSFT", "410"]];
export default class T extends View {
  render() {
    return Table.new("quotes").test_id("table").accessibility_label("Quotes").row_count(3).column_count(2).flex().flex_col()
      .child(TableCaption.new("cap").child("Prices"))
      .child(TableHeader.new("head").child(TableRow.new("h", 1).grid().grid_cols(2)
        .child(TableHead.new("sym", 1).test_id("th").child("Symbol")).child(TableHead.new("px", 2).child("Price"))))
      .child(TableBody.new("body").children(rows.map((r, i) => TableRow.new(`r${i}`, i + 2).test_id(`row${i}`).grid().grid_cols(2)
        .children(r.map((c, j) => TableCell.new(`c${i}${j}`, j + 1).child(c))))));
  }
}
"#);
    assert_eq!(h.prop("table", Prop::A11yValue), Some(PropValue::Str("3 rows, 2 columns".into())));
    assert_eq!(h.prop("th", Prop::A11yRole), Some(PropValue::I32(3)));
    assert_eq!(h.prop("row1", Prop::A11yValue), Some(PropValue::Str("3".into())));
    assert_eq!(h.text_of("row1"), "MSFT 410");
    let a = h.instance().tree().frame(h.find_text("AAPL").unwrap()).unwrap();
    let b = h.instance().tree().frame(h.find_text("190").unwrap()).unwrap();
    assert!(b.x > a.x, "cells sit side by side in the grid: {a:?} {b:?}");
}

#[test]
fn keys_travel_up_the_focus_path_and_from_the_root() {
    let mut h = app(r#"
import { View, div } from "gpui-kit";
import { v_flex, Input, InputState } from "gpui-base";
class Editor extends View {
  init(_p, cx) { this.text = InputState.new({}); }
  render() {
    return div().key_context("Editor").on_action("format", (e, cx) => { log.push("format"); })
      .child(Input.new(this.text).test_id("field").on_key_down((e) => log.push(`field:${e.keystroke}`)));
  }
}
const log = [];
export default class App extends View {
  init(_p, cx) {
    cx.bind_keys([{ keystroke: "cmd-s", action: "save" }, { keystroke: "cmd-f", action: "format", context: "Editor" },
      { keystroke: "cmd-f", action: "find" }, { keystroke: "cmd-j", action: "nobody" }]);
    this.editor = cx.new(Editor);
  }
  render() {
    return v_flex().size_full()
      .on_action("save", (e, cx) => { log.push("save"); cx.notify(); })
      .on_action("find", (e, cx) => { log.push("find"); cx.notify(); })
      .child(div().test_id("log").child(log.join(",")).on_click((_e, cx) => cx.notify()))
      .child(this.editor);
  }
}
"#);
    let field = h.find("field").unwrap();
    let key = |k: &str| format!(r#"{{"key":"{k}","modifiers":{{"cmd":true}},"phase":"down"}}"#);
    // Aimed at the focused input: the app-wide binding is handled on the root view.
    h.dispatch(field, EventType::Key, &key("s"));
    // A contextual binding wins inside its context.
    h.dispatch(field, EventType::Key, &key("f"));
    // Nothing focused: the root node; the global binding reaches the root view.
    h.dispatch(0, EventType::Key, &key("s"));
    h.dispatch(0, EventType::Key, &key("f"));
    // An unbound keystroke reaches the input's own handler.
    h.dispatch(field, EventType::Key, &key("k"));
    // A bound keystroke whose action nobody handles falls through to on_key_down, as in GPUI.
    h.dispatch(field, EventType::Key, &key("j"));
    h.tap("log");
    assert_eq!(h.text_of("log"), "save,format,save,find,field:cmd-k,field:cmd-j");
}
