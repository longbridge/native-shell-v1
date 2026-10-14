//! "gpui-component" conversation components: Message, Bubble, Attachment,
//! Marker and MessageScroller.

use native_shell::core::protocol::{Command, EventType, Prop, PropValue, ViewType};
use native_shell::core::theme::{Appearance, Theme};
use native_shell::testing::Harness;

fn color(appearance: Appearance, token: &str) -> u32 {
    Theme::default_for(appearance).color(token).unwrap()
}

fn harness(body: &str) -> Harness {
    let src = format!(
        r#"
import {{ View, div, image }} from "native-shell";
import {{ Message, Bubble, Attachment, Marker, MessageScroller, MessageScrollerState }} from "gpui-component";
{body}
"#
    );
    Harness::new(&[("main.js", &src), ("photo.png", PNG)])
}

fn app(body: &str) -> Harness {
    let h = harness(body);
    assert!(h.load_error().is_none(), "{:?} {:?}", h.load_error(), h.logs());
    h
}

/// A 1×1 PNG.
const PNG: &str = "\u{89}PNG";

fn errors(h: &Harness) -> Vec<String> {
    h.error_logs()
}

fn frame_x(h: &Harness, id: &str) -> (f32, f32) {
    let f = h.frame(id);
    (f.x, f.x + f.width)
}

#[test]
fn bubbles_follow_their_message_edge_and_variant() {
    let h = app(
        r#"
export default class C extends View {
  render() {
    return div().w(390).flex().flex_col()
      .child(new Message().test_id("in")
        .child(new Bubble().variant("secondary").test_id("b-in").child("Are we still on for Saturday?")))
      .child(new Message().alignment("end").test_id("out")
        .child(new Bubble().test_id("b-out").child("Yes!"))
        .child(new Bubble().tail().test_id("b-tail").child("Ridge trail?")))
      .child(new Bubble().variant("destructive").test_id("b-bad").child("Not delivered"));
  }
}
"#,
    );
    assert!(errors(&h).is_empty(), "{:?}", errors(&h));
    let light = Appearance::Light;
    assert_eq!(h.prop("b-in", Prop::BackgroundColor), Some(PropValue::Color(color(light, "secondary"))));
    assert_eq!(h.prop("b-out", Prop::BackgroundColor), Some(PropValue::Color(color(light, "primary"))));
    // Incoming bubbles start at the leading edge, outgoing ones end at the trailing edge.
    let (in_left, _) = frame_x(&h, "b-in");
    let (_, out_right) = frame_x(&h, "b-out");
    assert!(in_left < 1.0, "{}", h.dump());
    assert!(out_right > 389.0, "{}", h.dump());
    // A bubble stops at 80% of the row.
    assert!(h.frame("b-in").width <= 390.0 * 0.8 + 0.5);
    // The tail tightens the corner toward the sender (bottom right for your own).
    match h.prop("b-tail", Prop::CornerRadii) {
        Some(PropValue::F32Array(r)) => assert_eq!(r, vec![18.0, 18.0, 6.0, 18.0]),
        other => panic!("{other:?}"),
    }
    // Destructive is a translucent red surface with red text.
    match h.prop("b-bad", Prop::BackgroundColor) {
        Some(PropValue::Color(c)) => {
            assert_eq!(c & 0x00FF_FFFF, color(light, "destructive") & 0x00FF_FFFF);
            assert!(c >> 24 < 0x40, "translucent: {c:08x}");
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_message_places_avatar_header_and_footer_around_its_content() {
    let h = app(
        r#"
export default class C extends View {
  render() {
    return div().w(390).flex().flex_col()
      .child(new Message()
        .avatar(div().test_id("avatar").size_8().rounded_full().bg("secondary"))
        .header("Maya Chen")
        .footer(div().test_id("read").child("Read 9:21 AM"))
        .child(new Bubble().variant("secondary").test_id("bubble").child("Forecast says sunny.")))
      .child(new Message().alignment("end")
        .footer(div().test_id("delivered").child("Delivered"))
        .child(new Bubble().test_id("mine").child("8:15 works.")));
  }
}
"#,
    );
    assert!(errors(&h).is_empty(), "{:?}", errors(&h));
    let avatar = h.frame("avatar");
    let bubble = h.frame("bubble");
    // The avatar sits beside the content, level with its last bubble.
    assert!(avatar.x < 1.0 && bubble.x >= avatar.width + 7.5, "{}", h.dump());
    assert!((avatar.y + avatar.height - (bubble.y + bubble.height)).abs() < 0.5, "{}", h.dump());
    // The header names the sender above the bubble; the footer is under it, in the content column.
    let header = h.find_text("Maya Chen").expect("header");
    let header_frame = h.native().nodes[&header].frame.unwrap();
    assert!(header_frame.y < bubble.y);
    let read = h.frame("read");
    assert!(read.y >= bubble.y + bubble.height && read.x >= 40.0, "{}", h.dump());
    // An outgoing message's footer trails at the end.
    let (_, mine_right) = frame_x(&h, "mine");
    let (_, delivered_right) = frame_x(&h, "delivered");
    assert!(mine_right > 389.0 && delivered_right > 380.0, "{}", h.dump());
}

#[test]
fn bubble_and_message_refuse_unknown_values_with_gpui_shell_errors() {
    for (call, message) in [
        (r#"new Bubble().variant("loud")"#, "unsupported Bubble.variant value `loud`"),
        (r#"new Bubble().alignment("left")"#, "unsupported Bubble.alignment value `left`"),
        (r#"new Message().alignment("center")"#, "unsupported Message.alignment value `center`"),
        (r#"new Marker("")"#, "Marker expects a non-empty stable id"),
        (r#"new Marker("m").variant("dotted")"#, "unsupported Marker.variant value `dotted`"),
        (r#"new Attachment(" ")"#, "Attachment expects a non-empty stable id"),
        (r#"new Attachment("a").status("done")"#, "unsupported Attachment.status value `done`"),
        (r#"new MessageScroller("m", {}, () => null)"#, "MessageScroller expects a non-empty id, MessageScrollerState, and row renderer"),
        (r#"MessageScrollerState(2)"#, "MessageScrollerState(item_count) cannot run during render"),
    ] {
        let h = harness(&format!("export default class C extends View {{ render() {{ return div().child({call}); }} }}"));
        let error = h.load_error().map(|e| e.to_string()).unwrap_or_default();
        assert!(error.contains(message), "{call}: {error}");
    }
}

#[test]
fn markers_label_days_and_unread_lines() {
    let h = app(
        r#"
export default class C extends View {
  render() {
    return div().w(390).flex().flex_col()
      .child(new Marker("today").alignment("center").test_id("day").child("Today 9:12 AM"))
      .child(new Marker("unread").variant("separator").test_id("unread").child("2 unread messages"))
      .child(new Marker("start").variant("separator").alignment("start").test_id("lead").child("Earlier"))
      .child(new Marker("notice").variant("border").test_id("notice").child("Messages are end-to-end encrypted"))
      .child(new Marker("typing").loading(true).test_id("typing").child("Maya is typing"))
      .child(new Marker("thinking").loading(true).loading_style("shimmer").test_id("thinking").child("Summarizing"));
  }
}
"#,
    );
    assert!(errors(&h).is_empty(), "{:?}", errors(&h));
    // A centered plain marker: the label sits in the middle of the row.
    let label = h.native().nodes[&h.find_text("Today 9:12 AM").unwrap()].frame.unwrap();
    let day = h.frame("day");
    assert!((label.x + label.width / 2.0 - (day.x + day.width / 2.0)).abs() < 1.0, "{}", h.dump());
    // A separator draws a hairline on each side, hidden from assistive technologies.
    let unread = h.find("unread").unwrap();
    let lines: Vec<u32> = h.native().nodes[&unread]
        .children
        .iter()
        .copied()
        .filter(|n| h.native().prop(*n, Prop::A11yHidden) == Some(&PropValue::Bool(true)))
        .collect();
    assert_eq!(lines.len(), 2, "{}", h.dump());
    let line_color = color(Appearance::Light, "border");
    assert!(lines.iter().all(|n| h.native().prop(*n, Prop::BackgroundColor) == Some(&PropValue::Color(line_color))));
    // Aligned to the start, only the trailing line remains.
    let lead = h.find("lead").unwrap();
    let hidden = h.native().nodes[&lead]
        .children
        .iter()
        .filter(|n| h.native().prop(**n, Prop::A11yHidden) == Some(&PropValue::Bool(true)))
        .count();
    assert_eq!(hidden, 1);
    // The border variant underlines the row.
    match h.prop("notice", Prop::BorderWidths) {
        Some(PropValue::F32Array(w)) => assert_eq!(w, vec![0.0, 0.0, 1.0, 0.0]),
        other => panic!("{other:?}"),
    }
    // Loading puts a spinner before the words; the shimmer style keeps the words alone. Both announce busy.
    assert_eq!(h.nodes_of(ViewType::Spinner).len(), 1);
    for id in ["typing", "thinking"] {
        assert_eq!(h.prop(id, Prop::A11yState), Some(PropValue::Json(r#"{"busy":true}"#.into())), "{id}");
    }
    assert_eq!(h.prop("day", Prop::A11yState), None);
}

#[test]
fn attachments_draw_their_lifecycle_and_report_remove_and_retry() {
    let mut h = app(
        r#"
export default class C extends View {
  init() { this.log = []; }
  render() {
    const note = (s) => (_e, cx) => { this.log.push(s); cx.notify(); };
    return div().w(390).flex().flex_col().gap_2()
      .child(new Attachment("pdf").test_id("pdf").title("trail-map.pdf").description("PDF · 2.4 MB").on_click(note("open")))
      .child(new Attachment("up").test_id("up").status("uploading").progress(68.4).title("route.gpx").description("GPX · 84 KB").on_remove(note("remove")))
      .child(new Attachment("bad").test_id("bad").status("failed").title("permit.jpg").description("Upload failed").on_retry(note("retry")))
      .child(new Attachment("next").test_id("next").status("pending").size("small").title("packing-list.txt"))
      .child(new Attachment("photo").test_id("photo").axis("vertical").size("large").status("processing").media(image("photo.png")))
      .child(new Attachment("styled").test_id("styled").size(48))
      .child(div().test_id("log").child(this.log.join(",")));
  }
}
"#,
    );
    assert!(errors(&h).is_empty(), "{:?}", errors(&h));
    let light = Appearance::Light;
    // A complete chip: bordered card with title and description, a button when it opens.
    assert_eq!(h.prop("pdf", Prop::BorderColor), Some(PropValue::Color(color(light, "border"))));
    assert!(h.texts().contains(&"trail-map.pdf".to_string()));
    assert!(h.texts().contains(&"PDF · 2.4 MB".to_string()));
    match h.prop("pdf", Prop::A11yRole) {
        Some(PropValue::I32(_)) => {}
        other => panic!("{other:?}"),
    }
    h.tap("pdf");
    assert_eq!(h.text_of("log"), "open");

    // Uploading: the percentage joins the description and a progress bar announces it.
    assert!(h.texts().contains(&"GPX · 84 KB · 68%".to_string()), "{:?}", h.texts());
    let bar = h
        .native()
        .nodes
        .keys()
        .copied()
        .find(|n| h.native().prop(*n, Prop::A11yValue) == Some(&PropValue::Str("68%".into())))
        .expect("progress bar");
    assert_eq!(h.native().prop(bar, Prop::A11yLabel), Some(&PropValue::Str("Upload progress".into())));
    assert!(h.nodes_of(ViewType::Spinner).len() >= 2, "uploading and processing spin");
    assert_eq!(h.prop("up-remove", Prop::A11yLabel), Some(PropValue::Str("Remove".into())));
    h.tap("up-remove");
    assert_eq!(h.text_of("log"), "open,remove");

    // Failed: destructive edge and description, and a Retry control.
    assert_eq!(h.prop("bad", Prop::BorderColor), Some(PropValue::Color(color(light, "destructive"))));
    h.tap("bad-retry");
    assert_eq!(h.text_of("log"), "open,remove,retry");

    // Pending is dashed; a small chip is narrower than a medium one.
    assert!(h.frame("next").width < h.frame("pdf").width);

    // A vertical image tile is a square of the image under a scrim while processing.
    let photo = h.frame("photo");
    assert!((photo.width - photo.height).abs() < 0.5 && photo.width >= 200.0, "{}", h.dump());
    let img = h.nodes_of(ViewType::Image)[0];
    assert_eq!(h.native().prop(img, Prop::ContentMode), Some(&PropValue::I32(0)), "cover");

    // A length passed to size() is GPUI's size style.
    assert_eq!(h.frame("styled").width, 48.0);
}

#[test]
fn bubbles_follow_the_dark_appearance() {
    let mut h = app(
        r#"
export default class C extends View {
  render() {
    return div().child(new Message().child(new Bubble().variant("secondary").test_id("b").child("Hi")))
      .child(new Bubble().variant("destructive").test_id("bad").child("Not delivered"));
  }
}
"#,
    );
    h.set_environment(native_shell::core::host::Environment {
        appearance: Appearance::Dark,
        ..native_shell::core::host::Environment::default()
    });
    h.settle();
    let dark = Appearance::Dark;
    assert_eq!(h.prop("b", Prop::BackgroundColor), Some(PropValue::Color(color(dark, "secondary"))));
    match h.prop("bad", Prop::BackgroundColor) {
        Some(PropValue::Color(c)) => assert_eq!(c & 0x00FF_FFFF, color(dark, "destructive") & 0x00FF_FFFF),
        other => panic!("{other:?}"),
    }
}

const THREAD: &str = r#"
export default class C extends View {
  init() {
    this.rows = ["m1", "m2", "m3"];
    this.scroller = MessageScrollerState(this.rows.length);
  }
  render() {
    return div().size_full().flex().flex_col()
      .child(new MessageScroller("thread", this.scroller, (i) =>
          new Message().key(this.rows[i]).child(new Bubble().test_id(`b-${this.rows[i]}`).child(`Message ${this.rows[i]}`)))
        .test_id("thread")
        .jump_button_label("Latest messages")
        .flex_1())
      .child(div().test_id("info").child(`${this.scroller.item_count()} ${this.scroller.is_following_tail()}`))
      .child(div().test_id("send").on_click((_e, cx) => {
        this.rows = [...this.rows, `m${this.rows.length + 1}`];
        this.scroller.append(1);
        cx.notify();
      }).child("Send"))
      .child(div().test_id("older").on_click((_e, cx) => {
        this.rows = ["e1", "e2", ...this.rows];
        this.scroller.prepend(2);
        cx.notify();
      }).child("Older"));
  }
}
"#;

#[test]
fn message_scroller_is_a_bottom_aligned_list_that_grows_with_its_state() {
    let mut h = app(THREAD);
    assert!(errors(&h).is_empty(), "{:?}", errors(&h));
    assert_eq!(h.view_type("thread"), ViewType::List);
    assert_eq!(h.prop("thread", Prop::ListAlignment), Some(PropValue::I32(1)));
    assert_eq!(h.prop("thread", Prop::ItemCount), Some(PropValue::I32(3)));
    assert_eq!(h.text_of("info"), "3 true");
    assert!(h.find("b-m3").is_some());

    h.tap("send");
    assert_eq!(h.prop("thread", Prop::ItemCount), Some(PropValue::I32(4)));
    assert!(h.find("b-m4").is_some(), "{}", h.dump());

    // Older rows go above; the rows already shown keep their identity (keyed by the message).
    let m1 = h.find("b-m1").unwrap();
    h.tap("older");
    assert_eq!(h.prop("thread", Prop::ItemCount), Some(PropValue::I32(6)));
    h.scroll_list("thread", 0, 6);
    assert!(h.find("b-e1").is_some(), "{}", h.dump());
    assert_eq!(h.find("b-m1"), Some(m1), "the row was moved, not rebuilt");
}

#[test]
fn message_scroller_offers_a_jump_to_latest_once_scrolled_away() {
    let mut h = app(THREAD);
    let list = h.find("thread").unwrap();
    assert!(h.find("thread-jump").is_none());

    // Reading older rows: the button appears and the state stops following the tail.
    let away = r#"{"x":0,"y":0,"content_width":390,"content_height":1200,"viewport_width":390,"viewport_height":600}"#;
    h.dispatch(list, EventType::Scroll, away);
    assert!(h.find("thread-jump").is_some(), "{}", h.dump());
    assert_eq!(h.prop("thread-jump", Prop::A11yLabel), Some(PropValue::Str("Latest messages".into())));
    assert_eq!(h.text_of("info"), "3 false");

    // The button scrolls to the last row and goes away.
    h.clear_commands();
    h.tap("thread-jump");
    let scrolled = h.commands().iter().any(|(node, c, p)| {
        *node == list && *c == Command::ScrollToIndex && p.contains(r#""index":2"#) && p.contains(r#""position":"bottom""#)
    });
    assert!(scrolled, "{:?}", h.commands());
    assert!(h.find("thread-jump").is_none());
    assert_eq!(h.text_of("info"), "3 true");

    // Scrolling back to the end by hand hides it too.
    h.dispatch(list, EventType::Scroll, away);
    assert!(h.find("thread-jump").is_some());
    let end = r#"{"x":0,"y":600,"content_width":390,"content_height":1200,"viewport_width":390,"viewport_height":600}"#;
    h.dispatch(list, EventType::Scroll, end);
    assert!(h.find("thread-jump").is_none());
}

#[test]
fn message_scroller_without_jump_button_and_with_empty_rows() {
    let h = app(
        r#"
export default class C extends View {
  init() { this.scroller = MessageScrollerState(2); }
  render() {
    return div().size_full().flex().flex_col().child(
      new MessageScroller("t", this.scroller, (i) => (i === 0 ? null : new Marker("day").child("Today")))
        .test_id("t").jump_button(false).scrollbar(true).flex_1());
  }
}
"#,
    );
    assert!(errors(&h).is_empty(), "{:?}", errors(&h));
    assert_eq!(h.prop("t", Prop::ItemCount), Some(PropValue::I32(2)));
    assert!(h.texts().contains(&"Today".to_string()));
}
