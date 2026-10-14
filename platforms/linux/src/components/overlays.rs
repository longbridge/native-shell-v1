//! The root node and the surfaces the core attaches to it.
//!
//! The root (node 0) does not place its children itself: its content is a
//! [`Stage`] holding, bottom to top, an `AdwNavigationView` (the root content
//! is its first page, every `Page` node a pushed page), the open sheets and
//! dialogs in opening order, and the toasts. Pages, sheets and dialogs register
//! here when they are created, so the root knows what a child it is given is.
//! Presentation happens once a batch is complete, when content and props are
//! known (the core sends a pushed page and its `PresentPage` in one batch).
//!
//! The root also runs the root commands: toasts, `OpenUrl`, `SetClipboard`
//! and `Haptic`.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::{Rc, Weak};

use adw::prelude::*;
use gtk::subclass::prelude::*;
use gtk::{gdk, glib, graphene};

use native_shell_core::protocol::{Command, EventType, Prop, PropValue, ROOT_NODE};

use crate::component::{Component, NodeCx, Shared};
use crate::components::modal::ModalState;
use crate::components::pages::{self, PageState};
use crate::node_view::{rgba, NodeView};
use crate::toast::ToastCenter;

/// The presenter of one surface, reachable from every node through [`Shared`].
#[derive(Default)]
pub struct Overlays(RefCell<Option<Rc<Presenter>>>);

impl Overlays {
    pub fn presenter(&self) -> Option<Rc<Presenter>> {
        self.0.borrow().clone()
    }
}

pub(crate) enum Surface {
    Page(Rc<PageState>),
    Modal(Rc<ModalState>),
}

pub struct Presenter {
    pub(crate) shared: Weak<Shared>,
    pub(crate) stage: Stage,
    pub(crate) nav: adw::NavigationView,
    root_host: FrameHost,
    /// Pages, sheets and dialogs by view, registered when created.
    surfaces: RefCell<HashMap<NodeView, Surface>>,
    /// The root's children in the core's order (root content, pages, overlays).
    order: RefCell<Vec<NodeView>>,
    pages_changed: Cell<bool>,
    pages_animated: Cell<bool>,
    modals_changed: Cell<bool>,
    /// Set while the stack is changed to match the core, so pops are not taken
    /// for the user's.
    pub(crate) syncing: Cell<bool>,
    pub(crate) toasts: ToastCenter,
}

impl Presenter {
    fn new(cx: &NodeCx) -> Rc<Presenter> {
        let stage = Stage::default();
        let nav = adw::NavigationView::new();
        let root_host = FrameHost::default();
        let root_page = adw::NavigationPage::builder().child(&root_host).title("Home").tag("root").build();
        nav.add(&root_page);
        stage.add(&nav);
        let toasts = ToastCenter::new(&stage);
        cx.view.set_content(Some(stage.upcast_ref()));
        let presenter = Rc::new(Presenter {
            shared: Rc::downgrade(cx.shared),
            stage,
            nav,
            root_host,
            surfaces: RefCell::default(),
            order: RefCell::default(),
            pages_changed: Cell::new(false),
            pages_animated: Cell::new(false),
            modals_changed: Cell::new(false),
            syncing: Cell::new(false),
            toasts,
        });
        presenter.toasts.attach(&presenter);
        pages::install_back_handling(&presenter);
        presenter.theme_changed();
        presenter
    }

    pub(crate) fn emit(&self, node: u32, event: EventType, payload: String) {
        if let Some(shared) = self.shared.upgrade() {
            (shared.events)(node, event, payload);
        }
    }

    /// A color of the program's theme, with the fallback used before the first batch.
    pub(crate) fn color(&self, token: &str, fallback: u32) -> u32 {
        self.shared.upgrade().and_then(|s| s.theme.borrow().color(token)).unwrap_or(fallback)
    }

    pub(crate) fn register(&self, view: &NodeView, surface: Surface) {
        self.surfaces.borrow_mut().insert(view.clone(), surface);
    }

    #[cfg(test)]
    pub(crate) fn surfaces_ref(&self) -> std::cell::Ref<'_, HashMap<NodeView, Surface>> {
        self.surfaces.borrow()
    }

    pub(crate) fn unregister(&self, view: &NodeView) {
        self.surfaces.borrow_mut().remove(view);
    }

    pub(crate) fn mark_pages_changed(&self, animated: bool) {
        self.pages_changed.set(true);
        if animated {
            self.pages_animated.set(true);
        }
    }

    pub(crate) fn mark_modals_changed(&self) {
        self.modals_changed.set(true);
    }

    /// The pages attached to the root, in stack order.
    pub(crate) fn pages(&self) -> Vec<Rc<PageState>> {
        let surfaces = self.surfaces.borrow();
        self.order
            .borrow()
            .iter()
            .filter_map(|v| match surfaces.get(v) {
                Some(Surface::Page(p)) => Some(p.clone()),
                _ => None,
            })
            .collect()
    }

    pub(crate) fn root_page(&self) -> adw::NavigationPage {
        self.nav.find_page("root").expect("the root page is added at construction")
    }

    fn insert(&self, child: &NodeView, index: usize) {
        {
            let mut order = self.order.borrow_mut();
            order.retain(|v| v != child);
            let index = index.min(order.len());
            order.insert(index, child.clone());
        }
        let surface = match self.surfaces.borrow().get(child) {
            Some(Surface::Page(p)) => Some(Surface::Page(p.clone())),
            Some(Surface::Modal(m)) => Some(Surface::Modal(m.clone())),
            None => None,
        };
        match surface {
            Some(Surface::Page(page)) => {
                page.attached.set(true);
                self.pages_changed.set(true);
            }
            Some(Surface::Modal(modal)) => {
                modal.attached.set(true);
                self.modals_changed.set(true);
            }
            None => self.root_host.add(child),
        }
    }

    fn remove(&self, child: &NodeView) {
        self.order.borrow_mut().retain(|v| v != child);
        let surface = match self.surfaces.borrow().get(child) {
            Some(Surface::Page(p)) => Some(Surface::Page(p.clone())),
            Some(Surface::Modal(m)) => Some(Surface::Modal(m.clone())),
            None => None,
        };
        match surface {
            Some(Surface::Page(page)) => {
                page.attached.set(false);
                self.pages_changed.set(true);
            }
            Some(Surface::Modal(modal)) => {
                modal.attached.set(false);
                self.modals_changed.set(true);
            }
            None => child.detach(),
        }
    }

    /// System back: the top sheet or dialog first, then the top page.
    pub(crate) fn go_back(&self) -> bool {
        let top_modal = {
            let surfaces = self.surfaces.borrow();
            self.order.borrow().iter().rev().find_map(|v| match surfaces.get(v) {
                Some(Surface::Modal(m)) if m.is_open() => Some(m.clone()),
                _ => None,
            })
        };
        match top_modal {
            Some(modal) => {
                modal.back();
                true
            }
            None => pages::perform_back(self),
        }
    }

    /// The batch is complete: bring the navigation stack and the modals in line.
    fn batch_applied(self: &Rc<Self>) {
        if self.pages_changed.replace(false) {
            let animated = self.pages_animated.replace(false);
            pages::sync(self, animated);
        }
        if self.modals_changed.replace(false) {
            for modal in self.modals() {
                modal.present(self);
            }
            self.update_focus();
        }
    }

    /// The sheets and dialogs attached to the root, in opening order.
    fn modals(&self) -> Vec<Rc<ModalState>> {
        let surfaces = self.surfaces.borrow();
        self.order
            .borrow()
            .iter()
            .filter_map(|v| match surfaces.get(v) {
                Some(Surface::Modal(m)) => Some(m.clone()),
                _ => None,
            })
            .collect()
    }

    /// Keyboard focus stays in the top open sheet or dialog: nothing below it
    /// can take focus, so Tab cycles inside it. (Alerts trap focus themselves.)
    pub(crate) fn update_focus(&self) {
        let modals = self.modals();
        let hosts: Vec<_> = modals.iter().filter(|m| m.is_open()).filter_map(|m| m.host.borrow().clone()).collect();
        let top = hosts.last();
        self.nav.set_can_focus(top.is_none());
        for host in modals.iter().filter_map(|m| m.host.borrow().clone()) {
            host.set_can_focus(Some(&host) == top);
        }
    }

    fn theme_changed(&self) {
        let background = self.color("background", 0xFFFF_FFFF);
        self.root_host.set_background(Some(background));
        for surface in self.surfaces.borrow().values() {
            match surface {
                Surface::Page(p) => p.host.set_background(Some(background)),
                Surface::Modal(m) => m.theme_changed(),
            }
        }
        self.toasts.theme_changed(self);
    }

    fn command(self: &Rc<Self>, cx: &NodeCx, command: Command, payload: &str) {
        let payload: serde_json::Value = serde_json::from_str(payload).unwrap_or_default();
        let text = |key: &str| payload.get(key).and_then(|v| v.as_str()).map(str::to_owned);
        match command {
            Command::ShowToast => self.toasts.show(self, &payload),
            Command::DismissToast => self.toasts.dismiss(text("id").as_deref()),
            // OpenUrl, SetClipboard, Haptic.
            _ => {
                crate::root_commands::run(cx.view.upcast_ref(), command, &payload);
            }
        }
    }
}

/// The root node's component.
struct Root {
    presenter: Rc<Presenter>,
}

pub fn root(cx: &NodeCx) -> Box<dyn Component> {
    let presenter = Presenter::new(cx);
    cx.shared.overlays.0.replace(Some(presenter.clone()));
    Box::new(Root { presenter })
}

impl Component for Root {
    fn set_prop(&mut self, _cx: &NodeCx, prop: Prop, _value: &PropValue) -> bool {
        if prop == Prop::ThemeTokens {
            // The mounting manager has stored the tokens already.
            self.presenter.theme_changed();
        }
        false
    }

    fn command(&mut self, cx: &NodeCx, command: Command, payload: &str) {
        debug_assert_eq!(cx.node, ROOT_NODE);
        self.presenter.command(cx, command, payload);
    }

    fn insert_child(&mut self, _cx: &NodeCx, child: &NodeView, index: usize) {
        self.presenter.insert(child, index);
    }

    fn remove_child(&mut self, _cx: &NodeCx, child: &NodeView) {
        self.presenter.remove(child);
    }

    fn batch_applied(&mut self, _cx: &NodeCx) {
        self.presenter.batch_applied();
    }
}

/// Springs as UIKit's: critically damped, `response` seconds to settle.
pub(crate) fn spring(
    widget: &impl IsA<gtk::Widget>,
    from: f64,
    to: f64,
    response: f64,
    step: impl Fn(f64) + 'static,
) -> adw::SpringAnimation {
    let stiffness = (2.0 * std::f64::consts::PI / response).powi(2);
    let animation = adw::SpringAnimation::new(
        widget,
        from,
        to,
        adw::SpringParams::new(1.0, 1.0, stiffness),
        adw::CallbackAnimationTarget::new(step),
    );
    animation.set_epsilon(0.0005);
    animation
}

mod stage_imp {
    use super::*;

    #[derive(Default)]
    pub struct Stage;

    #[glib::object_subclass]
    impl ObjectSubclass for Stage {
        const NAME: &'static str = "NshStage";
        type Type = super::Stage;
        type ParentType = gtk::Widget;
    }

    impl ObjectImpl for Stage {
        fn dispose(&self) {
            while let Some(child) = self.obj().first_child() {
                child.unparent();
            }
        }
    }

    impl WidgetImpl for Stage {
        fn measure(&self, _orientation: gtk::Orientation, _for_size: i32) -> (i32, i32, i32, i32) {
            (0, 0, -1, -1)
        }

        fn size_allocate(&self, width: i32, height: i32, baseline: i32) {
            let mut child = self.obj().first_child();
            while let Some(widget) = child {
                widget.allocate(width, height, baseline, None);
                child = widget.next_sibling();
            }
        }
    }
}

glib::wrapper! {
    /// Layers filling the root: navigation, modals, toasts (bottom to top).
    pub struct Stage(ObjectSubclass<stage_imp::Stage>)
        @extends gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl Default for Stage {
    fn default() -> Self {
        glib::Object::new()
    }
}

impl Stage {
    /// Adds a layer on top of the others (below the toasts once they exist).
    pub fn add(&self, layer: &impl IsA<gtk::Widget>) {
        let toasts = self.last_child().filter(|w| w.is::<crate::toast::ToastLayer>());
        match toasts {
            Some(t) => layer.insert_before(self, Some(&t)),
            None => layer.insert_before(self, None::<&gtk::Widget>),
        }
        self.queue_allocate();
    }
}

mod host_imp {
    use super::*;

    #[derive(Default)]
    pub struct FrameHost {
        pub background: Cell<Option<u32>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for FrameHost {
        const NAME: &'static str = "NshFrameHost";
        type Type = super::FrameHost;
        type ParentType = gtk::Widget;
    }

    impl ObjectImpl for FrameHost {
        fn dispose(&self) {
            while let Some(child) = self.obj().first_child() {
                child.unparent();
            }
        }
    }

    impl WidgetImpl for FrameHost {
        fn measure(&self, _orientation: gtk::Orientation, _for_size: i32) -> (i32, i32, i32, i32) {
            (0, 0, -1, -1)
        }

        fn size_allocate(&self, _width: i32, _height: i32, _baseline: i32) {
            let mut child = self.obj().first_child();
            while let Some(widget) = child {
                child = widget.next_sibling();
                if let Some(node) = widget.downcast_ref::<NodeView>() {
                    node.allocate_at_frame();
                }
            }
        }

        fn snapshot(&self, snapshot: &gtk::Snapshot) {
            let obj = self.obj();
            if let Some(bg) = self.background.get() {
                let bounds = graphene::Rect::new(0.0, 0.0, obj.width() as f32, obj.height() as f32);
                snapshot.append_color(&rgba(bg), &bounds);
            }
            let mut child = obj.first_child();
            while let Some(widget) = child {
                obj.snapshot_child(&widget, snapshot);
                child = widget.next_sibling();
            }
        }
    }
}

glib::wrapper! {
    /// A full-screen surface (the root content or a page): an opaque theme
    /// background with the node views placed at their frames.
    pub struct FrameHost(ObjectSubclass<host_imp::FrameHost>)
        @extends gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl Default for FrameHost {
    fn default() -> Self {
        let host: FrameHost = glib::Object::new();
        host.set_overflow(gtk::Overflow::Hidden);
        host
    }
}

impl FrameHost {
    pub fn add(&self, view: &NodeView) {
        view.detach();
        view.insert_before(self, None::<&gtk::Widget>);
        self.queue_allocate();
    }

    /// Lets go of the node views (their nodes are gone).
    pub fn clear(&self) {
        while let Some(child) = self.first_child() {
            child.unparent();
        }
    }

    pub fn set_background(&self, color: Option<u32>) {
        self.imp().background.set(color);
        self.queue_draw();
    }
}

/// `true` when the event is the platform's "back" input: Escape, Alt+Left
/// (Alt+Right in right-to-left locales) or the mouse back button is handled by
/// the callers; this checks the key part.
pub(crate) fn is_back_key(widget: &gtk::Widget, key: gdk::Key, state: gdk::ModifierType) -> bool {
    let mods = state & gtk::accelerator_get_default_mod_mask();
    let back_arrow = if widget.direction() == gtk::TextDirection::Rtl { gdk::Key::Right } else { gdk::Key::Left };
    (key == gdk::Key::Escape && mods.is_empty())
        || (mods == gdk::ModifierType::ALT_MASK && key == back_arrow)
        || key == gdk::Key::Back
}
