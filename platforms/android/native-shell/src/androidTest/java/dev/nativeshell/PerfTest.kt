package dev.nativeshell

import android.os.SystemClock
import android.util.Log
import androidx.test.ext.junit.runners.AndroidJUnit4
import dev.nativeshell.protocol.BatchFlags
import dev.nativeshell.protocol.EventType
import dev.nativeshell.protocol.Prop
import dev.nativeshell.protocol.PropValue
import dev.nativeshell.protocol.ViewType
import dev.nativeshell.testing.BatchBuilder
import org.junit.Assume.assumeTrue
import org.junit.Test
import org.junit.runner.RunWith
import java.io.File

/**
 * First-frame and update timings at 1k / 3k / 10k native nodes, printed to
 * logcat under `NSH-PERF`. Renderer-only numbers use pre-encoded batches; the
 * end-to-end numbers run the real core.
 */
@RunWith(AndroidJUnit4::class)
class PerfTest : ActivityTest() {
    private val sizes = listOf(1_000, 3_000, 10_000)

    private fun text(i: Int, tick: Int) = """{"runs":[{"text":"Row $i · $tick","size":14,"color":4278190080}]}"""

    @Test
    fun rendererOnly() {
        for (n in sizes) {
            val h = harness()
            val rows = n / 2 // each row: a painted view + its text
            val first = BatchBuilder(1, BatchFlags.INITIAL).apply {
                create(1, ViewType.VIEW); insert(1, 0, 0); frame(1, 0f, 0f, 400f, rows * 10f)
                for (r in 0 until rows) {
                    val v = 10 + r * 2
                    create(v, ViewType.VIEW); insert(v, 1, r); frame(v, 0f, r * 10f, 400f, 10f)
                    prop(v, Prop.BACKGROUND_COLOR, PropValue.color(if (r % 2 == 0) 0xFFEEEEEE.toInt() else 0xFFFFFFFF.toInt()))
                    create(v + 1, ViewType.TEXT); insert(v + 1, v, 0); frame(v + 1, 4f, 0f, 200f, 10f)
                    prop(v + 1, Prop.ATTRIBUTED_TEXT, PropValue.json(text(r, 0)))
                }
            }.bytes()
            val update = BatchBuilder(2, 0).apply {
                for (r in 0 until rows) prop(10 + r * 2 + 1, Prop.ATTRIBUTED_TEXT, PropValue.json(text(r, 1)))
            }.bytes()
            val t0 = SystemClock.elapsedRealtimeNanos()
            onMain { h.applyBytes(first) }
            val firstMs = (SystemClock.elapsedRealtimeNanos() - t0) / 1e6
            instrumentation.waitForIdleSync()
            val drawMs = (SystemClock.elapsedRealtimeNanos() - t0) / 1e6
            val t1 = SystemClock.elapsedRealtimeNanos()
            onMain { h.applyBytes(update) }
            val updateMs = (SystemClock.elapsedRealtimeNanos() - t1) / 1e6
            val decodeMs = onMain { h.surface.renderer!!.lastDecodeMs }
            Log.i("NSH-PERF", "renderer n=$n batchBytes=${first.size} first-apply=${"%.1f".format(firstMs)}ms first-frame(idle)=${"%.1f".format(drawMs)}ms " +
                "update(${rows} texts)=${"%.1f".format(updateMs)}ms (decode ${"%.1f".format(decodeMs)}ms)")
            onMain { h.instance.destroy() }
        }
    }

    private fun metrics(inst: ShellInstance): org.json.JSONObject? =
        inst.metrics()?.takeIf { it.isNotEmpty() }?.let { runCatching { org.json.JSONObject(it) }.getOrNull() }

    @Test
    fun endToEnd() {
        assumeTrue(NativeShell.isNativeAvailable)
        for (n in sizes) {
            val rows = n / 2
            val program = """
                import { View, div, v_flex } from "native-shell";
                export default class Perf extends View {
                  init(props, cx) { this.tick = 0; }
                  render(cx) {
                    const rows = [];
                    for (let i = 0; i < $rows; i++) {
                      rows.push(div().h(10).bg(i % 2 ? "muted" : "background").child(`Row ${'$'}{i} · ${'$'}{this.tick}`));
                    }
                    return v_flex().test_id("root").size_full().on_click((_e, cx) => { this.tick++; cx.notify(); }).children(rows);
                  }
                }
            """.trimIndent()
            val surface = onMain { ShellSurfaceView(activity()).also { activity().show(it) } }
            val t0 = SystemClock.elapsedRealtimeNanos()
            val inst = onMain {
                val i = ShellInstance.create(activity(), ShellConfig().appId("perf.$n").dataDir(File(activity().cacheDir, "perf$n")))
                surface.attach(i)
                i.loadSources("""{"id":"perf.n$n","name":"Perf"}""", mapOf("main.js" to program))
                i
            }
            waitFor(60_000, "first batch n=$n") { surface.renderer?.awaitingFirstBatch == false }
            val firstMs = (SystemClock.elapsedRealtimeNanos() - t0) / 1e6
            val applyFirst = onMain { surface.renderer!!.lastApplyMs }
            val natives = onMain { surface.renderer!!.mounting.nodeCount }
            // Update: each click re-renders every row's text. Several clicks, core counters diffed per click.
            val rootId = onMain { surface.renderer!!.mounting.nodeForTestId("root")!!.id }
            val m0 = metrics(inst)
            Log.i("NSH-PERF", "e2e n=$n boot_ns=${m0?.optLong("boot_ns")} prelude_bytecode=${m0?.optBoolean("prelude_bytecode")} " +
                "first-batch bytes=${m0?.optLong("batch_bytes")} load→first-frame=${"%.1f".format(firstMs)}ms (apply ${"%.1f".format(applyFirst)}ms) natives=$natives")
            val clicks = 5
            var wall = 0.0
            var apply = 0.0
            var prev = m0
            val sums = LinkedHashMap<String, Double>()
            for (c in 0 until clicks) {
                val count = onMain { surface.renderer!!.mounting.batchesApplied }
                val t1 = SystemClock.elapsedRealtimeNanos()
                onMain { surface.renderer!!.dispatch(rootId, EventType.CLICK, """{"x":1,"y":1,"count":1}""") }
                waitFor(60_000, "update batch n=$n") { surface.renderer!!.mounting.batchesApplied > count }
                wall += (SystemClock.elapsedRealtimeNanos() - t1) / 1e6
                val tl = inst.timeline
                fun ms(a: Long, b: Long) = if (a == 0L || b == 0L) "?" else "%.1f".format((b - a) / 1e6)
                Log.i("NSH-PERF", "e2e n=$n click $c hops: dispatch→batchReady=${ms(tl.dispatched, tl.batchReady)}ms " +
                    "batchReady→drain=${ms(tl.batchReady, tl.drainStart)}ms drain→taken=${ms(tl.drainStart, tl.taken)}ms " +
                    "taken→applied=${ms(tl.taken, tl.applied)}ms")
                apply += onMain { surface.renderer!!.lastApplyMs }
                Thread.sleep(100) // let the core finish its tick bookkeeping
                val m = metrics(inst)
                if (prev != null && m != null) {
                    for (k in listOf("tick_ns", "render_ns", "convert_ns", "commit_ns", "encode_ns", "measure_text_ns", "measure_text_calls", "measure_text_batches", "measure_component_ns", "measure_component_calls", "dispatch_ns", "queue_wait_ns", "handoff_ns", "batch_bytes")) {
                        val v = if (k == "batch_bytes") m.optLong(k).toDouble() else (m.optLong(k) - prev.optLong(k)).toDouble()
                        sums[k] = (sums[k] ?: 0.0) + v
                    }
                }
                prev = m
            }
            val per = sums.entries.joinToString(" ") { (k, v) ->
                val avg = v / clicks
                if (k.endsWith("_ns")) "${k.removeSuffix("_ns")}=${"%.1f".format(avg / 1e6)}ms" else "$k=${"%.0f".format(avg)}"
            }
            Log.i("NSH-PERF", "e2e n=$n per click (avg of $clicks): wall=${"%.1f".format(wall / clicks)}ms main-apply=${"%.1f".format(apply / clicks)}ms core: ${per.ifEmpty { "metrics unavailable" }}")
            Log.i("NSH-PERF", "e2e n=$n metrics: ${prev ?: "unavailable"}")
            onMain { inst.destroy() }
        }
    }
}
