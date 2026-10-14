//! `List` / `ListRow`: a virtualized vertical list.
//!
//! The core renders only a window of rows, each a `ListRow` node carrying its
//! `RowIndex`, and sends `RowOffsets` for every row (measured or estimated).
//! The list places each rendered row at its offset inside a content widget as
//! tall as all rows, draws a placeholder for visible rows not rendered yet, and
//! reports the visible range widened by `Prefetch` with `RangeChange`, which
//! moves the core's window. Row views are the core's nodes: when the window
//! moves the core recycles them by key (a row keeps its view and gets a new
//! `RowIndex`), so the renderer never builds a view per item.
//!
//! A bottom-aligned list (`ListAlignment` 1, a chat thread) opens at its last
//! row and, while the user is at the end, keeps the end in view as rows
//! arrive, row heights get measured or the viewport shrinks. Whether the user
//! is at the end is decided after every settled scroll, before the next batch
//! changes the geometry; scrolled away, the position is kept.

use std::cell::{Cell, RefCell};
use std::rc::{Rc, Weak};

use gtk::prelude::*;
use gtk::subclass::prelude::*;
use gtk::{glib, graphene, gsk};

use native_shell_core::protocol::{Command, EventType, Frame, Prop, PropValue};

use crate::component::{Component, NodeCx, Shared};
use crate::components::scroll::{bool_prop, Scroller};
use crate::mount::{float, json};
use crate::node_view::{rgba, rounded_rect, NodeView};

/// Row height assumed past the end of `RowOffsets` (iOS's default row height).
const FALLBACK_ROW: f64 = 44.0;
/// Rows requested beyond the visible ones when the core sends no `Prefetch`.
const DEFAULT_PREFETCH: usize = 10;
/// How close to the end still counts as at the end.
const END_SLOP: f64 = 4.0;
/// Placeholder bars when the theme has no `muted` color (iOS's system fill).
const PLACEHOLDER: u32 = 0x3378_7880;
const ROW_INDEX_KEY: &str = "nsh-row-index";

/// Row geometry from `RowOffsets`: `offsets[i]` is row `i`'s top and the
/// element after the last row is the total height.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Rows<'a> {
    pub offsets: &'a [f32],
    pub count: usize,
}

impl Rows<'_> {
    /// Top of row `i`; rows missing from the offsets continue at the fallback height.
    pub fn top(&self, i: usize) -> f64 {
        match self.offsets.get(i) {
            Some(y) => *y as f64,
            None => match self.offsets.last() {
                Some(last) => *last as f64 + (i + 1 - self.offsets.len()) as f64 * FALLBACK_ROW,
                None => i as f64 * FALLBACK_ROW,
            },
        }
    }

    pub fn height(&self, i: usize) -> f64 {
        (self.top(i + 1) - self.top(i)).max(0.0)
    }

    pub fn total(&self) -> f64 {
        self.top(self.count)
    }

    /// The row containing `y` (the last row whose top is at or above it).
    pub fn row_at(&self, y: f64) -> usize {
        if self.count == 0 {
            return 0;
        }
        let (mut lo, mut hi) = (0, self.count - 1);
        while lo < hi {
            let mid = (lo + hi).div_ceil(2);
            if self.top(mid) <= y {
                lo = mid;
            } else {
                hi = mid - 1;
            }
        }
        lo
    }

    /// Rows `[first, end)` intersecting the viewport, widened by `prefetch` on both sides.
    pub fn range(&self, top: f64, height: f64, prefetch: usize) -> (usize, usize) {
        if self.count == 0 {
            return (0, 0);
        }
        let first = self.row_at(top.max(0.0));
        let end = (self.row_at(top + height.max(1.0) - 0.5) + 1).min(self.count);
        (first.saturating_sub(prefetch), (end + prefetch).min(self.count))
    }

    /// The offset that shows the last row at the bottom of `viewport`.
    pub fn end_offset(&self, viewport: f64) -> f64 {
        (self.total() - viewport).max(0.0)
    }

    /// Whether a viewport at `top` shows the end (a list shorter than it always does).
    pub fn at_end(&self, top: f64, viewport: f64) -> bool {
        top >= self.end_offset(viewport) - END_SLOP
    }

    /// The offset that shows row `index` at `position` ("top", "center" or "bottom").
    pub fn offset_for(&self, index: usize, position: &str, viewport: f64) -> f64 {
        if self.count == 0 {
            return 0.0;
        }
        let i = index.min(self.count - 1);
        let (top, h) = (self.top(i), self.height(i));
        let y = match position {
            "center" => top - (viewport - h) / 2.0,
            "bottom" => top + h - viewport,
            _ => top,
        };
        y.clamp(0.0, (self.total() - viewport).max(0.0))
    }
}

fn row_index(view: &NodeView) -> Option<usize> {
    // SAFETY: only `set_row_index` writes this key, always with an `i32`.
    let i = unsafe { view.data::<i32>(ROW_INDEX_KEY).map(|p| *p.as_ref()) }?;
    usize::try_from(i).ok()
}

fn set_row_index(view: &NodeView, index: i32) {
    // SAFETY: the key is private to this module and always holds an `i32`.
    unsafe { view.set_data(ROW_INDEX_KEY, index) };
}

mod imp {
    use super::*;

    #[derive(Default)]
    pub struct ListContent {
        pub offsets: RefCell<Vec<f32>>,
        pub count: Cell<usize>,
        pub prefetch: Cell<Option<usize>>,
        /// Height of the list node, for ranges computed before GTK lays it out.
        pub viewport: Cell<f64>,
        pub placeholder: Cell<u32>,
        pub reported: Cell<Option<(usize, usize)>>,
        /// `ListAlignment` 1.
        pub bottom: Cell<bool>,
        /// The user was at the end when the list last settled; a new list starts there.
        pub pinned: Cell<bool>,
        pub vadjustment: RefCell<Option<gtk::Adjustment>>,
        /// Schedules a range report.
        pub schedule: RefCell<Option<Box<dyn Fn()>>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for ListContent {
        const NAME: &'static str = "NshListContent";
        type Type = super::ListContent;
        type ParentType = gtk::Widget;
    }

    impl ObjectImpl for ListContent {
        fn dispose(&self) {
            while let Some(child) = self.obj().first_child() {
                child.unparent();
            }
        }
    }

    impl WidgetImpl for ListContent {
        fn measure(&self, orientation: gtk::Orientation, _for_size: i32) -> (i32, i32, i32, i32) {
            if orientation == gtk::Orientation::Vertical {
                let h = self.obj().rows_total().ceil() as i32;
                (h, h, -1, -1)
            } else {
                (0, 0, -1, -1)
            }
        }

        fn size_allocate(&self, width: i32, _height: i32, _baseline: i32) {
            let obj = self.obj();
            let offsets = self.offsets.borrow();
            let rows = Rows { offsets: &offsets, count: self.count.get() };
            let mut child = obj.first_child();
            while let Some(widget) = child {
                child = widget.next_sibling();
                let Some(view) = widget.downcast_ref::<NodeView>() else { continue };
                let Some(i) = row_index(view).filter(|i| *i < rows.count) else {
                    view.allocate(0, 0, -1, None);
                    continue;
                };
                // The row's own height once laid out; its offset slot until then.
                let h = view.frame().map(|f| f.height as f64).filter(|h| *h > 0.0).unwrap_or(rows.height(i));
                let t = gsk::Transform::new().translate(&graphene::Point::new(0.0, rows.top(i) as f32));
                view.allocate(width, h.round() as i32, -1, Some(t));
            }
            drop(offsets);
            if let Some(schedule) = self.schedule.borrow().as_ref() {
                schedule();
            }
        }

        fn snapshot(&self, snapshot: &gtk::Snapshot) {
            let obj = self.obj();
            obj.snapshot_placeholders(snapshot);
            let mut child = obj.first_child();
            while let Some(widget) = child {
                obj.snapshot_child(&widget, snapshot);
                child = widget.next_sibling();
            }
        }
    }
}

glib::wrapper! {
    /// The scrolled content of a list: as tall as all its rows.
    pub struct ListContent(ObjectSubclass<imp::ListContent>)
        @extends gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl ListContent {
    fn new() -> ListContent {
        let content: ListContent = glib::Object::new();
        content.imp().pinned.set(true);
        content
    }

    /// Remembers whether the user is at the end, once a scroll or layout settled.
    fn update_pin(&self) {
        let imp = self.imp();
        if !imp.bottom.get() {
            return;
        }
        let (top, height) = self.viewport();
        if height > 0.0 {
            let offsets = imp.offsets.borrow();
            imp.pinned.set(Rows { offsets: &offsets, count: imp.count.get() }.at_end(top, height));
        }
    }

    fn rows_total(&self) -> f64 {
        let offsets = self.imp().offsets.borrow();
        Rows { offsets: &offsets, count: self.imp().count.get() }.total()
    }

    fn viewport(&self) -> (f64, f64) {
        let imp = self.imp();
        let (top, page) = imp.vadjustment.borrow().as_ref().map_or((0.0, 0.0), |a| (a.value(), a.page_size()));
        (top, if page > 0.0 { page } else { imp.viewport.get() })
    }

    /// The visible range plus prefetch, or `None` before the list has a size or rows.
    pub fn range(&self) -> Option<(usize, usize)> {
        let imp = self.imp();
        let (top, height) = self.viewport();
        let count = imp.count.get();
        if count == 0 || height <= 0.0 {
            return None;
        }
        let offsets = imp.offsets.borrow();
        let prefetch = imp.prefetch.get().unwrap_or(DEFAULT_PREFETCH);
        Some(Rows { offsets: &offsets, count }.range(top, height, prefetch))
    }

    /// Quiet skeleton bars where visible rows have no view yet, so fast
    /// scrolling shows the shape of the list instead of a blank gap.
    fn snapshot_placeholders(&self, snapshot: &gtk::Snapshot) {
        let imp = self.imp();
        let count = imp.count.get();
        let (top, height) = self.viewport();
        if count == 0 || height <= 0.0 {
            return;
        }
        let offsets = imp.offsets.borrow();
        let rows = Rows { offsets: &offsets, count };
        let (first, end) = rows.range(top, height, 0);
        let mut rendered = vec![false; end - first];
        let mut child = self.first_child();
        while let Some(widget) = child {
            child = widget.next_sibling();
            if let Some(i) = widget.downcast_ref::<NodeView>().and_then(row_index) {
                if (first..end).contains(&i) {
                    rendered[i - first] = true;
                }
            }
        }
        let color = rgba(imp.placeholder.get());
        let width = self.width() as f32;
        for (i, _) in (first..end).zip(&rendered).filter(|(_, r)| !**r) {
            let (y, h) = (rows.top(i) as f32, rows.height(i) as f32);
            if h < 12.0 {
                continue;
            }
            let bar = graphene::Rect::new(16.0, y + (h - 12.0) / 2.0, (width * 0.45).min(220.0), 12.0);
            snapshot.push_rounded_clip(&rounded_rect(bar, [6.0; 4]));
            snapshot.append_color(&color, &bar);
            snapshot.pop();
        }
    }

    fn rows_changed(&self) {
        self.queue_resize();
        self.queue_draw();
    }
}

pub struct List {
    scroller: Scroller,
    content: ListContent,
}

impl List {
    pub fn new(cx: &NodeCx) -> List {
        let scroller = Scroller::new(cx);
        let content = ListContent::new();
        scroller.window.set_child(Some(&content));
        scroller.window.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
        content.imp().vadjustment.replace(Some(scroller.window.vadjustment()));
        content.imp().placeholder.set(PLACEHOLDER);
        cx.view.set_content(Some(scroller.window.upcast_ref()));

        let node = cx.node;
        let weak_shared: Weak<Shared> = Rc::downgrade(cx.shared);
        let weak_content = content.downgrade();
        scroller.set_on_frame(move || {
            let Some(content) = weak_content.upgrade() else { return };
            content.update_pin();
            let Some(range) = content.range() else { return };
            if content.imp().reported.replace(Some(range)) == Some(range) {
                return;
            }
            if let Some(shared) = weak_shared.upgrade() {
                let payload = serde_json::json!({ "first": range.0, "end": range.1 }).to_string();
                (shared.events)(node, EventType::RangeChange, payload);
            }
        });
        let schedule = scroller.scheduler();
        content.imp().schedule.replace(Some(Box::new(schedule)));
        List { scroller, content }
    }

    /// Scrolls a bottom-aligned list to its end if the user was there.
    fn keep_end_in_view(&self, cx: &NodeCx) {
        let imp = self.content.imp();
        let viewport = imp.viewport.get();
        if !imp.bottom.get() || !imp.pinned.get() || imp.count.get() == 0 || viewport <= 0.0 {
            return;
        }
        let (offsets, count) = self.rows();
        let rows = Rows { offsets: &offsets, count };
        let (_, top) = self.scroller.offset();
        let end = rows.end_offset(viewport);
        if (top - end).abs() > 0.5 {
            let width = cx.view.frame().map_or(0.0, |f| f.width as f64);
            self.scroller.scroll_to(0.0, end, (width, rows.total()), (width, viewport), false);
        }
    }

    fn rows(&self) -> (Vec<f32>, usize) {
        let imp = self.content.imp();
        (imp.offsets.borrow().clone(), imp.count.get())
    }
}

impl Component for List {
    fn set_prop(&mut self, _cx: &NodeCx, prop: Prop, value: &PropValue) -> bool {
        let imp = self.content.imp();
        match prop {
            Prop::ItemCount => imp.count.set(float(value).unwrap_or(0.0).max(0.0) as usize),
            Prop::RowOffsets => {
                let offsets = match value {
                    PropValue::F32Array(a) => a.clone(),
                    _ => Vec::new(),
                };
                imp.offsets.replace(offsets);
            }
            Prop::Prefetch => imp.prefetch.set(float(value).map(|p| p.max(0.0) as usize)),
            Prop::ListAlignment => imp.bottom.set(float(value) == Some(1.0)),
            Prop::ShowsIndicators => {
                let p = if bool_prop(value, true) { gtk::PolicyType::Automatic } else { gtk::PolicyType::External };
                self.scroller.window.set_policy(gtk::PolicyType::Never, p);
            }
            _ => return false,
        }
        true
    }

    fn set_frame(&mut self, cx: &NodeCx, frame: Frame) {
        self.content.imp().viewport.set(frame.height as f64);
        // A shorter viewport (a panel opening below) keeps a pinned end in view.
        self.keep_end_in_view(cx);
        self.scroller.schedule();
    }

    fn set_event_mask(&mut self, _cx: &NodeCx, mask: u32) {
        self.scroller.set_event_mask(mask);
    }

    fn batch_applied(&mut self, cx: &NodeCx) {
        let muted = cx.shared.theme.borrow().color("muted");
        self.content.imp().placeholder.set(muted.unwrap_or(PLACEHOLDER));
        self.content.rows_changed();
        self.keep_end_in_view(cx);
        self.scroller.schedule();
    }

    fn command(&mut self, cx: &NodeCx, command: Command, payload: &str) {
        let p = json(&PropValue::Json(payload.to_string())).unwrap_or_default();
        let animated = p.get("animated").and_then(|v| v.as_bool()).unwrap_or(true);
        let (offsets, count) = self.rows();
        let rows = Rows { offsets: &offsets, count };
        let width = cx.view.frame().map_or(0.0, |f| f.width as f64);
        let viewport = self.content.imp().viewport.get();
        let y = match command {
            Command::ScrollToIndex => {
                let Some(index) = p.get("index").and_then(|v| v.as_u64()) else { return };
                let position = p.get("position").and_then(|v| v.as_str()).unwrap_or("top");
                rows.offset_for(index as usize, position, viewport)
            }
            Command::ScrollTo => p.get("y").and_then(|v| v.as_f64()).unwrap_or(0.0),
            _ => return,
        };
        self.scroller.scroll_to(0.0, y, (width, rows.total()), (width, viewport), animated);
    }

    fn insert_child(&mut self, _cx: &NodeCx, child: &NodeView, _index: usize) {
        // Rows are positioned by index, so their order among siblings does not matter.
        child.detach();
        child.set_parent(&self.content);
        self.content.rows_changed();
    }

    fn remove_child(&mut self, _cx: &NodeCx, child: &NodeView) {
        child.detach();
        self.content.rows_changed();
    }

    fn unmount(&mut self, _cx: &NodeCx) {
        self.scroller.unmount();
        self.content.imp().schedule.replace(None);
    }
}

pub struct ListRow;

impl Component for ListRow {
    fn set_prop(&mut self, cx: &NodeCx, prop: Prop, value: &PropValue) -> bool {
        if prop != Prop::RowIndex {
            return false;
        }
        set_row_index(cx.view, float(value).map_or(-1, |i| i as i32));
        if let Some(content) = cx.view.parent().and_downcast::<ListContent>() {
            content.rows_changed();
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const OFFSETS: [f32; 6] = [0.0, 44.0, 88.0, 152.0, 196.0, 240.0];

    fn rows(count: usize) -> Rows<'static> {
        Rows { offsets: &OFFSETS, count }
    }

    #[test]
    fn tops_extrapolate_past_the_offsets() {
        let r = rows(5);
        assert_eq!(r.top(3), 152.0);
        assert_eq!(r.total(), 240.0);
        assert_eq!(r.top(7), 240.0 + 2.0 * FALLBACK_ROW);
        assert_eq!(Rows { offsets: &[], count: 3 }.total(), 3.0 * FALLBACK_ROW);
    }

    #[test]
    fn row_at_finds_the_containing_row() {
        let r = rows(5);
        assert_eq!(r.row_at(0.0), 0);
        assert_eq!(r.row_at(43.9), 0);
        assert_eq!(r.row_at(44.0), 1);
        assert_eq!(r.row_at(100.0), 2);
        assert_eq!(r.row_at(10_000.0), 4);
        assert_eq!(rows(0).row_at(5.0), 0);
    }

    #[test]
    fn range_covers_the_viewport_plus_prefetch() {
        let r = rows(5);
        assert_eq!(r.range(0.0, 88.0, 0), (0, 2));
        assert_eq!(r.range(50.0, 100.0, 0), (1, 3));
        assert_eq!(r.range(50.0, 100.0, 1), (0, 4));
        assert_eq!(r.range(0.0, 1000.0, 10), (0, 5));
        assert_eq!(rows(0).range(0.0, 100.0, 3), (0, 0));
        // A list taller than its offsets still reaches its last rows.
        let long = Rows { offsets: &OFFSETS, count: 1000 };
        assert_eq!(long.range(240.0 + 44.0 * 100.0, 440.0, 2), (103, 117));
    }

    #[test]
    fn at_end_allows_a_few_points() {
        let r = rows(5);
        assert_eq!(r.end_offset(100.0), 140.0);
        assert!(r.at_end(140.0, 100.0) && r.at_end(137.0, 100.0));
        assert!(!r.at_end(130.0, 100.0));
        // Shorter than the viewport: always at the end.
        assert_eq!(r.end_offset(500.0), 0.0);
        assert!(r.at_end(0.0, 500.0));
    }

    #[test]
    fn offset_for_positions_and_clamps() {
        let r = rows(5);
        assert_eq!(r.offset_for(2, "top", 100.0), 88.0);
        assert_eq!(r.offset_for(2, "center", 100.0), 88.0 - (100.0 - 64.0) / 2.0);
        assert_eq!(r.offset_for(2, "bottom", 100.0), 52.0);
        // Never past the end, never above the start, index clamped to the last row.
        assert_eq!(r.offset_for(4, "top", 100.0), 140.0);
        assert_eq!(r.offset_for(0, "bottom", 100.0), 0.0);
        assert_eq!(r.offset_for(99, "bottom", 100.0), 140.0);
        assert_eq!(rows(0).offset_for(3, "top", 100.0), 0.0);
    }
}
