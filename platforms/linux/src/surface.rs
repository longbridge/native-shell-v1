//! The widget an instance renders into: owns the mounting manager for node 0,
//! applies batches on the main thread as they arrive, and keeps the core's
//! [`Environment`] in sync with size, scale, appearance, text size and locale.
//!
//! Appearance follows libadwaita's style manager (the desktop's dark style
//! preference, or the app's override) and text size the desktop's text
//! scaling factor, which GTK exposes as `gtk-xft-dpi`; both apply live.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gtk::prelude::*;
use gtk::subclass::prelude::*;
use gtk::glib;

use native_shell_core::host::Environment;
use native_shell_core::protocol::EventType;
use native_shell_core::theme::Appearance;

use crate::component::Shared;
use crate::instance::Instance;
use crate::mount::MountingManager;
use crate::node_view::NodeView;
use crate::theme::ThemeTokens;

mod imp {
    use super::*;

    #[derive(Default)]
    pub struct SurfaceView {
        pub root: NodeView,
        pub instance: RefCell<Option<Rc<Instance>>>,
        pub manager: RefCell<Option<MountingManager>>,
        pub last_env: RefCell<Option<Environment>>,
        pub applied_batches: Cell<u64>,
        /// Handlers on process-wide objects, disconnected on dispose.
        pub watchers: RefCell<Vec<(glib::Object, glib::SignalHandlerId)>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for SurfaceView {
        const NAME: &'static str = "NshSurfaceView";
        type Type = super::SurfaceView;
        type ParentType = gtk::Widget;
    }

    impl ObjectImpl for SurfaceView {
        fn constructed(&self) {
            self.parent_constructed();
            self.root.set_parent(&*self.obj());
            self.obj().set_hexpand(true);
            self.obj().set_vexpand(true);
            self.obj().watch_environment();
        }

        fn dispose(&self) {
            for (object, handler) in self.watchers.take() {
                object.disconnect(handler);
            }
            self.root.unparent();
        }
    }

    impl WidgetImpl for SurfaceView {
        fn measure(&self, _orientation: gtk::Orientation, _for_size: i32) -> (i32, i32, i32, i32) {
            (0, 0, -1, -1)
        }

        fn size_allocate(&self, width: i32, height: i32, baseline: i32) {
            self.root.allocate(width, height, baseline, None);
            self.obj().sync_environment();
        }
    }
}

glib::wrapper! {
    pub struct SurfaceView(ObjectSubclass<imp::SurfaceView>)
        @extends gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl Default for SurfaceView {
    fn default() -> Self {
        glib::Object::new()
    }
}

impl SurfaceView {
    pub fn new() -> SurfaceView {
        Self::default()
    }

    /// Renders `instance` here and starts applying its batches.
    pub fn attach(&self, instance: Rc<Instance>) {
        let imp = self.imp();
        let weak_instance = Rc::downgrade(&instance);
        let weak_self = self.downgrade();
        let events = Rc::new(move |node, event, payload: String| {
            let (Some(instance), Some(surface)) = (weak_instance.upgrade(), weak_self.upgrade()) else { return };
            // Components also send events while a batch is applied (a focused
            // field told to blur sends Blur), when the manager is borrowed.
            let manager = surface.imp().manager.try_borrow();
            let seq = manager.ok().and_then(|m| m.as_ref().map(|m| m.seq())).unwrap_or(0);
            instance.dispatch_event(node, event, &payload, seq);
        });
        let shared = Rc::new(Shared {
            events,
            theme: RefCell::new(ThemeTokens::default()),
            root: imp.root.clone(),
            runtime: instance.runtime().clone(),
            overlays: Default::default(),
        });
        imp.manager.replace(Some(MountingManager::new(shared)));
        imp.instance.replace(Some(instance.clone()));
        imp.last_env.replace(None);
        imp.applied_batches.set(0);

        let ready = instance.ready.clone();
        let weak = self.downgrade();
        glib::spawn_future_local(async move {
            while ready.recv().await.is_ok() {
                let Some(surface) = weak.upgrade() else { break };
                surface.drain();
            }
        });
        self.drain();
        self.sync_environment();
    }

    /// Applies every finished batch.
    pub fn drain(&self) {
        let imp = self.imp();
        let Some(instance) = imp.instance.borrow().clone() else { return };
        while let Some(batch) = instance.take_batch() {
            if let Some(manager) = imp.manager.borrow_mut().as_mut() {
                match manager.apply_buffer(&batch) {
                    Ok(()) => imp.applied_batches.set(imp.applied_batches.get() + 1),
                    Err(e) => eprintln!("[native-shell] dropped a malformed batch: {e}"),
                }
            }
        }
    }

    /// The instance rendering here.
    pub fn instance(&self) -> Option<Rc<Instance>> {
        self.imp().instance.borrow().clone()
    }

    /// Batches applied since `attach` (diagnostics, tests).
    pub fn applied_batches(&self) -> u64 {
        self.imp().applied_batches.get()
    }

    /// Number of nodes currently mounted, the root included.
    pub fn mounted_node_count(&self) -> usize {
        self.imp().manager.borrow().as_ref().map_or(0, |m| m.node_count())
    }

    /// The node `widget` belongs to (its view, or a widget inside it).
    pub fn node_of(&self, widget: &impl IsA<gtk::Widget>) -> Option<u32> {
        self.imp().manager.borrow().as_ref()?.node_of(widget.upcast_ref())
    }

    /// The nearest node at or above `node` whose listeners include `mask` (`event_mask::*`).
    pub fn listener(&self, node: u32, mask: u32) -> Option<u32> {
        let manager = self.imp().manager.borrow();
        let manager = manager.as_ref()?;
        let mut current = Some(node);
        while let Some(n) = current {
            if manager.event_mask(n) & mask != 0 {
                return Some(n);
            }
            current = manager.parent(n);
        }
        None
    }

    /// Sends an event as if the node's widget had produced it, stamped with the applied seq.
    pub fn send_event(&self, node: u32, event: EventType, payload_json: &str) {
        let Some(instance) = self.instance() else { return };
        let seq = self.imp().manager.borrow().as_ref().map_or(0, |m| m.seq());
        instance.dispatch_event(node, event, payload_json, seq);
    }

    /// The Environment the surface currently reports.
    pub fn environment(&self) -> Environment {
        // Fractional scales (125 %…) are only known to the realized surface.
        let scale = self.native().and_then(|n| n.surface()).map_or(self.scale_factor() as f64, |s| s.scale());
        Environment { width: self.width() as f32, height: self.height() as f32, scale: scale as f32, ..initial_environment() }
    }

    fn sync_environment(&self) {
        let imp = self.imp();
        let Some(instance) = imp.instance.borrow().clone() else { return };
        let env = self.environment();
        if env.width <= 0.0 || env.height <= 0.0 || imp.last_env.borrow().as_ref() == Some(&env) {
            return;
        }
        imp.last_env.replace(Some(env.clone()));
        crate::text::set_scales(env.scale, env.font_scale);
        instance.set_environment(env);
    }

    fn watch_environment(&self) {
        let mut watchers = self.imp().watchers.borrow_mut();
        let style = adw::StyleManager::default();
        let weak = self.downgrade();
        let id = style.connect_dark_notify(move |_| {
            if let Some(surface) = weak.upgrade() {
                surface.sync_environment();
            }
        });
        watchers.push((style.upcast(), id));
        if let Some(settings) = gtk::Settings::default() {
            let weak = self.downgrade();
            let id = settings.connect_gtk_xft_dpi_notify(move |_| {
                if let Some(surface) = weak.upgrade() {
                    surface.sync_environment();
                }
            });
            watchers.push((settings.upcast(), id));
        }
        let weak = self.downgrade();
        self.connect_scale_factor_notify(move |_| {
            if let Some(surface) = weak.upgrade() {
                surface.sync_environment();
            }
        });
    }
}

/// What an instance starts with before a surface reports its size.
pub(crate) fn initial_environment() -> Environment {
    Environment {
        scale: 1.0,
        font_scale: font_scale(),
        appearance: if adw::StyleManager::default().is_dark() { Appearance::Dark } else { Appearance::Light },
        platform: "linux".into(),
        locale: locale(),
        ..Environment::default()
    }
}

/// The desktop's text scaling factor: GTK reports it as `gtk-xft-dpi`, in
/// 1024ths of a dot per inch, over a 96 dpi baseline.
fn font_scale() -> f32 {
    let dpi = gtk::Settings::default().map_or(-1, |s| s.gtk_xft_dpi());
    if dpi <= 0 {
        return 1.0;
    }
    (dpi as f32 / (96.0 * 1024.0)).clamp(0.5, 4.0)
}

/// The user's first language as a BCP 47 tag (`de_CH.UTF-8` → `de-CH`).
fn locale() -> String {
    glib::language_names()
        .iter()
        .map(|name| name.split(['.', '@']).next().unwrap_or("").replace('_', "-"))
        .find(|tag| !tag.is_empty() && tag != "C" && tag != "POSIX")
        .unwrap_or_else(|| "en".into())
}
