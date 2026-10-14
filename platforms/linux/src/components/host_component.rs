//! `HostComponent`: a node showing a widget the host app registered by name
//! ([`crate::HostComponentType`]). The component is created when the name
//! arrives and replaced if the name changes; its events reach the script only
//! while it listens (`HOST_EVENT`).

use std::cell::Cell;
use std::rc::Rc;

use native_shell_core::protocol::{event_mask, EventType, Prop, PropValue};
use serde_json::Value;

use crate::component::{Component, NodeCx};
use crate::runtime::{EventSink, HostComponent};

#[derive(Default)]
pub struct HostComponentNode {
    component: Option<Box<dyn HostComponent>>,
    name: Option<String>,
    props: Option<Value>,
    listening: Rc<Cell<bool>>,
}

impl HostComponentNode {
    fn mount(&mut self, cx: &NodeCx, name: Option<&str>) {
        if self.name.as_deref() == name {
            return;
        }
        self.component = None;
        self.name = name.map(str::to_string);
        cx.view.set_content(None);
        let Some(name) = name else { return };
        let Some(registered) = cx.shared.runtime.component(name) else {
            let message = format!("native-shell: host component `{name}` is not registered");
            cx.shared.runtime.log(native_shell_core::host::LogLevel::Error, &message);
            return;
        };
        let (node, events, listening) = (cx.node, cx.shared.events.clone(), self.listening.clone());
        let sink = EventSink(Rc::new(move |event: &str, payload: Value| {
            if listening.get() {
                events(node, EventType::HostEvent, serde_json::json!({ "name": event, "payload": payload }).to_string());
            }
        }));
        let mut component = registered.create(sink);
        cx.view.set_content(Some(&component.widget()));
        if let Some(props) = &self.props {
            component.update(props);
        }
        self.component = Some(component);
    }
}

impl Component for HostComponentNode {
    fn set_prop(&mut self, cx: &NodeCx, prop: Prop, value: &PropValue) -> bool {
        match prop {
            Prop::HostComponent => {
                self.mount(cx, if let PropValue::Str(name) = value { Some(name) } else { None });
                true
            }
            Prop::HostProps => {
                self.props = match value {
                    PropValue::Json(json) => serde_json::from_str(json).ok(),
                    _ => None,
                };
                if let (Some(component), Some(props)) = (self.component.as_mut(), &self.props) {
                    component.update(props);
                }
                true
            }
            _ => false,
        }
    }

    fn set_event_mask(&mut self, _cx: &NodeCx, mask: u32) {
        self.listening.set(mask & event_mask::HOST_EVENT != 0);
    }

    fn unmount(&mut self, cx: &NodeCx) {
        self.component = None;
        cx.view.set_content(None);
    }
}
