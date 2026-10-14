package dev.nativeshell

import java.nio.ByteBuffer

/**
 * JNI surface of `libnative_shell.so` (implemented in `crates/ffi/src/jni.rs`).
 * Same semantics as the C ABI in `native_shell.h`.
 */
internal object NativeBridge {
    /** Whether the native library loaded; false in JVM unit tests and when the `.so` is missing. */
    val isLoaded: Boolean by lazy {
        try {
            System.loadLibrary("native_shell")
            true
        } catch (e: UnsatisfiedLinkError) {
            false
        }
    }

    @JvmStatic external fun create(callbacks: HostCallbacks, configJson: String): Long
    @JvmStatic external fun loadSources(handle: Long, json: String): Int
    @JvmStatic external fun loadBundle(handle: Long, bytes: ByteArray): Int
    @JvmStatic external fun setEnvironment(handle: Long, json: String)
    @JvmStatic external fun dispatchEvent(handle: Long, node: Int, eventType: Int, payloadJson: String, seq: Int)
    @JvmStatic external fun hostRespond(handle: Long, requestId: Long, ok: Boolean, json: String)
    @JvmStatic external fun hostEmit(handle: Long, module: String, event: String, payloadJson: String)
    @JvmStatic external fun tick(handle: Long, nowMs: Double): Long
    @JvmStatic external fun takeBatch(handle: Long): Long
    @JvmStatic external fun batchBuffer(batch: Long): ByteBuffer
    @JvmStatic external fun freeBatch(batch: Long)
    @JvmStatic external fun setActive(handle: Long, active: Boolean)
    @JvmStatic external fun trimMemory(handle: Long)
    @JvmStatic external fun destroy(handle: Long)
    @JvmStatic external fun lastError(): String
    @JvmStatic external fun abiHash(): String
    @JvmStatic external fun protocolVersion(): Int
    /** Core-side timing counters (JSON). Newer libraries only. */
    @JvmStatic external fun metrics(handle: Long): String
}

/** Callbacks the native instance invokes, all on its JS thread. */
interface HostCallbacks {
    /** Returns `[width, height, baseline]` in points. */
    fun measureText(attributedJson: String, width: Float, widthMode: Int): FloatArray

    /** Returns `[width, height]` in points, or null when the component has no intrinsic size. */
    fun measureComponent(name: String, propsJson: String, width: Float, widthMode: Int): FloatArray?
    fun batchReady()

    /**
     * Max-content (or constrained) measurement of many texts at once: 3 floats
     * per item, identical to [measureText] item by item; may shape in parallel.
     */
    fun measureTextBatch(jsons: Array<String>, widths: FloatArray, modes: IntArray): FloatArray

    /** Throw to reject. */
    fun callSync(module: String, method: String, argsJson: String): String
    fun callAsync(requestId: Long, module: String, method: String, argsJson: String)
    fun log(level: Int, message: String)
    fun error(message: String)
}
