package dev.nativeshell

import android.content.res.Configuration
import android.util.Log
import androidx.test.ext.junit.runners.AndroidJUnit4
import dev.nativeshell.protocol.Prop
import dev.nativeshell.protocol.PropValue
import dev.nativeshell.protocol.ViewType
import dev.nativeshell.text.TextLayouts
import dev.nativeshell.views.ShellTextView
import org.junit.After
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import kotlin.math.abs
import kotlin.math.roundToInt

/**
 * Text measured on the "JS thread" path must be exactly what text views draw:
 * the measured size vs the drawn layout (and the selectable TextView's layout)
 * within 1 px, across scripts, line heights and font scales.
 */
@RunWith(AndroidJUnit4::class)
class TextGoldenTest : ActivityTest() {

    private val samples = linkedMapOf(
        "latin" to "The quick brown fox jumps over the lazy dog, again and again until it wraps.",
        "cjk" to "敏捷的棕色狐狸跳过了懒狗。日本語のテキストも折り返します。한국어 문장도 있습니다.",
        "emoji" to "Party 🎉🎉 with 👩‍👩‍👧‍👦 family and 🇯🇵 flags 😀😃😄😁",
        "mixed" to "Price 价格 ¥1,234.50 — مرحبا (RTL) and ελληνικά plus 😀 end.",
    )
    private val lineHeights = listOf(null, 1.0f, 1.3f, 2.0f)
    private val fontScales = listOf(1.0f, 1.3f, 2.0f)

    private fun json(text: String, size: Float, lineHeight: Float?, weight: Int = 400): String {
        val run = org.json.JSONObject().put("text", text).put("size", size.toDouble()).put("weight", weight).put("color", 0xFF111111L)
        if (lineHeight != null) run.put("line_height", (size * lineHeight).toDouble())
        return org.json.JSONObject().put("runs", org.json.JSONArray().put(run)).toString()
    }

    @After
    fun restoreMetrics() {
        onMain { TextLayouts.onConfigurationChanged(activity()) }
    }

    @Test
    fun measuredMatchesDrawnWithinOnePixel() {
        val h = harness()
        val report = StringBuilder()
        var failures = 0
        var cases = 0
        var node = 10
        for (scale in fontScales) {
            onMain {
                val cfg = Configuration(activity().resources.configuration).apply { fontScale = scale }
                TextLayouts.onConfigurationChanged(activity().createConfigurationContext(cfg))
            }
            for ((name, text) in samples) {
                for (lh in lineHeights) {
                    for (width in listOf(300f, 120f)) {
                        cases++
                        val j = json(text, 16f, lh, if (name == "latin") 600 else 400)
                        val id = ++node
                        val result = onMain {
                            val d = activity().resources.displayMetrics.density
                            // As the core does: snap the width up to the pixel grid and re-measure there.
                            val raw = TextLayouts.measure(j, width, TextLayouts.AT_MOST)
                            val snapped = kotlin.math.ceil(raw[0] * d - 1e-3f) / d
                            val m = TextLayouts.measure(j, snapped, TextLayouts.AT_MOST).let { floatArrayOf(snapped, it[1]) }
                            h.batch {
                                create(id, ViewType.TEXT); insert(id, 0, 0)
                                prop(id, Prop.ATTRIBUTED_TEXT, PropValue.json(j))
                                frame(id, 0f, 0f, m[0], m[1])
                            }
                            val v = h.view(id) as ShellTextView
                            val drawn = v.currentLayout()!!
                            val drawnW = (m[0] * d).roundToInt()
                            val dh = abs(drawn.height - m[1] * d)
                            val dw = abs(drawn.width - m[0] * d)
                            // The selectable path must lay out the same lines at the same height.
                            h.batch { prop(id, Prop.SELECTABLE, PropValue.bool(true)) }
                            val tv = v.getChildAt(0) as android.widget.TextView
                            dev.nativeshell.render.ShellViewGroup.measureExactly(tv, drawnW, (m[1] * d).roundToInt())
                            tv.layout(0, 0, drawnW, (m[1] * d).roundToInt())
                            val tvLayout = tv.layout
                            val sh = abs(tvLayout.height - m[1] * d)
                            val sameLines = tvLayout.lineCount == drawn.lineCount
                            h.batch { delete(id) }
                            arrayOf(m[0], m[1], drawn.height.toFloat(), dh, dw, tvLayout.height.toFloat(), sh, if (sameLines) 1f else 0f, drawn.lineCount.toFloat())
                        }
                        val ok = result[3] <= 1f && result[4] <= 1f && result[6] <= 1f && result[7] == 1f
                        if (!ok) failures++
                        val line = "scale=$scale script=$name lh=${lh ?: "normal"} maxw=$width measured=${result[0]}x${result[1]}pt " +
                            "lines=${result[8].toInt()} drawnH=${result[2]}px Δh=${"%.2f".format(result[3])} Δw=${"%.2f".format(result[4])} " +
                            "selectableH=${result[5]}px Δsel=${"%.2f".format(result[6])} sameLines=${result[7] == 1f} ${if (ok) "OK" else "FAIL"}"
                        Log.i("NSH-GOLDEN", line)
                        report.appendLine(line)
                    }
                }
            }
        }
        Log.i("NSH-GOLDEN", "cases=$cases failures=$failures")
        assertTrue("text golden failures: $failures of $cases\n$report", failures == 0)
    }

    /** Re-measuring at a returned width never re-wraps (the core lays out again at that width). */
    @Test
    fun remeasuringAtTheReturnedWidthIsStable() {
        val report = StringBuilder()
        var failures = 0
        onMain {
            TextLayouts.onConfigurationChanged(activity())
            for ((name, text) in samples) for (lh in lineHeights) {
                val j = json(text, 16f, lh)
                val d = activity().resources.displayMetrics.density
                val snap = { w: Float -> kotlin.math.ceil(w * d - 1e-3f) / d }
                val free = TextLayouts.measure(j, Float.POSITIVE_INFINITY, TextLayouts.UNDEFINED)
                val again = TextLayouts.measure(j, snap(free[0]), TextLayouts.AT_MOST)
                val exact = TextLayouts.measure(j, snap(free[0]), TextLayouts.EXACTLY)
                val lines = { w: Float -> TextLayouts.layoutFor(j, w).lineCount }
                val narrow = TextLayouts.measure(j, 150f, TextLayouts.AT_MOST)
                val narrowAgain = TextLayouts.measure(j, snap(narrow[0]), TextLayouts.AT_MOST)
                val minContent = TextLayouts.measure(j, 0f, TextLayouts.AT_MOST)
                val ok = kotlin.math.abs(again[0] - free[0]) < 0.01f && again[1] == free[1] && exact[1] == free[1] &&
                    lines(snap(free[0])) == lines(snap(again[0])) && narrow[0] <= 150f &&
                    kotlin.math.abs(narrowAgain[0] - narrow[0]) < 0.01f && narrowAgain[1] == narrow[1] &&
                    minContent[0] > 0f && minContent[0] <= narrow[0]
                if (!ok) failures++
                report.appendLine("$name lh=${lh ?: "normal"} undefined=${free.toList()} atMost(W)=${again.toList()} exactly(W)=${exact.toList()} " +
                    "atMost(150)=${narrow.toList()} again=${narrowAgain.toList()} minContent=${minContent[0]} ${if (ok) "OK" else "FAIL"}")
            }
        }
        android.util.Log.i("NSH-GOLDEN", report.toString())
        assertTrue("re-measure failures: $failures\n$report", failures == 0)
    }

    @Test
    fun emptyTextHasOneLineAndNoWidth() {
        onMain {
            val m = TextLayouts.measure(json("", 16f, null), 200f, TextLayouts.AT_MOST)
            assertTrue(m[0] == 0f && m[1] > 0f)
        }
    }

    @Test
    fun cacheHitsOnRepeatAndClearsOnConfigurationChange() {
        onMain {
            val j = json("cache me", 14f, null)
            TextLayouts.clear()
            val misses = TextLayouts.misses
            TextLayouts.measure(j, 100f, TextLayouts.AT_MOST)
            TextLayouts.measure(j, 100f, TextLayouts.AT_MOST)
            assertTrue(TextLayouts.misses == misses + 1)
            TextLayouts.onConfigurationChanged(activity())
            TextLayouts.measure(j, 100f, TextLayouts.AT_MOST)
            assertTrue(TextLayouts.misses == misses + 2)
        }
    }
}
