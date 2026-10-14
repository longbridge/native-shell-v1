package dev.nativeshell

import android.os.SystemClock
import android.util.Log
import androidx.test.ext.junit.runners.AndroidJUnit4
import dev.nativeshell.text.AttributedText
import dev.nativeshell.text.TextLayouts
import org.junit.Test
import org.junit.runner.RunWith

/** Where the time of one text measurement goes (logged under NSH-PERF). */
@RunWith(AndroidJUnit4::class)
class MeasureBenchTest : ActivityTest() {
    @Test
    fun batchMatchesSingleMeasurement() {
        onMain { TextLayouts.onConfigurationChanged(activity()) }
        val jsons = (0 until 200).map { i -> """{"runs":[{"text":"${"Line $i ".repeat(1 + i % 7)}中文 😀","size":${12 + i % 9},"weight":400,"color":4278848010${if (i % 3 == 0) ",\"line_height\":24" else ""}}]}""" }
        val widths = FloatArray(jsons.size) { if (it % 2 == 0) Float.POSITIVE_INFINITY else 120f }
        val modes = IntArray(jsons.size) { if (it % 2 == 0) TextLayouts.UNDEFINED else TextLayouts.AT_MOST }
        TextLayouts.clear()
        val batch = TextLayouts.measureBatch(jsons.toTypedArray(), widths, modes)
        TextLayouts.clear()
        for (i in jsons.indices) {
            val one = TextLayouts.measure(jsons[i], widths[i], modes[i])
            for (k in 0..2) org.junit.Assert.assertEquals("item $i[$k]", one[k], batch[i * 3 + k], 0f)
        }
    }

    @Test
    fun measureCost() {
        onMain { TextLayouts.onConfigurationChanged(activity()) }
        val dm = activity().resources.displayMetrics
        for (round in 0 until 3) {
            val jsons = (0 until 500).map { """{"runs":[{"text":"Row $it · $round$round","size":16,"weight":400,"color":4278848010}]}""" }
            var t = SystemClock.elapsedRealtimeNanos()
            jsons.forEach { org.json.JSONObject(it) }
            val json = (SystemClock.elapsedRealtimeNanos() - t) / 1e6
            t = SystemClock.elapsedRealtimeNanos()
            jsons.forEach { AttributedText.sp(16f, dm) }
            val sp = (SystemClock.elapsedRealtimeNanos() - t) / 1e6
            t = SystemClock.elapsedRealtimeNanos()
            jsons.forEach { AttributedText.parse(it, dm) }
            val parse = (SystemClock.elapsedRealtimeNanos() - t) / 1e6
            TextLayouts.clear()
            t = SystemClock.elapsedRealtimeNanos()
            jsons.forEach { TextLayouts.measure(it, Float.POSITIVE_INFINITY, TextLayouts.UNDEFINED) }
            val undefined = (SystemClock.elapsedRealtimeNanos() - t) / 1e6
            TextLayouts.clear()
            t = SystemClock.elapsedRealtimeNanos()
            jsons.forEach { TextLayouts.measure(it, 300f, TextLayouts.AT_MOST) }
            val atMost = (SystemClock.elapsedRealtimeNanos() - t) / 1e6
            TextLayouts.clear()
            t = SystemClock.elapsedRealtimeNanos()
            TextLayouts.measureBatch(jsons.toTypedArray(), FloatArray(jsons.size) { Float.POSITIVE_INFINITY }, IntArray(jsons.size))
            val batch = (SystemClock.elapsedRealtimeNanos() - t) / 1e6
            Log.i("NSH-PERF", "measure x500 round=$round batch(parallel)=${"%.1f".format(batch)}ms json=${"%.1f".format(json)}ms sp=${"%.1f".format(sp)}ms parse=${"%.1f".format(parse)}ms undefined=${"%.1f".format(undefined)}ms atMost=${"%.1f".format(atMost)}ms")
        }
    }
}
