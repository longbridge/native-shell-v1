//! `Paging` on a horizontal `Scroll` (UIScrollView's `isPagingEnabled`): the
//! scroll comes to rest on whole pages, a page being the viewport's width.
//!
//! GTK has no paging, so the scroll view takes over horizontal input while it
//! pages. A touchpad gesture moves the content with the fingers and, when it
//! ends, settles on the nearest page or, after a fling, on the next page in
//! the fling's direction (never further). A sideways wheel notch (or Shift
//! with the wheel) steps one page. On a
//! touch screen GTK's own drag moves the content without momentum and the
//! release settles the same way. Settling uses the scroller's spring, so
//! scroll events report every step as usual.

use std::cell::Cell;
use std::rc::{Rc, Weak};

use gtk::prelude::*;
use gtk::glib;

use crate::components::scroll::Scroller;

/// Release speed (points per second) that counts as a fling.
const FLING: f64 = 300.0;

/// Pages the content holds: whole viewports, the last one possibly partial.
pub(crate) fn page_count(content: f64, page: f64) -> usize {
    if page <= 0.0 || content <= 0.0 {
        return 1;
    }
    // A sliver from rounding is not a page.
    ((content - 0.5) / page).ceil().max(1.0) as usize
}

/// The page to rest on after a gesture released at `offset` with `velocity`
/// (positive moves toward later pages): the nearest one, or after a fling the
/// next page boundary in its direction.
pub(crate) fn rest_page(offset: f64, page: f64, pages: usize, velocity: f64) -> usize {
    if page <= 0.0 || pages == 0 {
        return 0;
    }
    let last = (pages - 1) as f64;
    let at = offset / page;
    let target = if velocity >= FLING {
        at.floor() + 1.0
    } else if velocity <= -FLING {
        at.ceil() - 1.0
    } else {
        at.round()
    };
    target.clamp(0.0, last) as usize
}

/// The offset showing `page`; the last page ends at the end of the content.
pub(crate) fn page_offset(page_index: usize, page: f64, content: f64) -> f64 {
    (page_index as f64 * page).min((content - page).max(0.0)).max(0.0)
}

pub(crate) struct Paging {
    scroller: Scroller,
    pub enabled: Cell<bool>,
    pub content: Cell<(f64, f64)>,
    pub viewport: Cell<(f64, f64)>,
    /// A touchpad gesture or touch drag is moving the content.
    dragging: Cell<bool>,
    velocity: Cell<f64>,
    settle_pending: Cell<bool>,
    /// Where the last settle or wheel step is going, so quick wheel notches add up.
    target: Cell<Option<usize>>,
    wheel: Cell<f64>,
}

impl Paging {
    pub fn new(scroller: Scroller) -> Rc<Paging> {
        let paging = Rc::new(Paging {
            scroller,
            enabled: Cell::new(false),
            content: Cell::new((0.0, 0.0)),
            viewport: Cell::new((0.0, 0.0)),
            dragging: Cell::new(false),
            velocity: Cell::new(0.0),
            settle_pending: Cell::new(false),
            target: Cell::new(None),
            wheel: Cell::new(0.0),
        });
        let window = paging.scroller.window.clone();
        window.add_controller(paging.touchpad_and_wheel());
        let (drag, swipe) = paging.touch();
        window.add_controller(drag);
        window.add_controller(swipe);
        paging
    }

    pub fn set_enabled(&self, on: bool) {
        self.enabled.set(on);
        // Momentum would carry the content past the page; settling replaces it.
        self.scroller.window.set_kinetic_scrolling(!on);
        if on {
            self.settle(false);
        }
    }

    fn page(&self) -> f64 {
        self.viewport.get().0
    }

    fn pages(&self) -> usize {
        page_count(self.content.get().0, self.page())
    }

    fn current_page(&self) -> usize {
        rest_page(self.scroller.offset().0, self.page(), self.pages(), 0.0)
    }

    fn go_to(&self, page_index: usize, animated: bool) {
        let page = self.page();
        if page <= 0.0 {
            return;
        }
        let (content, viewport) = (self.content.get(), self.viewport.get());
        self.target.set(Some(page_index));
        let x = page_offset(page_index, page, content.0);
        self.scroller.scroll_to(x, self.scroller.offset().1, content, viewport, animated);
    }

    /// Rests on the page the release velocity picks.
    fn settle(&self, animated: bool) {
        let page = rest_page(self.scroller.offset().0, self.page(), self.pages(), self.velocity.replace(0.0));
        self.go_to(page, animated);
    }

    /// Settles once the current event is handled: GTK reports a touchpad's
    /// fling velocity right after the gesture's end.
    fn settle_soon(self: &Rc<Self>) {
        if self.settle_pending.replace(true) {
            return;
        }
        let weak = Rc::downgrade(self);
        glib::idle_add_local_once(move || {
            if let Some(p) = weak.upgrade() {
                p.settle_pending.set(false);
                if p.enabled.get() && !p.dragging.get() {
                    p.settle(true);
                }
            }
        });
    }

    /// The new width keeps the same page in view.
    pub fn viewport_changed(&self, old_width: f64) {
        if !self.enabled.get() || old_width <= 0.0 || old_width == self.page() {
            return;
        }
        let at = rest_page(self.scroller.offset().0, old_width, page_count(self.content.get().0, old_width), 0.0);
        self.go_to(at, false);
    }

    fn touchpad_and_wheel(self: &Rc<Self>) -> gtk::EventControllerScroll {
        let flags = gtk::EventControllerScrollFlags::BOTH_AXES | gtk::EventControllerScrollFlags::KINETIC;
        let c = gtk::EventControllerScroll::new(flags);
        c.set_propagation_phase(gtk::PropagationPhase::Capture);
        let weak = Rc::downgrade(self);
        c.connect_scroll_begin(move |_| {
            if let Some(p) = upgrade(&weak) {
                p.dragging.set(true);
                p.target.set(None);
            }
        });
        let weak = Rc::downgrade(self);
        c.connect_scroll(move |c, dx, dy| {
            let Some(p) = upgrade(&weak) else { return glib::Propagation::Proceed };
            // Mostly vertical motion belongs to the page around the carousel.
            if dx.abs() < dy.abs() && p.dragging.get() {
                return glib::Propagation::Proceed;
            }
            let h = p.scroller.window.hadjustment();
            if p.dragging.get() {
                // A touchpad: the content follows the fingers.
                h.set_value(h.value() + dx);
            } else {
                // A wheel (notches, or fractions of one on a smooth wheel): one
                // page per notch. A vertical wheel turns pages only with Shift,
                // as GTK scrolls sideways; otherwise it scrolls the page around.
                let shift = c.current_event_state().contains(gtk::gdk::ModifierType::SHIFT_MASK);
                let delta = if dx != 0.0 {
                    dx
                } else if shift {
                    dy
                } else {
                    return glib::Propagation::Proceed;
                };
                let acc = p.wheel.get() + delta;
                if acc.abs() < 1.0 {
                    p.wheel.set(acc);
                    return glib::Propagation::Stop;
                }
                p.wheel.set(0.0);
                let from = p.target.get().unwrap_or_else(|| p.current_page());
                let to = if acc > 0.0 { (from + 1).min(p.pages() - 1) } else { from.saturating_sub(1) };
                p.go_to(to, true);
            }
            glib::Propagation::Stop
        });
        let weak = Rc::downgrade(self);
        c.connect_scroll_end(move |_| {
            if let Some(p) = upgrade(&weak) {
                p.dragging.set(false);
                p.settle_soon();
            }
        });
        let weak = Rc::downgrade(self);
        c.connect_decelerate(move |_, vx, _vy| {
            if let Some(p) = upgrade(&weak) {
                p.velocity.set(vx);
            }
        });
        c
    }

    /// Touch: GTK's own drag moves the content; these only watch the release.
    fn touch(self: &Rc<Self>) -> (gtk::GestureDrag, gtk::GestureSwipe) {
        let drag = gtk::GestureDrag::new();
        drag.set_touch_only(true);
        drag.set_propagation_phase(gtk::PropagationPhase::Capture);
        let weak = Rc::downgrade(self);
        drag.connect_drag_begin(move |_, _, _| {
            if let Some(p) = upgrade(&weak) {
                p.dragging.set(true);
                p.target.set(None);
            }
        });
        let weak = Rc::downgrade(self);
        drag.connect_drag_end(move |_, _, _| {
            if let Some(p) = upgrade(&weak) {
                p.dragging.set(false);
                p.settle_soon();
            }
        });
        let swipe = gtk::GestureSwipe::new();
        swipe.set_touch_only(true);
        swipe.set_propagation_phase(gtk::PropagationPhase::Capture);
        let weak = Rc::downgrade(self);
        swipe.connect_swipe(move |_, vx, _| {
            if let Some(p) = upgrade(&weak) {
                // The finger moving left moves the content toward later pages.
                p.velocity.set(-vx);
            }
        });
        (drag, swipe)
    }
}

/// The paging state, only while paging is on.
fn upgrade(weak: &Weak<Paging>) -> Option<Rc<Paging>> {
    weak.upgrade().filter(|p| p.enabled.get())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pages_are_whole_viewports() {
        assert_eq!(page_count(1170.0, 390.0), 3);
        assert_eq!(page_count(1170.4, 390.0), 3, "a rounding sliver is not a page");
        assert_eq!(page_count(1000.0, 390.0), 3);
        assert_eq!(page_count(200.0, 390.0), 1);
        assert_eq!(page_count(0.0, 0.0), 1);
    }

    #[test]
    fn rests_on_the_nearest_page_without_a_fling() {
        assert_eq!(rest_page(0.0, 390.0, 3, 0.0), 0);
        assert_eq!(rest_page(194.0, 390.0, 3, 0.0), 0);
        assert_eq!(rest_page(196.0, 390.0, 3, 100.0), 1);
        assert_eq!(rest_page(5000.0, 390.0, 3, 0.0), 2);
        assert_eq!(rest_page(-40.0, 390.0, 3, 0.0), 0);
    }

    #[test]
    fn a_fling_moves_to_the_next_page_in_its_direction_only() {
        assert_eq!(rest_page(40.0, 390.0, 3, 800.0), 1, "a short fling still turns the page");
        assert_eq!(rest_page(390.0, 390.0, 3, 5000.0), 2, "never more than one page");
        assert_eq!(rest_page(350.0, 390.0, 3, -800.0), 0);
        assert_eq!(rest_page(780.0, 390.0, 3, 800.0), 2, "clamped to the last page");
        assert_eq!(rest_page(0.0, 390.0, 3, -800.0), 0);
    }

    #[test]
    fn the_last_page_ends_with_the_content() {
        assert_eq!(page_offset(1, 390.0, 1170.0), 390.0);
        assert_eq!(page_offset(2, 390.0, 1000.0), 610.0);
        assert_eq!(page_offset(0, 390.0, 200.0), 0.0);
    }
}
