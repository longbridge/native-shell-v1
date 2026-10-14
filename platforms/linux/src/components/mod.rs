//! One module per view type. Types not implemented yet mount as plain
//! containers, so a program still shows everything else.

mod control;
mod date_picker;
mod list;
mod progress;
mod scroll;
mod segmented;
mod select;
mod slider;
mod text;
mod toggle;

use native_shell_core::protocol::ViewType;

use crate::component::{Component, NodeCx, Plain};

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
        _ => Box::new(Plain),
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
