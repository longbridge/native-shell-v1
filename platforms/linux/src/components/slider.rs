//! `Slider`, drawn as on iOS: a thin track filled with the accent up to a
//! white thumb. Dragging reports `{value, final: false}` at most every 50 ms
//! and `{value, final: true}` when it ends; keys move it a step at a time.

use std::cell::Cell;
use std::rc::Rc;
use std::time::{Duration, Instant};

use gtk::accessible::Property as A11y;
use gtk::prelude::*;
use gtk::{gdk, glib};

use native_shell_core::protocol::{Prop, PropValue};

use super::control::{self, bool_or, number_or, Canvas, Link};
use super::toggle::shared_prop;
use crate::component::{Component, NodeCx};

const THUMB: f32 = 28.0;
const TRACK: f32 = 4.0;
const THROTTLE: Duration = Duration::from_millis(50);

#[derive(Default)]
struct State {
    value: Cell<f64>,
    min: Cell<f64>,
    max: Cell<f64>,
    step: Cell<f64>,
    drag_x: Cell<f64>,
    last_sent: Cell<Option<Instant>>,
}

impl State {
    /// `(min, max)`, sane whatever order the props arrived in.
    fn range(&self) -> (f64, f64) {
        let min = self.min.get();
        (min, self.max.get().max(min))
    }

    /// `v` clamped to the range and rounded to the step.
    fn snap(&self, v: f64) -> f64 {
        let (min, max) = self.range();
        let step = self.step.get();
        let v = if step > 0.0 { min + ((v - min) / step).round() * step } else { v };
        v.clamp(min, max)
    }

    fn fraction(&self) -> f64 {
        let (min, max) = self.range();
        if max > min {
            ((self.value.get() - min) / (max - min)).clamp(0.0, 1.0)
        } else {
            0.0
        }
    }
}

pub struct Slider {
    link: Link,
    canvas: Canvas,
    state: Rc<State>,
}

impl Slider {
    pub fn new(cx: &NodeCx) -> Slider {
        let link = Link::new(cx);
        let canvas = Canvas::new(gtk::AccessibleRole::Slider, true);
        let state = Rc::new(State::default());
        state.max.set(1.0);

        let (l, s) = (link.clone(), state.clone());
        canvas.set_paint(move |c, snapshot| paint(c, snapshot, &l, &s));

        let drag = gtk::GestureDrag::new();
        let (weak, s, l) = (canvas.downgrade(), state.clone(), link.clone());
        drag.connect_drag_begin(move |_, x, _| {
            let Some(c) = Canvas::live(&weak) else { return };
            c.set_pressed(true);
            s.drag_x.set(x);
            s.last_sent.set(None);
            move_to(&c, &s, &l, x, false);
        });
        let (weak, s, l) = (canvas.downgrade(), state.clone(), link.clone());
        drag.connect_drag_update(move |_, dx, _| {
            let Some(c) = Canvas::live(&weak) else { return };
            move_to(&c, &s, &l, s.drag_x.get() + dx, false);
        });
        let (weak, s, l) = (canvas.downgrade(), state.clone(), link.clone());
        drag.connect_drag_end(move |_, dx, _| {
            let Some(c) = Canvas::live(&weak) else { return };
            c.set_pressed(false);
            move_to(&c, &s, &l, s.drag_x.get() + dx, true);
        });
        canvas.add_controller(drag);

        let keys = gtk::EventControllerKey::new();
        let (weak, s, l) = (canvas.downgrade(), state.clone(), link.clone());
        keys.connect_key_pressed(move |_, key, _, _| {
            let Some(c) = Canvas::live(&weak) else { return glib::Propagation::Proceed };
            let (min, max) = s.range();
            let step = if s.step.get() > 0.0 { s.step.get() } else { (max - min) / 100.0 };
            let page = ((max - min) / 10.0).max(step);
            let v = s.value.get();
            let next = match key {
                gdk::Key::Left | gdk::Key::Down | gdk::Key::KP_Left | gdk::Key::KP_Down => v - step,
                gdk::Key::Right | gdk::Key::Up | gdk::Key::KP_Right | gdk::Key::KP_Up => v + step,
                gdk::Key::Page_Down => v - page,
                gdk::Key::Page_Up => v + page,
                gdk::Key::Home => min,
                gdk::Key::End => max,
                _ => return glib::Propagation::Proceed,
            };
            set_value(&c, &s, &l, next, true);
            glib::Propagation::Stop
        });
        canvas.add_controller(keys);

        cx.view.set_content(Some(canvas.upcast_ref()));
        let slider = Slider { link, canvas, state };
        slider.sync();
        slider
    }

    fn sync(&self) {
        let (min, max) = self.state.range();
        self.canvas.update_property(&[A11y::ValueMin(min), A11y::ValueMax(max), A11y::ValueNow(self.state.value.get())]);
        self.canvas.queue_draw();
    }
}

/// The track runs between the thumb's centers at either end.
fn track_span(c: &Canvas) -> (f32, f32) {
    let w = c.width() as f32;
    (THUMB / 2.0, (w - THUMB).max(0.0))
}

fn move_to(c: &Canvas, s: &State, l: &Link, x: f64, last: bool) {
    let (start, len) = track_span(c);
    let f = if len > 0.0 { ((x - start as f64) / len as f64).clamp(0.0, 1.0) } else { 0.0 };
    let (min, max) = s.range();
    set_value(c, s, l, min + f * (max - min), last);
}

fn set_value(c: &Canvas, s: &State, l: &Link, v: f64, last: bool) {
    let v = s.snap(v);
    let changed = v != s.value.get();
    s.value.set(v);
    c.update_property(&[A11y::ValueNow(v)]);
    c.queue_draw();
    let due = s.last_sent.get().is_none_or(|t| t.elapsed() >= THROTTLE);
    if last || (changed && due) {
        s.last_sent.set(Some(Instant::now()));
        l.change(serde_json::json!({ "value": v, "final": last }));
    }
}

fn paint(c: &Canvas, snapshot: &gtk::Snapshot, l: &Link, s: &State) {
    let h = c.height() as f32;
    let (start, len) = track_span(c);
    let cy = h / 2.0;
    let x = start + len * s.fraction() as f32;
    let dim = !c.is_sensitive();
    if dim {
        snapshot.push_opacity(control::DISABLED_ALPHA as f64);
    }
    let track = control::round(control::rect(start, cy - TRACK / 2.0, len, TRACK), TRACK / 2.0);
    control::fill(snapshot, &track, l.color("secondary"));
    let filled = control::round(control::rect(start, cy - TRACK / 2.0, x - start, TRACK), TRACK / 2.0);
    control::fill(snapshot, &filled, l.accent());
    let thumb = control::round(control::rect(x - THUMB / 2.0, cy - THUMB / 2.0, THUMB, THUMB), THUMB / 2.0);
    control::knob_shadow(snapshot, &thumb);
    control::fill(snapshot, &thumb, 0xFFFFFFFF);
    if c.shows_focus() {
        control::focus_ring(snapshot, &thumb, l.ring());
    }
    if dim {
        snapshot.pop();
    }
}

impl Component for Slider {
    fn set_prop(&mut self, _cx: &NodeCx, prop: Prop, value: &PropValue) -> bool {
        let s = &self.state;
        match prop {
            Prop::Value => s.value.set(number_or(value, 0.0)),
            Prop::Min => s.min.set(number_or(value, 0.0)),
            Prop::Max => s.max.set(number_or(value, 1.0)),
            Prop::Step => s.step.set(number_or(value, 0.0).max(0.0)),
            Prop::Enabled => self.canvas.set_sensitive(bool_or(value, true)),
            Prop::A11yValue => {
                self.canvas.update_property(&[A11y::ValueText(control::str_or(value, ""))]);
                return false;
            }
            _ => return shared_prop(&self.link, &self.canvas, prop, value).unwrap_or(false),
        }
        self.sync();
        true
    }

    fn set_event_mask(&mut self, _cx: &NodeCx, mask: u32) {
        self.link.mask.set(mask);
    }
}
