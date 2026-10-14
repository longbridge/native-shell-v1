//! What a view type adds on top of the base [`NodeView`].
//!
//! The mounting manager owns one [`Component`] per node. Box properties
//! (background, border, opacity, transform, accessibility…) are handled by the
//! base view unless the component claims them first, so a component only
//! implements what its view type means.

use std::cell::RefCell;
use std::rc::Rc;

use native_shell_core::protocol::{Command, EventType, Frame, Prop, PropValue, ViewType};

use crate::node_view::NodeView;
use crate::theme::ThemeTokens;

/// Sends an event for a node; the manager stamps it with the applied batch seq.
pub type EventSink = Rc<dyn Fn(u32, EventType, String)>;

/// State shared by every node of one surface.
pub struct Shared {
    pub events: EventSink,
    pub theme: RefCell<ThemeTokens>,
    /// The surface's root node view (node 0), for overlays and toasts.
    pub root: NodeView,
    /// Host component types and the log sink.
    pub runtime: crate::runtime::Runtime,
}

/// The node a component call is about.
pub struct NodeCx<'a> {
    pub node: u32,
    pub view: &'a NodeView,
    pub shared: &'a Rc<Shared>,
}

impl NodeCx<'_> {
    pub fn emit(&self, event: EventType, payload: String) {
        (self.shared.events)(self.node, event, payload);
    }
}

pub trait Component {
    /// Applies a property; returns `false` when it is not this component's, so
    /// the base view handles it. `PropValue::Null` restores the default.
    fn set_prop(&mut self, _cx: &NodeCx, _prop: Prop, _value: &PropValue) -> bool {
        false
    }

    fn set_frame(&mut self, _cx: &NodeCx, _frame: Frame) {}

    /// The event listeners the script attached changed.
    fn set_event_mask(&mut self, _cx: &NodeCx, _mask: u32) {}

    fn command(&mut self, _cx: &NodeCx, _command: Command, _payload: &str) {}

    fn insert_child(&mut self, cx: &NodeCx, child: &NodeView, index: usize) {
        cx.view.insert_node_child(child, index);
    }

    fn remove_child(&mut self, _cx: &NodeCx, child: &NodeView) {
        child.detach();
    }

    /// Every mutation of the batch has been applied.
    fn batch_applied(&mut self, _cx: &NodeCx) {}

    /// The node is being deleted.
    fn unmount(&mut self, _cx: &NodeCx) {}
}

/// Plain container: everything is base view behavior.
pub struct Plain;

impl Component for Plain {}

pub fn create(view_type: ViewType, cx: &NodeCx) -> Box<dyn Component> {
    crate::components::create(view_type, cx)
}
