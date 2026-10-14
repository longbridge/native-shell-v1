package dev.nativeshell

import android.content.Context
import android.util.Log
import android.util.SizeF
import android.view.View
import android.view.ViewGroup
import android.view.inputmethod.EditorInfo
import android.widget.TextView
import androidx.test.ext.junit.runners.AndroidJUnit4
import dev.nativeshell.views.ShellEditText
import dev.nativeshell.views.ShellTextView
import org.json.JSONArray
import org.json.JSONObject
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Assume.assumeTrue
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith
import java.io.File

/** The real Rust core (`libnative_shell.so`) running the repository's example programs. */
@RunWith(AndroidJUnit4::class)
class NativeE2ETest : ActivityTest() {
    private var instance: ShellInstance? = null
    private lateinit var surface: ShellSurfaceView
    private val errors = ArrayList<String>()

    @Before
    fun requireNative() {
        assumeTrue("libnative_shell.so not packaged", NativeShell.isNativeAvailable)
        NativeShell.testIdAsContentDescription = true
        NativeShell.traceBatches = true
    }

    @After
    fun tearDown() {
        onMain { instance?.destroy() }
        NativeShell.testIdAsContentDescription = false
        NativeShell.traceBatches = false
    }

    private val testAssets: Context get() = instrumentation.context

    private fun start(program: String): ShellInstance {
        val dir = File(instrumentation.targetContext.cacheDir, "e2e-$program-${System.nanoTime()}")
        val inst = onMain {
            val i = ShellInstance.create(activity(), ShellConfig().appId("e2e.$program").dataDir(dir))
            i.errorListener = ShellInstance.ErrorListener { synchronized(errors) { errors += it } }
            i.logListener = ShellInstance.LogListener { level, msg -> Log.println(if (level >= LogLevel.ERROR) Log.ERROR else Log.INFO, "NSH-E2E", msg) }
            surface = ShellSurfaceView(activity())
            activity().show(surface)
            surface.attach(i)
            i.eventObserver = { node, type, payload -> events += "$node:$type:$payload" }
            i.loadSourcesFromAssets(testAssets, program)
            i
        }
        instance = inst
        return inst
    }

    private fun byTestId(id: String, timeoutMs: Long = 10_000): View {
        var v: View? = null
        waitFor(timeoutMs, "test id `$id` (errors: $errors)") { surface.findByTestId(id)?.also { v = it } != null }
        return v!!
    }

    /** All text drawn in a subtree, in view order. */
    private fun textOf(v: View): String = when (v) {
        is ShellTextView -> v.text.toString()
        is TextView -> v.text.toString()
        is ViewGroup -> (0 until v.childCount).joinToString(" ") { textOf(v.getChildAt(it)) }.trim()
        else -> ""
    }

    private fun waitText(id: String, expected: String, timeoutMs: Long = 5000) {
        waitFor(timeoutMs, "`$id` to read `$expected` (got `${onMain { surface.findByTestId(id)?.let(::textOf) }}`, errors: $errors)") {
            surface.findByTestId(id)?.let(::textOf) == expected
        }
    }

    private fun type(id: String, text: String) = onMain {
        val f = surface.findByTestId(id) as ShellEditText
        f.requestFocus()
        f.onCreateInputConnection(EditorInfo())!!.commitText(text, 1)
    }

    @Test
    fun tasks() = explain {
        start("tasks")
        byTestId("draft")
        waitFor(what = "first frame") { surface.renderer?.awaitingFirstBatch == false }
        type("draft", "Buy milk")
        waitFor(what = "add enabled") { surface.findByTestId("add")?.hasOnClickListeners() == true }
        onMain { byTestIdNow("add").performClick() }
        waitFor(what = "task row") { surface.findByTestId("tasks")?.let(::textOf)?.contains("Buy milk") == true }
        type("draft", "Walk the dog")
        waitFor(what = "add enabled again") { surface.findByTestId("add")?.hasOnClickListeners() == true }
        onMain { byTestIdNow("add").performClick() }
        waitFor(what = "second row") { surface.findByTestId("tasks")?.let(::textOf)?.contains("Walk the dog") == true }
        // The draft was cleared through the controlled-input protocol.
        waitFor(what = "draft cleared") { (surface.findByTestId("draft") as ShellEditText).text.isNullOrEmpty() }
        assertTrue(errors.toString(), errors.isEmpty())
    }

    /** The native tree with test ids and texts, for failure messages. */
    private fun dump(): String = onMain {
        val sb = StringBuilder()
        fun walk(v: View, depth: Int) {
            val tid = v.getTag(R.id.nsh_test_id)
            val t = when (v) { is ShellTextView -> " \"${v.text}\""; is TextView -> " \"${v.text}\""; else -> "" }
            sb.append("  ".repeat(depth)).append(v.javaClass.simpleName).append(if (tid != null) " #$tid" else "")
                .append(" ${v.width}x${v.height}").append(t).append('\n')
            if (v is ViewGroup) for (i in 0 until v.childCount) walk(v.getChildAt(i), depth + 1)
        }
        walk(surface, 0)
        sb.toString()
    }

    private fun <T> explain(block: () -> T): T = try {
        block()
    } catch (e: AssertionError) {
        throw AssertionError("${e.message}\nevents: $events\nviews:\n${dump()}\nnodes:\n${onMain { surface.renderer?.mounting?.dump() }}", e)
    }

    private val events = java.util.Collections.synchronizedList(ArrayList<String>())

    private fun byTestIdNow(id: String): View = surface.findByTestId(id) ?: throw AssertionError("no `$id`")

    @Test
    fun components() = explain {
        start("components")
        byTestId("index")
        // Open the Buttons page (a pushed native page), compose a post and publish it.
        onMain { byTestIdNow("open-buttons").performClick() }
        waitFor(what = "buttons page") { surface.pageCount == 1 && surface.findByTestId("post") != null }
        type("post", "Hello from the emulator")
        waitFor(what = "publish enabled") { surface.findByTestId("publish")?.hasOnClickListeners() == true }
        onMain { byTestIdNow("publish").performClick() }
        waitFor(10_000, "published") { surface.findByTestId("published") != null }
        // System back pops the page.
        onMain { activity().onBackPressedDispatcher.onBackPressed() }
        waitFor(what = "back to the index") { surface.pageCount == 0 }
        assertTrue(errors.toString(), errors.isEmpty())
    }

    /** Registers stand-ins for whatever the program's `host.json` declares, then runs it. */
    @Test
    fun watchlist() = explain {
        val program = "watchlist"
        val host = JSONObject(testAssets.assets.open("$program/host.json").bufferedReader().readText())
        val modules = host.getJSONArray("modules")
        val registered = ArrayList<String>()
        for (i in 0 until modules.length()) {
            val d = modules.getJSONObject(i)
            val name = d.getString("name")
            val methods = d.getJSONArray("methods")
            NativeShell.registerModule(object : HostModule {
                override val name = name
                override val methods = (0 until methods.length()).map {
                    val m = methods.getJSONObject(it)
                    HostMethod(m.getString("name"), if (m.optString("kind") == "sync") MethodKind.SYNC else MethodKind.ASYNC)
                }
                override val events = (0 until d.getJSONArray("events").length()).map { d.getJSONArray("events").getString(it) }
                override val requiresPermission = d.optBoolean("requires_permission", true)
                override fun callSync(method: String, args: JSONArray, context: CallContext): Any? = when (method) {
                    "__listeners" -> null
                    else -> mapOf("id" to "u1", "name" to "Test User")
                }
                // Quote per examples/watchlist/host.json: {symbol, last, prev_close, change_percent}.
                override fun call(method: String, args: JSONArray, context: CallContext, responder: Responder) {
                    val symbol = args.opt(0) as? String ?: "X"
                    responder.resolve(if (method == "quote") mapOf("symbol" to symbol, "last" to 101.0, "prev_close" to 100.0, "change_percent" to 1.0) else null)
                    if (method == "subscribe") {
                        val first = (args.opt(0) as? JSONArray)?.optString(0) ?: return
                        context.emit("tick", mapOf("symbol" to first, "last" to 99.0, "prev_close" to 100.0, "change_percent" to -1.0))
                    }
                }
            })
            registered += name
        }
        val comps = host.getJSONArray("components")
        for (i in 0 until comps.length()) {
            val name = comps.getJSONObject(i).getString("name")
            val requires = comps.getJSONObject(i).optBoolean("requires_permission", true)
            NativeShell.registerComponent(HostComponentSpec(name, listOf("select"), ComponentMeasurer { _, w -> SizeF(w, 56f * 2.625f) }, requires) {
                object : HostComponent {
                    override fun createView(context: Context): View = TextView(context)
                    override fun update(view: View, props: JSONObject) {
                        (view as TextView).text = "${props.optString("symbol")} ${props.optString("name")} ${props.opt("last")} ${props.opt("change_percent")}"
                    }
                }
            })
            registered += name
        }
        try {
            start(program)
            waitFor(10_000, "host component rows (errors: $errors)") {
                var found = false
                fun walk(v: View) {
                    if (v is dev.nativeshell.views.HostComponentContainer && v.componentView != null) found = true
                    if (v is ViewGroup) for (c in 0 until v.childCount) walk(v.getChildAt(c))
                }
                walk(surface)
                found
            }
            assertTrue(errors.toString(), errors.isEmpty())
        } finally {
            registered.forEach { NativeShell.unregisterModule(it); NativeShell.unregisterComponent(it) }
        }
    }

    @Test
    fun themeProviderOverridesTokens() = explain {
        val primary = 0xFF2563EB.toInt()
        NativeShell.themeProvider = ThemeProvider { dark -> mapOf("primary" to if (dark) 0xFF60A5FA.toInt() else primary) }
        try {
            val program = """
                import { View, div } from "native-shell";
                export default class T extends View {
                  render(cx) { return div().test_id("box").size_full().bg("primary"); }
                }
            """.trimIndent()
            onMain {
                val i = ShellInstance.create(activity(), ShellConfig().appId("e2e.theme").dataDir(File(activity().cacheDir, "theme")))
                i.errorListener = ShellInstance.ErrorListener { synchronized(errors) { errors += it } }
                instance = i
                surface = ShellSurfaceView(activity())
                activity().show(surface)
                surface.attach(i)
                i.loadSources("""{"id":"e2e.theme","name":"Theme"}""", mapOf("main.js" to program))
            }
            val box = byTestId("box")
            val dark = onMain { activity().resources.configuration.uiMode and android.content.res.Configuration.UI_MODE_NIGHT_MASK == android.content.res.Configuration.UI_MODE_NIGHT_YES }
            val fill = onMain { (box.background as dev.nativeshell.render.BoxDrawable).fillColor }
            assertEquals(Integer.toHexString(if (dark) 0xFF60A5FA.toInt() else primary), Integer.toHexString(fill))
            assertTrue(errors.toString(), errors.isEmpty())
        } finally {
            NativeShell.themeProvider = null
        }
    }

    @Test
    fun clipboardReadIsSync() {
        onMain {
            val cm = activity().getSystemService(android.content.ClipboardManager::class.java)
            cm.setPrimaryClip(android.content.ClipData.newPlainText("t", "copied text"))
        }
        val inst = onMain { ShellInstance.create(activity(), ShellConfig().appId("e2e.clip").dataDir(File(activity().cacheDir, "clip"))) }
        instance = inst
        // The core's host callbacks object, invoked as the JS thread would.
        val cb = ShellInstance::class.java.getDeclaredField("callbacks").apply { isAccessible = true }.get(inst) as HostCallbacks
        assertEquals("\"copied text\"", cb.callSync("nsh/clipboard", "read", "[]"))
    }

    @Test
    fun abiAndProtocolVersionMatch() {
        assertEquals(dev.nativeshell.protocol.Protocol.VERSION, NativeBridge.protocolVersion())
        Log.i("NSH-E2E", "abi hash ${NativeShell.abiHash}")
    }
}
