package dev.nativeshell.render

import android.graphics.Canvas
import android.graphics.ColorFilter
import android.graphics.LinearGradient
import android.graphics.Outline
import android.graphics.Paint
import android.graphics.Path
import android.graphics.PixelFormat
import android.graphics.Rect
import android.graphics.RectF
import android.graphics.Shader
import android.graphics.drawable.Drawable
import android.os.Build
import kotlin.math.abs
import kotlin.math.cos
import kotlin.math.min
import kotlin.math.sin

/**
 * Background of a node: fill color or linear gradient, per-corner radii and
 * per-side borders in one color, all in pixels.
 *
 * Borders are the ring between the outer rounded rect and the inner one inset
 * by each side's width (radii shrunk accordingly), filled even-odd: that is how
 * CSS draws borders of unequal widths with rounded corners, with one path and
 * no per-side seams.
 */
class BoxDrawable : Drawable() {
    var fillColor: Int = 0
    /** `angle` in degrees (CSS: 0 points up, 90 right) and `[argb, position]` stops. */
    var gradient: Pair<Float, List<Pair<Int, Float>>>? = null
    /** `[tl, tr, br, bl]`. */
    val radii = FloatArray(4)
    /** `[top, right, bottom, left]`. */
    val borders = FloatArray(4)
    var borderColor: Int = 0

    /** Temporary fill while pressed (`ActiveBackground`), or null. */
    var pressedFill: Int? = null

    private val fillPaint = Paint(Paint.ANTI_ALIAS_FLAG)
    private val borderPaint = Paint(Paint.ANTI_ALIAS_FLAG)
    private val outer = Path()
    private val ring = Path()
    private val rect = RectF()
    private var shaderBounds = Rect()
    private var shaderKey: Any? = null

    val isEmpty: Boolean
        get() = fillColor == 0 && gradient == null && pressedFill == null && !hasBorder

    val hasBorder: Boolean get() = borderColor ushr 24 != 0 && borders.any { it > 0f }

    fun hasRadii(): Boolean = radii.any { it > 0f }

    override fun draw(canvas: Canvas) {
        val b = bounds
        if (b.isEmpty) return
        rect.set(b)
        buildRoundRect(outer, rect, radii)
        val fill = pressedFill ?: fillColor
        val g = gradient
        if (g != null && pressedFill == null) {
            fillPaint.shader = shaderFor(b, g)
            fillPaint.color = 0xFF000000.toInt()
            canvas.drawPath(outer, fillPaint)
        } else if (fill ushr 24 != 0) {
            fillPaint.shader = null
            fillPaint.color = fill
            canvas.drawPath(outer, fillPaint)
        }
        if (hasBorder) {
            ring.reset()
            ring.fillType = Path.FillType.EVEN_ODD
            ring.addPath(outer)
            val inner = RectF(rect.left + borders[3], rect.top + borders[0], rect.right - borders[1], rect.bottom - borders[2])
            if (inner.width() > 0 && inner.height() > 0) {
                // CSS: each inner corner is the clamped outer corner minus the adjacent
                // border widths, per axis (elliptical when the sides differ).
                val c = clamp(radii, rect.width(), rect.height())
                val l = borders[3]; val t = borders[0]; val r = borders[1]; val bt = borders[2]
                val innerRadii = floatArrayOf(
                    c[0] - l, c[0] - t, c[1] - r, c[1] - t, c[2] - r, c[2] - bt, c[3] - l, c[3] - bt,
                ).map { it.coerceAtLeast(0f) }.toFloatArray()
                val innerPath = Path()
                innerPath.addRoundRect(inner, innerRadii, Path.Direction.CW)
                ring.addPath(innerPath)
            }
            borderPaint.color = borderColor
            canvas.drawPath(ring, borderPaint)
        }
    }

    private fun shaderFor(b: Rect, g: Pair<Float, List<Pair<Int, Float>>>): Shader {
        val cached = fillPaint.shader
        if (cached != null && shaderKey === g && shaderBounds == b) return cached
        shaderKey = g
        shaderBounds = Rect(b)
        // CSS gradient line: through the center, length so the corners hit 0% and 100%.
        val rad = Math.toRadians(g.first.toDouble())
        val dx = sin(rad).toFloat()
        val dy = -cos(rad).toFloat()
        val half = (abs(b.width() * dx) + abs(b.height() * dy)) / 2f
        val cx = b.exactCenterX()
        val cy = b.exactCenterY()
        val stops = g.second.ifEmpty { listOf(0 to 0f, 0 to 1f) }
        val colors = IntArray(stops.size) { stops[it].first }
        val positions = FloatArray(stops.size) { stops[it].second.coerceIn(0f, 1f) }
        return LinearGradient(cx - dx * half, cy - dy * half, cx + dx * half, cy + dy * half, colors, positions, Shader.TileMode.CLAMP)
    }

    override fun getOutline(outline: Outline) = outlineFor(bounds, outline)

    /** The rounded outline for [b], without touching this drawable's bounds. */
    fun outlineFor(b: Rect, outline: Outline) {
        if (!hasRadii()) {
            outline.setRect(b)
        } else if (radii.all { it == radii[0] }) {
            outline.setRoundRect(b, min(radii[0], min(b.width(), b.height()) / 2f))
        } else {
            val p = Path()
            buildRoundRect(p, RectF(b), radii)
            if (Build.VERSION.SDK_INT >= 30) outline.setPath(p) else @Suppress("DEPRECATION") outline.setConvexPath(p)
        }
        outline.alpha = if (fillColor ushr 24 == 0xFF || gradient != null) 1f else 0.99f
    }

    override fun setAlpha(alpha: Int) {}
    override fun setColorFilter(colorFilter: ColorFilter?) {}

    @Deprecated("Deprecated in Java")
    override fun getOpacity(): Int = PixelFormat.TRANSLUCENT

    companion object {
        /**
         * CSS radius clamping: when adjacent radii exceed a side, all four scale by the
         * same factor, so `rounded_full` on a square is a circle. Fill, border, outline,
         * clip and shadow all use these clamped radii.
         */
        fun clamp(radii: FloatArray, w: Float, h: Float): FloatArray {
            val (tl, tr, br, bl) = radii.toList()
            var f = 1f
            if (tl + tr > w) f = min(f, w / (tl + tr))
            if (bl + br > w) f = min(f, w / (bl + br))
            if (tl + bl > h) f = min(f, h / (tl + bl))
            if (tr + br > h) f = min(f, h / (tr + br))
            return floatArrayOf(tl * f, tr * f, br * f, bl * f)
        }

        fun buildRoundRect(path: Path, r: RectF, radii: FloatArray) {
            path.reset()
            val (tl, tr, br, bl) = clamp(radii, r.width(), r.height()).toList()
            path.addRoundRect(r, floatArrayOf(tl, tl, tr, tr, br, br, bl, bl), Path.Direction.CW)
        }
    }
}
