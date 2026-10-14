//! `Select`: a button showing the current choice and an up/down chevron (the
//! iOS pop-up button), drawn; it opens a GTK popover listing the items, with a
//! check on the selected one, destructive items in red and disabled ones
//! dimmed.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gtk::accessible::{Property, State as A11y};
use gtk::prelude::*;

use native_shell_core::protocol::{Prop, PropValue};

use super::control::{self, bool_or, int_or, Canvas, Link};
use super::toggle::shared_prop;
use crate::component::{Component, NodeCx};
use crate::mount::json;

pub const PADDING: f32 = 12.0;
pub const GAP: f32 = 6.0;
pub const MIN_HEIGHT: f32 = 44.0;
pub const FONT_SIZE: f32 = 16.0;
/// The drawn chevron pair's box.
pub const CHEVRON: (f32, f32) = (9.0, 14.0);

#[derive(Clone)]
struct Item {
    label: String,
    value: serde_json::Value,
    destructive: bool,
    disabled: bool,
}

struct TextStyle {
    size: f32,
    weight: u16,
    color: Option<u32>,
}

struct State {
    title: RefCell<String>,
    placeholder: RefCell<String>,
    items: RefCell<Vec<Item>>,
    selected: Cell<i32>,
    style: RefCell<TextStyle>,
    popover: RefCell<Option<gtk::Popover>>,
}

impl State {
    fn label(&self) -> Option<String> {
        let i = self.selected.get();
        (i >= 0).then(|| self.items.borrow().get(i as usize).map(|it| it.label.clone())).flatten()
    }
}

pub struct Select {
    link: Link,
    canvas: Canvas,
    state: Rc<State>,
}

impl Select {
    pub fn new(cx: &NodeCx) -> Select {
        let link = Link::new(cx);
        let canvas = Canvas::new(gtk::AccessibleRole::ComboBox, true);
        canvas.update_property(&[Property::HasPopup(true)]);
        canvas.update_state(&[A11y::Expanded(Some(false))]);
        let state = Rc::new(State {
            title: RefCell::default(),
            placeholder: RefCell::default(),
            items: RefCell::default(),
            selected: Cell::new(-1),
            style: RefCell::new(TextStyle { size: FONT_SIZE, weight: 400, color: None }),
            popover: RefCell::default(),
        });
        let (l, s) = (link.clone(), state.clone());
        canvas.set_paint(move |c, snapshot| paint(c, snapshot, &l, &s));
        let (l, s) = (link.clone(), state.clone());
        canvas.on_tap(move |c, _, _| open(c, &l, &s));
        let (l, s) = (link.clone(), state.clone());
        canvas.on_activate_key(move |c| open(c, &l, &s));
        cx.view.set_content(Some(canvas.upcast_ref()));
        Select { link, canvas, state }
    }
}

fn paint(c: &Canvas, snapshot: &gtk::Snapshot, l: &Link, s: &State) {
    let (w, h) = (c.width() as f32, c.height() as f32);
    let style = s.style.borrow();
    // The placeholder is drawn like a value, as iOS titles the button: menus
    // without a selection (a "Sort by" or "More" button) name themselves with it.
    let text = s.label().unwrap_or_else(|| s.placeholder.borrow().clone());
    let color = style.color.unwrap_or_else(|| l.color("foreground"));
    let alpha = if !c.is_sensitive() {
        control::DISABLED_ALPHA
    } else if c.pressed() || s.popover.borrow().as_ref().is_some_and(|p| p.is_visible()) {
        control::PRESSED_ALPHA
    } else {
        1.0
    };
    snapshot.push_opacity(alpha as f64);
    let layout = control::line(&text, style.size, style.weight, color, Some(w - 2.0 * PADDING - GAP - CHEVRON.0));
    let (_, th) = control::text_size(&layout);
    control::draw_text(snapshot, &layout, PADDING, (h - th) / 2.0);

    // chevron.up.chevron.down
    let (cw, ch) = CHEVRON;
    let x = w - PADDING - cw;
    let y = (h - ch) / 2.0;
    let accent = l.accent();
    control::stroke(snapshot, &[(x + 1.0, y + 5.0), (x + cw / 2.0, y + 1.5), (x + cw - 1.0, y + 5.0)], 1.6, accent);
    control::stroke(snapshot, &[(x + 1.0, y + ch - 5.0), (x + cw / 2.0, y + ch - 1.5), (x + cw - 1.0, y + ch - 5.0)], 1.6, accent);
    snapshot.pop();
    if c.shows_focus() {
        control::focus_ring(snapshot, &control::round(control::rect(0.0, 0.0, w, h), 8.0), l.ring());
    }
}

/// Opens the menu: a GTK popover under the button.
fn open(c: &Canvas, l: &Link, s: &Rc<State>) {
    if !c.is_sensitive() {
        return;
    }
    let popover = s
        .popover
        .borrow_mut()
        .get_or_insert_with(|| {
            let p = gtk::Popover::new();
            p.add_css_class("menu");
            p.set_position(gtk::PositionType::Bottom);
            p.set_parent(c);
            let weak = c.downgrade();
            p.connect_closed(move |_| {
                if let Some(c) = weak.upgrade() {
                    c.update_state(&[A11y::Expanded(Some(false))]);
                    c.queue_draw();
                }
            });
            p
        })
        .clone();
    let (content, current) = menu(&popover, l, s);
    popover.set_child(Some(&content));
    c.update_state(&[A11y::Expanded(Some(true))]);
    popover.popup();
    // Keyboard users start on the current choice.
    if let Some(row) = current {
        row.grab_focus();
    }
    c.queue_draw();
}

/// The menu's content, and the row of the current choice.
fn menu(popover: &gtk::Popover, l: &Link, s: &Rc<State>) -> (gtk::Widget, Option<gtk::ListBoxRow>) {
    let column = gtk::Box::new(gtk::Orientation::Vertical, 0);
    let title = s.title.borrow();
    if !title.is_empty() {
        let heading = gtk::Label::new(Some(&title));
        heading.add_css_class("dim-label");
        heading.add_css_class("caption-heading");
        heading.set_xalign(0.0);
        heading.set_margin_start(12);
        heading.set_margin_end(12);
        heading.set_margin_top(6);
        heading.set_margin_bottom(6);
        column.append(&heading);
    }
    let list = gtk::ListBox::new();
    list.set_selection_mode(gtk::SelectionMode::None);
    list.add_css_class("navigation-sidebar");
    let selected = s.selected.get();
    for (i, item) in s.items.borrow().iter().enumerate() {
        let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        row.set_margin_start(4);
        row.set_margin_end(12);
        row.set_margin_top(4);
        row.set_margin_bottom(4);
        let check = gtk::Image::from_icon_name("object-select-symbolic");
        check.set_opacity(if i as i32 == selected { 1.0 } else { 0.0 });
        row.append(&check);
        let label = gtk::Label::new(Some(&item.label));
        label.set_xalign(0.0);
        label.set_hexpand(true);
        if item.destructive {
            label.add_css_class("error");
        }
        row.append(&label);
        let list_row = gtk::ListBoxRow::new();
        list_row.set_child(Some(&row));
        list_row.set_sensitive(!item.disabled);
        if i as i32 == selected {
            list_row.update_state(&[A11y::Checked(gtk::AccessibleTristate::True)]);
        }
        list.append(&list_row);
    }
    let (weak, l, s) = (popover.downgrade(), l.clone(), s.clone());
    list.connect_row_activated(move |_, row| {
        if let Some(p) = weak.upgrade() {
            p.popdown();
            if let Some(c) = p.parent().and_then(|w| w.downcast::<Canvas>().ok()) {
                choose(&c, &l, &s, row.index());
            }
        }
    });
    let current = list.row_at_index(selected.max(0));
    let scroller = gtk::ScrolledWindow::new();
    scroller.set_hscrollbar_policy(gtk::PolicyType::Never);
    scroller.set_propagate_natural_height(true);
    scroller.set_max_content_height(360);
    scroller.set_child(Some(&list));
    column.append(&scroller);
    (column.upcast(), current)
}

/// What choosing menu item `i` does (also the test entry point).
fn choose(c: &Canvas, l: &Link, s: &State, i: i32) {
    let item = {
        let items = s.items.borrow();
        let Some(item) = usize::try_from(i).ok().and_then(|i| items.get(i)) else { return };
        if item.disabled {
            return;
        }
        item.clone()
    };
    s.selected.set(i);
    c.queue_draw();
    l.change(serde_json::json!({ "index": i, "value": item.value }));
}

fn parse_items(items: Option<&serde_json::Value>) -> Vec<Item> {
    let Some(serde_json::Value::Array(items)) = items else { return Vec::new() };
    items
        .iter()
        .map(|it| match it {
            serde_json::Value::Object(o) => Item {
                label: o.get("label").and_then(|v| v.as_str()).unwrap_or_default().to_string(),
                value: o.get("value").cloned().unwrap_or(serde_json::Value::Null),
                destructive: o.get("destructive").and_then(|v| v.as_bool()).unwrap_or(false),
                disabled: o.get("disabled").and_then(|v| v.as_bool()).unwrap_or(false),
            },
            serde_json::Value::String(s) => Item { label: s.clone(), value: it.clone(), destructive: false, disabled: false },
            other => Item { label: other.to_string(), value: other.clone(), destructive: false, disabled: false },
        })
        .collect()
}

impl Component for Select {
    fn set_prop(&mut self, _cx: &NodeCx, prop: Prop, value: &PropValue) -> bool {
        let s = &self.state;
        match prop {
            Prop::Menu => {
                let menu = json(value).unwrap_or_default();
                s.title.replace(menu["title"].as_str().unwrap_or_default().to_string());
                if let Some(p) = menu["placeholder"].as_str() {
                    s.placeholder.replace(p.to_string());
                }
                s.items.replace(parse_items(menu.get("items")));
            }
            Prop::Items => {
                s.items.replace(parse_items(json(value).as_ref()));
            }
            Prop::SelectedIndex => s.selected.set(int_or(value, -1)),
            Prop::Placeholder => {
                s.placeholder.replace(control::str_or(value, "").to_string());
            }
            Prop::Enabled => self.canvas.set_sensitive(bool_or(value, true)),
            Prop::TextStyle => {
                let v = json(value).unwrap_or_default();
                s.style.replace(TextStyle {
                    size: v["size"].as_f64().map_or(FONT_SIZE, |x| x as f32),
                    weight: v["weight"].as_u64().map_or(400, |w| w.clamp(100, 900) as u16),
                    color: v["color"].as_u64().map(|c| c as u32),
                });
            }
            _ => return shared_prop(&self.link, &self.canvas, prop, value).unwrap_or(false),
        }
        self.canvas.queue_draw();
        true
    }

    fn set_event_mask(&mut self, _cx: &NodeCx, mask: u32) {
        self.link.mask.set(mask);
    }

    fn unmount(&mut self, _cx: &NodeCx) {
        if let Some(p) = self.state.popover.take() {
            p.unparent();
        }
    }
}

/// `nsh/select` measurement (on the JS thread): the label at the control font
/// plus chevron and padding.
pub fn measure(props: &serde_json::Value) -> (f32, f32) {
    let label = props["label"].as_str().filter(|s| !s.is_empty()).or(props["placeholder"].as_str()).unwrap_or_default();
    let (tw, th) = control::line_size(label, FONT_SIZE, 400);
    ((PADDING + tw + GAP + CHEVRON.0 + PADDING).ceil(), MIN_HEIGHT.max((th + 16.0).ceil()))
}
