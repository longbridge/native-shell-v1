//! The base widget of every node, the counterpart of a `UIView` positioned by
//! the core.
//!
//! Layout is done by taffy in the core, so a node never asks its children for
//! sizes: it allocates each child node at the frame the batch gave it, and its
//! optional native `content` widget (an entry, a switch…) at its own bounds.
//! It paints the box decorations itself so radii, borders, gradients and
//! shadows match the other renderers exactly instead of going through CSS.
//!
//! Frames are fractional points. GTK allocates whole logical pixels, so a node
//! is allocated at the rounded-up size, placed with a fractional translation
//! and painted at its exact size; edges are snapped to device pixels in the
//! parent's coordinates (as Android does) so neighbours never drift apart.

use std::cell::{Cell, RefCell};
use std::rc::{Rc, Weak};

use gtk::prelude::*;
use gtk::subclass::prelude::*;
use gtk::{gdk, glib, graphene, gsk};

use native_shell_core::protocol::{EventType, Frame};

use crate::animator::{self, Anim, AnimationSpec};
use crate::component::Shared;

/// A paint callback for content drawn by the component (text, shapes…), in
/// the node's own coordinates, between the background and the children.
pub type DrawFn = Box<dyn Fn(&NodeView, &gtk::Snapshot)>;

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Shadow {
    pub color: u32,
    pub x: f32,
    pub y: f32,
    pub blur: f32,
    pub spread: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct BoxStyle {
    pub background: Option<u32>,
    /// `[top-left, top-right, bottom-right, bottom-left]`.
    pub radii: [f32; 4],
    /// `[top, right, bottom, left]`.
    pub border_widths: [f32; 4],
    pub border_color: Option<u32>,
    pub shadow: Option<Shadow>,
    pub clips: bool,
}

/// `Prop::Gradient`: linear, CSS angle convention (0 = towards the top, 90 = right).
#[derive(Debug, Clone, PartialEq)]
pub struct Gradient {
    pub angle: f32,
    /// `(0xAARRGGBB, offset)`, offsets ascending.
    pub stops: Vec<(u32, f32)>,
}

/// Native press feedback (`ActiveOpacity`, `ActiveBackground`).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Press {
    pub active_opacity: Option<f32>,
    pub active_background: Option<u32>,
    pub pressed: bool,
}

/// `0xAARRGGBB` to a GDK color.
pub fn rgba(color: u32) -> gdk::RGBA {
    let c = |shift: u32| ((color >> shift) & 0xff) as f32 / 255.0;
    gdk::RGBA::new(c(16), c(8), c(0), c(24))
}

struct Running<T> {
    from: T,
    to: T,
    anim: Anim,
    /// Frame clock time (µs) of the first tick.
    start: Option<i64>,
}

mod imp {
    use super::*;

    pub struct NodeView {
        pub node: Cell<u32>,
        pub shared: RefCell<Weak<Shared>>,
        pub mask: Cell<u32>,
        /// The frame the core gave.
        pub frame: Cell<Option<Frame>>,
        /// The frame on screen; differs from `frame` while a transition runs.
        pub presented: Cell<Option<Frame>>,
        /// Exact painted size (the allocation is rounded up to whole pixels).
        pub size: Cell<Option<(f32, f32)>>,
        pub style: Cell<BoxStyle>,
        pub gradient: RefCell<Option<Gradient>>,
        /// Extra transform applied around the frame's center (`Prop::Transform`).
        pub transform: RefCell<Option<gsk::Transform>>,
        pub content: RefCell<Option<gtk::Widget>>,
        pub draw: RefCell<Option<DrawFn>>,
        /// The opacity the core set, and the one on screen while it fades.
        pub opacity: Cell<f64>,
        pub shown_opacity: Cell<f64>,
        pub press: Cell<Press>,
        /// `[top, right, bottom, left]` added around the bounds for hit testing.
        pub hit_slop: Cell<[f32; 4]>,
        pub animation: Cell<Option<AnimationSpec>>,
        pub(super) frame_anim: RefCell<Option<Running<Frame>>>,
        pub(super) opacity_anim: RefCell<Option<Running<f64>>>,
        pub tick: RefCell<Option<gtk::TickCallbackId>>,
        pub gestures: RefCell<crate::gestures::Gestures>,
        pub a11y: RefCell<crate::a11y::A11y>,
        /// `Prop::Focusable`: a keyboard focus stop even without click handlers.
        pub focusable: Cell<bool>,
    }

    impl Default for NodeView {
        fn default() -> Self {
            NodeView {
                node: Cell::new(0),
                shared: RefCell::new(Weak::new()),
                mask: Cell::new(0),
                frame: Cell::new(None),
                presented: Cell::new(None),
                size: Cell::new(None),
                style: Cell::new(BoxStyle::default()),
                gradient: RefCell::new(None),
                transform: RefCell::new(None),
                content: RefCell::new(None),
                draw: RefCell::new(None),
                opacity: Cell::new(1.0),
                shown_opacity: Cell::new(1.0),
                press: Cell::new(Press::default()),
                hit_slop: Cell::new([0.0; 4]),
                animation: Cell::new(None),
                frame_anim: RefCell::new(None),
                opacity_anim: RefCell::new(None),
                tick: RefCell::new(None),
                gestures: RefCell::new(Default::default()),
                a11y: RefCell::new(Default::default()),
                focusable: Cell::new(false),
            }
        }
    }

    #[glib::object_subclass]
    impl ObjectSubclass for NodeView {
        const NAME: &'static str = "NshNodeView";
        type Type = super::NodeView;
        type ParentType = gtk::Widget;

        fn class_init(klass: &mut Self::Class) {
            klass.set_accessible_role(gtk::AccessibleRole::Generic);
            // Activation is the keyboard and assistive-technology click: bound
            // to Enter and Space on the focused node.
            klass.install_action("node.activate", None, |view, _, _| view.activate_click());
            for key in [
                gdk::Key::Return,
                gdk::Key::ISO_Enter,
                gdk::Key::KP_Enter,
                gdk::Key::space,
                gdk::Key::KP_Space,
            ] {
                klass.add_binding_action(key, gdk::ModifierType::empty(), "node.activate");
            }
        }
    }

    impl ObjectImpl for NodeView {
        fn dispose(&self) {
            if let Some(id) = self.tick.take() {
                id.remove();
            }
            while let Some(child) = self.obj().first_child() {
                child.unparent();
            }
        }
    }

    impl WidgetImpl for NodeView {
        fn measure(&self, _orientation: gtk::Orientation, _for_size: i32) -> (i32, i32, i32, i32) {
            (0, 0, -1, -1)
        }

        fn size_allocate(&self, width: i32, height: i32, _baseline: i32) {
            let obj = self.obj();
            let mut child = obj.first_child();
            while let Some(widget) = child {
                child = widget.next_sibling();
                if let Some(node) = widget.downcast_ref::<super::NodeView>() {
                    node.allocate_at_frame();
                } else {
                    // Native content fills the node. GTK refuses allocations
                    // below a widget's minimum, so give it at least that and
                    // let the node clip.
                    let (min_w, _, _, _) = widget.measure(gtk::Orientation::Horizontal, -1);
                    let w = width.max(min_w);
                    let (min_h, _, _, _) = widget.measure(gtk::Orientation::Vertical, w);
                    widget.allocate(w, height.max(min_h), -1, None);
                }
            }
        }

        fn contains(&self, x: f64, y: f64) -> bool {
            let (w, h) = self.obj().paint_size();
            let [t, r, b, l] = self.hit_slop.get();
            x >= -l as f64 && y >= -t as f64 && x < (w + r) as f64 && y < (h + b) as f64
        }

        fn state_flags_changed(&self, old: &gtk::StateFlags) {
            self.parent_state_flags_changed(old);
            let changed = self.obj().state_flags() ^ *old;
            if changed.intersects(gtk::StateFlags::FOCUS_VISIBLE | gtk::StateFlags::FOCUSED) {
                self.obj().queue_draw();
            }
        }

        fn snapshot(&self, snapshot: &gtk::Snapshot) {
            self.obj().paint(snapshot);
        }
    }
}

glib::wrapper! {
    pub struct NodeView(ObjectSubclass<imp::NodeView>)
        @extends gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl Default for NodeView {
    fn default() -> Self {
        Self::new()
    }
}

impl NodeView {
    pub fn new() -> NodeView {
        glib::Object::new()
    }

    /// Binds the view to its node: events go to `shared`'s sink with this id.
    pub fn bind(&self, node: u32, shared: &Rc<Shared>) {
        self.imp().node.set(node);
        self.imp().shared.replace(Rc::downgrade(shared));
    }

    pub fn node(&self) -> u32 {
        self.imp().node.get()
    }

    pub fn shared(&self) -> Option<Rc<Shared>> {
        self.imp().shared.borrow().upgrade()
    }

    /// Sends an event from this node.
    pub fn emit(&self, event: EventType, payload: String) {
        if let Some(shared) = self.shared() {
            (shared.events)(self.node(), event, payload);
        }
    }

    pub(crate) fn imp_gestures(&self) -> &RefCell<crate::gestures::Gestures> {
        &self.imp().gestures
    }

    pub(crate) fn imp_a11y(&self) -> &RefCell<crate::a11y::A11y> {
        &self.imp().a11y
    }

    pub fn event_mask(&self) -> u32 {
        self.imp().mask.get()
    }

    pub fn listens(&self, bit: u32) -> bool {
        self.event_mask() & bit != 0
    }

    /// The listeners the script attached changed: base gestures, keyboard
    /// focus and the accessible role follow them.
    pub fn set_event_mask(&self, mask: u32) {
        self.imp().mask.set(mask);
        crate::gestures::sync(self);
        self.update_focusable();
        crate::a11y::refresh(self);
    }

    pub fn frame(&self) -> Option<Frame> {
        self.imp().frame.get()
    }

    /// The frame on screen (mid-transition, it differs from [`Self::frame`]).
    pub fn presented_frame(&self) -> Option<Frame> {
        self.imp().presented.get()
    }

    pub fn set_frame(&self, frame: Frame) {
        self.set_frame_animated(frame, false);
    }

    /// Sets the frame; with `animated` (an ANIMATED batch) it moves there with
    /// the node's declared frame transition, if any. A node that was never
    /// placed or is off screen jumps, as do all moves under Reduce Motion.
    pub fn set_frame_animated(&self, frame: Frame, animated: bool) {
        let imp = self.imp();
        let old = imp.presented.get();
        imp.frame.set(Some(frame));
        let anim = imp.animation.get().and_then(|a| a.frame);
        match (old, anim) {
            (Some(from), Some(anim))
                if animated && from != frame && self.is_mapped() && !animator::reduce_motion() =>
            {
                imp.frame_anim.replace(Some(Running {
                    from,
                    to: frame,
                    anim,
                    start: None,
                }));
                self.ensure_ticking();
            }
            _ => {
                imp.frame_anim.replace(None);
                imp.presented.set(Some(frame));
            }
        }
        self.queue_place();
    }

    fn queue_place(&self) {
        if let Some(parent) = self.parent() {
            parent.queue_allocate();
        }
        self.queue_allocate();
    }

    /// The model opacity (`Prop::Opacity`), faded in ANIMATED batches when declared.
    pub fn set_opacity_animated(&self, opacity: f64, animated: bool) {
        let imp = self.imp();
        let opacity = if opacity.is_finite() {
            opacity.clamp(0.0, 1.0)
        } else {
            1.0
        };
        imp.opacity.set(opacity);
        let from = imp.shown_opacity.get();
        match imp.animation.get().and_then(|a| a.opacity) {
            // Fades explain a change rather than move anything: they run even with Reduce Motion.
            Some(anim) if animated && from != opacity && self.is_mapped() => {
                imp.opacity_anim.replace(Some(Running {
                    from,
                    to: opacity,
                    anim,
                    start: None,
                }));
                self.ensure_ticking();
            }
            _ => {
                imp.opacity_anim.replace(None);
                imp.shown_opacity.set(opacity);
                self.apply_opacity();
            }
        }
    }

    pub fn set_animation(&self, spec: Option<AnimationSpec>) {
        self.imp().animation.set(spec);
    }

    pub fn is_animating(&self) -> bool {
        self.imp().frame_anim.borrow().is_some() || self.imp().opacity_anim.borrow().is_some()
    }

    /// Jumps every running transition to its end.
    pub fn finish_animations(&self) {
        let imp = self.imp();
        if let Some(r) = imp.frame_anim.take() {
            imp.presented.set(Some(r.to));
            self.queue_place();
        }
        if let Some(r) = imp.opacity_anim.take() {
            imp.shown_opacity.set(r.to);
            self.apply_opacity();
        }
        if let Some(id) = imp.tick.take() {
            id.remove();
        }
    }

    fn ensure_ticking(&self) {
        let imp = self.imp();
        if imp.tick.borrow().is_some() {
            return;
        }
        let id = self.add_tick_callback(|view, clock| {
            if view.step_animations(clock.frame_time()) {
                glib::ControlFlow::Continue
            } else {
                view.imp().tick.take();
                glib::ControlFlow::Break
            }
        });
        imp.tick.replace(Some(id));
    }

    /// Advances running transitions to frame time `now` (µs); false when all
    /// are done. The frame clock drives this; tests and offscreen renders,
    /// which have no running clock, call it directly.
    pub fn step_animations(&self, now: i64) -> bool {
        let imp = self.imp();
        let advance = |start: &mut Option<i64>, anim: &Anim| {
            let start = *start.get_or_insert(now);
            let elapsed = (now - start) as f64 / 1e6;
            (anim.progress(elapsed), elapsed >= anim.total())
        };
        let mut more = false;
        let frame_step = imp.frame_anim.borrow_mut().as_mut().map(|r| {
            let (p, done) = advance(&mut r.start, &r.anim);
            (
                if done {
                    r.to
                } else {
                    animator::lerp_frame(r.from, r.to, p)
                },
                done,
            )
        });
        if let Some((frame, done)) = frame_step {
            imp.presented.set(Some(frame));
            if done {
                imp.frame_anim.replace(None);
            }
            more |= !done;
            self.queue_place();
        }
        let opacity_step = imp.opacity_anim.borrow_mut().as_mut().map(|r| {
            let (p, done) = advance(&mut r.start, &r.anim);
            (
                if done {
                    r.to
                } else {
                    animator::lerp(r.from, r.to, p).clamp(0.0, 1.0)
                },
                done,
            )
        });
        if let Some((opacity, done)) = opacity_step {
            imp.shown_opacity.set(opacity);
            if done {
                imp.opacity_anim.replace(None);
            }
            more |= !done;
            self.apply_opacity();
        }
        more
    }

    fn apply_opacity(&self) {
        let imp = self.imp();
        let press = imp.press.get();
        let factor = match press.active_opacity {
            Some(o) if press.pressed => o as f64,
            _ => 1.0,
        };
        self.set_opacity(imp.shown_opacity.get() * factor);
    }

    pub fn press(&self) -> Press {
        self.imp().press.get()
    }

    pub fn update_press(&self, f: impl FnOnce(&mut Press)) {
        let imp = self.imp();
        let old = imp.press.get();
        let mut press = old;
        f(&mut press);
        if press == old {
            return;
        }
        imp.press.set(press);
        self.apply_opacity();
        self.queue_draw();
        if press.active_opacity != old.active_opacity
            || press.active_background != old.active_background
        {
            crate::gestures::sync(self);
        }
    }

    pub fn set_pressed(&self, pressed: bool) {
        self.update_press(|p| p.pressed = pressed);
    }

    pub fn set_hit_slop(&self, slop: [f32; 4]) {
        self.imp()
            .hit_slop
            .set(slop.map(|v| if v.is_finite() { v.max(0.0) } else { 0.0 }));
    }

    pub fn style(&self) -> BoxStyle {
        self.imp().style.get()
    }

    pub fn update_style(&self, f: impl FnOnce(&mut BoxStyle)) {
        let mut style = self.imp().style.get();
        f(&mut style);
        self.imp().style.set(style);
        // Children outside a clipping node must not take its clicks either.
        self.set_overflow(if style.clips {
            gtk::Overflow::Hidden
        } else {
            gtk::Overflow::Visible
        });
        self.queue_draw();
    }

    pub fn set_gradient(&self, gradient: Option<Gradient>) {
        self.imp().gradient.replace(gradient);
        self.queue_draw();
    }

    pub fn set_transform(&self, transform: Option<gsk::Transform>) {
        self.imp().transform.replace(transform);
        if let Some(parent) = self.parent() {
            parent.queue_allocate();
        }
    }

    /// Sets the native widget that fills this node (replacing any previous one).
    pub fn set_content(&self, content: Option<&gtk::Widget>) {
        if let Some(old) = self.imp().content.take() {
            old.unparent();
        }
        if let Some(widget) = content {
            // Content sits below the child nodes.
            widget.insert_after(self, None::<&gtk::Widget>);
            self.imp().content.replace(Some(widget.clone()));
        }
        self.update_focusable();
        self.queue_allocate();
    }

    pub fn content(&self) -> Option<gtk::Widget> {
        self.imp().content.borrow().clone()
    }

    pub fn set_draw(&self, draw: Option<DrawFn>) {
        self.imp().draw.replace(draw);
        self.queue_draw();
    }

    /// `Prop::Focusable`.
    pub fn set_focusable_node(&self, focusable: bool) {
        self.imp().focusable.set(focusable);
        self.update_focusable();
    }

    pub fn is_focusable_node(&self) -> bool {
        self.imp().focusable.get()
    }

    /// A node is a keyboard focus stop when the script asked (`Focusable`) or
    /// when it is clickable, as every GTK button is. Nodes hosting a native
    /// control leave focus to the control.
    fn update_focusable(&self) {
        use native_shell_core::protocol::event_mask::CLICK;
        let wants = self.content().is_none() && (self.imp().focusable.get() || self.listens(CLICK));
        if self.is_focusable() != wants {
            self.set_focusable(wants);
        }
    }

    /// Enter / Space on the focused node, or an assistive technology's
    /// activation: the click the script listens for, at the center.
    pub fn activate_click(&self) {
        use native_shell_core::protocol::event_mask::CLICK;
        if !self.listens(CLICK) {
            return;
        }
        let (w, h) = self.paint_size();
        let payload = serde_json::json!({
            "x": w / 2.0, "y": h / 2.0, "count": 1,
            "modifiers": crate::keys::modifiers(gdk::ModifierType::empty()),
        });
        self.emit(EventType::Click, payload.to_string());
    }

    /// Inserts `child` as the `index`-th child node (content widgets don't count).
    pub fn insert_node_child(&self, child: &NodeView, index: usize) {
        child.detach();
        let mut previous: Option<gtk::Widget> = self.content();
        let mut seen = 0;
        let mut cursor = self.first_child();
        while let Some(widget) = cursor {
            cursor = widget.next_sibling();
            if widget.is::<NodeView>() {
                if seen == index {
                    break;
                }
                seen += 1;
                previous = Some(widget);
            }
        }
        child.insert_after(self, previous.as_ref());
        self.queue_allocate();
    }

    pub fn detach(&self) {
        if let Some(parent) = self.parent() {
            self.unparent();
            parent.queue_allocate();
        }
    }

    /// The exact size the node paints at: its snapped frame, or the
    /// allocation for nodes placed by someone else (the root, list rows).
    pub fn paint_size(&self) -> (f32, f32) {
        match self.imp().size.get() {
            Some(size) if self.presented_frame().is_some() => size,
            _ => (self.width() as f32, self.height() as f32),
        }
    }

    /// Device pixels per point, fractional scaling included.
    pub fn device_scale(&self) -> f32 {
        let scale = self
            .native()
            .and_then(|n| n.surface())
            .map_or(self.scale_factor() as f64, |s| s.scale());
        if scale.is_finite() && scale > 0.0 {
            scale as f32
        } else {
            1.0
        }
    }

    /// Allocates this node at its (presented) frame, in its parent's coordinates.
    pub fn allocate_at_frame(&self) {
        let Some(f) = self.presented_frame() else {
            self.allocate(0, 0, -1, None);
            return;
        };
        let (x, y, w, h) = snap_frame(f, self.device_scale());
        self.imp().size.set(Some((w, h)));
        let mut t = gsk::Transform::new().translate(&graphene::Point::new(x, y));
        if let Some(extra) = self.imp().transform.borrow().clone() {
            let center = graphene::Point::new(w / 2.0, h / 2.0);
            t = t
                .translate(&center)
                .transform(Some(&extra))
                .translate(&graphene::Point::new(-center.x(), -center.y()));
        }
        self.allocate(alloc_px(w), alloc_px(h), -1, Some(t));
    }

    fn paint(&self, snapshot: &gtk::Snapshot) {
        let imp = self.imp();
        let style = imp.style.get();
        let press = imp.press.get();
        let (w, h) = self.paint_size();
        let bounds = graphene::Rect::new(0.0, 0.0, w, h);
        let radii = clamp_radii(style.radii, w, h);
        let rounded = rounded_rect(bounds, radii);
        let curved = radii.iter().any(|r| *r > 0.0);

        if let Some(s) = style.shadow {
            snapshot.append_outset_shadow(&rounded, &rgba(s.color), s.x, s.y, s.spread, s.blur);
        }
        let background = match press.active_background {
            Some(bg) if press.pressed => Some(bg),
            _ => style.background,
        };
        let gradient = imp.gradient.borrow();
        if background.is_some() || gradient.is_some() {
            if curved {
                snapshot.push_rounded_clip(&rounded);
            }
            if let Some(bg) = background {
                snapshot.append_color(&rgba(bg), &bounds);
            }
            if let Some(g) = gradient.as_ref() {
                append_gradient(snapshot, &bounds, g);
            }
            if curved {
                snapshot.pop();
            }
        }
        drop(gradient);

        let clip = style.clips;
        if clip {
            snapshot.push_rounded_clip(&rounded);
        }
        if let Some(draw) = imp.draw.borrow().as_ref() {
            draw(self, snapshot);
        }
        let mut child = self.first_child();
        while let Some(widget) = child {
            self.snapshot_child(&widget, snapshot);
            child = widget.next_sibling();
        }
        if clip {
            snapshot.pop();
        }

        if let Some(color) = style.border_color {
            let [t, r, b, l] = style.border_widths;
            if t > 0.0 || r > 0.0 || b > 0.0 || l > 0.0 {
                let c = rgba(color);
                snapshot.append_border(&rounded, &[t, r, b, l], &[c, c, c, c]);
            }
        }

        // Keyboard focus, shown only after keyboard navigation as GTK does: a
        // ring in the tint color just outside the shape.
        if self.has_visible_focus() && self.is_focusable() {
            let (gap, width) = (2.0, 2.0);
            let out = gap + width;
            let ring = rounded_rect(
                graphene::Rect::new(-out, -out, w + 2.0 * out, h + 2.0 * out),
                radii.map(|r| if r > 0.0 { r + out } else { 0.0 }),
            );
            let color = self.shared().and_then(|s| {
                let theme = s.theme.borrow();
                theme.color("ring").or_else(|| theme.color("primary"))
            });
            let c = rgba(color.unwrap_or(0xFF00_7AFF));
            snapshot.append_border(&ring, &[width; 4], &[c; 4]);
        }
    }
}

/// A frame snapped to device pixels: both edges round to the pixel grid, so
/// adjacent nodes share an edge exactly and sizes never accumulate drift.
pub fn snap_frame(f: Frame, scale: f32) -> (f32, f32, f32, f32) {
    let finite = |v: f32| if v.is_finite() { v } else { 0.0 };
    let (x, y) = (finite(f.x), finite(f.y));
    let (w, h) = (finite(f.width).max(0.0), finite(f.height).max(0.0));
    let snap = |v: f32| (v * scale).round() / scale;
    let (l, t) = (snap(x), snap(y));
    (l, t, (snap(x + w) - l).max(0.0), (snap(y + h) - t).max(0.0))
}

/// The whole-pixel allocation for an exact size: rounded up, so content and
/// hit testing cover every painted pixel.
pub fn alloc_px(size: f32) -> i32 {
    (size - 0.001).ceil().max(0.0) as i32
}

/// CSS corner clamping: when adjacent radii overflow a side, all radii scale
/// down by the same factor (uniform radii end up at min(r, min(w, h) / 2)).
pub fn clamp_radii(radii: [f32; 4], w: f32, h: f32) -> [f32; 4] {
    let r = radii.map(|v| if v.is_finite() { v.max(0.0) } else { 0.0 });
    let mut f: f32 = 1.0;
    for (side, sum) in [
        (w, r[0] + r[1]),
        (h, r[1] + r[2]),
        (w, r[2] + r[3]),
        (h, r[3] + r[0]),
    ] {
        if sum > 0.0 {
            f = f.min(side.max(0.0) / sum);
        }
    }
    r.map(|v| v * f)
}

pub fn rounded_rect(bounds: graphene::Rect, radii: [f32; 4]) -> gsk::RoundedRect {
    let size = |r: f32| graphene::Size::new(r, r);
    gsk::RoundedRect::new(
        bounds,
        size(radii[0]),
        size(radii[1]),
        size(radii[2]),
        size(radii[3]),
    )
}

/// The CSS gradient line for `angle` across a `w`×`h` box: through the center,
/// long enough that the corners get the first and last stop colors.
pub fn gradient_line(angle: f32, w: f32, h: f32) -> ((f32, f32), (f32, f32)) {
    let a = angle.to_radians();
    let (dx, dy) = (a.sin(), -a.cos());
    let half = (w * dx.abs() + h * dy.abs()) / 2.0;
    let (cx, cy) = (w / 2.0, h / 2.0);
    (
        (cx - dx * half, cy - dy * half),
        (cx + dx * half, cy + dy * half),
    )
}

fn append_gradient(snapshot: &gtk::Snapshot, bounds: &graphene::Rect, g: &Gradient) {
    if let [(color, _)] = g.stops.as_slice() {
        snapshot.append_color(&rgba(*color), bounds);
        return;
    }
    let ((x0, y0), (x1, y1)) = gradient_line(g.angle, bounds.width(), bounds.height());
    let stops: Vec<gsk::ColorStop> = g
        .stops
        .iter()
        .map(|(c, o)| gsk::ColorStop::new(*o, rgba(*c)))
        .collect();
    snapshot.append_linear_gradient(
        bounds,
        &graphene::Point::new(x0, y0),
        &graphene::Point::new(x1, y1),
        &stops,
    );
}

/// Parses `Prop::Gradient` JSON `{angle, stops: [[color, offset]…]}`.
pub fn parse_gradient(v: &serde_json::Value) -> Option<Gradient> {
    let stops = v.get("stops")?.as_array()?;
    let n = stops.len();
    let mut out: Vec<(u32, f32)> = Vec::with_capacity(n);
    for (i, s) in stops.iter().enumerate() {
        let Some(color) = s.get(0).and_then(|c| c.as_u64()) else {
            continue;
        };
        let even = if n > 1 {
            i as f64 / (n - 1) as f64
        } else {
            0.0
        };
        let offset = s
            .get(1)
            .and_then(|o| o.as_f64())
            .filter(|o| o.is_finite())
            .unwrap_or(even)
            .clamp(0.0, 1.0) as f32;
        // GSK needs non-decreasing offsets; CSS clamps a smaller one up to the previous.
        let offset = out.last().map_or(offset, |(_, prev)| offset.max(*prev));
        out.push((color as u32, offset));
    }
    if out.is_empty() {
        return None;
    }
    let angle = v
        .get("angle")
        .and_then(|a| a.as_f64())
        .filter(|a| a.is_finite())
        .unwrap_or(180.0) as f32;
    Some(Gradient { angle, stops: out })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frames_snap_edges_without_drift() {
        // Three thirds of 100 pt tile exactly at 1x.
        let third = 100.0 / 3.0;
        let cells: Vec<_> = (0..3)
            .map(|i| {
                snap_frame(
                    Frame {
                        x: i as f32 * third,
                        y: 0.0,
                        width: third,
                        height: 10.0,
                    },
                    1.0,
                )
            })
            .collect();
        assert_eq!(cells[0].0 + cells[0].2, cells[1].0);
        assert_eq!(cells[1].0 + cells[1].2, cells[2].0);
        assert_eq!(cells[2].0 + cells[2].2, 100.0);
        // At 2x, half points survive.
        assert_eq!(
            snap_frame(
                Frame {
                    x: 0.5,
                    y: 0.25,
                    width: 10.5,
                    height: 1.0
                },
                2.0
            ),
            (0.5, 0.5, 10.5, 1.0)
        );
        // Hostile values never produce NaN or negative sizes.
        let (x, _, w, _) = snap_frame(
            Frame {
                x: f32::NAN,
                y: 0.0,
                width: -4.0,
                height: f32::INFINITY,
            },
            1.0,
        );
        assert_eq!((x, w), (0.0, 0.0));
        assert_eq!(alloc_px(10.0), 10);
        assert_eq!(alloc_px(10.5), 11);
        assert_eq!(alloc_px(0.0), 0);
    }

    #[test]
    fn radii_clamp_like_css() {
        assert_eq!(clamp_radii([100.0; 4], 40.0, 20.0), [10.0; 4]);
        assert_eq!(
            clamp_radii([8.0, 8.0, 0.0, 0.0], 100.0, 50.0),
            [8.0, 8.0, 0.0, 0.0]
        );
        assert_eq!(
            clamp_radii([-1.0, f32::NAN, 4.0, 4.0], 100.0, 100.0),
            [0.0, 0.0, 4.0, 4.0]
        );
    }

    #[test]
    fn gradient_angles_follow_css() {
        let close =
            |a: (f32, f32), b: (f32, f32)| (a.0 - b.0).abs() < 1e-3 && (a.1 - b.1).abs() < 1e-3;
        // 180deg: top to bottom.
        let (s, e) = gradient_line(180.0, 100.0, 50.0);
        assert!(close(s, (50.0, 0.0)) && close(e, (50.0, 50.0)));
        // 90deg: left to right.
        let (s, e) = gradient_line(90.0, 100.0, 50.0);
        assert!(close(s, (0.0, 25.0)) && close(e, (100.0, 25.0)));
        // 45deg on a square: bottom-left corner to top-right corner.
        let (s, e) = gradient_line(45.0, 10.0, 10.0);
        assert!(close(s, (0.0, 10.0)) && close(e, (10.0, 0.0)));
    }

    #[test]
    fn gradients_parse_leniently() {
        let g = parse_gradient(&serde_json::json!({"angle": 90, "stops": [[4294901760u32, 0.0], [4278190335u32, 1.0]]}))
            .unwrap();
        assert_eq!(
            g,
            Gradient {
                angle: 90.0,
                stops: vec![(0xFFFF_0000, 0.0), (0xFF00_00FF, 1.0)]
            }
        );
        // Missing offsets spread evenly, decreasing ones clamp, bad stops are skipped.
        let g = parse_gradient(&serde_json::json!({"stops": [[1], ["x"], [2, 0.8], [3, 0.2]]}))
            .unwrap();
        assert_eq!(g.angle, 180.0);
        assert_eq!(g.stops, vec![(1, 0.0), (2, 0.8), (3, 0.8)]);
        assert_eq!(parse_gradient(&serde_json::json!({"stops": []})), None);
    }
}
