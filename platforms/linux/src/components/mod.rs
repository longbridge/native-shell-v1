//! One module per view type. Types not implemented yet mount as plain
//! containers, so a program still shows everything else.

mod text;

use native_shell_core::protocol::ViewType;

use crate::component::{Component, NodeCx, Plain};

pub fn create(view_type: ViewType, cx: &NodeCx) -> Box<dyn Component> {
    match view_type {
        ViewType::View => Box::new(Plain),
        ViewType::Text => Box::new(text::Text::new(cx)),
        _ => Box::new(Plain),
    }
}
