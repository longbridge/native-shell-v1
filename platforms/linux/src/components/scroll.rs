//! `Scroll`: a `GtkScrolledWindow` around a content node the size of the
//! core's `ContentSize`; the scroll node's children are placed inside it at
//! the frames the core computed in content coordinates.
//!
//! Scrolling is GTK's own (kinetic on touchpads and touch screens, overlay
//! indicators that appear while scrolling). Scroll events are coalesced to
//! one per main-loop turn. `ScrollTo` animates with a critically damped
//! spring, which libadwaita skips when the desktop turns animations off.
//!
//! `Paging` (pages of the viewport's width) is in `paging.rs`.
//!
//! Pull to refresh has no desktop control, so it is mapped to what a desktop
//! offers: pulling past the top edge with a touchpad or touch screen (GTK's
//! `edge-overshot`), or F5 / Ctrl+R while focus is inside the scroll view.
//! While refreshing, a spinner band slides in above the content, as on iOS.

use std::cell::{Cell, RefCell};
use std::rc::{Rc, Weak};

use adw::prelude::*;
use gtk::{gdk, glib};

use native_shell_core::protocol::{event_mask, Command, EventType, Frame, Prop, PropValue};

use crate::component::{Component, NodeCx, Shared};
use crate::mount::json;
use crate::components::paging::Paging;
use crate::node_view::NodeView;

/// Height of the band that shows the refresh spinner.
const REFRESH_BAND: i32 = 56;

/// Installs the stylesheet scroll views share: no Adwaita overshoot glow or
/// undershoot shading (iOS shows neither).
fn ensure_css() {
    thread_local!(static DONE: Cell<bool> = const { Cell::new(false) });
    if DONE.with(|d| d.replace(true)) {
        return;
    }
    let Some(display) = gdk::Display::default() else { return };
    let provider = gtk::CssProvider::new();
    provider.load_from_string(
        "scrolledwindow.nsh-scroll > overshoot, scrolledwindow.nsh-scroll > undershoot { background: none; box-shadow: none; }",
    );
    gtk::style_context_add_provider_for_display(&display, &provider, gtk::STYLE_PROVIDER_PRIORITY_APPLICATION);
}

/// The scrolling part shared by `Scroll` and `List`: the scrolled window,
/// coalesced scroll events and animated programmatic scrolling.
#[derive(Clone)]
pub(crate) struct Scroller {
    pub window: gtk::ScrolledWindow,
    inner: Rc<ScrollerInner>,
}

struct ScrollerInner {
    node: u32,
    shared: Weak<Shared>,
    window: glib::WeakRef<gtk::ScrolledWindow>,
    mask: Cell<u32>,
    /// The offset moved since the last scroll event.
    moved: Cell<bool>,
    pending: Cell<bool>,
    /// Runs once per frame after any change the owner scheduled (a list's range report).
    on_frame: RefCell<Option<Box<dyn Fn()>>>,
    animation: RefCell<Option<adw::Animation>>,
}

impl Scroller {
    pub fn new(cx: &NodeCx) -> Scroller {
        ensure_css();
        let window = gtk::ScrolledWindow::new();
        window.add_css_class("nsh-scroll");
        window.set_kinetic_scrolling(true);
        window.set_overlay_scrolling(true);
        let inner = Rc::new(ScrollerInner {
            node: cx.node,
            shared: Rc::downgrade(cx.shared),
            window: window.downgrade(),
            mask: Cell::new(0),
            moved: Cell::new(false),
            pending: Cell::new(false),
            on_frame: RefCell::new(None),
            animation: RefCell::new(None),
        });
        for adj in [window.hadjustment(), window.vadjustment()] {
            let inner = inner.clone();
            adj.connect_value_changed(move |_| {
                inner.moved.set(true);
                inner.schedule();
            });
        }
        // The user takes over from a running ScrollTo animation.
        let wheel = gtk::EventControllerScroll::new(gtk::EventControllerScrollFlags::BOTH_AXES);
        wheel.set_propagation_phase(gtk::PropagationPhase::Capture);
        let weak = Rc::downgrade(&inner);
        wheel.connect_scroll(move |_, _, _| {
            if let Some(inner) = weak.upgrade() {
                inner.stop_animation();
            }
            glib::Propagation::Proceed
        });
        window.add_controller(wheel);
        Scroller { window, inner }
    }

    pub fn set_event_mask(&self, mask: u32) {
        self.inner.mask.set(mask);
    }

    /// Sets what runs, coalesced, after [`Scroller::schedule`] or a scroll.
    pub fn set_on_frame(&self, f: impl Fn() + 'static) {
        self.inner.on_frame.replace(Some(Box::new(f)));
    }

    pub fn schedule(&self) {
        self.inner.schedule();
    }

    /// [`Scroller::schedule`] for widgets that outlive no one: holds the scroller weakly.
    pub fn scheduler(&self) -> impl Fn() + 'static {
        let weak = Rc::downgrade(&self.inner);
        move || {
            if let Some(inner) = weak.upgrade() {
                inner.schedule();
            }
        }
    }

    pub fn offset(&self) -> (f64, f64) {
        (self.window.hadjustment().value(), self.window.vadjustment().value())
    }

    /// Scrolls to `(x, y)`, clamped to `content` minus `viewport`.
    ///
    /// The batch that grew the content usually carries the command too, before
    /// GTK laid the content out again, so the adjustments learn the new extent
    /// here; otherwise they would clamp the target to the old one.
    pub fn scroll_to(&self, x: f64, y: f64, content: (f64, f64), viewport: (f64, f64), animated: bool) {
        let (hadj, vadj) = (self.window.hadjustment(), self.window.vadjustment());
        let x = x.clamp(0.0, (content.0 - viewport.0).max(0.0));
        let y = y.clamp(0.0, (content.1 - viewport.1).max(0.0));
        for (adj, extent, page) in [(&hadj, content.0, viewport.0), (&vadj, content.1, viewport.1)] {
            if page > 0.0 {
                adj.set_page_size(page);
            }
            adj.set_upper(adj.upper().max(extent));
        }
        self.inner.stop_animation();
        let (x0, y0) = (hadj.value(), vadj.value());
        if !animated || ((x - x0).abs() < 0.5 && (y - y0).abs() < 0.5) {
            hadj.set_value(x);
            vadj.set_value(y);
            return;
        }
        let target = adw::CallbackAnimationTarget::new(move |p| {
            hadj.set_value(x0 + (x - x0) * p);
            vadj.set_value(y0 + (y - y0) * p);
        });
        // Critically damped: settles in about 0.4 s without overshooting.
        let spring = adw::SpringAnimation::new(&self.window, 0.0, 1.0, adw::SpringParams::new(1.0, 1.0, 230.0), target);
        let weak = Rc::downgrade(&self.inner);
        spring.connect_done(move |_| {
            if let Some(inner) = weak.upgrade() {
                inner.animation.replace(None);
            }
        });
        self.inner.animation.replace(Some(spring.clone().upcast()));
        spring.play();
    }

    pub fn unmount(&self) {
        self.inner.stop_animation();
        self.inner.on_frame.replace(None);
    }
}

impl ScrollerInner {
    fn stop_animation(&self) {
        if let Some(a) = self.animation.take() {
            a.pause();
        }
    }

    /// Runs [`ScrollerInner::frame`] once the main loop is idle, once: every
    /// offset change of a frame (kinetic steps, animation ticks, relayout)
    /// collapses into one event.
    fn schedule(self: &Rc<Self>) {
        if self.pending.replace(true) {
            return;
        }
        let weak = Rc::downgrade(self);
        glib::idle_add_local_once(move || {
            if let Some(inner) = weak.upgrade() {
                inner.pending.set(false);
                inner.frame();
            }
        });
    }

    fn frame(&self) {
        if self.moved.replace(false) && self.mask.get() & event_mask::SCROLL != 0 {
            if let (Some(window), Some(shared)) = (self.window.upgrade(), self.shared.upgrade()) {
                (shared.events)(self.node, EventType::Scroll, scroll_payload(&window));
            }
        }
        if let Some(f) = self.on_frame.borrow().as_ref() {
            f();
        }
    }
}

/// The scroll event payload, the same for scroll views and lists.
pub(crate) fn scroll_payload(window: &gtk::ScrolledWindow) -> String {
    let (h, v) = (window.hadjustment(), window.vadjustment());
    serde_json::json!({
        "x": h.value(),
        "y": v.value(),
        "content_width": h.upper(),
        "content_height": v.upper(),
        "viewport_width": h.page_size(),
        "viewport_height": v.page_size(),
    })
    .to_string()
}

pub(crate) fn bool_prop(value: &PropValue, default: bool) -> bool {
    match value {
        PropValue::Bool(b) => *b,
        _ => default,
    }
}

/// Refresh state the signal handlers share with the component.
struct Refresh {
    enabled: Cell<bool>,
    refreshing: Cell<bool>,
    revealer: gtk::Revealer,
}

impl Refresh {
    fn show(&self, on: bool) {
        self.refreshing.set(on);
        self.revealer.set_reveal_child(on);
    }

    /// The user asked for a refresh: show the spinner at once (as
    /// `UIRefreshControl` does) and tell the script, which ends it with
    /// `Refreshing(false)`.
    fn pull(&self, node: u32, shared: &Weak<Shared>) -> bool {
        if !self.enabled.get() || self.refreshing.get() {
            return false;
        }
        self.show(true);
        if let Some(shared) = shared.upgrade() {
            (shared.events)(node, EventType::Refresh, "{}".into());
        }
        true
    }
}

pub struct Scroll {
    scroller: Scroller,
    /// Holds the children; sized to `ContentSize`.
    content: NodeView,
    refresh: Rc<Refresh>,
    paging: Rc<Paging>,
    horizontal: bool,
    indicators: bool,
    content_size: (f64, f64),
    viewport: (f64, f64),
}

impl Scroll {
    pub fn new(cx: &NodeCx) -> Scroll {
        let scroller = Scroller::new(cx);
        let content = NodeView::new();
        scroller.window.set_child(Some(&content));

        let spinner = adw::Spinner::new();
        spinner.set_size_request(24, 24);
        spinner.set_halign(gtk::Align::Center);
        spinner.set_valign(gtk::Align::Center);
        let band = gtk::Box::new(gtk::Orientation::Vertical, 0);
        band.set_size_request(-1, REFRESH_BAND);
        band.append(&spinner);
        let revealer = gtk::Revealer::new();
        revealer.set_transition_type(gtk::RevealerTransitionType::SlideDown);
        revealer.set_transition_duration(250);
        revealer.set_child(Some(&band));

        let container = gtk::Box::new(gtk::Orientation::Vertical, 0);
        container.append(&revealer);
        scroller.window.set_vexpand(true);
        container.append(&scroller.window);
        cx.view.set_content(Some(container.upcast_ref()));

        let refresh = Rc::new(Refresh { enabled: Cell::new(false), refreshing: Cell::new(false), revealer });
        let (node, shared) = (cx.node, Rc::downgrade(cx.shared));
        {
            let refresh = refresh.clone();
            let shared = shared.clone();
            scroller.window.connect_edge_overshot(move |_, pos| {
                if pos == gtk::PositionType::Top {
                    refresh.pull(node, &shared);
                }
            });
        }
        let keys = gtk::ShortcutController::new();
        keys.set_scope(gtk::ShortcutScope::Managed);
        let action = {
            let refresh = refresh.clone();
            gtk::CallbackAction::new(move |_, _| {
                if refresh.pull(node, &shared) {
                    glib::Propagation::Stop
                } else {
                    glib::Propagation::Proceed
                }
            })
        };
        let trigger = gtk::ShortcutTrigger::parse_string("F5|<Control>r");
        keys.add_shortcut(gtk::Shortcut::new(trigger, Some(action)));
        container.add_controller(keys);

        let paging = Paging::new(scroller.clone());

        let mut scroll = Scroll {
            scroller,
            paging,
            content,
            refresh,
            horizontal: false,
            indicators: true,
            content_size: (0.0, 0.0),
            viewport: (0.0, 0.0),
        };
        scroll.apply_policy();
        scroll
    }

    fn apply_policy(&mut self) {
        // External keeps the view scrollable without drawing indicators.
        let p = if self.indicators { gtk::PolicyType::Automatic } else { gtk::PolicyType::External };
        self.scroller.window.set_policy(p, p);
    }
}

impl Component for Scroll {
    fn set_prop(&mut self, _cx: &NodeCx, prop: Prop, value: &PropValue) -> bool {
        match prop {
            Prop::ContentSize => {
                let size = match value {
                    PropValue::F32Array(a) if a.len() == 2 => (a[0] as f64, a[1] as f64),
                    _ => (0.0, 0.0),
                };
                self.content_size = size;
                self.paging.content.set(size);
                self.content.set_size_request(size.0.ceil() as i32, size.1.ceil() as i32);
            }
            Prop::Paging => self.paging.set_enabled(bool_prop(value, false)),
            Prop::Horizontal => {
                self.horizontal = bool_prop(value, false);
                // Pull to refresh only belongs to a vertical scroller.
                if self.horizontal && self.refresh.refreshing.get() {
                    self.refresh.show(false);
                }
            }
            Prop::ShowsIndicators => {
                self.indicators = bool_prop(value, true);
                self.apply_policy();
            }
            Prop::RefreshEnabled => {
                let on = bool_prop(value, false);
                self.refresh.enabled.set(on && !self.horizontal);
                if !on {
                    self.refresh.show(false);
                }
            }
            Prop::Refreshing => {
                let on = bool_prop(value, false) && !self.horizontal;
                if on != self.refresh.refreshing.get() {
                    self.refresh.show(on);
                }
            }
            _ => return false,
        }
        true
    }

    fn set_frame(&mut self, _cx: &NodeCx, frame: Frame) {
        let old = self.viewport.0;
        self.viewport = (frame.width as f64, frame.height as f64);
        self.paging.viewport.set(self.viewport);
        self.paging.viewport_changed(old);
    }

    fn set_event_mask(&mut self, _cx: &NodeCx, mask: u32) {
        self.scroller.set_event_mask(mask);
    }

    fn command(&mut self, _cx: &NodeCx, command: Command, payload: &str) {
        if command != Command::ScrollTo {
            return;
        }
        let p = json(&PropValue::Json(payload.to_string())).unwrap_or_default();
        let (x0, y0) = self.scroller.offset();
        let x = p.get("x").and_then(|v| v.as_f64()).unwrap_or(x0);
        let y = p.get("y").and_then(|v| v.as_f64()).unwrap_or(y0);
        let animated = p.get("animated").and_then(|v| v.as_bool()).unwrap_or(true);
        self.scroller.scroll_to(x, y, self.content_size, self.viewport, animated);
    }

    fn insert_child(&mut self, _cx: &NodeCx, child: &NodeView, index: usize) {
        self.content.insert_node_child(child, index);
    }

    fn unmount(&mut self, _cx: &NodeCx) {
        self.scroller.unmount();
    }
}
