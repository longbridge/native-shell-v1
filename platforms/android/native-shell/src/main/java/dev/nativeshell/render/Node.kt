package dev.nativeshell.render

import android.view.View
import dev.nativeshell.protocol.ViewType
import java.util.BitSet

/**
 * The renderer's record of one native node: its view, its place in the native
 * tree, the props currently set (so a pooled view can be reset exactly), its
 * frame in pixels and the state its view manager keeps.
 */
internal class Node(
    @JvmField val id: Int,
    @JvmField val type: Int,
    @JvmField val manager: ViewManager,
    @JvmField val view: View,
) {
    @JvmField var parent: Node? = null
    @JvmField val children = ArrayList<Node>(0)
    @JvmField val setProps = BitSet()
    @JvmField var eventMask = 0

    // Frame in px relative to the native parent.
    @JvmField var left = 0
    @JvmField var top = 0
    @JvmField var width = 0
    @JvmField var height = 0
    @JvmField var hasFrame = false
    /** Frame in points, as received (text layout lookups need the unrounded width). */
    @JvmField var widthPt = 0f
    @JvmField var heightPt = 0f

    @JvmField var box: BoxDrawable? = null
    @JvmField var animation: AnimationSpec? = null
    @JvmField var activeOpacity = Float.NaN
    @JvmField var activeBackground: Int? = null
    @JvmField var hoverOpacity = Float.NaN
    @JvmField var hoverBackground: Int? = null
    @JvmField var pressed = false
    @JvmField var hovered = false
    @JvmField var touch: TouchHandler? = null
    @JvmField var a11y: A11yState? = null
    @JvmField var baseAlpha = 1f
    @JvmField var testId: String? = null

    /** Per-type state owned by the manager (an EditText watcher, list adapter…). */
    @JvmField var state: Any? = null

    /** Whether this node is presented on its own (page, sheet, dialog) instead of as a child view. */
    val isSurface: Boolean
        get() = type == ViewType.PAGE || type == ViewType.SHEET || type == ViewType.DIALOG

    /** Children presented on their own (pages, sheets, dialogs); usually none. */
    @JvmField var surfaceChildren = 0

    /** Index among the parent's children that are views inside the parent's container. */
    fun inlineIndexOf(child: Node, logicalIndex: Int): Int {
        if (surfaceChildren == 0) return logicalIndex
        var n = 0
        for (c in children) {
            if (c === child) return n
            if (!c.isSurface) n++
        }
        return n
    }

    override fun toString(): String = "${ViewType.name(type)}#$id"
}
