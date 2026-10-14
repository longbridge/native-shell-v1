//! `Text`: draws the attributed text with the Pango configuration the
//! measurer used, laid out at exactly the frame width the core settled on, so
//! the lines on screen are the lines that were measured. Selectable text
//! switches to a selectable `GtkLabel` given the same text and attributes.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::Arc;

use gtk::prelude::*;
use gtk::{graphene, pango};

use native_shell_core::protocol::{event_mask, EventType, Prop, PropValue};

use crate::component::{Component, EventSink, NodeCx};
use crate::node_view::{rgba, NodeView};
use crate::text::{self, Align, ParsedText, TextLayout};

/// The parsed text and its layout at the last drawn width.
#[derive(Default)]
struct Content {
    parsed: Option<Arc<ParsedText>>,
    /// Width and font scale (as bits) the layout was built for.
    laid: Option<((u32, u32), TextLayout)>,
}

impl Content {
    fn layout(&mut self, width: f32) -> Option<&TextLayout> {
        let parsed = self.parsed.as_ref()?;
        let font_scale = text::font_scale();
        let key = (width.to_bits(), font_scale.to_bits());
        if self.laid.as_ref().is_none_or(|(k, _)| *k != key) {
            self.laid = Some((key, text::build(parsed, Some(width), font_scale)));
        }
        self.laid.as_ref().map(|(_, laid)| laid)
    }

    /// The link under a point in the node's coordinates.
    fn url_at(&mut self, width: f32, x: f64, y: f64) -> Option<String> {
        let parsed = self.parsed.clone()?;
        if !parsed.has_links() {
            return None;
        }
        let laid = self.layout(width)?;
        let scale = f64::from(pango::SCALE);
        let (inside, index, _) = laid.layout.xy_to_index((x * scale) as i32, (y * scale) as i32);
        inside.then(|| parsed.url_at(index.max(0) as usize).map(str::to_owned)).flatten()
    }
}

pub struct Text {
    content: Rc<RefCell<Content>>,
    label: Option<gtk::Label>,
    /// The core turns links on for text with link runs.
    links_enabled: Rc<Cell<bool>>,
    mask: Rc<Cell<u32>>,
    /// The script set its own accessibility label.
    explicit_a11y_label: bool,
    /// The selectable label's selection color rule, and the CSS it holds.
    selection_css: gtk::CssProvider,
    selection_rule: String,
}

impl Text {
    pub fn new(cx: &NodeCx) -> Text {
        let text = Text {
            content: Rc::default(),
            label: None,
            links_enabled: Rc::new(Cell::new(false)),
            mask: Rc::default(),
            explicit_a11y_label: false,
            selection_css: gtk::CssProvider::new(),
            selection_rule: String::new(),
        };
        cx.view.set_accessible_role(gtk::AccessibleRole::Label);
        text.install_draw(cx.view);
        text.install_links(cx);
        text
    }

    fn install_draw(&self, view: &NodeView) {
        let content = self.content.clone();
        view.set_draw(Some(Box::new(move |view, snapshot| {
            let Some(width) = view.frame().map(|f| f.width) else { return };
            let mut content = content.borrow_mut();
            let Some(laid) = content.layout(width) else { return };
            if laid.layout.text().is_empty() {
                return;
            }
            if let Some(h) = laid.clip {
                snapshot.push_clip(&graphene::Rect::new(0.0, 0.0, width, h));
            }
            // Run colors come from the attributes; this is only the fallback.
            snapshot.append_layout(&laid.layout, &rgba(0xff00_0000));
            if laid.clip.is_some() {
                snapshot.pop();
            }
        })));
    }

    /// Link runs: a tap sends `Link` when the script listens, otherwise the
    /// URL opens in the default handler. The pointer turns into a hand over a
    /// link, the desktop counterpart of a link's pressed state.
    fn install_links(&self, cx: &NodeCx) {
        let view = cx.view.clone();
        let pressed: Rc<RefCell<Option<String>>> = Rc::default();
        let click = gtk::GestureClick::new();
        // Capture phase so a selectable label underneath still gets the
        // press for selection; the gesture never claims it.
        click.set_propagation_phase(gtk::PropagationPhase::Capture);
        let (content, enabled, mask, weak) = (self.content.clone(), self.links_enabled.clone(), self.mask.clone(), view.downgrade());
        let link_at = move |x: f64, y: f64| -> Option<String> {
            let view = weak.upgrade()?;
            let active = enabled.get() || mask.get() & event_mask::LINK != 0;
            let width = view.frame()?.width;
            active.then(|| content.borrow_mut().url_at(width, x, y)).flatten()
        };
        let link_at = Rc::new(link_at);
        {
            let (link_at, pressed) = (link_at.clone(), pressed.clone());
            click.connect_pressed(move |_, _, x, y| {
                pressed.replace(link_at(x, y));
            });
        }
        {
            let (link_at, mask, events, node, label_source) = (link_at.clone(), self.mask.clone(), cx.shared.events.clone(), cx.node, view.downgrade());
            click.connect_released(move |_, _, x, y| {
                let Some(url) = pressed.take() else { return };
                if link_at(x, y).as_deref() != Some(url.as_str()) || has_selection(label_source.upgrade().as_ref()) {
                    return;
                }
                activate_link(&events, node, mask.get(), &url, label_source.upgrade().as_ref());
            });
        }
        view.add_controller(click);

        let motion = gtk::EventControllerMotion::new();
        let weak = view.downgrade();
        let hover = move |x: f64, y: f64| {
            let Some(view) = weak.upgrade() else { return };
            let cursor = link_at(x, y).map(|_| "pointer");
            view.set_cursor_from_name(cursor);
        };
        let hover = Rc::new(hover);
        let enter = hover.clone();
        motion.connect_enter(move |_, x, y| enter(x, y));
        motion.connect_motion(move |_, x, y| hover(x, y));
        view.add_controller(motion);
    }

    fn set_text(&mut self, cx: &NodeCx, json: Option<&str>) {
        {
            let mut content = self.content.borrow_mut();
            content.parsed = json.map(text::parse);
            content.laid = None;
        }
        self.sync_label(cx.view);
        cx.view.queue_draw();
    }

    fn set_selectable(&mut self, cx: &NodeCx, on: bool) {
        let view = cx.view;
        match (on, self.label.is_some()) {
            (true, false) => {
                let label = gtk::Label::new(None);
                label.add_css_class("nsh-text");
                add_css(&label, &self.selection_css);
                label.set_selectable(true);
                label.set_wrap(true);
                label.set_wrap_mode(pango::WrapMode::WordChar);
                label.set_xalign(0.0);
                label.set_yalign(0.0);
                label.set_valign(gtk::Align::Start);
                view.set_content(Some(label.upcast_ref()));
                self.label = Some(label);
                self.sync_label(view);
                self.sync_selection_color(cx);
            }
            (false, true) => {
                view.set_content(None);
                view.set_overflow(gtk::Overflow::Visible);
                self.label = None;
            }
            _ => {}
        }
        // The label draws the text itself.
        view.queue_draw();
        self.content.borrow_mut().laid = None;
        if on {
            view.set_draw(None);
        } else {
            self.install_draw(view);
        }
    }

    /// Gives the selectable label the text and attributes of the measured
    /// layout (fallback-run centering included), so it wraps the same way.
    fn sync_label(&self, view: &NodeView) {
        let Some(label) = &self.label else { return };
        let width = view.frame().map_or(0.0, |f| f.width);
        let mut content = self.content.borrow_mut();
        let parsed = content.parsed.clone();
        let Some((parsed, laid)) = parsed.zip(content.layout(width)) else {
            label.set_text("");
            return;
        };
        label.set_text(&laid.layout.text());
        label.set_attributes(laid.layout.attributes().as_ref());
        label.set_justify(match parsed.align {
            Align::Left => gtk::Justification::Left,
            Align::Center => gtk::Justification::Center,
            Align::Right => gtk::Justification::Right,
            Align::Justify => gtk::Justification::Fill,
        });
        label.set_xalign(match parsed.align {
            Align::Center => 0.5,
            Align::Right => 1.0,
            Align::Left | Align::Justify => 0.0,
        });
        let lines = parsed.max_lines.min(i32::MAX as usize) as i32;
        label.set_lines(if parsed.max_lines > 0 { lines } else { -1 });
        label.set_ellipsize(if parsed.truncate && parsed.max_lines > 0 {
            pango::EllipsizeMode::End
        } else {
            pango::EllipsizeMode::None
        });
        // Lines past a limit without an ellipsis are clipped, as when drawn.
        view.set_overflow(if laid.clip.is_some() { gtk::Overflow::Hidden } else { gtk::Overflow::Visible });
    }

    /// Selected text keeps its run colors over the theme's `selection` wash,
    /// as on the other renderers.
    fn sync_selection_color(&mut self, cx: &NodeCx) {
        if self.label.is_none() {
            return;
        }
        let theme = cx.shared.theme.borrow();
        // Without the token (an older core), a light wash of the tint.
        let color = theme.color("selection").or_else(|| theme.color("primary").map(|c| (c & 0x00ff_ffff) | 0x4000_0000));
        let color = color.unwrap_or(0x400a_84ff);
        let a = f32::from((color >> 24) as u8) / 255.0;
        let rule = format!(
            "label.nsh-text > selection {{ background-color: rgba({},{},{},{a:.3}); }}",
            (color >> 16) & 0xff,
            (color >> 8) & 0xff,
            color & 0xff
        );
        if rule != self.selection_rule {
            self.selection_css.load_from_string(&rule);
            self.selection_rule = rule;
        }
    }

    fn sync_a11y_label(&self, view: &NodeView) {
        if self.explicit_a11y_label {
            return;
        }
        let content = self.content.borrow();
        let text = content.parsed.as_ref().map_or("", |p| p.text.as_str());
        view.update_property(&[gtk::accessible::Property::Label(text)]);
    }
}

impl Component for Text {
    fn set_prop(&mut self, cx: &NodeCx, prop: Prop, value: &PropValue) -> bool {
        match prop {
            Prop::AttributedText => match value {
                PropValue::Json(json) | PropValue::Str(json) => self.set_text(cx, Some(json)),
                _ => self.set_text(cx, None),
            },
            Prop::Selectable => self.set_selectable(cx, matches!(value, PropValue::Bool(true))),
            Prop::LinksEnabled => self.links_enabled.set(matches!(value, PropValue::Bool(true))),
            Prop::A11yLabel => {
                self.explicit_a11y_label = !matches!(value, PropValue::Null);
                return false;
            }
            _ => return false,
        }
        true
    }

    fn set_frame(&mut self, cx: &NodeCx, _frame: native_shell_core::protocol::Frame) {
        // The label lays out at its allocation; only its line clip depends on the width.
        self.sync_label(cx.view);
        cx.view.queue_draw();
    }

    fn set_event_mask(&mut self, _cx: &NodeCx, mask: u32) {
        self.mask.set(mask);
    }

    fn batch_applied(&mut self, cx: &NodeCx) {
        self.sync_a11y_label(cx.view);
        self.sync_selection_color(cx);
    }
}

// A per-widget provider: the rule depends on the surface's theme, and a
// display-wide one would leak it into other surfaces.
#[allow(deprecated)]
fn add_css(widget: &impl IsA<gtk::Widget>, css: &gtk::CssProvider) {
    widget.style_context().add_provider(css, gtk::STYLE_PROVIDER_PRIORITY_APPLICATION);
}

fn has_selection(view: Option<&NodeView>) -> bool {
    view.and_then(|v| v.content()).and_downcast::<gtk::Label>().is_some_and(|l| l.selection_bounds().is_some())
}

fn activate_link(events: &EventSink, node: u32, mask: u32, url: &str, view: Option<&NodeView>) {
    if mask & event_mask::LINK != 0 {
        events(node, EventType::Link, serde_json::json!({ "url": url }).to_string());
        return;
    }
    let window = view.and_then(|v| v.root()).and_downcast::<gtk::Window>();
    gtk::UriLauncher::new(url).launch(window.as_ref(), gtk::gio::Cancellable::NONE, |result| {
        if let Err(e) = result {
            eprintln!("[native-shell] cannot open link: {e}");
        }
    });
}
