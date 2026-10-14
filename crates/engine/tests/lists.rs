//! Virtualized lists: only the reported window renders.

use native_shell::core::protocol::{Command, Prop, PropValue, ViewType};
use native_shell::testing::Harness;

const LIST: &str = r#"
import { View, div, v_flex, list, uniform_list } from "native-shell";
export default class L extends View {
  init() { this.items = Array.from({ length: 10000 }, (_, i) => ({ id: `item-${i}`, title: `Item ${i}` })); this.rows = 0; }
  render(cx) {
    return v_flex().size_full()
      .child(div().test_id("rows").child(`${this.rows}`))
      .child(list("items", this.items.length, (i) => this.items[i].id, (i, cx) => {
        this.rows += 1;
        return div().h_10().test_id(this.items[i].id).child(this.items[i].title).on_click((_e, cx) => cx.scroll_to_item("items", 9999, "bottom"));
      }).test_id("list").flex_1().estimated_row_height(40));
  }
}
"#;

#[test]
fn ten_thousand_items_render_only_the_window() {
    let mut h = Harness::new(&[("main.js", LIST)]);
    assert!(h.load_error().is_none(), "{:?}", h.load_error());
    assert_eq!(h.view_type("list"), ViewType::List);
    assert_eq!(h.prop("list", Prop::ItemCount), Some(PropValue::I32(10000)));
    // The renderer reported the rows that fit (825 px of 40 px rows: 0..=20) plus 10 of prefetch.
    let rows = h.native().children(h.find("list").unwrap()).len();
    assert_eq!(rows, 31, "{}", h.dump());
    assert!(h.find("item-0").is_some() && h.find("item-30").is_some() && h.find("item-31").is_none());
    let Some(PropValue::F32Array(offsets)) = h.prop("list", Prop::RowOffsets) else { panic!() };
    assert_eq!(offsets.len(), 10001);

    let renders = h.instance().metrics().renders;
    h.scroll_list("list", 500, 530);
    assert_eq!(h.instance().metrics().renders, renders + 1, "the range change re-rendered the list's view");
    assert!(h.find("item-0").is_none());
    assert!(h.find("item-500").is_some() && h.find("item-529").is_some() && h.find("item-530").is_none());
    assert_eq!(h.native().children(h.find("list").unwrap()).len(), 30);

    // The same range again changes nothing.
    let renders = h.instance().metrics().renders;
    h.scroll_list("list", 500, 530);
    assert_eq!(h.instance().metrics().renders, renders);
}

#[test]
fn scroll_to_item_targets_the_list_node() {
    let mut h = Harness::new(&[("main.js", LIST)]);
    h.clear_commands();
    h.tap("item-3");
    let list = h.find("list").unwrap();
    let (node, command, payload) = h.commands().last().cloned().expect("a command");
    assert_eq!((node, command), (list, Command::ScrollToIndex));
    assert!(payload.contains("\"index\":9999") && payload.contains("bottom"), "{payload}");
}

#[test]
fn uniform_lists_render_a_range_at_a_time() {
    let mut h = Harness::new(&[(
        "main.js",
        r#"
import { View, div, uniform_list } from "native-shell";
export default class U extends View {
  render() {
    return uniform_list("u", 1000, (range, cx) => {
      const out = [];
      for (let i = range.start; i < range.end; i++) out.push(div().h_8().test_id(`u${i}`).child(`row ${i}`));
      return out;
    }).size_full().test_id("u");
  }
}
"#,
    )]);
    assert!(h.load_error().is_none(), "{:?}", h.load_error());
    assert!(h.find("u0").is_some() && h.find("u20").is_some());
    h.scroll_list("u", 100, 110);
    assert!(h.find("u105").is_some() && h.find("u0").is_none());
}

#[test]
fn lists_outside_render_are_refused() {
    let h = Harness::new(&[(
        "main.js",
        r#"
import { View, div, list } from "native-shell";
const prebuilt = list("x", 1, () => "k", () => div());
export default class U extends View { render() { return div(); } }
"#,
    )]);
    let e = h.load_error().expect("module evaluation fails").to_string();
    assert!(e.contains("can only be built while rendering"), "{e}");
}

const CHAT: &str = r#"
import { View, div, v_flex, list } from "native-shell";
export default class C extends View {
  init() { this.messages = Array.from({ length: 200 }, (_, i) => `Message ${i}`); }
  render(cx) {
    return v_flex().size_full().pb(cx.window().keyboard_height)
      .child(list("thread", this.messages.length, (i) => `m${i}`, (i) => div().h_10().test_id(`m${i}`).child(this.messages[i]))
        .alignment("bottom").test_id("thread").flex_1().estimated_row_height(40))
      .child(div().test_id("send").h_10().child("Send").on_click((_e, cx) => {
        this.messages.push(`Message ${this.messages.length}`);
        cx.notify();
      }));
  }
}
"#;

#[test]
fn a_bottom_aligned_list_opens_at_its_end_and_follows_it() {
    let mut h = Harness::new(&[("main.js", CHAT)]);
    assert_eq!(h.prop("thread", Prop::ListAlignment), Some(PropValue::I32(1)));
    assert!(h.find("m199").is_some(), "the newest message is in view: {}", h.dump());
    assert!(h.find("m0").is_none());

    h.tap("send");
    assert!(h.find("m200").is_some(), "a new message at the end stays in view: {}", h.dump());

    // The keyboard shrinks the list: its end is still the last row in view.
    h.set_environment(native_shell::core::host::Environment { keyboard_height: 300.0, ..Default::default() });
    assert!(h.frame("thread").height <= 844.0 - 300.0 - 40.0, "the list shrank above the keyboard");
    assert!(h.find("m200").is_some(), "{}", h.dump());

    // Without a renderer reporting ranges, the first render already shows the tail.
    let quiet = Harness::build(&[("main.js", CHAT)]).emulate_list_ranges(false).start();
    assert!(quiet.find("m199").is_some() && quiet.find("m179").is_none() && quiet.find("m180").is_some());

    // Scrolled back into the history, new messages no longer pull the view down.
    h.scroll_list("thread", 0, 30);
    h.tap("send");
    assert!(h.find("m201").is_none() && h.find("m0").is_some());
    let e = Harness::new(&[(
        "main.js",
        r#"import { View, list, div } from "native-shell";
export default class B extends View { render() { return list("l", 1, () => "a", () => div()).alignment("end"); } }"#,
    )]);
    assert!(e.load_error().map(|e| e.to_string()).unwrap_or_default().contains(r#"must be "top" or "bottom""#));
}
