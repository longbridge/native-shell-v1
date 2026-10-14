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
        state.host.set_background(Some(p.color("background", 0xFFFF_FFFF)));
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
}

/// Sends `BackPressed` when the top page refuses to be left; `false` when
/// there is nothing to refuse (no page, or the page can go back).
pub(crate) fn back_refused(presenter: &Presenter) -> bool {
    let visible = presenter.nav.visible_page();
    let top = presenter.pages().into_iter().find(|p| Some(&p.nav_page) == visible.as_ref() && p.wanted());
    match top {
        Some(page) if !page.can_go_back.get() => {
            presenter.emit(page.node, EventType::BackPressed, "{}".into());
            true
        }
        _ => false,
    }
}

/// System back on the top page, as the input handlers above would do it.
#[cfg(test)]
pub(crate) fn perform_back(presenter: &Presenter) {
    if !back_refused(presenter) && stack(&presenter.nav).len() > 1 {
        presenter.nav.pop();
    }
}
