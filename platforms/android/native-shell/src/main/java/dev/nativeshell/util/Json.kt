package dev.nativeshell.util

import android.graphics.Color
import org.json.JSONArray
import org.json.JSONException
import org.json.JSONObject

/** JSON helpers shared by prop parsing and the host bridge. None of them throw on bad input. */
internal object Json {

    /** Serializes a JSON-able value (org.json types, Kotlin maps/lists/arrays, primitives) to JSON text. */
    fun stringify(value: Any?): String = when (val v = if (value is JSONObject || value is JSONArray) value else JSONObject.wrap(value)) {
        null, JSONObject.NULL -> "null"
        is JSONObject -> v.toString()
        is JSONArray -> v.toString()
        is String -> JSONObject.quote(v)
        is Boolean -> v.toString()
        is Number -> {
            val d = v.toDouble()
            if (d.isNaN() || d.isInfinite()) "null"
            else if (v is Int || v is Long || v is Short || v is Byte) v.toString()
            else JSONObject.numberToString(v)
        }
        else -> JSONObject.quote(v.toString())
    }

    fun obj(text: String?): JSONObject? {
        if (text.isNullOrEmpty()) return null
        return try { JSONObject(text) } catch (e: JSONException) { null }
    }

    fun arr(text: String?): JSONArray? {
        if (text.isNullOrEmpty()) return null
        return try { JSONArray(text) } catch (e: JSONException) { null }
    }

    /** Parses a JSON value of any kind (used for host-module results and event payloads). */
    fun parseAny(text: String?): Any? {
        if (text.isNullOrEmpty()) return null
        return try { org.json.JSONTokener(text).nextValue() } catch (e: JSONException) { null }
    }

    /**
     * A color inside a JSON value: an ARGB number (as `ValueKind::Color`), or a
     * `#rgb` / `#rrggbb` / `#rrggbbaa` string. Returns [default] when absent or invalid.
     */
    fun color(o: JSONObject?, key: String, default: Int): Int {
        if (o == null || !o.has(key) || o.isNull(key)) return default
        return colorOf(o.opt(key)) ?: default
    }

    fun colorOf(v: Any?): Int? = when (v) {
        is Number -> v.toLong().toInt()
        is String -> parseHex(v)
        else -> null
    }

    fun parseHex(s: String): Int? {
        if (!s.startsWith("#")) return null
        val h = s.substring(1)
        if (h.any { !it.isLetterOrDigit() }) return null
        return try {
            when (h.length) {
                3 -> Color.argb(255, dup(h[0]), dup(h[1]), dup(h[2]))
                4 -> Color.argb(dup(h[3]), dup(h[0]), dup(h[1]), dup(h[2]))
                6 -> (0xFF000000.toInt()) or h.toInt(16)
                8 -> {
                    val rgb = h.substring(0, 6).toInt(16)
                    val a = h.substring(6, 8).toInt(16)
                    (a shl 24) or rgb
                }
                else -> null
            }
        } catch (e: NumberFormatException) { null }
    }

    private fun dup(c: Char): Int = (c.toString() + c).toInt(16)

    fun float(o: JSONObject?, key: String, default: Float): Float {
        if (o == null || !o.has(key) || o.isNull(key)) return default
        val d = o.optDouble(key, Double.NaN)
        return if (d.isNaN()) default else d.toFloat()
    }

    fun int(o: JSONObject?, key: String, default: Int): Int {
        if (o == null || !o.has(key) || o.isNull(key)) return default
        val d = o.optDouble(key, Double.NaN)
        return if (d.isNaN()) default else d.toInt()
    }

    fun bool(o: JSONObject?, key: String, default: Boolean): Boolean {
        if (o == null || !o.has(key) || o.isNull(key)) return default
        return when (val v = o.opt(key)) {
            is Boolean -> v
            is Number -> v.toInt() != 0
            else -> default
        }
    }

    fun string(o: JSONObject?, key: String): String? {
        if (o == null || !o.has(key) || o.isNull(key)) return null
        return o.opt(key)?.toString()
    }

    fun strings(a: JSONArray?): List<String> {
        if (a == null) return emptyList()
        return (0 until a.length()).mapNotNull { a.opt(it)?.takeIf { v -> v != JSONObject.NULL }?.toString() }
    }
}
