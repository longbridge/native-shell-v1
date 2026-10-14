package dev.nativeshell.util

import dev.nativeshell.render.AnimationSpec
import org.json.JSONArray
import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner

// org.json is an Android API; Robolectric provides the real implementation on the JVM.
@RunWith(RobolectricTestRunner::class)
class JsonTest {
    @Test
    fun colorsAsNumbersOrHex() {
        val o = JSONObject("""{"a": 4278190335, "b": "#ff0000", "c": "#0f08", "d": "#11223344", "e": "red", "f": null}""")
        assertEquals(0xFF0000FF.toInt(), Json.color(o, "a", 0))
        assertEquals(0xFFFF0000.toInt(), Json.color(o, "b", 0))
        assertEquals(0x8800FF00.toInt(), Json.color(o, "c", 0))
        assertEquals(0x44112233, Json.color(o, "d", 0))
        assertEquals(7, Json.color(o, "e", 7))
        assertEquals(7, Json.color(o, "f", 7))
        assertEquals(7, Json.color(o, "missing", 7))
        assertNull(Json.parseHex("#12345"))
        assertNull(Json.parseHex("#gggggg"))
    }

    @Test
    fun stringifyHandlesHostValues() {
        assertEquals("null", Json.stringify(null))
        assertEquals("\"a\\\"b\"", Json.stringify("a\"b"))
        assertEquals("3", Json.stringify(3))
        assertEquals("1.5", Json.stringify(1.5))
        assertEquals("null", Json.stringify(Double.NaN))
        assertEquals("true", Json.stringify(true))
        assertEquals("""{"x":[1,2]}""", Json.stringify(mapOf("x" to listOf(1, 2))))
        assertEquals("""[1,"b"]""", Json.stringify(JSONArray().put(1).put("b")))
    }

    @Test
    fun badJsonIsNull() {
        assertNull(Json.obj("{"))
        assertNull(Json.obj(""))
        assertNull(Json.arr("[1,"))
        assertEquals(listOf("One", "Two"), Json.strings(Json.arr("""["One","Two",null]""")))
    }

    @Test
    fun typedReadersFallBack() {
        val o = JSONObject("""{"n": "x", "b": 1, "f": 2.5}""")
        assertEquals(9f, Json.float(o, "n", 9f))
        assertTrue(Json.bool(o, "b", false))
        assertFalse(Json.bool(o, "n", false))
        assertEquals(2, Json.int(o, "f", 0))
    }

    @Test
    fun animationSpec() {
        val a = AnimationSpec.parse("""{"opacity":{"kind":"timing","duration":120,"delay":0,"easing":"linear","response":0.35,"damping":0.825}}""")!!
        assertNull(a.frame)
        assertEquals(120L, a.opacity!!.durationMs)
        assertFalse(a.opacity!!.spring)
        val s = AnimationSpec.parse("""{"frame":{"kind":"spring","response":0.5,"damping":0.7}}""")!!
        assertTrue(s.frame!!.spring)
        assertEquals(0.7f, s.frame!!.damping)
        assertNull(AnimationSpec.parse("{}"))
        assertNull(AnimationSpec.parse("nope"))
    }
}
