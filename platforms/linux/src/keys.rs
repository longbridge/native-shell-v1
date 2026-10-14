//! Hardware keyboard events. Each key down/up is sent once, as `Key` on the
//! nearest node above the focused widget whose mask has KEY, or on the root
//! node when none has. A capture-phase controller on the window observes keys
//! before any widget does and never consumes them: entries still type and
//! shortcuts still fire.

use gtk::glib::translate::IntoGlib;
use gtk::prelude::*;
use gtk::{gdk, glib};
use serde_json::{json, Value};

use native_shell_core::protocol::event_mask::KEY;
use native_shell_core::protocol::EventType;

use crate::node_view::NodeView;

/// `{shift, ctrl, alt, cmd}`; `cmd` is the Super (logo) key, gpui's "platform" modifier on Linux.
pub fn modifiers(state: gdk::ModifierType) -> Value {
    json!({
        "shift": state.contains(gdk::ModifierType::SHIFT_MASK),
        "ctrl": state.contains(gdk::ModifierType::CONTROL_MASK),
        "alt": state.contains(gdk::ModifierType::ALT_MASK),
        "cmd": state.intersects(gdk::ModifierType::SUPER_MASK | gdk::ModifierType::META_MASK),
    })
}

/// The gpui key name: named keys, `f1`…`f12`, else the lowercase character
/// the key produces without modifiers. `None` for pure modifiers and keys
/// with no name.
pub fn key_name(key: gdk::Key, unmodified: gdk::Key) -> Option<String> {
    use gdk::Key;
    let named = match key {
        Key::Return | Key::KP_Enter | Key::ISO_Enter => "enter",
        Key::Escape => "escape",
        Key::Tab | Key::ISO_Left_Tab | Key::KP_Tab => "tab",
        Key::space | Key::KP_Space => "space",
        Key::BackSpace => "backspace",
        Key::Delete | Key::KP_Delete => "delete",
        Key::Up | Key::KP_Up => "up",
        Key::Down | Key::KP_Down => "down",
        Key::Left | Key::KP_Left => "left",
        Key::Right | Key::KP_Right => "right",
        Key::Home | Key::KP_Home => "home",
        Key::End | Key::KP_End => "end",
        Key::Page_Up | Key::KP_Page_Up => "pageup",
        Key::Page_Down | Key::KP_Page_Down => "pagedown",
        Key::Insert | Key::KP_Insert => "insert",
        Key::Menu => "menu",
        Key::Shift_L
        | Key::Shift_R
        | Key::Control_L
        | Key::Control_R
        | Key::Alt_L
        | Key::Alt_R
        | Key::Meta_L
        | Key::Meta_R
        | Key::Super_L
        | Key::Super_R
        | Key::Hyper_L
        | Key::Hyper_R
        | Key::Caps_Lock
        | Key::ISO_Level3_Shift
        | Key::Num_Lock => return None,
        _ => "",
    };
    if !named.is_empty() {
        return Some(named.into());
    }
    let (code, f1) = (key.into_glib(), Key::F1.into_glib());
    if (f1..=Key::F12.into_glib()).contains(&code) {
        return Some(format!("f{}", code - f1 + 1));
    }
    let c = unmodified
        .to_lower()
        .to_unicode()
        .or_else(|| key.to_lower().to_unicode())?;
    (!c.is_control()).then(|| c.to_lowercase().collect())
}

/// The `Key` payload, or `None` for keys gpui does not name.
pub fn payload(
    key: gdk::Key,
    unmodified: gdk::Key,
    state: gdk::ModifierType,
    phase: &str,
) -> Option<String> {
    let name = key_name(key, unmodified)?;
    // Named keys carry no character, except space.
    let key_char = if name.chars().count() > 1 && name != "space" {
        String::new()
    } else {
        key.to_unicode()
            .filter(|c| !c.is_control())
            .map(String::from)
            .unwrap_or_default()
    };
    Some(
        json!({"key": name, "key_char": key_char, "phase": phase, "modifiers": modifiers(state)})
            .to_string(),
    )
}

/// The node a key event goes to: the nearest KEY listener above `focus` that
/// belongs to `root`'s instance, else `root`. `None` when focus is in
/// something else (another surface in the same window).
pub fn target(root: &NodeView, focus: Option<gtk::Widget>) -> Option<NodeView> {
    let Some(focus) = focus else {
        return Some(root.clone());
    };
    let shared = root.shared();
    let mut inside = focus.is_ancestor(root) || &focus == root.upcast_ref::<gtk::Widget>();
    let mut cursor = Some(focus.clone());
    while let Some(w) = cursor {
        if let Some(node) = w.downcast_ref::<NodeView>() {
            let mine = match (&shared, node.shared()) {
                (Some(a), Some(b)) => std::rc::Rc::ptr_eq(a, &b),
                _ => false,
            };
            if mine {
                inside = true;
                if node.listens(KEY) {
                    return Some(node.clone());
                }
            }
        }
        cursor = w.parent();
    }
    // Focus on the window itself (nothing focused inside it) counts as ours.
    (inside || focus.is::<gtk::Window>()).then(|| root.clone())
}

/// Observes keys on the window `root` is shown in, for as long as it is mapped.
pub fn attach(root: &NodeView) {
    if root.is_mapped() {
        observe(root);
    }
    root.connect_map(observe);
}

fn observe(root: &NodeView) {
    let Some(window) = root.root().and_downcast::<gtk::Window>() else {
        return;
    };
    let keys = gtk::EventControllerKey::new();
    keys.set_propagation_phase(gtk::PropagationPhase::Capture);
    keys.set_name(Some("native-shell-keys"));
    let weak = root.downgrade();
    keys.connect_key_pressed(move |c, key, code, state| {
        if let Some(root) = weak.upgrade() {
            route(&root, c, key, code, state, "down");
        }
        glib::Propagation::Proceed
    });
    let weak = root.downgrade();
    keys.connect_key_released(move |c, key, code, state| {
        if let Some(root) = weak.upgrade() {
            route(&root, c, key, code, state, "up");
        }
    });
    window.add_controller(keys.clone());
    let window = window.downgrade();
    let handler = std::rc::Rc::new(std::cell::Cell::new(None));
    let h = handler.clone();
    handler.set(Some(root.connect_unmap(move |root| {
        if let Some(w) = window.upgrade() {
            w.remove_controller(&keys);
        }
        if let Some(id) = h.take() {
            root.disconnect(id);
        }
    })));
}

fn route(
    root: &NodeView,
    c: &gtk::EventControllerKey,
    key: gdk::Key,
    code: u32,
    state: gdk::ModifierType,
    phase: &str,
) {
    let window = c.widget();
    let focus = window
        .as_ref()
        .and_then(|w| w.root())
        .and_then(|r| r.focus());
    let Some(target) = target(root, focus) else {
        return;
    };
    // The key without modifiers names it (`shift-1` is "1", not "!"), in
    // the layout group the event was typed in.
    let unmodified = c
        .current_event()
        .zip(c.widget())
        .and_then(|(_, w)| {
            w.display()
                .translate_key(code, gdk::ModifierType::empty(), c.group() as i32)
        })
        .map_or(key, |(k, ..)| k);
    if let Some(payload) = payload(key, unmodified, state, phase) {
        target.emit(EventType::Key, payload);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gdk::Key;

    #[test]
    fn keys_use_gpui_names() {
        assert_eq!(key_name(Key::Return, Key::Return).as_deref(), Some("enter"));
        assert_eq!(
            key_name(Key::ISO_Left_Tab, Key::Tab).as_deref(),
            Some("tab")
        );
        assert_eq!(key_name(Key::F11, Key::F11).as_deref(), Some("f11"));
        assert_eq!(key_name(Key::Shift_L, Key::Shift_L), None);
        // Shifted keys are named by the key itself, like gpui.
        assert_eq!(key_name(Key::A, Key::a).as_deref(), Some("a"));
        assert_eq!(key_name(Key::exclam, Key::_1).as_deref(), Some("1"));
    }

    #[test]
    fn payloads_carry_the_typed_character() {
        let p: Value = serde_json::from_str(
            &payload(Key::A, Key::a, gdk::ModifierType::SHIFT_MASK, "down").unwrap(),
        )
        .unwrap();
        assert_eq!(p["key"], "a");
        assert_eq!(p["key_char"], "A");
        assert_eq!(p["phase"], "down");
        assert_eq!(p["modifiers"]["shift"], true);
        assert_eq!(p["modifiers"]["cmd"], false);
        let p: Value = serde_json::from_str(
            &payload(Key::Escape, Key::Escape, gdk::ModifierType::empty(), "up").unwrap(),
        )
        .unwrap();
        assert_eq!(p["key_char"], "");
        let p: Value = serde_json::from_str(
            &payload(Key::space, Key::space, gdk::ModifierType::empty(), "up").unwrap(),
        )
        .unwrap();
        assert_eq!(p["key_char"], " ");
    }
}
