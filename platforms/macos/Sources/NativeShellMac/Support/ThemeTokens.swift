// The shell's theme tokens on the renderer side, for what the renderer draws
// itself (control tints, toasts, sheet and page backgrounds). The source of
// truth is the core's `ThemeTokens` prop (defaults, host provider and the
// program's `set_theme`, resolved for the current appearance). Before it
// arrives, the core's defaults overridden by the host's theme provider stand in,
// as dynamic colors.

import AppKit
import NativeShellShared

enum ThemeTokens {
    // The core's defaults (crates/core/src/theme.rs) for the tokens the renderer draws with.
    private static let light: [String: String] = [
        "background": "#ffffff", "foreground": "#000000", "muted_foreground": "#8a8a8e", "border": "#c6c6c8",
        "destructive": "#ff3b30", "primary": "#007aff", "primary_foreground": "#ffffff", "selection": "#007aff33",
        "success": "#34c759", "warning": "#ff9500", "info": "#007aff",
    ]
    private static let dark: [String: String] = [
        "background": "#000000", "foreground": "#ffffff", "muted_foreground": "#8d8d93", "border": "#38383a",
        "destructive": "#ff453a", "primary": "#0a84ff", "primary_foreground": "#ffffff", "selection": "#0a84ff40",
        "success": "#30d158", "warning": "#ff9f0a", "info": "#0a84ff",
    ]

    /// Radius `lg` of the theme scale.
    static let radius: CGFloat = 12

    static func color(_ token: String, runtime: NSHRuntime, theme: ThemeState? = nil) -> NSColor {
        if let c = theme?.colors[token] { return c }
        let provider = runtime.themeProvider
        return NSColor(name: nil) { appearance in
            let a: NSHAppearance = appearance.isDark ? .dark : .light
            if let c = provider?(a)[token] { return c }
            let table = a == .dark ? dark : light
            return table[token].flatMap(Colors.hex) ?? .labelColor
        }
    }
}

extension NSAppearance {
    var isDark: Bool { bestMatch(from: [.aqua, .darkAqua]) == .darkAqua }
}

extension NSColor {
    /// The CGColor of this color in `appearance` (dynamic colors resolve at the time CGColor is read).
    func cgColor(in appearance: NSAppearance) -> CGColor {
        var out = cgColor
        appearance.performAsCurrentDrawingAppearance { out = self.cgColor }
        return out
    }
}

/// The tokens the core last sent for one instance (`ThemeTokens` on the root node).
final class ThemeState {
    private(set) var colors: [String: NSColor] = [:]
    private(set) var appearance: String?

    func update(_ json: [String: Any]) {
        var c: [String: NSColor] = [:]
        for (k, v) in json where k != "appearance" {
            if let color = Colors.parse(v) { c[k] = color }
        }
        colors = c
        appearance = json.string("appearance")
    }

    func reset() {
        colors = [:]
        appearance = nil
    }
}
