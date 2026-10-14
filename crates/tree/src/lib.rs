//! Keyed diff, view flattening, taffy layout and list windowing: descriptions in, mutations out.
//!
//! The tree keeps a shadow of every logical node a script described, mirrored in
//! a [`taffy::TaffyTree`], and a second, sparser tree of the native views the
//! renderer holds. A commit walks the new descriptions against the shadow and
//! produces the smallest batch that brings the renderer up to date:
//!
//! ```text
//! reconcile (keys, entity skips, props) ─▶ native child diff ─▶ taffy layout ─▶ frames
//! ```
//!
//! Three choices shape everything below:
//!
//! * **No reparenting.** A node keeps its parent for life. When its kind or its
//!   flattened status changes it is deleted and created again with its subtree,
//!   so a native view's native parent never changes and the child diff only has
//!   to reorder within one parent.
//! * **Layout-only views have no native view.** A `View` that draws nothing and
//!   listens to nothing is flattened: its children attach to the nearest native
//!   ancestor, frames offset by the flattened positions. On mobile this halves
//!   the view count of a typical screen.
//! * **Batches are ordered for the renderer, not for the algorithm.** Deletes
//!   first (they free indices), then creates with their initial props, then
//!   inserts and moves parent before child, then prop updates, then frames.
//!   Every index in an `Insert`/`Move` is valid at the moment it is applied.
//!
//! `Move { from, to }` removes the child at `from`, then inserts it so that it
//! ends up at index `to` of the resulting list.

mod children;
mod layout;
mod props;
mod reconcile;
pub mod testing;

use std::collections::HashMap;
use std::hash::{BuildHasherDefault, Hasher};
use std::rc::Rc;

use native_shell_core::desc::{CommitInput, Desc, ElementKind, EntityId, HandlerId};
use native_shell_core::host::{Environment, Measurer, TextMetrics};
use native_shell_core::protocol::{EventType, Mutation, Prop, PropValue, ViewType, ROOT_NODE};
use native_shell_core::style::{Resolved, TextStyle};
use taffy::{NodeId, TaffyTree};

pub use native_shell_core::protocol::Frame;

/// Rows a list renderer requests beyond the visible range unless the script says otherwise.
pub const DEFAULT_PREFETCH: f32 = 10.0;
/// Rows rendered before the renderer has reported a range.
pub const DEFAULT_WINDOW: u32 = 20;

/// Counters for tests and `check --print-tree`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct TreeStats {
    /// Logical nodes, flattened ones and surfaces included.
    pub nodes: usize,
    /// Nodes that have a native view.
    pub natives: usize,
    /// Layout-only views that have none.
    pub flattened: usize,
    /// Text measurements answered from the cache.
    pub measure_hits: u64,
    /// Single `measure_text` calls (constrained probes of texts that wrap).
    pub measure_misses: u64,
    /// `measure_text_batch` calls: at most one per commit.
    pub batch_calls: u64,
    /// Texts measured in those batches (max-content).
    pub batched_items: u64,
    /// Constrained probes answered from the max-content result: the text fits on the
    /// lines it has unconstrained.
    pub derived_hits: u64,
}

/// Component measurements by (name, props JSON, width mode, width bits).
pub(crate) type ComponentCache = HashMap<(Rc<str>, Rc<str>, u8, u32), Option<(f32, f32)>>;

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct Counters {
    pub hits: u64,
    pub misses: u64,
    pub batch_calls: u64,
    pub batched_items: u64,
    pub derived_hits: u64,
}

/// Node ids are dense small integers; SipHash would dominate the diff.
#[derive(Default)]
pub(crate) struct IdHasher(u64);

impl Hasher for IdHasher {
    fn write(&mut self, bytes: &[u8]) {
        for b in bytes {
            self.0 = (self.0 ^ *b as u64).wrapping_mul(0x0100_0000_01b3);
        }
    }

    fn write_u32(&mut self, n: u32) {
        self.0 = (n as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15);
    }

    fn finish(&self) -> u64 {
        self.0
    }
}

pub(crate) type IdMap<V> = HashMap<u32, V, BuildHasherDefault<IdHasher>>;

/// What a node is, for deciding whether a new description can update it in place.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Kind {
    Surface(SurfaceTag),
    View,
    Text,
    Image,
    TextInput,
    Switch,
    Checkbox,
    Slider,
    Progress,
    Spinner,
    Segmented,
    DatePicker,
    Select,
    Shape,
    List,
    ListRow,
    Host(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SurfaceTag {
    Root,
    Page,
    Sheet,
    Dialog,
    Alert,
}

/// Identity of a child among its siblings.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) enum Key {
    Explicit(String),
    /// Position among the unkeyed siblings.
    Pos(u32),
    /// Unkeyed list row: its index.
    Row(u32),
}

/// What a taffy leaf measures with.
#[derive(PartialEq)]
pub(crate) enum Measure {
    Text(Rc<str>),
    Component {
        name: Rc<str>,
        props: Rc<str>,
    },
    /// A control as wide as the widest thing it can show: one measurement per candidate
    /// props, the largest width and height.
    Widest {
        name: Rc<str>,
        candidates: Vec<Rc<str>>,
    },
}

pub(crate) struct Node {
    pub kind: Kind,
    /// `None`: flattened (layout only).
    pub native: Option<ViewType>,
    pub key: Key,
    pub parent: u32,
    /// Nearest native ancestor (`ROOT_NODE` for surfaces). Stable for the node's life.
    pub native_parent: u32,
    /// The node whose taffy tree this node is laid out in: its surface, or its list row.
    pub layout_root: u32,
    pub depth: u32,
    /// Enclosing list node, 0 when none.
    pub list: u32,
    pub children: Vec<u32>,
    pub taffy: NodeId,

    // Inputs of the last reconcile, compared to skip work.
    pub style: Rc<Resolved>,
    pub src_kind: Option<ElementKind>,
    pub src_props: Vec<(Prop, PropValue)>,
    pub handlers: Vec<(EventType, HandlerId)>,
    pub ref_id: Option<String>,
    /// Text style handed down by the parent, and the node's own after inheritance.
    pub inherited: TextStyle,
    pub text: TextStyle,
    /// The entity description this node was built from, kept alive so pointer equality stays meaningful.
    pub entity: Option<Rc<Desc>>,
    /// The entity id the parent described this node by (outermost of a chain).
    pub entity_id: Option<EntityId>,
    /// Entity-backed nodes only: the nearest entity-backed descendants. When this node is
    /// skipped as clean, only these are visited, since their descriptions may have changed
    /// without this one re-rendering.
    pub nested: Vec<u32>,
    pub epoch: u32,
    /// Re-derive props on the next commit even if the description did not change.
    pub force: bool,

    // What the renderer currently has.
    pub props: Vec<(Prop, PropValue)>,
    /// Props that come out of layout (content size, row offsets).
    pub layout_props: Vec<(Prop, PropValue)>,
    pub mask: u32,
    pub frame: Option<Frame>,
    /// Pixel-rounded frame in the layout root's coordinates.
    pub abs: Frame,

    /// Created in the current commit: its ops collect here, in order, until the batch is assembled.
    pub fresh: bool,
    pub pending: Vec<Mutation>,
}

pub(crate) struct ListState {
    pub count: u32,
    pub window: (u32, u32),
    pub estimated: f32,
    pub uniform: bool,
    pub width: f32,
    pub heights: Vec<Option<f32>>,
    /// `count + 1` entries, the last is the total.
    pub offsets: Vec<f32>,
}

/// Mutations of one commit, sorted into the order the batch is emitted in.
#[derive(Default)]
pub(crate) struct Batch {
    pub deletes: Vec<Mutation>,
    pub created: Vec<u32>,
    pub structure: Vec<Mutation>,
    pub updates: Vec<Mutation>,
    pub frames: Vec<Mutation>,
    pub dirty_native: Vec<u32>,
    pub dirty_native_set: IdMap<()>,
    pub dirty_roots: IdMap<()>,
    /// Attributed texts set in this commit, measured in one batch before layout.
    pub texts: Vec<Rc<str>>,
}

impl Batch {
    pub fn native_dirty(&mut self, id: u32) {
        if self.dirty_native_set.insert(id, ()).is_none() {
            self.dirty_native.push(id);
        }
    }

    pub fn root_dirty(&mut self, id: u32) {
        self.dirty_roots.insert(id, ());
    }
}

pub struct Tree {
    pub(crate) nodes: IdMap<Node>,
    pub(crate) taffy: TaffyTree<Measure>,
    next_id: u32,
    pub(crate) env: Environment,
    /// Bumped when something every description depends on changes (font scale, default color):
    /// entity subtrees built under an older epoch cannot be skipped.
    pub(crate) epoch: u32,
    pub(crate) default_text_color: u32,
    pub(crate) base_style: Rc<Resolved>,
    pub(crate) surfaces: Vec<u32>,
    pub(crate) surface_by_key: HashMap<String, u32>,
    pub(crate) native_children: IdMap<Vec<u32>>,
    pub(crate) refs: HashMap<String, u32>,
    pub(crate) lists: HashMap<String, ListState>,
    pub(crate) native_text: IdMap<u32>,
    /// Text measurements by (attributed JSON, width mode, width bits).
    pub(crate) text_cache: HashMap<(Rc<str>, u8, u32), TextMetrics>,
    pub(crate) counters: Counters,
    pub(crate) component_cache: ComponentCache,
    /// Lay out every surface on the next commit (environment or measurement changed).
    pub(crate) layout_all: bool,
}

impl Default for Tree {
    fn default() -> Self {
        Tree::new()
    }
}

impl Tree {
    pub fn new() -> Tree {
        let mut taffy = TaffyTree::new();
        // Frames are rounded to the device pixel grid here, not to whole points.
        taffy.disable_rounding();
        let base_style = Rc::new(Resolved::base());
        Tree {
            nodes: IdMap::default(),
            taffy,
            next_id: 1,
            env: Environment::default(),
            epoch: 0,
            default_text_color: 0xFF0A_0A0A,
            base_style,
            surfaces: Vec::new(),
            surface_by_key: HashMap::new(),
            native_children: IdMap::default(),
            refs: HashMap::new(),
            lists: HashMap::new(),
            native_text: IdMap::default(),
            text_cache: HashMap::new(),
            counters: Counters::default(),
            component_cache: ComponentCache::default(),
            layout_all: true,
        }
    }

    pub(crate) fn alloc(&mut self) -> u32 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    /// Takes effect on the next commit.
    pub fn set_environment(&mut self, env: &Environment) {
        if *env == self.env {
            return;
        }
        let font_scale_changed = env.font_scale != self.env.font_scale;
        self.env = env.clone();
        self.layout_all = true;
        if font_scale_changed {
            // Text sizes stay unscaled logical points (renderers apply the user's font scale),
            // but measured sizes and the effective rem change: every node re-derives its
            // layout lengths, and nothing measured under the old scale may be reused.
            self.epoch += 1;
            self.clear_measure_cache();
        }
    }

    /// The effective rem size: rem lengths are laid out at it (see
    /// `native_shell_core::style::rem_size`); text sizes are not.
    pub fn rem_size(&self) -> f32 {
        native_shell_core::style::rem_size(self.env.font_scale)
    }

    /// Color of text whose style chain sets none (the theme's `foreground`).
    pub fn set_default_text_color(&mut self, color: u32) {
        if color != self.default_text_color {
            self.default_text_color = color;
            self.epoch += 1;
        }
    }

    /// Diff, flatten, lay out every surface; returns the batch body (no Begin/EndBatch).
    pub fn commit(&mut self, input: &CommitInput, measurer: &dyn Measurer) -> Vec<Mutation> {
        let mut b = Batch::default();
        self.reconcile_surfaces(&mut b, input);
        self.sync_native_children(&mut b);
        self.layout(&mut b, measurer);
        self.layout_all = false;

        let mut out = b.deletes;
        for id in &b.created {
            if let Some(n) = self.nodes.get_mut(id) {
                n.fresh = false;
                out.append(&mut n.pending);
            }
        }
        out.extend(b.structure);
        out.extend(b.updates);
        out.extend(b.frames);
        out
    }

    pub fn handler(&self, node: u32, event: EventType) -> Option<HandlerId> {
        self.nodes
            .get(&node)?
            .handlers
            .iter()
            .find(|(e, _)| *e == event)
            .map(|(_, h)| *h)
    }

    pub fn node_for_ref(&self, ref_id: &str) -> Option<u32> {
        self.refs.get(ref_id).copied()
    }

    /// The list a node belongs to and its id, for RangeChange events.
    pub fn list_id(&self, node: u32) -> Option<&str> {
        let n = self.nodes.get(&node)?;
        let list = if n.kind == Kind::List {
            n
        } else {
            self.nodes.get(&n.list)?
        };
        match &list.src_kind {
            Some(ElementKind::List(spec)) => Some(&spec.id),
            _ => None,
        }
    }

    /// Current window `[first, last)` of a list (default `[0, min(count, 20))`; `[0, 20)` for a list
    /// never committed, which the caller clamps to its count).
    pub fn list_window(&self, list_id: &str) -> (u32, u32) {
        match self.lists.get(list_id) {
            Some(l) => l.window,
            None => (0, DEFAULT_WINDOW),
        }
    }

    /// The window to render a list with `count` rows in, before this commit: the current
    /// window, except that a bottom-aligned list (`ListAlignment` 1) opens at its tail and,
    /// while its window reaches the end, follows rows appended there. The commit applies the
    /// same rule, so rendering with this window needs no second pass.
    pub fn list_window_for(&self, list_id: &str, count: u32, bottom: bool) -> (u32, u32) {
        window_for(self.lists.get(list_id), count, bottom)
    }

    /// Records the range the renderer reported. Returns true if it changed (re-render the list).
    pub fn set_list_window(&mut self, list_id: &str, first: u32, last: u32) -> bool {
        let l = self
            .lists
            .entry(list_id.to_string())
            .or_insert_with(|| ListState::new(NEVER_COMMITTED));
        let last = last.min(l.count);
        let window = (first.min(last), last);
        if l.window == window {
            return false;
        }
        l.window = window;
        true
    }

    /// Native text changed: record it so the next diff does not echo it back.
    pub fn record_native_text(&mut self, node: u32, text: &str, count: u32) {
        let Some(n) = self.nodes.get_mut(&node) else {
            return;
        };
        self.native_text.insert(node, count);
        let value = PropValue::Json(props::text_value_json(text, count));
        match n
            .props
            .binary_search_by_key(&(Prop::TextValue as u16), |(p, _)| *p as u16)
        {
            Ok(i) => n.props[i].1 = value,
            Err(i) => n.props.insert(i, (Prop::TextValue, value)),
        }
        // A controlled field whose script ignores the change must be reverted on the next commit,
        // even inside entities that do not re-render: none of them may be skipped.
        n.force = true;
        let mut at = n.parent;
        while let Some(p) = self.nodes.get_mut(&at) {
            if p.entity.is_some() {
                p.force = true;
            }
            at = p.parent;
        }
    }

    pub fn native_text_count(&self, node: u32) -> u32 {
        self.native_text.get(&node).copied().unwrap_or(0)
    }

    /// Font scale or theme changed under the same descriptions: measure every text again.
    pub fn clear_measure_cache(&mut self) {
        self.text_cache.clear();
        self.component_cache.clear();
        for n in self.nodes.values() {
            if matches!(
                n.kind,
                Kind::Text | Kind::Host(_) | Kind::Select | Kind::DatePicker
            ) {
                let _ = self.taffy.mark_dirty(n.taffy);
            }
        }
        self.layout_all = true;
    }

    pub fn find_by_test_id(&self, id: &str) -> Option<u32> {
        self.nodes
            .iter()
            .filter(|(_, n)| {
                n.props
                    .iter()
                    .any(|(p, v)| *p == Prop::TestId && matches!(v, PropValue::Str(s) if s == id))
            })
            .map(|(k, _)| *k)
            .min()
    }

    /// Absolute frame in surface coordinates. Rows are placed at their offset in the list's
    /// content, ignoring the list's scroll position.
    pub fn frame(&self, node: u32) -> Option<Frame> {
        let n = self.nodes.get(&node)?;
        let (ox, oy) = self.root_origin(n.layout_root)?;
        Some(Frame {
            x: n.abs.x + ox,
            y: n.abs.y + oy,
            ..n.abs
        })
    }

    fn root_origin(&self, root: u32) -> Option<(f32, f32)> {
        let r = self.nodes.get(&root)?;
        if r.kind != Kind::ListRow {
            return Some((0.0, 0.0));
        }
        let list = self.frame(r.parent)?;
        let index = match r.src_kind {
            Some(ElementKind::ListRow(i)) => i,
            _ => 0,
        };
        let offset = self
            .list_of(r.parent)
            .and_then(|l| l.offsets.get(index as usize).copied())
            .unwrap_or(0.0);
        Some((list.x, list.y + offset))
    }

    pub(crate) fn list_of(&self, list_node: u32) -> Option<&ListState> {
        match &self.nodes.get(&list_node)?.src_kind {
            Some(ElementKind::List(spec)) => self.lists.get(&spec.id),
            _ => None,
        }
    }

    /// The native children the renderer holds for `node` (`ROOT_NODE` for the surfaces).
    pub fn native_children(&self, node: u32) -> Vec<u32> {
        self.native_children.get(&node).cloned().unwrap_or_default()
    }

    /// Native node of a surface after commit, by its key (`Root`: its `View` under `ROOT_NODE`).
    pub fn surface_node(&self, key: &str) -> Option<u32> {
        self.surface_by_key.get(key).copied()
    }

    /// Key of the surface a native node is (for Dismiss / Action / BackPressed events).
    pub fn surface_for_node(&self, node: u32) -> Option<&str> {
        self.surface_by_key
            .iter()
            .find(|(_, id)| **id == node)
            .map(|(k, _)| k.as_str())
    }

    /// Indented native tree with types, frames and key props.
    pub fn dump(&self) -> String {
        let mut out = String::new();
        self.dump_into(&mut out, ROOT_NODE, 0);
        out
    }

    fn dump_into(&self, out: &mut String, parent: u32, depth: usize) {
        use std::fmt::Write;
        for id in self
            .native_children
            .get(&parent)
            .map(Vec::as_slice)
            .unwrap_or(&[])
        {
            let Some(n) = self.nodes.get(id) else {
                continue;
            };
            let f = n.frame.unwrap_or(Frame {
                x: 0.0,
                y: 0.0,
                width: 0.0,
                height: 0.0,
            });
            let _ = write!(
                out,
                "{:indent$}{} #{id} ({}, {}, {}x{})",
                "",
                n.native.map(|t| t.name()).unwrap_or("?"),
                f.x,
                f.y,
                f.width,
                f.height,
                indent = depth * 2
            );
            for (p, v) in &n.props {
                match (p, v) {
                    (Prop::TestId, PropValue::Str(s)) => {
                        let _ = write!(out, " test_id={s:?}");
                    }
                    (Prop::Title, PropValue::Str(s)) => {
                        let _ = write!(out, " title={s:?}");
                    }
                    (Prop::RowIndex, PropValue::I32(i)) => {
                        let _ = write!(out, " row={i}");
                    }
                    (Prop::AttributedText, PropValue::Json(j)) => {
                        let _ = write!(out, " text={:?}", props::plain_text(j));
                    }
                    (Prop::HostComponent, PropValue::Str(s)) => {
                        let _ = write!(out, " component={s:?}");
                    }
                    _ => {}
                }
            }
            out.push('\n');
            self.dump_into(out, *id, depth + 1);
        }
    }

    pub fn native_count(&self) -> usize {
        self.nodes.values().filter(|n| n.native.is_some()).count()
    }

    pub fn stats(&self) -> TreeStats {
        let natives = self.native_count();
        let flattened = self.nodes.values().filter(|n| n.native.is_none()).count();
        let c = self.counters;
        TreeStats {
            nodes: self.nodes.len(),
            natives,
            flattened,
            measure_hits: c.hits,
            measure_misses: c.misses,
            batch_calls: c.batch_calls,
            batched_items: c.batched_items,
            derived_hits: c.derived_hits,
        }
    }
}

/// See [`Tree::list_window_for`].
pub(crate) fn window_for(state: Option<&ListState>, count: u32, bottom: bool) -> (u32, u32) {
    let tail = |len: u32| (count.saturating_sub(len), count);
    let clamp = |(first, last): (u32, u32)| {
        let last = last.min(count);
        (first.min(last), last)
    };
    match state {
        None if bottom => tail(DEFAULT_WINDOW),
        None => (0, DEFAULT_WINDOW.min(count)),
        // Committed before, at the bottom, and the count changed: follow the end.
        Some(l)
            if l.count != NEVER_COMMITTED
                && bottom
                && count != l.count
                && l.window.1 >= l.count =>
        {
            let len = l.window.1 - l.window.0;
            tail(if len == 0 { DEFAULT_WINDOW } else { len })
        }
        Some(l) => clamp(l.window),
    }
}

/// The count of a list whose window the renderer set before the list was committed.
pub(crate) const NEVER_COMMITTED: u32 = u32::MAX;

impl ListState {
    pub(crate) fn new(count: u32) -> ListState {
        ListState {
            count,
            window: (0, DEFAULT_WINDOW.min(count)),
            estimated: 44.0,
            uniform: false,
            width: 0.0,
            heights: Vec::new(),
            offsets: Vec::new(),
        }
    }
}
