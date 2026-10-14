package dev.nativeshell.views

import android.annotation.SuppressLint
import android.content.Context
import android.graphics.Canvas
import android.graphics.Matrix
import android.graphics.Paint
import android.graphics.Path
import android.view.View
import android.view.ViewGroup
import dev.nativeshell.protocol.Prop
import dev.nativeshell.protocol.PropValue
import dev.nativeshell.protocol.ValueKind
import dev.nativeshell.protocol.ViewType
import dev.nativeshell.render.MountHost
import dev.nativeshell.render.Node
import dev.nativeshell.render.ViewManager
import kotlin.math.min

/** Draws SVG path data scaled from its view box into the view (`xMidYMid meet`). */
@SuppressLint("ViewConstructor")
internal class ShapeView(context: Context) : View(context) {
    var pathData: String? = null
        set(value) {
            field = value
            path = value?.let { SvgPath.toPath(it) }?.also { it.fillType = fillType }
            invalidate()
        }
    var fillType: Path.FillType = Path.FillType.WINDING
        set(value) {
            field = value
            path?.fillType = value
            invalidate()
        }
    var viewBox: FloatArray? = null
        set(value) { field = value; invalidate() }
    var fill: Int = 0
        set(value) { field = value; invalidate() }
    var stroke: Int = 0
        set(value) { field = value; invalidate() }
    /** In view-box units, like SVG's stroke-width. */
    var strokeWidth: Float = 1f
        set(value) { field = value; invalidate() }

    private var path: Path? = null
    private val paint = Paint(Paint.ANTI_ALIAS_FLAG)
    private val matrix = Matrix()

    override fun onDraw(canvas: Canvas) {
        val p = path ?: return
        val vb = viewBox
        canvas.save()
        if (vb != null && vb.size >= 4 && vb[2] > 0f && vb[3] > 0f) {
            val s = min(width / vb[2], height / vb[3])
            matrix.reset()
            matrix.setTranslate(-vb[0], -vb[1])
            matrix.postScale(s, s)
            matrix.postTranslate((width - vb[2] * s) / 2f, (height - vb[3] * s) / 2f)
            canvas.concat(matrix)
        } else {
            // Without a view box, path coordinates are points.
            val d = resources.displayMetrics.density
            canvas.scale(d, d)
        }
        if (fill ushr 24 != 0) {
            paint.style = Paint.Style.FILL
            paint.color = fill
            canvas.drawPath(p, paint)
        }
        if (stroke ushr 24 != 0 && strokeWidth > 0f) {
            paint.style = Paint.Style.STROKE
            paint.strokeWidth = strokeWidth
            paint.strokeCap = Paint.Cap.ROUND
            paint.strokeJoin = Paint.Join.ROUND
            paint.color = stroke
            canvas.drawPath(p, paint)
        }
        canvas.restore()
    }
}

internal class ShapeManager : ViewManager(ViewType.SHAPE) {
    override fun createView(host: MountHost): View = ShapeView(host.context)

    override fun container(node: Node): ViewGroup? = null

    override fun setProp(node: Node, prop: Int, value: PropValue, host: MountHost): Boolean {
        val v = node.view as ShapeView
        when (prop) {
            Prop.PATH_DATA -> v.pathData = if (value.isNull) null else value.string
            Prop.FILL_COLOR -> v.fill = if (value.kind == ValueKind.COLOR) value.int else 0
            Prop.STROKE_COLOR -> v.stroke = if (value.kind == ValueKind.COLOR) value.int else 0
            Prop.STROKE_WIDTH -> v.strokeWidth = if (value.isNull) 1f else value.number()
            Prop.VIEW_BOX -> v.viewBox = if (value.isNull) null else value.floats
            Prop.FILL_RULE -> v.fillType = if (!value.isNull && value.intValue() == 1) Path.FillType.EVEN_ODD else Path.FillType.WINDING
            else -> return false
        }
        return true
    }
}
