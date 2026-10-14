package dev.nativeshell

import java.nio.ByteBuffer

/**
 * The script engine as the Android side sees it. [NativeEngine] is the real
 * one; tests substitute a fake that records events and hands out encoded
 * batches, so mounting and views are testable without the native library.
 */
internal interface Engine {
    fun loadSources(json: String): Int
    fun loadBundle(bytes: ByteArray): Int
    fun setEnvironment(json: String)
    fun dispatchEvent(node: Int, eventType: Int, payloadJson: String, seq: Int)
    fun hostRespond(requestId: Long, ok: Boolean, json: String)
    fun hostEmit(module: String, event: String, payloadJson: String)

    /** Caller-thread mode: runs due work; returns ms until the next timer (-1: none). */
    fun tick(nowMs: Double): Long

    /** Hands the next pending batch to [use] (valid only during the call). False when none. */
    fun takeBatch(use: (ByteBuffer) -> Unit): Boolean
    fun setActive(active: Boolean)
    fun trimMemory()
    fun destroy()

    /** Core-side counters as JSON, or null when the engine does not report them. */
    fun metrics(): String? = null
}

internal class NativeEngine private constructor(private val handle: Long) : Engine {
    override fun loadSources(json: String) = NativeBridge.loadSources(handle, json)
    override fun loadBundle(bytes: ByteArray) = NativeBridge.loadBundle(handle, bytes)
    override fun setEnvironment(json: String) = NativeBridge.setEnvironment(handle, json)
    override fun dispatchEvent(node: Int, eventType: Int, payloadJson: String, seq: Int) =
        NativeBridge.dispatchEvent(handle, node, eventType, payloadJson, seq)
    override fun hostRespond(requestId: Long, ok: Boolean, json: String) = NativeBridge.hostRespond(handle, requestId, ok, json)
    override fun hostEmit(module: String, event: String, payloadJson: String) = NativeBridge.hostEmit(handle, module, event, payloadJson)
    override fun tick(nowMs: Double): Long = NativeBridge.tick(handle, nowMs)

    override fun takeBatch(use: (ByteBuffer) -> Unit): Boolean {
        val batch = NativeBridge.takeBatch(handle)
        if (batch == 0L) return false
        try {
            use(NativeBridge.batchBuffer(batch))
        } finally {
            NativeBridge.freeBatch(batch)
        }
        return true
    }

    override fun setActive(active: Boolean) = NativeBridge.setActive(handle, active)
    override fun trimMemory() = NativeBridge.trimMemory(handle)
    override fun destroy() = NativeBridge.destroy(handle)

    override fun metrics(): String? = try {
        NativeBridge.metrics(handle)
    } catch (e: UnsatisfiedLinkError) {
        null
    }

    companion object {
        fun create(callbacks: HostCallbacks, configJson: String): NativeEngine {
            if (!NativeBridge.isLoaded) throw ShellException("libnative_shell.so is not available")
            val h = NativeBridge.create(callbacks, configJson)
            if (h == 0L) throw ShellException(NativeBridge.lastError())
            return NativeEngine(h)
        }
    }
}

class ShellException(message: String) : RuntimeException(message)
