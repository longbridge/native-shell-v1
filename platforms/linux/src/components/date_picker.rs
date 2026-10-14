//! `DatePicker`, as the iOS compact picker: the localized value in a pill (a
//! date pill and a time pill for date and time), drawn, trailing-aligned. A
//! pill opens a GTK popover: a `GtkCalendar` for the date, hour and minute spin buttons for
//! the time. Values are `YYYY-MM-DD`, `HH:mm` and `YYYY-MM-DDTHH:mm` by
//! `DateMode`, both in `Date` and in `{date}` change payloads.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::OnceLock;

use gtk::accessible::{Property, State as A11y};
use gtk::prelude::*;
use gtk::{gio, glib};

use native_shell_core::protocol::{Prop, PropValue};

use super::control::{self, bool_or, int_or, Canvas, Link};
use super::toggle::shared_prop;
use crate::component::{Component, NodeCx};

pub const PADDING: f32 = 12.0;
pub const GAP: f32 = 8.0;
pub const FONT_SIZE: f32 = 17.0;
const RADIUS: f32 = 6.0;

/// The picked value, in local wall-clock fields.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Value {
    pub year: i32,
    pub month: i32,
    pub day: i32,
    pub hour: i32,
    pub minute: i32,
}

impl Value {
    pub fn now() -> Value {
        let now = glib::DateTime::now_local().ok();
        let f = |g: fn(&glib::DateTime) -> i32, d: i32| now.as_ref().map_or(d, g);
        Value {
            year: f(|t| t.year(), 2000),
            month: f(|t| t.month(), 1),
            day: f(|t| t.day_of_month(), 1),
            hour: f(|t| t.hour(), 0),
            minute: f(|t| t.minute(), 0),
        }
    }

    /// Parses any of the three wire forms (so a mode change never drops the
    /// value); fields a form lacks come from `base`.
    pub fn parse(s: &str, base: Value) -> Option<Value> {
        let num = |t: &str| t.parse::<i32>().ok();
        let time = |t: &str| -> Option<(i32, i32)> {
            let (h, m) = t.split_once(':')?;
            let (h, m) = (num(h)?, num(m.get(..2)?)?);
            ((0..24).contains(&h) && (0..60).contains(&m)).then_some((h, m))
        };
        let date = |d: &str| -> Option<(i32, i32, i32)> {
            let mut parts = d.splitn(3, '-');
            let (y, m, d) = (num(parts.next()?)?, num(parts.next()?)?, num(parts.next()?)?);
            glib::DateTime::from_local(y, m, d, 0, 0, 0.0).ok().map(|_| (y, m, d))
        };
        let mut v = base;
        if let Some((d, t)) = s.split_once(['T', ' ']) {
            (v.year, v.month, v.day) = date(d)?;
            (v.hour, v.minute) = time(t)?;
        } else if s.contains('-') {
            (v.year, v.month, v.day) = date(s)?;
        } else {
            (v.hour, v.minute) = time(s)?;
        }
        Some(v)
    }

    /// The wire form for `mode`: 0 date, 1 time, 2 date and time.
    pub fn format(&self, mode: i32) -> String {
        let date = format!("{:04}-{:02}-{:02}", self.year, self.month, self.day);
        let time = format!("{:02}:{:02}", self.hour, self.minute);
        match mode {
            1 => time,
            2 => format!("{date}T{time}"),
            _ => date,
        }
    }

    fn date_time(&self) -> Option<glib::DateTime> {
        glib::DateTime::from_local(self.year, self.month, self.day, self.hour, self.minute, 0.0).ok()
    }

    /// The localized text of each pill: medium date, short time.
    pub fn display(&self, mode: i32) -> Vec<String> {
        let Some(t) = self.date_time() else { return vec![self.format(mode)] };
        let f = |fmt: &str| t.format(fmt).map(|s| s.to_string()).unwrap_or_default();
        let date = f(date_format());
        let time = f(if clock_24h() { "%H:%M" } else { "%-l:%M %p" });
        match mode {
            1 => vec![time],
            2 => vec![date, time],
            _ => vec![date],
        }
    }
}

/// glib has no medium date style: write the English ones out (`Oct 14, 2026`,
/// `14 Oct 2026`) and use the locale's preferred form elsewhere.
fn date_format() -> &'static str {
    static FORMAT: OnceLock<&'static str> = OnceLock::new();
    FORMAT.get_or_init(|| {
        let lang = glib::language_names().first().map(|s| s.to_string()).unwrap_or_default();
        if lang.starts_with("en_US") || lang == "C" || lang == "POSIX" || lang == "en" {
            "%b %-d, %Y"
        } else if lang.starts_with("en") {
            "%-d %b %Y"
        } else {
            "%x"
        }
    })
}

/// The desktop's 12/24-hour setting (GNOME's, where installed), else 24-hour.
fn clock_24h() -> bool {
    static H24: OnceLock<bool> = OnceLock::new();
    *H24.get_or_init(|| {
        let schema = "org.gnome.desktop.interface";
        let has_key = gio::SettingsSchemaSource::default()
            .and_then(|src| src.lookup(schema, true))
            .is_some_and(|s| s.has_key("clock-format"));
        !has_key || gio::Settings::new(schema).string("clock-format") != "12h"
    })
}

struct State {
    value: Cell<Value>,
    mode: Cell<i32>,
    /// `Date` as the core sent it, re-read when the mode changes.
    raw: RefCell<Option<String>>,
    /// Set while the popover widgets are updated from code, not the user.
    muted: Cell<bool>,
}

impl State {
    fn apply_raw(&self) {
        let base = Value::now();
        let v = self.raw.borrow().as_deref().and_then(|s| Value::parse(s, base)).unwrap_or(base);
        self.value.set(v);
    }
}

pub struct DatePicker {
    link: Link,
    canvas: Canvas,
    state: Rc<State>,
}

impl DatePicker {
    pub fn new(cx: &NodeCx) -> DatePicker {
        let link = Link::new(cx);
        let canvas = Canvas::new(gtk::AccessibleRole::Group, false);
        let state = Rc::new(State { value: Cell::new(Value::now()), mode: Cell::new(0), raw: RefCell::default(), muted: Cell::new(false) });
        let s = state.clone();
        canvas.set_layout(move |c, w, h| {
            // Pills sit side by side against the trailing edge, where a row's
            // other trailing controls end.
            let texts = s.value.get().display(s.mode.get());
            let ph = pill_height().min(h as f32).round();
            let widths: Vec<f32> = texts
                .iter()
                .map(|t| (control::text_size(&control::line(t, FONT_SIZE, 400, 0, None)).0 + 2.0 * PADDING).ceil())
                .collect();
            let total = widths.iter().sum::<f32>() + GAP * widths.len().saturating_sub(1) as f32;
            let rtl = c.direction() == gtk::TextDirection::Rtl;
            let mut x = if rtl { 0.0 } else { (w as f32 - total).max(0.0) };
            let y = ((h as f32 - ph) / 2.0).round();
            let order: Vec<(Canvas, f32)> = pills(c).into_iter().zip(widths).collect();
            let order: Vec<_> = if rtl { order.into_iter().rev().collect() } else { order };
            for (pill, pw) in order {
                pill.allocate(pw as i32, ph as i32, -1, Some(gtk::gsk::Transform::new().translate(&gtk::graphene::Point::new(x, y))));
                x += pw + GAP;
            }
        });
        cx.view.set_content(Some(canvas.upcast_ref()));
        let picker = DatePicker { link, canvas, state };
        picker.rebuild();
        picker
    }

    /// One pill per part the mode shows.
    fn rebuild(&self) {
        for pill in pills(&self.canvas) {
            pill.unparent();
        }
        let parts: &[Part] = match self.state.mode.get() {
            1 => &[Part::Time],
            2 => &[Part::Date, Part::Time],
            _ => &[Part::Date],
        };
        for (i, part) in parts.iter().enumerate() {
            let pill = Canvas::new(gtk::AccessibleRole::Button, true);
            pill.update_property(&[Property::HasPopup(true)]);
            let (l, s) = (self.link.clone(), self.state.clone());
            pill.set_paint(move |c, snapshot| paint_pill(c, snapshot, &l, &s, i));
            let (l, s, part) = (self.link.clone(), self.state.clone(), *part);
            pill.on_tap(move |c, _, _| open(c, &l, &s, part));
            let (l, s) = (self.link.clone(), self.state.clone());
            pill.on_activate_key(move |c| open(c, &l, &s, part));
            pill.set_parent(&self.canvas);
        }
        refresh(&self.canvas, &self.state);
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Part {
    Date,
    Time,
}

/// The pill widgets of a picker, in order.
fn pills(group: &Canvas) -> Vec<Canvas> {
    let mut out = Vec::new();
    let mut child = group.first_child();
    while let Some(w) = child {
        child = w.next_sibling();
        if let Ok(pill) = w.downcast::<Canvas>() {
            out.push(pill);
        }
    }
    out
}

/// Shows the current value: labels, sizes, text.
fn refresh(group: &Canvas, s: &State) {
    let texts = s.value.get().display(s.mode.get());
    for (pill, text) in pills(group).iter().zip(&texts) {
        pill.update_property(&[Property::Label(text)]);
        pill.queue_draw();
    }
    group.queue_allocate();
}

fn pill_height() -> f32 {
    let (_, h) = control::line_size("0", FONT_SIZE, 400);
    (h + 14.0).ceil()
}

fn popover_of(pill: &Canvas) -> Option<gtk::Popover> {
    pill.first_child().and_then(|p| p.downcast::<gtk::Popover>().ok())
}

fn paint_pill(c: &Canvas, snapshot: &gtk::Snapshot, l: &Link, s: &State, i: usize) {
    let Some(text) = s.value.get().display(s.mode.get()).get(i).cloned() else { return };
    let (w, h) = (c.width() as f32, c.height() as f32);
    let open = popover_of(c).is_some_and(|p| p.is_visible());
    let alpha = if !c.is_sensitive() {
        control::DISABLED_ALPHA
    } else if c.pressed() {
        control::PRESSED_ALPHA
    } else {
        1.0
    };
    let shape = control::round(control::rect(0.0, 0.0, w, h), RADIUS);
    snapshot.push_opacity(alpha as f64);
    control::fill(snapshot, &shape, l.color("secondary"));
    // The value being edited takes the accent, as on iOS.
    let color = if open { l.accent() } else { l.color("secondary_foreground") };
    let layout = control::line(&text, FONT_SIZE, 400, color, None);
    let (tw, th) = control::text_size(&layout);
    control::draw_text(snapshot, &layout, (w - tw) / 2.0, (h - th) / 2.0);
    snapshot.pop();
    if c.shows_focus() {
        control::focus_ring(snapshot, &shape, l.accent());
    }
}

/// The user picked `v`: show it and report it.
fn pick(pill: &Canvas, l: &Link, s: &State, v: Value) {
    if s.muted.get() || v == s.value.get() {
        return;
    }
    s.value.set(v);
    if let Some(group) = pill.parent().and_then(|g| g.downcast::<Canvas>().ok()) {
        refresh(&group, s);
    }
    l.change(serde_json::json!({ "date": v.format(s.mode.get()) }));
}

/// Opens the pill's popover, made on first use and set to the current value.
fn open(pill: &Canvas, l: &Link, s: &Rc<State>, part: Part) {
    if !pill.is_sensitive() {
        return;
    }
    let popover = popover_of(pill).unwrap_or_else(|| {
        let p = gtk::Popover::new();
        p.set_position(gtk::PositionType::Bottom);
        p.set_child(Some(&match part {
            Part::Date => calendar(pill, l, s),
            Part::Time => time_fields(pill, l, s),
        }));
        p.set_parent(pill);
        let weak = pill.downgrade();
        p.connect_closed(move |_| {
            if let Some(pill) = weak.upgrade() {
                pill.update_state(&[A11y::Expanded(Some(false))]);
                pill.queue_draw();
            }
        });
        p
    });
    // Show the current value without reporting it.
    s.muted.set(true);
    let v = s.value.get();
    match popover.child() {
        Some(w) if w.is::<gtk::Calendar>() => {
            if let (Some(cal), Some(date)) = (w.downcast_ref::<gtk::Calendar>(), v.date_time()) {
                cal.select_day(&date);
            }
        }
        Some(w) => {
            let spins: Vec<gtk::SpinButton> = descendants(&w).into_iter().filter_map(|w| w.downcast().ok()).collect();
            if let [hour, minute] = spins.as_slice() {
                hour.set_value(v.hour as f64);
                minute.set_value(v.minute as f64);
            }
        }
        None => {}
    }
    s.muted.set(false);
    pill.update_state(&[A11y::Expanded(Some(true))]);
    popover.popup();
    pill.queue_draw();
}

fn descendants(w: &gtk::Widget) -> Vec<gtk::Widget> {
    let mut out = Vec::new();
    let mut child = w.first_child();
    while let Some(c) = child {
        child = c.next_sibling();
        out.extend(descendants(&c));
        out.push(c);
    }
    out
}

/// The date popover: the GTK calendar. Picking a day closes it.
fn calendar(pill: &Canvas, l: &Link, s: &Rc<State>) -> gtk::Widget {
    let cal = gtk::Calendar::new();
    let (weak, l, s) = (pill.downgrade(), l.clone(), s.clone());
    cal.connect_day_selected(move |cal| {
        let Some(pill) = weak.upgrade() else { return };
        let d = cal.date();
        let v = Value { year: d.year(), month: d.month(), day: d.day_of_month(), ..s.value.get() };
        pick(&pill, &l, &s, v);
        if !s.muted.get() {
            if let Some(p) = popover_of(&pill) {
                p.popdown();
            }
        }
    });
    cal.upcast()
}

/// The time popover: hour and minute spin buttons (wrapping, two digits).
fn time_fields(pill: &Canvas, l: &Link, s: &Rc<State>) -> gtk::Widget {
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    row.set_margin_top(6);
    row.set_margin_bottom(6);
    let spin = |max: f64, label: &str| {
        let b = gtk::SpinButton::with_range(0.0, max, 1.0);
        b.set_wrap(true);
        b.set_numeric(true);
        b.set_orientation(gtk::Orientation::Vertical);
        b.update_property(&[Property::Label(label)]);
        b.connect_output(|b| {
            b.set_text(&format!("{:02}", b.value() as i32));
            glib::Propagation::Stop
        });
        b
    };
    let hour = spin(23.0, "Hour");
    let minute = spin(59.0, "Minute");
    row.append(&hour);
    row.append(&gtk::Label::new(Some(":")));
    row.append(&minute);
    for (b, is_hour) in [(&hour, true), (&minute, false)] {
        let (weak, l, s) = (pill.downgrade(), l.clone(), s.clone());
        b.connect_value_changed(move |b| {
            let Some(pill) = weak.upgrade() else { return };
            let mut v = s.value.get();
            if is_hour {
                v.hour = b.value() as i32;
            } else {
                v.minute = b.value() as i32;
            }
            pick(&pill, &l, &s, v);
        });
        // Enter confirms and closes, like the date popover's day click.
        let weak = pill.downgrade();
        b.connect_activate(move |_| {
            if let Some(p) = weak.upgrade().and_then(|pill| popover_of(&pill)) {
                p.popdown();
            }
        });
    }
    row.upcast()
}

impl Component for DatePicker {
    fn set_prop(&mut self, _cx: &NodeCx, prop: Prop, value: &PropValue) -> bool {
        let s = &self.state;
        match prop {
            Prop::Date => {
                s.raw.replace(match value {
                    PropValue::Str(v) => Some(v.clone()),
                    _ => None,
                });
                s.apply_raw();
                refresh(&self.canvas, s);
            }
            Prop::DateMode => {
                s.mode.set(int_or(value, 0).clamp(0, 2));
                s.apply_raw();
                self.rebuild();
            }
            Prop::Enabled => {
                let on = bool_or(value, true);
                self.canvas.set_sensitive(on);
            }
            _ => return shared_prop(&self.link, &self.canvas, prop, value).unwrap_or(false),
        }
        true
    }

    fn set_event_mask(&mut self, _cx: &NodeCx, mask: u32) {
        self.link.mask.set(mask);
    }
}

/// `nsh/date-picker` measurement (on the JS thread): the localized value in
/// its pill or pills.
pub fn measure(props: &serde_json::Value) -> (f32, f32) {
    let mode = match props["mode"].as_str() {
        Some("time") => 1,
        Some("datetime") => 2,
        _ => 0,
    };
    let base = Value::now();
    let v = props["value"].as_str().and_then(|s| Value::parse(s, base)).unwrap_or(base);
    let texts = v.display(mode);
    let width: f32 = texts.iter().map(|t| (control::line_size(t, FONT_SIZE, 400).0 + 2.0 * PADDING).ceil()).sum::<f32>()
        + GAP * (texts.len().saturating_sub(1)) as f32;
    (width, super::select::MIN_HEIGHT.max(pill_height()))
}

#[cfg(test)]
mod tests {
    use super::*;

    const BASE: Value = Value { year: 2026, month: 1, day: 2, hour: 3, minute: 4 };

    #[test]
    fn parses_every_form_and_formats_by_mode() {
        let v = Value::parse("2026-10-14T09:30", BASE).unwrap();
        assert_eq!(v, Value { year: 2026, month: 10, day: 14, hour: 9, minute: 30 });
        assert_eq!(v.format(0), "2026-10-14");
        assert_eq!(v.format(1), "09:30");
        assert_eq!(v.format(2), "2026-10-14T09:30");
        assert_eq!(Value::parse("2026-10-14", BASE).unwrap().format(2), "2026-10-14T03:04");
        assert_eq!(Value::parse("18:05", BASE).unwrap().format(2), "2026-01-02T18:05");
        assert_eq!(Value::parse("2026-02-30", BASE), None);
        assert_eq!(Value::parse("25:00", BASE), None);
        assert_eq!(Value::parse("nope", BASE), None);
    }
}
