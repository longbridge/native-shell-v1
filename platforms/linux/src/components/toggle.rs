//! `Switch` and `Checkbox` (checkbox or radio by `CheckStyle`), drawn as on iOS.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gtk::accessible::State as A11y;
use gtk::prelude::*;
use gtk::AccessibleTristate;

use native_shell_core::protocol::{Prop, PropValue};

use super::control::{self, bool_or, int_or, Canvas, Link};
use crate::component::{Component, NodeCx};
use crate::mount::color;
use crate::node_view::rgba;

/// Handles the props every control shares; `None` when `prop` is not one of them.
pub fn shared_prop(link: &Link, canvas: &Canvas, prop: Prop, value: &PropValue) -> Option<bool> {
    match prop {
        Prop::TintColor => {
            link.tint.set(color(value));
            canvas.queue_draw();
            Some(true)
        }
        Prop::A11yLabel => {
            // The control itself is what assistive technologies focus; the
            // base view still gets the label too.
            let label = control::str_or(value, "");
            canvas.update_property(&[gtk::accessible::Property::Label(label)]);
            link.label.replace(label.to_string());
            Some(false)
        }
        _ => None,
    }
}

fn tristate(on: bool) -> AccessibleTristate {
    if on {
        AccessibleTristate::True
    } else {
        AccessibleTristate::False
    }
}

// Switch -----------------------------------------------------------------------

const TRACK_W: f32 = 51.0;
const TRACK_H: f32 = 31.0;
const KNOB: f32 = 27.0;
/// How much the knob stretches while pressed, as on iOS.
const KNOB_STRETCH: f32 = 7.0;

struct SwitchState {
    checked: Cell<bool>,
    /// Knob position, 0 off to 1 on; animated between the two.
    pos: Cell<f64>,
    drag_from: Cell<Option<f64>>,
    animation: RefCell<Option<adw::SpringAnimation>>,
}

pub struct Switch {
    link: Link,
    canvas: Canvas,
    state: Rc<SwitchState>,
}

impl Switch {
    pub fn new(cx: &NodeCx) -> Switch {
        let link = Link::new(cx);
        let canvas = Canvas::new(gtk::AccessibleRole::Switch, true);
        let state = Rc::new(SwitchState {
            checked: Cell::new(false),
            pos: Cell::new(0.0),
            drag_from: Cell::new(None),
            animation: RefCell::new(None),
        });
        canvas.update_state(&[A11y::Checked(AccessibleTristate::False)]);

        let (l, s) = (link.clone(), state.clone());
        canvas.set_paint(move |c, snapshot| paint_switch(c, snapshot, &l, &s));

        let drag = gtk::GestureDrag::new();
        let (weak, s) = (canvas.downgrade(), state.clone());
        drag.connect_drag_begin(move |_, _, _| {
            let Some(c) = Canvas::live(&weak) else { return };
            c.set_pressed(true);
            s.drag_from.set(None);
        });
        let (weak, s) = (canvas.downgrade(), state.clone());
        drag.connect_drag_update(move |_, dx, _| {
            let Some(c) = Canvas::live(&weak) else { return };
            let start = match s.drag_from.get() {
                Some(start) => start,
                None if dx.abs() > 3.0 => {
                    s.animation.take();
                    s.drag_from.set(Some(s.pos.get()));
                    s.pos.get()
                }
                None => return,
            };
            let travel = (TRACK_W - KNOB - KNOB_STRETCH - 4.0) as f64 * scale(&c) as f64;
            s.pos.set((start + dx / travel).clamp(0.0, 1.0));
            c.queue_draw();
        });
        let (weak, s, l) = (canvas.downgrade(), state.clone(), link.clone());
        drag.connect_drag_end(move |_, _, _| {
            let Some(c) = Canvas::live(&weak) else { return };
            c.set_pressed(false);
            let on = if s.drag_from.take().is_some() { s.pos.get() > 0.5 } else { !s.checked.get() };
            set_switch(&c, &s, on, true);
            if c.is_sensitive() {
                l.change(serde_json::json!({ "checked": on }));
            }
        });
        let (weak, s) = (canvas.downgrade(), state.clone());
        drag.connect_cancel(move |_, _| {
            let Some(c) = Canvas::live(&weak) else { return };
            c.set_pressed(false);
            s.drag_from.set(None);
            let on = s.checked.get();
            set_switch(&c, &s, on, true);
        });
        canvas.add_controller(drag);

        let (s, l) = (state.clone(), link.clone());
        canvas.on_activate_key(move |c| {
            let on = !s.checked.get();
            set_switch(c, &s, on, true);
            l.change(serde_json::json!({ "checked": on }));
        });

        cx.view.set_content(Some(canvas.upcast_ref()));
        Switch { link, canvas, state }
    }
}

/// The track keeps the iOS size, scaled down only when the node is smaller.
fn scale(c: &Canvas) -> f32 {
    (c.width() as f32 / TRACK_W).min(c.height() as f32 / TRACK_H).min(1.0)
}

fn set_switch(c: &Canvas, s: &Rc<SwitchState>, on: bool, animated: bool) {
    s.checked.set(on);
    c.update_state(&[A11y::Checked(tristate(on))]);
    let to = if on { 1.0 } else { 0.0 };
    s.animation.take();
    if animated && c.is_mapped() && s.pos.get() != to {
        let (weak, state) = (c.downgrade(), Rc::downgrade(s));
        let animation = control::spring(c, s.pos.get(), to, move |v| {
            if let (Some(c), Some(s)) = (weak.upgrade(), state.upgrade()) {
                s.pos.set(v.clamp(0.0, 1.0));
                c.queue_draw();
            }
        });
        s.animation.replace(Some(animation));
    } else {
        s.pos.set(to);
        c.queue_draw();
    }
}

fn paint_switch(c: &Canvas, snapshot: &gtk::Snapshot, link: &Link, s: &SwitchState) {
    let k = scale(c);
    let (w, h) = (TRACK_W * k, TRACK_H * k);
    let x = (c.width() as f32 - w) / 2.0;
    let y = (c.height() as f32 - h) / 2.0;
    let pos = s.pos.get() as f32;
    // The on state is green (`success`, the iOS system green) unless the node sets a tint.
    let on = link.tint.get().unwrap_or_else(|| link.color("success"));
    let off = link.color("secondary");
    let dim = !c.is_sensitive();
    if dim {
        snapshot.push_opacity(control::DISABLED_ALPHA as f64);
    }
    let track = control::round(control::rect(x, y, w, h), h / 2.0);
    control::fill(snapshot, &track, off);
    control::fill(snapshot, &track, control::fade(on, pos));

    let margin = 2.0 * k;
    let knob_h = KNOB * k;
    let knob_w = knob_h + if c.pressed() { KNOB_STRETCH * k } else { 0.0 };
    let travel = w - 2.0 * margin - knob_w;
    let knob = control::round(control::rect(x + margin + travel * pos, y + margin, knob_w, knob_h), knob_h / 2.0);
    control::knob_shadow(snapshot, &knob);
    control::fill(snapshot, &knob, 0xFFFFFFFF);
    if c.shows_focus() {
        control::focus_ring(snapshot, &track, link.ring());
    }
    if dim {
        snapshot.pop();
    }
}

impl Component for Switch {
    fn set_prop(&mut self, _cx: &NodeCx, prop: Prop, value: &PropValue) -> bool {
        match prop {
            Prop::Checked => set_switch(&self.canvas, &self.state, bool_or(value, false), true),
            Prop::Enabled => self.canvas.set_sensitive(bool_or(value, true)),
            _ => return shared_prop(&self.link, &self.canvas, prop, value).unwrap_or(false),
        }
        true
    }

    fn set_event_mask(&mut self, _cx: &NodeCx, mask: u32) {
        self.link.mask.set(mask);
    }
}

// Checkbox and radio -----------------------------------------------------------

struct CheckState {
    checked: Cell<bool>,
    /// 0 checkbox, 1 radio.
    style: Cell<i32>,
}

pub struct Checkbox {
    link: Link,
    canvas: Canvas,
    state: Rc<CheckState>,
}

impl Checkbox {
    pub fn new(cx: &NodeCx) -> Checkbox {
        let link = Link::new(cx);
        let state = Rc::new(CheckState { checked: Cell::new(false), style: Cell::new(0) });
        let canvas = make_check(&link, &state);
        cx.view.set_content(Some(canvas.upcast_ref()));
        Checkbox { link, canvas, state }
    }
}

/// The accessible role is fixed per widget, so a style change makes a new one.
fn make_check(link: &Link, state: &Rc<CheckState>) -> Canvas {
    let role = if state.style.get() == 1 { gtk::AccessibleRole::Radio } else { gtk::AccessibleRole::Checkbox };
    let canvas = Canvas::new(role, true);
    canvas.update_property(&[gtk::accessible::Property::Label(&link.label.borrow())]);
    canvas.update_state(&[A11y::Checked(tristate(state.checked.get()))]);
    let (l, s) = (link.clone(), state.clone());
    canvas.set_paint(move |c, snapshot| paint_check(c, snapshot, &l, &s));
    let (l, s) = (link.clone(), state.clone());
    canvas.on_tap(move |c, _, _| toggle_check(c, &l, &s));
    let (l, s) = (link.clone(), state.clone());
    canvas.on_activate_key(move |c| toggle_check(c, &l, &s));
    canvas
}

fn toggle_check(c: &Canvas, link: &Link, s: &CheckState) {
    // A radio never unchecks itself.
    if s.style.get() == 1 && s.checked.get() {
        return;
    }
    let on = !s.checked.get();
    s.checked.set(on);
    c.update_state(&[A11y::Checked(tristate(on))]);
    c.queue_draw();
    link.change(serde_json::json!({ "checked": on }));
}

fn paint_check(c: &Canvas, snapshot: &gtk::Snapshot, link: &Link, s: &CheckState) {
    // The glyph fills the frame the core laid out (which grows with the font scale).
    let (bw, bh) = (c.width() as f32, c.height() as f32);
    let side = bw.min(bh).max(0.0);
    let unit = side / 22.0;
    let (x, y) = ((bw - side) / 2.0, (bh - side) / 2.0);
    let tint = link.accent();
    let checked = s.checked.get();
    let alpha = if !c.is_sensitive() {
        control::DISABLED_ALPHA
    } else if c.pressed() {
        0.7
    } else {
        1.0
    };
    snapshot.push_opacity(alpha as f64);
    let inset = 0.75 * unit;
    let r = control::rect(x + inset, y + inset, side - 2.0 * inset, side - 2.0 * inset);
    let stroke_color = if checked { tint } else { link.color("muted_foreground") };
    let line = 1.5 * unit;
    let shape = if s.style.get() == 1 {
        let ring = control::round(r, side / 2.0);
        let color = rgba(stroke_color);
        snapshot.append_border(&ring, &[line; 4], &[color; 4]);
        if checked {
            control::circle(snapshot, x + side / 2.0, y + side / 2.0, side * 0.23, tint);
        }
        ring
    } else {
        let square = control::round(r, side * 0.22);
        if checked {
            control::fill(snapshot, &square, tint);
            let p = |fx: f32, fy: f32| (x + side * fx, y + side * fy);
            control::stroke(snapshot, &[p(0.25, 0.5), p(0.43, 0.68), p(0.76, 0.32)], 2.0 * unit, link.color("primary_foreground"));
        } else {
            let color = rgba(stroke_color);
            snapshot.append_border(&square, &[line; 4], &[color; 4]);
        }
        square
    };
    snapshot.pop();
    if c.shows_focus() {
        control::focus_ring(snapshot, &shape, link.ring());
    }
}

impl Component for Checkbox {
    fn set_prop(&mut self, cx: &NodeCx, prop: Prop, value: &PropValue) -> bool {
        match prop {
            Prop::Checked => {
                let on = bool_or(value, false);
                self.state.checked.set(on);
                self.canvas.update_state(&[A11y::Checked(tristate(on))]);
            }
            Prop::Enabled => self.canvas.set_sensitive(bool_or(value, true)),
            Prop::CheckStyle => {
                let style = int_or(value, 0).clamp(0, 1);
                if self.state.style.replace(style) != style {
                    let sensitive = self.canvas.is_sensitive();
                    self.canvas = make_check(&self.link, &self.state);
                    self.canvas.set_sensitive(sensitive);
                    cx.view.set_content(Some(self.canvas.upcast_ref()));
                }
            }
            _ => return shared_prop(&self.link, &self.canvas, prop, value).unwrap_or(false),
        }
        self.canvas.queue_draw();
        true
    }

    fn set_event_mask(&mut self, _cx: &NodeCx, mask: u32) {
        self.link.mask.set(mask);
    }
}
