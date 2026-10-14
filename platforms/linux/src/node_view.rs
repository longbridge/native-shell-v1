//! The base widget of every node, the counterpart of a `UIView` positioned by
//! the core.
//!
//! Layout is done by taffy in the core, so a node never asks its children for
//! sizes: it allocates each child node at the frame the batch gave it, and its
//! optional native `content` widget (an entry, a switch…) at its own bounds.
//! It paints the box decorations itself so radii, borders and shadows match
//! the other renderers exactly instead of going through CSS.

use std::cell::{Cell, RefCell};

use gtk::prelude::*;
use gtk::subclass::prelude::*;
use gtk::{gdk, glib, graphene, gsk};

use native_shell_core::protocol::Frame;

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

/// `0xAARRGGBB` to a GDK color.
pub fn rgba(color: u32) -> gdk::RGBA {
    let c = |shift: u32| ((color >> shift) & 0xff) as f32 / 255.0;
    gdk::RGBA::new(c(16), c(8), c(0), c(24))
}

mod imp {
    use super::*;

    #[derive(Default)]
    pub struct NodeView {
        pub frame: Cell<Option<Frame>>,
        pub style: Cell<BoxStyle>,
        /// Extra transform applied around the frame's center (`Prop::Transform`).
        pub transform: RefCell<Option<gsk::Transform>>,
        pub content: RefCell<Option<gtk::Widget>>,
        pub draw: RefCell<Option<DrawFn>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for NodeView {
        const NAME: &'static str = "NshNodeView";
        type Type = super::NodeView;
        type ParentType = gtk::Widget;
    }

    impl ObjectImpl for NodeView {
        fn dispose(&self) {
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

        fn snapshot(&self, snapshot: &gtk::Snapshot) {
            let obj = self.obj();
            let style = self.style.get();
            let (w, h) = (obj.width() as f32, obj.height() as f32);
            let bounds = graphene::Rect::new(0.0, 0.0, w, h);
            let rounded = rounded_rect(bounds, style.radii);

            if let Some(s) = style.shadow {
                snapshot.append_outset_shadow(&rounded, &rgba(s.color), s.x, s.y, s.spread, s.blur);
            }
            if let Some(bg) = style.background {
                if style.radii.iter().any(|r| *r > 0.0) {
                    snapshot.push_rounded_clip(&rounded);
                    snapshot.append_color(&rgba(bg), &bounds);
                    snapshot.pop();
                } else {
                    snapshot.append_color(&rgba(bg), &bounds);
                }
            }

            let clip = style.clips;
            if clip {
                snapshot.push_rounded_clip(&rounded);
            }
            if let Some(draw) = self.draw.borrow().as_ref() {
                draw(&obj, snapshot);
            }
            let mut child = obj.first_child();
            while let Some(widget) = child {
                obj.snapshot_child(&widget, snapshot);
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

    pub fn frame(&self) -> Option<Frame> {
        self.imp().frame.get()
    }

    pub fn set_frame(&self, frame: Frame) {
        self.imp().frame.set(Some(frame));
        if let Some(parent) = self.parent() {
            parent.queue_allocate();
        }
        self.queue_allocate();
    }

    pub fn style(&self) -> BoxStyle {
        self.imp().style.get()
    }

    pub fn update_style(&self, f: impl FnOnce(&mut BoxStyle)) {
        let mut style = self.imp().style.get();
        f(&mut style);
        self.imp().style.set(style);
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
        self.queue_allocate();
    }

    pub fn content(&self) -> Option<gtk::Widget> {
        self.imp().content.borrow().clone()
    }

    pub fn set_draw(&self, draw: Option<DrawFn>) {
        self.imp().draw.replace(draw);
        self.queue_draw();
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

    fn allocate_at_frame(&self) {
        let Some(f) = self.frame() else {
            self.allocate(0, 0, -1, None);
            return;
        };
        let mut t = gsk::Transform::new().translate(&graphene::Point::new(f.x, f.y));
        if let Some(extra) = self.imp().transform.borrow().clone() {
            let center = graphene::Point::new(f.width / 2.0, f.height / 2.0);
            t = t
                .translate(&center)
                .transform(Some(&extra))
                .translate(&graphene::Point::new(-center.x(), -center.y()));
        }
        self.allocate(f.width.round() as i32, f.height.round() as i32, -1, Some(t));
    }
}

pub fn rounded_rect(bounds: graphene::Rect, radii: [f32; 4]) -> gsk::RoundedRect {
    let size = |r: f32| graphene::Size::new(r, r);
    gsk::RoundedRect::new(bounds, size(radii[0]), size(radii[1]), size(radii[2]), size(radii[3]))
}
