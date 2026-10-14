//! `Key` event payloads from GDK key presses, with gpui key names.

use gtk::gdk;
use gtk::glib::translate::IntoGlib;

/// The `Key` payload, or `None` for pure modifier keys.
pub fn payload(keyval: gdk::Key, state: gdk::ModifierType, phase: &str) -> Option<String> {
    let name = name(keyval)?;
    let key_char = keyval.to_unicode().filter(|c| !c.is_control()).map(String::from).unwrap_or_default();
    let has = |m: gdk::ModifierType| state.contains(m);
    Some(
        serde_json::json!({
            "key": name,
            "key_char": key_char,
            "phase": phase,
            "modifiers": {
                "shift": has(gdk::ModifierType::SHIFT_MASK),
                "ctrl": has(gdk::ModifierType::CONTROL_MASK),
                "alt": has(gdk::ModifierType::ALT_MASK),
                "cmd": has(gdk::ModifierType::SUPER_MASK) || has(gdk::ModifierType::META_MASK),
            },
        })
        .to_string(),
    )
}

/// gpui key names.
pub fn name(keyval: gdk::Key) -> Option<String> {
    use gdk::Key;
    let named = match keyval {
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
        Key::Shift_L | Key::Shift_R | Key::Control_L | Key::Control_R | Key::Alt_L | Key::Alt_R | Key::Super_L
        | Key::Super_R | Key::Meta_L | Key::Meta_R | Key::Caps_Lock | Key::ISO_Level3_Shift => return None,
        _ => {
            let (f, f1) = (keyval.into_glib(), Key::F1.into_glib());
            if (f1..=Key::F12.into_glib()).contains(&f) {
                return Some(format!("f{}", f - f1 + 1));
            }
            let c = keyval.to_lower().to_unicode().filter(|c| !c.is_control())?;
            return Some(c.to_lowercase().collect());
        }
    };
    Some(named.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_follow_gpui() {
        assert_eq!(name(gdk::Key::Return).as_deref(), Some("enter"));
        assert_eq!(name(gdk::Key::ISO_Left_Tab).as_deref(), Some("tab"));
        assert_eq!(name(gdk::Key::F5).as_deref(), Some("f5"));
        assert_eq!(name(gdk::Key::A).as_deref(), Some("a"));
        assert_eq!(name(gdk::Key::Shift_L), None);
    }

    #[test]
    fn payload_carries_modifiers() {
        let p = payload(gdk::Key::a, gdk::ModifierType::CONTROL_MASK, "down").unwrap_or_default();
        let v: serde_json::Value = serde_json::from_str(&p).unwrap_or_default();
        assert_eq!(v["key"], "a");
        assert_eq!(v["key_char"], "a");
        assert_eq!(v["modifiers"]["ctrl"], true);
        assert_eq!(v["phase"], "down");
    }
}
