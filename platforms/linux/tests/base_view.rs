//! The base view end to end: gestures and their payloads, press feedback,
//! keyboard activation and key routing, accessibility, box painting,
//! transforms, transitions, images and shapes.

mod support;

use std::process::ExitCode;
use std::time::Duration;

use gtk::{gdk, glib};
use native_shell_gtk::NodeView;
use support::*;

fn main() -> ExitCode {
    run(&[
        (
            "clicks carry the point and the count",
            clicks_carry_the_point_and_the_count,
        ),
        ("double and long presses", double_and_long_presses),
        ("press feedback is native", press_feedback_is_native),
        (
            "pans report phases and offsets",
            pans_report_phases_and_offsets,
        ),
        (
            "keyboard activates focusable nodes",
            keyboard_activates_focusable_nodes,
        ),
        ("keys reach the root view", keys_reach_the_root_view),
        (
            "accessibility follows the props",
            accessibility_follows_the_props,
        ),
        (
            "boxes paint radii gradients and borders",
            boxes_paint_radii_gradients_and_borders,
        ),
        (
            "transforms rotate around the center",
            transforms_rotate_around_the_center,
        ),
        (
            "hit slop extends the touch target",
            hit_slop_extends_the_touch_target,
        ),
        (
            "transitions animate frames and opacity",
            transitions_animate_frames_and_opacity,
        ),
        (
            "images load off the main thread",
            images_load_off_the_main_thread,
        ),
        ("shapes fill their paths", shapes_fill_their_paths),
        ("example screenshots", example_screenshots),
    ])
}

/// The app's event log, which it shows as the root's test id `log-…`.
fn log(f: &Fixture) -> String {
    f.settle();
    let root = f
        .find_where(|w| w.widget_name().starts_with("log-"))
        .expect("the log node");
    root.widget_name().trim_start_matches("log-").to_string()
}

fn node_view(f: &Fixture, test_id: &str) -> NodeView {
    f.expect(test_id)
        .downcast::<NodeView>()
        .expect("a node view")
}

fn gesture<T: IsA<gtk::EventController> + IsA<glib::Object>>(
    view: &NodeView,
    matches: impl Fn(&T) -> bool,
) -> T {
    let controllers = view.observe_controllers();
    (0..controllers.n_items())
        .filter_map(|i| controllers.item(i).and_downcast::<T>())
        .find(|g| matches(g))
        .expect("the gesture is attached")
}

fn primary(view: &NodeView) -> gtk::GestureClick {
    gesture(view, |g: &gtk::GestureClick| {
        g.button() == gdk::BUTTON_PRIMARY
    })
}

const LOGGER: &str = r##"
import { View, div, v_flex } from "native-shell";
export default class App extends View {
  init() { this.log = []; }
  note(s, cx) { this.log.push(s); cx.notify(); }
"##;

fn clicks_carry_the_point_and_the_count() {
    let f = Fixture::new(&format!(
        r##"{LOGGER}
  render() {{
    return v_flex().size_full().test_id(`log-${{this.log.join(",")}}`)
      .child(div().test_id("plain").w(100).h(20))
      .child(div().test_id("btn").w(100).h(44).bg("#ff0000")
        .on_click((e, cx) => this.note(`click${{e.click_count}}@${{Math.round(e.x)}}x${{Math.round(e.y)}}`, cx)));
  }}
}}"##
    ));
    // Nodes without handlers take no input.
    let plain = node_view(&f, "plain").observe_controllers();
    assert!((0..plain.n_items()).all(|i| !plain.item(i).is_some_and(|c| c.is::<gtk::Gesture>())));
    let btn = node_view(&f, "btn");
    assert!(
        btn.is_focusable(),
        "clickable nodes are keyboard focus stops"
    );
    f.click("btn");
    assert_eq!(log(&f), "click1@50x22");
    let g = primary(&btn);
    g.emit_by_name::<()>("pressed", &[&3i32, &10.0f64, &5.0f64]);
    g.emit_by_name::<()>("released", &[&3i32, &10.0f64, &5.0f64]);
    assert_eq!(log(&f), "click1@50x22,click3@10x5");
    // A release outside the node is no click.
    g.emit_by_name::<()>("pressed", &[&1i32, &10.0f64, &5.0f64]);
    g.emit_by_name::<()>("released", &[&1i32, &150.0f64, &5.0f64]);
    assert_eq!(log(&f), "click1@50x22,click3@10x5");
}

fn double_and_long_presses() {
    let f = Fixture::new(&format!(
        r##"{LOGGER}
  render() {{
    return v_flex().size_full().test_id(`log-${{this.log.join(",")}}`)
      .child(div().test_id("btn").w(100).h(44)
        .on_click((e, cx) => this.note(`click${{e.click_count}}`, cx))
        .on_double_click((e, cx) => this.note("double", cx))
        .on_long_press((e, cx) => this.note(`long@${{Math.round(e.x)}}`, cx)));
  }}
}}"##
    ));
    let btn = node_view(&f, "btn");
    let g = primary(&btn);
    for n in [1i32, 2] {
        g.emit_by_name::<()>("pressed", &[&n, &10.0f64, &10.0f64]);
        g.emit_by_name::<()>("released", &[&n, &10.0f64, &10.0f64]);
    }
    assert_eq!(log(&f), "click1,double");

    // Held for 500 ms: a long press, and no click on release.
    g.emit_by_name::<()>("pressed", &[&1i32, &30.0f64, &10.0f64]);
    pump_for(Duration::from_millis(350));
    assert_eq!(log(&f), "click1,double", "not yet");
    pump_for(Duration::from_millis(300));
    g.emit_by_name::<()>("released", &[&1i32, &30.0f64, &10.0f64]);
    assert_eq!(log(&f), "click1,double,long@30");

    // The secondary button is the desktop's long press.
    let secondary = gesture(&btn, |g: &gtk::GestureClick| {
        g.button() == gdk::BUTTON_SECONDARY
    });
    secondary.emit_by_name::<()>("pressed", &[&1i32, &40.0f64, &10.0f64]);
    assert_eq!(log(&f), "click1,double,long@30,long@40");
}

fn press_feedback_is_native() {
    let f = Fixture::new(&format!(
        r##"{LOGGER}
  render() {{
    return v_flex().size_full().bg("#ffffff").test_id(`log-${{this.log.join(",")}}`)
      .child(div().test_id("row").w(100).h(44).bg("#ff0000").active((s) => s.opacity(0.5))
        .on_press_in((e, cx) => this.note("in", cx))
        .on_press_out((e, cx) => this.note("out", cx)));
  }}
}}"##
    ));
    let row = node_view(&f, "row");
    let g = primary(&row);
    g.emit_by_name::<()>("pressed", &[&1i32, &10.0f64, &10.0f64]);
    assert!(
        (row.opacity() - 0.5).abs() < 0.01,
        "pressed look without a round trip"
    );
    let tinted = f.pixel(10, 10);
    assert!(
        tinted >> 8 & 0xff >= 0x7e && tinted >> 8 & 0xff <= 0x81,
        "half red over white: {tinted:08x}"
    );
    g.emit_by_name::<()>("released", &[&1i32, &10.0f64, &10.0f64]);
    assert_eq!(row.opacity(), 1.0);
    assert_eq!(log(&f), "in,out");
    // Another widget claiming the press (a scroll starting) releases it too.
    g.emit_by_name::<()>("pressed", &[&1i32, &10.0f64, &10.0f64]);
    g.emit_by_name::<()>("cancel", &[&None::<gdk::EventSequence>]);
    assert_eq!(row.opacity(), 1.0);
    assert_eq!(log(&f), "in,out,in,out");
}

fn pans_report_phases_and_offsets() {
    let f = Fixture::new(&format!(
        r##"{LOGGER}
  render() {{
    return v_flex().size_full().test_id(`log-${{this.log.join(",")}}`)
      .child(div().test_id("card").w(200).h(200)
        .on_click((e, cx) => this.note("click", cx))
        .on_pan((e, cx) => this.note(`${{e.phase}}:${{Math.round(e.dx)}}`, cx)));
  }}
}}"##
    ));
    let card = node_view(&f, "card");
    let drag = gesture(&card, |_: &gtk::GestureDrag| true);
    let press = primary(&card);
    press.emit_by_name::<()>("pressed", &[&1i32, &50.0f64, &50.0f64]);
    // Pan events are continuous: the core keeps the latest per node, so let
    // each one through before the next.
    drag.emit_by_name::<()>("drag-begin", &[&50.0f64, &50.0f64]);
    drag.emit_by_name::<()>("drag-update", &[&2.0f64, &0.0f64]);
    assert_eq!(log(&f), "", "within the slop nothing pans");
    drag.emit_by_name::<()>("drag-update", &[&12.0f64, &1.0f64]);
    assert_eq!(log(&f), "began:12");
    drag.emit_by_name::<()>("drag-update", &[&30.0f64, &2.0f64]);
    assert_eq!(log(&f), "began:12,changed:30");
    drag.emit_by_name::<()>("drag-end", &[&30.0f64, &2.0f64]);
    press.emit_by_name::<()>("released", &[&1i32, &80.0f64, &52.0f64]);
    assert_eq!(
        log(&f),
        "began:12,changed:30,ended:30",
        "a pan is never also a click"
    );
}

fn keyboard_activates_focusable_nodes() {
    let f = Fixture::new(&format!(
        r##"{LOGGER}
  render() {{
    return v_flex().size_full().test_id(`log-${{this.log.join(",")}}`)
      .child(div().test_id("btn").w(100).h(44).on_click((e, cx) => this.note(`click${{e.click_count}}@${{e.x}}`, cx)));
  }}
}}"##
    ));
    let btn = node_view(&f, "btn");
    assert!(btn.grab_focus());
    // Enter and Space are bound to the activation, which clicks at the center.
    assert!(btn.activate_action("node.activate", None).is_ok());
    assert_eq!(log(&f), "click1@50");
}

fn keys_reach_the_root_view() {
    let f = Fixture::new(&format!(
        r##"{LOGGER}
  render() {{
    return v_flex().size_full().test_id(`log-${{this.log.join(",")}}`)
      .on_key_down((e, cx) => this.note(`${{e.keystroke}}`, cx))
      .child(div().w(100).h(44));
  }}
}}"##
    ));
    let window = f
        .expect("log-")
        .root()
        .and_downcast::<gtk::Window>()
        .expect("a window");
    let controllers = window.observe_controllers();
    let keys = (0..controllers.n_items())
        .filter_map(|i| {
            controllers
                .item(i)
                .and_downcast::<gtk::EventControllerKey>()
        })
        .find(|c| {
            c.propagation_phase() == gtk::PropagationPhase::Capture
                && c.name().as_deref() == Some("native-shell-keys")
        })
        .expect("the surface observes the window's keys");
    use glib::translate::IntoGlib;
    let handled = keys.emit_by_name::<bool>(
        "key-pressed",
        &[
            &gdk::Key::s.into_glib(),
            &0u32,
            &gdk::ModifierType::CONTROL_MASK,
        ],
    );
    assert!(!handled, "keys are observed, never consumed");
    keys.emit_by_name::<bool>(
        "key-pressed",
        &[
            &gdk::Key::Escape.into_glib(),
            &0u32,
            &gdk::ModifierType::empty(),
        ],
    );
    assert_eq!(log(&f), "ctrl-s,escape");
}

fn accessibility_follows_the_props() {
    let f = Fixture::new(
        r##"
import { View, div, v_flex } from "native-shell";
export default class App extends View {
  render() {
    return v_flex().size_full()
      .child(div().test_id("title").h(20).role("header").accessibility_label("Inbox"))
      .child(div().test_id("btn").h(44).on_click(() => {}).accessibility_label("Compose").accessibility_hint("Starts a new message"))
      .child(div().test_id("link").h(20).role("link").on_click(() => {}))
      .child(div().test_id("box").h(20));
  }
}"##,
    );
    use gtk::prelude::AccessibleExt;
    assert_eq!(
        AccessibleExt::accessible_role(&f.expect("title")),
        gtk::AccessibleRole::Heading
    );
    assert_eq!(
        AccessibleExt::accessible_role(&f.expect("btn")),
        gtk::AccessibleRole::Button,
        "clickable"
    );
    assert_eq!(
        AccessibleExt::accessible_role(&f.expect("box")),
        gtk::AccessibleRole::Generic
    );
    assert_eq!(
        f.expect("link").cursor().map(|c| c.name()),
        Some(Some("pointer".into()))
    );
    assert!(
        f.expect("btn").cursor().is_none(),
        "buttons keep the arrow, as GTK's do"
    );
}

fn boxes_paint_radii_gradients_and_borders() {
    let f = Fixture::new(
        r##"
import { View, div, v_flex } from "native-shell";
export default class App extends View {
  render() {
    return v_flex().size_full().bg("#ffffff").p(20).gap(20)
      .child(div().test_id("pill").w(100).h(40).rounded(20).bg("#0000ff"))
      .child(div().test_id("grad").w(100).h(40).bg_gradient(90, "#ff0000", "#0000ff"))
      .child(div().test_id("framed").w(100).h(40).border_4().border_color("#00ff00").bg("#ffffff"))
      .child(div().test_id("shadowed").w(100).h(40).bg("#ffffff").shadow("#000000", 0, 8, 0));
  }
}"##,
    );
    // Rounded corners: the corner pixel stays the parent's color, the middle is filled.
    assert_eq!(f.pixel(21, 21), 0xffffffff);
    assert_eq!(f.pixel(70, 40), 0xff0000ff);
    // A 90° gradient runs left to right.
    let (left, right) = (f.pixel(21, 100), f.pixel(118, 100));
    assert!(
        left >> 16 & 0xff > 0xf0 && left & 0xff < 0x10,
        "left is red: {left:08x}"
    );
    assert!(
        right & 0xff > 0xf0 && right >> 16 & 0xff < 0x10,
        "right is blue: {right:08x}"
    );
    // Borders are inside the box.
    assert_eq!(f.pixel(21, 141), 0xff00ff00);
    assert_eq!(f.pixel(30, 150), 0xffffffff);
    // An offset shadow shows below the box.
    assert_eq!(f.pixel(70, 244), 0xff000000);
    assert_eq!(f.pixel(70, 236), 0xffffffff);
    f.screenshot("target/agents/linux-visual/boxes.png");
}

fn transforms_rotate_around_the_center() {
    let f = Fixture::new(
        r##"
import { View, div, v_flex } from "native-shell";
export default class App extends View {
  render() {
    return v_flex().size_full().p(100)
      .child(div().test_id("bar").w(100).h(20).rotate(90).bg("#000000"))
      .child(div().test_id("moved").w(10).h(10).translate(15, 5).scale(2).bg("#000000"));
  }
}"##,
    );
    let b = f.bounds(&f.expect("bar"));
    // A 100×20 bar at (100, 100) turned a quarter around its center (150, 110).
    assert!(
        (b.x() - 140.0).abs() < 0.01 && (b.y() - 60.0).abs() < 0.01,
        "{b:?}"
    );
    assert!(
        (b.width() - 20.0).abs() < 0.01 && (b.height() - 100.0).abs() < 0.01,
        "{b:?}"
    );
    let m = f.bounds(&f.expect("moved"));
    // Translated by (15, 5), then doubled around its center.
    assert!(
        (m.x() - 110.0).abs() < 0.01
            && (m.y() - 120.0).abs() < 0.01
            && (m.width() - 20.0).abs() < 0.01,
        "{m:?}"
    );
}

fn hit_slop_extends_the_touch_target() {
    let f = Fixture::new(
        r##"
import { View, div, v_flex } from "native-shell";
export default class App extends View {
  render() {
    return v_flex().size_full().p(40)
      .child(div().test_id("small").w(20).h(20).hit_slop([10, 0, 10, 12]).on_click(() => {}));
  }
}"##,
    );
    let small = f.expect("small");
    assert!(small.contains(-12.0, -10.0) && small.contains(19.0, 29.0));
    assert!(!small.contains(21.0, 5.0) && !small.contains(-13.0, 5.0));
    let surface = f
        .find_where(|w| w.type_().name() == "NshSurfaceView")
        .expect("surface");
    let hit = surface
        .pick(30.0, 45.0, gtk::PickFlags::DEFAULT)
        .expect("picked");
    assert_eq!(hit.widget_name(), "small", "picking honors the slop");
}

fn transitions_animate_frames_and_opacity() {
    let f = Fixture::new(
        r##"
import { View, div, v_flex } from "native-shell";
export default class App extends View {
  init() { this.open = false; }
  render() {
    return v_flex().size_full()
      .child(div().test_id("toggle").h(20).on_click((_e, cx) => { this.open = !this.open; cx.notify(); }))
      .child(div().test_id("panel").h(this.open ? 200 : 100).w(100).bg("#000000")
        .opacity(this.open ? 1 : 0.2)
        .transition("height", '{"duration": 200, "easing": "linear"}')
        .transition("opacity", '{"duration": 100, "easing": "linear"}'));
  }
}"##,
    );
    let panel = node_view(&f, "panel");
    assert_eq!(panel.presented_frame().map(|r| r.height), Some(100.0));
    assert!((panel.opacity() - 0.2).abs() < 1e-6);
    f.tap("toggle");
    // The model is at the target; the screen starts from where it was.
    assert_eq!(panel.frame().map(|r| r.height), Some(200.0));
    assert!(panel.is_animating());
    let t0 = panel.frame_clock().map_or(0, |c| c.frame_time());
    panel.step_animations(t0);
    assert_eq!(panel.presented_frame().map(|r| r.height), Some(100.0));
    panel.step_animations(t0 + 50_000);
    let h = panel.presented_frame().map_or(0.0, |r| r.height);
    assert!((h - 125.0).abs() < 0.5, "linear quarter of the way: {h}");
    assert!(
        (panel.opacity() - 0.6).abs() < 0.01,
        "opacity halfway: {}",
        panel.opacity()
    );
    assert!(!panel.step_animations(t0 + 200_000));
    assert_eq!(panel.presented_frame().map(|r| r.height), Some(200.0));
    assert_eq!(panel.opacity(), 1.0);
    assert!(!panel.is_animating());

    // Reduce Motion: frames jump, fades still run.
    let settings = gtk::Settings::default().expect("settings");
    settings.set_gtk_enable_animations(false);
    f.tap("toggle");
    assert_eq!(panel.presented_frame().map(|r| r.height), Some(100.0));
    assert!(panel.is_animating(), "the fade still runs");
    panel.finish_animations();
    settings.set_gtk_enable_animations(true);
}

/// A 4×4 PNG of one color, base64.
fn png(color: [u8; 4]) -> String {
    let bytes: Vec<u8> = color.iter().copied().cycle().take(4 * 4 * 4).collect();
    let texture = gdk::MemoryTexture::new(
        4,
        4,
        gdk::MemoryFormat::R8g8b8a8,
        &glib::Bytes::from(&bytes),
        16,
    );
    glib::base64_encode(&texture.save_to_png_bytes()).to_string()
}

fn images_load_off_the_main_thread() {
    let green = png([0, 255, 0, 255]);
    let blue = png([0, 0, 255, 255]);
    let main = format!(
        r##"
import {{ View, div, v_flex, image }} from "native-shell";
export default class App extends View {{
  render() {{
    return v_flex().size_full().bg("#ffffff")
      .child(image("photo.png").test_id("file").w(40).h(40))
      .child(image("data:image/png;base64,{blue}").test_id("inline").w(40).h(20).object_fit("contain"))
      .child(image("photo.png").test_id("tinted").w(40).h(40).tint("#ff0000").rounded(20));
  }}
}}"##
    );
    let f = Fixture::with_files(
        MANIFEST,
        &[
            ("main.js", &main),
            ("photo.png", &format!("data:image/png;base64,{green}")),
        ],
    );
    assert!(
        pump_until(Duration::from_secs(5), || f.pixel(20, 20) == 0xff00ff00),
        "file image drawn"
    );
    // Contain: a square image in a 40×20 box is 20 wide, centered.
    assert!(
        pump_until(Duration::from_secs(5), || f.pixel(20, 50) == 0xff0000ff),
        "data URI drawn"
    );
    assert_eq!(f.pixel(5, 50), 0xffffffff);
    // Tinted: the alpha keeps the shape, the color is the tint; rounded corners clip.
    assert!(
        pump_until(Duration::from_secs(5), || f.pixel(20, 80) == 0xffff0000),
        "tinted"
    );
    assert_eq!(f.pixel(1, 61), 0xffffffff);
}

fn shapes_fill_their_paths() {
    let f = Fixture::new(
        r##"
import { View, v_flex, shape } from "native-shell";
export default class App extends View {
  render() {
    return v_flex().size_full().bg("#ffffff")
      .child(shape("M0 0H10V10H0Z", { view_box: [0, 0, 10, 10] }).test_id("square").w(40).h(20).fill("#000000"))
      .child(shape("M2 2l6 6m0-6l-6 6", { view_box: [0, 0, 10, 10] }).test_id("cross").w(40).h(40).stroke("#ff0000", 2));
  }
}"##,
    );
    // xMidYMid meet: a 10×10 box in 40×20 is 20×20, centered.
    assert_eq!(f.pixel(20, 10), 0xff000000);
    assert_eq!(f.pixel(5, 10), 0xffffffff);
    // Stroked diagonals cross in the middle.
    assert_eq!(f.pixel(20, 40), 0xffff0000);
    assert_eq!(f.pixel(20, 25), 0xffffffff);
}

/// The accent the iOS and Android example apps give the shell, so example
/// screenshots compare with theirs.
fn example_runtime() -> native_shell_gtk::Runtime {
    let tokens = |c: &str| std::iter::once(("primary".to_string(), c.to_string())).collect();
    let theme = native_shell_gtk::ThemeConfig { light: tokens("#2563eb"), dark: tokens("#60a5fa") };
    let runtime = native_shell_gtk::Runtime::new();
    runtime.set_theme(Some(theme)).expect("theme");
    runtime
}

fn open_example(name: &str) -> Fixture {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples").join(name);
    let data = std::env::temp_dir().join(format!("native-shell-visual-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&data);
    let options = native_shell_gtk::InstanceOptions { data_dir: Some(data), ..Default::default() };
    Fixture::start_with(&example_runtime(), options, |i| i.load_dir(&dir))
}

fn screenshots(f: &Fixture, name: &str) {
    f.set_dark(false);
    f.screenshot(format!("screenshots/{name}-light.png"));
    f.set_dark(true);
    f.screenshot(format!("screenshots/{name}-dark.png"));
    f.set_dark(false);
}

fn example_screenshots() {
    gtk::Settings::default().expect("settings").set_gtk_enable_animations(false);
    let f = open_example("components");
    f.expect("index");
    screenshots(&f, "components");
    f.click("open-buttons");
    pump_for(Duration::from_millis(100));
    f.settle();
    screenshots(&f, "components-buttons");
    drop(f);

    let f = open_example("gpui-todolist");
    screenshots(&f, "gpui-todolist-empty");
    let entry = f.find_where(|w| w.is::<gtk::Text>()).and_downcast::<gtk::Text>().expect("the draft field");
    for todo in ["Buy oat milk", "Renew passport before the trip in March", "Call the dentist"] {
        entry.set_text(todo);
        f.settle();
        entry.emit_by_name::<()>("activate", &[]);
        f.settle();
    }
    screenshots(&f, "gpui-todolist");
}
