//! `Sheet` and `Dialog` nodes.
//!
//! A sheet slides up from the bottom edge as on iOS: a grabber, top corners
//! rounded, a dimmed backdrop, `medium` / `large` / fixed-height detents (the
//! content's own height when there are none). Dragging moves it between
//! detents, below the smallest one dismisses it. The core lays a sheet out at
//! the full window width, so it stays a bottom sheet on wide windows too.
//!
//! A dialog shows its content centered over a dimmed backdrop and scales in
//! as an iOS alert does. A dialog with an `AlertSpec` is a native
//! `AdwAlertDialog` instead.
//!
//! The user closes a sheet or dialog with Escape, a backdrop click or (sheets)
//! a drag down when `Dismissable`; it then hides at once and `Dismiss` is
//! sent. The core closes it by deleting the node; the views stay until the
//! exit animation ends.

use std::cell::{Cell, RefCell};
use std::rc::{Rc, Weak};

use adw::prelude::*;
use gtk::subclass::prelude::*;
use gtk::{glib, graphene, gsk};

use native_shell_core::protocol::{EventType, Prop, PropValue};
use native_shell_core::theme::Appearance;

use crate::component::{Component, NodeCx};
use crate::components::overlays::{is_back_key, spring, Presenter, Surface};
use crate::mount::json;
use crate::node_view::{rgba, rounded_rect, NodeView};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Detent {
    Medium,
    Large,
    Height(f32),
}

fn parse_detents(value: &serde_json::Value) -> Vec<Detent> {
    value
        .as_array()
        .map(|entries| {
            entries
                .iter()
                .filter_map(|d| match d {
                    serde_json::Value::String(s) if s == "medium" => Some(Detent::Medium),
                    serde_json::Value::String(s) if s == "large" => Some(Detent::Large),
                    serde_json::Value::Object(o) => {
                        o.get("height").and_then(|h| h.as_f64()).filter(|h| *h > 0.0).map(|h| Detent::Height(h as f32))
                    }
                    _ => None,
                })
                .collect()
        })
        .unwrap_or_default()
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ModalKind {
    Sheet,
    Dialog,
}

/// Gap above a sheet at the `large` detent, where the dimmed page stays visible.
const LARGE_TOP_GAP: f64 = 16.0;
const SHEET_RADIUS: f32 = 12.0;
const BACKDROP_ALPHA: f32 = 0.3;
/// Presentation spring response, seconds.
const RESPONSE: f64 = 0.4;

pub struct ModalState {
    pub node: u32,
    pub view: NodeView,
    pub kind: ModalKind,
    pub attached: Cell<bool>,
    pub dismissable: Cell<bool>,
    pub detents: RefCell<Vec<Detent>>,
    pub alert_spec: RefCell<Option<serde_json::Value>>,
    pub host: RefCell<Option<ModalHost>>,
    pub alert: RefCell<Option<adw::AlertDialog>>,
    /// Leaving the screen (by the user or the core); never shown again.
    pub closing: Cell<bool>,
    pub deleted: Cell<bool>,
    presenter: Weak<Presenter>,
}

impl ModalState {
    /// Shows the surface once it is attached and its batch is complete.
    pub fn present(self: &Rc<Self>, presenter: &Rc<Presenter>) {
        if !self.attached.get() || self.closing.get() {
            if !self.attached.get() && !self.closing.get() && (self.host.borrow().is_some() || self.alert.borrow().is_some()) {
                // Detached by the core without a delete: take it down.
                self.close(false);
            }
            return;
        }
        if let Some(spec) = self.alert_spec.borrow().clone() {
            if self.alert.borrow().is_none() {
                self.present_alert(presenter, &spec);
            }
            return;
        }
        if let Some(host) = self.host.borrow().as_ref() {
            host.queue_allocate();
            return;
        }
        let host = ModalHost::new(self);
        presenter.stage.add(&host);
        self.host.replace(Some(host.clone()));
        host.open();
    }

    fn present_alert(self: &Rc<Self>, presenter: &Presenter, spec: &serde_json::Value) {
        let text = |v: &serde_json::Value, key: &str| v.get(key).and_then(|t| t.as_str()).map(str::to_owned);
        let alert = adw::AlertDialog::new(text(spec, "title").as_deref(), text(spec, "message").as_deref());
        let actions = spec.get("actions").and_then(|a| a.as_array()).cloned().unwrap_or_default();
        // Escape answers with the cancel action, or with a plain dismissal.
        alert.set_close_response("dismiss");
        for (i, action) in actions.iter().enumerate() {
            let id = i.to_string();
            alert.add_response(&id, &text(action, "label").unwrap_or_else(|| "OK".into()));
            match action.get("style").and_then(|s| s.as_str()) {
                Some("destructive") => alert.set_response_appearance(&id, adw::ResponseAppearance::Destructive),
                Some("cancel") => alert.set_close_response(&id),
                _ => {}
            }
        }
        if actions.is_empty() {
            alert.add_response("dismiss", "OK");
        }
        let weak = Rc::downgrade(self);
        alert.connect_response(None, move |_, response| {
            let Some(state) = weak.upgrade() else { return };
            if state.closing.replace(true) {
                return;
            }
            match response.parse::<u32>() {
                Ok(index) => state.emit(EventType::Action, format!(r#"{{"index":{index}}}"#)),
                Err(_) => state.emit(EventType::Dismiss, "{}".into()),
            }
        });
        alert.present(Some(&presenter.stage));
        match_appearance(&alert, &presenter.stage, presenter.appearance() == Appearance::Dark);
        tint_alert(&alert, presenter);
        self.alert.replace(Some(alert));
    }

    fn emit(&self, event: EventType, payload: String) {
        if let Some(p) = self.presenter.upgrade() {
            p.emit(self.node, event, payload);
        }
    }

    /// On screen and not leaving.
    pub fn is_open(&self) -> bool {
        !self.closing.get() && (self.alert.borrow().is_some() || self.host.borrow().is_some())
    }

    /// System back: an alert answers as Escape would, a sheet or dialog
    /// closes when it is dismissable.
    pub fn back(&self) {
        let alert = self.alert.borrow().clone();
        match alert {
            Some(alert) => {
                alert.close();
            }
            None => self.user_dismiss(),
        }
    }

    /// The user asked to close it (Escape, backdrop, drag down).
    fn user_dismiss(&self) {
        if !self.dismissable.get() || self.closing.get() {
            return;
        }
        self.emit(EventType::Dismiss, "{}".into());
        self.close(true);
    }

    /// Takes it off screen; `animated` runs the exit animation first.
    fn close(&self, animated: bool) {
        self.closing.set(true);
        // What is below takes focus again before the host hands it back.
        if let Some(p) = self.presenter.upgrade() {
            p.update_focus();
        }
        if let Some(alert) = self.alert.take() {
            alert.force_close();
        }
        let host = self.host.borrow().clone();
        if let Some(host) = host {
            host.close(animated);
        }
    }

    /// The host finished leaving; the views go once the node is gone too.
    fn host_closed(&self) {
        let host = self.host.borrow().clone();
        if let Some(host) = host {
            host.unparent();
        }
        self.release_if_done();
    }

    fn release_if_done(&self) {
        let host_gone = self.host.borrow().as_ref().is_none_or(|h| h.parent().is_none());
        if self.deleted.get() && host_gone {
            if let Some(host) = self.host.take() {
                host.clear();
            }
            if let Some(p) = self.presenter.upgrade() {
                p.unregister(&self.view);
            }
        }
    }

    pub fn theme_changed(&self) {
        if let Some(host) = self.host.borrow().as_ref() {
            host.queue_draw();
        }
    }

    fn sheet_background(&self) -> u32 {
        self.presenter.upgrade().map_or(0xFFFF_FFFF, |p| p.color("background"))
    }

    /// The border color in the dark appearance, `None` in the light one.
    fn dark_edge_color(&self) -> Option<u32> {
        let p = self.presenter.upgrade()?;
        (p.appearance() == Appearance::Dark).then(|| p.color("border"))
    }

    fn grabber_color(&self) -> u32 {
        // The muted foreground at reduced opacity, like the system grabber.
        let c = self.presenter.upgrade().map_or(0xFF8A_8A8E, |p| p.color("muted_foreground"));
        (c & 0x00FF_FFFF) | 0x6600_0000
    }
}

/// libadwaita's light and dark palettes, for an alert whose program shows the
/// other appearance than the desktop: alerts follow the program, as
/// `UIAlertController` follows its view controller's interface style.
const APPEARANCE_CSS: &str = "
.nsh-appearance-dark {
  --window-bg-color: #222226; --window-fg-color: #ffffff;
  --view-bg-color: #1d1d20; --view-fg-color: #ffffff;
  --card-bg-color: rgb(255 255 255 / 8%); --card-fg-color: #ffffff;
  --dialog-bg-color: #36363a; --dialog-fg-color: #ffffff;
  --popover-bg-color: #36363a; --popover-fg-color: #ffffff;
  --shade-color: rgb(0 0 6 / 25%);
  --destructive-color: #ff938c; --destructive-bg-color: #c01c28;
}
.nsh-appearance-light {
  --window-bg-color: #fafafb; --window-fg-color: rgb(0 0 6 / 80%);
  --view-bg-color: #ffffff; --view-fg-color: rgb(0 0 6 / 80%);
  --card-bg-color: #ffffff; --card-fg-color: rgb(0 0 6 / 80%);
  --dialog-bg-color: #fafafb; --dialog-fg-color: rgb(0 0 6 / 80%);
  --popover-bg-color: #ffffff; --popover-fg-color: rgb(0 0 6 / 80%);
  --shade-color: rgb(0 0 6 / 7%);
  --destructive-color: #c30000; --destructive-bg-color: #e01b24;
}";

fn match_appearance(alert: &adw::AlertDialog, stage: &impl IsA<gtk::Widget>, dark: bool) {
    if adw::StyleManager::default().is_dark() == dark {
        return;
    }
    thread_local! {
        static INSTALLED: Cell<bool> = const { Cell::new(false) };
    }
    if !INSTALLED.with(|i| i.replace(true)) {
        let provider = gtk::CssProvider::new();
        provider.load_from_string(APPEARANCE_CSS);
        gtk::style_context_add_provider_for_display(&alert.display(), &provider, gtk::STYLE_PROVIDER_PRIORITY_APPLICATION);
    }
    let class = if dark { "nsh-appearance-dark" } else { "nsh-appearance-light" };
    alert.add_css_class(class);
    // Outside an AdwWindow the alert gets a window of its own, which paints its background.
    if let Some(window) = alert.root().and_downcast::<gtk::Window>() {
        if stage.as_ref().root().as_ref() != Some(window.upcast_ref::<gtk::Root>()) {
            window.add_css_class(class);
        }
    }
}

/// The program's tint and destructive color on the alert's buttons, as
/// `UIAlertController` takes the app's tint and system red.
fn tint_alert(alert: &adw::AlertDialog, presenter: &Presenter) {
    thread_local! {
        static PROVIDER: RefCell<Option<gtk::CssProvider>> = const { RefCell::new(None) };
    }
    let css = |token: &str| {
        let c = presenter.color(token);
        format!("rgba({}, {}, {}, {:.3})", (c >> 16) & 0xff, (c >> 8) & 0xff, c & 0xff, ((c >> 24) & 0xff) as f32 / 255.0)
    };
    let rules = format!(
        ".nsh-alert {{ --accent-color: {}; --accent-bg-color: {}; --accent-fg-color: {}; \
         --destructive-color: {}; --destructive-bg-color: {}; --destructive-fg-color: {}; }}",
        css("primary"),
        css("primary"),
        css("primary_foreground"),
        css("destructive"),
        css("destructive"),
        css("destructive_foreground"),
    );
    PROVIDER.with(|p| {
        let mut p = p.borrow_mut();
        let provider = p.get_or_insert_with(|| {
            let provider = gtk::CssProvider::new();
            // Above the appearance palette, which sets the destructive colors too.
            gtk::style_context_add_provider_for_display(&alert.display(), &provider, gtk::STYLE_PROVIDER_PRIORITY_APPLICATION + 1);
            provider
        });
        provider.load_from_string(&rules);
    });
    alert.add_css_class("nsh-alert");
}

pub struct ModalComponent {
    state: Rc<ModalState>,
}

pub fn create(cx: &NodeCx, kind: ModalKind) -> Box<dyn Component> {
    let presenter = cx.shared.overlays.presenter();
    let state = Rc::new(ModalState {
        node: cx.node,
        view: cx.view.clone(),
        kind,
        attached: Cell::new(false),
        dismissable: Cell::new(true),
        detents: RefCell::default(),
        alert_spec: RefCell::default(),
        host: RefCell::default(),
        alert: RefCell::default(),
        closing: Cell::new(false),
        deleted: Cell::new(false),
        presenter: presenter.as_ref().map(Rc::downgrade).unwrap_or_default(),
    });
    if let Some(p) = &presenter {
        p.register(cx.view, Surface::Modal(state.clone()));
    }
    Box::new(ModalComponent { state })
}

impl Component for ModalComponent {
    fn set_prop(&mut self, _cx: &NodeCx, prop: Prop, value: &PropValue) -> bool {
        let state = &self.state;
        match prop {
            Prop::Dismissable => state.dismissable.set(!matches!(value, PropValue::Bool(false))),
            Prop::Detents => {
                state.detents.replace(json(value).map(|v| parse_detents(&v)).unwrap_or_default());
            }
            Prop::AlertSpec => {
                state.alert_spec.replace(json(value));
            }
            _ => return false,
        }
        if let Some(p) = state.presenter.upgrade() {
            p.mark_modals_changed();
        }
        true
    }

    fn set_frame(&mut self, _cx: &NodeCx, _frame: native_shell_core::protocol::Frame) {
        // The node view asked its parent (the host) to place it again.
        if let Some(host) = self.state.host.borrow().as_ref() {
            host.queue_allocate();
        }
    }

    fn unmount(&mut self, _cx: &NodeCx) {
        let state = &self.state;
        state.deleted.set(true);
        if !state.closing.get() {
            state.close(true);
        }
        let host_open = state.host.borrow().as_ref().is_some_and(|h| h.parent().is_some());
        if !host_open {
            state.release_if_done();
        }
    }

    fn keeps_views_on_delete(&self) -> bool {
        true
    }
}

mod imp {
    use super::*;

    #[derive(Default)]
    pub struct ModalHost {
        pub state: RefCell<Weak<ModalState>>,
        /// Sheet: the card's top edge. Dialog: presentation progress, 0 to 1.
        pub position: Cell<f64>,
        /// Sheet: the detent it rests at (index into the resolved heights).
        pub detent: Cell<usize>,
        /// Sheet: card top when the drag began, while dragging.
        pub drag_from: Cell<Option<f64>>,
        pub animation: RefCell<Option<adw::SpringAnimation>>,
        pub opened: Cell<bool>,
        pub closing: Cell<bool>,
        /// What had the keyboard focus before, given back on close.
        pub previous_focus: RefCell<Option<glib::WeakRef<gtk::Widget>>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for ModalHost {
        const NAME: &'static str = "NshModalHost";
        type Type = super::ModalHost;
        type ParentType = gtk::Widget;
    }

    impl ObjectImpl for ModalHost {
        fn dispose(&self) {
            while let Some(child) = self.obj().first_child() {
                child.unparent();
            }
        }
    }

    impl WidgetImpl for ModalHost {
        fn measure(&self, _orientation: gtk::Orientation, _for_size: i32) -> (i32, i32, i32, i32) {
            (0, 0, -1, -1)
        }

        fn size_allocate(&self, width: i32, height: i32, _baseline: i32) {
            let obj = self.obj();
            let Some(state) = self.state.borrow().upgrade() else { return };
            let Some(frame) = state.view.frame() else { return };
            let (w, h) = (width as f32, height as f32);
            let transform = match state.kind {
                ModalKind::Sheet => {
                    if !self.opened.get() {
                        self.position.set(height as f64);
                    } else if self.animation.borrow().is_none() && self.drag_from.get().is_none() && !self.closing.get() {
                        // At rest: follow the window and content size.
                        self.position.set(obj.rest_top(self.detent.get()));
                    }
                    gsk::Transform::new().translate(&graphene::Point::new(0.0, self.position.get() as f32))
                }
                ModalKind::Dialog => {
                    let p = self.position.get() as f32;
                    let scale = 1.1 - 0.1 * p;
                    let (cx, cy) = (w / 2.0, h / 2.0);
                    gsk::Transform::new()
                        .translate(&graphene::Point::new(cx, cy))
                        .scale(scale, scale)
                        .translate(&graphene::Point::new(-frame.width / 2.0, -frame.height / 2.0))
                }
            };
            state.view.set_opacity(match state.kind {
                ModalKind::Dialog => self.position.get().clamp(0.0, 1.0),
                ModalKind::Sheet => 1.0,
            });
            state.view.allocate(frame.width.round() as i32, frame.height.round() as i32, -1, Some(transform));
        }

        fn snapshot(&self, snapshot: &gtk::Snapshot) {
            let obj = self.obj();
            let Some(state) = self.state.borrow().upgrade() else { return };
            let (w, h) = (obj.width() as f32, obj.height() as f32);
            let bounds = graphene::Rect::new(0.0, 0.0, w, h);
            let shown = obj.shown_fraction() as f32;
            snapshot.append_color(&gtk::gdk::RGBA::new(0.0, 0.0, 0.0, BACKDROP_ALPHA * shown), &bounds);
            match state.kind {
                ModalKind::Sheet => {
                    let top = self.position.get() as f32;
                    // The card runs past the bottom edge so a drag up shows no gap.
                    let card = graphene::Rect::new(0.0, top, w, (h - top).max(0.0) + 64.0);
                    let r = SHEET_RADIUS;
                    let rounded = rounded_rect(card, [r, r, 0.0, 0.0]);
                    snapshot.push_rounded_clip(&rounded);
                    snapshot.append_color(&rgba(state.sheet_background()), &card);
                    obj.snapshot_child(&state.view, snapshot);
                    let grabber = graphene::Rect::new((w - 36.0) / 2.0, top + 5.0, 36.0, 5.0);
                    snapshot.push_rounded_clip(&rounded_rect(grabber, [2.5; 4]));
                    snapshot.append_color(&rgba(state.grabber_color()), &grabber);
                    snapshot.pop();
                    snapshot.pop();
                    if let Some(edge) = state.dark_edge_color() {
                        // Dimming does not show on a dark page: a hairline marks the sheet's edge.
                        snapshot.append_border(&rounded, &[1.0, 0.0, 0.0, 0.0], &[rgba(edge); 4]);
                    }
                }
                ModalKind::Dialog => {
                    obj.snapshot_child(&state.view, snapshot);
                    // As for sheets: the card's edge on a dimmed dark page. The
                    // card is the dialog's content view, which has the corners.
                    let card = state.view.first_child().and_downcast::<NodeView>().unwrap_or_else(|| state.view.clone());
                    if let (Some(edge), Some(bounds)) = (state.dark_edge_color(), card.compute_bounds(&*obj)) {
                        let c = rgba(edge);
                        let c = gtk::gdk::RGBA::new(c.red(), c.green(), c.blue(), c.alpha() * shown);
                        snapshot.append_border(&rounded_rect(bounds, card.style().radii), &[1.0; 4], &[c; 4]);
                    }
                }
            }
        }
    }
}

glib::wrapper! {
    /// The full-window layer of one sheet or dialog: backdrop plus content.
    pub struct ModalHost(ObjectSubclass<imp::ModalHost>)
        @extends gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl ModalHost {
    fn new(state: &Rc<ModalState>) -> ModalHost {
        let host: ModalHost = glib::Object::builder()
            .property("accessible-role", gtk::AccessibleRole::Dialog)
            .property("focusable", true)
            .build();
        host.imp().state.replace(Rc::downgrade(state));
        host.update_property(&[gtk::accessible::Property::Modal(true)]);
        state.view.detach();
        state.view.set_parent(&host);
        host.install_input(state);
        host
    }

    fn state(&self) -> Option<Rc<ModalState>> {
        self.imp().state.borrow().upgrade()
    }

    fn install_input(&self, state: &Rc<ModalState>) {
        let keys = gtk::EventControllerKey::new();
        let weak = Rc::downgrade(state);
        keys.connect_key_pressed(move |controller, key, _, modifiers| {
            let (Some(state), Some(widget)) = (weak.upgrade(), controller.widget()) else {
                return glib::Propagation::Proceed;
            };
            if key == gtk::gdk::Key::Escape && is_back_key(&widget, key, modifiers) {
                state.user_dismiss();
                return glib::Propagation::Stop;
            }
            glib::Propagation::Proceed
        });
        self.add_controller(keys);

        let click = gtk::GestureClick::new();
        let weak = Rc::downgrade(state);
        click.connect_released(move |gesture, _, x, y| {
            let Some(state) = weak.upgrade() else { return };
            let Some(host) = gesture.widget().and_downcast::<ModalHost>() else { return };
            if !host.content_contains(x, y) {
                state.user_dismiss();
            }
        });
        self.add_controller(click);

        if state.kind == ModalKind::Sheet {
            let drag = gtk::GestureDrag::new();
            drag.connect_drag_update(|gesture, _dx, dy| {
                let Some(host) = gesture.widget().and_downcast::<ModalHost>() else { return };
                host.drag_update(gesture, dy);
            });
            drag.connect_drag_end(|gesture, _dx, dy| {
                let Some(host) = gesture.widget().and_downcast::<ModalHost>() else { return };
                host.drag_end(dy);
            });
            self.add_controller(drag);
        }
    }

    /// The point is on the sheet card or the dialog content.
    fn content_contains(&self, x: f64, y: f64) -> bool {
        let Some(state) = self.state() else { return false };
        match state.kind {
            ModalKind::Sheet => y >= self.imp().position.get(),
            ModalKind::Dialog => state.view.compute_bounds(self).is_some_and(|b| b.contains_point(&graphene::Point::new(x as f32, y as f32))),
        }
    }

    /// Resolved detent heights, ascending, each at most the `large` height.
    fn detent_heights(&self) -> Vec<f64> {
        let Some(state) = self.state() else { return vec![] };
        let h = self.stage_height();
        let large = (h - LARGE_TOP_GAP).max(0.0);
        let content = state.view.frame().map_or(large, |f| f.height as f64);
        let detents = state.detents.borrow();
        let mut heights: Vec<f64> = if detents.is_empty() {
            vec![content.min(large)]
        } else {
            detents
                .iter()
                .map(|d| match d {
                    Detent::Medium => (h / 2.0).min(large),
                    Detent::Large => large,
                    Detent::Height(px) => (*px as f64).min(large),
                })
                .collect()
        };
        heights.sort_by(f64::total_cmp);
        heights.dedup();
        heights
    }

    /// Card top at rest at detent `index`.
    fn rest_top(&self, index: usize) -> f64 {
        let heights = self.detent_heights();
        let height = heights.get(index.min(heights.len().saturating_sub(1))).copied().unwrap_or(0.0);
        self.stage_height() - height
    }

    /// The window area it covers; known before its own first allocation.
    fn stage_height(&self) -> f64 {
        let own = self.height();
        let h = if own > 0 { own } else { self.parent().map_or(0, |p| p.height()) };
        h as f64
    }

    /// How far it is presented, 0 (gone) to 1, for the backdrop.
    fn shown_fraction(&self) -> f64 {
        let Some(state) = self.state() else { return 0.0 };
        match state.kind {
            ModalKind::Dialog => self.imp().position.get().clamp(0.0, 1.0),
            ModalKind::Sheet => {
                let h = self.stage_height();
                let smallest = self.detent_heights().first().copied().unwrap_or(1.0).max(1.0);
                ((h - self.imp().position.get()) / smallest).clamp(0.0, 1.0)
            }
        }
    }

    fn animate_to(&self, to: f64, response: f64, done: impl Fn(&ModalHost) + 'static) {
        let imp = self.imp();
        if let Some(old) = imp.animation.take() {
            old.pause();
        }
        let weak = self.downgrade();
        let animation = spring(self, imp.position.get(), to, response, move |v| {
            if let Some(host) = weak.upgrade() {
                host.imp().position.set(v);
                host.queue_allocate();
                host.queue_draw();
            }
        });
        let weak = self.downgrade();
        animation.connect_done(move |_| {
            if let Some(host) = weak.upgrade() {
                host.imp().animation.replace(None);
                done(&host);
            }
        });
        imp.animation.replace(Some(animation.clone()));
        animation.play();
    }

    fn open(&self) {
        let imp = self.imp();
        imp.previous_focus.replace(self.root().and_then(|r| r.focus()).map(|w| w.downgrade()));
        let Some(state) = self.state() else { return };
        let target = match state.kind {
            ModalKind::Sheet => {
                imp.position.set(self.stage_height());
                self.rest_top(0)
            }
            ModalKind::Dialog => {
                imp.position.set(0.0);
                1.0
            }
        };
        imp.opened.set(true);
        self.grab_focus();
        self.animate_to(target, RESPONSE, |_| {});
    }

    fn close(&self, animated: bool) {
        let imp = self.imp();
        if imp.closing.replace(true) {
            return;
        }
        let to = match self.state().map(|s| s.kind) {
            Some(ModalKind::Sheet) => self.stage_height(),
            _ => 0.0,
        };
        self.give_back_focus();
        // Clicks during the exit reach what is below.
        self.set_can_target(false);
        let finish = |host: &ModalHost| {
            if let Some(state) = host.state() {
                state.host_closed();
            }
        };
        if animated && self.is_mapped() {
            self.animate_to(to, RESPONSE * 0.75, finish);
        } else {
            finish(self);
        }
    }

    fn give_back_focus(&self) {
        let previous = self.imp().previous_focus.take().and_then(|w| w.upgrade());
        let focus_inside = self.root().and_then(|r| r.focus()).is_some_and(|f| f.is_ancestor(self) || &f == self.upcast_ref::<gtk::Widget>());
        if focus_inside {
            if let Some(widget) = previous {
                widget.grab_focus();
            }
        }
    }

    fn drag_update(&self, gesture: &gtk::GestureDrag, dy: f64) {
        let imp = self.imp();
        if imp.closing.get() {
            return;
        }
        if imp.drag_from.get().is_none() {
            // Taps and small moves stay with the content.
            if dy.abs() < 8.0 {
                return;
            }
            let Some((_, y)) = gesture.start_point() else { return };
            if y < imp.position.get() {
                return;
            }
            gesture.set_state(gtk::EventSequenceState::Claimed);
            if let Some(a) = imp.animation.take() {
                a.pause();
            }
            imp.drag_from.set(Some(imp.position.get() - dy));
        }
        let Some(from) = imp.drag_from.get() else { return };
        let h = self.stage_height();
        let highest = h - self.detent_heights().last().copied().unwrap_or(0.0);
        let mut top = from + dy;
        if top < highest {
            // Past the largest detent it resists, like a rubber band.
            let over = highest - top;
            top = highest - over / (1.0 + over / 60.0);
        }
        imp.position.set(top);
        self.queue_allocate();
        self.queue_draw();
    }

    fn drag_end(&self, _dy: f64) {
        let imp = self.imp();
        if imp.drag_from.take().is_none() {
            return;
        }
        let Some(state) = self.state() else { return };
        let h = self.stage_height();
        let shown = h - imp.position.get();
        let heights = self.detent_heights();
        let smallest = heights.first().copied().unwrap_or(0.0);
        if shown < smallest * 0.6 && state.dismissable.get() {
            state.user_dismiss();
            return;
        }
        let nearest = heights
            .iter()
            .enumerate()
            .min_by(|a, b| (a.1 - shown).abs().total_cmp(&(b.1 - shown).abs()))
            .map_or(0, |(i, _)| i);
        imp.detent.set(nearest);
        self.animate_to(h - heights.get(nearest).copied().unwrap_or(0.0), RESPONSE, |_| {});
    }

    fn clear(&self) {
        while let Some(child) = self.first_child() {
            child.unparent();
        }
    }
}

/// Programmatic user input for tests.
#[cfg(test)]
impl ModalState {
    pub fn test_user_dismiss(&self) {
        self.user_dismiss();
    }

    pub fn test_drag(&self, dy: f64) {
        if let Some(host) = self.host.borrow().as_ref() {
            let imp = host.imp();
            imp.drag_from.set(Some(imp.position.get()));
            imp.position.set(imp.position.get() + dy);
            host.drag_end(dy);
        }
    }

    pub fn test_top(&self) -> Option<f64> {
        self.host.borrow().as_ref().map(|h| h.imp().position.get())
    }
}
