package dev.nativeshell.testing

import android.content.Context
import android.os.Looper
import dev.nativeshell.Engine
import dev.nativeshell.HostCallbacks
import dev.nativeshell.ShellConfig
import dev.nativeshell.ShellInstance
import dev.nativeshell.ShellSurfaceView
import dev.nativeshell.protocol.BatchEncoder
import dev.nativeshell.protocol.Mutation
import java.nio.ByteBuffer
import java.nio.ByteOrder

/** An event the renderer sent to the "script". */
data class SentEvent(val node: Int, val type: Int, val payload: String, val seq: Int)

/**
 * Stands in for the native engine: batches are encoded by the test with
 * [BatchEncoder] and handed out like the core would; everything the renderer
 * sends back is recorded.
 */
internal class FakeEngine(val callbacks: HostCallbacks) : Engine {
    private val batches = ArrayDeque<ByteArray>()
    val events = ArrayList<SentEvent>()
    val environments = ArrayList<String>()
    val responses = ArrayList<Triple<Long, Boolean, String>>()
    val emits = ArrayList<Triple<String, String, String>>()
    var destroyed = false
    var loaded: String? = null

    fun push(bytes: ByteArray) {
        synchronized(batches) { batches.addLast(bytes) }
        callbacks.batchReady()
    }

    override fun loadSources(json: String): Int { loaded = json; return 0 }
    override fun loadBundle(bytes: ByteArray): Int = 0
    override fun setEnvironment(json: String) { environments += json }
    override fun dispatchEvent(node: Int, eventType: Int, payloadJson: String, seq: Int) {
        events += SentEvent(node, eventType, payloadJson, seq)
    }
    override fun hostRespond(requestId: Long, ok: Boolean, json: String) { synchronized(responses) { responses += Triple(requestId, ok, json) } }
    override fun hostEmit(module: String, event: String, payloadJson: String) { emits += Triple(module, event, payloadJson) }
    override fun tick(nowMs: Double): Long = -1
    override fun takeBatch(use: (ByteBuffer) -> Unit): Boolean {
        val b = synchronized(batches) { batches.removeFirstOrNull() } ?: return false
        // Native batches are direct little-endian buffers.
        val direct = ByteBuffer.allocateDirect(b.size).order(ByteOrder.LITTLE_ENDIAN)
        direct.put(b).flip()
        use(direct)
        return true
    }
    override fun setActive(active: Boolean) {}
    override fun trimMemory() {}
    override fun destroy() { destroyed = true }
}

/** Builds batches the way the core does: `BeginBatch … EndBatch`. */
class BatchBuilder(private val seq: Int, private val flags: Int) {
    val mutations = ArrayList<Mutation>().apply { add(Mutation.BeginBatch(seq, flags)) }
    fun create(node: Int, type: Int) = apply { mutations += Mutation.Create(node, type) }
    fun insert(node: Int, parent: Int, index: Int) = apply { mutations += Mutation.Insert(node, parent, index) }
    fun remove(node: Int, parent: Int, index: Int) = apply { mutations += Mutation.Remove(node, parent, index) }
    fun delete(node: Int) = apply { mutations += Mutation.Delete(node) }
    fun move(node: Int, parent: Int, from: Int, to: Int) = apply { mutations += Mutation.Move(node, parent, from, to) }
    fun prop(node: Int, prop: Int, value: dev.nativeshell.protocol.PropValue) = apply { mutations += Mutation.SetProp(node, prop, value) }
    fun frame(node: Int, x: Float, y: Float, w: Float, h: Float) = apply { mutations += Mutation.SetFrame(node, x, y, w, h) }
    fun mask(node: Int, mask: Int) = apply { mutations += Mutation.SetEventMask(node, mask) }
    fun command(node: Int, command: Int, payload: String = "{}") = apply { mutations += Mutation.Command(node, command, payload) }
    fun bytes(): ByteArray = BatchEncoder.encode(mutations + Mutation.EndBatch)
}

/**
 * A surface view attached to an instance backed by [FakeEngine]. Main thread only.
 */
internal class Harness(context: Context, config: ShellConfig = ShellConfig().appId("test.harness")) {
    lateinit var engine: FakeEngine
        private set
    val instance: ShellInstance = ShellInstance(context, config) { cb, _ -> FakeEngine(cb).also { engine = it } }
    val surface = ShellSurfaceView(context)
    private var seq = 0

    init {
        surface.attach(instance)
    }

    val mounting get() = surface.renderer!!.mounting

    /** Encodes, queues and applies one batch synchronously. */
    fun batch(flags: Int = 0, build: BatchBuilder.() -> Unit) {
        val b = BatchBuilder(++seq, flags)
        b.build()
        applyBytes(b.bytes())
    }

    fun applyBytes(bytes: ByteArray) {
        check(Looper.myLooper() == Looper.getMainLooper()) { "apply on the main thread" }
        engine.push(bytes)
        instance.drainNow()
    }

    fun view(node: Int) = mounting.node(node)!!.view

    fun eventsOf(type: Int) = engine.events.filter { it.type == type }
}
