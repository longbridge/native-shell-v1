//! One module per view type. Types not implemented yet mount as plain
//! containers, so a program still shows everything else.

use native_shell_core::protocol::ViewType;

use crate::component::{Component, NodeCx, Plain};

pub fn create(view_type: ViewType, _cx: &NodeCx) -> Box<dyn Component> {
    match view_type {
        ViewType::View => Box::new(Plain),
        _ => Box::new(Plain),
    }
}
