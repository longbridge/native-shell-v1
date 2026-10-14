package dev.nativeshell.example

import android.app.Application
import android.os.Handler
import android.os.Looper
import android.util.SizeF
import dev.nativeshell.CallContext
import dev.nativeshell.HostComponentSpec
import dev.nativeshell.HostMethod
import dev.nativeshell.HostModule
import dev.nativeshell.MethodKind
import dev.nativeshell.NativeShell
import dev.nativeshell.Responder
import dev.nativeshell.ThemeProvider
import org.json.JSONArray
import org.json.JSONObject
import kotlin.random.Random

/**
 * Registers what examples/watchlist uses (the quote and user modules and the
 * stock cell) once per process, before any instance starts, and maps the
 * app's accent onto the shell's theme.
 */
class DemoApplication : Application() {
    override fun onCreate() {
        super.onCreate()
        NativeShell.themeProvider = ThemeProvider { dark -> mapOf("primary" to AppPalette.accent(dark)) }
        NativeShell.registerModule(QuoteModule())
        NativeShell.registerModule(UserModule())
        NativeShell.registerComponent(
            HostComponentSpec(
                name = "demo-stock-cell",
                events = listOf("select"),
                measure = { _, maxWidthPx -> SizeF(maxWidthPx, StockCellView.heightPx(this)) },
                propsTypeScript = "{ symbol: string; name: string; last?: number | null; change_percent?: number | null }",
                factory = { StockCell() },
            ),
        )
    }
}

/**
 * `demo/quote`: fictional quotes, and a push channel that ticks one subscribed
 * symbol a second on the main looper (standing in for the app's own socket)
 * while a script listens.
 */
class QuoteModule : HostModule {
    override val name = "demo/quote"
    override val methods = listOf(
        HostMethod("quote", MethodKind.ASYNC, "(symbol: string) => Promise<Quote>"),
        HostMethod("subscribe", MethodKind.ASYNC, "(symbols: string[]) => Promise<void>"),
        HostMethod("unsubscribe", MethodKind.ASYNC, "(symbols: string[]) => Promise<void>"),
    )
    override val events = listOf("tick")
    override val typeScriptDeclarations = "interface Quote { symbol: string; last: number; prev_close: number; change_percent: number }"

    private val main = Handler(Looper.getMainLooper())
    private val prices = HashMap(INSTRUMENTS.mapValues { (_, v) -> v.first * (1 + v.second) })
    private val subscribed = LinkedHashSet<String>()
    private var listeners = 0

    private val ticker = object : Runnable {
        override fun run() {
            val symbol = synchronized(subscribed) { subscribed.randomOrNull() }
            if (symbol != null) {
                synchronized(prices) { prices[symbol] = prices.getValue(symbol) * (1 + Random.nextDouble(-0.004, 0.004)) }
                quote(symbol)?.let { NativeShell.emit(name, "tick", it) }
            }
            schedule()
        }
    }

    private fun schedule() {
        main.removeCallbacks(ticker)
        if (listeners > 0 && synchronized(subscribed) { subscribed.isNotEmpty() }) main.postDelayed(ticker, 1000)
    }

    private fun quote(symbol: String): JSONObject? = synchronized(prices) {
        val close = INSTRUMENTS[symbol]?.first ?: return null
        val last = prices.getValue(symbol)
        JSONObject()
            .put("symbol", symbol)
            .put("last", Math.round(last * 100) / 100.0)
            .put("prev_close", close)
            .put("change_percent", Math.round((last - close) / close * 10000) / 100.0)
    }

    override fun callSync(method: String, args: JSONArray, context: CallContext): Any? {
        if (method != "__listeners") throw UnsupportedOperationException(method)
        // Start the feed on the first listener, stop it after the last.
        main.post {
            listeners = args.optInt(1)
            schedule()
        }
        return null
    }

    override fun call(method: String, args: JSONArray, context: CallContext, responder: Responder) {
        // A short delay stands in for the network round trip.
        main.postDelayed({
            when (method) {
                "quote" -> quote(args.optString(0))?.let { responder.resolve(it) } ?: responder.reject("No quote for ${args.optString(0)}")
                "subscribe", "unsubscribe" -> {
                    val list = args.optJSONArray(0) ?: JSONArray()
                    synchronized(subscribed) {
                        for (i in 0 until list.length()) if (method == "subscribe") subscribed += list.getString(i) else subscribed -= list.getString(i)
                    }
                    schedule()
                    responder.resolve(null)
                }
                else -> responder.reject("demo/quote has no method `$method`")
            }
        }, 250)
    }

    companion object {
        /** Previous close and today's opening move of each fictional instrument. */
        val INSTRUMENTS = mapOf(
            "HALO.US" to (182.40 to 0.0124), "MAPL.US" to (64.15 to -0.0062), "KITE.US" to (27.80 to 0.0310),
            "ORCA.US" to (311.05 to -0.0185), "FERN.US" to (18.92 to 0.0040), "PINE.HK" to (46.30 to -0.0015),
            "TIDE.HK" to (9.87 to 0.0205), "LUMA.US" to (121.60 to -0.0090),
        )
    }
}

/** `demo/user`: a sync read of in-memory state. */
class UserModule : HostModule {
    override val name = "demo/user"
    override val methods = listOf(HostMethod("current_user", MethodKind.SYNC, "() => { id: string; name: string } | null"))
    override val requiresPermission = false

    override fun callSync(method: String, args: JSONArray, context: CallContext): Any? = when (method) {
        "current_user" -> JSONObject().put("id", "u-1").put("name", "Ada Lovelace")
        "__listeners" -> null
        else -> throw UnsupportedOperationException(method)
    }
}
