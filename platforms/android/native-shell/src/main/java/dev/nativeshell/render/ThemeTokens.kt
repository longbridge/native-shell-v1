package dev.nativeshell.render

import android.content.Context
import android.content.res.Configuration
import dev.nativeshell.NativeShell

/**
 * The shell color tokens native chrome (toasts, picker fields) draws with: the
 * app's theme provider when set, else the defaults of crates/core/src/theme.rs.
 */
internal class ThemeTokens(
    val background: Int,
    val foreground: Int,
    val muted: Int,
    val mutedForeground: Int,
    val border: Int,
    val destructive: Int,
    val primary: Int,
    val primaryForeground: Int,
    val dark: Boolean = false,
) {
    companion object {
        fun isDark(context: Context) =
            context.resources.configuration.uiMode and Configuration.UI_MODE_NIGHT_MASK == Configuration.UI_MODE_NIGHT_YES

        /**
         * The tokens for views created from [context]: the surface's `ThemeTokens`
         * prop once the core sent it, else the theme provider / defaults.
         */
        fun of(context: Context): ThemeTokens = surfaceTokens(context) ?: of(isDark(context))

        private fun surfaceTokens(context: Context): ThemeTokens? {
            var c: Context? = context
            while (c != null) {
                if (c is ShellThemeContext) return c.tokens
                c = (c as? android.content.ContextWrapper)?.baseContext
            }
            return null
        }

        /** The `ThemeTokens` prop JSON; missing tokens fall back to the defaults of its appearance. */
        fun parse(json: String?): ThemeTokens? {
            val o = dev.nativeshell.util.Json.obj(json) ?: return null
            val d = of(dev.nativeshell.util.Json.string(o, "appearance") == "dark")
            fun c(k: String, def: Int) = dev.nativeshell.util.Json.color(o, k, def)
            return ThemeTokens(
                c("background", d.background), c("foreground", d.foreground), c("muted", d.muted),
                c("muted_foreground", d.mutedForeground), c("border", d.border), c("destructive", d.destructive),
                c("primary", d.primary), c("primary_foreground", d.primaryForeground), dark = d.dark,
            )
        }

        fun of(dark: Boolean): ThemeTokens {
            val t = NativeShell.themeProvider?.colors(dark).orEmpty()
            return if (dark) ThemeTokens(
                t["background"] ?: 0xFF0A0A0A.toInt(), t["foreground"] ?: 0xFFFAFAFA.toInt(), t["muted"] ?: 0xFF262626.toInt(),
                t["muted_foreground"] ?: 0xFFA3A3A3.toInt(), t["border"] ?: 0xFF262626.toInt(), t["destructive"] ?: 0xFFF87171.toInt(),
                t["primary"] ?: 0xFFFAFAFA.toInt(), t["primary_foreground"] ?: 0xFF171717.toInt(), dark = true,
            ) else ThemeTokens(
                t["background"] ?: 0xFFFFFFFF.toInt(), t["foreground"] ?: 0xFF0A0A0A.toInt(), t["muted"] ?: 0xFFF5F5F5.toInt(),
                t["muted_foreground"] ?: 0xFF737373.toInt(), t["border"] ?: 0xFFE5E5E5.toInt(), t["destructive"] ?: 0xFFEF4444.toInt(),
                t["primary"] ?: 0xFF171717.toInt(), t["primary_foreground"] ?: 0xFFFAFAFA.toInt(),
            )
        }
    }
}

/** The context a surface creates its views from; carries the core's resolved tokens. */
internal class ShellThemeContext(base: Context, theme: Int) : android.view.ContextThemeWrapper(base, theme) {
    @Volatile var tokens: ThemeTokens? = null
}
