//! "gpui-component" feedback components: Alert, Badge, Tag, Skeleton, Empty,
//! Kbd, Label, Separator, ShimmerText and Notification.

use native_shell::core::host::Environment;
use native_shell::core::protocol::{Command, Prop, PropValue, ROOT_NODE};
use native_shell::core::theme::{Appearance, Theme};
use native_shell::testing::Harness;

fn program(body: &str) -> String {
    format!(
        r#"
import {{ View, div, h_flex, v_flex, text, Button }} from "native-shell";
import {{
  Alert, InfoAlert, SuccessAlert, WarningAlert, ErrorAlert, Badge, Tag, Skeleton, Empty, EmptyHeader, EmptyMedia,
  EmptyTitle, EmptyDescription, EmptyContent, Kbd, Label, Separator, VerticalSeparator, DashedSeparator,
  VerticalDashedSeparator, ShimmerText, Notification, TitleBar,
}} from "gpui-component";
{body}
"#
    )
}

fn start(body: &str) -> Harness {
    let h = Harness::new(&[("main.js", &program(body))]);
    assert!(h.load_error().is_none(), "{:?} {:?}", h.load_error(), h.logs());
    h
}

fn start_with(env: Environment, body: &str) -> Harness {
    let h = Harness::build(&[("main.js", &program(body))]).environment(env).start();
    assert!(h.load_error().is_none(), "{:?} {:?}", h.load_error(), h.logs());
    h
}

/// Renders `expr` and answers the error it throws.
fn render_error(expr: &str) -> String {
    let h = start(&format!(
        r#"export default class C extends View {{
  render() {{ try {{ {expr}; return text("ok"); }} catch (e) {{ return text(String(e.message)).test_id("err"); }} }}
}}"#
    ));
    h.text_of("err")
}

/// Every node announced with `label`.
fn labeled(h: &Harness, label: &str) -> Vec<u32> {
    let mut nodes: Vec<u32> = h
        .native()
        .nodes
        .iter()
        .filter(|(_, n)| matches!(n.props.get(&Prop::A11yLabel), Some(PropValue::Str(s)) if s == label))
        .map(|(id, _)| *id)
        .collect();
    nodes.sort_unstable();
    nodes
}

fn color(appearance: Appearance, token: &str) -> u32 {
    Theme::default_for(appearance).color(token).unwrap()
}

fn rgb(c: u32) -> u32 {
    c & 0xFF_FFFF
}

fn alpha(c: u32) -> u32 {
    c >> 24
}

fn bg(h: &Harness, id: &str) -> u32 {
    match h.prop(id, Prop::BackgroundColor) {
        Some(PropValue::Color(c)) => c,
        other => panic!("{id} has no background: {other:?}"),
    }
}

/// The text runs of the text node `node`.
fn runs_of(h: &Harness, node: u32) -> Vec<serde_json::Value> {
    match h.native().prop(node, Prop::AttributedText) {
        Some(PropValue::Json(j)) => serde_json::from_str::<serde_json::Value>(j).unwrap()["runs"].as_array().unwrap().clone(),
        other => panic!("node {node} has no text: {other:?}"),
    }
}

#[test]
fn gpui_component_still_exports_title_bar_next_to_the_area_modules() {
    let h = start(r#"export default class C extends View { render() { return new TitleBar().test_id("bar"); } }"#);
    assert!(h.find("bar").is_some());
}

#[test]
fn constructors_work_with_new_and_with_the_new_method() {
    let h = start(
        r#"export default class C extends View {
  render() {
    return v_flex().child(new Tag().test_id("a").child("A")).child(Tag.new().test_id("b").child("B"))
      .child(new InfoAlert("i", "Hi").test_id("c"));
  }
}"#,
    );
    assert_eq!(h.text_of("a"), "A");
    assert_eq!(h.text_of("b"), "B");
    assert_eq!(h.text_of("c"), "Hi");
}

const ALERTS: &str = r#"
export default class C extends View {
  init() { this.shown = true; this.closed = 0; }
  render() {
    return v_flex().w_full().gap_2()
      .child(text(`closed ${this.closed}`).test_id("log"))
      .child(ErrorAlert.new("pay", "Your card was declined.").title("Payment failed").test_id("error")
        .visible(this.shown).on_close((_e, cx) => { this.shown = false; this.closed += 1; cx.notify(); }))
      .child(Alert.new("plain", "Orders ship within two days.").test_id("plain"))
      .child(SuccessAlert.new("ok", "Delivered").test_id("success"))
      .child(InfoAlert.new("info", "Tracking updates hourly").test_id("info"))
      .child(WarningAlert.new("offline", "You're offline. Changes sync when you reconnect.").title("Hidden in a banner").banner().test_id("banner"))
      .child(InfoAlert.new("small", "Small").size("small").test_id("small"));
  }
}
"#;

#[test]
fn alerts_announce_title_and_message_with_the_variant_tint() {
    let h = start(ALERTS);
    assert_eq!(h.prop("error", Prop::A11yRole), Some(PropValue::I32(17)));
    assert_eq!(h.prop("error", Prop::A11yLabel), Some(PropValue::Str("Payment failed. Your card was declined.".into())));
    assert_eq!(h.text_of("error"), "Payment failed Your card was declined.");

    let light = Appearance::Light;
    let error = bg(&h, "error");
    assert_eq!(rgb(error), rgb(color(light, "destructive")), "tinted with the variant token");
    assert!((30..=42).contains(&alpha(error)), "a translucent tint, not a fill: {error:#x}");
    assert_eq!(rgb(bg(&h, "success")), rgb(color(light, "success")));
    assert_eq!(rgb(bg(&h, "info")), rgb(color(light, "info")));
    assert_eq!(bg(&h, "plain"), color(light, "surface"), "the default alert is a bordered card");
    assert_eq!(h.prop("plain", Prop::BorderColor), Some(PropValue::Color(color(light, "border"))));
    assert_eq!(h.prop("error", Prop::CornerRadii), Some(PropValue::F32Array(vec![12.0; 4])));

    // A banner is a full-width strip: square corners and no title.
    assert_eq!(h.prop("banner", Prop::CornerRadii), None);
    assert_eq!(h.text_of("banner"), "You're offline. Changes sync when you reconnect.");
    assert!(h.frame("small").height < h.frame("info").height, "size(\"small\") is tighter");
}

#[test]
fn alert_close_button_calls_on_close_and_visible_false_hides_it() {
    let mut h = start(ALERTS);
    let dismiss = labeled(&h, "Dismiss");
    assert_eq!(dismiss.len(), 1, "only the alert with on_close has a dismiss button");
    assert_eq!(h.native().prop(dismiss[0], Prop::A11yRole), Some(&PropValue::I32(1)));
    h.tap_node(dismiss[0]);
    assert_eq!(h.text_of("log"), "closed 1");
    assert_eq!(h.prop("error", Prop::Hidden), Some(PropValue::Bool(true)));
    assert_eq!(h.prop("error", Prop::A11yHidden), Some(PropValue::Bool(true)));
    assert!(labeled(&h, "Dismiss").is_empty());
}

#[test]
fn alert_rejects_what_gpui_shell_rejects() {
    assert_eq!(render_error(r#"Alert.new("", "x")"#), "Alert id must not be empty");
    assert_eq!(render_error(r#"ErrorAlert.new("a")"#), "ErrorAlert(id, message) expects two strings");
    assert_eq!(
        render_error(r#"Alert.new("a", "b").size("huge")"#),
        r#"Alert.size expects one of xsmall, small, medium, large, got "huge""#
    );
    assert_eq!(render_error(r#"Alert.new("a", "b").visible(1)"#), "Alert.visible(visible) expects a boolean");
}

const BADGES: &str = r#"
export default class C extends View {
  init() { this.n = 3; }
  render() {
    return h_flex().gap_6().p_6()
      .child(Badge.new().count(this.n).test_id("count").child(div().size_10().bg("muted").test_id("avatar")))
      .child(Badge.new().count(120).test_id("capped").child(div().size_10().bg("muted")))
      .child(Badge.new().count(120).max(999).test_id("raised").child(div().size_10().bg("muted")))
      .child(Badge.new().count(0).test_id("zero").child(div().size_10().bg("muted")))
      .child(Badge.new().dot().color("primary").test_id("dot").child(div().size_10().bg("muted")))
      .child(Button.new("clear").test_id("clear").label("Clear").on_click((_e, cx) => { this.n = 0; cx.notify(); }));
  }
}
"#;

#[test]
fn badges_count_cap_and_hide_at_zero() {
    let mut h = start(BADGES);
    assert_eq!(h.text_of("count"), "3");
    assert_eq!(h.prop("count", Prop::A11yValue), Some(PropValue::Str("3".into())));
    assert_eq!(h.text_of("capped"), "99+");
    assert_eq!(h.text_of("raised"), "120");
    assert_eq!(h.text_of("zero"), "");
    assert_eq!(h.native().children(h.find("zero").unwrap()).len(), 1, "no capsule at zero");

    // The capsule sits over the child's top trailing corner, red by default.
    let avatar = h.frame("avatar");
    let capsule = h.native().children(h.find("count").unwrap())[1];
    let f = h.native().nodes[&capsule].frame.unwrap();
    assert!(f.y < 0.0 && f.x + f.width > avatar.width, "overhangs the corner: {f:?}");
    assert_eq!(h.native().prop(capsule, Prop::BackgroundColor), Some(&PropValue::Color(color(Appearance::Light, "destructive"))));

    let dot = h.native().children(h.find("dot").unwrap())[1];
    assert_eq!(h.native().prop(dot, Prop::BackgroundColor), Some(&PropValue::Color(color(Appearance::Light, "primary"))));

    h.tap("clear");
    assert_eq!(h.text_of("count"), "");
    assert_eq!(h.prop("count", Prop::A11yValue), None);
}

#[test]
fn badge_rejects_fractional_negative_and_unknown_colors() {
    assert_eq!(render_error("Badge.new().count(1.5)"), "Badge.count expects a non-negative integer");
    assert_eq!(render_error("Badge.new().max(-1)"), "Badge.max expects a non-negative integer");
    assert!(render_error(r#"Badge.new().color("bright")"#).starts_with("invalid Badge color"));
}

#[test]
fn tags_fill_tint_or_outline_by_variant_in_both_appearances() {
    let body = r#"
export default class C extends View {
  init() { this.removed = 0; }
  render() {
    return v_flex().items_start().gap_2()
      .child(text(`removed ${this.removed}`).test_id("log"))
      .child(Tag.new().variant("primary").test_id("primary").child("New"))
      .child(Tag.new().test_id("secondary").child("Draft"))
      .child(Tag.new().variant("success").test_id("success").child("Paid"))
      .child(Tag.new().variant("warning").outline().rounded_full().test_id("outline").child("Late"))
      .child(Tag.new().variant("info").size("large").test_id("large").child("Large"))
      .child(Tag.new().variant("danger").test_id("filter").child("Refunded")
        .on_close((_e, cx) => { this.removed += 1; cx.notify(); }));
  }
}
"#;
    for appearance in [Appearance::Light, Appearance::Dark] {
        let mut h = start_with(Environment { appearance, ..Environment::default() }, body);
        assert_eq!(bg(&h, "primary"), color(appearance, "primary"));
        assert_eq!(bg(&h, "secondary"), color(appearance, "secondary"));
        let success = bg(&h, "success");
        assert_eq!(rgb(success), rgb(color(appearance, "success")));
        assert!(alpha(success) < 64);

        // Status ink is the token pulled towards the label color: darker on
        // white, lighter on black, so it stays legible on its own tint.
        let label = h.native().children(h.find("success").unwrap())[0];
        let ink = runs_of(&h, label)[0]["color"].as_u64().unwrap() as u32;
        let lum = |c: u32| ((c >> 16) & 255) + ((c >> 8) & 255) + (c & 255);
        let token = color(appearance, "success");
        match appearance {
            Appearance::Light => assert!(lum(ink) < lum(token)),
            Appearance::Dark => assert!(lum(ink) > lum(token)),
        }

        assert_eq!(h.prop("outline", Prop::BackgroundColor), None, "outline keeps only the border");
        assert!(h.prop("outline", Prop::BorderColor).is_some());
        assert!(matches!(h.prop("outline", Prop::CornerRadii), Some(PropValue::F32Array(r)) if r[0] > 8.0));
        assert!(h.frame("large").height > h.frame("success").height);

        let dismiss = labeled(&h, "Dismiss");
        assert_eq!(dismiss.len(), 1);
        h.tap_node(dismiss[0]);
        assert_eq!(h.text_of("log"), "removed 1");
    }
    assert_eq!(render_error(r#"Tag.new().variant("loud")"#), "unsupported Tag variant `loud`");
}

#[test]
fn skeletons_are_still_placeholders_hidden_from_assistive_technology() {
    let h = start(
        r#"export default class C extends View {
  render() {
    return v_flex().w_64().gap_2()
      .child(Skeleton.new().test_id("line"))
      .child(Skeleton.new().secondary().w_24().test_id("short"))
      .child(Skeleton.new().size_10().rounded_full().test_id("avatar"));
  }
}"#,
    );
    assert_eq!(bg(&h, "line"), color(Appearance::Light, "secondary"));
    assert_eq!(h.frame("line").height, 16.0);
    assert_eq!(h.frame("line").width, 256.0, "full width by default");
    assert_eq!(h.frame("short").width, 96.0, "script sizes win");
    assert_eq!(h.prop("short", Prop::Opacity), Some(PropValue::F32(0.5)));
    assert_eq!(h.frame("avatar").width, 40.0);
    assert_eq!(h.prop("line", Prop::A11yHidden), Some(PropValue::Bool(true)));
    assert_eq!(h.prop("line", Prop::Animation), None, "nothing moves");
}

#[test]
fn empty_places_header_parts_in_order_then_content_then_children() {
    let h = start(
        r#"export default class C extends View {
  render() {
    return Empty.new().test_id("empty")
      .child(text("Footnote").test_id("extra"))
      .content(EmptyContent.new().child(Button.new("compose").test_id("compose").label("New Message")))
      .header(EmptyHeader.new()
        .description(EmptyDescription.new().child("Messages you receive appear here."))
        .title(EmptyTitle.new().test_id("title").child("No Messages"))
        .media(EmptyMedia.new().variant("icon").test_id("media").child("✉")));
  }
}"#,
    );
    assert_eq!(h.text_of("empty"), "✉ No Messages Messages you receive appear here. New Message Footnote");
    assert_eq!(h.prop("title", Prop::A11yRole), Some(PropValue::I32(3)), "the title is a heading");
    assert_eq!(h.prop("media", Prop::A11yHidden), Some(PropValue::Bool(true)));
    assert_eq!(bg(&h, "media"), color(Appearance::Light, "muted"), "the icon variant frames the media");
    let (empty, title) = (h.frame("empty"), h.frame("title"));
    assert!((title.x + title.width / 2.0 - empty.width / 2.0).abs() < 1.0, "centered");

    assert_eq!(
        render_error("EmptyHeader.new().child(\"x\")"),
        "EmptyHeader does not accept children; use media, title, and description"
    );
    assert_eq!(render_error("EmptyHeader.new().title(div())"), "title expects one registered Empty part");
    assert_eq!(render_error("Empty.new().header(EmptyContent.new())"), "header expects one registered Empty part");
    assert_eq!(render_error(r#"EmptyMedia.new().variant("frame")"#), "EmptyMedia.variant expects default or icon");
}

#[test]
fn labels_add_secondary_text_mask_and_highlight_matches() {
    let h = start(
        r#"export default class C extends View {
  render() {
    return v_flex()
      .child(Label.new("Groceries").secondary("12 items").test_id("plain"))
      .child(Label.new("Card ending 4242").masked(true).test_id("masked"))
      .child(Label.new("Grocery run").highlights("gro").test_id("hit"));
  }
}"#,
    );
    let plain = runs_of(&h, h.find("plain").unwrap());
    assert_eq!(plain.len(), 2);
    assert_eq!(plain[1]["text"], " 12 items");
    assert_eq!(plain[1]["color"].as_u64(), Some(color(Appearance::Light, "muted_foreground") as u64));
    assert_eq!(h.text_of("masked"), "•".repeat(16));
    assert_eq!(h.prop("masked", Prop::A11yLabel), Some(PropValue::Str("Hidden".into())));
    let hit = runs_of(&h, h.find("hit").unwrap());
    assert_eq!(hit[0]["text"], "Gro");
    assert_eq!(hit[0]["color"].as_u64(), Some(color(Appearance::Light, "primary") as u64));
    assert_eq!(render_error(r#"Label.new("")"#), "Label text must not be empty");
    assert_eq!(render_error(r#"Label.new("a").masked("yes")"#), "Label.masked expects one boolean");
}

#[test]
fn kbd_spells_the_keystroke_per_platform_and_reads_it_in_words() {
    let body = r#"export default class C extends View {
  render() {
    return v_flex().items_start().child(Kbd.new("cmd-shift-k").test_id("k")).child(Kbd.new("secondary-s").outline().test_id("s"))
      .child(Kbd.new("escape").appearance(false).test_id("plain"));
  }
}"#;
    let ios = start_with(Environment { platform: "ios".into(), ..Environment::default() }, body);
    assert_eq!(ios.text_of("k"), "⇧⌘K");
    assert_eq!(ios.text_of("s"), "⌘S");
    assert_eq!(ios.prop("k", Prop::A11yLabel), Some(PropValue::Str("Shift Command K".into())));
    assert!(ios.prop("k", Prop::BackgroundColor).is_some(), "a keycap");
    assert_eq!(ios.prop("s", Prop::BackgroundColor), None, "outline keeps only the border");
    assert_eq!(ios.prop("plain", Prop::BorderColor), None, "appearance(false) is bare text");

    let android = start_with(Environment { platform: "android".into(), ..Environment::default() }, body);
    assert_eq!(android.text_of("k"), "Shift+Win+K");
    assert_eq!(android.text_of("s"), "Ctrl+S");
    assert_eq!(android.text_of("plain"), "Esc");

    assert!(render_error(r#"Kbd.new("hyper-k")"#).starts_with("invalid Kbd keystroke: unknown modifier `hyper`"));
    assert!(render_error(r#"Kbd.new("")"#).starts_with("invalid Kbd keystroke"));
}

#[test]
fn separators_are_hairlines_solid_dashed_vertical_or_labeled() {
    let h = start_with(
        Environment { scale: 2.0, ..Environment::default() },
        r#"export default class C extends View {
  render() {
    return v_flex().w_64()
      .child(Separator.new().test_id("solid"))
      .child(DashedSeparator.new().test_id("dashed"))
      .child(Separator.new().label("or").test_id("labeled"))
      .child(h_flex().h_10().child(VerticalSeparator.new().test_id("v")).child(VerticalDashedSeparator.new().color("primary").test_id("vd")));
  }
}"#,
    );
    assert_eq!(h.frame("solid").height, 0.5, "one device pixel");
    assert_eq!(h.frame("solid").width, 256.0);
    assert_eq!(h.prop("solid", Prop::A11yHidden), Some(PropValue::Bool(true)));
    assert_eq!(h.frame("v").width, 0.5);
    assert_eq!(h.frame("v").height, 40.0);
    assert_eq!(h.frame("vd").height, 40.0);
    let dashed = h.native().children(h.find("dashed").unwrap())[0];
    assert_eq!(h.native().prop(dashed, Prop::ClipsToBounds), Some(&PropValue::Bool(true)), "the dash run is clipped to the line");
    assert_eq!(h.text_of("labeled"), "or");
    assert_eq!(h.prop("labeled", Prop::A11yHidden), None, "a labeled separator is read");
    assert!(render_error(r#"Separator.new().color("loud")"#).starts_with("invalid Separator color"));
}

#[test]
fn shimmer_text_is_still_muted_text_announced_busy() {
    let h = start(
        r#"export default class C extends View {
  render() { return ShimmerText.new("Thinking…").duration_ms(1500).spread(0.3).reverse(true).once(false).test_id("s"); }
}"#,
    );
    assert_eq!(h.text_of("s"), "Thinking…");
    assert_eq!(h.prop("s", Prop::A11yState), Some(PropValue::Json(r#"{"busy":true}"#.into())));
    assert_eq!(h.prop("s", Prop::Animation), None);
    assert_eq!(
        render_error(r#"ShimmerText.new("x").duration_ms(-1)"#),
        "ShimmerText.duration_ms expects a finite non-negative duration"
    );
    assert_eq!(render_error("ShimmerText.new(3)"), "ShimmerText expects text");
}

const NOTIFY: &str = r#"
export default class C extends View {
  init() { this.log = []; }
  note(s, cx) { this.log.push(s); cx.notify(); }
  render() {
    return v_flex()
      .child(text(this.log.join(",")).test_id("log"))
      .child(Notification.new("saved", "Save draft", (m, cx) => this.note(`error:${m}`, cx))
        .title("Draft saved").message("Find it in Drafts.").type("success").test_id("saved")
        .on_click((cx) => this.note("open", cx)).on_close((cx) => this.note("closed", cx)))
      .child(Notification.new("sticky", "Go offline", (m, cx) => this.note(`error:${m}`, cx))
        .message("You're offline").type("warning").autohide(false).test_id("sticky"))
      .child(Notification.new("off", "Disabled", () => {}).disabled(true).test_id("off"));
  }
}
"#;

#[test]
fn notification_button_shows_a_toast_with_its_action_and_reports_close() {
    let mut h = start(NOTIFY);
    assert_eq!(h.prop("saved", Prop::A11yRole), Some(PropValue::I32(1)));
    assert_eq!(h.text_of("saved"), "Save draft");
    h.clear_commands();
    h.tap("saved");
    let (node, cmd, payload) = h.commands()[0].clone();
    assert_eq!((node, cmd), (ROOT_NODE, Command::ShowToast));
    let toast: serde_json::Value = serde_json::from_str(&payload).unwrap();
    assert_eq!(toast["id"], "saved");
    assert_eq!(toast["title"], "Draft saved");
    assert_eq!(toast["description"], "Find it in Drafts.");
    assert_eq!(toast["level"], "success");
    assert_eq!(toast["duration"], 3000);
    assert_eq!(toast["action"]["label"], "View");

    h.toast_action("saved");
    assert_eq!(h.text_of("log"), "open,closed");
    h.advance(5000.0);
    assert_eq!(h.text_of("log"), "open,closed", "closes once");

    // Left alone, it closes when it expires.
    h.tap("saved");
    h.advance(3100.0);
    assert_eq!(h.text_of("log"), "open,closed,closed");

    h.clear_commands();
    h.tap("sticky");
    let toast: serde_json::Value = serde_json::from_str(&h.commands()[0].2).unwrap();
    assert_eq!(toast["title"], "You're offline", "a message alone is the title");
    assert_eq!(toast["duration"], 0, "autohide(false) stays until dismissed");
    assert!(toast.get("action").is_none());

    h.clear_commands();
    h.tap("off");
    assert!(h.commands().is_empty(), "a disabled notification button does nothing");
}

#[test]
fn notification_rejects_bad_arguments_like_gpui_shell() {
    assert_eq!(
        render_error(r#"Notification.new("n", "Go", () => {}).type("fatal")"#),
        "type expects info, success, warning, or error"
    );
    assert_eq!(render_error(r#"Notification.new("n", "Go", () => {}).autohide(1)"#), "autohide(value) expects boolean");
    assert_eq!(render_error(r#"Notification.new("", "Go", () => {})"#), "Notification id must not be empty");
}
