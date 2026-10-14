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
        ("paging settles on pages, flings turn one page", paging_settles_on_pages),
        ("the components carousel follows swipes and its buttons", components_carousel),
        ("pull to refresh shows the spinner until the script ends it", pull_to_refresh),
        ("lists render the visible range plus prefetch", list_range),
        ("scroll_to_item positions the row", scroll_to_item),
        ("a bottom-aligned list opens at its end and follows it", bottom_aligned_list),
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
    // A spring: it passes through offsets between start and target, and settles within a second.
    let mut seen = Vec::new();
    let done = pump_until(Duration::from_secs(1), || {
        seen.push(vadj.value());
        vadj.value() == 500.0
    });
    assert!(done, "scrolled to the end of the content (500), at {}", vadj.value());
    assert!(seen.iter().any(|y| *y > 7.0 && *y < 500.0), "animated: {seen:?}");
    f.wait_for("y-500");
}

const CAROUSEL: &str = r#"
import { View, div, h_flex, v_flex } from "native-shell";
export default class App extends View {
  init() { this.x = 0; }
  render() {
    const strip = h_flex().test_id("carousel").overflow_x_scroll().w_full().h(200)
      .on_scroll((e, cx) => { this.x = e.x; cx.notify(); })
      .children([0, 1, 2, 3].map((i) => div().flex_none().w_full().h(200)));
    // Gpui-component's Carousel sets this; no element method names it yet.
    strip._P().paging = true;
    return v_flex().size_full().child(div().test_id(`x-${Math.round(this.x)}`).h(4)).child(strip);
  }
}"#;

/// The scroll controller that pages a carousel's touchpad and wheel input.
fn paging_controller(window: &gtk::ScrolledWindow) -> gtk::EventControllerScroll {
    let flags = gtk::EventControllerScrollFlags::BOTH_AXES | gtk::EventControllerScrollFlags::KINETIC;
    let controllers = window.observe_controllers();
    (0..controllers.n_items())
        .filter_map(|i| controllers.item(i).and_downcast::<gtk::EventControllerScroll>())
        .find(|c| c.flags() == flags)
        .expect("the paging controller")
}

/// A touchpad gesture moving the content by `dx`, released with `velocity`.
fn swipe(c: &gtk::EventControllerScroll, dx: f64, velocity: f64) {
    c.emit_by_name::<()>("scroll-begin", &[]);
    c.emit_by_name::<bool>("scroll", &[&dx, &0.0f64]);
    c.emit_by_name::<()>("scroll-end", &[]);
    if velocity != 0.0 {
        c.emit_by_name::<()>("decelerate", &[&velocity, &0.0f64]);
    }
}

fn paging_settles_on_pages() {
    let f = Fixture::new(CAROUSEL);
    let window = scrolled(&f, "carousel");
    let hadj = window.hadjustment();
    assert_eq!(hadj.upper(), 4.0 * 390.0);
    assert!(!window.is_kinetic_scrolling(), "settling replaces GTK's momentum");
    let c = paging_controller(&window);
    let rest = |x: f64| pump_until(Duration::from_secs(1), || hadj.value() == x);

    // Less than half a page springs back; more turns the page.
    swipe(&c, 150.0, 0.0);
    assert_eq!(hadj.value(), 150.0, "the content follows the fingers");
    assert!(rest(0.0), "back to the first page, at {}", hadj.value());
    swipe(&c, 250.0, 0.0);
    assert!(rest(390.0), "on to the second page, at {}", hadj.value());
    f.wait_for("x-390");

    // A fling turns exactly one page, however fast.
    swipe(&c, 40.0, 5000.0);
    assert!(rest(780.0), "a fling turns one page, at {}", hadj.value());
    swipe(&c, -40.0, -800.0);
    assert!(rest(390.0), "and back, at {}", hadj.value());

    // A sideways wheel notch turns a page; a vertical one belongs to the page around.
    c.emit_by_name::<bool>("scroll", &[&1.0f64, &0.0f64]);
    assert!(rest(780.0), "a notch turns one page, at {}", hadj.value());
    let handled = c.emit_by_name::<bool>("scroll", &[&0.0f64, &1.0f64]);
    assert!(!handled, "a vertical wheel is not the carousel's");
    f.wait_for("x-780");

    // A narrower viewport keeps the same page in view.
    f.resize(320, HEIGHT);
    assert!(rest(640.0), "page 2 at the new width, at {}", hadj.value());
}

/// Gpui-component's Carousel on Paging: the slide a swipe rests on is the
/// one its Next and Previous buttons move from.
fn components_carousel() {
    let f = Fixture::open_dir("../../examples/components");
    f.tap("open-photos");
    f.wait_for("slides");
    let window = scrolled(&f, "slides");
    let hadj = window.hadjustment();
    let width = hadj.page_size();
    assert!(width > 0.0 && hadj.upper() > 2.0 * width, "several slides of {width}");
    let rest = |x: f64| {
        pump_until(Duration::from_secs(1), || {
            f.settle();
            hadj.value() == x
        })
    };

    swipe(&paging_controller(&window), width * 0.6, 0.0);
    assert!(rest(width), "the swipe rests on the second slide, at {}", hadj.value());
    f.tap("next");
    assert!(rest(2.0 * width), "Next goes on from the slide swiped to, at {}", hadj.value());
    f.tap("previous");
    f.tap("previous");
    assert!(rest(0.0), "Previous twice is back at the first slide, at {}", hadj.value());
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
    assert!(pump_until(Duration::from_secs(2), || {
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
    let done = pump_until(Duration::from_secs(1), || vadj.value() == target);
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

/// Whether every overlay scrollbar of `window` has faded out.
fn indicators_hidden(window: &gtk::ScrolledWindow) -> bool {
    let mut child = window.first_child();
    while let Some(w) = child {
        if w.is::<gtk::Scrollbar>() && w.is_child_visible() && w.opacity() > 0.0 {
            return false;
        }
        child = w.next_sibling();
    }
    true
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

fn bottom_aligned_list() {
    let f = Fixture::new(
        r#"
import { View, div, v_flex, list } from "native-shell";
export default class App extends View {
  init() { this.n = 100; this.panel = false; }
  render() {
    return v_flex().size_full()
      .child(div().test_id("add").h(44).on_click((_e, cx) => { this.n += 1; cx.notify(); }))
      .child(div().test_id("panel").h(44).on_click((_e, cx) => { this.panel = !this.panel; cx.notify(); }))
      .child(list("thread", this.n, (i) => `m${i}`, (i) => div().h(50).test_id(`m-${i}`))
        .alignment("bottom").test_id("thread").flex_1())
      .when(this.panel, (el) => el.child(div().h(200)));
  }
}"#,
    );
    let vadj = scrolled(&f, "thread").vadjustment();
    let end = |rows: f64| rows * 50.0 - vadj.page_size();
    assert_eq!(vadj.value(), end(100.0), "opens at its end");
    f.expect("m-99");
    assert!(f.find("m-0").is_none(), "the first range covers the end");

    f.tap("add");
    assert_eq!(vadj.value(), end(101.0), "a new row at the end stays in view");
    f.wait_for("m-100");

    let page = vadj.page_size();
    f.tap("panel");
    assert_eq!(vadj.page_size(), page - 200.0);
    assert_eq!(vadj.value(), end(101.0), "a shorter viewport keeps the end in view");

    vadj.set_value(1000.0);
    f.settle();
    f.tap("add");
    assert_eq!(vadj.value(), 1000.0, "scrolled away, the position is kept");
}

/// The chat example opens at its newest message, in both appearances.
fn chat_opens_at_the_newest_message() {
    // The example apps' blue accent, so the screenshots match the other platforms'.
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/chat");
    let f = Fixture::start_with(&example_runtime(), native_shell_gtk::InstanceOptions::default(), |i| i.load_dir(&dir));
    let vadj = scrolled(&f, "messages").vadjustment();
    let at_end = || vadj.value() > 0.0 && vadj.value() == vadj.upper() - vadj.page_size();
    // Its list is bottom-aligned: it opens at the end, with no scrolling to get there.
    assert!(at_end(), "at {} of {} - {}", vadj.value(), vadj.upper(), vadj.page_size());
    // The map attachment decodes off the main thread; the end stays in view meanwhile.
    let decoded = pump_until(Duration::from_secs(5), || native_shell_gtk::image_loads_in_flight() == 0);
    assert!(decoded, "the attachment image decodes");
    f.settle();
    assert!(at_end(), "at {} of {} - {}", vadj.value(), vadj.upper(), vadj.page_size());
    // GTK flashes its overlay indicator when the list scrolls itself to the end; let it fade.
    let window = scrolled(&f, "messages");
    let faded = pump_until(Duration::from_secs(5), || indicators_hidden(&window));
    assert!(faded, "the overlay scroll indicator fades out");
    f.screenshot("screenshots/chat-light.png");
    f.set_dark(true);
    f.screenshot("screenshots/chat-dark.png");
}
