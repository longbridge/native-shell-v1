package dev.nativeshell.views

import android.content.res.ColorStateList
import android.view.View
import com.google.android.material.button.MaterialButton
import com.google.android.material.button.MaterialButtonToggleGroup
import com.google.android.material.checkbox.MaterialCheckBox
import com.google.android.material.materialswitch.MaterialSwitch
import com.google.android.material.progressindicator.BaseProgressIndicator
import com.google.android.material.slider.Slider
import dev.nativeshell.R
import dev.nativeshell.render.ThemeTokens

/**
 * Native controls take the shell's `primary` token (or the node's `TintColor`)
 * instead of the Material theme's baseline color, with `primary_foreground` on
 * top of it. Applied at creation, on `TintColor` changes and again when the
 * appearance changes.
 */
internal object ControlTint {
    private val CHECKED = intArrayOf(android.R.attr.state_checked)
    private val NONE = intArrayOf()

    /** Sets (or clears) the node's own tint and re-applies. */
    fun setOverride(view: View, color: Int?) {
        view.setTag(R.id.nsh_tint, color)
        apply(view)
    }

    fun apply(view: View) {
        val t = ThemeTokens.of(view.context)
        val accent = view.getTag(R.id.nsh_tint) as? Int ?: t.primary
        fun checked(on: Int, off: Int) = ColorStateList(arrayOf(CHECKED, NONE), intArrayOf(on, off))
        when (view) {
            is MaterialSwitch -> {
                view.trackTintList = checked(accent, t.muted)
                view.trackDecorationTintList = checked(accent, t.border)
                view.thumbTintList = checked(t.primaryForeground, t.mutedForeground)
            }
            is CheckContainer -> {
                view.button.buttonTintList = checked(accent, t.mutedForeground)
                (view.button as? MaterialCheckBox)?.buttonIconTintList = ColorStateList.valueOf(t.primaryForeground)
            }
            is Slider -> {
                view.trackActiveTintList = ColorStateList.valueOf(accent)
                view.trackInactiveTintList = ColorStateList.valueOf(t.muted)
                view.thumbTintList = ColorStateList.valueOf(accent)
                view.haloTintList = ColorStateList.valueOf((accent and 0x00FFFFFF) or 0x33000000)
                view.tickTintList = ColorStateList.valueOf(t.mutedForeground)
            }
            // Text selection highlight (fields and selectable text).
            is android.widget.EditText -> view.highlightColor = t.selection
            is ShellTextView -> view.selectionHighlight = t.selection
            is BaseProgressIndicator<*> -> {
                view.setIndicatorColor(accent)
                view.trackColor = t.muted
            }
            // Material 3's segmented button: a quiet secondary-container fill for the selected
            // segment, not a solid accent; a script's TintColor still tints it.
            is MaterialButtonToggleGroup -> for (i in 0 until view.childCount) {
                val b = view.getChildAt(i) as? MaterialButton ?: continue
                val tinted = view.getTag(R.id.nsh_tint) as? Int
                b.backgroundTintList = checked(tinted ?: t.secondary, 0x00000000)
                b.setTextColor(checked(if (tinted != null) t.primaryForeground else t.secondaryForeground, t.foreground))
                b.strokeColor = ColorStateList.valueOf(t.border)
                b.rippleColor = ColorStateList.valueOf((t.foreground and 0x00FFFFFF) or 0x1F000000)
            }
        }
    }

    /** View types [apply] knows. */
    fun handles(view: View) = view is MaterialSwitch || view is CheckContainer || view is Slider ||
        view is BaseProgressIndicator<*> || view is MaterialButtonToggleGroup || view is android.widget.EditText || view is ShellTextView
}
