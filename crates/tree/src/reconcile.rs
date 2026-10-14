//! Descriptions against the shadow tree.
//!
//! Children match per parent by explicit key, else by position among the unkeyed
//! siblings. A match whose kind or native view type differs is recreated with
//! its subtree. An entity whose description is the same `Rc` as last commit,
//! under the same inherited text style and epoch, is not visited at all.

use std::collections::HashMap;
use std::rc::Rc;

use native_shell_core::desc::{CommitInput, Desc, ElementKind, EntityId, Surface, SurfaceKind};
use native_shell_core::protocol::{Mutation, Prop, PropValue, ViewType, ROOT_NODE};
use native_shell_core::style::{ScrollAxis, TextStyle, DEFAULT_LINE_HEIGHT, DEFAULT_TEXT_SIZE};
use taffy::prelude::*;

use crate::props::{self, TextEnv};
use crate::{Batch, Key, Kind, ListState, Measure, Node, SurfaceTag, Tree};

/// Entities may nest (an entity rendering only another entity); deeper chains are a script bug.
const MAX_ENTITY_CHAIN: usize = 64;

/// Where a child is being reconciled.
struct Parent {
    id: u32,
    native: u32,
    root: u32,
    depth: u32,
    list: u32,
    text: TextStyle,
}

#[derive(Clone, Copy)]
enum KeyRef<'a> {
    Explicit(&'a str),
    Pos(u32),
    Row(u32),
}

impl<'a> KeyRef<'a> {
    fn of(k: &'a Key) -> KeyRef<'a> {
        match k {
            Key::Explicit(s) => KeyRef::Explicit(s),
            Key::Pos(i) => KeyRef::Pos(*i),
            Key::Row(i) => KeyRef::Row(*i),
        }
    }

    fn to_owned(self) -> Key {
        match self {
            KeyRef::Explicit(s) => Key::Explicit(s.to_string()),
            KeyRef::Pos(i) => Key::Pos(i),
            KeyRef::Row(i) => Key::Row(i),
        }
    }

    fn matches(self, k: &Key) -> bool {
        match (self, k) {
            (KeyRef::Explicit(a), Key::Explicit(b)) => a == b,
            (KeyRef::Pos(a), Key::Pos(b)) | (KeyRef::Row(a), Key::Row(b)) => a == *b,
            _ => false,
        }
    }
}

fn kind_of(desc: &Desc) -> Kind {
    match &desc.kind {
        ElementKind::View | ElementKind::Entity(_) => Kind::View,
        ElementKind::Text(_) => Kind::Text,
        ElementKind::Image => Kind::Image,
        ElementKind::TextInput => Kind::TextInput,
        ElementKind::Switch => Kind::Switch,
        ElementKind::Checkbox => Kind::Checkbox,
        ElementKind::Slider => Kind::Slider,
        ElementKind::Progress => Kind::Progress,
        ElementKind::Spinner => Kind::Spinner,
        ElementKind::Segmented => Kind::Segmented,
        ElementKind::DatePicker => Kind::DatePicker,
        ElementKind::Select => Kind::Select,
        ElementKind::Shape { .. } => Kind::Shape,
        ElementKind::List(_) => Kind::List,
        ElementKind::ListRow(_) => Kind::ListRow,
        ElementKind::HostComponent(name) => Kind::Host(name.clone()),
    }
}

/// The flattening rule: a view that paints nothing, listens to nothing (handlers include
/// hover), carries no accessibility, test, tooltip or ref identity, has no press or pointer
/// feedback, is visible and does not scroll needs no native view.
fn flattenable(desc: &Desc) -> bool {
    let s = &desc.style;
    !s.visual.paints()
        && !s.has_border()
        && desc.handlers.is_empty()
        && desc.props.is_empty()
        && desc.ref_id.is_none()
        && !s.is_hidden()
        && s.scroll == ScrollAxis::None
        && s.transitions.is_empty()
        && s.active_opacity.is_none()
        && s.active_background.is_none()
        && s.hover_opacity.is_none()
        && s.hover_background.is_none()
        && s.cursor.is_none()
}

fn native_type(desc: &Desc) -> Option<ViewType> {
    Some(match &desc.kind {
        ElementKind::View | ElementKind::Entity(_) => {
            if desc.style.scroll != ScrollAxis::None {
                ViewType::Scroll
            } else if flattenable(desc) {
                return None;
            } else {
                ViewType::View
            }
        }
        ElementKind::Text(_) => ViewType::Text,
        ElementKind::Image => ViewType::Image,
        ElementKind::TextInput => ViewType::TextInput,
        ElementKind::Switch => ViewType::Switch,
        ElementKind::Checkbox => ViewType::Checkbox,
        ElementKind::Slider => ViewType::Slider,
        ElementKind::Progress => ViewType::Progress,
        ElementKind::Spinner => ViewType::Spinner,
        ElementKind::Segmented => ViewType::Segmented,
        ElementKind::DatePicker => ViewType::DatePicker,
        ElementKind::Select => ViewType::Select,
        ElementKind::Shape { .. } => ViewType::Shape,
        ElementKind::List(_) => ViewType::List,
        ElementKind::ListRow(_) => ViewType::ListRow,
        ElementKind::HostComponent(_) => ViewType::HostComponent,
    })
}

fn surface_tag(kind: &SurfaceKind) -> SurfaceTag {
    match kind {
        SurfaceKind::Root => SurfaceTag::Root,
        SurfaceKind::Page { .. } => SurfaceTag::Page,
        SurfaceKind::Sheet { .. } => SurfaceTag::Sheet,
        SurfaceKind::Dialog { .. } => SurfaceTag::Dialog,
        SurfaceKind::Alert { .. } => SurfaceTag::Alert,
    }
}

/// Sizes of controls whose style does not give one. Natively these controls have a fixed
/// look; the numbers are the iOS defaults, close enough on Android.
///
/// | element | default |
/// | --- | --- |
/// | Switch | 51 × 31 |
/// | Checkbox / Radio, Spinner | 24 × 24 |
/// | Slider | height 32 |
/// | Progress | height 4 |
/// | Segmented | height 32 |
/// | DatePicker, Select | height 36 when the style sets a width; otherwise measured by the host (`nsh/select`, `nsh/date-picker`), 96 × 36 if it cannot |
/// | TextInput | height `size × 1.2 + 16`, multiline `size × 1.2 × 3 + 16` |
fn default_size(kind: &Kind, desc: &Desc, text: &TextStyle) -> (Option<f32>, Option<f32>) {
    match kind {
        Kind::Switch => (Some(51.0), Some(31.0)),
        Kind::Checkbox | Kind::Spinner => (Some(24.0), Some(24.0)),
        Kind::Slider | Kind::Segmented => (None, Some(32.0)),
        Kind::Progress => (None, Some(4.0)),
        Kind::DatePicker | Kind::Select if !measured_control(desc) => (None, Some(36.0)),
        Kind::TextInput => {
            let size = text.size.unwrap_or(DEFAULT_TEXT_SIZE);
            let lines = if desc.prop(Prop::Multiline) == Some(&PropValue::Bool(true)) {
                3.0
            } else {
                1.0
            };
            (None, Some(size * 1.2 * lines + 16.0))
        }
        _ => (None, None),
    }
}

/// Select and DatePicker size to what they show unless the style gives them a width.
fn measured_control(desc: &Desc) -> bool {
    matches!(desc.kind, ElementKind::Select | ElementKind::DatePicker)
        && desc.style.layout.size.width.is_auto()
}

/// The reserved component a built-in control is measured as, and the props of everything it
/// can show (`docs/protocol-json.md`, "Built-in control measurement"). The control is as wide
/// as the widest of them, so its width does not jump when the selection changes.
fn control_measure(desc: &Desc, props: &[(Prop, PropValue)]) -> Option<Measure> {
    let prop = |p: Prop| props.iter().find(|(k, _)| *k == p).map(|(_, v)| v);
    let one = |key: &str, value: &str| -> Rc<str> {
        let mut m = serde_json::Map::new();
        m.insert(key.into(), value.into());
        Rc::from(serde_json::Value::Object(m).to_string())
    };
    let (name, candidates) = match desc.kind {
        ElementKind::Select => {
            let menu = match prop(Prop::Menu) {
                Some(PropValue::Json(j)) => {
                    serde_json::from_str::<serde_json::Value>(j).unwrap_or_default()
                }
                _ => serde_json::Value::Null,
            };
            let mut c: Vec<Rc<str>> = menu["items"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|it| it["label"].as_str())
                .map(|label| one("label", label))
                .collect();
            if let Some(p) = menu["placeholder"].as_str() {
                c.push(one("placeholder", p));
            }
            if c.is_empty() {
                c.push(Rc::from("{}"));
            }
            ("nsh/select", c)
        }
        ElementKind::DatePicker => {
            let value = match prop(Prop::Date) {
                Some(PropValue::Str(v)) => v.as_str(),
                _ => "",
            };
            let mode = match prop(Prop::DateMode) {
                Some(PropValue::I32(1)) => "time",
                Some(PropValue::I32(2)) => "datetime",
                _ => "date",
            };
            ("nsh/date-picker", date_candidates(mode, value))
        }
        _ => return None,
    };
    Some(Measure::Widest {
        name: Rc::from(name),
        candidates,
    })
}

/// Values covering a picker's widest localized output: month names differ in width (the
/// 28th exists in every month) and 12-hour times differ from 24-hour ones (10:58, 22:58).
fn date_candidates(mode: &str, value: &str) -> Vec<Rc<str>> {
    let year = value
        .get(..4)
        .filter(|y| y.bytes().all(|b| b.is_ascii_digit()))
        .map(str::to_string)
        .unwrap_or_else(current_year);
    let props = |v: String| -> Rc<str> {
        Rc::from(serde_json::json!({ "mode": mode, "value": v }).to_string())
    };
    let dates = || (1..=12).map(|m| format!("{year}-{m:02}-28"));
    match mode {
        "time" => ["10:58", "22:58"].map(|t| props(t.to_string())).to_vec(),
        "datetime" => dates()
            .flat_map(|d| ["10:58", "22:58"].map(move |t| format!("{d}T{t}")))
            .map(props)
            .collect(),
        _ => dates().map(props).collect(),
    }
}

/// The current year (UTC), for a picker that has no value yet.
fn current_year() -> String {
    let days = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() / 86_400) as i64;
    // Days since 1970-01-01 to a civil year (Howard Hinnant's `civil_from_days`).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let year = yoe + era * 400 + i64::from(mp >= 10);
    year.to_string()
}

impl Tree {
    fn text_env(&self) -> TextEnv {
        TextEnv {
            color: self.default_text_color,
            rem: self.rem_size(),
        }
    }

    pub(crate) fn reconcile_surfaces(&mut self, b: &mut Batch, input: &CommitInput) {
        let old = std::mem::take(&mut self.surfaces);
        let mut by_key: HashMap<String, u32> = std::mem::take(&mut self.surface_by_key);
        let mut new = Vec::with_capacity(input.surfaces.len());
        for s in input.surfaces {
            let tag = Kind::Surface(surface_tag(&s.kind));
            let id = match by_key.remove(&s.key) {
                Some(id) if self.nodes.get(&id).is_some_and(|n| n.kind == tag) => id,
                other => {
                    if let Some(stale) = other {
                        self.delete_subtree(b, stale, true);
                    }
                    self.create_surface(b, s, tag)
                }
            };
            self.update_surface(b, input, id, s);
            new.push(id);
            self.surface_by_key.insert(s.key.clone(), id);
        }
        for (_, id) in by_key {
            if !new.contains(&id) {
                self.delete_subtree(b, id, true);
            }
        }
        if new != old {
            b.native_dirty(ROOT_NODE);
        }
        self.surfaces = new;
    }

    fn create_surface(&mut self, b: &mut Batch, s: &Surface, tag: Kind) -> u32 {
        let id = self.alloc();
        let view_type = match s.kind {
            SurfaceKind::Root => ViewType::View,
            SurfaceKind::Page { .. } => ViewType::Page,
            SurfaceKind::Sheet { .. } => ViewType::Sheet,
            SurfaceKind::Dialog { .. } | SurfaceKind::Alert { .. } => ViewType::Dialog,
        };
        let taffy = self.taffy.new_leaf(Style::default()).expect("taffy leaf");
        let mut node = self.blank(tag, Some(view_type), Key::Explicit(s.key.clone()), taffy);
        node.native_parent = ROOT_NODE;
        node.layout_root = id;
        node.depth = 1;
        // GPUI's root text style, inherited by everything in the surface.
        node.text = TextStyle {
            size: Some(DEFAULT_TEXT_SIZE),
            line_height: Some(DEFAULT_LINE_HEIGHT),
            ..TextStyle::default()
        };
        node.pending.push(Mutation::Create {
            node: id,
            view_type,
        });
        self.nodes.insert(id, node);
        b.created.push(id);
        b.native_dirty(id);
        b.root_dirty(id);
        id
    }

    fn update_surface(&mut self, b: &mut Batch, input: &CommitInput, id: u32, s: &Surface) {
        let env = &self.env;
        let mut layout = self.base_style.layout.clone();
        let (w, h) = match s.kind {
            SurfaceKind::Root | SurfaceKind::Page { .. } => (Some(env.width), Some(env.height)),
            SurfaceKind::Sheet { .. } => (Some(env.width), None),
            SurfaceKind::Dialog { .. } => (Some((env.width - 48.0).clamp(0.0, 400.0)), None),
            SurfaceKind::Alert { .. } => (None, None),
        };
        layout.size = Size {
            width: w.map_or(Dimension::auto(), length),
            height: h.map_or(Dimension::auto(), length),
        };
        let mut props = Vec::new();
        let mut mask = native_shell_core::protocol::event_mask::DISMISS;
        match &s.kind {
            SurfaceKind::Root => mask = 0,
            SurfaceKind::Page {
                title,
                can_go_back,
                nav_bar_hidden,
            } => {
                props.push((Prop::Title, PropValue::Str(title.clone())));
                props.push((Prop::CanGoBack, PropValue::Bool(*can_go_back)));
                props.push((Prop::NavBarHidden, PropValue::Bool(*nav_bar_hidden)));
            }
            SurfaceKind::Sheet {
                detents,
                dismissable,
            } => {
                // Entries are names ("medium") or JSON objects ({"height": 320}).
                let entries: Vec<serde_json::Value> = detents
                    .iter()
                    .map(|d| match serde_json::from_str::<serde_json::Value>(d) {
                        Ok(v @ serde_json::Value::Object(_)) => v,
                        _ => serde_json::Value::String(d.clone()),
                    })
                    .collect();
                let json = serde_json::Value::Array(entries).to_string();
                props.push((Prop::Detents, PropValue::Json(json)));
                props.push((Prop::Dismissable, PropValue::Bool(*dismissable)));
            }
            SurfaceKind::Dialog { dismissable } => {
                props.push((Prop::Dismissable, PropValue::Bool(*dismissable)))
            }
            SurfaceKind::Alert { spec_json } => {
                props.push((Prop::AlertSpec, PropValue::Json(spec_json.clone())));
                mask |= native_shell_core::protocol::event_mask::ACTION;
            }
        }
        props.sort_by_key(|(p, _)| *p as u16);

        let n = self.nodes.get(&id).expect("surface node");
        if *self.taffy.style(n.taffy).expect("taffy style") != layout {
            let _ = self.taffy.set_style(n.taffy, layout);
            b.root_dirty(id);
        }
        self.apply_props(b, id, props, mask);

        let content: &[Desc] = if matches!(s.kind, SurfaceKind::Alert { .. }) {
            &[]
        } else {
            std::slice::from_ref(&s.content)
        };
        self.reconcile_children(b, input, id, content);
    }

    fn blank(&self, kind: Kind, native: Option<ViewType>, key: Key, taffy: taffy::NodeId) -> Node {
        Node {
            kind,
            native,
            key,
            parent: ROOT_NODE,
            native_parent: ROOT_NODE,
            layout_root: 0,
            depth: 0,
            list: 0,
            children: Vec::new(),
            taffy,
            style: self.base_style.clone(),
            src_kind: None,
            src_props: Vec::new(),
            handlers: Vec::new(),
            ref_id: None,
            inherited: TextStyle::default(),
            text: TextStyle::default(),
            entity: None,
            entity_id: None,
            nested: Vec::new(),
            epoch: self.epoch,
            force: true,
            props: Vec::new(),
            layout_props: Vec::new(),
            mask: 0,
            frame: None,
            abs: crate::Frame {
                x: 0.0,
                y: 0.0,
                width: 0.0,
                height: 0.0,
            },
            fresh: true,
            pending: Vec::new(),
        }
    }

    /// Diffs `props` into the node's current set and the event mask, routing the ops to the
    /// node's creation block when it was created in this commit.
    pub(crate) fn apply_props(
        &mut self,
        b: &mut Batch,
        id: u32,
        props: Vec<(Prop, PropValue)>,
        mask: u32,
    ) {
        let Some(n) = self.nodes.get_mut(&id) else {
            return;
        };
        let out = if n.fresh {
            &mut n.pending
        } else {
            &mut b.updates
        };
        props::diff(&mut n.props, props, |prop, value| {
            out.push(Mutation::SetProp {
                node: id,
                prop,
                value,
            })
        });
        if n.mask != mask {
            n.mask = mask;
            out.push(Mutation::SetEventMask { node: id, mask });
        }
    }

    /// Sets a prop computed by layout (content size, row offsets, insets) if it changed;
    /// `Null` removes it (sent only if it was set).
    pub(crate) fn set_layout_prop(&mut self, b: &mut Batch, id: u32, prop: Prop, value: PropValue) {
        let Some(n) = self.nodes.get_mut(&id) else {
            return;
        };
        let at = n.layout_props.iter().position(|(p, _)| *p == prop);
        match (at, &value) {
            (None, PropValue::Null) => return,
            (Some(i), PropValue::Null) => {
                n.layout_props.remove(i);
            }
            (Some(i), _) if n.layout_props[i].1 == value => return,
            (Some(i), _) => n.layout_props[i].1 = value.clone(),
            (None, _) => n.layout_props.push((prop, value.clone())),
        }
        let out = if n.fresh {
            &mut n.pending
        } else {
            &mut b.updates
        };
        out.push(Mutation::SetProp {
            node: id,
            prop,
            value,
        });
    }

    pub(crate) fn reconcile_children(
        &mut self,
        b: &mut Batch,
        input: &CommitInput,
        id: u32,
        descs: &[Desc],
    ) {
        let Some(parent) = self.parent_ctx(id) else {
            return;
        };
        let Some(n) = self.nodes.get_mut(&id) else {
            return;
        };
        let old = std::mem::take(&mut n.children);
        let lays_out_children = !matches!(n.kind, Kind::List | Kind::Text);

        let mut slots: Vec<Option<u32>> = old.iter().copied().map(Some).collect();
        let mut index: Option<HashMap<Key, usize>> = None;
        let mut new = Vec::with_capacity(descs.len());
        let mut unkeyed = 0;
        // An entity with no description renders nothing (it takes no position either).
        for (i, d) in descs.iter().filter(|d| resolves(d, input)).enumerate() {
            let key = key_of(d, input, &mut unkeyed);
            let positional = slots
                .get(i)
                .copied()
                .flatten()
                .filter(|o| self.nodes.get(o).is_some_and(|n| key.matches(&n.key)));
            let found = match positional {
                Some(o) => {
                    slots[i] = None;
                    Some(o)
                }
                None => {
                    let index = index.get_or_insert_with(|| {
                        let mut m = HashMap::with_capacity(slots.len());
                        for (j, s) in slots.iter().enumerate() {
                            if let Some(n) = s.and_then(|s| self.nodes.get(&s)) {
                                m.insert(n.key.clone(), j);
                            }
                        }
                        m
                    });
                    index.remove(&key.to_owned()).and_then(|j| slots[j].take())
                }
            };
            new.push(self.reconcile(b, input, found, d, key, &parent));
        }
        for stale in slots.into_iter().flatten() {
            self.delete_subtree(b, stale, true);
        }
        if new != old {
            b.native_dirty(parent.native);
            if lays_out_children {
                b.root_dirty(parent.root);
                if let Some(n) = self.nodes.get(&id) {
                    let kids: Vec<taffy::NodeId> = new
                        .iter()
                        .filter_map(|c| self.nodes.get(c))
                        .map(|c| c.taffy)
                        .collect();
                    let _ = self.taffy.set_children(n.taffy, &kids);
                }
            }
        }
        if let Some(n) = self.nodes.get_mut(&id) {
            n.children = new;
        }
    }

    /// The context children of `id` are reconciled in.
    fn parent_ctx(&self, id: u32) -> Option<Parent> {
        let n = self.nodes.get(&id)?;
        Some(Parent {
            id,
            native: if n.native.is_some() {
                id
            } else {
                n.native_parent
            },
            root: n.layout_root,
            depth: n.depth + 1,
            list: if n.kind == Kind::List { id } else { n.list },
            text: n.text.clone(),
        })
    }

    /// Nearest entity-backed descendants of `id` (not looking inside them).
    fn collect_nested(&self, id: u32, out: &mut Vec<u32>) {
        let Some(n) = self.nodes.get(&id) else { return };
        for c in &n.children {
            match self.nodes.get(c) {
                Some(child) if child.entity.is_some() => out.push(*c),
                Some(_) => self.collect_nested(*c, out),
                None => {}
            }
        }
    }

    /// `id` is an entity that did not re-render: its own subtree is unchanged, but entities
    /// nested in it may have. Visits only the nested entities, O(nested entities).
    fn descend_clean(&mut self, b: &mut Batch, input: &CommitInput, id: u32) {
        let Some(n) = self.nodes.get_mut(&id) else {
            return;
        };
        let mut nested = std::mem::take(&mut n.nested);
        for c in nested.iter_mut() {
            let Some(n) = self.nodes.get(c) else { continue };
            let Some(eid) = n.entity_id else { continue };
            let placeholder = Desc::new(ElementKind::Entity(eid), self.base_style.clone());
            // Still referenced by the unchanged parent description; a missing id is left as is.
            if !resolves(&placeholder, input) {
                continue;
            }
            let Some(parent) = self.parent_ctx(n.parent) else {
                continue;
            };
            let key = n.key.clone();
            let new = self.reconcile(b, input, Some(*c), &placeholder, KeyRef::of(&key), &parent);
            if new != *c {
                self.replace_child(b, &parent, *c, new);
                *c = new;
            }
        }
        if let Some(n) = self.nodes.get_mut(&id) {
            n.nested = nested;
        }
    }

    /// A child recreated outside its parent's reconcile (kind change of a nested entity).
    fn replace_child(&mut self, b: &mut Batch, parent: &Parent, old: u32, new: u32) {
        let Some(p) = self.nodes.get_mut(&parent.id) else {
            return;
        };
        for c in p.children.iter_mut().filter(|c| **c == old) {
            *c = new;
        }
        b.native_dirty(parent.native);
        let Some(p) = self.nodes.get(&parent.id) else {
            return;
        };
        if !matches!(p.kind, Kind::List | Kind::Text) {
            b.root_dirty(parent.root);
            let kids: Vec<taffy::NodeId> = p
                .children
                .iter()
                .filter_map(|c| self.nodes.get(c))
                .map(|c| c.taffy)
                .collect();
            let _ = self.taffy.set_children(p.taffy, &kids);
        }
    }

    fn reconcile(
        &mut self,
        b: &mut Batch,
        input: &CommitInput,
        old: Option<u32>,
        desc: &Desc,
        key: KeyRef,
        parent: &Parent,
    ) -> u32 {
        let mut desc = desc;
        let mut entity: Option<&Rc<Desc>> = None;
        let entity_id = match desc.kind {
            ElementKind::Entity(eid) => Some(eid),
            _ => None,
        };
        for _ in 0..MAX_ENTITY_CHAIN {
            let ElementKind::Entity(eid) = desc.kind else {
                break;
            };
            // `resolves` filtered out missing ids before this point.
            let Some(rc) = input.entities.get(&eid) else {
                break;
            };
            entity = Some(rc);
            desc = rc;
        }
        let kind = kind_of(desc);
        let native = native_type(desc);
        if let Some(id) = old {
            if let Some(n) = self.nodes.get(&id) {
                if n.kind == kind && n.native == native {
                    let unchanged_entity =
                        matches!((&n.entity, entity), (Some(a), Some(e)) if Rc::ptr_eq(a, e));
                    if unchanged_entity
                        && n.epoch == self.epoch
                        && n.inherited == parent.text
                        && !n.force
                    {
                        self.descend_clean(b, input, id);
                        return id;
                    }
                    self.update(b, input, id, desc, entity.cloned(), parent);
                    self.after_update(id, entity_id);
                    return id;
                }
            }
            self.delete_subtree(b, id, true);
        }
        let id = self.alloc();
        let taffy = self.taffy.new_leaf(Style::default()).expect("taffy leaf");
        let mut node = self.blank(kind, native, key.to_owned(), taffy);
        node.parent = parent.id;
        node.native_parent = parent.native;
        node.layout_root = if node.kind == Kind::ListRow {
            id
        } else {
            parent.root
        };
        node.depth = parent.depth;
        node.list = parent.list;
        if let Some(t) = native {
            node.pending.push(Mutation::Create {
                node: id,
                view_type: t,
            });
            if let ElementKind::HostComponent(name) = &desc.kind {
                node.pending.push(Mutation::SetProp {
                    node: id,
                    prop: Prop::HostComponent,
                    value: PropValue::Str(name.clone()),
                });
            }
            b.created.push(id);
            b.native_dirty(id);
        }
        let root = node.layout_root;
        self.nodes.insert(id, node);
        b.native_dirty(parent.native);
        b.root_dirty(root);
        self.update(b, input, id, desc, entity.cloned(), parent);
        self.after_update(id, entity_id);
        id
    }

    fn after_update(&mut self, id: u32, entity_id: Option<EntityId>) {
        let mut nested = Vec::new();
        if entity_id.is_some() {
            self.collect_nested(id, &mut nested);
        }
        if let Some(n) = self.nodes.get_mut(&id) {
            n.entity_id = entity_id;
            n.nested = nested;
        }
    }

    fn update(
        &mut self,
        b: &mut Batch,
        input: &CommitInput,
        id: u32,
        desc: &Desc,
        entity: Option<Rc<Desc>>,
        parent: &Parent,
    ) {
        let epoch = self.epoch;
        let n = self.nodes.get_mut(&id).expect("reconciled node");
        let changed = n.force
            || n.epoch != epoch
            || !Rc::ptr_eq(&n.style, &desc.style)
            || n.inherited != parent.text
            || n.src_kind.as_ref() != Some(&desc.kind)
            || n.src_props != desc.props
            || n.handlers != desc.handlers
            || n.ref_id != desc.ref_id;
        n.entity = entity;
        n.epoch = epoch;
        if changed {
            n.force = false;
            n.text = desc.style.text.inherit(&parent.text);
            n.inherited = parent.text.clone();
            n.style = desc.style.clone();
            n.src_kind = Some(desc.kind.clone());
            if n.src_props != desc.props {
                n.src_props = desc.props.clone();
            }
            if n.handlers != desc.handlers {
                n.handlers = desc.handlers.clone();
            }
            if n.ref_id != desc.ref_id {
                if let Some(old) = n.ref_id.take() {
                    if self.refs.get(&old) == Some(&id) {
                        self.refs.remove(&old);
                    }
                }
                n.ref_id = desc.ref_id.clone();
                if let Some(r) = &desc.ref_id {
                    self.refs.insert(r.clone(), id);
                }
            }
            self.apply_desc(b, id, desc);
            // A scroll container's display and direction decide its children's shrink
            // (see `apply_desc`): they must recompute even if their own descriptions did not change.
            if self
                .nodes
                .get(&id)
                .is_some_and(|n| n.native == Some(ViewType::Scroll))
            {
                let kids = self
                    .nodes
                    .get(&id)
                    .map(|n| n.children.clone())
                    .unwrap_or_default();
                for c in kids {
                    if let Some(c) = self.nodes.get_mut(&c) {
                        c.force = true;
                    }
                }
            }
        }
        if !matches!(desc.kind, ElementKind::Text(_)) {
            self.reconcile_children(b, input, id, &desc.children);
        }
    }

    /// Everything derived from one description: taffy style, measure context, list state, props.
    fn apply_desc(&mut self, b: &mut Batch, id: u32, desc: &Desc) {
        let text_env = self.text_env();
        let native_count = self.native_text.get(&id).copied().unwrap_or(0);
        let n = self.nodes.get(&id).expect("reconciled node");
        let (kind, native, root, tnode) = (n.kind.clone(), n.native, n.layout_root, n.taffy);

        // Rem lengths at the effective rem: they scale with the user's text size.
        let mut style = desc.style.scaled_layout(self.rem_size());
        let (dw, dh) = default_size(&kind, desc, &n.text);
        if let Some(w) = dw.filter(|_| style.size.width.is_auto()) {
            style.size.width = length(w);
        }
        if let Some(h) = dh.filter(|_| style.size.height.is_auto()) {
            style.size.height = length(h);
        }
        if kind == Kind::ListRow {
            let w = self.list_of(n.parent).map(|l| l.width).unwrap_or(0.0);
            style.size.width = length(w);
        }
        // Like a native scroll view's content, children of a scroll container never shrink
        // along the scrolling axis to fit the viewport. Block containers never shrink their
        // children: they stack them and let them overflow, which is already the scroll content.
        if let Some(p) = self.nodes.get(&n.parent).filter(|p| {
            p.native == Some(ViewType::Scroll) && p.style.layout.display == Display::Flex
        }) {
            let row = matches!(
                p.style.layout.flex_direction,
                FlexDirection::Row | FlexDirection::RowReverse
            );
            let scroll = p.style.scroll;
            if (row && scroll != ScrollAxis::Y) || (!row && scroll != ScrollAxis::X) {
                style.flex_shrink = 0.0;
            }
        }
        if *self.taffy.style(tnode).expect("taffy style") != style {
            let _ = self.taffy.set_style(tnode, style);
            b.root_dirty(root);
        }

        if let ElementKind::List(spec) = &desc.kind {
            let bottom = desc.prop(Prop::ListAlignment) == Some(&PropValue::I32(1));
            let window = crate::window_for(self.lists.get(&spec.id), spec.count, bottom);
            let l = self
                .lists
                .entry(spec.id.clone())
                .or_insert_with(|| ListState::new(spec.count));
            if (l.count, l.estimated, l.uniform)
                != (spec.count, spec.estimated_row_height, spec.uniform)
                || l.offsets.is_empty()
            {
                // Row offsets are recomputed when the list's root is laid out.
                b.root_dirty(root);
            }
            l.count = spec.count;
            l.estimated = spec.estimated_row_height;
            l.uniform = spec.uniform;
            l.window = window;
            l.heights.resize(spec.count as usize, None);
        }

        let Some(native) = native else { return };
        let n = self.nodes.get(&id).expect("reconciled node");
        let props = props::derive(desc, native, &n.text, &text_env, native_count);
        let mask = desc.handlers.iter().fold(0, |m, (e, _)| m | e.mask_bit());

        let measure = match &desc.kind {
            ElementKind::Text(_) => props
                .iter()
                .find(|(p, _)| *p == Prop::AttributedText)
                .and_then(|(_, v)| match v {
                    PropValue::Json(j) => Some(Measure::Text(Rc::from(j.as_str()))),
                    _ => None,
                }),
            ElementKind::HostComponent(name) => {
                let json = match desc.prop(Prop::HostProps) {
                    Some(PropValue::Json(j)) | Some(PropValue::Str(j)) => j.as_str(),
                    _ => "{}",
                };
                Some(Measure::Component {
                    name: Rc::from(name.as_str()),
                    props: Rc::from(json),
                })
            }
            _ if measured_control(desc) => control_measure(desc, &props),
            _ => None,
        };
        // A control that gained a definite width stops being measured.
        if measure.is_none() && self.taffy.get_node_context(tnode).is_some() {
            let _ = self.taffy.set_node_context(tnode, None);
            b.root_dirty(root);
        }
        if let Some(m) = measure {
            let same = self.taffy.get_node_context(tnode) == Some(&m);
            if !same {
                if let Measure::Text(json) = &m {
                    b.texts.push(json.clone());
                }
                let _ = self.taffy.set_node_context(tnode, Some(m));
                b.root_dirty(root);
            }
        }
        self.apply_props(b, id, props, mask);
        // A laid-out stretched shape whose path changed is redrawn at its current size;
        // a new one, or one whose size changes, is drawn when layout emits its frame.
        if matches!(desc.kind, ElementKind::Shape { stretch: true }) {
            if let Some(n) = self.nodes.get(&id).filter(|n| n.frame.is_some()) {
                let (w, h) = (n.abs.width, n.abs.height);
                self.stretch_to(b, id, w, h);
            }
        }
    }

    /// Sends a stretched shape its path and view box for a `w` × `h` frame.
    pub(crate) fn stretch_to(&mut self, b: &mut Batch, id: u32, w: f32, h: f32) {
        let Some(n) = self.nodes.get(&id) else {
            return;
        };
        let (path, view_box) = match crate::stretch::stretched_props(&n.src_props, w, h) {
            Some((p, v)) => (p, v),
            None => (PropValue::Null, PropValue::Null),
        };
        self.set_layout_prop(b, id, Prop::PathData, path);
        self.set_layout_prop(b, id, Prop::ViewBox, view_box);
    }

    /// Removes a subtree. With `emit`, sends `Delete` for its topmost native views (the
    /// renderer destroys what is below them).
    pub(crate) fn delete_subtree(&mut self, b: &mut Batch, id: u32, emit: bool) {
        let Some(n) = self.nodes.remove(&id) else {
            return;
        };
        if emit && n.native.is_some() {
            b.deletes.push(Mutation::Delete { node: id });
        }
        let child_emit = emit && n.native.is_none();
        for c in &n.children {
            self.delete_subtree(b, *c, child_emit);
        }
        let _ = self.taffy.remove(n.taffy);
        self.native_children.remove(&id);
        self.native_text.remove(&id);
        if let Some(r) = n.ref_id {
            if self.refs.get(&r) == Some(&id) {
                self.refs.remove(&r);
            }
        }
        if emit {
            b.native_dirty(n.native_parent);
            if n.layout_root != id {
                b.root_dirty(n.layout_root);
            }
        }
    }
}

/// False when the description is an entity chain ending in an id with no description.
fn resolves(d: &Desc, input: &CommitInput) -> bool {
    let mut d = d;
    for _ in 0..MAX_ENTITY_CHAIN {
        let ElementKind::Entity(eid) = d.kind else {
            return true;
        };
        match input.entities.get(&eid) {
            Some(rc) => d = rc,
            None => return false,
        }
    }
    false
}

fn key_of<'a>(d: &'a Desc, input: &'a CommitInput, unkeyed: &mut u32) -> KeyRef<'a> {
    let mut key = d.key.as_deref();
    if key.is_none() {
        if let ElementKind::Entity(eid) = d.kind {
            key = input.entities.get(&eid).and_then(|e| e.key.as_deref());
        }
    }
    if let Some(k) = key {
        return KeyRef::Explicit(k);
    }
    if let ElementKind::ListRow(i) = d.kind {
        return KeyRef::Row(i);
    }
    *unkeyed += 1;
    KeyRef::Pos(*unkeyed - 1)
}
