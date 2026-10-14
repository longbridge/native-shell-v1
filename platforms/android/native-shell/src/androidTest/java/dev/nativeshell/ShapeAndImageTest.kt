package dev.nativeshell

import android.graphics.Bitmap
import android.graphics.Canvas
import android.graphics.Color
import android.widget.ImageView
import androidx.test.ext.junit.runners.AndroidJUnit4
import dev.nativeshell.protocol.Prop
import dev.nativeshell.protocol.PropValue
import dev.nativeshell.protocol.ViewType
import org.junit.Assert.assertEquals
import org.junit.Test
import org.junit.runner.RunWith
import java.io.File

/** Shape fill rules (svg icons) and images from absolute file paths. */
@RunWith(AndroidJUnit4::class)
class ShapeAndImageTest : ActivityTest() {
    // Two clockwise squares: nonzero fills the hole, even-odd leaves it empty.
    private val ring = "M2 2H22V22H2Z M8 8H16V16H8Z"

    private fun render(v: android.view.View): Bitmap =
        Bitmap.createBitmap(v.width, v.height, Bitmap.Config.ARGB_8888).also { it.eraseColor(Color.WHITE); v.draw(Canvas(it)) }

    @Test
    fun fillRuleSelectsNonzeroOrEvenOdd() {
        val h = harness()
        onMain {
            h.batch {
                create(1, ViewType.SHAPE); insert(1, 0, 0); frame(1, 0f, 0f, 48f, 48f)
                prop(1, Prop.PATH_DATA, PropValue.str(ring))
                prop(1, Prop.VIEW_BOX, PropValue.floats(floatArrayOf(0f, 0f, 24f, 24f)))
                prop(1, Prop.FILL_COLOR, PropValue.color(0xFF000000.toInt()))
            }
        }
        val v = onMain { h.view(1) }
        val nonzero = onMain { render(v) }
        assertEquals("nonzero fills the hole", Color.BLACK, nonzero.getPixel(nonzero.width / 2, nonzero.height / 2))
        onMain { h.batch { prop(1, Prop.FILL_RULE, PropValue.i32(1)) } }
        val evenOdd = onMain { render(v) }
        assertEquals("even-odd leaves a hole", Color.WHITE, evenOdd.getPixel(evenOdd.width / 2, evenOdd.height / 2))
        assertEquals(Color.BLACK, evenOdd.getPixel(evenOdd.width / 8, evenOdd.height / 2))
        // Null resets to nonzero, also across a later path change.
        onMain { h.batch { prop(1, Prop.FILL_RULE, PropValue.NULL); prop(1, Prop.PATH_DATA, PropValue.str(ring)) } }
        val reset = onMain { render(v) }
        assertEquals(Color.BLACK, reset.getPixel(reset.width / 2, reset.height / 2))
        // Even-odd survives a path change.
        onMain { h.batch { prop(1, Prop.FILL_RULE, PropValue.i32(1)); prop(1, Prop.PATH_DATA, PropValue.str(ring)) } }
        val again = onMain { render(v) }
        assertEquals(Color.WHITE, again.getPixel(again.width / 2, again.height / 2))
    }

    @Test
    fun imagesLoadFromAbsoluteFilePaths() {
        val file = File(instrumentation.targetContext.filesDir, "native-shell/test/assets/red.png").apply { parentFile!!.mkdirs() }
        file.outputStream().use { Bitmap.createBitmap(4, 4, Bitmap.Config.ARGB_8888).apply { eraseColor(Color.RED) }.compress(Bitmap.CompressFormat.PNG, 100, it) }
        val h = harness()
        onMain {
            h.batch {
                create(1, ViewType.IMAGE); insert(1, 0, 0); frame(1, 0f, 0f, 40f, 40f); prop(1, Prop.IMAGE_SOURCE, PropValue.str(file.absolutePath))
                create(2, ViewType.IMAGE); insert(2, 0, 1); frame(2, 0f, 50f, 40f, 40f); prop(2, Prop.IMAGE_SOURCE, PropValue.str("file://" + file.absolutePath))
            }
        }
        waitFor(what = "absolute path image") { (h.view(1) as ImageView).drawable != null }
        waitFor(what = "file:// image") { (h.view(2) as ImageView).drawable != null }
        val bmp = onMain { render(h.view(1)) }
        assertEquals(Color.RED, bmp.getPixel(bmp.width / 2, bmp.height / 2))
    }
}
