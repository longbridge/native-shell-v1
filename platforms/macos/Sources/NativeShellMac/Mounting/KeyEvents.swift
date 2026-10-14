// Hardware keyboard events. Each key down/up is sent once, as `Key` on the
// nearest node on the first responder's view chain whose mask has KEY, or on
// the root node when none has. Events are observed, never consumed: text
// fields still type and menus still take their key equivalents.

import AppKit
import Carbon.HIToolbox

enum KeyEvents {
    /// The component a key event goes to: the nearest KEY listener above
    /// `responder` that belongs to `manager`, else the manager's root. Nil when
    /// the responder is not inside this instance at all.
    static func target(for responder: NSResponder?, in manager: MountingManager, window: NSWindow?) -> NSHComponentView? {
        var view: NSView? = responder as? NSView
        // A text field's editor is a subview of the field, so the chain reaches its input node.
        var inside = false
        while let v = view {
            if let c = v as? NSHComponentView, c.context === manager {
                inside = true
                if c.listens(EventMask.key) { return c }
            }
            if v === manager.rootView { inside = true }
            view = v.superview
        }
        if inside { return manager.rootView }
        // Nothing focused inside a component: the window or the surface itself has focus.
        if let w = window, responder === w || responder === w.contentView || responder == nil { return manager.rootView }
        return nil
    }

    static func modifiers(_ m: NSEvent.ModifierFlags) -> [String: Bool] {
        ["shift": m.contains(.shift), "ctrl": m.contains(.control), "alt": m.contains(.option), "cmd": m.contains(.command)]
    }

    /// The `Key` payload, or nil for pure modifier and system keys.
    static func payload(keyCode: UInt16, characters: String, ignoringModifiers: String,
                        modifiers m: NSEvent.ModifierFlags, phase: String) -> [String: Any]? {
        guard let name = name(keyCode, ignoringModifiers) else { return nil }
        // Named keys carry private-use characters in AppKit; gpui sends none for them.
        let named = name.count > 1
        return [
            "key": name,
            "key_char": named && name != "space" ? "" : characters,
            "phase": phase,
            "modifiers": modifiers(m),
        ]
    }

    static func payload(_ e: NSEvent) -> [String: Any]? {
        guard e.type == .keyDown || e.type == .keyUp else { return nil }
        return payload(keyCode: e.keyCode, characters: e.characters ?? "", ignoringModifiers: e.charactersIgnoringModifiers ?? "",
                       modifiers: e.modifierFlags, phase: e.type == .keyDown ? "down" : "up")
    }

    /// gpui key names.
    static func name(_ code: UInt16, _ chars: String) -> String? {
        switch Int(code) {
        case kVK_Return, kVK_ANSI_KeypadEnter: return "enter"
        case kVK_Escape: return "escape"
        case kVK_Tab: return "tab"
        case kVK_Space: return "space"
        case kVK_Delete: return "backspace"
        case kVK_ForwardDelete: return "delete"
        case kVK_UpArrow: return "up"
        case kVK_DownArrow: return "down"
        case kVK_LeftArrow: return "left"
        case kVK_RightArrow: return "right"
        case kVK_Home: return "home"
        case kVK_End: return "end"
        case kVK_PageUp: return "pageup"
        case kVK_PageDown: return "pagedown"
        case kVK_Shift, kVK_RightShift, kVK_Control, kVK_RightControl, kVK_Option, kVK_RightOption, kVK_Command,
             kVK_RightCommand, kVK_CapsLock, kVK_Function:
            return nil
        default: break
        }
        let fKeys = [kVK_F1, kVK_F2, kVK_F3, kVK_F4, kVK_F5, kVK_F6, kVK_F7, kVK_F8, kVK_F9, kVK_F10, kVK_F11, kVK_F12]
        if let i = fKeys.firstIndex(of: Int(code)) { return "f\(i + 1)" }
        let c = chars.lowercased()
        // Remaining function-key characters (help, clear…) have no gpui name.
        guard let scalar = c.unicodeScalars.first, !(0xF700 ... 0xF8FF).contains(scalar.value) else { return nil }
        return c.isEmpty ? nil : c
    }
}
