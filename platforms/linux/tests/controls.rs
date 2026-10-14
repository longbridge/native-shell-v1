//! Native controls end to end: what the user does reaches the script as the
//! `Change` payloads of each control, props render, and removed props reset.

mod support;

use std::process::ExitCode;

use gtk::glib::translate::IntoGlib;
use gtk::{gdk, glib};
use support::*;

fn main() -> ExitCode {
    run(&[
        ("switch toggles by tap, drag and key", switch_toggles),
        ("checkbox and radio report checked", checkbox_and_radio),
        ("disabled controls ignore input", disabled_controls_ignore_input),
        ("slider drags, steps and resets", slider_drags_steps_and_resets),
        ("segmented control selects by tap and arrows", segmented_selects),
        ("select chooses from its menu", select_chooses),
        ("date picker picks date and time", date_picker_picks),
        ("progress and spinner follow the theme", progress_and_spinner_draw),
        ("controls screenshots", controls_screenshots),
        ("settings example: switches, time and theme", settings_example),
        ("components example: control pages", components_pages),
    ])
}

const PROGRAM: &str = r##"
import { View, div, h_flex, v_flex, Switch, Checkbox, Radio, Slider, Tabs, Select, DatePicker, Progress, Spinner } from "native-shell";
export default class App extends View {
  init() {
    Object.assign(this, { sw: false, cb: false, radio: false, vol: 0.5, seg: 1, fruit: "b", date: "2026-10-14", time: "09:30", custom: true, off: false, last: "" });
  }
  render(cx) {
    const t = cx.theme();
    const log = (k) => (v, cx) => { this[k] = v; this.last = `${k}=${v}`; cx.notify(); };
    return v_flex().size_full().gap_2().p_4().bg(t.background)
      .child(div().test_id(`last:${this.last}`).h(4))
      .child(Switch.new("sw").test_id("sw").checked(this.sw).disabled(this.off).on_change(log("sw")))
      .child(Checkbox.new("cb").test_id("cb").checked(this.cb).disabled(this.off).on_change(log("cb")))
      .child(Radio.new("radio").test_id("radio").checked(this.radio).on_change(log("radio")))
      .child(Slider.new("vol").test_id("vol").w(200).value(this.vol)
        .when(this.custom, (el) => el.min(10).max(20).step(5)).disabled(this.off).on_change(log("vol")))
      .child(Tabs.new("seg").test_id("seg").w(240).items(["Day", "Week", "Month"]).selected(this.seg).on_change(log("seg")))
      .child(h_flex().child(Select.new("fruit").test_id("fruit").items([
        { label: "Apple", value: "a" }, { label: "Banana", value: "b" }, { label: "Cherry", value: "c" },
      ]).value(this.fruit).on_change(log("fruit"))))
      .child(DatePicker.new("date").test_id("date").value(this.date).on_change(log("date")))
      .child(DatePicker.new("time").test_id("time").mode("time").value(this.time).on_change(log("time")))
      .child(Progress.new("bar").test_id("bar").value(50))
      .child(Progress.new("busy").test_id("busy").indeterminate())
      .child(Spinner.new().test_id("spin"))
      .child(div().test_id("toggle-custom").h(10).w(10).on_click((_e, cx) => { this.custom = !this.custom; cx.notify(); }))
      .child(div().test_id("brand").h(10).w(10).on_click((_e, cx) => cx.set_theme({ tokens: { primary: "#ff0000" } })))
      .child(div().test_id("toggle-off").h(10).w(10).on_click((_e, cx) => { this.off = !this.off; cx.notify(); }));
  }
}"##;

fn fixture() -> Fixture {
    // Animations would wait for a frame clock Broadway never ticks.
    gtk::Settings::default().expect("settings").set_gtk_enable_animations(false);
    Fixture::new(PROGRAM)
}

/// The drawn control widgets inside the node `test_id`, outermost first.
fn controls(f: &Fixture, test_id: &str) -> Vec<gtk::Widget> {
    fn walk(w: &gtk::Widget, out: &mut Vec<gtk::Widget>) {
        if w.type_().name() == "NshControl" {
            out.push(w.clone());
        }
        let mut child = w.first_child();
        while let Some(c) = child {
            walk(&c, out);
            child = c.next_sibling();
        }
    }
    let mut out = Vec::new();
    walk(&f.expect(test_id), &mut out);
    assert!(!out.is_empty(), "{test_id:?} has no drawn control");
    out
}

fn control(f: &Fixture, test_id: &str) -> gtk::Widget {
    controls(f, test_id).remove(0)
}

fn gesture<T: IsA<gtk::EventController> + IsA<glib::Object>>(w: &gtk::Widget) -> T {
    let list = w.observe_controllers();
    (0..list.n_items()).find_map(|i| list.item(i).and_downcast::<T>()).unwrap_or_else(|| panic!("no {} on {w:?}", T::static_type()))
}

fn tap(f: &Fixture, w: &gtk::Widget) {
    let (x, y) = (w.width() as f64 / 2.0, w.height() as f64 / 2.0);
    let click: gtk::GestureClick = gesture(w);
    click.emit_by_name::<()>("pressed", &[&1i32, &x, &y]);
    click.emit_by_name::<()>("released", &[&1i32, &x, &y]);
    f.settle();
}

/// Offers a key press to the widget's key controllers until one handles it.
fn key(f: &Fixture, w: &gtk::Widget, key: gdk::Key) {
    let list = w.observe_controllers();
    for i in 0..list.n_items() {
        let Some(keys) = list.item(i).and_downcast::<gtk::EventControllerKey>() else { continue };
        if keys.emit_by_name::<bool>("key-pressed", &[&key.into_glib(), &0u32, &gdk::ModifierType::empty()]) {
            break;
        }
    }
    f.settle();
}

fn drag(f: &Fixture, w: &gtk::Widget, x: f64, dx: f64) {
    let y = w.height() as f64 / 2.0;
    let drag: gtk::GestureDrag = gesture(w);
    drag.emit_by_name::<()>("drag-begin", &[&x, &y]);
    drag.emit_by_name::<()>("drag-update", &[&(dx / 2.0), &0.0f64]);
    drag.emit_by_name::<()>("drag-update", &[&dx, &0.0f64]);
    drag.emit_by_name::<()>("drag-end", &[&dx, &0.0f64]);
    f.settle();
}

fn switch_toggles() {
    let f = fixture();
    let sw = control(&f, "sw");
    assert_eq!(sw.accessible_role(), gtk::AccessibleRole::Switch);
    let b = f.bounds(&f.expect("sw"));
    assert_eq!((b.width(), b.height()), (51.0, 31.0));
    drag(&f, &sw, 10.0, 0.0);
    f.expect("last:sw=true");
    // Dragging the knob to the off side turns it off.
    drag(&f, &sw, 40.0, -30.0);
    f.expect("last:sw=false");
    sw.grab_focus();
    key(&f, &sw, gdk::Key::space);
    f.expect("last:sw=true");
}

fn checkbox_and_radio() {
    let f = fixture();
    let cb = control(&f, "cb");
    assert_eq!(cb.accessible_role(), gtk::AccessibleRole::Checkbox);
    tap(&f, &cb);
    f.expect("last:cb=true");
    key(&f, &cb, gdk::Key::Return);
    f.expect("last:cb=false");
    let radio = control(&f, "radio");
    assert_eq!(radio.accessible_role(), gtk::AccessibleRole::Radio);
    tap(&f, &radio);
    f.expect("last:radio=true");
    // A radio never unchecks itself.
    tap(&f, &radio);
    f.expect("last:radio=true");
}

fn disabled_controls_ignore_input() {
    let f = fixture();
    f.tap("toggle-off");
    let (sw, cb, vol) = (control(&f, "sw"), control(&f, "cb"), control(&f, "vol"));
    assert!(!sw.is_sensitive() && !cb.is_sensitive() && !vol.is_sensitive());
    tap(&f, &cb);
    drag(&f, &sw, 10.0, 0.0);
    key(&f, &vol, gdk::Key::End);
    f.expect("last:");
    // Enabled again: the reset to enabled takes input.
    f.tap("toggle-off");
    tap(&f, &control(&f, "cb"));
    f.expect("last:cb=true");
}

fn slider_drags_steps_and_resets() {
    let f = fixture();
    let vol = control(&f, "vol");
    assert_eq!(vol.accessible_role(), gtk::AccessibleRole::Slider);
    // 10…20 in steps of 5: the far right end is 20.
    drag(&f, &vol, 14.0, 300.0);
    f.expect("last:vol=20");
    key(&f, &vol, gdk::Key::Left);
    f.expect("last:vol=15");
    key(&f, &vol, gdk::Key::Home);
    f.expect("last:vol=10");
    // Without min/max/step the slider is back to 0…1, continuous.
    f.tap("toggle-custom");
    key(&f, &vol, gdk::Key::End);
    f.expect("last:vol=1");
    let vol = control(&f, "vol");
    drag(&f, &vol, 14.0, 86.0);
    f.expect("last:vol=0.5");
}

fn segmented_selects() {
    let f = fixture();
    let segs = controls(&f, "seg");
    assert_eq!(segs.len(), 4, "the group and three segments");
    assert_eq!(segs[0].accessible_role(), gtk::AccessibleRole::TabList);
    assert_eq!(segs[1].accessible_role(), gtk::AccessibleRole::Tab);
    tap(&f, &segs[3]);
    f.expect("last:seg=2");
    key(&f, &segs[3], gdk::Key::Left);
    f.expect("last:seg=1");
    // The selected segment again sends nothing.
    tap(&f, &controls(&f, "seg")[1]);
    f.expect("last:seg=0");
    tap(&f, &controls(&f, "seg")[1]);
    f.expect("last:seg=0");
}

fn select_chooses() {
    let f = fixture();
    let fruit = control(&f, "fruit");
    assert_eq!(fruit.accessible_role(), gtk::AccessibleRole::ComboBox);
    // Sized by the host to its label (`nsh/select`).
    let b = f.bounds(&f.expect("fruit"));
    assert!(b.width() > 40.0 && b.width() < 200.0 && b.height() >= 44.0, "{b:?}");
    tap(&f, &fruit);
    let popover = fruit.first_child().and_downcast::<gtk::Popover>().expect("menu popover");
    assert!(popover.is_visible());
    let list = find(popover.upcast_ref(), |w| w.is::<gtk::ListBox>()).and_downcast::<gtk::ListBox>().expect("menu list");
    let row = list.row_at_index(2).expect("third item");
    list.emit_by_name::<()>("row-activated", &[&row]);
    f.settle();
    f.expect("last:fruit=c");
    assert!(!popover.is_visible());
}

fn date_picker_picks() {
    let f = fixture();
    let pill = controls(&f, "date").remove(1);
    tap(&f, &pill);
    let popover = pill.first_child().and_downcast::<gtk::Popover>().expect("date popover");
    let cal = popover.child().and_downcast::<gtk::Calendar>().expect("calendar");
    assert_eq!((cal.year(), cal.month(), cal.day()), (2026, 9, 14));
    cal.select_day(&glib::DateTime::from_local(2026, 11, 3, 0, 0, 0.0).unwrap());
    f.settle();
    f.expect("last:date=2026-11-03");

    let pill = controls(&f, "time").remove(1);
    tap(&f, &pill);
    let popover = pill.first_child().and_downcast::<gtk::Popover>().expect("time popover");
    let spins: Vec<gtk::SpinButton> = descendants(popover.upcast_ref()).into_iter().filter_map(|w| w.downcast().ok()).collect();
    assert_eq!(spins.len(), 2);
    assert_eq!((spins[0].value(), spins[1].value()), (9.0, 30.0));
    spins[0].set_value(18.0);
    f.settle();
    f.expect("last:time=18:30");
}

fn progress_and_spinner_draw() {
    let f = fixture();
    let bar = f.bounds(&f.expect("bar"));
    let y = (bar.y() + bar.height() / 2.0) as i32;
    // Half filled with the theme's primary, the rest is the track.
    assert_eq!(f.pixel((bar.x() + 20.0) as i32, y), 0xff171717);
    assert_ne!(f.pixel((bar.x() + bar.width() - 20.0) as i32, y), 0xff171717);
    // A new primary redraws the chrome drawn from it.
    f.tap("brand");
    assert_eq!(f.pixel((bar.x() + 20.0) as i32, y), 0xffff0000);
    assert_eq!(control(&f, "busy").accessible_role(), gtk::AccessibleRole::ProgressBar);
    let spin = f.bounds(&f.expect("spin"));
    assert_eq!((spin.width(), spin.height()), (24.0, 24.0));
}

fn controls_screenshots() {
    let f = fixture();
    f.screenshot("screenshots/controls-light.png");
    f.set_dark(true);
    f.screenshot("screenshots/controls-dark.png");
}

/// Light and dark screenshots of what `f` shows, into `screenshots/`.
fn screenshots(f: &Fixture, name: &str) {
    f.set_dark(false);
    f.screenshot(format!("screenshots/controls-{name}-light.png"));
    f.set_dark(true);
    f.screenshot(format!("screenshots/controls-{name}-dark.png"));
    f.set_dark(false);
}

fn settings_example() {
    gtk::Settings::default().expect("settings").set_gtk_enable_animations(false);
    let f = Fixture::open_dir("../../examples/settings");
    screenshots(&f, "settings");
    // Notifications are on by default, so is the daily summary and its time.
    f.expect("summary-time");
    let notifications = control(&f, "notifications");
    key(&f, &notifications, gdk::Key::space);
    assert!(f.find("summary-time").is_none());
    assert!(!control(&f, "summary").is_sensitive());
    key(&f, &control(&f, "notifications"), gdk::Key::space);
    assert!(control(&f, "summary").is_sensitive());

    // The theme menu switches the program to dark.
    let theme = control(&f, "theme");
    tap(&f, &theme);
    let popover = theme.first_child().and_downcast::<gtk::Popover>().expect("theme menu");
    let list = find(popover.upcast_ref(), |w| w.is::<gtk::ListBox>()).and_downcast::<gtk::ListBox>().expect("menu list");
    list.emit_by_name::<()>("row-activated", &[&list.row_at_index(2).expect("Dark")]);
    f.settle();
    assert_eq!(f.pixel(4, 200), 0xff0a0a0a, "the dark background");
}

/// A components example page as the program's root view.
fn components_page(page: &str) -> Fixture {
    gtk::Settings::default().expect("settings").set_gtk_enable_animations(false);
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/components");
    let read = |p: &str| std::fs::read_to_string(dir.join(p)).unwrap_or_else(|e| panic!("{p}: {e}"));
    let mut files: Vec<(String, String)> = vec![("main.js".into(), format!("export {{ default }} from \"./pages/{page}.js\";"))];
    files.push(("ui.js".into(), read("ui.js")));
    for entry in std::fs::read_dir(dir.join("pages")).expect("pages").flatten() {
        let name = format!("pages/{}", entry.file_name().to_string_lossy());
        let text = read(&name);
        files.push((name, text));
    }
    let files: Vec<(&str, &str)> = files.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
    Fixture::with_files(&read("native-shell.json"), &files)
}

fn components_pages() {
    let f = components_page("selection");
    assert_eq!(control(&f, "switch-orders").accessible_role(), gtk::AccessibleRole::Switch);
    key(&f, &control(&f, "switch-prices"), gdk::Key::space);
    screenshots(&f, "components-selection");
    drop(f);

    let f = components_page("progress");
    assert_eq!(control(&f, "volume").accessible_role(), gtk::AccessibleRole::Slider);
    key(&f, &control(&f, "volume"), gdk::Key::End);
    f.tap("upload-start");
    f.expect("upload-spinner");
    screenshots(&f, "components-progress");
    pump_for(std::time::Duration::from_millis(1300));
    f.wait_for("upload-progress");
    screenshots(&f, "components-progress-uploading");
    drop(f);

    let f = components_page("pickers");
    let sitting = controls(&f, "sitting");
    tap(&f, &sitting[1]);
    let guests = control(&f, "guests");
    tap(&f, &guests);
    let popover = guests.first_child().and_downcast::<gtk::Popover>().expect("guests menu");
    let list = find(popover.upcast_ref(), |w| w.is::<gtk::ListBox>()).and_downcast::<gtk::ListBox>().expect("menu list");
    list.emit_by_name::<()>("row-activated", &[&list.row_at_index(3).expect("4 guests")]);
    f.settle();
    screenshots(&f, "components-pickers");
}

fn descendants(w: &gtk::Widget) -> Vec<gtk::Widget> {
    let mut out = Vec::new();
    let mut child = w.first_child();
    while let Some(c) = child {
        out.extend(descendants(&c));
        out.push(c.clone());
        child = c.next_sibling();
    }
    out
}

fn find(w: &gtk::Widget, matches: impl Fn(&gtk::Widget) -> bool) -> Option<gtk::Widget> {
    descendants(w).into_iter().find(|c| matches(c))
}
