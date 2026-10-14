//! Applies decoded mutation batches to the widget tree.
//!
//! The whole batch is applied inside one main-loop iteration, before GTK's
//! next frame clock tick, so no frame ever shows half a batch. Instructions
//! naming unknown nodes are skipped: a renderer bug or a hostile buffer must
//! never crash the app.

use std::collections::HashMap;
use std::rc::Rc;

use gtk::prelude::*;
use gtk::{graphene, gsk};

use native_shell_core::protocol::{
    self, batch_flags, Mutation, Prop, PropValue, ViewType, ROOT_NODE,
};

use crate::animator::AnimationSpec;
use crate::component::{self, Component, NodeCx, Shared};
use crate::node_view::{self, NodeView, Shadow};
use crate::theme::ThemeTokens;

struct Node {
    view: NodeView,
    view_type: ViewType,
    component: Box<dyn Component>,
    parent: Option<u32>,
    /// Child node ids in order, as the core sees them.
    children: Vec<u32>,
    mask: u32,
}

pub struct MountingManager {
    nodes: HashMap<u32, Node>,
    shared: Rc<Shared>,
    /// Seq of the last batch applied; events carry it.
    seq: u32,
    /// The batch being applied is flagged ANIMATED.
    animated: bool,
}

impl MountingManager {
    pub fn new(shared: Rc<Shared>) -> MountingManager {
        let root = shared.root.clone();
        root.bind(ROOT_NODE, &shared);
        crate::keys::attach(&root);
        let mut nodes = HashMap::new();
        let root_cx = NodeCx { node: ROOT_NODE, view: &shared.root, shared: &shared };
        let root_component = crate::components::root(&root_cx);
        nodes.insert(
            ROOT_NODE,
            Node {
                view: root,
                view_type: ViewType::View,
                component: root_component,
                parent: None,
                children: Vec::new(),
                mask: 0,
            },
        );
        MountingManager {
            nodes,
            shared,
            seq: 0,
            animated: false,
        }
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
                if self
                    .nodes
                    .get(&view.node())
                    .is_some_and(|n| &n.view == view)
                {
                    return Some(view.node());
                }
            }
            current = w.parent();
        }
        None
    }

    /// Live nodes, the root included.
    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    /// Child node ids of `node`, in order.
    pub fn children(&self, node: u32) -> &[u32] {
        self.nodes.get(&node).map_or(&[], |n| &n.children)
    }

    /// Decodes and applies one batch buffer. A malformed buffer is rejected whole.
    pub fn apply_buffer(&mut self, bytes: &[u8]) -> Result<(), protocol::DecodeError> {
        let mutations = protocol::decode(bytes)?;
        self.apply(&mutations);
        Ok(())
    }

    pub fn apply(&mut self, mutations: &[Mutation]) {
        let mut touched: Vec<u32> = Vec::new();
        self.animated = false;
        for m in mutations {
            match m {
                Mutation::BeginBatch { seq, flags } => {
                    self.seq = *seq;
                    self.animated = flags & batch_flags::ANIMATED != 0;
                }
                Mutation::EndBatch => {}
                Mutation::Create { node, view_type } => self.create(*node, *view_type),
                Mutation::Insert {
                    node,
                    parent,
                    index,
                } => {
                    self.insert(*node, *parent, *index as usize);
                    touched.push(*parent);
                }
                Mutation::Remove { node, parent, .. } => {
                    self.remove(*node, *parent);
                    touched.push(*parent);
                }
                Mutation::Move {
                    node, parent, to, ..
                } => {
                    self.insert(*node, *parent, *to as usize);
                    touched.push(*parent);
                }
                Mutation::Delete { node } => self.delete(*node),
                Mutation::SetProp { node, prop, value } => {
                    self.set_prop(*node, *prop, value);
                    touched.push(*node);
                }
                Mutation::SetFrame { node, frame } => {
                    let animated = self.animated;
                    self.with_node(*node, |component, cx| {
                        cx.view.set_frame_animated(*frame, animated);
                        component.set_frame(cx, *frame);
                    });
                }
                Mutation::SetEventMask { node, mask } => {
                    let Some(n) = self.nodes.get_mut(node) else {
                        continue;
                    };
                    n.mask = *mask;
                    n.view.set_event_mask(*mask);
                    self.with_node(*node, |component, cx| component.set_event_mask(cx, *mask));
                }
                Mutation::Command {
                    node,
                    command,
                    payload,
                } => {
                    self.with_node(*node, |component, cx| {
                        component.command(cx, *command, payload)
                    });
                }
            }
        }
        // The root presents the surfaces the batch changed once it is complete.
        touched.push(ROOT_NODE);
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
            let cx = NodeCx {
                node,
                view: &view,
                shared: &shared,
            };
            f(n.component.as_mut(), &cx);
        }
    }

    fn create(&mut self, node: u32, view_type: ViewType) {
        if node == ROOT_NODE {
            return;
        }
        if self.nodes.contains_key(&node) {
            // A live id reused: the old subtree is gone for the core.
            self.delete(node);
        }
        let view = NodeView::new();
        view.bind(node, &self.shared);
        let cx = NodeCx {
            node,
            view: &view,
            shared: &self.shared,
        };
        let component = component::create(view_type, &cx);
        self.nodes.insert(
            node,
            Node {
                view,
                view_type,
                component,
                parent: None,
                children: Vec::new(),
                mask: 0,
            },
        );
    }

    /// Insert and Move: attaches `node` as the `index`-th child of `parent`,
    /// detaching it from wherever it was.
    fn insert(&mut self, node: u32, parent: u32, index: usize) {
        if node == parent || node == ROOT_NODE || !self.nodes.contains_key(&parent) {
            return;
        }
        let Some(child) = self.nodes.get_mut(&node) else {
            return;
        };
        let old = child.parent.replace(parent);
        let child_view = child.view.clone();
        if let Some(old) = old {
            if let Some(p) = self.nodes.get_mut(&old) {
                p.children.retain(|c| *c != node);
            }
            if old != parent {
                self.with_node(old, |component, cx| component.remove_child(cx, &child_view));
            }
        }
        if let Some(p) = self.nodes.get_mut(&parent) {
            let at = index.min(p.children.len());
            p.children.insert(at, node);
        }
        self.with_node(parent, |component, cx| {
            component.insert_child(cx, &child_view, index)
        });
    }

    fn remove(&mut self, node: u32, parent: u32) {
        let Some(child) = self.nodes.get_mut(&node) else {
            return;
        };
        if child.parent != Some(parent) {
            return;
        }
        child.parent = None;
        let child_view = child.view.clone();
        if let Some(p) = self.nodes.get_mut(&parent) {
            p.children.retain(|c| *c != node);
        }
        self.with_node(parent, |component, cx| {
            component.remove_child(cx, &child_view)
        });
    }

    /// Deletes a subtree (the core sends `Delete` for the subtree root only),
    /// walking the recorded children: linear in the subtree's size.
    fn delete(&mut self, node: u32) {
        if node == ROOT_NODE {
            return;
        }
        let Some(n) = self.nodes.get(&node) else {
            return;
        };
        // A page or sheet animating out keeps its views; only the table forgets them.
        let keep_views = n.component.keeps_views_on_delete();
        match n.parent {
            Some(parent) if !keep_views => self.remove(node, parent),
            Some(parent) => {
                if let Some(p) = self.nodes.get_mut(&parent) {
                    p.children.retain(|c| *c != node);
                }
            }
            None => {}
        }
        let mut stack = vec![node];
        while let Some(id) = stack.pop() {
            self.with_node(id, |component, cx| component.unmount(cx));
            if let Some(n) = self.nodes.remove(&id) {
                stack.extend(n.children.iter().copied());
                if !keep_views {
                    n.view.finish_animations();
                    n.view.detach();
                }
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
        self.with_node(node, |component, cx| {
            handled = component.set_prop(cx, prop, value)
        });
        if !handled {
            if let Some(n) = self.nodes.get(&node) {
                apply_base_prop(&n.view, prop, value, self.animated);
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

/// Box, accessibility and interaction properties every node supports.
/// `PropValue::Null` (or a value of the wrong kind) restores the default.
pub fn apply_base_prop(view: &NodeView, prop: Prop, value: &PropValue, animated: bool) {
    match prop {
        Prop::BackgroundColor => view.update_style(|s| s.background = color(value)),
        Prop::CornerRadii => view.update_style(|s| s.radii = quad(value)),
        Prop::BorderWidths => view.update_style(|s| s.border_widths = quad(value)),
        Prop::BorderColor => view.update_style(|s| s.border_color = color(value)),
        Prop::ClipsToBounds => {
            view.update_style(|s| s.clips = matches!(value, PropValue::Bool(true)))
        }
        Prop::Shadow => {
            view.update_style(|s| s.shadow = json(value).and_then(|v| parse_shadow(&v)))
        }
        Prop::Gradient => {
            view.set_gradient(json(value).and_then(|v| node_view::parse_gradient(&v)))
        }
        Prop::Opacity => view.set_opacity_animated(float(value).unwrap_or(1.0) as f64, animated),
        Prop::Hidden => view.set_visible(!matches!(value, PropValue::Bool(true))),
        Prop::Transform => view.set_transform(json(value).map(|v| parse_transform(&v))),
        Prop::Animation => view.set_animation(match value {
            PropValue::Json(s) => AnimationSpec::parse(s),
            _ => None,
        }),
        Prop::TestId => view.set_widget_name(match value {
            PropValue::Str(s) => s,
            _ => "",
        }),
        Prop::ActiveOpacity => {
            view.update_press(|p| p.active_opacity = float(value).map(|o| o.clamp(0.0, 1.0)))
        }
        Prop::ActiveBackground => view.update_press(|p| p.active_background = color(value)),
        Prop::HitSlop => view.set_hit_slop(quad(value)),
        // Bool, or the tab order as I32 (GTK orders focus by the widget tree).
        Prop::Focusable => view.set_focusable_node(match value {
            PropValue::Bool(b) => *b,
            PropValue::I32(_) => true,
            _ => false,
        }),
        Prop::A11yLabel
        | Prop::A11yRole
        | Prop::A11yHint
        | Prop::A11yValue
        | Prop::A11yState
        | Prop::A11yHidden => {
            crate::a11y::set(view, prop, value);
        }
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
        PropValue::F32(f) if f.is_finite() => Some(*f),
        PropValue::I32(i) => Some(*i as f32),
        _ => None,
    }
}

/// Four sides or corners; a single value applies to all four.
pub fn quad(value: &PropValue) -> [f32; 4] {
    let clean = |v: f32| if v.is_finite() { v } else { 0.0 };
    match value {
        PropValue::F32Array(a) if a.len() >= 4 => {
            [clean(a[0]), clean(a[1]), clean(a[2]), clean(a[3])]
        }
        PropValue::F32Array(a) if a.len() == 1 => [clean(a[0]); 4],
        _ => [0.0; 4],
    }
}

pub fn json(value: &PropValue) -> Option<serde_json::Value> {
    match value {
        PropValue::Json(s) => serde_json::from_str(s).ok(),
        _ => None,
    }
}

fn num(v: &serde_json::Value, key: &str) -> Option<f32> {
    v.get(key)
        .and_then(|x| x.as_f64())
        .filter(|x| x.is_finite())
        .map(|x| x as f32)
}

fn parse_shadow(v: &serde_json::Value) -> Option<Shadow> {
    Some(Shadow {
        color: v.get("color")?.as_u64()? as u32,
        x: num(v, "x").unwrap_or(0.0),
        y: num(v, "y").unwrap_or(0.0),
        blur: num(v, "blur").unwrap_or(0.0).max(0.0),
        spread: num(v, "spread").unwrap_or(0.0),
    })
}

/// `{translate_x, translate_y, scale, scale_x, scale_y, rotate}`: translate,
/// then rotate (degrees), then scale; applied around the center.
pub fn parse_transform(v: &serde_json::Value) -> gsk::Transform {
    let scale = num(v, "scale").unwrap_or(1.0);
    gsk::Transform::new()
        .translate(&graphene::Point::new(
            num(v, "translate_x").unwrap_or(0.0),
            num(v, "translate_y").unwrap_or(0.0),
        ))
        .rotate(num(v, "rotate").unwrap_or(0.0))
        .scale(
            num(v, "scale_x").unwrap_or(scale),
            num(v, "scale_y").unwrap_or(scale),
        )
}
