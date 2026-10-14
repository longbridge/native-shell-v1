//! `Segmented`, drawn as the iOS segmented control: equal segments on a
//! rounded fill, the selected one under an accent thumb that slides between
//! them. Each segment is its own focusable tab for keyboards and assistive
//! technologies; arrow keys move the selection.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gtk::accessible::{Property, State as A11y};
use gtk::prelude::*;
use gtk::{gdk, glib, pango};

use native_shell_core::protocol::{Prop, PropValue};

use super::control::{self, bool_or, int_or, Canvas, Link};
use super::toggle::shared_prop;
use crate::component::{Component, NodeCx};
use crate::mount::json;

const INSET: f32 = 2.0;
const RADIUS: f32 = 9.0;
const FONT_SIZE: f32 = 13.0;

#[derive(Default)]
struct State {
    items: RefCell<Vec<String>>,
    /// `SelectedIndex` as the core sent it.
    selected: Cell<i32>,
    /// Where the thumb is drawn, in segments; animated toward `selected`.
    thumb: Cell<f64>,
    animation: RefCell<Option<adw::SpringAnimation>>,
}

impl State {
    fn count(&self) -> usize {
        self.items.borrow().len()
    }

    fn valid(&self, i: i32) -> bool {
        i >= 0 && (i as usize) < self.count()
    }
}

pub struct Segmented {
    link: Link,
    canvas: Canvas,
    state: Rc<State>,
}

impl Segmented {
    pub fn new(cx: &NodeCx) -> Segmented {
        let link = Link::new(cx);
        let canvas = Canvas::new(gtk::AccessibleRole::TabList, false);
        let state = Rc::new(State { selected: Cell::new(-1), thumb: Cell::new(-1.0), ..State::default() });
        let (l, s) = (link.clone(), state.clone());
        canvas.set_paint(move |c, snapshot| paint_track(c, snapshot, &l, &s));
        canvas.set_layout(|c, w, h| {
            let segments: Vec<gtk::Widget> = children(c);
            let n = segments.len().max(1) as f32;
            for (i, seg) in segments.iter().enumerate() {
                let x0 = (w as f32 * i as f32 / n).round() as i32;
                let x1 = (w as f32 * (i + 1) as f32 / n).round() as i32;
                let t = gtk::gsk::Transform::new().translate(&gtk::graphene::Point::new(x0 as f32, 0.0));
                seg.allocate(x1 - x0, h, -1, Some(t));
            }
        });
        cx.view.set_content(Some(canvas.upcast_ref()));
        Segmented { link, canvas, state }
    }

    fn rebuild(&self) {
        for child in children(&self.canvas) {
            child.unparent();
        }
        let count = self.state.count();
        for i in 0..count {
            let seg = Canvas::new(gtk::AccessibleRole::Tab, true);
            seg.update_property(&[Property::Label(&self.state.items.borrow()[i])]);
            let (l, s) = (self.link.clone(), self.state.clone());
            seg.set_paint(move |c, snapshot| paint_segment(c, snapshot, &l, &s, i));
            let (l, s) = (self.link.clone(), self.state.clone());
            seg.on_tap(move |c, _, _| choose(c, &l, &s, i as i32));
            let (l, s) = (self.link.clone(), self.state.clone());
            seg.on_activate_key(move |c| choose(c, &l, &s, i as i32));
            let keys = gtk::EventControllerKey::new();
            let (weak, l, s) = (seg.downgrade(), self.link.clone(), self.state.clone());
            keys.connect_key_pressed(move |_, key, _, _| {
                let Some(seg) = weak.upgrade() else { return glib::Propagation::Proceed };
                let rtl = seg.direction() == gtk::TextDirection::Rtl;
                let delta = match key {
                    gdk::Key::Left | gdk::Key::KP_Left => if rtl { 1 } else { -1 },
                    gdk::Key::Right | gdk::Key::KP_Right => if rtl { -1 } else { 1 },
                    _ => return glib::Propagation::Proceed,
                };
                let to = i as i32 + delta;
                if let (true, Some(group)) = (s.valid(to), seg.parent()) {
                    if let Some(target) = children(&group).get(to as usize) {
                        target.grab_focus();
                    }
                    choose(&seg, &l, &s, to);
                }
                glib::Propagation::Stop
            });
            seg.add_controller(keys);
            seg.set_parent(&self.canvas);
        }
        self.select(self.state.selected.get(), false);
    }

    fn select(&self, i: i32, animated: bool) {
        select(&self.canvas, &self.state, i, animated);
    }
}

fn children(w: &impl IsA<gtk::Widget>) -> Vec<gtk::Widget> {
    let mut out = Vec::new();
    let mut child = w.as_ref().first_child();
    while let Some(c) = child {
        child = c.next_sibling();
        out.push(c);
    }
    out
}

/// Selects `i`. An index beyond the items is kept (the items may come later)
/// but shows no selection.
fn select(group: &Canvas, s: &Rc<State>, requested: i32, animated: bool) {
    s.selected.set(requested);
    let i = if s.valid(requested) { requested } else { -1 };
    for (k, seg) in children(group).iter().enumerate() {
        seg.update_state(&[A11y::Selected(Some(k as i32 == i))]);
        seg.queue_draw();
    }
    s.animation.take();
    let from = s.thumb.get();
    if animated && group.is_mapped() && from >= 0.0 && i >= 0 {
        let (weak, state) = (group.downgrade(), Rc::downgrade(s));
        let animation = control::spring(group, from, i as f64, move |v| {
            if let (Some(g), Some(s)) = (weak.upgrade(), state.upgrade()) {
                s.thumb.set(v);
                g.queue_draw();
            }
        });
        s.animation.replace(Some(animation));
    } else {
        s.thumb.set(i as f64);
    }
    group.queue_draw();
}

/// A tap or key on segment `i`: selects it and reports `{index, value}` (the label).
fn choose(seg: &Canvas, l: &Link, s: &Rc<State>, i: i32) {
    if i == s.selected.get() || !s.valid(i) {
        return;
    }
    let Some(group) = seg.parent().and_then(|p| p.downcast::<Canvas>().ok()) else { return };
    select(&group, s, i, true);
    let label = s.items.borrow()[i as usize].clone();
    l.change(serde_json::json!({ "index": i, "value": label }));
}

fn paint_track(c: &Canvas, snapshot: &gtk::Snapshot, l: &Link, s: &State) {
    let (w, h) = (c.width() as f32, c.height() as f32);
    let dim = !c.is_sensitive();
    if dim {
        snapshot.push_opacity(control::DISABLED_ALPHA as f64);
    }
    control::fill(snapshot, &control::round(control::rect(0.0, 0.0, w, h), RADIUS), l.color("secondary"));
    let n = s.count();
    if n > 0 {
        let seg_w = w / n as f32;
        let thumb = s.thumb.get() as f32;
        // Hairlines between segments the thumb is not next to.
        let sep = control::fade(l.color("muted_foreground"), 0.3);
        for k in 1..n {
            if (k as f32 - 0.5 - thumb).abs() > 1.0 {
                let x = (seg_w * k as f32).round() - 0.5;
                control::fill(snapshot, &control::round(control::rect(x, h * 0.3, 1.0, h * 0.4), 0.0), sep);
            }
        }
        if thumb >= 0.0 {
            let r = control::rect(seg_w * thumb + INSET, INSET, seg_w - 2.0 * INSET, h - 2.0 * INSET);
            let shape = control::round(r, RADIUS - INSET);
            snapshot.append_outset_shadow(&shape, &crate::node_view::rgba(0x1F000000), 0.0, 3.0, 0.0, 8.0);
            control::fill(snapshot, &shape, l.accent());
        }
    }
    if dim {
        snapshot.pop();
    }
}

fn paint_segment(c: &Canvas, snapshot: &gtk::Snapshot, l: &Link, s: &State, i: usize) {
    let Some(label) = s.items.borrow().get(i).cloned() else { return };
    let (w, h) = (c.width() as f32, c.height() as f32);
    let selected = s.selected.get() == i as i32;
    let color = if selected { l.color("primary_foreground") } else { l.color("foreground") };
    let alpha = if !c.is_sensitive() {
        control::DISABLED_ALPHA
    } else if c.pressed() && !selected {
        control::PRESSED_ALPHA
    } else {
        1.0
    };
    let weight = if selected { 600 } else { 500 };
    let layout = control::line(&label, FONT_SIZE, weight, control::fade(color, alpha), Some(w - 12.0));
    layout.set_alignment(pango::Alignment::Center);
    let (_, th) = control::text_size(&layout);
    control::draw_text(snapshot, &layout, 6.0, (h - th) / 2.0);
    if c.shows_focus() {
        let shape = control::round(control::rect(INSET, INSET, w - 2.0 * INSET, h - 2.0 * INSET), RADIUS - INSET);
        control::focus_ring(snapshot, &shape, l.accent());
    }
}

/// `Items` entries are labels, or objects with a `label`.
fn parse_items(value: &PropValue) -> Vec<String> {
    let Some(serde_json::Value::Array(items)) = json(value) else { return Vec::new() };
    items
        .iter()
        .map(|item| match item {
            serde_json::Value::String(s) => s.clone(),
            serde_json::Value::Object(o) => o.get("label").and_then(|l| l.as_str()).unwrap_or_default().to_string(),
            other => other.to_string(),
        })
        .collect()
}

impl Component for Segmented {
    fn set_prop(&mut self, _cx: &NodeCx, prop: Prop, value: &PropValue) -> bool {
        match prop {
            Prop::Items => {
                self.state.items.replace(parse_items(value));
                self.rebuild();
            }
            Prop::SelectedIndex => self.select(int_or(value, -1), true),
            Prop::Enabled => self.canvas.set_sensitive(bool_or(value, true)),
            _ => return shared_prop(&self.link, &self.canvas, prop, value).unwrap_or(false),
        }
        true
    }

    fn set_event_mask(&mut self, _cx: &NodeCx, mask: u32) {
        self.link.mask.set(mask);
    }
}
