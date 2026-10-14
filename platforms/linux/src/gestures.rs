//! Pointer and touch input of a plain node, attached only for the events the
//! script listens to (or for native press feedback), so nodes without
//! handlers never take input from their ancestors.
//!
//! - Primary button or touch: `PressIn` / `PressOut`, press feedback,
//!   `Click {x, y, count, modifiers}` (count from GTK's multi-press), and on
//!   the second press of a double click `DoubleClick` instead when the node
//!   listens for it, as on macOS.
//! - `LongPress` after 500 ms held still, or at once on a secondary click: on
//!   a desktop the context click is the long press.
//! - `Pan {phase, dx, dy, vx, vy}` after a 4 pt drag; `Pinch {phase, scale}`
//!   from touchscreen and touchpad pinches.
//!
//! Like GTK's own buttons, a node claims the touch sequence only when it
//! fires (click, long press, pan): a scrolled window above still scrolls when
//! the drag starts on a clickable row, and the innermost clickable node wins.

use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;

use gtk::prelude::*;
use gtk::{gdk, glib};
use serde_json::json;

use native_shell_core::protocol::event_mask::*;
use native_shell_core::protocol::EventType;

use crate::node_view::NodeView;

pub const LONG_PRESS_DELAY: Duration = Duration::from_millis(500);
/// Movement (points) that turns a press into a pan.
pub const PAN_SLOP: f64 = 4.0;
/// A release this long after the last motion carries no fling velocity.
const VELOCITY_STALE_US: i64 = 80_000;

/// The controllers a node has attached, and the press being tracked.
#[derive(Default)]
pub struct Gestures {
    press: Option<gtk::GestureClick>,
    secondary: Option<gtk::GestureClick>,
    drag: Option<gtk::GestureDrag>,
    zoom: Option<gtk::GestureZoom>,
    hover: Option<gtk::EventControllerMotion>,
    track: Rc<Track>,
}

#[derive(Default)]
struct Track {
    /// A press is down on the node and still counts as a tap.
    down: Cell<bool>,
    long_pressed: Cell<bool>,
    panning: Cell<bool>,
    timer: Cell<Option<glib::SourceId>>,
    /// Last drag sample `(dx, dy, µs)` and the smoothed velocity in points/s.
    last: Cell<(f64, f64, i64)>,
    velocity: Cell<(f64, f64)>,
    /// Where the pointer last was over the node (GTK's leave has no position).
    pointer: Cell<(f64, f64)>,
}

impl Track {
    fn cancel_timer(&self) {
        if let Some(id) = self.timer.take() {
            id.remove();
        }
    }
}

/// Adds or removes controllers to match the node's event mask and press props.
pub fn sync(view: &NodeView) {
    let press = view.press();
    let wants_press = view.listens(CLICK | DOUBLE_CLICK | LONG_PRESS | PRESS_IN | PRESS_OUT)
        || press.active_opacity.is_some()
        || press.active_background.is_some();
    let mut g = view.imp_gestures().borrow_mut();
    let track = g.track.clone();
    toggle(view, &mut g.press, wants_press, || {
        primary_press(view, track.clone())
    });
    toggle(view, &mut g.secondary, view.listens(LONG_PRESS), || {
        secondary_press(view)
    });
    toggle(view, &mut g.drag, view.listens(PAN), || {
        pan(view, track.clone())
    });
    toggle(view, &mut g.zoom, view.listens(PINCH), || pinch(view));
    let wants_hover = view.listens(HOVER) || press.hover_opacity.is_some() || press.hover_background.is_some();
    toggle(view, &mut g.hover, wants_hover, || hover(view, track.clone()));
    if !wants_press && !view.listens(PAN) {
        g.track.cancel_timer();
        g.track.down.set(false);
    }
    drop(g);
    // Nothing tracks the pointer any more: no stale hovered look.
    if !wants_hover && press.hovered {
        view.set_hovered(false);
    }
}

fn toggle<C: IsA<gtk::EventController>>(
    view: &NodeView,
    slot: &mut Option<C>,
    on: bool,
    make: impl FnOnce() -> C,
) {
    match (slot.is_some(), on) {
        (false, true) => {
            let c = make();
            view.add_controller(c.clone());
            *slot = Some(c);
        }
        (true, false) => {
            if let Some(c) = slot.take() {
                view.remove_controller(&c);
            }
        }
        _ => {}
    }
}

fn point(x: f64, y: f64) -> String {
    json!({"x": x, "y": y}).to_string()
}

fn primary_press(view: &NodeView, track: Rc<Track>) -> gtk::GestureClick {
    let g = gtk::GestureClick::new();
    g.set_button(gdk::BUTTON_PRIMARY);
    let weak = view.downgrade();
    let t = track.clone();
    g.connect_pressed(move |g, _n, x, y| {
        let Some(view) = weak.upgrade() else { return };
        if view.is_focusable_node() && !view.has_focus() {
            view.grab_focus();
        }
        t.down.set(true);
        t.long_pressed.set(false);
        t.panning.set(false);
        view.set_pressed(true);
        if view.listens(PRESS_IN) {
            view.emit(EventType::PressIn, point(x, y));
        }
        t.cancel_timer();
        if view.listens(LONG_PRESS) {
            let (weak, t2, g) = (view.downgrade(), t.clone(), g.downgrade());
            t.timer.set(Some(glib::timeout_add_local_once(
                LONG_PRESS_DELAY,
                move || {
                    t2.timer.set(None);
                    let (Some(view), Some(g)) = (weak.upgrade(), g.upgrade()) else {
                        return;
                    };
                    if !t2.down.get() || t2.panning.get() {
                        return;
                    }
                    t2.long_pressed.set(true);
                    // The press is spent: no click on release, and no ancestor takes it.
                    g.set_state(gtk::EventSequenceState::Claimed);
                    view.emit(EventType::LongPress, point(x, y));
                },
            )));
        }
    });
    let weak = view.downgrade();
    let t = track.clone();
    g.connect_released(move |g, n, x, y| {
        let Some(view) = weak.upgrade() else { return };
        t.cancel_timer();
        let was_down = t.down.replace(false);
        view.set_pressed(false);
        if was_down && view.listens(PRESS_OUT) {
            view.emit(EventType::PressOut, point(x, y));
        }
        if !was_down || t.long_pressed.get() || t.panning.get() || !view.contains(x, y) {
            return;
        }
        let modifiers = crate::keys::modifiers(g.current_event_state());
        if n == 2 && view.listens(DOUBLE_CLICK) {
            g.set_state(gtk::EventSequenceState::Claimed);
            view.emit(
                EventType::DoubleClick,
                json!({"x": x, "y": y, "count": 2, "modifiers": modifiers}).to_string(),
            );
        } else if view.listens(CLICK) {
            g.set_state(gtk::EventSequenceState::Claimed);
            view.emit(
                EventType::Click,
                json!({"x": x, "y": y, "count": n.max(1), "modifiers": modifiers}).to_string(),
            );
        }
    });
    // Moved past the drag threshold, or another widget claimed the press.
    let end = move |view: &NodeView, t: &Track| {
        t.cancel_timer();
        if t.down.replace(false) {
            view.set_pressed(false);
            if view.listens(PRESS_OUT) {
                view.emit(EventType::PressOut, "{}".into());
            }
        }
    };
    let (weak, t) = (view.downgrade(), track.clone());
    g.connect_stopped(move |_| {
        if let Some(view) = weak.upgrade() {
            end(&view, &t);
        }
    });
    let (weak, t) = (view.downgrade(), track);
    g.connect_cancel(move |_, _| {
        if let Some(view) = weak.upgrade() {
            end(&view, &t);
        }
    });
    g
}

fn secondary_press(view: &NodeView) -> gtk::GestureClick {
    let g = gtk::GestureClick::new();
    g.set_button(gdk::BUTTON_SECONDARY);
    let weak = view.downgrade();
    g.connect_pressed(move |g, n, x, y| {
        let Some(view) = weak.upgrade() else { return };
        if n == 1 {
            g.set_state(gtk::EventSequenceState::Claimed);
            view.emit(EventType::LongPress, point(x, y));
        }
    });
    g
}

fn pan(view: &NodeView, track: Rc<Track>) -> gtk::GestureDrag {
    let g = gtk::GestureDrag::new();
    g.set_button(gdk::BUTTON_PRIMARY);
    let t = track.clone();
    g.connect_drag_begin(move |g, _, _| {
        t.panning.set(false);
        t.last.set((0.0, 0.0, event_time_us(g)));
        t.velocity.set((0.0, 0.0));
    });
    let (weak, t) = (view.downgrade(), track.clone());
    g.connect_drag_update(move |g, dx, dy| {
        let Some(view) = weak.upgrade() else { return };
        sample_velocity(&t, dx, dy, event_time_us(g));
        if !t.panning.get() {
            if dx.hypot(dy) < PAN_SLOP || t.long_pressed.get() {
                return;
            }
            t.panning.set(true);
            t.cancel_timer();
            // A drag is not a tap: drop the pressed look, as iOS buttons do.
            if t.down.replace(false) {
                view.set_pressed(false);
                if view.listens(PRESS_OUT) {
                    view.emit(EventType::PressOut, "{}".into());
                }
            }
            g.set_state(gtk::EventSequenceState::Claimed);
            send_pan(&view, &t, "began", dx, dy);
        } else {
            send_pan(&view, &t, "changed", dx, dy);
        }
    });
    let (weak, t) = (view.downgrade(), track.clone());
    g.connect_drag_end(move |g, dx, dy| {
        let Some(view) = weak.upgrade() else { return };
        // `panning` stays set until the next press so the release is not a click.
        if t.panning.get() {
            if event_time_us(g) - t.last.get().2 > VELOCITY_STALE_US {
                t.velocity.set((0.0, 0.0));
            }
            send_pan(&view, &t, "ended", dx, dy);
        }
    });
    let (weak, t) = (view.downgrade(), track);
    g.connect_cancel(move |g, _| {
        let Some(view) = weak.upgrade() else { return };
        if t.panning.get() {
            let (dx, dy) = g.offset().unwrap_or((0.0, 0.0));
            send_pan(&view, &t, "cancelled", dx, dy);
        }
    });
    g
}

fn event_time_us(g: &impl IsA<gtk::EventController>) -> i64 {
    g.current_event_time() as i64 * 1000
}

/// Velocity of the drag in points per second, lightly smoothed so one jittery
/// sample does not decide a fling.
fn sample_velocity(t: &Track, dx: f64, dy: f64, now: i64) {
    let (lx, ly, lt) = t.last.get();
    let dt = (now - lt) as f64 / 1e6;
    if dt <= 0.0 {
        t.last.set((dx, dy, lt));
        return;
    }
    let (vx, vy) = ((dx - lx) / dt, (dy - ly) / dt);
    let (ox, oy) = t.velocity.get();
    t.velocity.set((0.8 * vx + 0.2 * ox, 0.8 * vy + 0.2 * oy));
    t.last.set((dx, dy, now));
}

fn send_pan(view: &NodeView, t: &Track, phase: &str, dx: f64, dy: f64) {
    let (vx, vy) = t.velocity.get();
    view.emit(
        EventType::Pan,
        json!({"phase": phase, "dx": dx, "dy": dy, "vx": vx, "vy": vy}).to_string(),
    );
}

fn pinch(view: &NodeView) -> gtk::GestureZoom {
    let g = gtk::GestureZoom::new();
    let weak = view.downgrade();
    g.connect_begin(move |g, _| {
        let Some(view) = weak.upgrade() else { return };
        g.set_state(gtk::EventSequenceState::Claimed);
        view.emit(
            EventType::Pinch,
            json!({"phase": "began", "scale": 1.0}).to_string(),
        );
    });
    let weak = view.downgrade();
    g.connect_scale_changed(move |_, scale| {
        if let Some(view) = weak.upgrade() {
            view.emit(
                EventType::Pinch,
                json!({"phase": "changed", "scale": scale}).to_string(),
            );
        }
    });
    let weak = view.downgrade();
    g.connect_end(move |g, _| {
        if let Some(view) = weak.upgrade() {
            view.emit(
                EventType::Pinch,
                json!({"phase": "ended", "scale": g.scale_delta()}).to_string(),
            );
        }
    });
    let weak = view.downgrade();
    g.connect_cancel(move |g, _| {
        if let Some(view) = weak.upgrade() {
            view.emit(
                EventType::Pinch,
                json!({"phase": "cancelled", "scale": g.scale_delta()}).to_string(),
            );
        }
    });
    g
}

/// Pointer entry and exit: hover feedback, and `Hover {entered, x, y}` for
/// HOVER listeners. Touch input does not hover.
fn hover(view: &NodeView, track: Rc<Track>) -> gtk::EventControllerMotion {
    let c = gtk::EventControllerMotion::new();
    let (weak, t) = (view.downgrade(), track.clone());
    c.connect_enter(move |c, x, y| {
        let Some(view) = weak.upgrade() else { return };
        if is_touch(c) {
            return;
        }
        t.pointer.set((x, y));
        pointer_hover(&view, true, x, y);
    });
    let t = track.clone();
    c.connect_motion(move |_, x, y| t.pointer.set((x, y)));
    let weak = view.downgrade();
    c.connect_leave(move |_| {
        let Some(view) = weak.upgrade() else { return };
        let (x, y) = track.pointer.get();
        pointer_hover(&view, false, x, y);
    });
    c
}

fn is_touch(c: &gtk::EventControllerMotion) -> bool {
    c.current_event_device().is_some_and(|d| d.source() == gdk::InputSource::Touchscreen)
}

/// The pointer entered or left `view` (the motion controller's crossings;
/// tests call it directly).
pub fn pointer_hover(view: &NodeView, entered: bool, x: f64, y: f64) {
    if view.press().hovered == entered {
        return;
    }
    view.set_hovered(entered);
    if view.listens(HOVER) {
        view.emit(EventType::Hover, json!({"entered": entered, "x": x, "y": y}).to_string());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn velocity_is_points_per_second() {
        let t = Track::default();
        t.last.set((0.0, 0.0, 0));
        sample_velocity(&t, 10.0, -5.0, 10_000);
        let (vx, vy) = t.velocity.get();
        assert!((vx - 800.0).abs() < 1e-6 && (vy + 400.0).abs() < 1e-6);
        // Two samples with the same timestamp never divide by zero.
        sample_velocity(&t, 12.0, -5.0, 10_000);
        assert!(t.velocity.get().0.is_finite());
    }
}
