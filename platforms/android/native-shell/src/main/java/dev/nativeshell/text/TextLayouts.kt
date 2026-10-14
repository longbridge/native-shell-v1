package dev.nativeshell.text

import android.content.Context
import android.graphics.Paint
import android.os.Build
import android.text.BoringLayout
import android.text.Layout
import android.text.TextDirectionHeuristics
import android.text.StaticLayout
import android.text.TextPaint
import android.text.TextUtils
import android.util.DisplayMetrics
import android.util.LruCache
import kotlin.math.ceil
import kotlin.math.floor
import kotlin.math.min
import kotlin.math.roundToInt

/**
 * Text measurement and the layouts text views draw.
 *
 * The core measures text during layout on the JS thread with the exact
 * `AttributedText` JSON the renderer later receives. The measured
 * [StaticLayout] is cached under `(json, width in px)`; the text view draws the
 * very same object when its frame has that width, so what is measured is what
 * is drawn. Every layout owns its paint: layouts built on the JS thread are
 * drawn on the main thread.
 */
internal object TextLayouts {
    /** Width modes, as in `WidthMode::to_raw`. */
    const val UNDEFINED = 0
    const val AT_MOST = 1
    const val EXACTLY = 2

    @Volatile private var metrics: DisplayMetrics = DisplayMetrics().apply { setToDefaults() }

    private data class MeasureKey(val json: String, val mode: Int, val width: Float)
    private data class LayoutKey(val json: String, val widthPx: Int)

    private val parsed = LruCache<String, AttributedText>(512)
    private val measured = LruCache<MeasureKey, FloatArray>(2048)
    private val layouts = LruCache<LayoutKey, Layout>(512)

    @Volatile var hits = 0
        private set
    @Volatile var misses = 0
        private set

    /** The context whose configuration (font scale, locale) measurement follows. */
    @Volatile var context: Context? = null
        private set

    fun init(context: Context) {
        metrics = context.applicationContext.resources.displayMetrics
        if (this.context == null) this.context = context.applicationContext
    }

    /**
     * Font scale, density or locale changed: everything measured is stale.
     * [context] is the one whose configuration applies (the surface's activity).
     */
    fun onConfigurationChanged(context: Context) {
        this.context = context
        metrics = DisplayMetrics().apply { setTo(context.resources.displayMetrics) }
        clear()
    }

    fun clear() {
        parsed.evictAll()
        measured.evictAll()
        layouts.evictAll()
    }

    val density: Float get() = metrics.density

    fun attributed(json: String): AttributedText =
        parsed.get(json) ?: AttributedText.parse(json, metrics).also { parsed.put(json, it) }

    /** `[width, height, baseline]` in points. */
    fun measure(json: String, width: Float, mode: Int): FloatArray {
        val key = MeasureKey(json, mode, width)
        measured.get(key)?.let { hits++; return it }
        misses++
        val d = metrics.density
        val text = attributed(json)
        val paint = basePaint(text)
        val boring = boringMetrics(text, paint)
        var available = if (mode == UNDEFINED || !width.isFinite()) Int.MAX_VALUE else floor(width * d + 1e-3f).toInt().coerceAtLeast(0)
        // AtMost(0) is the min-content probe: the widest unbreakable segment.
        if (mode == AT_MOST && available == 0) available = minContentWidth(text, paint)
        val layoutWidth: Int
        if (mode == EXACTLY) {
            layoutWidth = available
        } else {
            // Intrinsic width: the longest line unwrapped, capped by the available width.
            // A single-line "boring" text gets it from the one shaping pass BoringLayout needs.
            val desired = boring?.width ?: ceil(Layout.getDesiredWidth(text.text, paint)).toInt()
            layoutWidth = min(desired, available)
        }
        var layout = build(text, layoutWidth, paint, boring)
        var usedWidth = layoutWidth
        var widest = 0f
        if (mode != EXACTLY) {
            // Wrapped lines may be narrower than the cap: shrink-wrap to the widest one.
            for (i in 0 until layout.lineCount) widest = maxOf(widest, layout.getLineWidth(i))
            val tight = ceil(widest).toInt().coerceAtMost(layoutWidth)
            // Greedy breaking keeps the same lines at any width >= the widest line, so a
            // start-aligned layout draws identically at the tighter width: rebuild only
            // when alignment depends on the width.
            if (tight < layoutWidth && (text.alignment != Layout.Alignment.ALIGN_NORMAL || text.justify || text.ellipsize)) {
                layout = build(text, tight, basePaint(text), boring)
            }
            usedWidth = tight
        }
        layouts.put(LayoutKey(json, usedWidth), layout)
        val result = floatArrayOf(
            // Unrounded widest line (GPUI rule): the core snaps it up to the pixel grid,
            // which is the width the layout is cached under.
            if (mode == EXACTLY) width else minOf(widest, usedWidth.toFloat()) / d,
            layout.height / d,
            (if (layout.lineCount > 0) layout.getLineBaseline(0) else 0) / d,
        )
        measured.put(key, result)
        return result
    }

    /** Width in px of the widest segment line breaking cannot split (a word; a grapheme for CJK). */
    private fun minContentWidth(text: AttributedText, paint: TextPaint): Int {
        val s = text.text
        val it = java.text.BreakIterator.getLineInstance()
        it.setText(s.toString())
        var widest = 0f
        var start = it.first()
        var end = it.next()
        while (end != java.text.BreakIterator.DONE) {
            var e = end
            while (e > start && Character.isWhitespace(s[e - 1])) e--
            if (e > start) widest = maxOf(widest, Layout.getDesiredWidth(s, start, e, paint))
            start = end
            end = it.next()
        }
        return ceil(widest).toInt()
    }

    private val pool: java.util.concurrent.ExecutorService by lazy {
        val n = minOf(4, Runtime.getRuntime().availableProcessors()).coerceAtLeast(1)
        java.util.concurrent.Executors.newFixedThreadPool(n) { r -> Thread(r, "native-shell-measure").apply { isDaemon = true } }
    }

    /**
     * [measure] for many texts, results concatenated (3 floats each). Shaping is
     * thread-safe here (every layout has its own paint; caches are synchronized),
     * so large batches are split across a small pool.
     */
    fun measureBatch(jsons: Array<String>, widths: FloatArray, modes: IntArray): FloatArray {
        val n = jsons.size
        val out = FloatArray(n * 3)
        fun run(from: Int, to: Int) {
            for (i in from until to) {
                val r = measure(jsons[i], widths.getOrElse(i) { Float.POSITIVE_INFINITY }, modes.getOrElse(i) { UNDEFINED })
                out[i * 3] = r[0]; out[i * 3 + 1] = r[1]; out[i * 3 + 2] = r[2]
            }
        }
        val threads = minOf(4, Runtime.getRuntime().availableProcessors())
        if (n < 64 || threads < 2) {
            run(0, n)
            return out
        }
        val chunk = (n + threads - 1) / threads
        // The calling (JS) thread takes the first chunk itself.
        val futures = (1 until threads).mapNotNull { t ->
            val from = t * chunk
            val to = minOf(n, from + chunk)
            if (from < to) pool.submit { run(from, to) } else null
        }
        run(0, minOf(n, chunk))
        futures.forEach { it.get() }
        return out
    }

    /**
     * The layout to draw at a frame width (points). Usually the one measurement
     * cached; otherwise built now (e.g. after a cache trim).
     */
    fun layoutFor(json: String, widthPt: Float): Layout {
        val widthPx = (widthPt * metrics.density).roundToInt().coerceAtLeast(0)
        val key = LayoutKey(json, widthPx)
        layouts.get(key)?.let { return it }
        val text = attributed(json)
        val paint = basePaint(text)
        return build(text, widthPx, paint, boringMetrics(text, paint)).also { layouts.put(key, it) }
    }

    fun basePaint(text: AttributedText): TextPaint = TextPaint(Paint.ANTI_ALIAS_FLAG).apply {
        density = metrics.density
        textSize = text.baseSizePx
        typeface = text.baseTypeface
        color = AttributedText.DEFAULT_COLOR
    }

    /**
     * Metrics when the text is one line without paragraph spans (no line height,
     * no hard breaks, no bidi), so it can use a BoringLayout. API 33+ only: that is
     * where BoringLayout honors fallback line spacing like StaticLayout and TextView.
     */
    fun boringMetrics(text: AttributedText, paint: TextPaint): BoringLayout.Metrics? {
        if (Build.VERSION.SDK_INT < 33 || text.justify) return null
        return BoringLayout.isBoring(text.text, paint, TextDirectionHeuristics.FIRSTSTRONG_LTR, true, null)
    }

    fun build(text: AttributedText, width: Int, paint: TextPaint, boring: BoringLayout.Metrics?): Layout {
        if (boring != null && boring.width <= width && Build.VERSION.SDK_INT >= 33) {
            return BoringLayout.make(text.text, paint, width.coerceAtLeast(0), text.alignment, boring, false, null, width, true)
        }
        val b = StaticLayout.Builder.obtain(text.text, 0, text.text.length, paint, width.coerceAtLeast(0))
            .setAlignment(text.alignment)
            .setIncludePad(false)
            .setLineSpacing(0f, 1f)
            .setBreakStrategy(Layout.BREAK_STRATEGY_SIMPLE)
            .setHyphenationFrequency(Layout.HYPHENATION_FREQUENCY_NONE)
        if (text.maxLines > 0) {
            b.setMaxLines(text.maxLines)
            if (text.ellipsize) b.setEllipsize(TextUtils.TruncateAt.END).setEllipsizedWidth(width.coerceAtLeast(0))
        }
        if (text.justify && Build.VERSION.SDK_INT >= 26) b.setJustificationMode(Layout.JUSTIFICATION_MODE_INTER_WORD)
        if (Build.VERSION.SDK_INT >= 28) b.setUseLineSpacingFromFallbacks(true)
        return b.build()
    }
}
