package dev.nativeshell.example

import android.content.Context
import android.content.res.Configuration
import android.graphics.Canvas
import android.graphics.Paint
import android.graphics.Typeface
import android.util.TypedValue
import android.view.View
import dev.nativeshell.EventSink
import dev.nativeshell.HostComponent
import org.json.JSONObject
import kotlin.math.abs
import kotlin.math.ceil
import kotlin.math.max

/**
 * The example app's own colors. The shell's theme takes its accent from here
 * ([DemoApplication]), and the native stock cell draws with the rest, so both
 * sides of the screen come from one palette in either appearance.
 */
object AppPalette {
    fun accent(dark: Boolean) = if (dark) 0xFF60A5FA.toInt() else 0xFF2563EB.toInt()
    fun foreground(dark: Boolean) = if (dark) 0xFFFAFAFA.toInt() else 0xFF0A0A0A.toInt()
    fun mutedForeground(dark: Boolean) = if (dark) 0xFFA3A3A3.toInt() else 0xFF737373.toInt()
    fun pressed(dark: Boolean) = if (dark) 0xFF262626.toInt() else 0xFFF5F5F5.toInt()
    fun gain(dark: Boolean) = if (dark) 0xFF4ADE80.toInt() else 0xFF15803D.toInt()
    fun loss(dark: Boolean) = if (dark) 0xFFF87171.toInt() else 0xFFDC2626.toInt()

    fun isDark(context: Context) =
        (context.resources.configuration.uiMode and Configuration.UI_MODE_NIGHT_MASK) == Configuration.UI_MODE_NIGHT_YES
}

/**
 * The app's own quote row, as an existing native view would be: symbol and
 * name lead, price and the day's change trail, right-aligned with tabular
 * digits. The change carries its sign, so gain and loss never depend on color
 * alone. Text sizes are in sp, so the row follows the user's font scale.
 */
class StockCellView(context: Context) : View(context) {
    var symbol = ""
    var name = ""
    var last: Double? = null
    var change: Double? = null

    private val dp = context.resources.displayMetrics.density
    private val title = paint(context, TITLE_SP, Typeface.create(Typeface.DEFAULT, Typeface.BOLD))
    private val detail = paint(context, DETAIL_SP, Typeface.DEFAULT)
    private val price = paint(context, TITLE_SP, Typeface.create("sans-serif-medium", Typeface.NORMAL)).apply {
        textAlign = Paint.Align.RIGHT
        fontFeatureSettings = "tnum"
    }
    private val delta = paint(context, DETAIL_SP, Typeface.create("sans-serif-medium", Typeface.NORMAL)).apply {
        textAlign = Paint.Align.RIGHT
        fontFeatureSettings = "tnum"
    }

    init {
        isClickable = true
        isFocusable = true
    }

    override fun drawableStateChanged() {
        super.drawableStateChanged()
        invalidate()
    }

    override fun onDraw(canvas: Canvas) {
        val dark = AppPalette.isDark(context)
        if (isPressed) canvas.drawColor(AppPalette.pressed(dark))
        val inset = INSET_DP * dp
        val gap = LINE_GAP_DP * dp
        val block = lineHeight(title) + gap + lineHeight(detail)
        val top = (height - block) / 2f
        val firstBaseline = top - title.fontMetrics.ascent
        val secondBaseline = top + lineHeight(title) + gap - detail.fontMetrics.ascent
        val right = width - inset

        title.color = AppPalette.foreground(dark)
        detail.color = AppPalette.mutedForeground(dark)
        price.color = AppPalette.foreground(dark)
        val c = change
        delta.color = when {
            c == null -> AppPalette.mutedForeground(dark)
            c >= 0 -> AppPalette.gain(dark)
            else -> AppPalette.loss(dark)
        }
        val priceText = last?.let { "%.2f".format(it) } ?: "—"
        val deltaText = c?.let { "%s%.2f%%".format(if (it >= 0) "+" else "−", abs(it)) } ?: "—"
        val trailing = max(price.measureText(priceText), delta.measureText(deltaText))
        val leadingWidth = right - trailing - 12 * dp - inset

        canvas.drawText(ellipsize(symbol, title, leadingWidth), inset, firstBaseline, title)
        canvas.drawText(ellipsize(name, detail, leadingWidth), inset, secondBaseline, detail)
        canvas.drawText(priceText, right, firstBaseline, price)
        canvas.drawText(deltaText, right, secondBaseline, delta)
    }

    private fun ellipsize(text: String, paint: Paint, width: Float): String {
        if (paint.measureText(text) <= width) return text
        var end = text.length
        while (end > 0 && paint.measureText(text, 0, end) + paint.measureText("…") > width) end--
        return text.substring(0, end) + "…"
    }

    companion object {
        private const val TITLE_SP = 17f
        private const val DETAIL_SP = 14f
        private const val INSET_DP = 16f
        private const val LINE_GAP_DP = 2f
        private const val PADDING_DP = 12f
        private const val MIN_HEIGHT_DP = 64f

        private fun paint(context: Context, sp: Float, face: Typeface) = Paint(Paint.ANTI_ALIAS_FLAG).apply {
            textSize = TypedValue.applyDimension(TypedValue.COMPLEX_UNIT_SP, sp, context.resources.displayMetrics)
            typeface = face
        }

        private fun lineHeight(p: Paint) = p.fontMetrics.let { it.descent - it.ascent }

        /** Two lines of text and their padding, never below the 64 dp row the list was designed around. */
        fun heightPx(context: Context): Float {
            val dp = context.resources.displayMetrics.density
            val text = lineHeight(paint(context, TITLE_SP, Typeface.DEFAULT_BOLD)) + LINE_GAP_DP * dp + lineHeight(paint(context, DETAIL_SP, Typeface.DEFAULT))
            return ceil(max(MIN_HEIGHT_DP * dp, text + 2 * PADDING_DP * dp))
        }
    }
}

/** Adapter between the shell and [StockCellView]: one per pooled view. */
class StockCell : HostComponent {
    override fun createView(context: Context): View = StockCellView(context)

    override fun update(view: View, props: JSONObject) {
        val v = view as StockCellView
        v.symbol = props.optString("symbol")
        v.name = props.optString("name")
        v.last = if (props.isNull("last")) null else props.optDouble("last")
        v.change = if (props.isNull("change_percent")) null else props.optDouble("change_percent")
        v.contentDescription = listOfNotNull(
            v.symbol,
            v.name.ifEmpty { null },
            v.last?.let { "%.2f".format(it) },
            v.change?.let { "%s %.2f percent".format(if (it >= 0) "up" else "down", abs(it)) },
        ).joinToString(", ")
        v.invalidate()
    }

    override fun reset(view: View) {
        update(view, JSONObject())
        view.setOnClickListener(null)
    }

    override fun setEventSink(view: View, sink: EventSink) {
        view.setOnClickListener { sink.emit("select", JSONObject().put("symbol", (view as StockCellView).symbol)) }
    }
}
