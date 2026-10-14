// The shell's theme tokens on the renderer side, for what the renderer draws
// itself (control tints, toasts, picker chrome, sheet backgrounds). The source
// of truth is the core's `ThemeTokens` prop (defaults, host provider and the
// program's `set_theme`, resolved for the current appearance). Before it
// arrives, the core's defaults overridden by the host's theme provider stand in,
// as dynamic colors.

import UIKit

enum ThemeTokens {
    private static let light: [String: String] = [
        "background": "#ffffff", "foreground": "#0a0a0a", "muted_foreground": "#737373",
        "border": "#e5e5e5", "destructive": "#ef4444", "primary": "#171717", "primary_foreground": "#fafafa",
    ]
    private static let dark: [String: String] = [
        "background": "#0a0a0a", "foreground": "#fafafa", "muted_foreground": "#a3a3a3",
        "border": "#262626", "destructive": "#f87171", "primary": "#fafafa", "primary_foreground": "#171717",
    ]

    /// Radius `lg` of the theme scale.
    static let radius: CGFloat = 8

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
