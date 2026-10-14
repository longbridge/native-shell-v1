//! Accessibility props mapped to GtkAccessible: role, label, description
//! (hint), value text and states. Screen readers reach a node's click through
//! the `node.activate` action (see [`NodeView::activate_click`]).

use gtk::prelude::*;
use serde_json::Value;

use native_shell_core::protocol::event_mask::CLICK;
use native_shell_core::protocol::{A11yRole, Prop, PropValue};

use crate::node_view::NodeView;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct A11y {
    pub label: Option<String>,
    pub hint: Option<String>,
    pub value: Option<String>,
    pub role: Option<A11yRole>,
    pub state: State,
    pub hidden: bool,
    /// The role the component gave its view (text is a label…), kept when
    /// the script declares none.
    component_role: Option<gtk::AccessibleRole>,
    /// What GTK was last told (role excluded), so unchanged nodes cost nothing.
    pushed: Option<Box<A11y>>,
}

/// `A11yState`: only the keys present are set.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct State {
    pub disabled: bool,
    pub selected: Option<bool>,
    pub checked: Option<bool>,
    pub expanded: Option<bool>,
    pub busy: bool,
}

impl State {
    pub fn parse(json: &str) -> State {
        let v: Value = serde_json::from_str(json).unwrap_or(Value::Null);
        let flag = |k: &str| v.get(k).and_then(Value::as_bool);
        State {
            disabled: flag("disabled").unwrap_or(false),
            selected: flag("selected"),
            checked: flag("checked"),
            expanded: flag("expanded"),
            busy: flag("busy").unwrap_or(false),
        }
    }
}

/// The GTK role of a node: its declared role, else a button when it is
/// clickable (as macOS does), else the role its component gave it.
pub fn gtk_role(role: Option<A11yRole>, clickable: bool, fallback: gtk::AccessibleRole) -> gtk::AccessibleRole {
    use gtk::AccessibleRole as R;
    match role {
        Some(A11yRole::Button) => R::Button,
        Some(A11yRole::Link) => R::Link,
        Some(A11yRole::Header) => R::Heading,
        Some(A11yRole::Image) => R::Img,
        Some(A11yRole::Text) => R::Label,
        Some(A11yRole::TextField) => R::TextBox,
        Some(A11yRole::Switch) => R::Switch,
        Some(A11yRole::Checkbox) => R::Checkbox,
        Some(A11yRole::Radio) => R::Radio,
        Some(A11yRole::Slider) => R::Slider,
        Some(A11yRole::ProgressBar) => R::ProgressBar,
        Some(A11yRole::Tab) => R::Tab,
        Some(A11yRole::TabList) => R::TabList,
        Some(A11yRole::List) => R::List,
        Some(A11yRole::ListItem) => R::ListItem,
        Some(A11yRole::Menu) => R::Menu,
        Some(A11yRole::Alert) => R::Alert,
        Some(A11yRole::Search) => R::SearchBox,
        Some(A11yRole::None) | None if clickable && fallback == R::Generic => R::Button,
        Some(A11yRole::None) | None => fallback,
    }
}

/// Applies an accessibility prop; false when `prop` is not one.
pub fn set(view: &NodeView, prop: Prop, value: &PropValue) -> bool {
    let text = || match value {
        PropValue::Str(s) if !s.is_empty() => Some(s.clone()),
        _ => None,
    };
    {
        let mut a = view.imp_a11y().borrow_mut();
        match prop {
            Prop::A11yLabel => a.label = text(),
            Prop::A11yHint => a.hint = text(),
            Prop::A11yValue => a.value = text(),
            Prop::A11yRole => {
                a.role = match value {
                    PropValue::I32(r) => A11yRole::from_raw(*r),
                    _ => None,
                }
            }
            Prop::A11yState => {
                a.state = match value {
                    PropValue::Json(j) => State::parse(j),
                    _ => State::default(),
                }
            }
            Prop::A11yHidden => a.hidden = matches!(value, PropValue::Bool(true)),
            _ => return false,
        }
    }
    refresh(view);
    true
}

/// Pushes the node's accessibility state to GTK.
pub fn refresh(view: &NodeView) {
    use gtk::accessible::{Property, State as S};
    let current = gtk::prelude::AccessibleExt::accessible_role(view);
    let fallback = *view.imp_a11y().borrow_mut().component_role.get_or_insert(current);
    let mut a = view.imp_a11y().borrow().clone();
    let role = gtk_role(a.role, view.listens(CLICK), fallback);
    if current != role {
        // GTK fixes the role once an assistive technology has seen the node;
        // roles are set right after creation, before it is shown.
        view.set_accessible_role(role);
    }
    view.update_cursor();
    let pushed = a.pushed.take();
    let fresh = A11y::default();
    if pushed.as_deref().unwrap_or(&fresh) == &a {
        return;
    }
    view.imp_a11y().borrow_mut().pushed = Some(Box::new(a.clone()));

    let mut props = Vec::new();
    let mut reset = Vec::new();
    match &a.label {
        Some(l) => props.push(Property::Label(l)),
        None => reset.push(gtk::AccessibleProperty::Label),
    }
    match &a.hint {
        Some(h) => props.push(Property::Description(h)),
        None => reset.push(gtk::AccessibleProperty::Description),
    }
    match &a.value {
        Some(v) => props.push(Property::ValueText(v)),
        None => reset.push(gtk::AccessibleProperty::ValueText),
    }
    if role == gtk::AccessibleRole::Heading {
        props.push(Property::Level(2));
    }
    if !props.is_empty() {
        view.update_property(&props);
    }
    for p in reset {
        view.reset_property(p);
    }

    let checked = match a.state.checked {
        Some(true) => gtk::AccessibleTristate::True,
        Some(false) => gtk::AccessibleTristate::False,
        None => gtk::AccessibleTristate::Mixed,
    };
    let mut states = vec![
        S::Disabled(a.state.disabled),
        S::Busy(a.state.busy),
        S::Hidden(a.hidden),
    ];
    states.push(S::Selected(a.state.selected));
    states.push(S::Expanded(a.state.expanded));
    if a.state.checked.is_some() {
        states.push(S::Checked(checked));
    } else {
        view.reset_state(gtk::AccessibleState::Checked);
    }
    view.update_state(&states);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roles_map_to_gtk() {
        use gtk::AccessibleRole as R;
        assert_eq!(gtk_role(Some(A11yRole::Header), false, R::Generic), R::Heading);
        assert_eq!(gtk_role(Some(A11yRole::Search), false, R::Generic), R::SearchBox);
        assert_eq!(gtk_role(None, true, R::Generic), R::Button);
        assert_eq!(gtk_role(Some(A11yRole::None), false, R::Generic), R::Generic);
        // A declared role wins over clickability; a component's own role over both.
        assert_eq!(gtk_role(Some(A11yRole::Link), true, R::Generic), R::Link);
        assert_eq!(gtk_role(None, true, R::Label), R::Label);
    }

    #[test]
    fn states_keep_absent_keys_unset() {
        let s = State::parse(r#"{"disabled": true, "checked": false}"#);
        assert!(s.disabled && !s.busy);
        assert_eq!(s.checked, Some(false));
        assert_eq!(s.selected, None);
        assert_eq!(State::parse("garbage"), State::default());
    }
}
