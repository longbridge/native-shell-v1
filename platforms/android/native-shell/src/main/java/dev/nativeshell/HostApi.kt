package dev.nativeshell

import android.content.Context
import android.util.SizeF
import android.view.View
import android.widget.ImageView
import org.json.JSONArray
import org.json.JSONObject

/** How a host-module method answers: on the JS thread before returning, or later through a [Responder]. */
enum class MethodKind { SYNC, ASYNC }

/** One method of a [HostModule]. `ts` is its TypeScript signature for generated `.d.ts` files. */
class HostMethod @JvmOverloads constructor(
    val name: String,
    val kind: MethodKind = MethodKind.ASYNC,
    val ts: String? = null,
)

/**
 * A JS-importable module answered by app code (`import { quote } from "demo/quote"`).
 *
 * Sync methods run on the instance's JS thread and must be fast. Async methods
 * may answer the [Responder] on any thread, exactly once. Arguments arrive as a
 * JSON array; results are JSON-able values ([JSONObject], [JSONArray], strings,
 * numbers, booleans, `null`, or Kotlin maps/lists, which are converted).
 */
interface HostModule {
    val name: String
    val methods: List<HostMethod>
    val events: List<String> get() = emptyList()
    val requiresPermission: Boolean get() = true
    val typeScriptDeclarations: String? get() = null

    /** Throw to make the script call throw with the exception's message. */
    fun callSync(method: String, args: JSONArray, context: CallContext): Any? =
        throw UnsupportedOperationException("$name.$method is not a sync method")

    fun call(method: String, args: JSONArray, context: CallContext, responder: Responder) {
        responder.reject("$name.$method is not implemented")
    }
}

/** Answers one async call. Only the first `resolve`/`reject` counts; later ones are ignored. */
interface Responder {
    fun resolve(value: Any?)
    fun reject(message: String)
}

/** The instance a host-module call came from. */
interface CallContext {
    val appId: String

    /** Push an event of the called module to this instance only. */
    fun emit(event: String, payload: Any?)
}

/** Lets a host component send events to the script (`.on(name, fn)`). */
fun interface EventSink {
    fun emit(name: String, payload: Any?)
}

/**
 * A native view type the app registers by name. One [HostComponent] object is
 * created per native view (pooled per name), so it may keep per-view state.
 */
interface HostComponent {
    fun createView(context: Context): View

    /** Called with the full props on every change. */
    fun update(view: View, props: JSONObject)

    /** Restore defaults before the view is reused for another node. */
    fun reset(view: View) {}

    fun setEventSink(view: View, sink: EventSink) {}
}

/**
 * Registration record of a host component.
 *
 * [measure], when present, makes the component measurable: the core asks for
 * its size during layout, on the JS thread, with the available width in pixels;
 * it returns a size in pixels.
 */
class HostComponentSpec @JvmOverloads constructor(
    val name: String,
    val events: List<String> = emptyList(),
    val measure: ComponentMeasurer? = null,
    val requiresPermission: Boolean = true,
    val propsTypeScript: String? = null,
    val factory: HostComponentFactory,
)

fun interface HostComponentFactory {
    fun create(): HostComponent
}

fun interface ComponentMeasurer {
    fun measure(props: JSONObject, maxWidthPx: Float): SizeF?
}

/** Maps the app's design system onto the 17 shell color tokens (`"primary"` → ARGB int). */
fun interface ThemeProvider {
    fun colors(dark: Boolean): Map<String, Int>
}

/** Loads `url` into `imageView`. Called on the main thread; load asynchronously. */
fun interface ImageLoader {
    fun load(url: String, imageView: ImageView)
}

fun interface UrlOpener {
    fun open(context: Context, url: String): Boolean
}

fun interface ShellLogger {
    fun log(level: Int, tag: String, message: String)
}

/** A trusted bundle-signing key: Ed25519 public key in base64. */
class TrustedKey(val id: String, val publicKey: String)

object LogLevel {
    const val DEBUG = 0
    const val INFO = 1
    const val WARN = 2
    const val ERROR = 3
}
