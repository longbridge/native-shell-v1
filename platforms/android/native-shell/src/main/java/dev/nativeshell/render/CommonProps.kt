package dev.nativeshell.render

import android.graphics.Color
import android.os.Build
import android.view.View
import android.view.ViewOutlineProvider
import androidx.core.view.ViewCompat
import dev.nativeshell.NativeShell
import dev.nativeshell.R
import dev.nativeshell.protocol.Prop
import dev.nativeshell.protocol.PropValue
import dev.nativeshell.protocol.ValueKind
import dev.nativeshell.protocol.ViewType
import dev.nativeshell.util.Json
import kotlin.math.roundToInt

/**
 * Props every view type accepts: decoration, opacity, visibility, transform,
 * animation declarations, accessibility, test ids and press feedback.
 */
internal object CommonProps {

    /** Returns false for props that are not common (the caller logs them). */
    fun set(node: Node, prop: Int, v: PropValue, host: MountHost, animated: Boolean): Boolean {
        val view = node.view
        val d = host.density
        when (prop) {
            Prop.BACKGROUND_COLOR -> box(node).fillColor = if (v.kind == ValueKind.COLOR) v.int else 0
            Prop.CORNER_RADII -> setFour(box(node).radii, v, d)
            Prop.BORDER_WIDTHS -> setFour(box(node).borders, v, d)
            Prop.BORDER_COLOR -> box(node).borderColor = if (v.kind == ValueKind.COLOR) v.int else 0
            Prop.GRADIENT -> box(node).gradient = parseGradient(v.string)
            Prop.OPACITY -> {
                val a = if (v.isNull) 1f else v.number().coerceIn(0f, 1f)
                FrameApplier.setAlpha(node, a, animated)
                return true
            }
            Prop.SHADOW -> { setShadow(node, v, d); return true }
            Prop.CLIPS_TO_BOUNDS -> {
                val clip = !v.isNull && v.bool
                view.clipToOutline = clip
                ensureOutline(node)
                return true
            }
            Prop.HIDDEN -> {
                view.visibility = if (!v.isNull && v.bool) View.INVISIBLE else View.VISIBLE
                return true
            }
            Prop.TRANSFORM -> { setTransform(view, v, d); return true }
            Prop.ANIMATION -> { node.animation = if (v.isNull) null else AnimationSpec.parse(v.string); return true }
            Prop.A11Y_LABEL, Prop.A11Y_ROLE, Prop.A11Y_HINT, Prop.A11Y_VALUE, Prop.A11Y_STATE, Prop.A11Y_HIDDEN ->
                { A11y.set(node, prop, v); return true }
            Prop.TEST_ID -> {
                node.testId = if (v.isNull) null else v.string
                view.setTag(R.id.nsh_test_id, node.testId)
                if (NativeShell.testIdAsContentDescription) A11y.refreshDescription(node)
                return true
            }
            Prop.ACTIVE_OPACITY -> {
                node.activeOpacity = if (v.isNull) Float.NaN else v.number().coerceIn(0f, 1f)
                TouchHandler.sync(node, host)
                return true
            }
            Prop.ACTIVE_BACKGROUND -> {
                node.activeBackground = if (v.kind == ValueKind.COLOR) v.int else null
                TouchHandler.sync(node, host)
                return true
            }
            Prop.HOVER_OPACITY -> {
                node.hoverOpacity = if (v.isNull) Float.NaN else v.number().coerceIn(0f, 1f)
                Pointer.syncHover(node, host)
                Pointer.applyFeedback(node)
                return true
            }
            Prop.HOVER_BACKGROUND -> {
                node.hoverBackground = if (v.kind == ValueKind.COLOR) v.int else null
                Pointer.syncHover(node, host)
                Pointer.applyFeedback(node)
                return true
            }
            Prop.CURSOR -> { Pointer.setCursor(view, if (v.isNull) null else v.intValue()); return true }
            Prop.TOOLTIP -> { Pointer.setTooltip(view, if (v.isNull) null else v.string); return true }
            Prop.HIT_SLOP -> {
                val a = v.floats
                view.setTag(R.id.nsh_hit_slop, if (a != null && a.size >= 4) IntArray(4) { (a[it] * d).roundToInt().coerceAtLeast(0) } else null)
                return true
            }
            Prop.FOCUSABLE -> {
                // Text inputs are always focusable; the prop only matters for plain views.
                if (view is android.widget.EditText) return true
                view.isFocusable = !v.isNull && (v.kind != ValueKind.BOOL || v.bool)
                return true
            }
            else -> return false
        }
        applyBox(node)
        // Images clip to their rounded corners like CSS `img` with border-radius.
        if (node.type == ViewType.IMAGE) view.clipToOutline = node.box?.hasRadii() == true || node.setProps.get(Prop.CLIPS_TO_BOUNDS)
        return true
    }

    private fun box(node: Node): BoxDrawable = node.box ?: BoxDrawable().also { node.box = it }

    private fun setFour(target: FloatArray, v: PropValue, d: Float) {
        val a = v.floats
        for (i in 0..3) target[i] = if (a != null && i < a.size && a[i].isFinite()) (a[i] * d).coerceAtLeast(0f) else 0f
    }

    /**
     * Installs the node's decoration as its background, or restores the view's
     * own background (a widget's ripple) when the decoration is empty.
     */
    fun applyBox(node: Node) {
        val b = node.box ?: return
        val view = node.view
        if (b.isEmpty && !b.hasRadii()) {
            if (view.background === b) view.background = (view.getTag(R.id.nsh_native_background) as? NativeBackground)?.drawable
        } else {
            if (view.background !== b) {
                if (view.getTag(R.id.nsh_native_background) == null) view.setTag(R.id.nsh_native_background, NativeBackground(view.background))
                view.background = b
            }
            b.invalidateSelf()
        }
        ensureOutline(node)
        view.invalidateOutline()
    }

    /** The background a view had before the shell decorated it (may be null). */
    private class NativeBackground(val drawable: android.graphics.drawable.Drawable?)

    private fun ensureOutline(node: Node) {
        val view = node.view
        val box = node.box
        view.outlineProvider = if (box != null && box.hasRadii()) {
            object : ViewOutlineProvider() {
                private val rect = android.graphics.Rect()
                override fun getOutline(v: View, outline: android.graphics.Outline) {
                    rect.set(0, 0, v.width, v.height)
                    box.outlineFor(rect, outline)
                }
            }
        } else if (view.background == null && (view.clipToOutline || view.elevation > 0f)) {
            // Clipping or a shadow on a view without background still needs an outline.
            ViewOutlineProvider.BOUNDS
        } else ViewOutlineProvider.BACKGROUND
    }

    private fun parseGradient(json: String?): Pair<Float, List<Pair<Int, Float>>>? {
        val o = Json.obj(json) ?: return null
        val stops = o.optJSONArray("stops") ?: return null
        val list = ArrayList<Pair<Int, Float>>()
        for (i in 0 until stops.length()) {
            val s = stops.optJSONArray(i) ?: continue
            val c = Json.colorOf(s.opt(0)) ?: continue
            val pos = s.optDouble(1, if (stops.length() > 1) i / (stops.length() - 1.0) else 0.0).toFloat()
            list += c to pos
        }
        if (list.isEmpty()) return null
        return Json.float(o, "angle", 180f) to list
    }

    private fun setShadow(node: Node, v: PropValue, d: Float) {
        val view = node.view
        val o = if (v.isNull) null else Json.obj(v.string)
        if (o == null) {
            ViewCompat.setElevation(view, 0f)
            if (Build.VERSION.SDK_INT >= 28) {
                view.outlineSpotShadowColor = Color.BLACK
                view.outlineAmbientShadowColor = Color.BLACK
            }
            return
        }
        val blur = Json.float(o, "blur", 0f)
        val y = Json.float(o, "y", 0f)
        // Elevation approximates a blurred drop shadow; offsets other than "downward" are not representable.
        ViewCompat.setElevation(view, ((blur / 2f) + y.coerceAtLeast(0f)).coerceAtLeast(0f) * d)
        if (Build.VERSION.SDK_INT >= 28) {
            val c = Json.color(o, "color", Color.BLACK)
            view.outlineSpotShadowColor = c
            view.outlineAmbientShadowColor = c
        }
        ensureOutline(node)
    }

    private fun setTransform(view: View, v: PropValue, d: Float) {
        val o = if (v.isNull) null else Json.obj(v.string)
        view.translationX = Json.float(o, "translate_x", 0f) * d
        view.translationY = Json.float(o, "translate_y", 0f) * d
        val s = Json.float(o, "scale", 1f)
        view.scaleX = Json.float(o, "scale_x", s)
        view.scaleY = Json.float(o, "scale_y", s)
        view.rotation = Json.float(o, "rotate", 0f)
    }
}
