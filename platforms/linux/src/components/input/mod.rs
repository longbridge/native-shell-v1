//! `TextInput`: a controlled native editor, `GtkText` for one line and a
//! `GtkTextView` when `Multiline`. See [`controlled`] for the value protocol.
//!
//! Properties are recorded as they arrive and applied in `batch_applied`, so
//! the editor is rebuilt at most once per batch (a `Multiline` change) and the
//! value is written after the filters and the editor kind of the same batch.

mod controlled;
#[cfg(test)]
mod tests;

use std::cell::{Cell, RefCell};
use std::rc::{Rc, Weak};

use gtk::prelude::*;
use gtk::glib;

use native_shell_core::protocol::{event_mask, Command, EventType, Prop, PropValue};

use crate::component::{Component, NodeCx, Shared};
use crate::mount::{color, json, quad};
use controlled::{char_to_utf16, ControlledText, Decision, InputFilter};

/// Default body size when a `TextStyle` has none, as on iOS.
const DEFAULT_SIZE: f32 = 16.0;

pub fn create(cx: &NodeCx) -> Box<dyn Component> {
    let input = Inner::new(cx);
    cx.view.set_content(Some(input.overlay.upcast_ref()));
    input.build(false);
    Box::new(TextInput(input))
}

pub struct TextInput(Rc<Inner>);

#[derive(Clone)]
enum Editor {
    Line(gtk::Text),
    Area { view: gtk::TextView, scroller: gtk::ScrolledWindow },
}

impl Editor {
    /// The widget that takes focus and keys.
    fn widget(&self) -> gtk::Widget {
        match self {
            Editor::Line(t) => t.clone().upcast(),
            Editor::Area { view, .. } => view.clone().upcast(),
        }
    }

    /// The widget placed in the overlay.
    fn outer(&self) -> gtk::Widget {
        match self {
            Editor::Line(t) => t.clone().upcast(),
            Editor::Area { scroller, .. } => scroller.clone().upcast(),
        }
    }

    fn text(&self) -> String {
        match self {
            Editor::Line(t) => t.text().into(),
            Editor::Area { view, .. } => {
                let b = view.buffer();
                b.text(&b.start_iter(), &b.end_iter(), false).into()
            }
        }
    }

    /// Selection (or caret) as character offsets.
    fn selection_chars(&self) -> (usize, usize) {
        let (a, b) = match self {
            Editor::Line(t) => t.selection_bounds().unwrap_or((t.position(), t.position())),
            Editor::Area { view, .. } => {
                let buf = view.buffer();
                match buf.selection_bounds() {
                    Some((s, e)) => (s.offset(), e.offset()),
                    None => {
                        let o = buf.iter_at_mark(&buf.get_insert()).offset();
                        (o, o)
                    }
                }
            }
        };
        let (a, b) = (a.max(0) as usize, b.max(0) as usize);
        (a.min(b), a.max(b))
    }

    /// Replaces the text and puts the caret `chars` characters from the start.
    fn set_text(&self, text: &str, caret: usize) {
        let caret = caret as i32;
        match self {
            Editor::Line(t) => {
                t.set_text(text);
                t.set_position(caret);
            }
            Editor::Area { view, .. } => {
                let buf = view.buffer();
                buf.set_text(text);
                buf.place_cursor(&buf.iter_at_offset(caret));
            }
        }
    }
}

/// `TextStyle` and the other properties that shape the editor.
struct Config {
    style: serde_json::Value,
    placeholder: String,
    placeholder_color: Option<u32>,
    insets: [f32; 4],
    secure: bool,
    editable: bool,
    keyboard_type: i32,
    content_type: i32,
    auto_capitalize: i32,
    return_key: i32,
    /// `A11yLabel`: the editor is what assistive technologies focus.
    label: String,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            style: serde_json::Value::Null,
            placeholder: String::new(),
            placeholder_color: None,
            insets: [0.0; 4],
            secure: false,
            editable: true,
            keyboard_type: 0,
            content_type: 0,
            auto_capitalize: 2,
            return_key: 0,
            label: String::new(),
        }
    }
}

/// A value write waiting for `batch_applied`.
enum Incoming {
    Value { text: String, count: u32 },
    Reset,
}

struct Inner {
    node: u32,
    shared: Rc<Shared>,
    weak: Weak<Inner>,
    overlay: gtk::Overlay,
    placeholder: gtk::Label,
    css: gtk::CssProvider,
    css_text: RefCell<String>,
    editor: RefCell<Option<Editor>>,
    multiline: Cell<bool>,
    wanted_multiline: Cell<bool>,
    config: RefCell<Config>,
    incoming: RefCell<Option<Incoming>>,
    mask: Cell<u32>,
    state: RefCell<ControlledText>,
    filter: RefCell<InputFilter>,
    /// Set while the renderer writes text, so the editor's signals are not reported as edits.
    writing: Cell<bool>,
    /// The IME's uncommitted text; GTK keeps it out of the buffer.
    preedit: RefCell<String>,
    was_composing: Cell<bool>,
    /// Follows the desktop text size, which scales `TextStyle` sizes.
    text_scale_watch: RefCell<Option<glib::SignalHandlerId>>,
}

impl Drop for Inner {
    fn drop(&mut self) {
        if let (Some(id), Some(settings)) = (self.text_scale_watch.take(), gtk::Settings::default()) {
            settings.disconnect(id);
        }
    }
}

impl Inner {
    fn new(cx: &NodeCx) -> Rc<Inner> {
        let overlay = gtk::Overlay::new();
        let placeholder = gtk::Label::new(None);
        placeholder.add_css_class("nsh-placeholder");
        placeholder.set_can_target(false);
        placeholder.set_visible(false);
        // The editor announces it as its placeholder instead.
        placeholder.update_state(&[gtk::accessible::State::Hidden(true)]);
        overlay.add_overlay(&placeholder);
        let css = gtk::CssProvider::new();
        add_css(&placeholder, &css);
        let inner = Rc::new_cyclic(|weak| Inner {
            node: cx.node,
            shared: cx.shared.clone(),
            weak: weak.clone(),
            overlay,
            placeholder,
            css,
            css_text: RefCell::new(String::new()),
            editor: RefCell::new(None),
            multiline: Cell::new(false),
            wanted_multiline: Cell::new(false),
            config: RefCell::new(Config::default()),
            incoming: RefCell::new(None),
            mask: Cell::new(0),
            state: RefCell::new(ControlledText::default()),
            filter: RefCell::new(InputFilter::default()),
            writing: Cell::new(false),
            preedit: RefCell::new(String::new()),
            was_composing: Cell::new(false),
            text_scale_watch: RefCell::new(None),
        });
        if let Some(settings) = gtk::Settings::default() {
            let weak = inner.weak.clone();
            let id = settings.connect_gtk_xft_dpi_notify(move |_| {
                let weak = weak.clone();
                // After the surface has recorded the new scale (`text::font_scale`).
                glib::idle_add_local_once(move || {
                    if let Some(inner) = weak.upgrade() {
                        inner.apply_config();
                    }
                });
            });
            inner.text_scale_watch.replace(Some(id));
        }
        // Taps in the padding around an inset editor focus it too.
        let tap = gtk::GestureClick::new();
        let weak = inner.weak.clone();
        tap.connect_released(move |_, _, _, _| {
            let Some(widget) = weak.upgrade().and_then(|i| i.editor()).map(|e| e.widget()) else { return };
            if widget.can_focus() && !widget.has_focus() {
                focus(&widget);
            }
        });
        inner.overlay.add_controller(tap);
        inner
    }

    fn editor(&self) -> Option<Editor> {
        self.editor.borrow().clone()
    }

    fn text(&self) -> String {
        self.editor().map(|e| e.text()).unwrap_or_default()
    }

    fn is_composing(&self) -> bool {
        !self.preedit.borrow().is_empty()
    }

    fn listens(&self, bit: u32) -> bool {
        self.mask.get() & bit != 0
    }

    fn emit(&self, event: EventType, payload: String) {
        (self.shared.events)(self.node, event, payload);
    }

    /// Creates the editor of the wanted kind, carrying the text and focus over.
    fn build(&self, multiline: bool) {
        let old = self.editor.borrow_mut().take();
        let (text, had_focus) = match &old {
            Some(e) => (e.text(), e.widget().has_focus()),
            None => (String::new(), false),
        };
        if let Some(e) = &old {
            e.outer().unparent();
        }
        self.preedit.borrow_mut().clear();
        self.was_composing.set(false);
        self.multiline.set(multiline);

        let editor = if multiline { self.make_area() } else { self.make_line() };
        let widget = editor.widget();
        widget.add_css_class("nsh-input");
        add_css(&widget, &self.css);
        if let Editor::Area { scroller, .. } = &editor {
            scroller.add_css_class("nsh-input");
            add_css(scroller, &self.css);
        }
        self.wire_common(&widget);
        self.overlay.set_child(Some(&editor.outer()));
        self.writing.set(true);
        editor.set_text(&text, text.chars().count());
        self.writing.set(false);
        self.editor.replace(Some(editor));
        self.apply_config();
        if had_focus {
            focus(&widget);
        }
    }

    fn make_line(&self) -> Editor {
        let t = gtk::Text::new();
        // The node's frame decides the width; never ask for more.
        t.set_width_chars(0);
        t.set_propagate_text_width(false);
        let weak = self.weak.clone();
        t.connect_insert_text(move |t, s, pos| {
            let Some(inner) = weak.upgrade() else { return };
            if inner.writing.get() {
                return;
            }
            let filter = inner.filter.borrow().clone();
            if !filter.is_active() {
                return;
            }
            match filter.admit(&t.text(), s) {
                Some(admitted) if admitted == s => {}
                Some(admitted) => {
                    // Insert the shortened/filtered text instead (it passes
                    // this filter again); `changed` reports it as one edit.
                    t.stop_signal_emission_by_name("insert-text");
                    t.insert_text(&admitted, pos);
                }
                None => {
                    t.stop_signal_emission_by_name("insert-text");
                    t.error_bell();
                }
            }
        });
        let weak = self.weak.clone();
        t.connect_changed(move |_| {
            if let Some(inner) = weak.upgrade() {
                inner.edited();
            }
        });
        let weak = self.weak.clone();
        t.connect_local("preedit-changed", false, move |args| {
            let preedit = args.get(1).and_then(|v| v.get::<String>().ok()).unwrap_or_default();
            if let Some(inner) = weak.upgrade() {
                inner.preedit_changed(preedit);
            }
            None
        });
        let weak = self.weak.clone();
        t.connect_activate(move |t| {
            let Some(inner) = weak.upgrade() else { return };
            if inner.listens(event_mask::SUBMIT) {
                inner.emit(EventType::Submit, serde_json::json!({ "text": inner.text() }).to_string());
            }
            if inner.config.borrow().return_key == 1 {
                unfocus(t.upcast_ref());
            }
        });
        Editor::Line(t)
    }

    fn make_area(&self) -> Editor {
        let view = gtk::TextView::new();
        view.set_wrap_mode(gtk::WrapMode::WordChar);
        let scroller = gtk::ScrolledWindow::new();
        scroller.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
        scroller.set_has_frame(false);
        scroller.set_child(Some(&view));
        let buffer = view.buffer();
        let weak = self.weak.clone();
        buffer.connect_insert_text(move |buf, iter, s| {
            let Some(inner) = weak.upgrade() else { return };
            if inner.writing.get() {
                return;
            }
            let filter = inner.filter.borrow().clone();
            if !filter.is_active() {
                return;
            }
            let current: String = buf.text(&buf.start_iter(), &buf.end_iter(), false).into();
            match filter.admit(&current, s) {
                Some(admitted) if admitted == s => {}
                Some(admitted) => {
                    // The buffer must not change inside its own insert-text:
                    // insert the admitted text right after, where the user typed.
                    buf.stop_signal_emission_by_name("insert-text");
                    let mark = buf.create_mark(None, iter, false);
                    let buf = buf.clone();
                    glib::idle_add_local_once(move || {
                        if mark.is_deleted() {
                            return;
                        }
                        let mut at = buf.iter_at_mark(&mark);
                        buf.insert(&mut at, &admitted);
                        buf.place_cursor(&at);
                        buf.delete_mark(&mark);
                    });
                }
                None => {
                    buf.stop_signal_emission_by_name("insert-text");
                    inner.overlay.error_bell();
                }
            }
        });
        let weak = self.weak.clone();
        buffer.connect_changed(move |_| {
            if let Some(inner) = weak.upgrade() {
                inner.edited();
            }
        });
        let weak = self.weak.clone();
        view.connect_preedit_changed(move |_, preedit| {
            if let Some(inner) = weak.upgrade() {
                inner.preedit_changed(preedit.to_owned());
            }
        });
        Editor::Area { view, scroller }
    }

    /// Focus and blur listeners shared by both editors. (`Key` events are the
    /// base layer's: it routes them from the window to the nearest listener.)
    fn wire_common(&self, widget: &gtk::Widget) {
        let focus = gtk::EventControllerFocus::new();
        let weak = self.weak.clone();
        focus.connect_enter(move |_| {
            if let Some(inner) = weak.upgrade().filter(|i| i.listens(event_mask::FOCUS)) {
                inner.emit(EventType::Focus, "{}".into());
            }
        });
        let weak = self.weak.clone();
        focus.connect_leave(move |_| {
            if let Some(inner) = weak.upgrade().filter(|i| i.listens(event_mask::BLUR)) {
                inner.emit(EventType::Blur, "{}".into());
            }
        });
        widget.add_controller(focus);
    }

    fn preedit_changed(&self, preedit: String) {
        if *self.preedit.borrow() == preedit {
            return;
        }
        self.preedit.replace(preedit);
        self.edited();
    }

    /// A user edit landed (typing, paste, IME preedit update or commit).
    fn edited(&self) {
        if self.writing.get() {
            return;
        }
        let Some(editor) = self.editor() else { return };
        let committed = editor.text();
        let composing = self.is_composing();
        let count = self.state.borrow_mut().did_edit();
        self.update_placeholder();
        if self.listens(event_mask::CHANGE) {
            let (start, end) = editor.selection_chars();
            // While composing, report the text as the user sees it, preedit
            // included at the caret, like UIKit's marked text.
            let (text, selection) = if composing {
                let preedit = self.preedit.borrow();
                let at = committed.char_indices().nth(start).map_or(committed.len(), |(i, _)| i);
                let text = format!("{}{}{}", &committed[..at], preedit, &committed[at..]);
                let caret = char_to_utf16(&committed, start) + controlled::utf16_len(&preedit);
                (text, [caret, caret])
            } else {
                (committed.clone(), [char_to_utf16(&committed, start), char_to_utf16(&committed, end)])
            };
            self.emit(
                EventType::Change,
                serde_json::json!({ "text": text, "selection": selection, "composing": composing, "count": count })
                    .to_string(),
            );
        }
        if self.was_composing.get() && !composing {
            let decision = self.state.borrow_mut().composition_ended(&committed);
            if let Decision::Apply(t) = decision {
                self.write(&t);
            }
        }
        self.was_composing.set(composing);
    }

    fn receive(&self, incoming: Incoming) {
        let current = self.text();
        let decision = match incoming {
            Incoming::Reset => {
                self.state.borrow_mut().reset();
                if current.is_empty() {
                    Decision::Skip
                } else {
                    Decision::Apply(String::new())
                }
            }
            Incoming::Value { text, count } => {
                let composing = self.is_composing();
                self.state.borrow_mut().incoming(&text, count, &current, composing)
            }
        };
        if let Decision::Apply(t) = decision {
            self.write(&t);
        }
    }

    /// Writes text without reporting it, keeping the caret at the same distance from the end.
    fn write(&self, text: &str) {
        let Some(editor) = self.editor() else { return };
        let old = editor.text();
        let from_end = old.chars().count().saturating_sub(editor.selection_chars().1);
        self.writing.set(true);
        editor.set_text(text, text.chars().count().saturating_sub(from_end));
        self.writing.set(false);
        self.update_placeholder();
    }

    fn update_placeholder(&self) {
        let empty = self.text().is_empty() && !self.is_composing();
        let has = !self.config.borrow().placeholder.is_empty();
        self.placeholder.set_visible(empty && has);
    }

    /// Applies every recorded property to the current editor (idempotent).
    fn apply_config(&self) {
        let Some(editor) = self.editor() else { return };
        let c = self.config.borrow();
        let align = c.style.get("align").and_then(|a| a.as_str()).unwrap_or("left");
        let xalign = match align {
            "center" => 0.5,
            "right" => 1.0,
            _ => 0.0,
        };
        let [top, right, bottom, left] = c.insets.map(|v| v.round().max(0.0) as i32);
        let purpose = input_purpose(c.secure, c.keyboard_type, c.content_type);
        let hints = input_hints(c.auto_capitalize, purpose);
        match &editor {
            Editor::Line(t) => {
                t.set_margin_top(top);
                t.set_margin_end(right);
                t.set_margin_bottom(bottom);
                t.set_margin_start(left);
                t.set_alignment(xalign);
                t.set_visibility(!c.secure);
                t.set_input_purpose(purpose);
                t.set_input_hints(hints);
                t.set_editable(c.editable);
                t.set_can_focus(c.editable);
            }
            Editor::Area { view, .. } => {
                view.set_top_margin(top);
                view.set_right_margin(right);
                view.set_bottom_margin(bottom);
                view.set_left_margin(left);
                view.set_justification(match align {
                    "center" => gtk::Justification::Center,
                    "right" => gtk::Justification::Right,
                    _ => gtk::Justification::Left,
                });
                view.set_input_purpose(purpose);
                view.set_input_hints(hints);
                view.set_editable(c.editable);
                view.set_cursor_visible(c.editable);
                view.set_can_focus(c.editable);
            }
        }

        editor.widget().update_property(&[
            gtk::accessible::Property::Label(&c.label),
            gtk::accessible::Property::Placeholder(&c.placeholder),
            gtk::accessible::Property::MultiLine(self.multiline.get()),
        ]);

        let p = &self.placeholder;
        p.set_label(&c.placeholder);
        p.set_xalign(xalign);
        p.set_halign(gtk::Align::Fill);
        p.set_margin_top(top);
        p.set_margin_end(right);
        p.set_margin_bottom(bottom);
        p.set_margin_start(left);
        if self.multiline.get() {
            p.set_valign(gtk::Align::Start);
            p.set_wrap(true);
            p.set_wrap_mode(gtk::pango::WrapMode::WordChar);
            p.set_ellipsize(gtk::pango::EllipsizeMode::None);
        } else {
            p.set_valign(gtk::Align::Center);
            p.set_wrap(false);
            p.set_ellipsize(gtk::pango::EllipsizeMode::End);
        }

        let theme = self.shared.theme.borrow();
        let css = stylesheet(&c, crate::text::font_scale(), &Palette::from_theme(&theme));
        drop(theme);
        drop(c);
        if *self.css_text.borrow() != css {
            self.css.load_from_string(&css);
            self.css_text.replace(css);
        }
        self.update_placeholder();
    }
}

impl Component for TextInput {
    fn set_prop(&mut self, _cx: &NodeCx, prop: Prop, value: &PropValue) -> bool {
        let inner = &self.0;
        let mut c = inner.config.borrow_mut();
        let int = |default: i32| match value {
            PropValue::I32(i) => *i,
            _ => default,
        };
        let flag = |default: bool| match value {
            PropValue::Bool(b) => *b,
            _ => default,
        };
        match prop {
            Prop::TextValue => {
                let incoming = match json(value) {
                    Some(v) if !matches!(value, PropValue::Null) => Incoming::Value {
                        text: v.get("text").and_then(|t| t.as_str()).unwrap_or_default().to_owned(),
                        count: v.get("count").and_then(|n| n.as_u64()).unwrap_or(0).min(u32::MAX as u64) as u32,
                    },
                    _ => Incoming::Reset,
                };
                inner.incoming.replace(Some(incoming));
            }
            Prop::Multiline => inner.wanted_multiline.set(flag(false)),
            Prop::Placeholder => {
                c.placeholder = match value {
                    PropValue::Str(s) => s.clone(),
                    _ => String::new(),
                }
            }
            Prop::PlaceholderColor => c.placeholder_color = color(value),
            Prop::Secure => c.secure = flag(false),
            Prop::KeyboardType => c.keyboard_type = int(0),
            Prop::ContentType => c.content_type = int(0),
            Prop::AutoCapitalize => c.auto_capitalize = int(2),
            Prop::ReturnKey => c.return_key = int(0),
            Prop::Editable => c.editable = flag(true),
            Prop::TextStyle => c.style = json(value).unwrap_or(serde_json::Value::Null),
            Prop::ContentInsets => c.insets = quad(value),
            Prop::MaxLength => inner.filter.borrow_mut().max_length = int(0).max(0) as usize,
            Prop::AllowedChars => {
                inner.filter.borrow_mut().allowed = match value {
                    PropValue::Str(s) if !s.is_empty() => Some(s.chars().collect()),
                    _ => None,
                }
            }
            Prop::A11yLabel => {
                c.label = match value {
                    PropValue::Str(s) => s.clone(),
                    _ => String::new(),
                };
                // The base view keeps it too.
                return false;
            }
            _ => return false,
        }
        true
    }

    fn set_event_mask(&mut self, _cx: &NodeCx, mask: u32) {
        self.0.mask.set(mask);
    }

    fn batch_applied(&mut self, _cx: &NodeCx) {
        let inner = &self.0;
        if inner.wanted_multiline.get() != inner.multiline.get() {
            inner.build(inner.wanted_multiline.get());
        } else {
            inner.apply_config();
        }
        let incoming = inner.incoming.borrow_mut().take();
        if let Some(incoming) = incoming {
            inner.receive(incoming);
        }
    }

    fn command(&mut self, _cx: &NodeCx, command: Command, _payload: &str) {
        let Some(editor) = self.0.editor() else { return };
        match command {
            Command::Focus => focus(&editor.widget()),
            Command::Blur => unfocus(&editor.widget()),
            _ => {}
        }
    }

    fn unmount(&mut self, _cx: &NodeCx) {
        let inner = &self.0;
        if let Some(e) = inner.editor() {
            unfocus(&e.widget());
        }
        inner.state.borrow_mut().reset();
    }
}

fn focus(widget: &gtk::Widget) {
    // `grab_focus_without_selecting` would bypass `can-focus` (a disabled field).
    if !widget.can_focus() {
        return;
    }
    match widget.downcast_ref::<gtk::Text>() {
        // Plain `grab_focus` would select all the text, unlike a tap on iOS.
        Some(t) => {
            t.grab_focus_without_selecting();
        }
        None => {
            widget.grab_focus();
        }
    }
}

fn unfocus(widget: &gtk::Widget) {
    if widget.has_focus() {
        if let Some(root) = widget.root() {
            root.set_focus(None::<&gtk::Widget>);
        }
    }
}

/// The provider styles one input's widgets only; adding it per widget keeps a
/// style change from restyling the whole window, as a display-wide one would.
#[allow(deprecated)]
fn add_css(widget: &impl IsA<gtk::Widget>, css: &gtk::CssProvider) {
    widget.style_context().add_provider(css, gtk::STYLE_PROVIDER_PRIORITY_APPLICATION);
}

fn input_purpose(secure: bool, keyboard: i32, content: i32) -> gtk::InputPurpose {
    use gtk::InputPurpose as P;
    if secure || content == 3 {
        return P::Password;
    }
    match (content, keyboard) {
        (1, _) => P::Pin,
        (4, _) | (_, 3) => P::Email,
        (_, 1) => P::Digits,
        (_, 2) => P::Number,
        (_, 4) => P::Phone,
        (_, 5) => P::Url,
        _ => P::FreeForm,
    }
}

fn input_hints(auto_capitalize: i32, purpose: gtk::InputPurpose) -> gtk::InputHints {
    use gtk::InputHints as H;
    if !matches!(purpose, gtk::InputPurpose::FreeForm) {
        return H::NO_SPELLCHECK;
    }
    match auto_capitalize {
        0 => H::NONE,
        1 => H::UPPERCASE_WORDS,
        3 => H::UPPERCASE_CHARS,
        _ => H::UPPERCASE_SENTENCES,
    }
}

fn css_color(c: u32) -> String {
    let a = ((c >> 24) & 0xff) as f32 / 255.0;
    format!("rgba({},{},{},{:.3})", (c >> 16) & 0xff, (c >> 8) & 0xff, c & 0xff, a)
}

/// The theme tokens the editor's own chrome uses.
#[derive(Default)]
struct Palette {
    foreground: Option<u32>,
    muted: Option<u32>,
    /// The caret, as UIKit's tint and AppKit's insertion point.
    tint: Option<u32>,
    /// The selection highlight (`selection`, translucent).
    selection: Option<u32>,
}

impl Palette {
    fn from_theme(theme: &crate::theme::ThemeTokens) -> Palette {
        Palette {
            foreground: theme.color("foreground"),
            muted: theme.color("muted_foreground"),
            tint: theme.color("primary"),
            selection: theme.color("selection"),
        }
    }
}

/// The editor's CSS: no GTK entry chrome (the node paints the box), the
/// `TextStyle` font (scaled by the user's text size) and colors, the tint on
/// the caret and the theme's selection color behind selected text.
fn stylesheet(c: &Config, scale: f32, palette: &Palette) -> String {
    let s = &c.style;
    let num = |k: &str| s.get(k).and_then(|v| v.as_f64()).map(|v| v as f32);
    let size = num("size").unwrap_or(DEFAULT_SIZE) * scale;
    let weight = num("weight").unwrap_or(400.0).clamp(100.0, 900.0).round();
    let color = s.get("color").and_then(|v| v.as_u64()).map(|v| v as u32).or(palette.foreground).unwrap_or(0xff00_0000);
    let placeholder = c.placeholder_color.or(palette.muted).unwrap_or(0xff73_7373);
    let tint = palette.tint.unwrap_or(color);
    // Without the token (an older core), a light wash of the tint.
    let selection = palette.selection.unwrap_or((tint & 0x00ff_ffff) | 0x4000_0000);
    let mut font = format!("font-size: {size:.2}px; font-weight: {weight};");
    // The same face list `Text` draws with (the system UI face by default).
    let families = crate::text::families(s.get("family").and_then(|v| v.as_str()));
    let families: Vec<String> =
        families.split(',').map(|f| format!("\"{}\"", f.trim().replace(['"', '\\'], ""))).collect();
    font.push_str(&format!(" font-family: {};", families.join(", ")));
    if s.get("italic").and_then(|v| v.as_bool()) == Some(true) {
        font.push_str(" font-style: italic;");
    }
    if let Some(lh) = num("line_height") {
        font.push_str(&format!(" line-height: {:.2}px;", lh * scale));
    }
    format!(
        ".nsh-input, .nsh-input > text, .nsh-input > undershoot {{ background: none; border: none; box-shadow: none; \
         outline: none; padding: 0; margin: 0; min-height: 0; min-width: 0; }}\n\
         .nsh-input, .nsh-input > text {{ color: {color}; caret-color: {tint}; {font} }}\n\
         .nsh-input selection, .nsh-input > text > selection {{ background-color: {selection}; color: {color}; }}\n\
         label.nsh-placeholder {{ color: {placeholder}; {font} }}\n",
        color = css_color(color),
        tint = css_color(tint),
        selection = css_color(selection),
        placeholder = css_color(placeholder),
    )
}


