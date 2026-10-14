package dev.nativeshell.views

import android.graphics.Bitmap
import android.graphics.Canvas
import android.graphics.Color
import android.os.Looper
import android.view.View
import androidx.activity.ComponentActivity
import dev.nativeshell.protocol.Prop
import dev.nativeshell.protocol.PropValue
import dev.nativeshell.protocol.ViewType
import dev.nativeshell.testing.Harness
import org.junit.Assert.assertEquals
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.Robolectric
import org.robolectric.RobolectricTestRunner
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode

/** A list's rows never draw outside its bounds (over a header above it), even in an overflow-visible parent. */
@RunWith(RobolectricTestRunner::class)
@GraphicsMode(GraphicsMode.Mode.NATIVE)
@Config(sdk = [34], qualifiers = "w400dp-h800dp-mdpi")
class ListClipTest {
    @Test
    fun scrolledRowsAreClippedToTheList() {
        val activity = Robolectric.buildActivity(ComponentActivity::class.java).setup().get()
        activity.setTheme(com.google.android.material.R.style.Theme_Material3_DayNight_NoActionBar)
        val h = Harness(activity)
        activity.setContentView(h.surface)
        h.batch {
            create(1, ViewType.VIEW); insert(1, 0, 0); frame(1, 0f, 0f, 400f, 800f)
            prop(1, Prop.BACKGROUND_COLOR, PropValue.color(Color.WHITE))
            create(2, ViewType.LIST); insert(2, 1, 0); frame(2, 0f, 100f, 400f, 200f)
            prop(2, Prop.ITEM_COUNT, PropValue.i32(20))
            prop(2, Prop.ROW_OFFSETS, PropValue.floats(FloatArray(21) { it * 50f }))
            for (i in 0 until 10) {
                val id = 100 + i
                create(id, ViewType.LIST_ROW); prop(id, Prop.ROW_INDEX, PropValue.i32(i)); insert(id, 2, i); frame(id, 0f, 0f, 400f, 50f)
                prop(id, Prop.BACKGROUND_COLOR, PropValue.color(Color.RED))
            }
        }
        h.surface.measure(View.MeasureSpec.makeMeasureSpec(400, View.MeasureSpec.EXACTLY), View.MeasureSpec.makeMeasureSpec(800, View.MeasureSpec.EXACTLY))
        h.surface.layout(0, 0, 400, 800)
        shadowOf(Looper.getMainLooper()).idle()
        val list = h.view(2) as ShellListView
        list.scrollBy(0, 25)
        shadowOf(Looper.getMainLooper()).idle()
        val root = h.view(1)
        val b = Bitmap.createBitmap(400, 800, Bitmap.Config.ARGB_8888)
        root.draw(Canvas(b))
        assertEquals("above the list stays the parent's", Color.WHITE, b.getPixel(200, 95))
        assertEquals("inside the list, rows draw", Color.RED, b.getPixel(200, 110))
        assertEquals("below the list stays the parent's", Color.WHITE, b.getPixel(200, 305))
    }
}
