package dev.nativeshell

import android.content.Context
import android.os.Handler
import android.os.Looper
import android.os.SystemClock
import android.view.Choreographer
import dev.nativeshell.text.TextLayouts
import dev.nativeshell.util.Json
import org.json.JSONArray
import org.json.JSONObject
import java.nio.ByteBuffer
import java.util.concurrent.atomic.AtomicBoolean

/**
 * One isolated script program: its own QuickJS VM on its own JS thread.
 *
 * Create it, attach it to a [ShellSurfaceView], then load sources or a signed
 * bundle. Several instances may run side by side; host modules registered on
 * [NativeShell] are shared by all of them.
 */
class ShellInstance internal constructor(
    context: Context,
    val config: ShellConfig,
    engineFactory: (HostCallbacks, String) -> Engine,
) {
    fun interface ErrorListener {
        fun onError(message: String)
    }

    fun interface LogListener {
        fun onLog(level: Int, message: String)
    }

    private val appContext: Context = context.applicationContext ?: context
    private val main = Handler(Looper.getMainLooper())
    private val destroyedFlag = AtomicBoolean(false)
    internal val engine: Engine

    /** Called on the main thread. */
    var errorListener: ErrorListener? = null
    /** Called on the thread that logged (usually the JS thread). */
    var logListener: LogListener? = null

    /** Base for bundle-relative image paths (see [ShellConfig.resourceBase]). */
    var resourceBase: String? = config.resourceBase

    val appId: String get() = config.appId ?: ""

    internal var surface: SurfaceRenderer? = null
        set(value) {
            field = value
            if (value != null) scheduleDrain()
        }

    private var drainScheduled = false
    private val frameCallback = Choreographer.FrameCallback { drainNow() }
    private val drainRunnable = Runnable { drainNow() }
    private val tickRunnable = Runnable { tickNow() }

    /**
     * Timestamps (System.nanoTime) of the last hop of each kind, to see where an
     * event→frame round trip spends its time: dispatch → batchReady (JS thread) →
     * drain start (frame callback) → batch taken → applied.
     */
    internal class Timeline {
        @Volatile var dispatched = 0L
        @Volatile var batchReady = 0L
        @Volatile var drainStart = 0L
        @Volatile var taken = 0L
        @Volatile var applied = 0L
    }

    internal val timeline = Timeline()

    /** Observes every event sent to the script (tests, debugging). Main thread. */
    internal var eventObserver: ((node: Int, type: Int, payload: String) -> Unit)? = null

    val isDestroyed: Boolean get() = destroyedFlag.get()

    private val callbacks = object : HostCallbacks {
        override fun measureText(attributedJson: String, width: Float, widthMode: Int): FloatArray =
            TextLayouts.measure(attributedJson, width, widthMode)

        override fun measureComponent(name: String, propsJson: String, width: Float, widthMode: Int): FloatArray? {
            // Built-in controls whose size depends on their text (reserved `nsh/` names).
            if (name.startsWith("nsh/")) {
                val d = TextLayouts.density
                val (w, h) = dev.nativeshell.views.ValueFieldMetrics(TextLayouts.context ?: appContext)
                    .measureBuiltIn(name, Json.obj(propsJson) ?: JSONObject()) ?: return null
                val maxW = if (widthMode == 0 || !width.isFinite()) Float.POSITIVE_INFINITY else width * d
                return floatArrayOf(minOf(w, maxW) / d, h / d)
            }
            val spec = NativeShell.component(name) ?: return null
            val m = spec.measure ?: return null
            val d = TextLayouts.density
            val props = Json.obj(propsJson) ?: JSONObject()
            val max = if (widthMode == 0 || !width.isFinite()) Float.POSITIVE_INFINITY else width * d
            val size = try { m.measure(props, max) } catch (e: Exception) { null } ?: return null
            return floatArrayOf(size.width / d, size.height / d)
        }

        override fun measureTextBatch(jsons: Array<String>, widths: FloatArray, modes: IntArray): FloatArray =
            TextLayouts.measureBatch(jsons, widths, modes)

        override fun batchReady() {
            timeline.batchReady = System.nanoTime()
            main.post { scheduleDrain() }
        }

        override fun callSync(module: String, method: String, argsJson: String): String {
            // Reserved module; the core already checked the clipboard capability.
            if (module == CLIPBOARD_MODULE && method == "read") return Json.stringify(readClipboard())
            val m = NativeShell.module(module) ?: throw IllegalStateException("host module `$module` is not registered")
            val args = Json.arr(argsJson) ?: JSONArray()
            return Json.stringify(m.callSync(method, args, context(module)))
        }

        override fun callAsync(requestId: Long, module: String, method: String, argsJson: String) {
            val responder = ResponderImpl(requestId)
            val m = NativeShell.module(module)
            if (m == null) {
                responder.reject("host module `$module` is not registered")
                return
            }
            try {
                m.call(method, Json.arr(argsJson) ?: JSONArray(), context(module), responder)
            } catch (e: Exception) {
                responder.reject(e.message ?: e.javaClass.simpleName)
            }
        }

        override fun log(level: Int, message: String) = this@ShellInstance.log(level, message)

        override fun error(message: String) {
            log(LogLevel.ERROR, message)
            main.post { errorListener?.onError(message) }
        }
    }

    init {
        TextLayouts.init(appContext)
        engine = engineFactory(callbacks, config.toJson(appContext, defaultEnvironment(appContext)))
        NativeShell.track(this)
        if (config.callerThread) main.post(tickRunnable)
    }

    /** ClipboardManager is safe to read off the main thread on API 24+. */
    private fun readClipboard(): String? {
        val cm = appContext.getSystemService(Context.CLIPBOARD_SERVICE) as? android.content.ClipboardManager ?: return null
        val clip = cm.primaryClip?.takeIf { it.itemCount > 0 } ?: return null
        return clip.getItemAt(0).coerceToText(appContext)?.toString()
    }

    private fun context(module: String) = object : CallContext {
        override val appId: String get() = this@ShellInstance.appId
        override fun emit(event: String, payload: Any?) = hostEmit(module, event, Json.stringify(payload))
    }

    private inner class ResponderImpl(private val id: Long) : Responder {
        private val done = AtomicBoolean(false)
        override fun resolve(value: Any?) {
            if (done.compareAndSet(false, true) && !isDestroyed) {
                engine.hostRespond(id, true, Json.stringify(value))
                wake()
            }
        }

        override fun reject(message: String) {
            if (done.compareAndSet(false, true) && !isDestroyed) {
                engine.hostRespond(id, false, message)
                wake()
            }
        }
    }

    // ---- Loading ------------------------------------------------------------------------

    /** Loads plain sources: `{"manifest": {…}, "files": {"main.js": "…"}}`. Errors arrive at [errorListener]. */
    fun loadSources(json: String) {
        check(!isDestroyed) { "instance destroyed" }
        if (engine.loadSources(json) != 0) failed("loadSources")
        wake()
    }

    fun loadSources(manifestJson: String, files: Map<String, String>) {
        val o = JSONObject().put("manifest", JSONObject(manifestJson)).put("files", JSONObject(files as Map<*, *>))
        loadSources(o.toString())
    }

    /**
     * Loads a program from an APK asset directory holding `native-shell.json`
     * and its `.js` files; images resolve relative to the same directory.
     */
    fun loadSourcesFromAssets(context: Context, dir: String) {
        val clean = dir.trim('/')
        val files = LinkedHashMap<String, String>()
        fun walk(path: String, rel: String) {
            val list = context.assets.list(path) ?: return
            if (list.isEmpty()) {
                // Text files go as text; binary ones (images…) as `data:` URLs, which the
                // engine materializes for image(path) (the sources contract is text only).
                files[rel] = programFile(rel, context.assets.open(path).use { it.readBytes() })
                return
            }
            for (name in list) walk("$path/$name", if (rel.isEmpty()) name else "$rel/$name")
        }
        walk(clean, "")
        val manifest = files.remove("native-shell.json") ?: throw ShellException("$dir has no native-shell.json")
        if (resourceBase == null) resourceBase = "asset://$clean/"
        loadSources(manifest, files)
    }

    /** Loads a signed `.nspkg` bundle; the signature is verified before any script runs. */
    fun loadBundle(bytes: ByteArray) {
        check(!isDestroyed) { "instance destroyed" }
        if (engine.loadBundle(bytes) != 0) failed("loadBundle")
        wake()
    }

    private fun failed(what: String) {
        val msg = if (NativeBridge.isLoaded) NativeBridge.lastError() else "$what failed"
        throw ShellException(msg)
    }

    // ---- Lifecycle ----------------------------------------------------------------------

    /** Core-side timing and cache counters (JSON), or null when the native library does not report them. */
    fun metrics(): String? = if (isDestroyed) null else engine.metrics()

    /** Pauses timers while the app is in the background. */
    fun setActive(active: Boolean) {
        if (!isDestroyed) engine.setActive(active)
    }

    fun trimMemory() {
        if (!isDestroyed) engine.trimMemory()
        dev.nativeshell.text.TextLayouts.clear()
    }

    /** Stops the VM. Pending async host calls are dropped. Safe to call twice. */
    fun destroy() {
        if (!destroyedFlag.compareAndSet(false, true)) return
        NativeShell.untrack(this)
        main.removeCallbacks(drainRunnable)
        main.removeCallbacks(tickRunnable)
        Choreographer.getInstance().removeFrameCallback(frameCallback)
        surface?.detachInstance()
        surface = null
        engine.destroy()
    }

    // ---- Engine traffic -----------------------------------------------------------------

    internal fun hostEmit(module: String, event: String, payloadJson: String) {
        if (isDestroyed) return
        engine.hostEmit(module, event, payloadJson)
        wake()
    }

    internal fun setEnvironment(json: String) {
        if (isDestroyed) return
        engine.setEnvironment(json)
        wake()
    }

    internal fun dispatchEvent(node: Int, type: Int, payload: String, seq: Int) {
        if (isDestroyed) return
        timeline.dispatched = System.nanoTime()
        eventObserver?.invoke(node, type, payload)
        engine.dispatchEvent(node, type, payload, seq)
        wake()
    }

    private fun log(level: Int, message: String) {
        logListener?.onLog(level, message)
        NativeShell.logger.log(level, "native-shell/${appId.ifEmpty { "instance" }}", message)
    }

    /** Caller-thread mode runs the VM on the main thread: give it a turn after every input. */
    private fun wake() {
        if (config.callerThread && !isDestroyed) {
            main.removeCallbacks(tickRunnable)
            main.post(tickRunnable)
        }
    }

    private fun tickNow() {
        if (isDestroyed) return
        val next = engine.tick(SystemClock.uptimeMillis().toDouble())
        drainNow()
        main.removeCallbacks(tickRunnable)
        if (next >= 0) main.postDelayed(tickRunnable, next.coerceAtLeast(1))
    }

    /** Applies pending batches next frame; right away while the surface waits for its first one. */
    internal fun scheduleDrain() {
        val s = surface ?: return
        if (drainScheduled || isDestroyed) return
        drainScheduled = true
        if (s.awaitingFirstBatch) main.postAtFrontOfQueue(drainRunnable)
        else Choreographer.getInstance().postFrameCallback(frameCallback)
    }

    /** Applies every pending batch now (main thread). */
    internal fun drainNow() {
        drainScheduled = false
        main.removeCallbacks(drainRunnable)
        Choreographer.getInstance().removeFrameCallback(frameCallback)
        val s = surface ?: return
        if (isDestroyed) return
        timeline.drainStart = System.nanoTime()
        while (engine.takeBatch { buf: ByteBuffer ->
                timeline.taken = System.nanoTime()
                s.applyBuffer(buf)
                timeline.applied = System.nanoTime()
            }
        ) { /* drain */ }
    }

    companion object {
        private const val CLIPBOARD_MODULE = "nsh/clipboard"

        /** One program file in the sources JSON: text as is, anything else as a base64 `data:` URL. */
        internal fun programFile(name: String, bytes: ByteArray): String {
            val ext = name.substringAfterLast('.', "").lowercase()
            if (ext in TEXT_EXTENSIONS) return String(bytes, Charsets.UTF_8)
            return "data:${mimeType(ext)};base64," + android.util.Base64.encodeToString(bytes, android.util.Base64.NO_WRAP)
        }

        private val TEXT_EXTENSIONS = setOf("js", "mjs", "json", "svg", "txt", "md", "css", "csv", "html", "xml", "ts")

        internal fun mimeType(ext: String): String = when (ext) {
            "png" -> "image/png"
            "jpg", "jpeg" -> "image/jpeg"
            "gif" -> "image/gif"
            "webp" -> "image/webp"
            "bmp" -> "image/bmp"
            "ico" -> "image/x-icon"
            "pdf" -> "application/pdf"
            else -> android.webkit.MimeTypeMap.getSingleton().getMimeTypeFromExtension(ext) ?: "application/octet-stream"
        }

        /**
         * Creates an instance backed by the native engine.
         * @throws ShellException when the native library is missing or the config is rejected.
         */
        @JvmStatic
        @JvmOverloads
        fun create(context: Context, config: ShellConfig = ShellConfig()): ShellInstance =
            ShellInstance(context, config) { cb, json -> NativeEngine.create(cb, json) }

        internal fun defaultEnvironment(context: Context): JSONObject {
            val dm = context.resources.displayMetrics
            val cfg = context.resources.configuration
            val dark = cfg.uiMode and android.content.res.Configuration.UI_MODE_NIGHT_MASK == android.content.res.Configuration.UI_MODE_NIGHT_YES
            return JSONObject()
                .put("width", (dm.widthPixels / dm.density).toDouble())
                .put("height", (dm.heightPixels / dm.density).toDouble())
                .put("scale", dm.density.toDouble())
                .put("font_scale", cfg.fontScale.toDouble())
                .put("safe_area", JSONArray(listOf(0, 0, 0, 0)))
                .put("keyboard_height", 0)
                .put("appearance", if (dark) "dark" else "light")
                .put("platform", "android")
                .put("locale", java.util.Locale.getDefault().toLanguageTag())
                .put("clipboard_feedback", android.os.Build.VERSION.SDK_INT >= 33)
        }
    }
}
