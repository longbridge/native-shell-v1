//! The binary mutation stream the core hands the platform renderer.
//!
//! One buffer per batch. Everything is little-endian and 4-byte aligned, so a
//! renderer reads it as a `[u32]` with no parsing beyond a switch per word:
//!
//! ```text
//! u32 magic "NSB1"   u32 protocol version   u32 instruction words   u32 string count
//! u32 f32-array count
//! [instruction words…]
//! [strings: u32 byte length, UTF-8 bytes padded to 4]…
//! [f32 arrays: u32 element count, f32…]…
//! ```
//!
//! Each instruction starts with a header word `opcode:8 | node:24`. A node id
//! that does not fit 24 bits sets [`EXTENDED_NODE`] in the opcode byte and is
//! written in full in the next word. Strings and float arrays never appear
//! inline: an instruction carries their index into the tables at the end.
//! Strings are UTF-8 bytes, never JNI modified UTF-8.
//!
//! The instruction set is closed and versioned. [`PROTOCOL_VERSION`] is part of
//! the engine ABI hash a signed bundle declares.

use std::collections::HashMap;

pub const MAGIC: u32 = u32::from_le_bytes(*b"NSB1");
pub const PROTOCOL_VERSION: u32 = 1;
pub const HEADER_WORDS: usize = 5;

/// Set in the opcode byte when the node id follows in its own word.
pub const EXTENDED_NODE: u8 = 0x80;
const NODE_MASK: u32 = 0x00FF_FFFF;

/// The surface every instance renders into. The platform creates it; the core
/// never sends `Create` for it.
pub const ROOT_NODE: u32 = 0;

macro_rules! closed_enum {
    ($(#[$m:meta])* $name:ident : $repr:ty { $($(#[$vm:meta])* $variant:ident = $value:expr,)* }) => {
        $(#[$m])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        #[repr($repr)]
        pub enum $name { $($(#[$vm])* $variant = $value,)* }

        impl $name {
            pub const ALL: &'static [$name] = &[$($name::$variant,)*];

            pub fn from_raw(raw: $repr) -> Option<Self> {
                match raw { $($value => Some($name::$variant),)* _ => None }
            }

            pub fn name(self) -> &'static str {
                match self { $($name::$variant => stringify!($variant),)* }
            }
        }
    };
}

closed_enum! {
    /// One instruction of a batch.
    Op: u8 {
        /// `seq: u32, flags: u32`. Node field is unused.
        BeginBatch = 1,
        EndBatch = 2,
        /// `type: u32`. Allocate a native view of that type for the node.
        Create = 3,
        /// `parent: u32, index: u32`. Attach the node as a child.
        Insert = 4,
        /// `parent: u32, index: u32`. Detach the node, keep it alive.
        Remove = 5,
        /// Destroy the node and every native view below it; detach first if attached.
        Delete = 6,
        /// `parent: u32, from: u32, to: u32`. Reorder within one parent.
        Move = 7,
        /// `prop: u16 | kind: u8 << 16`, then the value words for that kind.
        SetProp = 8,
        /// `x, y, width, height: f32`, relative to the native parent.
        SetFrame = 9,
        /// `bits: u32` of [`EventMask`]. Only attach listeners for these.
        SetEventMask = 10,
        /// `command: u32`, `payload: string index` (JSON, may be empty).
        Command = 11,
    }
}

/// Batch flags (`BeginBatch` second word).
pub mod batch_flags {
    /// Frames and opacity in this batch animate with the node's declared transitions.
    pub const ANIMATED: u32 = 1;
    /// First batch of a surface: apply synchronously, before first display.
    pub const INITIAL: u32 = 2;
}

closed_enum! {
    /// Native view types. Each renderer registers one implementation per type.
    ViewType: u32 {
        View = 1,
        Text = 2,
        Image = 3,
        Scroll = 4,
        TextInput = 5,
        Switch = 6,
        Checkbox = 7,
        Slider = 8,
        Progress = 9,
        List = 10,
        ListRow = 11,
        Page = 12,
        Sheet = 13,
        Dialog = 14,
        Segmented = 15,
        DatePicker = 16,
        Select = 17,
        Shape = 18,
        Spinner = 19,
        /// A native component the host app registered by name (`Prop::HostComponent`).
        HostComponent = 20,
    }
}

closed_enum! {
    /// Value encodings of `SetProp`.
    ValueKind: u8 {
        /// No value words: reset the property to its default.
        Null = 0,
        Bool = 1,
        I32 = 2,
        F32 = 3,
        /// `0xAARRGGBB`.
        Color = 4,
        /// String table index.
        Str = 5,
        /// String table index of a JSON document (rich values: text runs, shadows…).
        Json = 6,
        /// Float-array table index.
        F32Array = 7,
    }
}

closed_enum! {
    /// Properties. A renderer that receives `Null` restores that property's default:
    /// that is the reset contract that makes view pooling safe.
    Prop: u16 {
        BackgroundColor = 1,
        /// F32 array `[tl, tr, br, bl]`.
        CornerRadii = 2,
        /// F32 array `[top, right, bottom, left]`.
        BorderWidths = 3,
        BorderColor = 4,
        Opacity = 5,
        /// JSON `{color, x, y, blur, spread}`.
        Shadow = 6,
        /// JSON `{angle, stops: [[color, position]…]}`.
        Gradient = 7,
        ClipsToBounds = 8,
        Hidden = 9,
        /// JSON `{translate_x, translate_y, scale, rotate}`.
        Transform = 10,
        /// JSON `{opacity?: Anim, frame?: Anim}`, `Anim = {kind, duration, delay, easing, response, damping}`.
        Animation = 11,

        /// JSON `{runs: [{text, size, weight, color, family, italic, underline, strike, line_height, letter_spacing}], align, lines, truncate}`.
        AttributedText = 20,
        Selectable = 21,
        /// Link runs carry `url`; tapping one sends `Link` to script if it listens, else opens natively.
        LinksEnabled = 22,

        ImageSource = 30,
        /// 0 cover, 1 contain, 2 fill, 3 center.
        ContentMode = 31,
        TintColor = 32,

        Horizontal = 40,
        /// F32 array `[width, height]` of the scrollable content.
        ContentSize = 41,
        Refreshing = 42,
        ShowsIndicators = 43,
        /// Pull-to-refresh control is attached.
        RefreshEnabled = 44,

        /// JSON `{text, count}`: controlled value with the event count the script had seen.
        TextValue = 50,
        Placeholder = 51,
        Multiline = 52,
        Secure = 53,
        /// 0 default, 1 number, 2 decimal, 3 email, 4 phone, 5 url.
        KeyboardType = 54,
        MaxLength = 55,
        /// Characters accepted; empty string means any.
        AllowedChars = 56,
        Editable = 57,
        /// 0 default, 1 done, 2 go, 3 next, 4 search, 5 send.
        ReturnKey = 58,
        /// JSON text style of the field `{size, weight, color, family, align}`.
        TextStyle = 59,
        /// 0 none, 1 one-time code, 2 username, 3 password, 4 email.
        ContentType = 60,
        PlaceholderColor = 61,
        /// 0 none, 1 words, 2 sentences, 3 characters.
        AutoCapitalize = 62,
        /// F32 array `[top, right, bottom, left]`: the input's padding, applied as text insets.
        ContentInsets = 63,

        Checked = 70,
        Enabled = 71,
        /// 0 checkbox, 1 radio.
        CheckStyle = 72,

        Value = 80,
        Min = 81,
        Max = 82,
        Step = 83,
        Indeterminate = 84,

        ItemCount = 90,
        /// F32 array of every row's y offset followed by the total height.
        RowOffsets = 91,
        RowIndex = 92,
        /// F32 `prefetch` rows the renderer should request beyond the visible range.
        Prefetch = 93,
        /// I32, GPUI's `ListAlignment`: 0 top (default), 1 bottom — the list opens at its end
        /// and keeps the end in view while the user is there (rows added, viewport shrinking).
        ListAlignment = 94,

        Title = 100,
        CanGoBack = 101,
        NavBarHidden = 102,
        /// JSON `["medium", "large"]`.
        Detents = 103,
        Dismissable = 104,
        /// JSON `{title, message, actions: [{label, style}]}`.
        AlertSpec = 105,

        /// JSON `["One", "Two"]`.
        Items = 110,
        SelectedIndex = 111,
        /// ISO date `YYYY-MM-DD` or `YYYY-MM-DDTHH:MM`.
        Date = 112,
        /// 0 date, 1 time, 2 date and time.
        DateMode = 113,
        /// JSON `{title, items: [{label, value, destructive, disabled}]}`.
        Menu = 114,

        /// SVG-style path data.
        PathData = 120,
        FillColor = 121,
        StrokeColor = 122,
        StrokeWidth = 123,
        /// F32 array `[min_x, min_y, width, height]`.
        ViewBox = 124,
        /// 0 nonzero (default), 1 even-odd.
        FillRule = 125,

        A11yLabel = 130,
        /// See [`A11yRole`].
        A11yRole = 131,
        A11yHint = 132,
        A11yValue = 133,
        /// JSON `{disabled, selected, checked, expanded, busy}`.
        A11yState = 134,
        /// Stable identifier for UI tests: `accessibilityIdentifier` / view tag.
        TestId = 135,
        A11yHidden = 136,

        /// Opacity while pressed, applied natively without a round trip.
        ActiveOpacity = 140,
        ActiveBackground = 141,
        /// F32 array `[top, right, bottom, left]`.
        HitSlop = 142,
        /// The node may become first responder; carries its tab order.
        Focusable = 143,
        /// Opacity and background while a pointer is over the node (`.hover(s => …)`),
        /// applied natively like the pressed ones. Pointer platforms only.
        HoverOpacity = 144,
        HoverBackground = 145,
        /// See [`CursorStyle`]: the pointer's shape over the node. Pointer platforms only.
        Cursor = 146,
        /// Text shown after the pointer rests on the node, where the platform has
        /// tooltips (macOS, Android `View.tooltipText`, Linux); ignored elsewhere.
        Tooltip = 147,

        /// Registered name of a host component. Sent once, right after `Create`.
        HostComponent = 150,
        /// JSON object of the component's props, as the script passed them.
        HostProps = 151,
        /// On ROOT_NODE: JSON object of every resolved theme color token (`{"primary": 0xAARRGGBB, …}`),
        /// plus `"appearance"`. Sent in the first batch and whenever the resolved theme changes, so
        /// renderer-drawn chrome (control tints, toasts, sheets, pickers) follows `set_theme`.
        ThemeTokens = 152,
    }
}

closed_enum! {
    /// Accessibility roles, mapped to UIKit traits and Android class names.
    A11yRole: i32 {
        None = 0,
        Button = 1,
        Link = 2,
        Header = 3,
        Image = 4,
        Text = 5,
        TextField = 6,
        Switch = 7,
        Checkbox = 8,
        Radio = 9,
        Slider = 10,
        ProgressBar = 11,
        Tab = 12,
        TabList = 13,
        List = 14,
        ListItem = 15,
        Menu = 16,
        Alert = 17,
        Search = 18,
    }
}

closed_enum! {
    /// GPUI's `CursorStyle`, in its order.
    CursorStyle: i32 {
        Arrow = 0,
        IBeam = 1,
        Crosshair = 2,
        ClosedHand = 3,
        OpenHand = 4,
        PointingHand = 5,
        ResizeLeft = 6,
        ResizeRight = 7,
        ResizeLeftRight = 8,
        ResizeUp = 9,
        ResizeDown = 10,
        ResizeUpDown = 11,
        ResizeUpLeftDownRight = 12,
        ResizeUpRightDownLeft = 13,
        ResizeColumn = 14,
        ResizeRow = 15,
        IBeamCursorForVerticalLayout = 16,
        OperationNotAllowed = 17,
        DragLink = 18,
        DragCopy = 19,
        ContextualMenu = 20,
    }
}

closed_enum! {
    /// Commands: imperative requests that are not part of the tree.
    Command: u32 {
        /// Payload `{}`: make the node first responder.
        Focus = 1,
        Blur = 2,
        /// Payload `{x, y, animated}`.
        ScrollTo = 3,
        /// Payload `{index, position: "top"|"center"|"bottom", animated}`.
        ScrollToIndex = 4,
        /// Payload `{id, title, description, level, duration}` on the root node.
        ShowToast = 5,
        /// Payload `{url}` on the root node; only sent after the capability check.
        OpenUrl = 6,
        /// Payload `{text}`: clipboard write on the root node.
        SetClipboard = 7,
        /// Payload `{}`: the page node is pushed (animated) — renderer starts the transition.
        PresentPage = 8,
        /// Payload `{}`: the page node is being popped.
        DismissPage = 9,
        /// Payload `{haptic: "light"|"medium"|"heavy"|"success"|"warning"|"error"|"selection"}`.
        Haptic = 10,
        /// Payload `{id}` on the root node: retract that toast if still showing; `{}` retracts all.
        DismissToast = 11,
    }
}

/// Events the renderer attaches listeners for, only when the bit is set.
pub mod event_mask {
    pub const CLICK: u32 = 1 << 0;
    pub const DOUBLE_CLICK: u32 = 1 << 1;
    pub const LONG_PRESS: u32 = 1 << 2;
    pub const CHANGE: u32 = 1 << 3;
    pub const SUBMIT: u32 = 1 << 4;
    pub const FOCUS: u32 = 1 << 5;
    pub const BLUR: u32 = 1 << 6;
    pub const SCROLL: u32 = 1 << 7;
    pub const REFRESH: u32 = 1 << 8;
    pub const PAN: u32 = 1 << 9;
    pub const PINCH: u32 = 1 << 10;
    pub const LINK: u32 = 1 << 11;
    pub const KEY: u32 = 1 << 12;
    pub const DISMISS: u32 = 1 << 13;
    pub const ACTION: u32 = 1 << 14;
    pub const PRESS_IN: u32 = 1 << 15;
    pub const PRESS_OUT: u32 = 1 << 16;
    /// Host components: forward every event the component emits.
    pub const HOST_EVENT: u32 = 1 << 17;
    /// Pointer platforms: entry and exit of the pointer.
    pub const HOVER: u32 = 1 << 18;
}

closed_enum! {
    /// Events the renderer sends back, with a JSON payload.
    EventType: u32 {
        /// `{x, y, count, modifiers?: {shift, ctrl, alt, cmd}}`
        Click = 1,
        DoubleClick = 2,
        LongPress = 3,
        /// Text input: `{text, selection: [start, end], composing, count}` (offsets in UTF-16 units).
        /// Switch / checkbox: `{checked}`. Slider: `{value, final}`. Segmented / select: `{index, value}`.
        /// Date picker: `{date}`.
        Change = 4,
        Submit = 5,
        Focus = 6,
        Blur = 7,
        /// `{x, y, content_width, content_height, viewport_width, viewport_height}`
        Scroll = 8,
        Refresh = 9,
        /// `{phase: "began"|"changed"|"ended", dx, dy, vx, vy}`
        Pan = 10,
        /// `{phase, scale}`
        Pinch = 11,
        /// `{url}`
        Link = 12,
        /// `{key, key_char, modifiers: {shift, ctrl, alt, cmd}, phase: "down"|"up"}`
        Key = 13,
        /// Sheet, dialog or page dismissed by the user (swipe, back, backdrop).
        Dismiss = 14,
        /// Alert action chosen: `{index}`.
        Action = 15,
        /// List visible range: `{first, end}` (end exclusive) including prefetch.
        RangeChange = 16,
        /// List row measured natively (unused by default renderers): `{index, height}`.
        RowHeight = 17,
        /// System back pressed on the top page while `can_go_back` is false.
        BackPressed = 18,
        PressIn = 19,
        PressOut = 20,
        /// Emitted by a host component: `{name, payload}`. Script handlers subscribe by `name`.
        HostEvent = 21,
        /// On the root node: the action button of toast `{id}` was tapped (the renderer dismisses it).
        ToastAction = 22,
        /// The pointer entered (`entered: true`) or left the node: `{entered, x, y}`. Pointer platforms only.
        Hover = 23,
    }
}

impl EventType {
    /// Continuous events are coalesced to the latest per node; discrete ones are delivered in order.
    pub fn is_continuous(self) -> bool {
        matches!(self, EventType::Scroll | EventType::Pan | EventType::Pinch | EventType::RangeChange)
    }

    pub fn mask_bit(self) -> u32 {
        use event_mask::*;
        match self {
            EventType::Click => CLICK,
            EventType::DoubleClick => DOUBLE_CLICK,
            EventType::LongPress => LONG_PRESS,
            EventType::Change => CHANGE,
            EventType::Submit => SUBMIT,
            EventType::Focus => FOCUS,
            EventType::Blur => BLUR,
            EventType::Scroll => SCROLL,
            EventType::Refresh => REFRESH,
            EventType::Pan => PAN,
            EventType::Pinch => PINCH,
            EventType::Link => LINK,
            EventType::Key => KEY,
            EventType::Dismiss => DISMISS,
            EventType::Action => ACTION,
            EventType::PressIn => PRESS_IN,
            EventType::PressOut => PRESS_OUT,
            EventType::HostEvent => HOST_EVENT,
            EventType::Hover => HOVER,
            EventType::RangeChange | EventType::RowHeight | EventType::BackPressed | EventType::ToastAction => 0,
        }
    }

    /// Name used by script handlers (`on_click` → `"click"`).
    pub fn script_name(self) -> &'static str {
        match self {
            EventType::Click => "click",
            EventType::DoubleClick => "double_click",
            EventType::LongPress => "long_press",
            EventType::Change => "change",
            EventType::Submit => "submit",
            EventType::Focus => "focus",
            EventType::Blur => "blur",
            EventType::Scroll => "scroll",
            EventType::Refresh => "refresh",
            EventType::Pan => "pan",
            EventType::Pinch => "pinch",
            EventType::Link => "link",
            EventType::Key => "key",
            EventType::Dismiss => "dismiss",
            EventType::Action => "action",
            EventType::RangeChange => "range",
            EventType::RowHeight => "row_height",
            EventType::BackPressed => "back",
            EventType::PressIn => "press_in",
            EventType::PressOut => "press_out",
            EventType::HostEvent => "host_event",
            EventType::ToastAction => "toast_action",
            EventType::Hover => "hover",
        }
    }

    pub fn from_script_name(name: &str) -> Option<Self> {
        EventType::ALL.iter().copied().find(|e| e.script_name() == name)
    }
}

/// A property value as the core holds it, before table interning.
#[derive(Debug, Clone, PartialEq)]
pub enum PropValue {
    Null,
    Bool(bool),
    I32(i32),
    F32(f32),
    Color(u32),
    Str(String),
    Json(String),
    F32Array(Vec<f32>),
}

impl PropValue {
    pub fn kind(&self) -> ValueKind {
        match self {
            PropValue::Null => ValueKind::Null,
            PropValue::Bool(_) => ValueKind::Bool,
            PropValue::I32(_) => ValueKind::I32,
            PropValue::F32(_) => ValueKind::F32,
            PropValue::Color(_) => ValueKind::Color,
            PropValue::Str(_) => ValueKind::Str,
            PropValue::Json(_) => ValueKind::Json,
            PropValue::F32Array(_) => ValueKind::F32Array,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Frame {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

/// A decoded instruction. The encoder works from these too, so tests can compare batches structurally.
#[derive(Debug, Clone, PartialEq)]
pub enum Mutation {
    BeginBatch { seq: u32, flags: u32 },
    EndBatch,
    Create { node: u32, view_type: ViewType },
    Insert { node: u32, parent: u32, index: u32 },
    Remove { node: u32, parent: u32, index: u32 },
    Delete { node: u32 },
    Move { node: u32, parent: u32, from: u32, to: u32 },
    SetProp { node: u32, prop: Prop, value: PropValue },
    SetFrame { node: u32, frame: Frame },
    SetEventMask { node: u32, mask: u32 },
    Command { node: u32, command: Command, payload: String },
}

/// Builds one batch buffer.
#[derive(Default)]
pub struct Encoder {
    words: Vec<u32>,
    strings: Vec<String>,
    string_index: HashMap<String, u32>,
    arrays: Vec<Vec<f32>>,
    count: usize,
}

impl Encoder {
    pub fn new() -> Self {
        Self::default()
    }

    /// Number of instructions written so far (`BeginBatch` / `EndBatch` included).
    pub fn len(&self) -> usize {
        self.count
    }

    pub fn is_empty(&self) -> bool {
        self.count == 0
    }

    fn header(&mut self, op: Op, node: u32) {
        self.count += 1;
        if node > NODE_MASK {
            self.words.push((op as u8 | EXTENDED_NODE) as u32);
            self.words.push(node);
        } else {
            self.words.push(op as u32 | (node << 8));
        }
    }

    fn intern(&mut self, s: &str) -> u32 {
        if let Some(&i) = self.string_index.get(s) {
            return i;
        }
        let i = self.strings.len() as u32;
        self.strings.push(s.to_owned());
        self.string_index.insert(s.to_owned(), i);
        i
    }

    pub fn push(&mut self, m: &Mutation) {
        match m {
            Mutation::BeginBatch { seq, flags } => {
                self.header(Op::BeginBatch, 0);
                self.words.extend([*seq, *flags]);
            }
            Mutation::EndBatch => self.header(Op::EndBatch, 0),
            Mutation::Create { node, view_type } => {
                self.header(Op::Create, *node);
                self.words.push(*view_type as u32);
            }
            Mutation::Insert { node, parent, index } => {
                self.header(Op::Insert, *node);
                self.words.extend([*parent, *index]);
            }
            Mutation::Remove { node, parent, index } => {
                self.header(Op::Remove, *node);
                self.words.extend([*parent, *index]);
            }
            Mutation::Delete { node } => self.header(Op::Delete, *node),
            Mutation::Move { node, parent, from, to } => {
                self.header(Op::Move, *node);
                self.words.extend([*parent, *from, *to]);
            }
            Mutation::SetProp { node, prop, value } => {
                self.header(Op::SetProp, *node);
                self.words.push(*prop as u32 | ((value.kind() as u32) << 16));
                match value {
                    PropValue::Null => {}
                    PropValue::Bool(b) => self.words.push(*b as u32),
                    PropValue::I32(i) => self.words.push(*i as u32),
                    PropValue::F32(f) => self.words.push(f.to_bits()),
                    PropValue::Color(c) => self.words.push(*c),
                    PropValue::Str(s) | PropValue::Json(s) => {
                        let i = self.intern(s);
                        self.words.push(i);
                    }
                    PropValue::F32Array(a) => {
                        self.words.push(self.arrays.len() as u32);
                        self.arrays.push(a.clone());
                    }
                }
            }
            Mutation::SetFrame { node, frame } => {
                self.header(Op::SetFrame, *node);
                self.words.extend([frame.x.to_bits(), frame.y.to_bits(), frame.width.to_bits(), frame.height.to_bits()]);
            }
            Mutation::SetEventMask { node, mask } => {
                self.header(Op::SetEventMask, *node);
                self.words.push(*mask);
            }
            Mutation::Command { node, command, payload } => {
                self.header(Op::Command, *node);
                let i = self.intern(payload);
                self.words.extend([*command as u32, i]);
            }
        }
    }

    pub fn finish(self) -> Vec<u8> {
        let mut out: Vec<u32> = Vec::with_capacity(HEADER_WORDS + self.words.len() + self.strings.len() * 4);
        out.extend([MAGIC, PROTOCOL_VERSION, self.words.len() as u32, self.strings.len() as u32, self.arrays.len() as u32]);
        out.extend_from_slice(&self.words);
        for s in &self.strings {
            let bytes = s.as_bytes();
            out.push(bytes.len() as u32);
            for chunk in bytes.chunks(4) {
                let mut w = [0u8; 4];
                w[..chunk.len()].copy_from_slice(chunk);
                out.push(u32::from_le_bytes(w));
            }
        }
        for a in &self.arrays {
            out.push(a.len() as u32);
            out.extend(a.iter().map(|f| f.to_bits()));
        }
        out.iter().flat_map(|w| w.to_le_bytes()).collect()
    }
}

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum DecodeError {
    #[error("buffer is not a whole number of words")]
    Misaligned,
    #[error("bad magic or unsupported protocol version {0}")]
    Version(u32),
    #[error("truncated at word {0}")]
    Truncated(usize),
    #[error("unknown opcode {0} at word {1}")]
    Opcode(u8, usize),
    #[error("unknown {0} value {1}")]
    Unknown(&'static str, u32),
    #[error("table index {0} out of range")]
    Index(u32),
    #[error("string table entry is not UTF-8")]
    Utf8,
}

/// Reference decoder. Renderers implement the same loop natively; this one backs
/// the tests and the fuzz target, so it validates every index and never panics.
pub fn decode(bytes: &[u8]) -> Result<Vec<Mutation>, DecodeError> {
    if !bytes.len().is_multiple_of(4) {
        return Err(DecodeError::Misaligned);
    }
    let words: Vec<u32> = bytes.as_chunks::<4>().0.iter().map(|c| u32::from_le_bytes(*c)).collect();
    if words.len() < HEADER_WORDS {
        return Err(DecodeError::Truncated(words.len()));
    }
    if words[0] != MAGIC || words[1] != PROTOCOL_VERSION {
        return Err(DecodeError::Version(words[1]));
    }
    let n = words[2] as usize;
    let string_count = words[3] as usize;
    let array_count = words[4] as usize;
    let body_end = HEADER_WORDS.checked_add(n).filter(|&e| e <= words.len()).ok_or(DecodeError::Truncated(words.len()))?;

    let mut at = body_end;
    let mut strings = Vec::with_capacity(string_count.min(words.len()));
    for _ in 0..string_count {
        let len = *words.get(at).ok_or(DecodeError::Truncated(at))? as usize;
        at += 1;
        let wc = len.div_ceil(4);
        if at + wc > words.len() {
            return Err(DecodeError::Truncated(at));
        }
        let raw = &bytes[at * 4..at * 4 + len];
        strings.push(std::str::from_utf8(raw).map_err(|_| DecodeError::Utf8)?.to_owned());
        at += wc;
    }
    let mut arrays = Vec::with_capacity(array_count.min(words.len()));
    for _ in 0..array_count {
        let len = *words.get(at).ok_or(DecodeError::Truncated(at))? as usize;
        at += 1;
        if at + len > words.len() {
            return Err(DecodeError::Truncated(at));
        }
        arrays.push(words[at..at + len].iter().map(|w| f32::from_bits(*w)).collect::<Vec<_>>());
        at += len;
    }

    let mut out = Vec::new();
    let mut i = HEADER_WORDS;
    let take = |i: &mut usize, count: usize| -> Result<&[u32], DecodeError> {
        if *i + count > body_end {
            return Err(DecodeError::Truncated(*i));
        }
        let s = &words[*i..*i + count];
        *i += count;
        Ok(s)
    };
    let string = |idx: u32| strings.get(idx as usize).cloned().ok_or(DecodeError::Index(idx));
    while i < body_end {
        let at = i;
        let head = take(&mut i, 1)?[0];
        let raw_op = (head & 0xFF) as u8;
        let node = if raw_op & EXTENDED_NODE != 0 { take(&mut i, 1)?[0] } else { head >> 8 };
        let op = Op::from_raw(raw_op & !EXTENDED_NODE).ok_or(DecodeError::Opcode(raw_op, at))?;
        out.push(match op {
            Op::BeginBatch => {
                let w = take(&mut i, 2)?;
                Mutation::BeginBatch { seq: w[0], flags: w[1] }
            }
            Op::EndBatch => Mutation::EndBatch,
            Op::Create => {
                let t = take(&mut i, 1)?[0];
                Mutation::Create { node, view_type: ViewType::from_raw(t).ok_or(DecodeError::Unknown("view type", t))? }
            }
            Op::Insert => {
                let w = take(&mut i, 2)?;
                Mutation::Insert { node, parent: w[0], index: w[1] }
            }
            Op::Remove => {
                let w = take(&mut i, 2)?;
                Mutation::Remove { node, parent: w[0], index: w[1] }
            }
            Op::Delete => Mutation::Delete { node },
            Op::Move => {
                let w = take(&mut i, 3)?;
                Mutation::Move { node, parent: w[0], from: w[1], to: w[2] }
            }
            Op::SetProp => {
                let w = take(&mut i, 1)?[0];
                let prop = Prop::from_raw((w & 0xFFFF) as u16).ok_or(DecodeError::Unknown("prop", w & 0xFFFF))?;
                let kind_raw = ((w >> 16) & 0xFF) as u8;
                let kind = ValueKind::from_raw(kind_raw).ok_or(DecodeError::Unknown("value kind", kind_raw as u32))?;
                let value = match kind {
                    ValueKind::Null => PropValue::Null,
                    ValueKind::Bool => PropValue::Bool(take(&mut i, 1)?[0] != 0),
                    ValueKind::I32 => PropValue::I32(take(&mut i, 1)?[0] as i32),
                    ValueKind::F32 => PropValue::F32(f32::from_bits(take(&mut i, 1)?[0])),
                    ValueKind::Color => PropValue::Color(take(&mut i, 1)?[0]),
                    ValueKind::Str => PropValue::Str(string(take(&mut i, 1)?[0])?),
                    ValueKind::Json => PropValue::Json(string(take(&mut i, 1)?[0])?),
                    ValueKind::F32Array => {
                        let idx = take(&mut i, 1)?[0];
                        PropValue::F32Array(arrays.get(idx as usize).cloned().ok_or(DecodeError::Index(idx))?)
                    }
                };
                Mutation::SetProp { node, prop, value }
            }
            Op::SetFrame => {
                let w = take(&mut i, 4)?;
                let f = |k: usize| f32::from_bits(w[k]);
                Mutation::SetFrame { node, frame: Frame { x: f(0), y: f(1), width: f(2), height: f(3) } }
            }
            Op::SetEventMask => Mutation::SetEventMask { node, mask: take(&mut i, 1)?[0] },
            Op::Command => {
                let w = take(&mut i, 2)?;
                Mutation::Command {
                    node,
                    command: Command::from_raw(w[0]).ok_or(DecodeError::Unknown("command", w[0]))?,
                    payload: string(w[1])?,
                }
            }
        });
    }
    Ok(out)
}

pub fn encode(mutations: &[Mutation]) -> Vec<u8> {
    let mut e = Encoder::new();
    for m in mutations {
        e.push(m);
    }
    e.finish()
}

/// A stable hash of everything a renderer and a bytecode bundle must agree on.
pub fn abi_hash() -> String {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    h.update(PROTOCOL_VERSION.to_le_bytes());
    for op in Op::ALL {
        h.update([*op as u8]);
        h.update(op.name());
    }
    for t in ViewType::ALL {
        h.update((*t as u32).to_le_bytes());
        h.update(t.name());
    }
    for p in Prop::ALL {
        h.update((*p as u16).to_le_bytes());
        h.update(p.name());
    }
    for e in EventType::ALL {
        h.update((*e as u32).to_le_bytes());
        h.update(e.name());
    }
    for c in Command::ALL {
        h.update((*c as u32).to_le_bytes());
        h.update(c.name());
    }
    h.update(crate::ENGINE_VERSION);
    let d = h.finalize();
    d[..8].iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pointer_additions_follow_gpui() {
        // Hover has its own mask bit and the script name `on_hover` registers under.
        assert_eq!(EventType::Hover.mask_bit(), event_mask::HOVER);
        assert_eq!(EventType::from_script_name("hover"), Some(EventType::Hover));
        let bits: Vec<u32> = EventType::ALL.iter().map(|e| e.mask_bit()).filter(|b| *b != 0).collect();
        let unique: std::collections::HashSet<_> = bits.iter().collect();
        assert_eq!(bits.len(), unique.len(), "every mask bit belongs to one event");
        // GPUI's CursorStyle, in its declaration order.
        assert_eq!(CursorStyle::ALL.len(), 21);
        for (i, c) in CursorStyle::ALL.iter().enumerate() {
            assert_eq!(*c as i32, i as i32, "{c:?}");
        }
        assert_eq!(CursorStyle::from_raw(5), Some(CursorStyle::PointingHand));
    }

    fn sample() -> Vec<Mutation> {
        vec![
            Mutation::BeginBatch { seq: 7, flags: batch_flags::INITIAL },
            Mutation::Create { node: 1, view_type: ViewType::View },
            Mutation::SetProp { node: 1, prop: Prop::BackgroundColor, value: PropValue::Color(0xFF112233) },
            Mutation::SetProp { node: 1, prop: Prop::CornerRadii, value: PropValue::F32Array(vec![4.0, 4.0, 0.0, 0.0]) },
            Mutation::SetProp { node: 1, prop: Prop::A11yLabel, value: PropValue::Str("héllo 😀".into()) },
            Mutation::SetProp { node: 1, prop: Prop::TestId, value: PropValue::Str("héllo 😀".into()) },
            Mutation::SetProp { node: 1, prop: Prop::Opacity, value: PropValue::Null },
            Mutation::SetProp { node: 1, prop: Prop::MaxLength, value: PropValue::I32(-3) },
            Mutation::SetProp { node: 1, prop: Prop::Hidden, value: PropValue::Bool(true) },
            Mutation::SetProp { node: 1, prop: Prop::Value, value: PropValue::F32(0.5) },
            Mutation::SetProp { node: 1, prop: Prop::AttributedText, value: PropValue::Json("{\"runs\":[]}".into()) },
            Mutation::Insert { node: 1, parent: ROOT_NODE, index: 0 },
            Mutation::Create { node: 0x0100_0000, view_type: ViewType::Text },
            Mutation::Insert { node: 0x0100_0000, parent: 1, index: 0 },
            Mutation::Move { node: 2, parent: 1, from: 3, to: 0 },
            Mutation::Remove { node: 2, parent: 1, index: 0 },
            Mutation::SetFrame { node: 1, frame: Frame { x: 0.0, y: 1.5, width: 320.0, height: 44.25 } },
            Mutation::SetEventMask { node: 1, mask: event_mask::CLICK | event_mask::LONG_PRESS },
            Mutation::Command { node: 0, command: Command::ShowToast, payload: "{\"title\":\"hi\"}".into() },
            Mutation::Delete { node: 2 },
            Mutation::EndBatch,
        ]
    }

    #[test]
    fn round_trips_every_instruction() {
        let m = sample();
        let bytes = encode(&m);
        assert_eq!(bytes.len() % 4, 0);
        assert_eq!(decode(&bytes).unwrap(), m);
    }

    #[test]
    fn strings_are_interned_once() {
        let bytes = encode(&sample());
        let words: Vec<u32> = bytes.as_chunks::<4>().0.iter().map(|c| u32::from_le_bytes(*c)).collect();
        // "héllo 😀" twice, the JSON text and the toast payload: three distinct strings.
        assert_eq!(words[3], 3);
    }

    #[test]
    fn extended_node_ids_cost_one_word() {
        let small = encode(&[Mutation::Delete { node: 5 }]);
        let big = encode(&[Mutation::Delete { node: 0x0100_0000 }]);
        assert_eq!(big.len(), small.len() + 4);
    }

    #[test]
    fn hostile_buffers_are_rejected_without_panicking() {
        let good = encode(&sample());
        for cut in 0..good.len() {
            let _ = decode(&good[..cut]);
        }
        let mut bad = good.clone();
        bad[20] = 0xEE;
        assert!(decode(&bad).is_err());
        // Pseudo-random garbage with a valid header.
        let mut seed = 0x1234_5678u32;
        for _ in 0..2000 {
            let mut buf = good.clone();
            for _ in 0..4 {
                seed ^= seed << 13;
                seed ^= seed >> 17;
                seed ^= seed << 5;
                let at = HEADER_WORDS * 4 + (seed as usize % (buf.len() - HEADER_WORDS * 4));
                buf[at] = (seed >> 8) as u8;
            }
            let _ = decode(&buf);
        }
        assert_eq!(decode(&[1, 2, 3]), Err(DecodeError::Misaligned));
    }

    #[test]
    fn abi_hash_is_stable_within_a_build() {
        assert_eq!(abi_hash(), abi_hash());
        assert_eq!(abi_hash().len(), 16);
    }

    #[test]
    fn event_names_round_trip() {
        for e in EventType::ALL {
            assert_eq!(EventType::from_script_name(e.script_name()), Some(*e));
        }
    }
}
