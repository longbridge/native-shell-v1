package dev.nativeshell.text

import android.graphics.Paint
import android.graphics.Typeface
import android.os.Build
import android.text.Layout
import android.text.SpannableStringBuilder
import android.text.Spanned
import android.text.TextPaint
import android.text.style.ClickableSpan
import android.text.style.ForegroundColorSpan
import android.text.style.LineHeightSpan
import android.text.style.MetricAffectingSpan
import android.text.style.StrikethroughSpan
import android.text.style.UnderlineSpan
import android.util.DisplayMetrics
import android.util.TypedValue
import android.view.View
import dev.nativeshell.util.Json
import org.json.JSONObject
import kotlin.math.ceil
import kotlin.math.roundToInt

/**
 * `Prop::AttributedText` JSON turned into what Android lays out: a spanned
 * string plus paragraph settings. Sizes are scaled-pixel values (`sp`), so the
 * user's font scale (including Android 14's non-linear scaling) applies.
 */
internal class AttributedText(
    val text: CharSequence,
    val alignment: Layout.Alignment,
    val justify: Boolean,
    /** 0 = unlimited. */
    val maxLines: Int,
    val ellipsize: Boolean,
    val hasLinks: Boolean,
    /** Size of the first run in px: the base paint size (empty text still has a line). */
    val baseSizePx: Float,
    val baseTypeface: Typeface,
) {
    companion object {
        const val DEFAULT_SIZE = 16f
        const val DEFAULT_COLOR = 0xFF0A0A0A.toInt()

        fun parse(json: String, metrics: DisplayMetrics): AttributedText {
            val o = Json.obj(json) ?: JSONObject()
            val runs = o.optJSONArray("runs")
            val sb = SpannableStringBuilder()
            var maxLineHeight = 0
            var hasLinks = false
            var baseSize = Float.NaN
            var baseTypeface: Typeface? = null
            if (runs != null) {
                for (i in 0 until runs.length()) {
                    val r = runs.optJSONObject(i) ?: continue
                    val text = r.optString("text", "")
                    if (text.isEmpty() && i > 0) continue
                    val start = sb.length
                    sb.append(text)
                    val end = sb.length
                    val sizePx = sp(Json.float(r, "size", DEFAULT_SIZE).coerceIn(1f, 1000f), metrics)
                    val weight = Json.int(r, "weight", 400).coerceIn(1, 1000)
                    val italic = Json.bool(r, "italic", false)
                    val typeface = typeface(Json.string(r, "family"), weight, italic)
                    val letterSpacing = Json.float(r, "letter_spacing", 0f) * metrics.density
                    if (baseSize.isNaN()) { baseSize = sizePx; baseTypeface = typeface }
                    if (end > start) {
                        sb.setSpan(RunSpan(sizePx, typeface, weight, italic, letterSpacing), start, end, Spanned.SPAN_EXCLUSIVE_EXCLUSIVE)
                        sb.setSpan(ForegroundColorSpan(Json.color(r, "color", DEFAULT_COLOR)), start, end, Spanned.SPAN_EXCLUSIVE_EXCLUSIVE)
                        if (Json.bool(r, "underline", false)) sb.setSpan(UnderlineSpan(), start, end, Spanned.SPAN_EXCLUSIVE_EXCLUSIVE)
                        if (Json.bool(r, "strike", false)) sb.setSpan(StrikethroughSpan(), start, end, Spanned.SPAN_EXCLUSIVE_EXCLUSIVE)
                        Json.string(r, "url")?.let {
                            hasLinks = true
                            sb.setSpan(LinkSpan(it), start, end, Spanned.SPAN_EXCLUSIVE_EXCLUSIVE)
                        }
                    }
                    val lh = lineHeightPx(r, metrics)
                    if (lh > maxLineHeight) maxLineHeight = lh
                }
            }
            if (baseSize.isNaN()) baseSize = sp(DEFAULT_SIZE, metrics)
            if (maxLineHeight > 0) {
                // Wrap in at least one char so the paragraph span applies to empty text too.
                sb.setSpan(ExactLineHeightSpan(maxLineHeight), 0, sb.length, Spanned.SPAN_INCLUSIVE_INCLUSIVE)
            }
            val alignName = Json.string(o, "align")
            val alignment = when (alignName) {
                "center" -> Layout.Alignment.ALIGN_CENTER
                "right", "end" -> Layout.Alignment.ALIGN_OPPOSITE
                else -> Layout.Alignment.ALIGN_NORMAL
            }
            val truncate = Json.bool(o, "truncate", false)
            var lines = Json.int(o, "lines", 0).coerceAtLeast(0)
            if (truncate && lines == 0) lines = 1
            return AttributedText(
                sb, alignment, alignName == "justify", lines, truncate, hasLinks, baseSize,
                baseTypeface ?: Typeface.DEFAULT,
            )
        }

        /** Scaled pixels; uses the display's font scale (non-linear on Android 14). */
        fun sp(size: Float, metrics: DisplayMetrics): Float =
            TypedValue.applyDimension(TypedValue.COMPLEX_UNIT_SP, size, metrics)

        /** `line_height` in points (the core resolves multiples), scaled like the font; 0 = natural. */
        private fun lineHeightPx(r: JSONObject, metrics: DisplayMetrics): Int {
            val pt = Json.float(r, "line_height", 0f)
            val px = sp(pt, metrics)
            return if (px.isFinite() && px > 0f) ceil(px).toInt() else 0
        }

        private val typefaceCache = HashMap<String, Typeface>()

        fun typeface(family: String?, weight: Int, italic: Boolean): Typeface = synchronized(typefaceCache) {
            typefaceCache.getOrPut("$family|$weight|$italic") {
                val base = when (family) {
                    null, "", "system", "sans-serif", "system-ui" -> Typeface.SANS_SERIF
                    "serif" -> Typeface.SERIF
                    "monospace", "mono" -> Typeface.MONOSPACE
                    else -> Typeface.create(family, Typeface.NORMAL)
                }
                if (Build.VERSION.SDK_INT >= 28) {
                    Typeface.create(base, weight, italic)
                } else {
                    val style = (if (weight >= 600) Typeface.BOLD else 0) or (if (italic) Typeface.ITALIC else 0)
                    Typeface.create(base, style)
                }
            }
        }
    }
}

/** Size, face and tracking of one run. */
internal class RunSpan(
    private val sizePx: Float,
    private val typeface: Typeface,
    private val weight: Int,
    private val italic: Boolean,
    private val letterSpacingPx: Float,
) : MetricAffectingSpan() {
    override fun updateMeasureState(paint: TextPaint) = apply(paint)
    override fun updateDrawState(tp: TextPaint) = apply(tp)

    private fun apply(p: TextPaint) {
        p.textSize = sizePx
        p.typeface = typeface
        if (Build.VERSION.SDK_INT < 28) {
            // Without variable weights, synthesize bold/italic the font lacks.
            val want = (if (weight >= 600) Typeface.BOLD else 0) or (if (italic) Typeface.ITALIC else 0)
            val missing = want and typeface.style.inv()
            p.isFakeBoldText = missing and Typeface.BOLD != 0
            p.textSkewX = if (missing and Typeface.ITALIC != 0) -0.25f else 0f
        }
        p.letterSpacing = if (sizePx > 0f) letterSpacingPx / sizePx else 0f
    }
}

/**
 * A fixed line height with the extra space split evenly above and below the
 * glyphs (CSS half-leading), so measurement and drawing agree on every line.
 */
internal class ExactLineHeightSpan(private val heightPx: Int) : LineHeightSpan {
    override fun chooseHeight(text: CharSequence, start: Int, end: Int, spanstartv: Int, lineHeight: Int, fm: Paint.FontMetricsInt) {
        val current = fm.descent - fm.ascent
        if (current <= 0) return
        val extra = heightPx - current
        val top = (extra / 2f).roundToInt()
        fm.ascent -= top
        fm.descent += extra - top
        fm.top = fm.ascent
        fm.bottom = fm.descent
    }
}

/** Receives taps on link runs (the text node view). */
internal interface LinkHandler {
    fun onLink(url: String)
}

/** A link run. Drawing comes from the run's own color/underline spans. */
internal class LinkSpan(val url: String) : ClickableSpan() {
    override fun onClick(widget: View) {
        var v: Any? = widget
        while (v is View) {
            if (v is LinkHandler) return v.onLink(url)
            v = v.parent
        }
    }

    override fun updateDrawState(ds: TextPaint) {}
}
