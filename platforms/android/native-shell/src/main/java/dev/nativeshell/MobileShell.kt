package dev.nativeshell

import android.content.ActivityNotFoundException
import android.content.Intent
import android.net.Uri
import android.util.Log
import dev.nativeshell.render.HttpImageLoader
import dev.nativeshell.util.Json
import org.json.JSONArray
import org.json.JSONObject
import java.util.Collections
import java.util.WeakHashMap
import java.util.concurrent.ConcurrentHashMap

/**
 * Process-wide registration: host modules and components, theme mapping and
 * platform hooks. Register before creating instances; an instance sees what
 * was registered when it was created.
 */
object NativeShell {
    private val modules = ConcurrentHashMap<String, HostModule>()
    private val components = ConcurrentHashMap<String, HostComponentSpec>()
    private val instances = Collections.synchronizedSet(Collections.newSetFromMap(WeakHashMap<ShellInstance, Boolean>()))

    @JvmStatic
    fun registerModule(module: HostModule) {
        modules[module.name] = module
    }

    @JvmStatic
    fun registerComponent(spec: HostComponentSpec) {
        components[spec.name] = spec
    }

    @JvmStatic
    fun unregisterModule(name: String) {
        modules.remove(name)
    }

    @JvmStatic
    fun unregisterComponent(name: String) {
        components.remove(name)
    }

    /** Pushes a module event to every live instance (scripts that listen receive it). */
    @JvmStatic
    fun emit(module: String, event: String, payload: Any?) {
        val json = Json.stringify(payload)
        val live = synchronized(instances) { instances.toList() }
        for (i in live) i.hostEmit(module, event, json)
    }

    /** Maps the app's design system to the shell color tokens; sent to instances at creation. */
    @JvmStatic
    var themeProvider: ThemeProvider? = null

    @JvmStatic
    var imageLoader: ImageLoader = HttpImageLoader

    @JvmStatic
    var urlOpener: UrlOpener = UrlOpener { context, url ->
        try {
            context.startActivity(Intent(Intent.ACTION_VIEW, Uri.parse(url)).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK))
            true
        } catch (e: ActivityNotFoundException) {
            false
        }
    }

    @JvmStatic
    var logger: ShellLogger = ShellLogger { level, tag, message ->
        when (level) {
            LogLevel.DEBUG -> Log.d(tag, message)
            LogLevel.INFO -> Log.i(tag, message)
            LogLevel.WARN -> Log.w(tag, message)
            else -> Log.e(tag, message)
        }
    }

    /** Keys whose signatures [ShellInstance.loadBundle] accepts. */
    @JvmStatic
    var trustedKeys: List<TrustedKey> = emptyList()

    /** SHA-256 (hex) of bundles that must not load. */
    @JvmStatic
    var revokedBundles: List<String> = emptyList()

    /**
     * Exposes `TestId` as contentDescription when a node has no accessibility
     * label, so black-box UI test tools can find it. Off by default: it would be
     * read out by screen readers.
     */
    @JvmStatic
    var testIdAsContentDescription: Boolean = false

    /** Logs every applied mutation (debug level, tag `native-shell`). Very verbose; for debugging only. */
    @JvmStatic
    var traceBatches: Boolean = false

    /** The engine's ABI hash, or null when the native library is not available. */
    @JvmStatic
    val abiHash: String?
        get() = if (NativeBridge.isLoaded) NativeBridge.abiHash() else null

    @JvmStatic
    val isNativeAvailable: Boolean
        get() = NativeBridge.isLoaded

    internal fun module(name: String): HostModule? = modules[name]

    internal fun component(name: String): HostComponentSpec? = components[name]

    internal fun track(instance: ShellInstance) {
        instances.add(instance)
    }

    internal fun untrack(instance: ShellInstance) {
        instances.remove(instance)
    }

    /** `HostRegistry` JSON (crates/core/src/host.rs). */
    internal fun registryJson(): JSONObject {
        val mods = JSONArray()
        for (m in modules.values.sortedBy { it.name }) {
            val methods = JSONArray()
            for (meth in m.methods) {
                methods.put(JSONObject().put("name", meth.name).put("kind", if (meth.kind == MethodKind.SYNC) "sync" else "async").apply {
                    meth.ts?.let { put("ts", it) }
                })
            }
            mods.put(
                JSONObject().put("name", m.name).put("methods", methods).put("events", JSONArray(m.events))
                    .put("requires_permission", m.requiresPermission).apply { m.typeScriptDeclarations?.let { put("dts", it) } },
            )
        }
        val comps = JSONArray()
        for (c in components.values.sortedBy { it.name }) {
            comps.put(
                JSONObject().put("name", c.name).put("events", JSONArray(c.events)).put("measurable", c.measure != null)
                    .put("requires_permission", c.requiresPermission).apply { c.propsTypeScript?.let { put("props_ts", it) } },
            )
        }
        return JSONObject().put("modules", mods).put("components", comps)
    }

    /** `{light: {token: "#rrggbbaa"}, dark: {…}}` from [themeProvider], or null. */
    internal fun themeJson(): JSONObject? {
        val p = themeProvider ?: return null
        fun table(dark: Boolean): JSONObject {
            val o = JSONObject()
            for ((k, v) in p.colors(dark)) o.put(k, argbToHex(v))
            return o
        }
        return JSONObject().put("light", table(false)).put("dark", table(true))
    }

    internal fun argbToHex(argb: Int): String = "#%06x%02x".format(argb and 0xFFFFFF, (argb ushr 24) and 0xFF)
}
