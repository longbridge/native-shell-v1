package dev.nativeshell.views

import android.graphics.Path
import kotlin.math.PI
import kotlin.math.abs
import kotlin.math.atan2
import kotlin.math.ceil
import kotlin.math.cos
import kotlin.math.sin
import kotlin.math.sqrt
import kotlin.math.tan

/**
 * SVG path data (`M L H V C S Q T A Z`, absolute and relative) to drawing
 * commands. Parsing is separate from [Path] so it is testable on the JVM:
 * [parse] emits to a [Sink]; [toPath] adapts that to an Android [Path].
 * Invalid input stops at the first error, keeping what was parsed (as browsers do).
 */
internal object SvgPath {

    interface Sink {
        fun moveTo(x: Float, y: Float)
        fun lineTo(x: Float, y: Float)
        fun cubicTo(x1: Float, y1: Float, x2: Float, y2: Float, x: Float, y: Float)
        fun quadTo(x1: Float, y1: Float, x: Float, y: Float)
        fun close()
    }

    fun toPath(d: String): Path {
        val p = Path()
        parse(d, object : Sink {
            override fun moveTo(x: Float, y: Float) = p.moveTo(x, y)
            override fun lineTo(x: Float, y: Float) = p.lineTo(x, y)
            override fun cubicTo(x1: Float, y1: Float, x2: Float, y2: Float, x: Float, y: Float) = p.cubicTo(x1, y1, x2, y2, x, y)
            override fun quadTo(x1: Float, y1: Float, x: Float, y: Float) = p.quadTo(x1, y1, x, y)
            override fun close() = p.close()
        })
        return p
    }

    /** Returns false when the data had an error (everything before it was emitted). */
    fun parse(d: String, sink: Sink): Boolean {
        val s = Scanner(d)
        var cx = 0f; var cy = 0f          // current point
        var sx = 0f; var sy = 0f          // subpath start
        var lcx = 0f; var lcy = 0f        // last control point (for S/T)
        var prev = ' '
        var cmd = ' '
        while (true) {
            s.skipSeparators()
            if (s.atEnd) return true
            val c = s.peek()
            if (c.isLetter()) {
                cmd = c
                s.advance()
            } else if (cmd == ' ' || cmd == 'Z' || cmd == 'z') {
                return false
            } else if (cmd == 'M') {
                cmd = 'L' // implicit lineto after moveto
            } else if (cmd == 'm') {
                cmd = 'l'
            }
            val rel = cmd.isLowerCase()
            val ox = if (rel) cx else 0f
            val oy = if (rel) cy else 0f
            when (cmd.uppercaseChar()) {
                'M' -> {
                    val x = s.number() ?: return false; val y = s.number() ?: return false
                    cx = ox + x; cy = oy + y; sx = cx; sy = cy
                    sink.moveTo(cx, cy)
                }
                'L' -> {
                    val x = s.number() ?: return false; val y = s.number() ?: return false
                    cx = ox + x; cy = oy + y
                    sink.lineTo(cx, cy)
                }
                'H' -> {
                    val x = s.number() ?: return false
                    cx = ox + x
                    sink.lineTo(cx, cy)
                }
                'V' -> {
                    val y = s.number() ?: return false
                    cy = oy + y
                    sink.lineTo(cx, cy)
                }
                'C' -> {
                    val n = s.numbers(6) ?: return false
                    sink.cubicTo(ox + n[0], oy + n[1], ox + n[2], oy + n[3], ox + n[4], oy + n[5])
                    lcx = ox + n[2]; lcy = oy + n[3]; cx = ox + n[4]; cy = oy + n[5]
                }
                'S' -> {
                    val n = s.numbers(4) ?: return false
                    val (x1, y1) = if (prev.uppercaseChar() == 'C' || prev.uppercaseChar() == 'S') (2 * cx - lcx) to (2 * cy - lcy) else cx to cy
                    sink.cubicTo(x1, y1, ox + n[0], oy + n[1], ox + n[2], oy + n[3])
                    lcx = ox + n[0]; lcy = oy + n[1]; cx = ox + n[2]; cy = oy + n[3]
                }
                'Q' -> {
                    val n = s.numbers(4) ?: return false
                    sink.quadTo(ox + n[0], oy + n[1], ox + n[2], oy + n[3])
                    lcx = ox + n[0]; lcy = oy + n[1]; cx = ox + n[2]; cy = oy + n[3]
                }
                'T' -> {
                    val n = s.numbers(2) ?: return false
                    val (x1, y1) = if (prev.uppercaseChar() == 'Q' || prev.uppercaseChar() == 'T') (2 * cx - lcx) to (2 * cy - lcy) else cx to cy
                    sink.quadTo(x1, y1, ox + n[0], oy + n[1])
                    lcx = x1; lcy = y1; cx = ox + n[0]; cy = oy + n[1]
                }
                'A' -> {
                    val rx = s.number() ?: return false; val ry = s.number() ?: return false
                    val rot = s.number() ?: return false
                    val large = s.flag() ?: return false; val sweep = s.flag() ?: return false
                    val x = s.number() ?: return false; val y = s.number() ?: return false
                    arc(sink, cx, cy, rx, ry, rot, large, sweep, ox + x, oy + y)
                    cx = ox + x; cy = oy + y
                }
                'Z' -> {
                    sink.close()
                    cx = sx; cy = sy
                }
                else -> return false
            }
            prev = cmd
        }
    }

    /** Endpoint-parameterized elliptical arc as cubic Béziers (SVG 1.1 appendix F.6). */
    private fun arc(sink: Sink, x1: Float, y1: Float, rxIn: Float, ryIn: Float, angle: Float, large: Boolean, sweep: Boolean, x2: Float, y2: Float) {
        if (x1 == x2 && y1 == y2) return
        var rx = abs(rxIn).toDouble()
        var ry = abs(ryIn).toDouble()
        if (rx == 0.0 || ry == 0.0) { sink.lineTo(x2, y2); return }
        val phi = Math.toRadians(angle.toDouble())
        val cosPhi = cos(phi); val sinPhi = sin(phi)
        val dx = (x1 - x2) / 2.0; val dy = (y1 - y2) / 2.0
        val x1p = cosPhi * dx + sinPhi * dy
        val y1p = -sinPhi * dx + cosPhi * dy
        val lambda = (x1p * x1p) / (rx * rx) + (y1p * y1p) / (ry * ry)
        if (lambda > 1) { val k = sqrt(lambda); rx *= k; ry *= k }
        val num = rx * rx * ry * ry - rx * rx * y1p * y1p - ry * ry * x1p * x1p
        val den = rx * rx * y1p * y1p + ry * ry * x1p * x1p
        var coef = if (den == 0.0) 0.0 else sqrt((num / den).coerceAtLeast(0.0))
        if (large == sweep) coef = -coef
        val cxp = coef * rx * y1p / ry
        val cyp = -coef * ry * x1p / rx
        val cx = cosPhi * cxp - sinPhi * cyp + (x1 + x2) / 2.0
        val cy = sinPhi * cxp + cosPhi * cyp + (y1 + y2) / 2.0
        val theta1 = atan2((y1p - cyp) / ry, (x1p - cxp) / rx)
        var dTheta = atan2((-y1p - cyp) / ry, (-x1p - cxp) / rx) - theta1
        if (sweep && dTheta < 0) dTheta += 2 * PI
        if (!sweep && dTheta > 0) dTheta -= 2 * PI
        val segments = ceil(abs(dTheta) / (PI / 2)).toInt().coerceAtLeast(1)
        val delta = dTheta / segments
        val t = 4.0 / 3.0 * tan(delta / 4)
        var th = theta1
        var px = x1.toDouble(); var py = y1.toDouble()
        for (i in 0 until segments) {
            val cos1 = cos(th); val sin1 = sin(th)
            val th2 = th + delta
            val cos2 = cos(th2); val sin2 = sin(th2)
            val ex = cx + rx * cosPhi * cos2 - ry * sinPhi * sin2
            val ey = cy + rx * sinPhi * cos2 + ry * cosPhi * sin2
            val d1x = -rx * cosPhi * sin1 - ry * sinPhi * cos1
            val d1y = -rx * sinPhi * sin1 + ry * cosPhi * cos1
            val d2x = -rx * cosPhi * sin2 - ry * sinPhi * cos2
            val d2y = -rx * sinPhi * sin2 + ry * cosPhi * cos2
            sink.cubicTo(
                (px + t * d1x).toFloat(), (py + t * d1y).toFloat(),
                (ex - t * d2x).toFloat(), (ey - t * d2y).toFloat(),
                if (i == segments - 1) x2 else ex.toFloat(), if (i == segments - 1) y2 else ey.toFloat(),
            )
            px = ex; py = ey; th = th2
        }
    }

    private class Scanner(val s: String) {
        var i = 0
        val atEnd get() = i >= s.length
        fun peek() = s[i]
        fun advance() { i++ }

        fun skipSeparators() {
            while (i < s.length && (s[i].isWhitespace() || s[i] == ',')) i++
        }

        fun numbers(n: Int): FloatArray? {
            val out = FloatArray(n)
            for (k in 0 until n) out[k] = number() ?: return null
            return out
        }

        /** Arc flags are single digits and may be written without separators ("a1 1 0 00 1 1"). */
        fun flag(): Boolean? {
            skipSeparators()
            if (atEnd) return null
            return when (s[i]) {
                '0' -> { i++; false }
                '1' -> { i++; true }
                else -> null
            }
        }

        fun number(): Float? {
            skipSeparators()
            val start = i
            if (i < s.length && (s[i] == '+' || s[i] == '-')) i++
            var digits = false
            while (i < s.length && s[i].isDigit()) { i++; digits = true }
            if (i < s.length && s[i] == '.') {
                i++
                while (i < s.length && s[i].isDigit()) { i++; digits = true }
            }
            if (!digits) { i = start; return null }
            if (i < s.length && (s[i] == 'e' || s[i] == 'E')) {
                val save = i
                i++
                if (i < s.length && (s[i] == '+' || s[i] == '-')) i++
                var exp = false
                while (i < s.length && s[i].isDigit()) { i++; exp = true }
                if (!exp) i = save
            }
            return s.substring(start, i).toFloatOrNull()?.takeIf { it.isFinite() }
        }
    }
}
