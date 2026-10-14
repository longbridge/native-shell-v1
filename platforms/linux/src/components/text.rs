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
}

impl Text {
    pub fn new(cx: &NodeCx) -> Text {
        let text = Text {
            content: Rc::default(),
            label: None,
            links_enabled: Rc::new(Cell::new(false)),
            mask: Rc::default(),
            explicit_a11y_label: false,
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

    fn set_selectable(&mut self, view: &NodeView, on: bool) {
        match (on, self.label.is_some()) {
            (true, false) => {
                let label = gtk::Label::new(None);
                label.set_selectable(true);
                label.set_wrap(true);
                label.set_wrap_mode(pango::WrapMode::WordChar);
                label.set_xalign(0.0);
                label.set_yalign(0.0);
                label.set_valign(gtk::Align::Start);
                view.set_content(Some(label.upcast_ref()));
                self.label = Some(label);
                self.sync_label(view);
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

    fn sync_a11y_label(&self, view: &NodeView) {
        if self.explicit_a11y_label {
            return;
        }
        let content = self.content.borrow();
        let text = content.parsed.as_ref().map_or("", |p| p.text.as_str());
        view.update_property(&[gtk::accessible::Property::Label(text)]);
    }

    /// The layout drawn at the current frame width (tests).
    #[cfg(test)]
    pub fn drawn(&self, width: f32) -> Option<(f32, f32, i32)> {
        let mut content = self.content.borrow_mut();
        content.layout(width).map(|l| (l.width, l.height, l.layout.line_count()))
    }

    #[cfg(test)]
    pub fn label(&self) -> Option<&gtk::Label> {
        self.label.as_ref()
    }
}

impl Component for Text {
    fn set_prop(&mut self, cx: &NodeCx, prop: Prop, value: &PropValue) -> bool {
        match prop {
            Prop::AttributedText => match value {
                PropValue::Json(json) | PropValue::Str(json) => self.set_text(cx, Some(json)),
                _ => self.set_text(cx, None),
            },
            Prop::Selectable => self.set_selectable(cx.view, matches!(value, PropValue::Bool(true))),
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
    }
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

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;

    use gtk::prelude::*;
    use native_shell_core::host::WidthMode;
    use native_shell_core::protocol::{event_mask, EventType, Frame, Prop, PropValue};

    use super::Text;
    use crate::component::{Component, NodeCx, Shared};
    use crate::node_view::NodeView;
    use crate::theme::ThemeTokens;

    const PARAGRAPH: &str = r#"{"runs":[{"text":"Your order shipped and arrives Tuesday. 包裹已发货 🚚 Track it at ","size":17,"weight":400,"color":4278190080,"line_height":22},{"text":"example.org","size":17,"weight":600,"color":4278218495,"underline":true,"url":"https://example.org/track","line_height":22}]}"#;

    /// GTK runs on one thread per process, so every GTK case lives in this test.
    /// Needs a display (CI and agents use Broadway); skipped without one.
    #[test]
    fn text_component_draws_what_was_measured() {
        if gtk::init().is_err() {
            eprintln!("skipped: no display for GTK");
            return;
        }
        let sent: Rc<RefCell<Vec<(u32, EventType, String)>>> = Rc::default();
        let events = {
            let sent = sent.clone();
            Rc::new(move |node, event, payload| sent.borrow_mut().push((node, event, payload)))
        };
        let shared = Rc::new(Shared { events, theme: RefCell::new(ThemeTokens::default()), root: NodeView::new() });
        let view = NodeView::new();
        let cx = NodeCx { node: 7, view: &view, shared: &shared };
        let mut text = Text::new(&cx);
        assert!(text.set_prop(&cx, Prop::AttributedText, &PropValue::Json(PARAGRAPH.into())));

        // Laid out at the frame width the core settles on, the drawn layout has
        // the measured size and lines.
        for width in [140.0f32, 220.5, 361.0] {
            let measured = crate::text::measure_at(PARAGRAPH, WidthMode::Exactly(width), 1.0, 1.0);
            view.set_frame(Frame { x: 0.0, y: 0.0, width, height: measured.height });
            text.set_frame(&cx, Frame { x: 0.0, y: 0.0, width, height: measured.height });
            let (w, h, lines) = text.drawn(width).unwrap_or_default();
            assert!(w <= width && (h - measured.height).abs() <= 1.0, "{width}: drawn {w}×{h} vs {measured:?}");
            assert_eq!(h, lines as f32 * 22.0, "{width}");
        }

        // Rendered for real, the ink stays inside the measured box.
        let width = 220.5;
        let measured = crate::text::measure_at(PARAGRAPH, WidthMode::Exactly(width), 1.0, 1.0);
        view.set_frame(Frame { x: 0.0, y: 0.0, width, height: measured.height });
        let window = gtk::Window::new();
        let fixed = gtk::Fixed::new();
        fixed.put(&view, 0.0, 0.0);
        view.set_size_request(width.ceil() as i32, measured.height.ceil() as i32);
        window.set_child(Some(&fixed));
        window.set_default_size(400, 300);
        window.present();
        settle(|| view.width() > 0);
        let (ink_rows, ink_top, ink_bottom) = ink(&view);
        assert!(ink_rows > 10, "text was drawn");
        assert!(ink_top >= 0 && ink_bottom as f32 <= measured.height + 1.0, "ink {ink_top}..{ink_bottom} in {measured:?}");

        // Link taps: `Link` to the script when it listens.
        text.set_prop(&cx, Prop::LinksEnabled, &PropValue::Bool(true));
        text.set_event_mask(&cx, event_mask::LINK);
        let url = text.content.borrow_mut().url_at(width, 0.0, 0.0);
        assert_eq!(url, None, "plain run");
        let link = link_point(&mut text, width).expect("a link run");
        let found = text.content.borrow_mut().url_at(width, link.0, link.1);
        assert_eq!(found.as_deref(), Some("https://example.org/track"));
        super::activate_link(&shared.events, 7, event_mask::LINK, "https://example.org/track", Some(&view));
        assert_eq!(sent.borrow().last().map(|(n, e, p)| (*n, *e, p.as_str())), Some((7, EventType::Link, r#"{"url":"https://example.org/track"}"#)));

        // Selectable text is a GtkLabel with the same text, attributes and lines.
        text.set_prop(&cx, Prop::Selectable, &PropValue::Bool(true));
        let label = text.label().cloned().expect("selectable label");
        settle(|| label.width() >= width as i32);
        assert!(label.is_selectable());
        let layout = label.layout();
        let (_, logical) = layout.pixel_extents();
        let (_, _, lines) = text.drawn(width).unwrap_or_default();
        assert_eq!(layout.line_count(), lines, "label wraps like the measurement");
        assert!((logical.height() as f32 - measured.height).abs() <= 1.0, "label {logical:?} vs {measured:?}");

        text.set_prop(&cx, Prop::Selectable, &PropValue::Bool(false));
        assert!(text.label().is_none() && view.content().is_none());
        window.destroy();
    }

    /// Runs the main loop until `done` (a frame has been laid out), at most 2 s.
    fn settle(done: impl Fn() -> bool) {
        let context = gtk::glib::MainContext::default();
        let start = std::time::Instant::now();
        while !done() && start.elapsed() < std::time::Duration::from_secs(2) {
            context.iteration(false);
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        for _ in 0..20 {
            context.iteration(false);
        }
    }

    /// A point on the first link run's glyphs.
    fn link_point(text: &mut Text, width: f32) -> Option<(f64, f64)> {
        let mut content = text.content.borrow_mut();
        let parsed = content.parsed.clone()?;
        let start = parsed.runs.iter().find(|r| r.url.is_some())?.start;
        let laid = content.layout(width)?;
        let pos = laid.layout.index_to_pos(start as i32 + 2);
        let s = f64::from(gtk::pango::SCALE);
        Some((f64::from(pos.x()) / s + 1.0, f64::from(pos.y() + pos.height() / 2) / s))
    }

    /// Rows with ink and the first/last of them, from the rendered view.
    fn ink(view: &NodeView) -> (usize, i32, i32) {
        let paintable = gtk::WidgetPaintable::new(Some(view));
        let (w, h) = (view.width().max(1), view.height().max(1));
        let snapshot = gtk::Snapshot::new();
        paintable.snapshot(&snapshot, f64::from(w), f64::from(h));
        let Some(node) = snapshot.to_node() else { return (0, -1, -1) };
        let Some(renderer) = view.native().and_then(|n| n.renderer()) else { return (0, -1, -1) };
        let texture = renderer.render_texture(&node, Some(&gtk::graphene::Rect::new(0.0, 0.0, w as f32, h as f32)));
        let mut downloader = gtk::gdk::TextureDownloader::new(&texture);
        downloader.set_format(gtk::gdk::MemoryFormat::R8g8b8a8);
        let (bytes, stride) = downloader.download_bytes();
        let rows: Vec<i32> = (0..h)
            .filter(|y| (0..w).any(|x| bytes[(*y as usize) * stride + (x as usize) * 4 + 3] > 40))
            .collect();
        (rows.len(), rows.first().copied().unwrap_or(-1), rows.last().copied().unwrap_or(-1))
    }
}
