//! One module per view type.

mod control;
mod date_picker;
mod host_component;
mod image;
mod input;
mod list;
pub mod modal;
#[cfg(test)]
mod overlay_tests;
pub mod overlays;
pub mod pages;
mod progress;
mod scroll;
mod segmented;
mod select;
mod shape;
mod slider;
mod text;
mod toggle;

use native_shell_core::protocol::ViewType;

use crate::component::{Component, NodeCx, Plain};

pub use overlays::root;

pub fn create(view_type: ViewType, cx: &NodeCx) -> Box<dyn Component> {
    match view_type {
        ViewType::View => Box::new(Plain),
        ViewType::Text => Box::new(text::Text::new(cx)),
        ViewType::Scroll => Box::new(scroll::Scroll::new(cx)),
        ViewType::List => Box::new(list::List::new(cx)),
        ViewType::ListRow => Box::new(list::ListRow),
        ViewType::Switch => Box::new(toggle::Switch::new(cx)),
        ViewType::Checkbox => Box::new(toggle::Checkbox::new(cx)),
        ViewType::Slider => Box::new(slider::Slider::new(cx)),
        ViewType::Progress => Box::new(progress::Progress::new(cx)),
        ViewType::Spinner => Box::new(progress::Spinner::new(cx)),
        ViewType::Segmented => Box::new(segmented::Segmented::new(cx)),
        ViewType::Select => Box::new(select::Select::new(cx)),
        ViewType::DatePicker => Box::new(date_picker::DatePicker::new(cx)),
        ViewType::HostComponent => Box::<host_component::HostComponentNode>::default(),
        ViewType::Page => pages::create(cx),
        ViewType::Sheet => modal::create(cx, modal::ModalKind::Sheet),
        ViewType::Dialog => modal::create(cx, modal::ModalKind::Dialog),
        ViewType::TextInput => input::create(cx),
        ViewType::Image => Box::new(image::Image::new(cx)),
        ViewType::Shape => Box::new(shape::Shape::new(cx)),
    }
}

/// `measure_component` for the reserved built-in names (`docs/protocol-json.md`,
/// "Built-in control measurement"); runs on the JS thread.
pub fn measure_builtin(name: &str, props_json: &str) -> Option<(f32, f32)> {
    let measure = match name {
        "nsh/select" => select::measure,
        "nsh/date-picker" => date_picker::measure,
        _ => return None,
    };
    Some(measure(&serde_json::from_str(props_json).ok()?))
}
