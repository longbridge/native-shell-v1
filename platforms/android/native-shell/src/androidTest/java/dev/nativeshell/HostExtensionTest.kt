package dev.nativeshell

import android.content.Context
import android.util.SizeF
import android.view.View
import android.widget.TextView
import androidx.test.ext.junit.runners.AndroidJUnit4
import dev.nativeshell.protocol.EventMask
import dev.nativeshell.protocol.EventType
import dev.nativeshell.protocol.Prop
import dev.nativeshell.protocol.PropValue
import dev.nativeshell.protocol.ViewType
import dev.nativeshell.views.HostComponentContainer
import org.json.JSONArray
import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertSame
import org.junit.Assert.assertTrue
import org.junit.Assert.fail
import org.junit.Test
import org.junit.runner.RunWith
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit

/** Host modules (sync, async, emit, listener counts) and host components (props, events, measure, pooling). */
@RunWith(AndroidJUnit4::class)
class HostExtensionTest : ActivityTest() {

    private class Counter : HostModule {
        override val name = "test/counter"
        override val methods = listOf(HostMethod("get", MethodKind.SYNC), HostMethod("add", MethodKind.ASYNC), HostMethod("fail", MethodKind.ASYNC))
        override val events = listOf("tick")
        var value = 0
        var listeners = -1
        override fun callSync(method: String, args: JSONArray, context: CallContext): Any? = when (method) {
            "get" -> mapOf("value" to value, "app" to context.appId)
            "__listeners" -> { listeners = args.getInt(1); null }
            else -> throw IllegalArgumentException("no sync method $method")
        }
        override fun call(method: String, args: JSONArray, context: CallContext, responder: Responder) {
            when (method) {
                "add" -> Thread {
                    value += args.getInt(0)
                    responder.resolve(value)
                    responder.resolve(-1) // ignored: answered once
                    context.emit("tick", JSONObject().put("value", value))
                }.start()
                else -> responder.reject("boom")
            }
        }
    }

    @Test
    fun modulesAnswerSyncAsyncAndEmit() {
        val m = Counter()
        NativeShell.registerModule(m)
        val h = harness()
        val cb = h.engine.callbacks
        // The registry the instance was created with lists the module.
        val reg = NativeShell.registryJson().getJSONArray("modules")
        assertTrue((0 until reg.length()).any { reg.getJSONObject(it).getString("name") == "test/counter" })

        assertEquals("""{"app":"test.harness","value":0}""", JSONObject(cb.callSync("test/counter", "get", "[]")).let {
            JSONObject().put("app", it.getString("app")).put("value", it.getInt("value")).toString()
        })
        try {
            cb.callSync("test/counter", "nope", "[]")
            fail("expected a throw")
        } catch (e: IllegalArgumentException) {
            assertEquals("no sync method nope", e.message)
        }
        try {
            cb.callSync("test/missing", "get", "[]")
            fail("expected a throw")
        } catch (e: IllegalStateException) {
            assertTrue(e.message!!.contains("not registered"))
        }
        cb.callSync("test/counter", "__listeners", """["tick", 2]""")
        assertEquals(2, m.listeners)

        cb.callAsync(7, "test/counter", "add", "[5]")
        cb.callAsync(8, "test/counter", "fail", "[]")
        cb.callAsync(9, "test/missing", "x", "[]")
        waitFor(what = "async answers") { synchronized(h.engine.responses) { h.engine.responses.size == 3 } }
        val byId = synchronized(h.engine.responses) { h.engine.responses.associateBy { it.first } }
        assertEquals(Triple(7L, true, "5"), byId[7])
        assertEquals(Triple(8L, false, "boom"), byId[8])
        assertEquals(false, byId[9]!!.second)
        waitFor(what = "context emit") { h.engine.emits.isNotEmpty() }
        assertEquals(Triple("test/counter", "tick", """{"value":5}"""), h.engine.emits.first())

        // Broadcast to every live instance.
        NativeShell.emit("test/counter", "tick", mapOf("value" to 9))
        assertEquals(Triple("test/counter", "tick", """{"value":9}"""), h.engine.emits.last())
        // Destroyed instances drop late answers.
        onMain { h.instance.destroy() }
        val before = h.engine.responses.size
        cb.callAsync(10, "test/counter", "add", "[1]")
        Thread.sleep(200)
        assertEquals(before, h.engine.responses.size)
        NativeShell.unregisterModule("test/counter")
    }

    private class Cell : HostComponent {
        var updates = 0
        var sink: EventSink? = null
        override fun createView(context: Context): View = TextView(context)
        override fun update(view: View, props: JSONObject) {
            updates++
            (view as TextView).text = "${props.optString("symbol")} ${props.optDouble("last", 0.0)}"
        }
        override fun reset(view: View) { (view as TextView).text = "" }
        override fun setEventSink(view: View, sink: EventSink) {
            this.sink = sink
            view.setOnClickListener { sink.emit("select", mapOf("symbol" to (view as TextView).text.split(" ")[0])) }
        }
    }

    @Test
    fun componentsReceivePropsEmitEventsAndMeasure() {
        val created = ArrayList<Cell>()
        NativeShell.registerComponent(HostComponentSpec(
            name = "test-stock-cell",
            events = listOf("select"),
            measure = ComponentMeasurer { props, maxWidthPx -> SizeF(minOf(maxWidthPx, 600f), if (props.optBoolean("compact")) 90f else 150f) },
        ) { Cell().also { created += it } })
        val h = harness()
        val cb = h.engine.callbacks
        val d = onMain { activity().resources.displayMetrics.density }

        val size = cb.measureComponent("test-stock-cell", """{"compact":true}""", 400f, 1)!!
        assertEquals(minOf(400f * d, 600f) / d, size[0], 0.01f)
        assertEquals(90f / d, size[1], 0.01f)
        assertNull(cb.measureComponent("test-unknown", "{}", 400f, 1))

        onMain {
            h.batch {
                create(5, ViewType.HOST_COMPONENT); prop(5, Prop.HOST_COMPONENT, PropValue.str("test-stock-cell"))
                prop(5, Prop.HOST_PROPS, PropValue.json("""{"symbol":"ABC","last":12.5}"""))
                mask(5, EventMask.HOST_EVENT); insert(5, 0, 0); frame(5, 0f, 0f, 300f, 60f)
            }
        }
        val container = onMain { h.view(5) as HostComponentContainer }
        val tv = onMain { container.componentView as TextView }
        assertEquals("ABC 12.5", onMain { tv.text.toString() })
        assertEquals(1, created.single().updates)
        onMain { h.batch { prop(5, Prop.HOST_PROPS, PropValue.json("""{"symbol":"ABC","last":13}""")) } }
        assertEquals("ABC 13.0", onMain { tv.text.toString() })

        onMain { tv.performClick() }
        val e = h.eventsOf(EventType.HOST_EVENT).single()
        assertEquals(5, e.node)
        assertEquals("""{"name":"select","payload":{"symbol":"ABC"}}""", e.payload)

        // Released components are reset and reused for the next node of the same name.
        onMain { h.batch { delete(5) } }
        assertEquals("", onMain { tv.text.toString() })
        onMain {
            h.batch {
                create(6, ViewType.HOST_COMPONENT); prop(6, Prop.HOST_COMPONENT, PropValue.str("test-stock-cell"))
                prop(6, Prop.HOST_PROPS, PropValue.json("""{"symbol":"XYZ"}""")); insert(6, 0, 0)
            }
        }
        assertSame(tv, onMain { (h.view(6) as HostComponentContainer).componentView })
        assertEquals(1, created.size)
        NativeShell.unregisterComponent("test-stock-cell")
    }
}
