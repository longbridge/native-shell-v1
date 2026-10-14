//! Toasts at the bottom of the surface, as gpui-shell shows them: at most
//! three visible (stacked, newest at the bottom), the rest queued in order. A
//! new toast never retracts an earlier one; `duration` 0 keeps a toast until
//! it is dismissed. The card follows the iOS renderer: a neutral surface with
//! a hairline border, the level shown by a leading symbol whose shape differs
//! per level (never by color alone), an optional trailing text action.

use std::cell::{Cell, RefCell};
use std::rc::{Rc, Weak};
use std::time::Duration;

use adw::prelude::*;
use gtk::subclass::prelude::*;
use gtk::{glib, graphene, gsk, pango};

use native_shell_core::protocol::{EventType, ROOT_NODE};

use crate::components::overlays::{spring, Presenter, Stage};
use crate::node_view::{rgba, rounded_rect, Shadow};

const MAX_VISIBLE: usize = 3;
const MAX_WIDTH: f32 = 420.0;
const MARGIN: f32 = 16.0;
const GAP: f32 = 8.0;
const RADIUS: f32 = 8.0;

#[derive(Debug, Clone, Copy, PartialEq)]
enum Level {
    Info,
    Success,
    Warning,
    Error,
}

impl Level {
    fn parse(s: Option<&str>) -> Level {
        match s {
            Some("success") => Level::Success,
            Some("warning") => Level::Warning,
            Some("error") => Level::Error,
            _ => Level::Info,
        }
    }

    fn icon(self) -> &'static str {
        match self {
            Level::Info => "dialog-information-symbolic",
            Level::Success => "emblem-ok-symbolic",
            Level::Warning => "dialog-warning-symbolic",
            Level::Error => "dialog-error-symbolic",
        }
    }

    fn name(self) -> &'static str {
        match self {
            Level::Info => "Info",
            Level::Success => "Success",
            Level::Warning => "Warning",
            Level::Error => "Error",
        }
    }

    /// The symbol's tint. Success and warning use the iOS system green and
    /// orange, which the theme has no token for.
    fn tint(self, presenter: &Presenter, dark: bool) -> u32 {
        match self {
            Level::Info => presenter.color("primary", 0xFF17_1717),
            Level::Success => if dark { 0xFF30_D158 } else { 0xFF34_C759 },
            Level::Warning => if dark { 0xFFFF_9F0A } else { 0xFFFF_9500 },
            Level::Error => presenter.color("destructive", 0xFFEF_4444),
        }
    }
}

/// One toast card.
pub struct Toast {
    pub id: String,
    level: Level,
    card: ToastCard,
    icon: TintedIcon,
    title: gtk::Label,
    description: gtk::Label,
    action: Option<gtk::Button>,
    /// Milliseconds; 0 = until dismissed.
    duration: u32,
}

pub struct ToastCenter {
    layer: ToastLayer,
    visible: RefCell<Vec<Rc<Toast>>>,
    queued: RefCell<Vec<Rc<Toast>>>,
}

impl ToastCenter {
    pub fn new(stage: &Stage) -> ToastCenter {
        let layer = ToastLayer::default();
        layer.insert_before(stage, None::<&gtk::Widget>);
        ToastCenter { layer, visible: RefCell::default(), queued: RefCell::default() }
    }

    /// Connects the layer to the presenter that owns this center.
    pub fn attach(&self, presenter: &Rc<Presenter>) {
        self.layer.imp().presenter.replace(Rc::downgrade(presenter));
    }

    pub fn show(&self, presenter: &Rc<Presenter>, payload: &serde_json::Value) {
        let toast = Rc::new(build(presenter, payload));
        if self.visible.borrow().len() < MAX_VISIBLE {
            self.present(presenter, toast);
        } else {
            self.queued.borrow_mut().push(toast);
        }
    }

    /// Retracts toast `id` (visible or queued), or every toast when `None`.
    pub fn dismiss(&self, id: Option<&str>) {
        let Some(id) = id else {
            self.queued.borrow_mut().clear();
            let all: Vec<_> = self.visible.borrow().clone();
            for t in all {
                self.retract(&t);
            }
            return;
        };
        self.queued.borrow_mut().retain(|t| t.id != id);
        let matching: Vec<_> = self.visible.borrow().iter().filter(|t| t.id == id).cloned().collect();
        for t in matching {
            self.retract(&t);
        }
    }

    pub fn theme_changed(&self, presenter: &Presenter) {
        for t in self.visible.borrow().iter().chain(self.queued.borrow().iter()) {
            apply_colors(t, presenter);
        }
    }

    fn present(&self, presenter: &Rc<Presenter>, toast: Rc<Toast>) {
        if let Some(button) = &toast.action {
            let weak = Rc::downgrade(presenter);
            let t = Rc::downgrade(&toast);
            button.connect_clicked(move |_| {
                let (Some(p), Some(t)) = (weak.upgrade(), t.upgrade()) else { return };
                p.toasts.retract(&t);
                p.emit(ROOT_NODE, EventType::ToastAction, serde_json::json!({ "id": t.id }).to_string());
            });
        }
        toast.card.set_opacity(0.0);
        toast.card.insert_before(&self.layer, None::<&gtk::Widget>);
        self.visible.borrow_mut().push(toast.clone());
        self.relayout();
        let card = toast.card.clone();
        spring(&toast.card, 0.0, 1.0, 0.25, move |v| card.set_opacity(v)).play();
        if toast.duration > 0 {
            let weak = Rc::downgrade(presenter);
            let t = Rc::downgrade(&toast);
            glib::timeout_add_local_once(Duration::from_millis(toast.duration as u64), move || {
                if let (Some(p), Some(t)) = (weak.upgrade(), t.upgrade()) {
                    p.toasts.retract(&t);
                }
            });
        }
    }

    fn retract(&self, toast: &Rc<Toast>) {
        let removed = {
            let mut visible = self.visible.borrow_mut();
            let before = visible.len();
            visible.retain(|t| !Rc::ptr_eq(t, toast));
            visible.len() != before
        };
        if !removed {
            return;
        }
        let card = toast.card.clone();
        card.set_can_target(false);
        let fade = spring(&toast.card, card.opacity(), 0.0, 0.2, {
            let card = card.clone();
            move |v| card.set_opacity(v)
        });
        fade.connect_done(move |_| card.unparent());
        fade.play();
        let next = {
            let mut queued = self.queued.borrow_mut();
            (!queued.is_empty()).then(|| queued.remove(0))
        };
        if let Some(next) = next {
            if let Some(p) = self.layer.presenter() {
                self.present(&p, next);
            }
        }
        self.relayout();
    }

    /// Stacks the visible toasts upward from the bottom edge, sliding the
    /// ones that move.
    fn relayout(&self) {
        let (width, height) = (self.layer.width() as f32, self.layer.height() as f32);
        if width > 0.0 {
            self.place(width, height, true);
        }
        self.layer.queue_allocate();
    }

    fn place(&self, width: f32, height: f32, animated: bool) {
        let card_width = card_width(width);
        let mut bottom = height - MARGIN;
        for t in self.visible.borrow().iter().rev() {
            let (min_h, _, _, _) = t.card.measure(gtk::Orientation::Vertical, card_width as i32);
            let target = bottom - min_h as f32;
            bottom = target - GAP;
            let imp = t.card.imp();
            let first = !imp.placed.replace(true);
            if first || !animated || (imp.y.get() - target).abs() < 0.5 {
                imp.y.set(target);
                continue;
            }
            let card = t.card.downgrade();
            let animation = spring(&t.card, imp.y.get() as f64, target as f64, 0.3, move |v| {
                if let Some(card) = card.upgrade() {
                    card.imp().y.set(v as f32);
                    if let Some(layer) = card.parent() {
                        layer.queue_allocate();
                    }
                }
            });
            animation.play();
            imp.animation.replace(Some(animation));
        }
    }

    #[cfg(test)]
    pub fn visible_ids(&self) -> Vec<String> {
        self.visible.borrow().iter().map(|t| t.id.clone()).collect()
    }

    #[cfg(test)]
    pub fn queued_ids(&self) -> Vec<String> {
        self.queued.borrow().iter().map(|t| t.id.clone()).collect()
    }

    #[cfg(test)]
    pub fn tap_action(&self, id: &str) {
        let t = self.visible.borrow().iter().find(|t| t.id == id).cloned();
        if let Some(button) = t.and_then(|t| t.action.clone()) {
            button.emit_clicked();
        }
    }
}

fn card_width(layer_width: f32) -> f32 {
    (layer_width - 2.0 * MARGIN).clamp(0.0, MAX_WIDTH)
}

fn build(presenter: &Presenter, payload: &serde_json::Value) -> Toast {
    let text = |key: &str| payload.get(key).and_then(|v| v.as_str()).filter(|s| !s.is_empty()).map(str::to_owned);
    let id = payload.get("id").map(|v| v.as_str().map_or_else(|| v.to_string(), str::to_owned)).unwrap_or_default();
    let ms = payload.get("duration").and_then(|d| d.as_f64()).unwrap_or(3000.0);
    let duration = if ms <= 0.0 { 0 } else { ms.max(500.0) as u32 };
    let level = Level::parse(payload.get("level").and_then(|l| l.as_str()));

    let icon = TintedIcon::new(level.icon(), 20);
    icon.set_valign(gtk::Align::Start);
    icon.set_margin_top(1);
    let title = label(text("title"), 15.0, pango::Weight::Medium, 2);
    let description = label(text("description"), 13.0, pango::Weight::Normal, 3);
    let texts = gtk::Box::new(gtk::Orientation::Vertical, 2);
    texts.set_hexpand(true);
    texts.set_valign(gtk::Align::Center);
    texts.append(&title);
    texts.append(&description);

    let row = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    row.set_margin_start(14);
    row.set_margin_end(16);
    row.set_margin_top(12);
    row.set_margin_bottom(12);
    row.append(&icon);
    row.append(&texts);

    let action = payload.get("action").and_then(|a| a.get("label")).and_then(|l| l.as_str()).filter(|l| !l.is_empty()).map(|l| {
        let button = gtk::Button::new();
        button.add_css_class("flat");
        button.set_child(Some(&label(Some(l.to_owned()), 15.0, pango::Weight::Medium, 1)));
        button.set_valign(gtk::Align::Center);
        // A 44 pt touch target.
        button.set_size_request(44, 44);
        button.set_widget_name("nsh-toast-action");
        row.set_margin_end(4);
        row.append(&button);
        button
    });

    let card = ToastCard::default();
    row.set_parent(&card);
    card.set_widget_name("nsh-toast");
    let summary: Vec<String> = std::iter::once(level.name().to_owned()).chain(text("title")).chain(text("description")).collect();
    card.update_property(&[gtk::accessible::Property::Label(&summary.join(". "))]);

    let toast = Toast {
        id,
        level,
        card,
        icon,
        title,
        description,
        action,
        duration,
    };
    apply_colors(&toast, presenter);
    toast
}

fn label(text: Option<String>, size: f64, weight: pango::Weight, lines: i32) -> gtk::Label {
    let label = gtk::Label::new(text.as_deref());
    label.set_visible(text.is_some());
    label.set_xalign(0.0);
    label.set_wrap(true);
    label.set_wrap_mode(pango::WrapMode::WordChar);
    label.set_lines(lines);
    label.set_ellipsize(pango::EllipsizeMode::End);
    let attrs = pango::AttrList::new();
    attrs.insert(pango::AttrSize::new_size_absolute((size * pango::SCALE as f64) as i32));
    attrs.insert(pango::AttrInt::new_weight(weight));
    label.set_attributes(Some(&attrs));
    label
}

fn with_color(label: &gtk::Label, color: u32) {
    let attrs = label.attributes().unwrap_or_default();
    let c = |shift: u32| (((color >> shift) & 0xff) as u16) * 257;
    attrs.change(pango::AttrColor::new_foreground(c(16), c(8), c(0)));
    attrs.change(pango::AttrInt::new_foreground_alpha(c(24)));
    label.set_attributes(Some(&attrs));
}

fn apply_colors(t: &Toast, presenter: &Presenter) {
    let dark = presenter.shared.upgrade().is_some_and(|s| s.theme.borrow().appearance == native_shell_core::theme::Appearance::Dark);
    let imp = t.card.imp();
    imp.background.set(presenter.color("background", 0xFFFF_FFFF));
    imp.border.set(presenter.color("border", 0xFFE5_E5E5));
    t.card.queue_draw();
    t.icon.set_color(t.level.tint(presenter, dark));
    with_color(&t.title, presenter.color("foreground", 0xFF0A_0A0A));
    with_color(&t.description, presenter.color("muted_foreground", 0xFF73_7373));
    if let Some(label) = t.action.as_ref().and_then(|b| b.child()).and_downcast::<gtk::Label>() {
        with_color(&label, presenter.color("foreground", 0xFF0A_0A0A));
    }
}

mod card_imp {
    use super::*;

    #[derive(Default)]
    pub struct ToastCard {
        pub background: Cell<u32>,
        pub border: Cell<u32>,
        /// Top edge in the layer, animated when the stack moves.
        pub y: Cell<f32>,
        pub placed: Cell<bool>,
        pub animation: RefCell<Option<adw::SpringAnimation>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for ToastCard {
        const NAME: &'static str = "NshToastCard";
        type Type = super::ToastCard;
        type ParentType = gtk::Widget;

        fn class_init(klass: &mut Self::Class) {
            klass.set_layout_manager_type::<gtk::BinLayout>();
            klass.set_accessible_role(gtk::AccessibleRole::Alert);
        }
    }

    impl ObjectImpl for ToastCard {
        fn dispose(&self) {
            while let Some(child) = self.obj().first_child() {
                child.unparent();
            }
        }
    }

    impl WidgetImpl for ToastCard {
        fn snapshot(&self, snapshot: &gtk::Snapshot) {
            let obj = self.obj();
            let bounds = graphene::Rect::new(0.0, 0.0, obj.width() as f32, obj.height() as f32);
            let rounded = rounded_rect(bounds, [RADIUS; 4]);
            let shadow = Shadow { color: 0x1400_0000, x: 0.0, y: 2.0, blur: 16.0, spread: 0.0 };
            snapshot.append_outset_shadow(&rounded, &rgba(shadow.color), shadow.x, shadow.y, shadow.spread, shadow.blur);
            snapshot.push_rounded_clip(&rounded);
            snapshot.append_color(&rgba(self.background.get()), &bounds);
            snapshot.pop();
            let border = rgba(self.border.get());
            snapshot.append_border(&rounded, &[1.0; 4], &[border; 4]);
            let mut child = obj.first_child();
            while let Some(widget) = child {
                obj.snapshot_child(&widget, snapshot);
                child = widget.next_sibling();
            }
        }
    }
}

glib::wrapper! {
    pub struct ToastCard(ObjectSubclass<card_imp::ToastCard>)
        @extends gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl Default for ToastCard {
    fn default() -> Self {
        glib::Object::new()
    }
}

mod layer_imp {
    use super::*;

    #[derive(Default)]
    pub struct ToastLayer {
        pub presenter: RefCell<Weak<Presenter>>,
        pub size: Cell<(i32, i32)>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for ToastLayer {
        const NAME: &'static str = "NshToastLayer";
        type Type = super::ToastLayer;
        type ParentType = gtk::Widget;
    }

    impl ObjectImpl for ToastLayer {
        fn dispose(&self) {
            while let Some(child) = self.obj().first_child() {
                child.unparent();
            }
        }
    }

    impl WidgetImpl for ToastLayer {
        fn measure(&self, _orientation: gtk::Orientation, _for_size: i32) -> (i32, i32, i32, i32) {
            (0, 0, -1, -1)
        }

        fn size_allocate(&self, width: i32, height: i32, _baseline: i32) {
            let obj = self.obj();
            let Some(presenter) = obj.presenter() else { return };
            let size = (width, height);
            let unplaced = presenter.toasts.visible.borrow().iter().any(|t| !t.card.imp().placed.get());
            if self.size.replace(size) != size || unplaced {
                presenter.toasts.place(width as f32, height as f32, false);
            }
            let card_width = card_width(width as f32);
            let mut child = obj.first_child();
            while let Some(widget) = child {
                child = widget.next_sibling();
                let Some(card) = widget.downcast_ref::<ToastCard>() else { continue };
                let (min_h, _, _, _) = card.measure(gtk::Orientation::Vertical, card_width as i32);
                let x = (width as f32 - card_width) / 2.0;
                let t = gsk::Transform::new().translate(&graphene::Point::new(x, card.imp().y.get()));
                card.allocate(card_width as i32, min_h, -1, Some(t));
            }
        }

        /// Only the cards take clicks; the rest passes through to the surface.
        fn contains(&self, _x: f64, _y: f64) -> bool {
            false
        }
    }
}

glib::wrapper! {
    pub struct ToastLayer(ObjectSubclass<layer_imp::ToastLayer>)
        @extends gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl Default for ToastLayer {
    fn default() -> Self {
        glib::Object::new()
    }
}

impl ToastLayer {
    fn presenter(&self) -> Option<Rc<Presenter>> {
        self.imp().presenter.borrow().upgrade()
    }
}

mod icon_imp {
    use super::*;

    #[derive(Default)]
    pub struct TintedIcon {
        pub name: RefCell<String>,
        pub size: Cell<i32>,
        pub color: Cell<u32>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for TintedIcon {
        const NAME: &'static str = "NshTintedIcon";
        type Type = super::TintedIcon;
        type ParentType = gtk::Widget;
    }

    impl ObjectImpl for TintedIcon {}

    impl WidgetImpl for TintedIcon {
        fn measure(&self, _orientation: gtk::Orientation, _for_size: i32) -> (i32, i32, i32, i32) {
            let s = self.size.get();
            (s, s, -1, -1)
        }

        fn snapshot(&self, snapshot: &gtk::Snapshot) {
            let obj = self.obj();
            let size = self.size.get();
            let theme = gtk::IconTheme::for_display(&obj.display());
            let icon = theme.lookup_icon(
                &self.name.borrow(),
                &[],
                size,
                obj.scale_factor(),
                obj.direction(),
                gtk::IconLookupFlags::empty(),
            );
            icon.snapshot_symbolic(snapshot, size as f64, size as f64, &[rgba(self.color.get())]);
        }
    }
}

glib::wrapper! {
    /// A symbolic icon in an arbitrary color (the theme tokens are not CSS).
    pub struct TintedIcon(ObjectSubclass<icon_imp::TintedIcon>)
        @extends gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl TintedIcon {
    fn new(name: &str, size: i32) -> TintedIcon {
        let icon: TintedIcon = glib::Object::new();
        icon.imp().name.replace(name.to_owned());
        icon.imp().size.set(size);
        icon.set_accessible_role(gtk::AccessibleRole::Presentation);
        icon
    }

    fn set_color(&self, color: u32) {
        self.imp().color.set(color);
        self.queue_draw();
    }
}
