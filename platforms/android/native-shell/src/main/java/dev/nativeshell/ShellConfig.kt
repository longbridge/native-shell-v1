package dev.nativeshell

import android.content.Context
import org.json.JSONArray
import org.json.JSONObject
import java.io.File

/**
 * Configuration of one instance. Java-friendly builder style:
 * `new ShellConfig().appId("com.example.todo").dev(true)`.
 */
class ShellConfig {
    var appId: String? = null
        private set
    var dataDir: File? = null
        private set
    var dev: Boolean = false
        private set
    /** JSON of `Capabilities` granted by the app (overrides the manifest). */
    var capabilitiesJson: String? = null
        private set
    /** JSON of limits `{memory_mb, stack_kb, render_ms, event_ms, module_ms}`. */
    var limitsJson: String? = null
        private set
    /** Base for bundle-relative image paths: `asset://dir/` or a directory path. */
    var resourceBase: String? = null
        private set
    /** Drive the JS thread from the main thread (`MS_THREAD_CALLER`); default is a dedicated thread. */
    var callerThread: Boolean = false
        private set

    fun appId(id: String) = apply { appId = id }
    fun dataDir(dir: File) = apply { dataDir = dir }
    fun dev(enabled: Boolean) = apply { dev = enabled }
    fun capabilities(json: String) = apply { capabilitiesJson = json }
    fun limits(json: String) = apply { limitsJson = json }
    fun resourceBase(base: String) = apply { resourceBase = base }
    fun callerThread(enabled: Boolean) = apply { callerThread = enabled }

    internal fun toJson(context: Context, environment: JSONObject): String {
        val o = JSONObject()
        appId?.let { o.put("app_id", it) }
        val dir = dataDir ?: File(context.filesDir, "native-shell/${appId ?: "default"}")
        o.put("data_dir", dir.absolutePath)
        o.put("threading", if (callerThread) 1 else 0)
        o.put("dev", dev)
        limitsJson?.let { o.put("limits", JSONObject(it)) }
        capabilitiesJson?.let { o.put("capabilities", JSONObject(it)) }
        o.put("registry", NativeShell.registryJson())
        o.put("environment", environment)
        val keys = JSONArray()
        for (k in NativeShell.trustedKeys) keys.put(JSONObject().put("id", k.id).put("public_key", k.publicKey))
        o.put("trusted_keys", keys)
        o.put("revoked", JSONArray(NativeShell.revokedBundles))
        NativeShell.themeJson()?.let { o.put("theme", it) }
        return o.toString()
    }
}
