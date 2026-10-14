//! `Progress` (a thin bar; indeterminate slides a segment along it) and
//! `Spinner` (the iOS activity indicator), both drawn.

use std::cell::{Cell, RefCell};
use std::f32::consts::TAU;
use std::rc::Rc;

use gtk::accessible::{Property, State as A11y};
use gtk::prelude::*;
use gtk::{glib, graphene};

use native_shell_core::protocol::{Prop, PropValue};

use super::control::{self, bool_or, number_or, Canvas, Link};
use super::toggle::shared_prop;
use crate::component::{Component, NodeCx};

const BAR: f32 = 4.0;

/// Redraws `canvas` every frame while it is mapped and `wanted()` holds.
struct Ticker {
    id: RefCell<Option<gtk::TickCallbackId>>,
}

impl Ticker {
    fn new(canvas: &Canvas, wanted: impl Fn() -> bool + 'static) -> Rc<Ticker> {
        let ticker = Rc::new(Ticker { id: RefCell::new(None) });
        let wanted = Rc::new(wanted);
        let (t, w) = (ticker.clone(), wanted.clone());
        canvas.connect_map(move |c| t.update(c, w()));
        let t = ticker.clone();
        canvas.connect_unmap(move |c| t.update(c, false));
        ticker
    }

    fn update(&self, canvas: &Canvas, run: bool) {
        let run = run && canvas.is_mapped();
        let running = self.id.borrow().is_some();
        if run && !running {
            let id = canvas.add_tick_callback(|c, _| {
                c.queue_draw();
                glib::ControlFlow::Continue
            });
            self.id.replace(Some(id));
        } else if !run && running {
            if let Some(id) = self.id.take() {
                id.remove();
            }
        }
    }
}

/// Seconds on the monotonic clock, for animation phases.
fn now() -> f64 {
    glib::monotonic_time() as f64 / 1e6
}

fn animations_enabled(c: &Canvas) -> bool {
    c.settings().is_gtk_enable_animations()
}

// Progress ---------------------------------------------------------------------

pub struct Progress {
    link: Link,
    canvas: Canvas,
    value: Rc<Cell<f64>>,
    indeterminate: Rc<Cell<bool>>,
    ticker: Rc<Ticker>,
}

impl Progress {
    pub fn new(cx: &NodeCx) -> Progress {
        let link = Link::new(cx);
        let canvas = Canvas::new(gtk::AccessibleRole::ProgressBar, false);
        canvas.update_property(&[Property::ValueMin(0.0), Property::ValueMax(1.0), Property::ValueNow(0.0)]);
        let value = Rc::new(Cell::new(0.0));
        let indeterminate = Rc::new(Cell::new(false));
        let (l, v, i) = (link.clone(), value.clone(), indeterminate.clone());
        canvas.set_paint(move |c, snapshot| paint_bar(c, snapshot, &l, v.get(), i.get()));
        let i = indeterminate.clone();
        let ticker = Ticker::new(&canvas, move || i.get());
        cx.view.set_content(Some(canvas.upcast_ref()));
        Progress { link, canvas, value, indeterminate, ticker }
    }
}

fn paint_bar(c: &Canvas, snapshot: &gtk::Snapshot, l: &Link, value: f64, indeterminate: bool) {
    let (w, h) = (c.width() as f32, c.height() as f32);
    let bar = BAR.min(h);
    let y = (h - bar) / 2.0;
    let track = control::round(control::rect(0.0, y, w, bar), bar / 2.0);
    control::fill(snapshot, &track, l.color("input"));
    let (start, len) = if indeterminate {
        // A third of the track sweeps across once a second and a half; held
        // still in the middle when animations are off.
        let len = w * 0.3;
        let t = if animations_enabled(c) { (now() / 1.5).fract() as f32 } else { 0.5 };
        let ease = 0.5 - 0.5 * (t * std::f32::consts::PI).cos();
        (-len + (w + len) * ease, len)
    } else {
        (0.0, w * value.clamp(0.0, 1.0) as f32)
    };
    snapshot.push_rounded_clip(&track);
    control::fill(snapshot, &control::round(control::rect(start, y, len, bar), bar / 2.0), l.accent());
    snapshot.pop();
}

impl Component for Progress {
    fn set_prop(&mut self, _cx: &NodeCx, prop: Prop, value: &PropValue) -> bool {
        match prop {
            Prop::Value => {
                let v = number_or(value, 0.0).clamp(0.0, 1.0);
                self.value.set(v);
                self.canvas.update_property(&[Property::ValueNow(v)]);
            }
            Prop::Indeterminate => {
                let on = bool_or(value, false);
                self.indeterminate.set(on);
                self.canvas.update_state(&[A11y::Busy(on)]);
                self.ticker.update(&self.canvas, on);
            }
            _ => return shared_prop(&self.link, &self.canvas, prop, value).unwrap_or(false),
        }
        self.canvas.queue_draw();
        true
    }
}

// Spinner ----------------------------------------------------------------------

const SPOKES: usize = 8;

pub struct Spinner {
    link: Link,
    canvas: Canvas,
}

impl Spinner {
    pub fn new(cx: &NodeCx) -> Spinner {
        let link = Link::new(cx);
        let canvas = Canvas::new(gtk::AccessibleRole::ProgressBar, false);
        canvas.update_state(&[A11y::Busy(true)]);
        let l = link.clone();
        canvas.set_paint(move |c, snapshot| paint_spinner(c, snapshot, &l));
        // It spins even with animations off: it is the only sign of progress.
        Ticker::new(&canvas, || true);
        cx.view.set_content(Some(canvas.upcast_ref()));
        Spinner { link, canvas }
    }
}

/// Eight spokes fading behind the leading one, which steps around once a second.
fn paint_spinner(c: &Canvas, snapshot: &gtk::Snapshot, l: &Link) {
    let (w, h) = (c.width() as f32, c.height() as f32);
    let side = w.min(h);
    if side <= 0.0 {
        return;
    }
    let color = l.accent();
    let lead = (now().fract() * SPOKES as f64) as usize % SPOKES;
    let (inner, outer, width) = (side * 0.24, side * 0.46, (side * 0.09).max(1.5));
    snapshot.save();
    snapshot.translate(&graphene::Point::new(w / 2.0, h / 2.0));
    for i in 0..SPOKES {
        let age = (lead + SPOKES - i) % SPOKES;
        let alpha = 1.0 - age as f32 / SPOKES as f32 * 0.75;
        let a = i as f32 / SPOKES as f32 * TAU - TAU / 4.0;
        let (sin, cos) = a.sin_cos();
        control::stroke(snapshot, &[(cos * inner, sin * inner), (cos * outer, sin * outer)], width, control::fade(color, alpha));
    }
    snapshot.restore();
}

impl Component for Spinner {
    fn set_prop(&mut self, _cx: &NodeCx, prop: Prop, value: &PropValue) -> bool {
        match prop {
            // An activity indicator has no value (as on iOS).
            Prop::Indeterminate | Prop::Value => true,
            _ => shared_prop(&self.link, &self.canvas, prop, value).unwrap_or(false),
        }
    }
}
