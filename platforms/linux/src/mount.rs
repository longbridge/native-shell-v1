//! Applies decoded mutation batches to the widget tree.
//!
//! The whole batch is applied inside one main-loop iteration, before GTK's
//! next frame clock tick, so no frame ever shows half a batch.

use std::collections::HashMap;
use std::rc::Rc;

use gtk::prelude::*;
use gtk::{graphene, gsk};

use native_shell_core::protocol::{self, Mutation, Prop, PropValue, ViewType, ROOT_NODE};

use crate::component::{self, Component, NodeCx, Plain, Shared};
use crate::node_view::{NodeView, Shadow};
use crate::theme::ThemeTokens;

struct Node {
    view: NodeView,
    view_type: ViewType,
    component: Box<dyn Component>,
    parent: Option<u32>,
    mask: u32,
}

pub struct MountingManager {
    nodes: HashMap<u32, Node>,
    shared: Rc<Shared>,
    /// Seq of the last batch applied; events carry it.
    seq: u32,
}

impl MountingManager {
    pub fn new(shared: Rc<Shared>) -> MountingManager {
        let mut nodes = HashMap::new();
        nodes.insert(
            ROOT_NODE,
            Node {
                view: shared.root.clone(),
                view_type: ViewType::View,
                component: Box::new(Plain),
                parent: None,
                mask: 0,
            },
        );
        MountingManager { nodes, shared, seq: 0 }
    }

    pub fn seq(&self) -> u32 {
        self.seq
    }

    pub fn view(&self, node: u32) -> Option<&NodeView> {
        self.nodes.get(&node).map(|n| &n.view)
    }

    pub fn view_type(&self, node: u32) -> Option<ViewType> {
        self.nodes.get(&node).map(|n| n.view_type)
    }

    pub fn event_mask(&self, node: u32) -> u32 {
        self.nodes.get(&node).map_or(0, |n| n.mask)
    }

    pub fn parent(&self, node: u32) -> Option<u32> {
        self.nodes.get(&node).and_then(|n| n.parent)
    }

    /// The node whose view is `widget` or contains it.
    pub fn node_of(&self, widget: &gtk::Widget) -> Option<u32> {
        let mut current = Some(widget.clone());
        while let Some(w) = current {
            if let Some(view) = w.downcast_ref::<NodeView>() {
                if let Some((id, _)) = self.nodes.iter().find(|(_, n)| &n.view == view) {
                    return Some(*id);
                }
            }
            current = w.parent();
        }
        None
    }

    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    /// Decodes and applies one batch buffer. A malformed buffer is rejected whole.
    pub fn apply_buffer(&mut self, bytes: &[u8]) -> Result<(), protocol::DecodeError> {
        let mutations = protocol::decode(bytes)?;
        self.apply(&mutations);
        Ok(())
    }

    pub fn apply(&mut self, mutations: &[Mutation]) {
        let mut touched: Vec<u32> = Vec::new();
        for m in mutations {
            match m {
                Mutation::BeginBatch { seq, .. } => self.seq = *seq,
                Mutation::EndBatch => {}
                Mutation::Create { node, view_type } => self.create(*node, *view_type),
                Mutation::Insert { node, parent, index } => self.insert(*node, *parent, *index as usize),
                Mutation::Remove { node, parent, .. } => self.remove(*node, *parent),
                Mutation::Move { node, parent, to, .. } => self.insert(*node, *parent, *to as usize),
                Mutation::Delete { node } => self.delete(*node),
                Mutation::SetProp { node, prop, value } => {
                    self.set_prop(*node, *prop, value);
                    touched.push(*node);
                }
                Mutation::SetFrame { node, frame } => {
                    self.with_node(*node, |component, cx| {
                        cx.view.set_frame(*frame);
                        component.set_frame(cx, *frame);
                    });
                }
                Mutation::SetEventMask { node, mask } => {
                    if let Some(n) = self.nodes.get_mut(node) {
                        n.mask = *mask;
                    }
                    self.with_node(*node, |component, cx| component.set_event_mask(cx, *mask));
                }
                Mutation::Command { node, command, payload } => {
                    self.with_node(*node, |component, cx| component.command(cx, *command, payload));
                }
            }
        }
        touched.sort_unstable();
        touched.dedup();
        for node in touched {
            self.with_node(node, |component, cx| component.batch_applied(cx));
        }
    }

    fn with_node(&mut self, node: u32, f: impl FnOnce(&mut dyn Component, &NodeCx)) {
        let shared = self.shared.clone();
        if let Some(n) = self.nodes.get_mut(&node) {
            let view = n.view.clone();
            let cx = NodeCx { node, view: &view, shared: &shared };
            f(n.component.as_mut(), &cx);
        }
    }

    fn create(&mut self, node: u32, view_type: ViewType) {
        let view = NodeView::new();
        let cx = NodeCx { node, view: &view, shared: &self.shared };
        let component = component::create(view_type, &cx);
        self.nodes.insert(node, Node { view, view_type, component, parent: None, mask: 0 });
    }

    fn insert(&mut self, node: u32, parent: u32, index: usize) {
        let Some(child) = self.nodes.get_mut(&node) else { return };
        child.parent = Some(parent);
        let child_view = child.view.clone();
        self.with_node(parent, |component, cx| component.insert_child(cx, &child_view, index));
    }

    fn remove(&mut self, node: u32, parent: u32) {
        let Some(child) = self.nodes.get_mut(&node) else { return };
        child.parent = None;
        let child_view = child.view.clone();
        self.with_node(parent, |component, cx| component.remove_child(cx, &child_view));
    }

    /// Deletes a subtree. The core sends `Delete` for the subtree root only.
    fn delete(&mut self, node: u32) {
        if node == ROOT_NODE {
            return;
        }
        let mut stack = vec![node];
        while let Some(id) = stack.pop() {
            stack.extend(self.nodes.iter().filter(|(_, n)| n.parent == Some(id)).map(|(k, _)| *k));
            self.with_node(id, |component, cx| component.unmount(cx));
            if let Some(n) = self.nodes.remove(&id) {
                n.view.detach();
            }
        }
    }

    fn set_prop(&mut self, node: u32, prop: Prop, value: &PropValue) {
        if node == ROOT_NODE && prop == Prop::ThemeTokens {
            if let PropValue::Json(json) = value {
                if let Some(tokens) = ThemeTokens::parse(json) {
                    self.shared.theme.replace(tokens);
                    // Chrome drawn from tokens (control tints…) has no prop that changed.
                    queue_draw_tree(self.shared.root.upcast_ref());
                }
            }
        }
        let mut handled = false;
        self.with_node(node, |component, cx| handled = component.set_prop(cx, prop, value));
        if !handled {
            if let Some(n) = self.nodes.get(&node) {
                apply_base_prop(&n.view, prop, value);
            }
        }
    }
}

fn queue_draw_tree(widget: &gtk::Widget) {
    widget.queue_draw();
    let mut child = widget.first_child();
    while let Some(w) = child {
        queue_draw_tree(&w);
        child = w.next_sibling();
    }
}

/// Box properties every node supports.
fn apply_base_prop(view: &NodeView, prop: Prop, value: &PropValue) {
    match prop {
        Prop::BackgroundColor => view.update_style(|s| s.background = color(value)),
        Prop::CornerRadii => view.update_style(|s| s.radii = quad(value)),
        Prop::BorderWidths => view.update_style(|s| s.border_widths = quad(value)),
        Prop::BorderColor => view.update_style(|s| s.border_color = color(value)),
        Prop::ClipsToBounds => view.update_style(|s| s.clips = matches!(value, PropValue::Bool(true))),
        Prop::Shadow => view.update_style(|s| s.shadow = json(value).and_then(|v| parse_shadow(&v))),
        Prop::Opacity => view.set_opacity(float(value).unwrap_or(1.0) as f64),
        Prop::Hidden => view.set_visible(!matches!(value, PropValue::Bool(true))),
        Prop::Transform => view.set_transform(json(value).map(|v| parse_transform(&v))),
        Prop::TestId => view.set_widget_name(match value {
            PropValue::Str(s) => s,
            _ => "",
        }),
        _ => {}
    }
}

pub fn color(value: &PropValue) -> Option<u32> {
    match value {
        PropValue::Color(c) => Some(*c),
        _ => None,
    }
}

pub fn float(value: &PropValue) -> Option<f32> {
    match value {
        PropValue::F32(f) => Some(*f),
        PropValue::I32(i) => Some(*i as f32),
        _ => None,
    }
}

pub fn quad(value: &PropValue) -> [f32; 4] {
    match value {
        PropValue::F32Array(a) if a.len() >= 4 => [a[0], a[1], a[2], a[3]],
        _ => [0.0; 4],
    }
}

pub fn json(value: &PropValue) -> Option<serde_json::Value> {
    match value {
        PropValue::Json(s) => serde_json::from_str(s).ok(),
        _ => None,
    }
}

fn num(v: &serde_json::Value, key: &str) -> f32 {
    v.get(key).and_then(|x| x.as_f64()).unwrap_or(0.0) as f32
}

fn parse_shadow(v: &serde_json::Value) -> Option<Shadow> {
    Some(Shadow {
        color: v.get("color")?.as_u64()? as u32,
        x: num(v, "x"),
        y: num(v, "y"),
        blur: num(v, "blur"),
        spread: num(v, "spread"),
    })
}

fn parse_transform(v: &serde_json::Value) -> gsk::Transform {
    let scale = v.get("scale").and_then(|x| x.as_f64()).unwrap_or(1.0) as f32;
    gsk::Transform::new()
        .translate(&graphene::Point::new(num(v, "translate_x"), num(v, "translate_y")))
        .rotate(num(v, "rotate"))
        .scale(scale, scale)
}
