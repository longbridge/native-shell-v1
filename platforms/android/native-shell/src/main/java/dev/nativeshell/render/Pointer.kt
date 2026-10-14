package dev.nativeshell.render

import android.annotation.SuppressLint
import android.os.Build
import android.view.MotionEvent
import android.view.PointerIcon
import android.view.View
import androidx.core.view.ViewCompat
import dev.nativeshell.protocol.EventMask
import dev.nativeshell.protocol.EventType

/**
 * Pointer platforms (Android with a mouse or stylus): hover feedback and
 * `Hover` events, the cursor shape and tooltips. Touch input never hovers, so
 * on phones nothing here runs.
 */
internal object Pointer {

    /**
     * Pressed and hover visuals of a node. Pressed wins over hover; with neither the
     * node shows its own opacity and background.
     */
    fun applyFeedback(node: Node) {
        val view = node.view
        val alphaFactor = when {
            node.pressed && !node.activeOpacity.isNaN() -> node.activeOpacity
            node.hovered && !node.hoverOpacity.isNaN() -> node.hoverOpacity
            else -> 1f
        }
        if (!node.activeOpacity.isNaN() || !node.hoverOpacity.isNaN()) view.alpha = node.baseAlpha * alphaFactor
        val fill = when {
            node.pressed && node.activeBackground != null -> node.activeBackground
            node.hovered && node.hoverBackground != null -> node.hoverBackground
            else -> null
        }
        if (node.activeBackground != null || node.hoverBackground != null || node.box?.pressedFill != null) {
            val box = node.box ?: BoxDrawable().also { node.box = it }
            if (box.pressedFill != fill) {
                box.pressedFill = fill
                CommonProps.applyBox(node)
            }
        }
    }

    /** Attaches a hover listener when the node has hover visuals or listens for `Hover`. */
    @SuppressLint("ClickableViewAccessibility")
    fun syncHover(node: Node, host: MountHost) {
        val wants = node.eventMask and EventMask.HOVER != 0 || !node.hoverOpacity.isNaN() || node.hoverBackground != null
        if (!wants) {
            node.view.setOnHoverListener(null)
            if (node.hovered) {
                node.hovered = false
                applyFeedback(node)
            }
            return
        }
        node.view.setOnHoverListener { v, e ->
            val entered = when (e.actionMasked) {
                MotionEvent.ACTION_HOVER_ENTER -> true
                MotionEvent.ACTION_HOVER_EXIT -> false
                else -> return@setOnHoverListener false
            }
            node.hovered = entered
            applyFeedback(node)
            if (node.eventMask and EventMask.HOVER != 0) {
                val d = host.density
                host.dispatch(node.id, EventType.HOVER, "{\"entered\":$entered,\"x\":${e.x / d},\"y\":${e.y / d}}")
            }
            // Not consumed: the view's own hover handling (tooltips) still runs.
            false
        }
    }

    /** GPUI's `CursorStyle` (protocol order) to Android's system pointer icons; null = default. */
    fun iconType(style: Int): Int? {
        if (Build.VERSION.SDK_INT < 24) return null
        return when (style) {
            0 -> PointerIcon.TYPE_ARROW
            1 -> PointerIcon.TYPE_TEXT
            2 -> PointerIcon.TYPE_CROSSHAIR
            3 -> PointerIcon.TYPE_GRABBING
            4 -> PointerIcon.TYPE_GRAB
            5 -> PointerIcon.TYPE_HAND
            6, 7, 8, 14 -> PointerIcon.TYPE_HORIZONTAL_DOUBLE_ARROW
            9, 10, 11, 15 -> PointerIcon.TYPE_VERTICAL_DOUBLE_ARROW
            12 -> PointerIcon.TYPE_TOP_LEFT_DIAGONAL_DOUBLE_ARROW
            13 -> PointerIcon.TYPE_TOP_RIGHT_DIAGONAL_DOUBLE_ARROW
            16 -> PointerIcon.TYPE_VERTICAL_TEXT
            17 -> PointerIcon.TYPE_NO_DROP
            18 -> PointerIcon.TYPE_ALIAS
            19 -> PointerIcon.TYPE_COPY
            20 -> PointerIcon.TYPE_CONTEXT_MENU
            else -> PointerIcon.TYPE_ARROW
        }
    }

    fun setCursor(view: View, style: Int?) {
        if (Build.VERSION.SDK_INT < 24) return
        val type = style?.let { iconType(it) }
        view.pointerIcon = type?.let { PointerIcon.getSystemIcon(view.context, it) }
    }

    fun setTooltip(view: View, text: String?) {
        ViewCompat.setTooltipText(view, text?.ifEmpty { null })
    }

    /** `modifiers` of a click from a pointer, or null when none is held. */
    fun modifiers(e: MotionEvent): String? {
        val shift = e.isShiftPressed()
        val ctrl = e.metaState and android.view.KeyEvent.META_CTRL_ON != 0
        val alt = e.metaState and android.view.KeyEvent.META_ALT_ON != 0
        val cmd = e.metaState and android.view.KeyEvent.META_META_ON != 0
        if (!shift && !ctrl && !alt && !cmd) return null
        return "{\"shift\":$shift,\"ctrl\":$ctrl,\"alt\":$alt,\"cmd\":$cmd}"
    }

    private fun MotionEvent.isShiftPressed() = metaState and android.view.KeyEvent.META_SHIFT_ON != 0
}
