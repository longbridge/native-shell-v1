// The shell's theme tokens on the renderer side, for the few things the renderer
// draws itself (toasts). Defaults mirror crates/core/src/theme.rs; the host's
// theme provider overrides them. Colors are dynamic, so they follow the trait
// collection's appearance.

import UIKit

enum ThemeTokens {
    private static let light: [String: String] = [
        "background": "#ffffff", "foreground": "#0a0a0a", "muted_foreground": "#737373",
        "border": "#e5e5e5", "destructive": "#ef4444",
    ]
    private static let dark: [String: String] = [
        "background": "#0a0a0a", "foreground": "#fafafa", "muted_foreground": "#a3a3a3",
        "border": "#262626", "destructive": "#f87171",
    ]

    /// Radius `lg` of the theme scale.
    static let radius: CGFloat = 8

    static func color(_ token: String, runtime: NSHRuntime) -> UIColor {
        let provider = runtime.themeProvider
        return UIColor { traits in
            let appearance: NSHAppearance = traits.userInterfaceStyle == .dark ? .dark : .light
            if let c = provider?(appearance)[token] { return c }
            let table = appearance == .dark ? dark : light
            return table[token].flatMap(Colors.hex) ?? .label
        }
    }
}
