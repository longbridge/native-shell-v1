// The shell's theme tokens on the renderer side, for what the renderer draws
// itself (control tints, toasts, picker chrome, sheet backgrounds). The source
// of truth is the core's `ThemeTokens` prop (defaults, host provider and the
// program's `set_theme`, resolved for the current appearance). Before it
// arrives, the core's defaults overridden by the host's theme provider stand in,
// as dynamic colors.

import NativeShellShared
import UIKit

enum ThemeTokens {
    /// The core's defaults (crates/core/src/theme.rs), keep in sync.
    private static let light: [String: String] = [
        "background": "#ffffff", "foreground": "#000000", "surface": "#ffffff",
        "surface_foreground": "#000000", "primary": "#007aff", "primary_foreground": "#ffffff",
        "secondary": "#e5e5ea", "secondary_foreground": "#000000", "muted": "#f2f2f7",
        "muted_foreground": "#8a8a8e", "accent": "#007aff1f", "accent_foreground": "#007aff",
        "destructive": "#ff3b30", "destructive_foreground": "#ffffff", "border": "#c6c6c8",
        "input": "#d1d1d6", "ring": "#007aff80", "selection": "#007aff33", "success": "#34c759",
        "success_foreground": "#ffffff", "warning": "#ff9500", "warning_foreground": "#000000",
        "info": "#007aff", "info_foreground": "#ffffff",
    ]
    private static let dark: [String: String] = [
        "background": "#000000", "foreground": "#ffffff", "surface": "#1c1c1e",
        "surface_foreground": "#ffffff", "primary": "#0a84ff", "primary_foreground": "#ffffff",
        "secondary": "#2c2c2e", "secondary_foreground": "#ffffff", "muted": "#1c1c1e",
        "muted_foreground": "#8d8d93", "accent": "#0a84ff2e", "accent_foreground": "#0a84ff",
        "destructive": "#ff453a", "destructive_foreground": "#ffffff", "border": "#38383a",
        "input": "#3a3a3c", "ring": "#0a84ff80", "selection": "#0a84ff40", "success": "#30d158",
        "success_foreground": "#000000", "warning": "#ff9f0a", "warning_foreground": "#000000",
        "info": "#0a84ff", "info_foreground": "#ffffff",
    ]

    /// Radius `lg` of the theme scale (iOS cards and filled buttons).
    static let radius: CGFloat = 12

    static func color(_ token: String, runtime: NSHRuntime, theme: ThemeState? = nil) -> UIColor {
        if let c = theme?.colors[token] { return c }
        let provider = runtime.themeProvider
        return UIColor { traits in
            let appearance: NSHAppearance = traits.userInterfaceStyle == .dark ? .dark : .light
            if let c = provider?(appearance)[token] { return c }
            let table = appearance == .dark ? dark : light
            return table[token].flatMap(Colors.hex) ?? .label
        }
    }
}

/// The tokens the core last sent for one instance (`ThemeTokens` on the root node).
final class ThemeState {
    private(set) var colors: [String: UIColor] = [:]
    private(set) var appearance: String?

    func update(_ json: [String: Any]) {
        var c: [String: UIColor] = [:]
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
