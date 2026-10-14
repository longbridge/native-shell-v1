//! Scroll views and virtualized lists end to end: the tests move GTK's scroll
//! adjustments the way the user's scrolling does and watch the program react.

mod support;

use std::process::ExitCode;
use std::time::Duration;

use native_shell_gtk::NodeView;
use support::*;

fn main() -> ExitCode {
    run(&[
        ("scroll content follows the core's content size", content_size),
        ("scrolling reports scroll events", scroll_events),
        ("scroll_to scrolls to the clamped target", scroll_to),
        ("a mouse wheel scrolls a horizontal scroller sideways", sideways_wheel),
        ("pull to refresh shows the spinner until the script ends it", pull_to_refresh),
        ("lists render the visible range plus prefetch", list_range),
        ("scroll_to_item positions the row", scroll_to_item),
        ("chat opens at the newest message", chat_opens_at_the_newest_message),
    ])
}

const SCROLL: &str = r#"
import { View, div, v_flex } from "native-shell";
export default class App extends View {
  init() { this.y = 0; this.refreshes = 0; this.refreshing = false; }
  render(cx) {
    return v_flex().size_full()
      .child(div().test_id(`y-${Math.round(this.y)}`).h(4))
      .child(div().test_id(`refreshes-${this.refreshes}`).h(4))
      .child(div().id("scroller").test_id("scroller").overflow_y_scroll().h(200)
        .on_scroll((e, cx) => {
          this.y = e.y;
          cx.notify();
          // Scrolling to 7 asks for a scroll_to past the end.
          if (Math.round(e.y) === 7) cx.scroll_to("scroller", { y: 5000, animated: true });
        })
        .on_refresh((_e, cx) => {
          this.refreshes += 1;
          this.refreshing = true;
          cx.notify();
          cx.timer.after(50, (cx) => { this.refreshing = false; cx.notify(); });
        })
        .refreshing(this.refreshing)
        .child(div().test_id("first").h(300))
        .child(div().test_id("second").h(400)));
  }
}"#;

const LIST: &str = r##"
import { View, div, v_flex, list } from "native-shell";
export default class App extends View {
  render(cx) {
    return v_flex().size_full()
      .child(div().test_id("jump").h(44).on_click((_e, cx) => cx.scroll_to_item("items", 900, "center")))
      .child(list("items", 1000, (i) => `k${i}`, (i) => div().h(50).px(16).justify_center().border_b_1().border_color("#e5e5ea").test_id(`row-${i}`).child(`Row ${i}`))
        .test_id("items").flex_1().estimated_row_height(50));
  }
}"##;

fn content_size() {
    let f = Fixture::new(SCROLL);
    let window = scrolled(&f, "scroller");
    assert_eq!(window.vadjustment().upper(), 700.0);
    assert_eq!(window.vadjustment().page_size(), 200.0);
    // Children sit in the scrolled content at their frames.
    let content = window.child().and_then(|v| v.first_child()).unwrap();
    let second = f.expect("second");
    let p = second.compute_point(&content, &gtk::graphene::Point::zero()).unwrap();
    assert_eq!(p.y(), 300.0);
}

fn scroll_events() {
    let f = Fixture::new(SCROLL);
    let vadj = scrolled(&f, "scroller").vadjustment();
    for y in [40.0, 80.0, 120.0] {
        vadj.set_value(y);
    }
    f.wait_for("y-120");
}

fn scroll_to() {
    let f = Fixture::new(SCROLL);
    let vadj = scrolled(&f, "scroller").vadjustment();
    vadj.set_value(7.0);
    f.wait_for("y-7");
    // Animated with a spring; on a frame clock Broadway throttles, it may take a second or two.
    let done = pump_until(Duration::from_secs(5), || vadj.value() == 500.0);
    assert!(done, "scrolled to the end of the content (500), at {}", vadj.value());
    f.wait_for("y-500");
}

fn sideways_wheel() {
    let f = Fixture::new(
        r#"
import { View, div, h_flex, v_flex } from "native-shell";
export default class App extends View {
  render() {
    return v_flex().size_full().child(h_flex().test_id("strip").overflow_x_scroll().h(100)
      .children([0, 1, 2, 3, 4].map((i) => div().flex_none().w(200).h(100))));
  }
}"#,
    );
    let window = scrolled(&f, "strip");
    assert_eq!(window.hadjustment().upper(), 1000.0);
    let controllers = window.observe_controllers();
    let wheel = (0..controllers.n_items())
        .filter_map(|i| controllers.item(i).and_downcast::<gtk::EventControllerScroll>())
        .find(|c| c.flags() == gtk::EventControllerScrollFlags::VERTICAL)
        .expect("the sideways wheel controller");
    wheel.emit_by_name::<bool>("scroll", &[&0.0f64, &1.0f64]);
    assert!(window.hadjustment().value() > 0.0, "one notch down scrolled right");
    assert_eq!(window.vadjustment().value(), 0.0);
}

fn pull_to_refresh() {
    let f = Fixture::new(SCROLL);
    let window = scrolled(&f, "scroller");
    let revealer = window.prev_sibling().and_downcast::<gtk::Revealer>().unwrap();
    assert!(!revealer.reveals_child());
    window.emit_by_name::<()>("edge-overshot", &[&gtk::PositionType::Top]);
    assert!(revealer.reveals_child(), "the spinner shows as soon as the user pulls");
    // A second pull while refreshing is not another refresh.
    window.emit_by_name::<()>("edge-overshot", &[&gtk::PositionType::Top]);
    f.wait_for("refreshes-1");
    assert!(pump_until(Duration::from_secs(5), || {
        f.settle();
        !revealer.reveals_child()
    }));
    assert!(f.find("refreshes-1").is_some());
    // Pulling at the bottom is not a refresh.
    window.emit_by_name::<()>("edge-overshot", &[&gtk::PositionType::Bottom]);
    assert!(!revealer.reveals_child());
}

fn list_range() {
    let f = Fixture::new(LIST);
    let window = scrolled(&f, "items");
    let content = window.child().and_then(|v| v.first_child()).unwrap();
    let vadj = window.vadjustment();
    assert_eq!(vadj.upper(), 50_000.0, "every row counts toward the height");
    let visible = (vadj.page_size() / 50.0).ceil() as usize;
    assert_eq!(rows(&content), visible + 10, "visible rows plus the default prefetch");

    vadj.set_value(400.0 * 50.0);
    // Before the core answers the new range, visible rows are placeholders.
    f.layout();
    assert!(f.find("row-400").is_none());
    f.screenshot("target/agents/linux-scroll/list-placeholders.png");
    f.wait_for("row-400");
    assert!(f.find("row-0").is_none() && f.find("row-389").is_none() && f.find("row-390").is_some());
    assert_eq!(rows(&content), visible + 20);
    let row = f.expect("row-400");
    let p = row.compute_point(&content, &gtk::graphene::Point::zero()).unwrap();
    assert_eq!(p.y(), 20_000.0, "a row sits at its offset");
    assert_eq!(row.width(), content.width());
}

fn scroll_to_item() {
    let f = Fixture::new(LIST);
    let vadj = scrolled(&f, "items").vadjustment();
    f.tap("jump");
    let target = 900.0 * 50.0 - (vadj.page_size() - 50.0) / 2.0;
    let done = pump_until(Duration::from_secs(5), || vadj.value() == target);
    assert!(done, "row 900 centered: {} != {target}", vadj.value());
    f.wait_for("row-900");
}

/// The scrolled window of the node `test_id` (not of a node inside it).
fn scrolled(f: &Fixture, test_id: &str) -> gtk::ScrolledWindow {
    fn first(w: &gtk::Widget) -> Option<gtk::ScrolledWindow> {
        if let Some(s) = w.downcast_ref::<gtk::ScrolledWindow>() {
            return Some(s.clone());
        }
        let mut child = w.first_child();
        while let Some(c) = child {
            if !c.is::<NodeView>() {
                if let Some(found) = first(&c) {
                    return Some(found);
                }
            }
            child = c.next_sibling();
        }
        None
    }
    first(&f.expect(test_id)).unwrap_or_else(|| panic!("{test_id} has no scrolled window"))
}

/// Row views mounted in a list's content.
fn rows(content: &gtk::Widget) -> usize {
    let mut n = 0;
    let mut child = content.first_child();
    while let Some(c) = child {
        n += usize::from(c.is::<NodeView>());
        child = c.next_sibling();
    }
    n
}

/// The chat example opens at its newest message, in both appearances.
fn chat_opens_at_the_newest_message() {
    let f = Fixture::open_dir("../../examples/chat");
    let vadj = scrolled(&f, "messages").vadjustment();
    // scroll_to_item animates; on Broadway's throttled frame clock that takes a few seconds.
    let at_end = pump_until(Duration::from_secs(10), || {
        f.settle();
        vadj.value() > 0.0 && vadj.value() == vadj.upper() - vadj.page_size()
    });
    assert!(at_end, "at {} of {} - {}", vadj.value(), vadj.upper(), vadj.page_size());
    f.screenshot("screenshots/chat-light.png");
    f.set_dark(true);
    f.screenshot("screenshots/chat-dark.png");
}
