// Hardware keyboard events. Each key down/up is sent once, as `Key` on the
// nearest node on the responder chain whose mask has KEY, or on the root node
// when none has. Presses are never consumed: text fields still type.

import UIKit

enum KeyEvents {
    private static var markKey = 0

    /// Called by every responder on the chain; the first one that listens (or
    /// the surface, last) takes the press for that phase.
    static func route(_ presses: Set<UIPress>, phase: String, to view: NSHComponentView?, isRoot: Bool = false) {
        for press in presses {
            guard let key = press.key, !claimed(press, phase) else { continue }
            guard let target = view, isRoot || target.listens(EventMask.key) else { continue }
            guard let payload = payload(keyCode: key.keyCode, characters: key.characters,
                                        ignoringModifiers: key.charactersIgnoringModifiers, modifiers: key.modifierFlags, phase: phase)
            else { continue }
            claim(press, phase)
            target.send(.key, payload)
        }
    }

    private static func claimed(_ press: UIPress, _ phase: String) -> Bool {
        (objc_getAssociatedObject(press, &markKey) as? Set<String>)?.contains(phase) ?? false
    }

    private static func claim(_ press: UIPress, _ phase: String) {
        var s = (objc_getAssociatedObject(press, &markKey) as? Set<String>) ?? []
        s.insert(phase)
        objc_setAssociatedObject(press, &markKey, s, .OBJC_ASSOCIATION_RETAIN_NONATOMIC)
    }

    /// The `Key` payload, or nil for pure modifier and system keys.
    static func payload(keyCode: UIKeyboardHIDUsage, characters: String, ignoringModifiers: String,
                        modifiers m: UIKeyModifierFlags, phase: String) -> [String: Any]? {
        guard let name = name(keyCode, ignoringModifiers) else { return nil }
        return [
            "key": name,
            "key_char": characters,
            "phase": phase,
            "modifiers": ["shift": m.contains(.shift), "ctrl": m.contains(.control), "alt": m.contains(.alternate), "cmd": m.contains(.command)],
        ]
    }

    /// gpui key names.
    static func name(_ code: UIKeyboardHIDUsage, _ chars: String) -> String? {
        switch code {
        case .keyboardReturnOrEnter, .keypadEnter: return "enter"
        case .keyboardEscape: return "escape"
        case .keyboardTab: return "tab"
        case .keyboardSpacebar: return "space"
        case .keyboardDeleteOrBackspace: return "backspace"
        case .keyboardDeleteForward: return "delete"
        case .keyboardUpArrow: return "up"
        case .keyboardDownArrow: return "down"
        case .keyboardLeftArrow: return "left"
        case .keyboardRightArrow: return "right"
        case .keyboardHome: return "home"
        case .keyboardEnd: return "end"
        case .keyboardPageUp: return "pageup"
        case .keyboardPageDown: return "pagedown"
        case .keyboardLeftShift, .keyboardRightShift, .keyboardLeftControl, .keyboardRightControl, .keyboardLeftAlt,
             .keyboardRightAlt, .keyboardLeftGUI, .keyboardRightGUI, .keyboardCapsLock, .keyboardLockingCapsLock:
            return nil
        default: break
        }
        if code.rawValue >= UIKeyboardHIDUsage.keyboardF1.rawValue, code.rawValue <= UIKeyboardHIDUsage.keyboardF12.rawValue {
            return "f\(code.rawValue - UIKeyboardHIDUsage.keyboardF1.rawValue + 1)"
        }
        let c = chars.lowercased()
        return c.isEmpty ? nil : c
    }
}
