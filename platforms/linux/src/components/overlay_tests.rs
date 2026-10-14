//! Pages, sheets, dialogs, alerts and toasts against the real core: the engine
//! harness runs the program, its batches are applied to GTK widgets, and the
//! events the renderer sends go back to the program.

use std::cell::RefCell;
use std::rc::Rc;

use adw::prelude::*;

use native_shell::core::host::Environment;
use native_shell::core::protocol::{EventType, ViewType, ROOT_NODE};
use native_shell::testing::Harness;

use crate::component::Shared;
use crate::components::modal::ModalState;
use crate::components::overlays::{Presenter, Surface};
use crate::components::pages;
use crate::gtk_test::{self, flush, wait_until};
use crate::mount::MountingManager;
use crate::node_view::NodeView;
use crate::theme::ThemeTokens;

const APP: &str = r##"
import { View, div, v_flex, Button } from "native-shell";
class Detail extends View {
  init(props) { this.id = props.id; this.n = 0; }
  render(cx) {
    return v_flex().size_full()
      .child(div().test_id("detail").child(`detail ${this.id} n=${this.n}`))
      .child(Button.new("more").test_id("push-more").label("more").on_click((_e, cx) => cx.navigator.push(Detail, { id: this.id + 1 }, { title: `Detail ${this.id + 1}` })))
      .child(Button.new("lock").test_id("lock").label("lock").on_click((_e, cx) => {
        cx.set_can_go_back(false);
        cx.on_back((cx) => { this.n = 100; cx.notify(); });
      }))
      .child(Button.new("pop").test_id("pop").label("pop").on_click((_e, cx) => cx.navigator.pop()));
  }
}
export default class App extends View {
  init() { this.log = []; }
  note(s, cx) { this.log.push(s); cx.notify(); }
  render(cx) {
    return v_flex().size_full()
      .child(div().test_id("log").child(this.log.join(",")))
      .child(Button.new("push").test_id("push").label("push").on_click((_e, cx) => cx.navigator.push(Detail, { id: 1 }, { title: "Detail 1" })))
      .child(Button.new("sheet").test_id("sheet").label("sheet").on_click((_e, cx) => {
        cx.open_sheet(() => v_flex().p_4().test_id("sheet-body").child(div().h(120).child("Share"))
          .child(Button.new("close").test_id("close-sheet").label("close").on_click((_e, cx) => cx.close_sheet())),
          { detents: ["medium", "large"], on_dismiss: (cx) => this.note("sheet dismissed", cx) });
      }))
      .child(Button.new("fit").test_id("fit").label("fit").on_click((_e, cx) => {
        cx.open_sheet(() => div().h(200).test_id("fit-body").child("Fits"), { detents: [], on_dismiss: (cx) => this.note("fit dismissed", cx) });
      }))
      .child(Button.new("dialog").test_id("dialog").label("dialog").on_click((_e, cx) => {
        cx.open_dialog(() => div().p_4().test_id("dialog-body").child("Rename"), { on_dismiss: () => this.note("dialog dismissed", cx) });
      }))
      .child(Button.new("modal").test_id("modal").label("modal").on_click((_e, cx) => {
        cx.open_dialog(() => div().p_4().child("Required"), { dismissable: false });
      }))
      .child(Button.new("alert").test_id("alert").label("alert").on_click((_e, cx) => {
        cx.alert({ title: "Delete “Groceries”?", message: "This can’t be undone.", actions: [
          { label: "Cancel", style: "cancel", on_press: (cx) => this.note("cancel", cx) },
          { label: "Delete", style: "destructive", on_press: (cx) => this.note("deleted", cx) },
        ] });
      }))
      .child(Button.new("toast").test_id("toast").label("toast").on_click((_e, cx) => {
        cx.push_toast({ id: "undo", title: "Deleted “Milk”", level: "success", timeout: 4000,
          action: { label: "Undo", on_click: (cx) => this.note("undo", cx) } });
      }))
      .child(Button.new("many").test_id("many").label("many").on_click((_e, cx) => {
        for (let i = 1; i <= 4; i++) cx.push_toast({ id: `t${i}`, title: `Toast ${i}`, timeout: null });
      }))
      .child(Button.new("drop").test_id("drop").label("drop").on_click(() => window.remove_toast("t2")));
  }
}
"##;

type Events = Rc<RefCell<Vec<(u32, EventType, String)>>>;

/// A harness instance mounted into a real window.
struct Rig {
    h: Harness,
    manager: MountingManager,
    shared: Rc<Shared>,
    events: Events,
    window: adw::Window,
}

impl Rig {
    fn new() -> Rig {
        let env = Environment { width: 390.0, height: 844.0, scale: 1.0, platform: "linux".into(), ..Environment::default() };
        let h = Harness::build(&[("main.js", APP)]).environment(env.clone()).start();
        assert!(h.load_error().is_none(), "{:?} {:?}", h.load_error(), h.logs());
        let events: Events = Rc::default();
        let sink = events.clone();
        let root = NodeView::new();
        let shared = Rc::new(Shared {
            events: Rc::new(move |node, event, payload| sink.borrow_mut().push((node, event, payload))),
            theme: RefCell::new(ThemeTokens::default()),
            root: root.clone(),
            runtime: Default::default(),
            overlays: Default::default(),
        });
        let manager = MountingManager::new(shared.clone());
        let window = adw::Window::builder().default_width(390).default_height(844).content(&root).build();
        window.present();
        assert!(wait_until(3000, || root.width() > 0), "the window shows");
        let mut rig = Rig { h, manager, shared, events, window };
        // The core lays out at the window's real size, as the surface view tells it.
        let env = Environment { width: root.width() as f32, height: root.height() as f32, ..env };
        rig.h.set_environment(env);
        rig.pump();
        rig
    }

    fn presenter(&self) -> Rc<Presenter> {
        self.shared.overlays.presenter().expect("the root component sets up the presenter")
    }

    /// Applies what the core sent and feeds the renderer's events back until both are quiet.
    fn pump(&mut self) {
        for _ in 0..20 {
            let mutations = self.h.mutations().to_vec();
            self.h.clear_mutations();
            self.manager.apply(&mutations);
            layout(self);
            let events: Vec<_> = self.events.borrow_mut().drain(..).collect();
            if events.is_empty() && self.h.mutations().is_empty() {
                return;
            }
            for (node, event, payload) in events {
                self.h.dispatch(node, event, &payload);
            }
        }
        panic!("the renderer and the core kept talking");
    }

    fn tap(&mut self, test_id: &str) {
        self.h.tap(test_id);
        self.pump();
    }

    fn surfaces(&self) -> Vec<(u32, ViewType)> {
        self.h.surfaces()
    }

    fn nav_titles(&self) -> Vec<String> {
        let nav = &self.presenter().nav;
        nav.navigation_stack()
            .iter::<adw::NavigationPage>()
            .filter_map(Result::ok)
            .map(|p| p.title().to_string())
            .collect()
    }

    fn modal(&self, node: u32) -> Rc<ModalState> {
        let view = self.manager.view(node).expect("modal node").clone();
        self.modal_for(&view).expect("a registered modal")
    }

    fn modal_for(&self, view: &NodeView) -> Option<Rc<ModalState>> {
        let p = self.presenter();
        let surfaces = p.surfaces_ref();
        match surfaces.get(view) {
            Some(Surface::Modal(m)) => Some(m.clone()),
            _ => None,
        }
    }

    fn log(&self) -> String {
        self.h.text_of("log")
    }
}

impl Drop for Rig {
    fn drop(&mut self) {
        self.window.destroy();
        flush();
    }
}

/// Runs what is pending and lays the surface out now: a Broadway display
/// without a connected browser ticks its frame clock only about once a second.
fn layout(rig: &Rig) {
    flush();
    let root = &rig.shared.root;
    root.allocate(root.width(), root.height(), -1, None);
}

fn height(rig: &Rig) -> f64 {
    rig.shared.root.height() as f64
}

fn set_animations(on: bool) {
    if let Some(settings) = gtk::Settings::default() {
        settings.set_gtk_enable_animations(on);
    }
}

#[test]
fn pages_push_pop_and_user_back() {
    gtk_test::run(|| {
        set_animations(false);
        let mut rig = Rig::new();
        assert_eq!(rig.nav_titles(), ["Home"]);

        rig.tap("push");
        assert_eq!(rig.nav_titles(), ["Home", "Detail 1"]);
        rig.tap("push-more");
        assert_eq!(rig.nav_titles(), ["Home", "Detail 1", "Detail 2"]);
        assert!(rig.h.texts().iter().any(|t| t == "detail 2 n=0"));

        // The program pops: DismissPage + Delete; the page leaves the stack.
        rig.tap("pop");
        assert_eq!(rig.nav_titles(), ["Home", "Detail 1"]);
        assert_eq!(rig.surfaces().len(), 2);

        // The user goes back: the renderer pops and reports Dismiss; the core follows.
        assert!(pages::perform_back(&rig.presenter()));
        rig.pump();
        assert_eq!(rig.nav_titles(), ["Home"]);
        assert_eq!(rig.surfaces().len(), 1);
        assert!(rig.h.find("detail").is_none());
    });
}

#[test]
fn a_page_that_cannot_go_back_gets_back_pressed() {
    gtk_test::run(|| {
        set_animations(false);
        let mut rig = Rig::new();
        rig.tap("push");
        rig.tap("lock");
        let page = rig.presenter().nav.visible_page().expect("a page");
        assert!(!page.can_pop(), "gestures and shortcuts are off");

        assert!(pages::perform_back(&rig.presenter()));
        rig.pump();
        assert_eq!(rig.nav_titles(), ["Home", "Detail 1"], "the page stays");
        assert_eq!(rig.h.text_of("detail"), "detail 1 n=100", "the page's back handler ran");

        // A native pop (the swipe, which a locked page refuses on iOS) is still honored.
        rig.presenter().nav.pop();
        rig.pump();
        assert_eq!(rig.nav_titles(), ["Home"]);
        assert_eq!(rig.surfaces().len(), 1);
    });
}

/// The touch edge-swipe recognizer the presenter adds to the navigation view.
fn edge_swipe(rig: &Rig) -> gtk::GestureDrag {
    let nav = rig.presenter().nav.clone();
    let controllers = nav.observe_controllers();
    (0..controllers.n_items())
        .filter_map(|i| controllers.item(i).and_downcast::<gtk::GestureDrag>())
        .find(|g| g.is_touch_only())
        .expect("the edge swipe gesture")
}

#[test]
fn a_refused_edge_swipe_is_reported_once() {
    gtk_test::run(|| {
        set_animations(false);
        let mut rig = Rig::new();
        rig.tap("push");
        rig.tap("lock");
        let swipe = edge_swipe(&rig);
        swipe.emit_by_name::<()>("drag-begin", &[&4.0f64, &300.0f64]);
        swipe.emit_by_name::<()>("drag-update", &[&20.0f64, &2.0f64]);
        rig.pump();
        assert_eq!(rig.h.text_of("detail"), "detail 1 n=0", "too short to count");
        swipe.emit_by_name::<()>("drag-update", &[&60.0f64, &4.0f64]);
        swipe.emit_by_name::<()>("drag-update", &[&120.0f64, &4.0f64]);
        rig.pump();
        assert_eq!(rig.h.text_of("detail"), "detail 1 n=100", "BackPressed reached the page");
        assert_eq!(rig.nav_titles(), ["Home", "Detail 1"], "the page stays");
    });
}

#[test]
fn focus_stays_in_the_top_sheet_or_dialog() {
    gtk_test::run(|| {
        set_animations(false);
        let mut rig = Rig::new();
        let nav = rig.presenter().nav.clone();
        assert!(nav.can_focus());
        rig.tap("sheet");
        let sheet = rig.modal(rig.surfaces()[1].0);
        let host = sheet.host.borrow().clone().expect("host");
        assert!(!nav.can_focus(), "nothing below the sheet takes focus");
        assert!(host.can_focus());
        let window = rig.window.clone();
        let mut inside = 0;
        for _ in 0..6 {
            window.child_focus(gtk::DirectionType::TabForward);
            let focus = gtk::prelude::GtkWindowExt::focus(&window);
            assert!(focus.as_ref().is_none_or(|f| f == host.upcast_ref::<gtk::Widget>() || f.is_ancestor(&host)), "Tab stays in the sheet: {focus:?}");
            inside += usize::from(focus.is_some());
        }
        assert!(inside > 0, "Tab reaches the sheet's controls");
        sheet.test_user_dismiss();
        rig.pump();
        assert!(nav.can_focus(), "focus returns below once it closes");
    });
}

#[test]
fn toast_symbols_take_the_status_tokens() {
    gtk_test::run(|| {
        set_animations(false);
        let mut rig = Rig::new();
        let p = rig.presenter();
        let light = native_shell::core::theme::Theme::default_for(native_shell::core::theme::Appearance::Light);
        for (level, token) in [("info", "info"), ("success", "success"), ("warning", "warning"), ("error", "destructive")] {
            rig.manager.apply(&[native_shell::core::protocol::Mutation::Command {
                node: ROOT_NODE,
                command: native_shell::core::protocol::Command::ShowToast,
                payload: format!(r#"{{"id":"{level}","title":"{level}","level":"{level}","duration":0}}"#),
            }]);
            assert_eq!(p.toasts.icon_color(level), light.color(token), "{level}");
            if level == "success" {
                assert_eq!(p.toasts.icon_color(level), Some(0xFF34_C759), "iOS system green by default");
            }
            p.toasts.dismiss(Some(level));
        }
        rig.pump();
    });
}

#[test]
fn a_deleted_page_stays_until_its_exit_transition_ends() {
    gtk_test::run(|| {
        set_animations(true);
        let mut rig = Rig::new();
        rig.tap("push");
        assert!(wait_until(3000, || rig.presenter().nav.visible_page().is_some_and(|p| p.title() == "Detail 1")));
        // Let the push transition finish.
        let detail = rig.manager.view(rig.h.find("detail").expect("detail node")).expect("view").clone();
        assert!(wait_until(3000, || detail.is_mapped()));
        std::thread::sleep(std::time::Duration::from_millis(600));
        flush();

        rig.tap("pop");
        assert_eq!(rig.surfaces().len(), 1, "the core deleted the page");
        assert!(detail.parent().is_some(), "views stay on screen during the transition");
        assert!(wait_until(3000, || detail.root().is_none()), "released after the transition");
        assert_eq!(rig.nav_titles(), ["Home"]);
        set_animations(false);
    });
}

#[test]
fn sheets_present_with_detents_and_dismiss() {
    gtk_test::run(|| {
        set_animations(false);
        let mut rig = Rig::new();
        rig.tap("sheet");
        let (node, kind) = rig.surfaces()[1];
        assert_eq!(kind, ViewType::Sheet);
        let sheet = rig.modal(node);
        assert!(sheet.host.borrow().as_ref().is_some_and(|h| h.parent().is_some()), "presented on the stage");
        layout(&rig);
        // Medium detent: the card's top is halfway down.
        assert_eq!(sheet.test_top(), Some(height(&rig) / 2.0));

        // Dragging up past the middle settles at large.
        sheet.test_drag(-300.0);
        layout(&rig);
        assert_eq!(sheet.test_top(), Some(16.0));

        // Dragging down below the smallest detent dismisses it.
        sheet.test_drag(700.0);
        rig.pump();
        assert_eq!(rig.surfaces().len(), 1);
        assert_eq!(rig.log(), "sheet dismissed");
        assert!(rig.modal_for(&sheet.view).is_none(), "released once closed and deleted");

        // Closed by the program: no Dismiss.
        rig.tap("sheet");
        rig.tap("close-sheet");
        assert_eq!(rig.surfaces().len(), 1);
        assert_eq!(rig.log(), "sheet dismissed");
    });
}

#[test]
fn a_sheet_without_detents_fits_its_content() {
    gtk_test::run(|| {
        set_animations(false);
        let mut rig = Rig::new();
        rig.tap("fit");
        let sheet = rig.modal(rig.surfaces()[1].0);
        layout(&rig);
        assert_eq!(sheet.test_top(), Some(height(&rig) - 200.0));
        sheet.test_user_dismiss();
        rig.pump();
        assert_eq!(rig.log(), "fit dismissed");
    });
}

#[test]
fn dialogs_dismiss_only_when_dismissable() {
    gtk_test::run(|| {
        set_animations(false);
        let mut rig = Rig::new();
        rig.tap("modal");
        let modal = rig.modal(rig.surfaces()[1].0);
        modal.test_user_dismiss();
        rig.pump();
        assert_eq!(rig.surfaces().len(), 2, "a non-dismissable dialog stays");
        rig.h.dismiss();
        rig.pump();
        assert_eq!(rig.surfaces().len(), 1);

        rig.tap("dialog");
        let dialog = rig.modal(rig.surfaces()[1].0);
        layout(&rig);
        let host = dialog.host.borrow().clone().expect("host");
        let bounds = dialog.view.compute_bounds(&host).expect("bounds");
        assert!(bounds.width() > 0.0);
        assert_eq!(bounds.x().round(), ((host.width() as f32 - bounds.width()) / 2.0).round(), "centered");
        dialog.test_user_dismiss();
        rig.pump();
        assert_eq!(rig.surfaces().len(), 1);
        assert_eq!(rig.log(), "dialog dismissed");
    });
}

#[test]
fn alerts_report_the_chosen_action() {
    gtk_test::run(|| {
        set_animations(false);
        let mut rig = Rig::new();
        rig.tap("alert");
        let alert = rig.modal(rig.surfaces()[1].0).alert.borrow().clone().expect("a native alert");
        assert_eq!(alert.heading().as_deref(), Some("Delete “Groceries”?"));
        assert_eq!(alert.response_label("1"), "Delete");
        assert_eq!(alert.response_appearance("1"), adw::ResponseAppearance::Destructive);
        assert_eq!(alert.close_response(), "0", "Escape answers Cancel");
        alert.emit_by_name::<()>("response", &[&"1"]);
        rig.pump();
        assert_eq!(rig.log(), "deleted");
        assert_eq!(rig.surfaces().len(), 1);

        rig.tap("alert");
        let alert = rig.modal(rig.surfaces()[1].0).alert.borrow().clone().expect("a native alert");
        alert.close();
        rig.pump();
        assert_eq!(rig.log(), "deleted,cancel");
        assert_eq!(rig.surfaces().len(), 1);
    });
}

#[test]
fn toasts_stack_queue_time_out_and_run_actions() {
    gtk_test::run(|| {
        set_animations(false);
        let mut rig = Rig::new();
        let p = rig.presenter();
        rig.tap("many");
        assert_eq!(p.toasts.visible_ids(), ["t1", "t2", "t3"]);
        assert_eq!(p.toasts.queued_ids(), ["t4"]);
        rig.tap("drop");
        assert_eq!(p.toasts.visible_ids(), ["t1", "t3", "t4"]);
        assert!(p.toasts.queued_ids().is_empty());
        rig.manager.apply(&[native_shell::core::protocol::Mutation::Command {
            node: ROOT_NODE,
            command: native_shell::core::protocol::Command::DismissToast,
            payload: "{}".into(),
        }]);
        assert!(p.toasts.visible_ids().is_empty());

        rig.tap("toast");
        assert_eq!(p.toasts.visible_ids(), ["undo"]);
        p.toasts.tap_action("undo");
        rig.pump();
        assert_eq!(rig.log(), "undo");
        assert!(p.toasts.visible_ids().is_empty(), "the action retracts the toast");

        rig.tap("toast");
        assert!(wait_until(6000, || p.toasts.visible_ids().is_empty()), "retracted after its timeout");
    });
}

