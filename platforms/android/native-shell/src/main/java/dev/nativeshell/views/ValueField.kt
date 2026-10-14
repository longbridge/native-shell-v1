package dev.nativeshell.views

import android.annotation.SuppressLint
import android.content.Context
import android.content.res.ColorStateList
import android.graphics.Canvas
import android.graphics.Paint
import android.graphics.Path
import android.graphics.drawable.GradientDrawable
import android.graphics.drawable.RippleDrawable
import android.text.TextPaint
import android.text.TextUtils
import android.text.format.DateFormat
import android.util.TypedValue
import android.view.View
import dev.nativeshell.render.ThemeTokens
import org.json.JSONObject
import java.util.Calendar
import kotlin.math.ceil

/**
 * The quiet value field Select and DatePicker show: the value (or a muted
 * placeholder) on a subtle muted fill with a small trailing chevron, like the
 * iOS compact picker. [ValueFieldMetrics] measures exactly what it draws.
 */
@SuppressLint("ViewConstructor")
internal class ValueField(context: Context) : View(context) {
    var value: String? = null
        set(v) { field = v; contentDescription = v ?: placeholder; requestLayout(); invalidate() }
    var placeholder: String? = null
        set(v) { field = v; if (value == null) contentDescription = v; requestLayout(); invalidate() }

    private val m = ValueFieldMetrics(context)
    private var tokens = ThemeTokens.of(context)
    private val textPaint = m.textPaint()
    private val chevronPaint = Paint(Paint.ANTI_ALIAS_FLAG).apply {
        style = Paint.Style.STROKE
        strokeWidth = 1.75f * m.dp
        strokeCap = Paint.Cap.ROUND
        strokeJoin = Paint.Join.ROUND
        color = tokens.mutedForeground
    }
    private val chevron = Path()

    init {
        isClickable = true
        isFocusable = true
        applyTokens()
    }

    /** (Re)applies the surface's theme tokens: fill, ripple and text colors. */
    fun applyTokens() {
        tokens = ThemeTokens.of(context)
        chevronPaint.color = tokens.mutedForeground
        val shape = GradientDrawable().apply { setColor(tokens.muted); cornerRadius = ValueFieldMetrics.RADIUS * m.dp }
        val mask = GradientDrawable().apply { setColor(0xFF000000.toInt()); cornerRadius = ValueFieldMetrics.RADIUS * m.dp }
        // Keep a script-set decoration (BoxDrawable) in place.
        if (background == null || background is RippleDrawable) {
            background = RippleDrawable(ColorStateList.valueOf(tokens.foreground and 0x1FFFFFFF), shape, mask)
        }
        invalidate()
    }

    override fun onMeasure(widthMeasureSpec: Int, heightMeasureSpec: Int) {
        val (w, h) = m.sizePx(value ?: placeholder ?: "")
        setMeasuredDimension(resolveSize(ceil(w).toInt(), widthMeasureSpec), resolveSize(ceil(h).toInt(), heightMeasureSpec))
    }

    override fun onDraw(canvas: Canvas) {
        val text = value ?: placeholder ?: ""
        textPaint.color = if (value != null) tokens.foreground else tokens.mutedForeground
        val pad = ValueFieldMetrics.PAD_X * m.dp
        val chevronW = ValueFieldMetrics.CHEVRON * m.dp
        val gap = ValueFieldMetrics.GAP * m.dp
        val avail = (width - pad * 2 - chevronW - gap).coerceAtLeast(0f)
        val shown = TextUtils.ellipsize(text, textPaint, avail, TextUtils.TruncateAt.END)
        val fm = textPaint.fontMetrics
        val baseline = height / 2f - (fm.ascent + fm.descent) / 2f
        canvas.drawText(shown, 0, shown.length, pad, baseline, textPaint)
        // Down chevron, vertically centered at the trailing edge.
        val cx = width - pad - chevronW / 2f
        val cy = height / 2f
        val s = chevronW / 2f
        chevron.reset()
        chevron.moveTo(cx - s * 0.8f, cy - s * 0.35f)
        chevron.lineTo(cx, cy + s * 0.45f)
        chevron.lineTo(cx + s * 0.8f, cy - s * 0.35f)
        canvas.drawPath(chevron, chevronPaint)
    }
}

/** Geometry shared by [ValueField] and the core's `nsh/select` / `nsh/date-picker` measurements. */
internal class ValueFieldMetrics(private val context: Context) {
    val dp = context.resources.displayMetrics.density

    fun textPaint() = TextPaint(Paint.ANTI_ALIAS_FLAG).apply {
        textSize = TypedValue.applyDimension(TypedValue.COMPLEX_UNIT_SP, TEXT_SP, context.resources.displayMetrics)
    }

    /** Width and height in px of a field showing [text]. */
    fun sizePx(text: String): Pair<Float, Float> {
        val p = textPaint()
        val fm = p.fontMetrics
        val w = p.measureText(text) + (PAD_X * 2 + GAP + CHEVRON) * dp
        val h = maxOf((fm.descent - fm.ascent) + PAD_Y * 2 * dp, MIN_HEIGHT * dp)
        return w to h
    }

    /** Localized display text of a `Date` value: medium date, short time (system 12/24 h), or both. */
    fun displayDate(value: String, mode: Int): String? {
        if (value.isEmpty()) return null
        val c = DatePickerManager.parse(value)
        val date = java.text.DateFormat.getDateInstance(java.text.DateFormat.MEDIUM).format(c.time)
        val time = DateFormat.getTimeFormat(context).format(c.time)
        return when (mode) {
            1 -> time
            2 -> "$date, $time"
            else -> date
        }
    }

    /** `measure_component` for the reserved built-in names; sizes in px, or null for other names. */
    fun measureBuiltIn(name: String, props: JSONObject): Pair<Float, Float>? = when (name) {
        "nsh/select" -> sizePx(props.optString("label").ifEmpty { props.optString("placeholder") })
        "nsh/date-picker" -> {
            val mode = when (props.optString("mode")) { "time" -> 1; "datetime" -> 2; else -> 0 }
            sizePx(displayDate(props.optString("value"), mode) ?: sampleDate(mode))
        }
        else -> null
    }

    /** An empty picker is as wide as a typical value, so it does not jump when one is picked. */
    private fun sampleDate(mode: Int): String {
        val c = Calendar.getInstance().apply { set(2026, Calendar.SEPTEMBER, 28, 23, 59) }
        return displayDate(DatePickerManager.format(c, mode), mode) ?: ""
    }

    companion object {
        const val TEXT_SP = 16f
        const val PAD_X = 12f
        const val PAD_Y = 8f
        const val GAP = 8f
        const val CHEVRON = 12f
        const val MIN_HEIGHT = 40f
        const val RADIUS = 6f
    }
}
