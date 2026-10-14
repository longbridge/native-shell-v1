//! `Page` nodes on the navigation stack.
//!
//! Pages are full screen and programs draw their own headers, so there is no
//! navigation bar; `Title` names the page for accessibility. The stack is an
//! `AdwNavigationView`: it gives the slide transitions (which follow the
//! system's reduce-motion setting), the edge swipe on touchscreens and
//! touchpads, and Escape / Alt+Left / the mouse back button. The core owns
//! the stack: a page the user pops is reported with `Dismiss`, and a page
//! whose `CanGoBack` is false is not popped but reported with `BackPressed`.
//!
//! A deleted page stays on screen until its exit transition ends; only then
//! are its views released.

use std::cell::Cell;
use std::rc::{Rc, Weak};

use adw::prelude::*;
use gtk::glib;

use native_shell_core::protocol::{Command, EventType, Prop, PropValue};

use crate::component::{Component, NodeCx};
use crate::components::overlays::{is_back_key, FrameHost, Presenter, Surface};
use crate::node_view::NodeView;

pub struct PageState {
    pub node: u32,
    pub view: NodeView,
    pub nav_page: adw::NavigationPage,
    pub host: FrameHost,
    pub can_go_back: Cell<bool>,
    /// Attached under the root by the core.
    pub attached: Cell<bool>,
    /// `DismissPage` arrived: the page leaves the stack; its `Delete` may come later.
    pub dismissing: Cell<bool>,
    /// The user popped it (and `Dismiss` was sent); the core removes it next.
    pub user_popped: Cell<bool>,
    pub deleted: Cell<bool>,
    /// Between the page's `showing` and `hidden`: its views are on screen.
    pub on_screen: Cell<bool>,
}

impl PageState {
    /// On the stack as far as the core is concerned.
    fn wanted(&self) -> bool {
        self.attached.get() && !self.deleted.get() && !self.dismissing.get() && !self.user_popped.get()
    }
}

pub struct PageComponent {
    state: Rc<PageState>,
    presenter: Weak<Presenter>,
}

pub fn create(cx: &NodeCx) -> Box<dyn Component> {
    let host = FrameHost::default();
    host.add(cx.view);
    let nav_page = adw::NavigationPage::builder().child(&host).title(" ").build();
    let state = Rc::new(PageState {
        node: cx.node,
        view: cx.view.clone(),
        nav_page,
        host,
        can_go_back: Cell::new(true),
        attached: Cell::new(false),
        dismissing: Cell::new(false),
        user_popped: Cell::new(false),
        deleted: Cell::new(false),
        on_screen: Cell::new(false),
    });
    let presenter = cx.shared.overlays.presenter();
    if let Some(p) = &presenter {
        state.host.set_background(Some(p.color("background")));
        p.register(cx.view, Surface::Page(state.clone()));
        let weak_state = Rc::downgrade(&state);
        state.nav_page.connect_showing(move |_| {
            if let Some(s) = weak_state.upgrade() {
                s.on_screen.set(true);
            }
        });
        let (weak_state, weak_presenter) = (Rc::downgrade(&state), Rc::downgrade(p));
        state.nav_page.connect_hidden(move |_| {
            let (Some(s), Some(p)) = (weak_state.upgrade(), weak_presenter.upgrade()) else { return };
            s.on_screen.set(false);
            if s.deleted.get() {
                release(&p, &s);
            }
        });
    }
    Box::new(PageComponent { state, presenter: presenter.as_ref().map(Rc::downgrade).unwrap_or_default() })
}

impl Component for PageComponent {
    fn set_prop(&mut self, _cx: &NodeCx, prop: Prop, value: &PropValue) -> bool {
        match (prop, value) {
            (Prop::Title, PropValue::Str(title)) => {
                // The navigation view wants a title on every page.
                self.state.nav_page.set_title(if title.is_empty() { " " } else { title });
            }
            (Prop::Title, _) => self.state.nav_page.set_title(" "),
            (Prop::CanGoBack, v) => {
                let can = !matches!(v, PropValue::Bool(false));
                self.state.can_go_back.set(can);
                // Gestures and shortcuts stop; programmatic pops still work.
                self.state.nav_page.set_can_pop(can);
            }
            // Reserved: renderers draw no navigation bar.
            (Prop::NavBarHidden, _) => {}
            _ => return false,
        }
        true
    }

    fn command(&mut self, _cx: &NodeCx, command: Command, payload: &str) {
        let animated = serde_json::from_str::<serde_json::Value>(payload)
            .ok()
            .and_then(|v| v.get("animated").and_then(|a| a.as_bool()))
            .unwrap_or(true);
        match command {
            Command::PresentPage => {
                self.state.dismissing.set(false);
                self.state.user_popped.set(false);
            }
            Command::DismissPage => self.state.dismissing.set(true),
            _ => return,
        }
        if let Some(p) = self.presenter.upgrade() {
            p.mark_pages_changed(animated);
        }
    }

    fn unmount(&mut self, _cx: &NodeCx) {
        let state = &self.state;
        state.deleted.set(true);
        let Some(p) = self.presenter.upgrade() else { return };
        let in_stack = stack(&p.nav).contains(&state.nav_page);
        if in_stack {
            // Deleted without `DismissPage`: the next sync pops it.
            p.mark_pages_changed(false);
        } else if !state.on_screen.get() {
            release(&p, state);
        }
    }

    fn keeps_views_on_delete(&self) -> bool {
        true
    }
}

fn release(presenter: &Presenter, state: &PageState) {
    presenter.unregister(&state.view);
    state.host.clear();
}

fn stack(nav: &adw::NavigationView) -> Vec<adw::NavigationPage> {
    nav.navigation_stack().iter::<adw::NavigationPage>().filter_map(Result::ok).collect()
}

/// Makes the navigation stack match the core's pages.
pub(crate) fn sync(presenter: &Rc<Presenter>, animated: bool) {
    let nav = &presenter.nav;
    let pages = presenter.pages();
    let wanted: Vec<adw::NavigationPage> = std::iter::once(presenter.root_page())
        .chain(pages.iter().filter(|p| p.wanted()).map(|p| p.nav_page.clone()))
        .collect();
    let current = stack(nav);
    if wanted == current {
        return;
    }
    // Pages present in the first batch, or pushed before the window shows, appear without a transition.
    let animate = animated && nav.is_mapped();
    presenter.syncing.set(true);
    let common = wanted.iter().zip(&current).take_while(|(a, b)| a == b).count();
    if common == current.len() {
        let last = wanted.len() - 1;
        for (i, page) in wanted.iter().enumerate().skip(common) {
            nav.set_animate_transitions(animate && i == last);
            nav.push(page);
        }
    } else if common == wanted.len() {
        nav.set_animate_transitions(animate);
        nav.pop_to_page(&wanted[common - 1]);
    } else {
        // A different top: drop what changed at once, then push the new pages.
        nav.set_animate_transitions(false);
        nav.pop_to_page(&wanted[common - 1]);
        let last = wanted.len() - 1;
        for (i, page) in wanted.iter().enumerate().skip(common) {
            nav.set_animate_transitions(animate && i == last);
            nav.push(page);
        }
    }
    nav.set_animate_transitions(true);
    presenter.syncing.set(false);
}

/// Reports the user's pops and the back input a page refuses.
pub(crate) fn install_back_handling(presenter: &Rc<Presenter>) {
    let nav = &presenter.nav;
    let weak = Rc::downgrade(presenter);
    nav.connect_popped(move |_, popped| {
        let Some(p) = weak.upgrade() else { return };
        if p.syncing.get() {
            return;
        }
        if let Some(page) = p.pages().into_iter().find(|s| &s.nav_page == popped) {
            if page.wanted() {
                page.user_popped.set(true);
                p.emit(page.node, EventType::Dismiss, "{}".into());
            }
        }
    });

    // The navigation view consumes these when the top page can be popped;
    // otherwise they reach us and the page decides.
    let keys = gtk::EventControllerKey::new();
    let weak = Rc::downgrade(presenter);
    keys.connect_key_pressed(move |controller, key, _, state| {
        let Some(p) = weak.upgrade() else { return glib::Propagation::Proceed };
        let Some(widget) = controller.widget() else { return glib::Propagation::Proceed };
        if is_back_key(&widget, key, state) && back_refused(&p) {
            return glib::Propagation::Stop;
        }
        glib::Propagation::Proceed
    });
    nav.add_controller(keys);

    let click = gtk::GestureClick::builder().button(8).build();
    let weak = Rc::downgrade(presenter);
    click.connect_pressed(move |gesture, _, _, _| {
        if let Some(p) = weak.upgrade() {
            if back_refused(&p) {
                gesture.set_state(gtk::EventSequenceState::Claimed);
            }
        }
    });
    nav.add_controller(click);

    install_refused_swipes(presenter);
}

/// From how far in from the leading edge a touch drag counts as the back swipe.
const EDGE: f64 = 24.0;
/// How far a swipe travels before it counts as a back attempt.
const SWIPE: f64 = 40.0;

/// A horizontal movement of `forward` points toward the trailing edge, `across` sideways.
fn is_back_swipe(forward: f64, across: f64) -> bool {
    forward > SWIPE && across.abs() < forward
}

/// The navigation view does not start its swipe on a page that cannot go back;
/// the same swipes are reported as `BackPressed` instead, once per gesture, as
/// iOS refuses its edge swipe and tells the page.
fn install_refused_swipes(presenter: &Rc<Presenter>) {
    let nav = &presenter.nav;
    let rtl = |w: &gtk::Widget| w.direction() == gtk::TextDirection::Rtl;

    // Touchscreen: a drag from the leading edge.
    let drag = gtk::GestureDrag::builder().touch_only(true).propagation_phase(gtk::PropagationPhase::Capture).build();
    // Set once the gesture has been reported.
    let reported = Rc::new(std::cell::Cell::new(false));
    let weak = Rc::downgrade(presenter);
    let r = reported.clone();
    drag.connect_drag_begin(move |gesture, x, _| {
        r.set(false);
        let Some(p) = weak.upgrade() else { return };
        let Some(widget) = gesture.widget() else { return };
        let from_edge = if rtl(&widget) { widget.width() as f64 - x } else { x };
        if from_edge > EDGE || top_refusing(&p).is_none() {
            gesture.set_state(gtk::EventSequenceState::Denied);
        }
    });
    let weak = Rc::downgrade(presenter);
    drag.connect_drag_update(move |gesture, dx, dy| {
        let Some(p) = weak.upgrade() else { return };
        let Some(widget) = gesture.widget() else { return };
        let forward = if rtl(&widget) { -dx } else { dx };
        if !reported.get() && is_back_swipe(forward, dy) && top_refusing(&p).is_some() {
            reported.set(true);
            gesture.set_state(gtk::EventSequenceState::Claimed);
            back_refused(&p);
        }
    });
    nav.add_controller(drag);

    // Touchpad: a two-finger swipe toward the trailing edge.
    let scroll = gtk::EventControllerScroll::new(gtk::EventControllerScrollFlags::HORIZONTAL);
    let travel = Rc::new(std::cell::Cell::new(Some(0.0)));
    let t = travel.clone();
    scroll.connect_scroll_begin(move |_| t.set(Some(0.0)));
    let t = travel.clone();
    scroll.connect_scroll_end(move |_| t.set(Some(0.0)));
    let weak = Rc::downgrade(presenter);
    scroll.connect_scroll(move |controller, dx, dy| {
        let Some(p) = weak.upgrade() else { return glib::Propagation::Proceed };
        let Some(widget) = controller.widget() else { return glib::Propagation::Proceed };
        // Only touchpads scroll in surface units; wheels are not swipes.
        if controller.unit() != gtk::gdk::ScrollUnit::Surface || top_refusing(&p).is_none() {
            return glib::Propagation::Proceed;
        }
        // `None` once this gesture has been reported.
        let Some(sofar) = travel.get() else { return glib::Propagation::Stop };
        // Fingers moving toward the trailing edge scroll content back: negative dx in LTR.
        let forward = sofar + if rtl(&widget) { dx } else { -dx };
        if is_back_swipe(forward, dy) {
            travel.set(None);
            back_refused(&p);
            return glib::Propagation::Stop;
        }
        travel.set(Some(forward));
        glib::Propagation::Proceed
    });
    nav.add_controller(scroll);
}

/// The top page, when it is on the stack and cannot go back.
fn top_refusing(presenter: &Presenter) -> Option<Rc<PageState>> {
    let visible = presenter.nav.visible_page()?;
    presenter.pages().into_iter().find(|p| p.nav_page == visible && p.wanted() && !p.can_go_back.get())
}

/// Sends `BackPressed` when the top page refuses to be left; `false` when
/// there is nothing to refuse (no page, or the page can go back).
pub(crate) fn back_refused(presenter: &Presenter) -> bool {
    match top_refusing(presenter) {
        Some(page) => {
            presenter.emit(page.node, EventType::BackPressed, "{}".into());
            true
        }
        None => false,
    }
}

/// System back on the top page; `false` when only the root content is shown.
pub(crate) fn perform_back(presenter: &Presenter) -> bool {
    if back_refused(presenter) {
        return true;
    }
    stack(&presenter.nav).len() > 1 && presenter.nav.pop()
}

#[cfg(test)]
mod tests {
    use super::is_back_swipe;

    #[test]
    fn back_swipes_are_long_and_mostly_horizontal() {
        assert!(is_back_swipe(60.0, 10.0));
        assert!(!is_back_swipe(30.0, 0.0), "too short");
        assert!(!is_back_swipe(60.0, 80.0), "mostly vertical");
        assert!(!is_back_swipe(-60.0, 0.0), "toward the leading edge");
    }
}
