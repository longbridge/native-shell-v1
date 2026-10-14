package dev.nativeshell

import android.graphics.Bitmap
import android.graphics.Canvas
import android.graphics.Color
import android.view.View
import androidx.test.ext.junit.runners.AndroidJUnit4
import dev.nativeshell.protocol.Prop
import dev.nativeshell.protocol.PropValue
import dev.nativeshell.protocol.ViewType
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith

/** Background, border and radius props draw on every view type, including platform widgets. */
@RunWith(AndroidJUnit4::class)
class DecorationPixelTest : ActivityTest() {
    private val fill = 0xFFFF0000.toInt()
    private val border = 0xFF0000FF.toInt()

    private fun render(v: View): Bitmap {
        val b = Bitmap.createBitmap(v.width, v.height, Bitmap.Config.ARGB_8888)
        b.eraseColor(Color.WHITE)
        v.draw(Canvas(b))
        return b
    }

    private fun near(a: Int, b: Int) = Math.abs(Color.red(a) - Color.red(b)) < 30 &&
        Math.abs(Color.green(a) - Color.green(b)) < 30 && Math.abs(Color.blue(a) - Color.blue(b)) < 30

    @Test
    fun everyTypeDrawsBackgroundBorderAndRadius() {
        val h = harness()
        val types = listOf(
            ViewType.TEXT_INPUT, ViewType.VIEW, ViewType.TEXT, ViewType.IMAGE, ViewType.SWITCH, ViewType.CHECKBOX,
            ViewType.SLIDER, ViewType.PROGRESS, ViewType.SCROLL, ViewType.LIST, ViewType.SEGMENTED, ViewType.SELECT,
            ViewType.DATE_PICKER, ViewType.SHAPE, ViewType.SPINNER, ViewType.HOST_COMPONENT,
        )
        val failures = ArrayList<String>()
        for ((i, type) in types.withIndex()) {
            val id = 100 + i
            onMain {
                h.batch {
                    create(id, type); insert(id, 0, 0); frame(id, 0f, 0f, 200f, 60f)
                    prop(id, Prop.BACKGROUND_COLOR, PropValue.color(fill))
                    prop(id, Prop.BORDER_WIDTHS, PropValue.floats(floatArrayOf(3f, 3f, 3f, 3f)))
                    prop(id, Prop.BORDER_COLOR, PropValue.color(border))
                    prop(id, Prop.CORNER_RADII, PropValue.floats(floatArrayOf(12f, 12f, 12f, 12f)))
                }
            }
            val bmp = onMain { render(h.view(id)) }
            val d = onMain { h.view(id).resources.displayMetrics.density }
            val edge = bmp.getPixel(bmp.width / 2, (1.5f * d).toInt())          // inside the top border
            val inner = bmp.getPixel((20 * d).toInt(), (10 * d).toInt())          // inside the fill, near a corner
            val corner = bmp.getPixel(0, 0)                                      // cut off by the radius
            val name = dev.nativeshell.protocol.ViewType.name(type)
            if (!near(edge, border)) failures += "$name border=${Integer.toHexString(edge)}"
            if (!near(inner, fill) && type != ViewType.SWITCH && type != ViewType.SLIDER && type != ViewType.PROGRESS &&
                type != ViewType.SEGMENTED && type != ViewType.CHECKBOX) failures += "$name fill=${Integer.toHexString(inner)}"
            if (!near(corner, Color.WHITE)) failures += "$name corner=${Integer.toHexString(corner)}"
            // Reset restores the type's own look (no decoration left behind).
            onMain {
                h.batch {
                    for (p in listOf(Prop.BACKGROUND_COLOR, Prop.BORDER_WIDTHS, Prop.BORDER_COLOR, Prop.CORNER_RADII)) prop(id, p, PropValue.NULL)
                }
            }
            val afterBmp = onMain { render(h.view(id)) }
            val after = afterBmp.getPixel(afterBmp.width / 2, (1.5f * d).toInt())
            if (near(after, border)) failures += "$name border not reset"
            onMain { h.batch { delete(id) } }
        }
        assertTrue(failures.joinToString("\n"), failures.isEmpty())
    }

    @Test
    fun fullRadiusRingIsACircleAndRadiiClampLikeCss() {
        val h = harness()
        onMain {
            h.batch {
                create(1, ViewType.VIEW); insert(1, 0, 0); frame(1, 0f, 0f, 24f, 24f)
                prop(1, Prop.CORNER_RADII, PropValue.floats(floatArrayOf(9999f, 9999f, 9999f, 9999f)))
                prop(1, Prop.BORDER_WIDTHS, PropValue.floats(floatArrayOf(2f, 2f, 2f, 2f)))
                prop(1, Prop.BORDER_COLOR, PropValue.color(border))
                create(2, ViewType.VIEW); insert(2, 0, 1); frame(2, 0f, 40f, 100f, 60f)
                prop(2, Prop.CORNER_RADII, PropValue.floats(floatArrayOf(80f, 10f, 10f, 80f)))
                prop(2, Prop.BACKGROUND_COLOR, PropValue.color(fill))
                prop(2, Prop.BORDER_WIDTHS, PropValue.floats(floatArrayOf(2f, 2f, 2f, 2f)))
                prop(2, Prop.BORDER_COLOR, PropValue.color(border))
            }
        }
        val d = onMain { h.view(1).resources.displayMetrics.density }
        val ring = onMain { render(h.view(1)) }
        val c = ring.width / 2f
        val failures = ArrayList<String>()
        // On the circle (radius 11 dp: the middle of the 2 dp stroke) at 8 angles: border color everywhere.
        for (k in 0 until 8) {
            val a = Math.PI * 2 * k / 8
            val x = (c + Math.cos(a) * 11 * d).toInt().coerceIn(0, ring.width - 1)
            val y = (c + Math.sin(a) * 11 * d).toInt().coerceIn(0, ring.height - 1)
            if (!near(ring.getPixel(x, y), border)) failures += "ring at ${k * 45}° = ${Integer.toHexString(ring.getPixel(x, y))}"
        }
        // Inside the square but outside the circle (a lumpy rounded square would paint here), and the center: empty.
        for ((x, y) in listOf((2 * d).toInt() to (2 * d).toInt(), ring.width - (3 * d).toInt() to (2 * d).toInt(), c.toInt() to c.toInt())) {
            if (!near(ring.getPixel(x, y), Color.WHITE)) failures += "outside ring ($x,$y) = ${Integer.toHexString(ring.getPixel(x, y))}"
        }
        // Non-uniform: [80,10,10,80] on 100x60 scales by 60/160, so the left corners are 30 dp, the right ones 3.75 dp.
        val box = onMain { render(h.view(2)) }
        val left = 30f * d
        if (!near(box.getPixel((left * 0.25f).toInt(), (left * 0.25f).toInt()), Color.WHITE)) failures += "left corner not cut"
        if (!near(box.getPixel(box.width - (1 * d).toInt() - 1, (5 * d).toInt()), border)) failures += "right edge border missing"
        if (!near(box.getPixel(box.width / 2, box.height / 2), fill)) failures += "fill missing"
        // The outline (clip/shadow) uses the same clamped shape.
        val outline = onMain {
            val o = android.graphics.Outline()
            h.view(1).outlineProvider.getOutline(h.view(1), o)
            o.radius
        }
        if (Math.abs(outline - 12 * d) > 1f) failures += "outline radius $outline"
        assertTrue(failures.joinToString("\n"), failures.isEmpty())
    }
}
