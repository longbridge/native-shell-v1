package dev.nativeshell.render

import android.content.Context
import android.content.res.Configuration
import dev.nativeshell.NativeShell
import dev.nativeshell.util.Json

/**
 * The shell color tokens native chrome (control tints, toasts, picker fields,
 * sheet and dialog backgrounds, text selection) draws with: the core's
 * `ThemeTokens` prop once it arrived, else the app's theme provider, else the
 * defaults of crates/core/src/theme.rs. Unknown keys are ignored.
 */
internal class ThemeTokens(private val colors: Map<String, Int>, val dark: Boolean) {
    private fun c(name: String): Int = colors[name] ?: defaults(dark).getValue(name)

    val background get() = c("background")
    val foreground get() = c("foreground")
    val muted get() = c("muted")
    val mutedForeground get() = c("muted_foreground")
    val border get() = c("border")
    val destructive get() = c("destructive")
    val primary get() = c("primary")
    val primaryForeground get() = c("primary_foreground")
    val selection get() = c("selection")
    val success get() = c("success")
    val warning get() = c("warning")
    val info get() = c("info")

    companion object {
        /** crates/core/src/theme.rs (iOS system colors), as `#rrggbb[aa]`. */
        private val LIGHT = mapOf(
            "background" to "#ffffff", "foreground" to "#000000", "surface" to "#ffffff", "surface_foreground" to "#000000",
            "primary" to "#007aff", "primary_foreground" to "#ffffff", "secondary" to "#e5e5ea", "secondary_foreground" to "#000000",
            "muted" to "#f2f2f7", "muted_foreground" to "#8a8a8e", "accent" to "#007aff1f", "accent_foreground" to "#007aff",
            "destructive" to "#ff3b30", "destructive_foreground" to "#ffffff", "border" to "#c6c6c8", "input" to "#d1d1d6",
            "ring" to "#007aff80", "selection" to "#007aff33", "success" to "#34c759", "success_foreground" to "#ffffff",
            "warning" to "#ff9500", "warning_foreground" to "#000000", "info" to "#007aff", "info_foreground" to "#ffffff",
        )
        private val DARK = mapOf(
            "background" to "#000000", "foreground" to "#ffffff", "surface" to "#1c1c1e", "surface_foreground" to "#ffffff",
            "primary" to "#0a84ff", "primary_foreground" to "#ffffff", "secondary" to "#2c2c2e", "secondary_foreground" to "#ffffff",
            "muted" to "#1c1c1e", "muted_foreground" to "#8d8d93", "accent" to "#0a84ff2e", "accent_foreground" to "#0a84ff",
            "destructive" to "#ff453a", "destructive_foreground" to "#ffffff", "border" to "#38383a", "input" to "#3a3a3c",
            "ring" to "#0a84ff80", "selection" to "#0a84ff40", "success" to "#30d158", "success_foreground" to "#000000",
            "warning" to "#ff9f0a", "warning_foreground" to "#000000", "info" to "#0a84ff", "info_foreground" to "#ffffff",
        )
        private val parsedLight = LIGHT.mapValues { Json.parseHex(it.value)!! }
        private val parsedDark = DARK.mapValues { Json.parseHex(it.value)!! }

        private fun defaults(dark: Boolean) = if (dark) parsedDark else parsedLight

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
            val o = Json.obj(json) ?: return null
            val dark = Json.string(o, "appearance") == "dark"
            val colors = HashMap<String, Int>()
            for (k in o.keys()) Json.colorOf(o.opt(k))?.let { if (k != "appearance") colors[k] = it }
            return ThemeTokens(colors, dark)
        }

        /** Before the core's tokens arrive: the app's theme provider over the defaults. */
        fun of(dark: Boolean): ThemeTokens = ThemeTokens(NativeShell.themeProvider?.colors(dark).orEmpty(), dark)
    }
}

/** The context a surface creates its views from; carries the core's resolved tokens. */
internal class ShellThemeContext(base: Context, theme: Int) : android.view.ContextThemeWrapper(base, theme) {
    @Volatile var tokens: ThemeTokens? = null
}
