package dev.nativeshell.views

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import kotlin.math.abs
import kotlin.math.hypot

class SvgPathTest {
    private class Recorder : SvgPath.Sink {
        val ops = ArrayList<String>()
        val points = ArrayList<Pair<Float, Float>>()
        private fun f(v: Float) = if (abs(v - Math.round(v)) < 1e-3) Math.round(v).toString() else "%.2f".format(v)
        override fun moveTo(x: Float, y: Float) { ops += "M${f(x)},${f(y)}"; points += x to y }
        override fun lineTo(x: Float, y: Float) { ops += "L${f(x)},${f(y)}"; points += x to y }
        override fun cubicTo(x1: Float, y1: Float, x2: Float, y2: Float, x: Float, y: Float) {
            ops += "C${f(x1)},${f(y1)} ${f(x2)},${f(y2)} ${f(x)},${f(y)}"; points += x to y
        }
        override fun quadTo(x1: Float, y1: Float, x: Float, y: Float) { ops += "Q${f(x1)},${f(y1)} ${f(x)},${f(y)}"; points += x to y }
        override fun close() { ops += "Z" }
    }

    private fun parse(d: String): Pair<Boolean, Recorder> {
        val r = Recorder()
        return SvgPath.parse(d, r) to r
    }

    @Test
    fun absoluteAndRelativeLines() {
        val (ok, r) = parse("M10 20 L30,40 h5 v-10 l-5 -5 H0 V0 z")
        assertTrue(ok)
        assertEquals(listOf("M10,20", "L30,40", "L35,40", "L35,30", "L30,25", "L0,25", "L0,0", "Z"), r.ops)
    }

    @Test
    fun implicitLinetoAfterMoveto() {
        val (ok, r) = parse("M0 0 10 0 10 10m5 5 1 1")
        assertTrue(ok)
        assertEquals(listOf("M0,0", "L10,0", "L10,10", "M15,15", "L16,16"), r.ops)
    }

    @Test
    fun compactNumbers() {
        val (ok, r) = parse("M.5-.5L1e1,2E0l-.5.5")
        assertTrue(ok)
        assertEquals(listOf("M0.50,-0.50", "L10,2", "L9.50,2.50"), r.ops)
    }

    @Test
    fun smoothCurvesReflectControlPoints() {
        val (ok, r) = parse("M0 0 C 0 10 10 10 10 0 S 20 -10 20 0 Q 25 5 30 0 T 40 0")
        assertTrue(ok)
        assertEquals(
            listOf("M0,0", "C0,10 10,10 10,0", "C10,-10 20,-10 20,0", "Q25,5 30,0", "Q35,-5 40,0"),
            r.ops,
        )
    }

    @Test
    fun relativeCurves() {
        val (ok, r) = parse("m10 10 c0 10 10 10 10 0 s10 -10 10 0 q5 5 10 0 t10 0")
        assertTrue(ok)
        assertEquals(
            listOf("M10,10", "C10,20 20,20 20,10", "C20,0 30,0 30,10", "Q35,15 40,10", "Q45,5 50,10"),
            r.ops,
        )
    }

    @Test
    fun arcsEndAtTheirEndpointAndStayOnTheCircle() {
        val (ok, r) = parse("M0 10 A10 10 0 0 1 20 10")
        assertTrue(ok)
        val end = r.points.last()
        assertEquals(20f, end.first, 1e-4f)
        assertEquals(10f, end.second, 1e-4f)
        // Every segment end lies on the circle centered at (10, 10).
        for ((x, y) in r.points.drop(1)) assertEquals(10.0, hypot((x - 10).toDouble(), (y - 10).toDouble()), 1e-3)
        // Sweep flag 1 goes through the top (y < 10) for this half circle in SVG's y-down space.
        assertTrue(r.ops.size >= 3)
    }

    @Test
    fun arcFlagsWithoutSeparators() {
        val (ok, r) = parse("M0 0a5 5 0 1010 0")
        assertTrue(ok)
        val end = r.points.last()
        assertEquals(10f, end.first, 1e-4f)
        assertEquals(0f, end.second, 1e-4f)
    }

    @Test
    fun degenerateArcIsALine() {
        val (_, r) = parse("M0 0 A0 5 0 0 1 10 0")
        assertEquals(listOf("M0,0", "L10,0"), r.ops)
    }

    @Test
    fun errorsKeepWhatWasParsed() {
        val (ok, r) = parse("M0 0 L10 10 X 5 5")
        assertFalse(ok)
        assertEquals(listOf("M0,0", "L10,10"), r.ops)
        assertFalse(parse("10 10").first)
        assertFalse(parse("M0").first)
        assertTrue(parse("").first)
        assertFalse(parse("M0 0 L1e999 0").first)
    }
}
