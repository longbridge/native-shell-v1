//! The widget an instance renders into: owns the mounting manager for node 0,
//! applies batches on the main thread as they arrive, and keeps the core's
//! [`Environment`] in sync with size, scale and appearance.

use std::cell::RefCell;
use std::rc::Rc;

use gtk::prelude::*;
use gtk::subclass::prelude::*;
use gtk::glib;

use native_shell_core::host::Environment;
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
        }

        fn dispose(&self) {
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
            let seq = surface.imp().manager.borrow().as_ref().map_or(0, |m| m.seq());
            instance.dispatch_event(node, event, &payload, seq);
        });
        let shared = Rc::new(Shared { events, theme: RefCell::new(ThemeTokens::default()), root: imp.root.clone() });
        imp.manager.replace(Some(MountingManager::new(shared)));
        imp.instance.replace(Some(instance.clone()));
        imp.last_env.replace(None);

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
                if let Err(e) = manager.apply_buffer(&batch) {
                    eprintln!("[native-shell] dropped a malformed batch: {e}");
                }
            }
        }
    }

    fn sync_environment(&self) {
        let imp = self.imp();
        let Some(instance) = imp.instance.borrow().clone() else { return };
        let dark = adw::StyleManager::default().is_dark();
        let env = Environment {
            width: self.width() as f32,
            height: self.height() as f32,
            scale: self.scale_factor() as f32,
            appearance: if dark { Appearance::Dark } else { Appearance::Light },
            platform: "linux".into(),
            ..Environment::default()
        };
        if env.width <= 0.0 || imp.last_env.borrow().as_ref() == Some(&env) {
            return;
        }
        imp.last_env.replace(Some(env.clone()));
        instance.set_environment(env);
    }
}
