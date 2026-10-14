//! What the native controls share: the widget they draw on, theme colors, the
//! `Change` link back to the script, and the iOS-style chrome helpers.
//!
//! The controls a phone draws itself (switch, checkbox, slider, progress,
//! spinner, segmented control, the select and date picker buttons) are drawn
//! here rather than restyled GTK widgets: GTK themes differ per desktop and a
//! per-node tint cannot be expressed with a display-wide CSS provider, while
//! drawing gives the exact iOS geometry on every theme. What the OS provides
//! (the select menu, the calendar and time fields) stays GTK-native, in
//! popovers anchored on the drawn buttons.

use std::cell::{Cell, RefCell};
use std::rc::{Rc, Weak};

use adw::prelude::*;
use gtk::subclass::prelude::*;
use gtk::{gdk, glib, graphene, gsk, pango};

use native_shell_core::host::WidthMode;
use native_shell_core::protocol::{event_mask, EventType, PropValue};
use native_shell_core::theme::{Appearance, Theme};

use crate::component::{NodeCx, Shared};
use crate::node_view::{rgba, rounded_rect};

pub type Paint = Box<dyn Fn(&Canvas, &gtk::Snapshot)>;
pub type Layout = Box<dyn Fn(&Canvas, i32, i32)>;

mod imp {
    use super::*;

    #[derive(Default)]
    pub struct Canvas {
        pub paint: RefCell<Option<Paint>>,
        pub layout: RefCell<Option<Layout>>,
        pub pressed: Cell<bool>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Canvas {
        const NAME: &'static str = "NshControl";
        type Type = super::Canvas;
        type ParentType = gtk::Widget;
    }

    impl ObjectImpl for Canvas {
        fn constructed(&self) {
            self.parent_constructed();
            let obj = self.obj();
            // Focus and sensitivity change what is drawn (ring, dimming).
            let focus = gtk::EventControllerFocus::new();
            let weak = obj.downgrade();
            focus.connect_enter(move |_| redraw(&weak));
            let weak = obj.downgrade();
            focus.connect_leave(move |_| redraw(&weak));
            obj.add_controller(focus);
            obj.connect_sensitive_notify(|c| c.queue_draw());
        }

        fn dispose(&self) {
            while let Some(child) = self.obj().first_child() {
                child.unparent();
            }
        }
    }

    fn redraw(weak: &glib::WeakRef<super::Canvas>) {
        if let Some(c) = weak.upgrade() {
            c.queue_draw();
        }
    }

    impl WidgetImpl for Canvas {
        fn measure(&self, _orientation: gtk::Orientation, _for_size: i32) -> (i32, i32, i32, i32) {
            (0, 0, -1, -1)
        }

        fn size_allocate(&self, width: i32, height: i32, _baseline: i32) {
            let obj = self.obj();
            if let Some(layout) = self.layout.borrow().as_ref() {
                layout(&obj, width, height);
            }
            let mut child = obj.first_child();
            while let Some(widget) = child {
                child = widget.next_sibling();
                if let Some(popover) = widget.downcast_ref::<gtk::Popover>() {
                    popover.present();
                }
            }
        }

        fn snapshot(&self, snapshot: &gtk::Snapshot) {
            let obj = self.obj();
            if let Some(paint) = self.paint.borrow().as_ref() {
                paint(&obj, snapshot);
            }
            let mut child = obj.first_child();
            while let Some(widget) = child {
                child = widget.next_sibling();
                if !widget.is::<gtk::Native>() {
                    obj.snapshot_child(&widget, snapshot);
                }
            }
        }
    }
}

glib::wrapper! {
    /// A focusable widget with a fixed accessible role that paints through a
    /// closure and lays out its (drawn) children through another.
    pub struct Canvas(ObjectSubclass<imp::Canvas>)
        @extends gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl Canvas {
    /// The accessible role is fixed at construction in GTK.
    pub fn new(role: gtk::AccessibleRole, focusable: bool) -> Canvas {
        glib::Object::builder().property("accessible-role", role).property("focusable", focusable).build()
    }

    pub fn set_paint(&self, paint: impl Fn(&Canvas, &gtk::Snapshot) + 'static) {
        self.imp().paint.replace(Some(Box::new(paint)));
        self.queue_draw();
    }

    pub fn set_layout(&self, layout: impl Fn(&Canvas, i32, i32) + 'static) {
        self.imp().layout.replace(Some(Box::new(layout)));
        self.queue_allocate();
    }

    pub fn pressed(&self) -> bool {
        self.imp().pressed.get()
    }

    pub fn set_pressed(&self, pressed: bool) {
        if self.imp().pressed.replace(pressed) != pressed {
            self.queue_draw();
        }
    }

    /// Keyboard focus is shown only when it came from the keyboard, as GTK does.
    pub fn shows_focus(&self) -> bool {
        self.has_visible_focus()
    }

    /// The canvas, if it takes input: GTK sends none to an insensitive widget,
    /// but its gestures may still be driven programmatically.
    pub fn live(weak: &glib::WeakRef<Canvas>) -> Option<Canvas> {
        weak.upgrade().filter(|c| c.is_sensitive())
    }

    /// Activates on Space and Enter, like a GTK button.
    pub fn on_activate_key(&self, f: impl Fn(&Canvas) + 'static) {
        let keys = gtk::EventControllerKey::new();
        let weak = self.downgrade();
        keys.connect_key_pressed(move |_, key, _, mods| {
            let Some(canvas) = weak.upgrade().filter(|c| c.is_sensitive()) else { return glib::Propagation::Proceed };
            let plain = !mods.intersects(gdk::ModifierType::CONTROL_MASK | gdk::ModifierType::ALT_MASK);
            if plain && matches!(key, gdk::Key::space | gdk::Key::Return | gdk::Key::KP_Enter | gdk::Key::ISO_Enter) {
                f(&canvas);
                return glib::Propagation::Stop;
            }
            glib::Propagation::Proceed
        });
        self.add_controller(keys);
    }

    /// A tap (press and release inside) calls `f`; the canvas shows its pressed
    /// state meanwhile.
    pub fn on_tap(&self, f: impl Fn(&Canvas, f64, f64) + 'static) {
        let click = gtk::GestureClick::new();
        click.set_button(gdk::BUTTON_PRIMARY);
        let weak = self.downgrade();
        click.connect_pressed(move |_, _, _, _| {
            if let Some(c) = weak.upgrade().filter(|c| c.is_sensitive()) {
                c.set_pressed(true);
            }
        });
        let weak = self.downgrade();
        click.connect_released(move |gesture, _, x, y| {
            let Some(c) = weak.upgrade().filter(|c| c.is_sensitive()) else { return };
            c.set_pressed(false);
            let inside = x >= 0.0 && y >= 0.0 && x <= c.width() as f64 && y <= c.height() as f64;
            if inside {
                gesture.set_state(gtk::EventSequenceState::Claimed);
                f(&c, x, y);
            }
        });
        let weak = self.downgrade();
        click.connect_stopped(move |_| {
            if let Some(c) = weak.upgrade() {
                c.set_pressed(false);
            }
        });
        self.add_controller(click);
    }
}

/// A control's way back to its node and the surface's theme, held by the
/// closures on its widgets (weakly, since the root view owns those widgets).
#[derive(Clone)]
pub struct Link {
    pub node: u32,
    shared: Weak<Shared>,
    /// The listeners the script attached (`SetEventMask`).
    pub mask: Rc<Cell<u32>>,
    /// `TintColor` from the core, overriding the theme's `primary`.
    pub tint: Rc<Cell<Option<u32>>>,
    /// `A11yLabel`, kept for widgets made after it arrived.
    pub label: Rc<RefCell<String>>,
}

impl Link {
    pub fn new(cx: &NodeCx) -> Link {
        Link {
            node: cx.node,
            shared: Rc::downgrade(cx.shared),
            mask: Rc::new(Cell::new(0)),
            tint: Rc::new(Cell::new(None)),
            label: Rc::default(),
        }
    }

    /// Sends `Change` when the script listens for it.
    pub fn change(&self, payload: serde_json::Value) {
        if self.mask.get() & event_mask::CHANGE == 0 {
            return;
        }
        if let Some(shared) = self.shared.upgrade() {
            (shared.events)(self.node, EventType::Change, payload.to_string());
        }
    }

    /// A theme color token, from the core's `ThemeTokens` or the defaults
    /// before the first ones arrive.
    pub fn color(&self, token: &str) -> u32 {
        let Some(shared) = self.shared.upgrade() else { return 0xFF00_0000 };
        let theme = shared.theme.borrow();
        theme
            .color(token)
            .or_else(|| Theme::default_for(theme.appearance).color(token))
            .unwrap_or(0xFF00_0000)
    }

    pub fn dark(&self) -> bool {
        self.shared.upgrade().is_some_and(|s| s.theme.borrow().appearance == Appearance::Dark)
    }

    /// The accent: the node's `TintColor`, else the theme's `primary`.
    pub fn accent(&self) -> u32 {
        self.tint.get().unwrap_or_else(|| self.color("primary"))
    }
}

// Prop values ------------------------------------------------------------------

pub fn bool_or(value: &PropValue, default: bool) -> bool {
    match value {
        PropValue::Bool(b) => *b,
        PropValue::I32(i) => *i != 0,
        _ => default,
    }
}

pub fn int_or(value: &PropValue, default: i32) -> i32 {
    match value {
        PropValue::I32(i) => *i,
        PropValue::F32(f) if f.is_finite() => *f as i32,
        _ => default,
    }
}

pub fn number_or(value: &PropValue, default: f64) -> f64 {
    match value {
        PropValue::F32(f) if f.is_finite() => *f as f64,
        PropValue::I32(i) => *i as f64,
        _ => default,
    }
}

pub fn str_or<'a>(value: &'a PropValue, default: &'a str) -> &'a str {
    match value {
        PropValue::Str(s) | PropValue::Json(s) => s,
        _ => default,
    }
}

// Drawing ----------------------------------------------------------------------

/// `color` with its alpha multiplied by `factor`.
pub fn fade(color: u32, factor: f32) -> u32 {
    let a = ((color >> 24) as f32 * factor.clamp(0.0, 1.0)).round() as u32;
    (a << 24) | (color & 0x00FF_FFFF)
}

/// Opacity of a disabled control's drawing (the iOS dimming).
pub const DISABLED_ALPHA: f32 = 0.4;
/// Opacity of a pressed button's content (the iOS highlight).
pub const PRESSED_ALPHA: f32 = 0.5;

pub fn rect(x: f32, y: f32, w: f32, h: f32) -> graphene::Rect {
    graphene::Rect::new(x, y, w.max(0.0), h.max(0.0))
}

/// A rectangle with every corner rounded by `r`.
pub fn round(bounds: graphene::Rect, r: f32) -> gsk::RoundedRect {
    let r = r.min(bounds.width() / 2.0).min(bounds.height() / 2.0).max(0.0);
    rounded_rect(bounds, [r; 4])
}

pub fn fill(snapshot: &gtk::Snapshot, shape: &gsk::RoundedRect, color: u32) {
    snapshot.push_rounded_clip(shape);
    snapshot.append_color(&rgba(color), shape.bounds());
    snapshot.pop();
}

/// The keyboard focus ring: a soft accent halo just outside `shape`.
pub fn focus_ring(snapshot: &gtk::Snapshot, shape: &gsk::RoundedRect, accent: u32) {
    let mut outer = *shape;
    outer.shrink(-3.0, -3.0, -3.0, -3.0);
    let c = rgba(fade(accent, 0.5));
    snapshot.append_border(&outer, &[3.0; 4], &[c, c, c, c]);
}

/// The drop shadow under an iOS knob or thumb.
pub fn knob_shadow(snapshot: &gtk::Snapshot, shape: &gsk::RoundedRect) {
    snapshot.append_outset_shadow(shape, &rgba(0x26000000), 0.0, 3.0, 0.0, 8.0);
    snapshot.append_outset_shadow(shape, &rgba(0x0F000000), 0.0, 0.5, 0.0, 1.0);
}

/// One line of control text, `size` points at CSS `weight` in `color`, as the
/// text module lays it out, so what is drawn matches what the JS thread
/// measured. Too long for `width`, it ends in an ellipsis.
pub fn line(text: &str, size: f32, weight: u16, color: u32, width: Option<f32>) -> pango::Layout {
    let mut json: serde_json::Value = serde_json::from_str(&line_json(text, size, weight)).unwrap_or_default();
    json["runs"][0]["color"] = color.into();
    let layout = crate::text::build(&crate::text::parse(&json.to_string()), None, crate::text::font_scale()).layout;
    if let Some(w) = width {
        layout.set_width((w.max(0.0) * pango::SCALE as f32) as i32);
        layout.set_ellipsize(pango::EllipsizeMode::End);
    }
    layout
}

fn line_json(text: &str, size: f32, weight: u16) -> String {
    serde_json::json!({ "runs": [{ "text": text, "size": size, "weight": weight }], "lines": 1 }).to_string()
}

/// Size of a line of control text, on any thread (the font scale applied).
pub fn line_size(text: &str, size: f32, weight: u16) -> (f32, f32) {
    let m = crate::text::measure(&line_json(text, size, weight), WidthMode::Undefined);
    (m.width, m.height)
}

/// Draws `layout` with its top-left corner at `(x, y)`.
pub fn draw_text(snapshot: &gtk::Snapshot, layout: &pango::Layout, x: f32, y: f32) {
    snapshot.save();
    snapshot.translate(&graphene::Point::new(x, y));
    // The layout carries its color.
    snapshot.append_layout(layout, &gdk::RGBA::BLACK);
    snapshot.restore();
}

/// Size of `layout` in points.
pub fn text_size(layout: &pango::Layout) -> (f32, f32) {
    let (_, logical) = layout.extents();
    let s = pango::SCALE as f32;
    (logical.width() as f32 / s, logical.height() as f32 / s)
}

/// Strokes a polyline with round caps and joins.
pub fn stroke(snapshot: &gtk::Snapshot, points: &[(f32, f32)], width: f32, color: u32) {
    let Some((first, rest)) = points.split_first() else { return };
    let builder = gsk::PathBuilder::new();
    builder.move_to(first.0, first.1);
    for p in rest {
        builder.line_to(p.0, p.1);
    }
    let s = gsk::Stroke::new(width);
    s.set_line_cap(gsk::LineCap::Round);
    s.set_line_join(gsk::LineJoin::Round);
    snapshot.append_stroke(&builder.to_path(), &s, &rgba(color));
}

pub fn circle(snapshot: &gtk::Snapshot, cx: f32, cy: f32, r: f32, color: u32) {
    fill(snapshot, &round(rect(cx - r, cy - r, 2.0 * r, 2.0 * r), r), color);
}

/// An iOS-like spring from `from` to `to`; skipped when animations are off
/// (Reduce Motion), which `adw` honors.
pub fn spring(widget: &impl IsA<gtk::Widget>, from: f64, to: f64, step: impl Fn(f64) + 'static) -> adw::SpringAnimation {
    let target = adw::CallbackAnimationTarget::new(step);
    let animation =
        adw::SpringAnimation::new(widget, from, to, adw::SpringParams::new(0.9, 1.0, 600.0), target);
    animation.set_clamp(false);
    animation.play();
    animation
}

