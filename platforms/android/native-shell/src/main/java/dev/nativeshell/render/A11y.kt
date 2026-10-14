package dev.nativeshell.render

import android.view.View
import androidx.core.view.AccessibilityDelegateCompat
import androidx.core.view.ViewCompat
import androidx.core.view.accessibility.AccessibilityNodeInfoCompat
import dev.nativeshell.NativeShell
import dev.nativeshell.protocol.A11yRole
import dev.nativeshell.protocol.Prop
import dev.nativeshell.protocol.PropValue
import dev.nativeshell.util.Json

/** Accessibility props of one node. */
internal class A11yState {
    var label: String? = null
    var role = A11yRole.NONE
    var hint: String? = null
    var value: String? = null
    var disabled = false
    var selected = false
    var checked: Boolean? = null
    var expanded: Boolean? = null
    var busy = false
    var hidden = false
}

internal object A11y {

    fun set(node: Node, prop: Int, v: PropValue) {
        val s = node.a11y ?: A11yState().also { node.a11y = it }
        val view = node.view
        when (prop) {
            Prop.A11Y_LABEL -> { s.label = if (v.isNull) null else v.string; refreshDescription(node) }
            Prop.A11Y_ROLE -> s.role = if (v.isNull) A11yRole.NONE else v.intValue()
            Prop.A11Y_HINT -> s.hint = if (v.isNull) null else v.string
            Prop.A11Y_VALUE -> {
                s.value = if (v.isNull) null else v.string
                ViewCompat.setStateDescription(view, s.value)
            }
            Prop.A11Y_STATE -> {
                val o = if (v.isNull) null else Json.obj(v.string)
                s.disabled = Json.bool(o, "disabled", false)
                s.selected = Json.bool(o, "selected", false)
                s.checked = if (o != null && o.has("checked") && !o.isNull("checked")) Json.bool(o, "checked", false) else null
                s.expanded = if (o != null && o.has("expanded") && !o.isNull("expanded")) Json.bool(o, "expanded", false) else null
                s.busy = Json.bool(o, "busy", false)
            }
            Prop.A11Y_HIDDEN -> {
                s.hidden = !v.isNull && v.bool
                view.importantForAccessibility =
                    if (s.hidden) View.IMPORTANT_FOR_ACCESSIBILITY_NO_HIDE_DESCENDANTS else View.IMPORTANT_FOR_ACCESSIBILITY_AUTO
            }
        }
        ViewCompat.setAccessibilityHeading(view, s.role == A11yRole.HEADER)
        val empty = s.label == null && s.role == A11yRole.NONE && s.hint == null && s.value == null &&
            !s.disabled && !s.selected && s.checked == null && s.expanded == null && !s.busy && !s.hidden
        if (empty) {
            ViewCompat.setAccessibilityDelegate(view, null)
            node.a11y = null
        } else if (ViewCompat.getAccessibilityDelegate(view) !is Delegate) {
            ViewCompat.setAccessibilityDelegate(view, Delegate(node))
        }
    }

    /** contentDescription is the label, or (opt-in, for UI tests) the test id when there is no label. */
    fun refreshDescription(node: Node) {
        val label = node.a11y?.label
        node.view.contentDescription = label ?: if (NativeShell.testIdAsContentDescription) node.testId else null
    }

    private class Delegate(val node: Node) : AccessibilityDelegateCompat() {
        override fun onInitializeAccessibilityNodeInfo(host: View, info: AccessibilityNodeInfoCompat) {
            super.onInitializeAccessibilityNodeInfo(host, info)
            val s = node.a11y ?: return
            className(s.role)?.let { info.className = it }
            if (s.role == A11yRole.HEADER) info.isHeading = true
            s.hint?.let { info.hintText = it }
            if (s.disabled) info.isEnabled = false
            if (s.selected) info.isSelected = true
            s.checked?.let { info.isCheckable = true; info.isChecked = it }
            s.expanded?.let {
                info.addAction(
                    if (it) AccessibilityNodeInfoCompat.AccessibilityActionCompat.ACTION_COLLAPSE
                    else AccessibilityNodeInfoCompat.AccessibilityActionCompat.ACTION_EXPAND,
                )
            }
            if (s.role == A11yRole.BUTTON || s.role == A11yRole.LINK || s.role == A11yRole.TAB) info.isClickable = true
            if (s.busy) info.roleDescription = "busy"
        }
    }

    private fun className(role: Int): String? = when (role) {
        A11yRole.BUTTON, A11yRole.LINK -> "android.widget.Button"
        A11yRole.IMAGE -> "android.widget.ImageView"
        A11yRole.TEXT, A11yRole.HEADER -> "android.widget.TextView"
        A11yRole.TEXT_FIELD, A11yRole.SEARCH -> "android.widget.EditText"
        A11yRole.SWITCH -> "android.widget.Switch"
        A11yRole.CHECKBOX -> "android.widget.CheckBox"
        A11yRole.RADIO -> "android.widget.RadioButton"
        A11yRole.SLIDER -> "android.widget.SeekBar"
        A11yRole.PROGRESS_BAR -> "android.widget.ProgressBar"
        A11yRole.TAB -> "android.widget.TabWidget"
        A11yRole.TAB_LIST -> "android.widget.TabHost"
        A11yRole.LIST -> "android.widget.ListView"
        A11yRole.MENU -> "android.widget.PopupMenu"
        A11yRole.ALERT -> "android.app.AlertDialog"
        else -> null
    }
}
