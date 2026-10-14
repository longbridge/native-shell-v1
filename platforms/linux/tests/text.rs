//! Text end to end: frames from Pango measurement hold the drawn text, links,
//! selectable text, the user's text size, and screenshots of a text-heavy screen.

mod support;

use std::process::ExitCode;

use support::*;

fn main() -> ExitCode {
    run(&[
        ("drawn text fits its measured frames", drawn_text_fits_its_measured_frames),
        ("a link tap reaches the script", a_link_tap_reaches_the_script),
        ("selectable text is a label wrapped like the measurement", selectable_text_is_a_label_wrapped_like_the_measurement),
        ("the selection color follows the theme alone", the_selection_color_follows_the_theme_alone),
        ("text follows the desktop text size", text_follows_the_desktop_text_size),
        ("text screenshots", text_screenshots),
    ])
}

/// An order update, as a shopping app would show it: a title, wrapped body
/// text mixing scripts and emoji, a clamped preview and a centered footnote.
const ORDER: &str = r#"
import { View, div, v_flex } from "native-shell";
export default class App extends View {
  render(cx) {
    const t = cx.theme();
    return v_flex().size_full().bg(t.background).p_4().gap_3()
      .child(div().test_id("title").text_3xl().font_bold().text_color(t.foreground).child("Order shipped"))
      .child(div().test_id("body").text_base().line_height(1.375).text_color(t.foreground)
        .child("Your package left the warehouse this morning and arrives Tuesday. 包裹已从仓库发出，预计周二送达。 🚚📦 Tracking updates appear here as soon as the carrier scans it."))
      .child(div().test_id("preview").text_sm().text_color(t.muted_foreground).line_clamp(2)
        .child("Note from the seller: thank you for ordering again! We packed the ceramic mugs in recycled paper so they arrive in one piece, and added a few samples of the new winter blend."))
      .child(div().test_id("footnote").text_xs().text_center().text_color(t.muted_foreground)
        .child("Delivery times are estimates and can change during holidays."));
  }
}"#;

fn text_nodes(root: &gtk::Widget, out: &mut Vec<gtk::Widget>) {
    if root.accessible_role() == gtk::AccessibleRole::Label && root.is::<native_shell_gtk::NodeView>() {
        out.push(root.clone());
    }
    let mut child = root.first_child();
    while let Some(w) = child {
        text_nodes(&w, out);
        child = w.next_sibling();
    }
}

/// Rows of `widget`'s rendering with visible ink: (count, first, last).
fn ink_rows(widget: &gtk::Widget) -> (usize, i32, i32) {
    let texture = render(widget);
    let mut downloader = gtk::gdk::TextureDownloader::new(&texture);
    downloader.set_format(gtk::gdk::MemoryFormat::R8g8b8a8);
    let (bytes, stride) = downloader.download_bytes();
    let (w, h) = (texture.width() as usize, texture.height() as usize);
    let rows: Vec<i32> = (0..h).filter(|y| (0..w).any(|x| bytes[y * stride + x * 4 + 3] > 40)).map(|y| y as i32).collect();
    (rows.len(), rows.first().copied().unwrap_or(-1), rows.last().copied().unwrap_or(-1))
}

fn drawn_text_fits_its_measured_frames() {
    let f = Fixture::new(ORDER);
    let mut texts = Vec::new();
    text_nodes(f.surface.upcast_ref(), &mut texts);
    assert!(texts.len() >= 4, "found {} text nodes", texts.len());
    for text in &texts {
        let (rows, top, bottom) = ink_rows(text);
        let h = text.height();
        assert!(rows > 0, "{:?} drew nothing", text.widget_name());
        assert!(top >= 0 && bottom <= h, "{:?}: ink {top}..{bottom} outside 0..{h}", text.widget_name());
    }
    // The body wraps over several lines at 22 pt (16 × 1.375) each.
    let body = f.bounds(&f.expect("body"));
    assert!(body.height() >= 66.0 && (body.height() / 22.0).fract() < 0.05, "{body:?}");
    // The clamp shows two lines (14 pt at GPUI's default 1.618 line height) and clips the rest.
    let preview = f.bounds(&f.expect("preview"));
    assert!(preview.height() <= (14.0f32 * 1.618 * 2.0).ceil(), "{preview:?}");
}

fn a_link_tap_reaches_the_script() {
    let f = Fixture::new(
        r#"
import { View, div, v_flex } from "native-shell";
import { TextView } from "gpui-base";
export default class App extends View {
  init() { this.opened = "none"; }
  render(cx) {
    return v_flex().size_full().p_4()
      .child(TextView.markdown("notes", "[Read the release notes](https://example.org/notes)")
        .on_link_click((url, cx) => { this.opened = url; cx.notify(); }))
      .child(div().test_id(`opened:${this.opened}`).h(1));
  }
}"#,
    );
    f.expect("opened:none");
    let link = f
        .find_where(|w| w.accessible_role() == gtk::AccessibleRole::Label && w.is::<native_shell_gtk::NodeView>())
        .expect("the link text");
    // On the glyphs: the text node spans the column, the words its start.
    click_at(&link, 20.0, f64::from(link.height()) / 2.0);
    f.settle();
    f.expect("opened:https://example.org/notes");
}

/// Presses and releases the click gestures on `widget` at a point.
fn click_at(widget: &gtk::Widget, x: f64, y: f64) {
    let controllers = widget.observe_controllers();
    let gestures: Vec<gtk::GestureClick> = (0..controllers.n_items()).filter_map(|i| controllers.item(i).and_downcast()).collect();
    assert!(!gestures.is_empty(), "no click gesture on {widget:?}");
    for g in &gestures {
        g.emit_by_name::<()>("pressed", &[&1i32, &x, &y]);
    }
    for g in &gestures {
        g.emit_by_name::<()>("released", &[&1i32, &x, &y]);
    }
}

fn selectable_text_is_a_label_wrapped_like_the_measurement() {
    let f = Fixture::new(
        r#"
import { View, v_flex } from "native-shell";
import { TextView } from "gpui-base";
export default class App extends View {
  render(cx) {
    return v_flex().size_full().p_4()
      .child(TextView.markdown("terms", "Refunds are issued to the original payment method within 5–7 business days after we receive the returned item. 退款将在收到退货后五到七个工作日内退回原支付方式。").selectable(true));
  }
}"#,
    );
    let label = f.find_where(|w| w.is::<gtk::Label>()).and_downcast::<gtk::Label>().expect("a selectable label");
    assert!(label.is_selectable());
    let node = label.parent().expect("the text node");
    let (_, logical) = label.layout().pixel_extents();
    assert!(label.layout().line_count() >= 3, "wraps");
    assert!((logical.height() - node.height()).abs() <= 1, "label {} vs frame {}", logical.height(), node.height());

    // Selection and Copy are the label's own (drag, double click, Ctrl+C, context menu).
    assert!(label.is_focusable(), "keyboard selection needs focus");
    // The theme's selection wash shows behind selected text.
    let (plain, _) = tinted_pixels(label.upcast_ref());
    label.select_region(0, -1);
    let (selected, wash) = tinted_pixels(label.upcast_ref());
    assert!(selected > plain + 1000, "selection drew {selected} tinted pixels vs {plain}");
    // The light theme's token, #007aff33 (± premultiplication rounding), not GTK's own selection color.
    let expected = [0u8, 122, 255, 51];
    assert!(wash.iter().zip(expected).all(|(a, b)| a.abs_diff(b) <= 3), "selection wash {wash:?}");
    label.select_region(0, 7);
    assert_eq!(label.selection_bounds(), Some((0, 7)));
    label.emit_by_name::<()>("copy-clipboard", &[]);
    let clipboard = label.clipboard();
    let copied = gtk::glib::MainContext::default().block_on(clipboard.read_text_future());
    assert_eq!(copied.ok().flatten().as_deref(), Some("Refunds"));
}

/// Tinted pixels of a rendering (the selection wash) and the most common of them.
fn tinted_pixels(widget: &gtk::Widget) -> (usize, [u8; 4]) {
    let texture = render(widget);
    let mut downloader = gtk::gdk::TextureDownloader::new(&texture);
    downloader.set_format(gtk::gdk::MemoryFormat::R8g8b8a8);
    let (bytes, _) = downloader.download_bytes();
    let mut counts = std::collections::HashMap::<[u8; 4], usize>::new();
    // Saturated pixels: text is neutral, the wash is a tint.
    let saturated = |p: &[u8]| p[..3].iter().max().unwrap_or(&0) - p[..3].iter().min().unwrap_or(&0) > 40;
    for p in bytes.chunks(4).filter(|p| p[3] > 20 && saturated(p)) {
        *counts.entry([p[0], p[1], p[2], p[3]]).or_default() += 1;
    }
    let common = counts.iter().max_by_key(|(_, n)| **n).map_or([0; 4], |(c, _)| *c);
    (counts.values().sum(), common)
}

fn the_selection_color_follows_the_theme_alone() {
    // Changing only `selection` re-sends no prop of the text node.
    let f = Fixture::new(
        r##"
import { View, div, v_flex, set_theme } from "native-shell";
import { TextView } from "gpui-base";
export default class App extends View {
  render(cx) {
    return v_flex().size_full().p_4()
      .child(div().test_id("tint").h(44).on_click(() => set_theme({ tokens: { selection: "#ff000040" } })))
      .child(TextView.markdown("quote", "Ships within two business days.").selectable(true));
  }
}"##,
    );
    let label = f.find_where(|w| w.is::<gtk::Label>()).and_downcast::<gtk::Label>().expect("a selectable label");
    label.select_region(0, -1);
    let (_, before) = tinted_pixels(label.upcast_ref());
    assert!(before[2] > 200 && before[0] < 30, "the default blue wash: {before:?}");
    f.tap("tint");
    let (_, after) = tinted_pixels(label.upcast_ref());
    let expected = [255u8, 0, 0, 64];
    assert!(after.iter().zip(expected).all(|(a, b)| a.abs_diff(b) <= 3), "the new wash: {after:?}");
}

fn text_follows_the_desktop_text_size() {
    let f = Fixture::new(ORDER);
    let before = f.bounds(&f.expect("title"));
    let settings = gtk::Settings::default().expect("settings");
    let dpi = settings.gtk_xft_dpi();
    settings.set_gtk_xft_dpi((96.0 * 1024.0 * 1.3) as i32);
    f.settle();
    let after = f.bounds(&f.expect("title"));
    settings.set_gtk_xft_dpi(dpi);
    f.settle();
    let ratio = after.height() / before.height();
    assert!((1.2..1.4).contains(&ratio), "title {before:?} → {after:?}");
}

fn text_screenshots() {
    let f = Fixture::new(ORDER);
    f.screenshot("screenshots/text-light.png");
    f.set_dark(true);
    f.screenshot("screenshots/text-dark.png");
}
