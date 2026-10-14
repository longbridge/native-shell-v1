package dev.nativeshell.views

import android.graphics.Bitmap
import android.graphics.Canvas
import android.graphics.Color
import android.view.View
import androidx.test.core.app.ApplicationProvider
import org.junit.Assert.assertEquals
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode

/** svg() icons (view-box paths) scale with their frame, so they grow with rem-based sizes. */
@RunWith(RobolectricTestRunner::class)
@GraphicsMode(GraphicsMode.Mode.NATIVE)
@Config(sdk = [34])
class ShapeScaleTest {
    private fun draw(sizePx: Int): Bitmap {
        val v = ShapeView(ApplicationProvider.getApplicationContext())
        // A 12×12 square in the bottom-right quadrant of a 24×24 view box.
        v.pathData = "M12 12H24V24H12Z"
        v.viewBox = floatArrayOf(0f, 0f, 24f, 24f)
        v.fill = Color.BLACK
        v.measure(View.MeasureSpec.makeMeasureSpec(sizePx, View.MeasureSpec.EXACTLY), View.MeasureSpec.makeMeasureSpec(sizePx, View.MeasureSpec.EXACTLY))
        v.layout(0, 0, sizePx, sizePx)
        return Bitmap.createBitmap(sizePx, sizePx, Bitmap.Config.ARGB_8888).also { it.eraseColor(Color.WHITE); v.draw(Canvas(it)) }
    }

    @Test
    fun viewBoxPathsFillTheirFrameAtAnySize() {
        for (size in listOf(48, 96, 154)) {
            val b = draw(size)
            assertEquals("size $size: top-left quadrant empty", Color.WHITE, b.getPixel(size / 4, size / 4))
            assertEquals("size $size: bottom-right quadrant filled", Color.BLACK, b.getPixel(size * 3 / 4, size * 3 / 4))
            assertEquals("size $size: square reaches the far corner", Color.BLACK, b.getPixel(size - 1, size - 1))
        }
    }
}
