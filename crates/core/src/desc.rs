//! The description a script's `render(cx)` produces, after the engine converts
//! it out of JS. This is the contract between the engine (which builds it) and
//! the tree (which diffs, flattens and lays it out).
//!
//! A description is immutable once built. Children of a view that did not
//! re-render are reached through [`ElementKind::Entity`] and shared by `Rc`, so
//! the tree can skip a whole clean subtree by pointer equality.

use std::collections::HashMap;
use std::rc::Rc;

use crate::protocol::{EventType, Prop, PropValue};
use crate::style::{Resolved, TextStyle};

pub type HandlerId = u32;
pub type EntityId = u32;

/// What a description node renders as.
#[derive(Debug, Clone, PartialEq)]
pub enum ElementKind {
    /// `div`, `h_flex`, `v_flex`, buttons and other pressables. Becomes a Scroll
    /// natively when its style has an `overflow_*_scroll`.
    View,
    /// A text leaf: consecutive string children of a view, or `text(…)`.
    Text(Vec<TextRun>),
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
    /// A vector path. With `stretch`, the path fills its laid-out frame: the
    /// tree scales its view box to the frame on each axis apart (renderers
    /// otherwise fit it uniformly), so a chart's line spans whatever width the
    /// layout gives it.
    Shape { stretch: bool },
    /// A virtualized list. `children` are the rendered rows, each a
    /// [`ElementKind::ListRow`] carrying its index.
    List(ListSpec),
    ListRow(u32),
    /// A native component registered by the host app, by name.
    HostComponent(String),
    /// The latest description of a nested view (`cx.new(Class, props)`).
    Entity(EntityId),
}

#[derive(Debug, Clone, PartialEq)]
pub struct TextRun {
    pub text: String,
    /// The run's own text style; inherited properties come from ancestors.
    pub style: TextStyle,
    /// Link runs (`TextView` links, `link(url, …)`).
    pub url: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ListSpec {
    /// Stable script id of the list (`list(id, …)`), the key of its window state.
    pub id: String,
    pub count: u32,
    /// Height assumed for rows that were never rendered.
    pub estimated_row_height: f32,
    /// `uniform_list`: every row has the first rendered row's height.
    pub uniform: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Desc {
    pub kind: ElementKind,
    /// Script-provided key (`.key(k)` / list row keys). Unkeyed siblings match by position and kind.
    pub key: Option<String>,
    /// `.id(name)`: target of focus requests and `scroll_to` handles. Implies a native view.
    pub ref_id: Option<String>,
    pub style: Rc<Resolved>,
    /// Explicit properties (a11y, placeholder, checked, value…), already typed.
    /// Style-derived properties (background, radii…) are not here: the tree derives them.
    pub props: Vec<(Prop, PropValue)>,
    pub handlers: Vec<(EventType, HandlerId)>,
    pub children: Vec<Desc>,
}

impl Desc {
    pub fn new(kind: ElementKind, style: Rc<Resolved>) -> Self {
        Desc { kind, key: None, ref_id: None, style, props: Vec::new(), handlers: Vec::new(), children: Vec::new() }
    }

    pub fn prop(&self, p: Prop) -> Option<&PropValue> {
        self.props.iter().find(|(k, _)| *k == p).map(|(_, v)| v)
    }

    pub fn handler(&self, e: EventType) -> Option<HandlerId> {
        self.handlers.iter().find(|(k, _)| *k == e).map(|(_, h)| *h)
    }
}

/// A root the tree lays out on its own: the main content, each pushed page,
/// each open sheet or dialog. Native renderers present them as screens/overlays.
#[derive(Debug, Clone, PartialEq)]
pub struct Surface {
    /// Stable identity across commits (`"root"`, `"page:3"`, `"sheet:1"`).
    pub key: String,
    pub kind: SurfaceKind,
    pub content: Desc,
}

#[derive(Debug, Clone, PartialEq)]
pub enum SurfaceKind {
    /// The instance's main content, sized to the environment.
    Root,
    /// A navigation page: `navigator.push(…)`. Sized to the environment.
    Page { title: String, can_go_back: bool, nav_bar_hidden: bool },
    /// A bottom sheet. Width of the environment, height from content (capped by detents natively).
    Sheet { detents: Vec<String>, dismissable: bool },
    /// A centered dialog with custom content.
    Dialog { dismissable: bool },
    /// A system alert: no content tree, only the spec.
    Alert { spec_json: String },
}

/// Everything a commit needs besides the surfaces themselves.
pub struct CommitInput<'a> {
    pub surfaces: &'a [Surface],
    /// Latest description of each live entity.
    pub entities: &'a HashMap<EntityId, Rc<Desc>>,
}
