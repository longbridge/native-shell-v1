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
) {
    companion object {
        fun isDark(context: Context) =
            context.resources.configuration.uiMode and Configuration.UI_MODE_NIGHT_MASK == Configuration.UI_MODE_NIGHT_YES

        fun of(context: Context): ThemeTokens = of(isDark(context))

        fun of(dark: Boolean): ThemeTokens {
            val t = NativeShell.themeProvider?.colors(dark).orEmpty()
            return if (dark) ThemeTokens(
                t["background"] ?: 0xFF0A0A0A.toInt(), t["foreground"] ?: 0xFFFAFAFA.toInt(), t["muted"] ?: 0xFF262626.toInt(),
                t["muted_foreground"] ?: 0xFFA3A3A3.toInt(), t["border"] ?: 0xFF262626.toInt(), t["destructive"] ?: 0xFFF87171.toInt(),
            ) else ThemeTokens(
                t["background"] ?: 0xFFFFFFFF.toInt(), t["foreground"] ?: 0xFF0A0A0A.toInt(), t["muted"] ?: 0xFFF5F5F5.toInt(),
                t["muted_foreground"] ?: 0xFF737373.toInt(), t["border"] ?: 0xFFE5E5E5.toInt(), t["destructive"] ?: 0xFFEF4444.toInt(),
            )
        }
    }
}
