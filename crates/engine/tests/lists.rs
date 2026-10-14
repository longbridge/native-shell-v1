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
