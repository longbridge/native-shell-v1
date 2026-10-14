package dev.nativeshell.render

import android.os.Looper
import android.view.View
import androidx.activity.ComponentActivity
import androidx.recyclerview.widget.LinearLayoutManager
import dev.nativeshell.protocol.EventType
import dev.nativeshell.protocol.Prop
import dev.nativeshell.protocol.PropValue
import dev.nativeshell.protocol.ViewType
import dev.nativeshell.testing.Harness
import dev.nativeshell.views.ShellListView
import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.Robolectric
import org.robolectric.RobolectricTestRunner
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config

/** `ListAlignment` bottom: opens at the end, stays pinned while there, keeps position when scrolled away. */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [34], qualifiers = "w400dp-h800dp-mdpi")
class ListAlignmentTest {
    private fun idle(h: Harness) {
        h.surface.measure(View.MeasureSpec.makeMeasureSpec(400, View.MeasureSpec.EXACTLY), View.MeasureSpec.makeMeasureSpec(800, View.MeasureSpec.EXACTLY))
        h.surface.layout(0, 0, 400, 800)
        shadowOf(Looper.getMainLooper()).idle()
    }

    private fun setRows(h: Harness, n: Int) = h.batch {
        prop(2, Prop.ITEM_COUNT, PropValue.i32(n))
        prop(2, Prop.ROW_OFFSETS, PropValue.floats(FloatArray(n + 1) { it * 50f }))
    }

    @Test
    fun bottomAlignedListOpensAtTheEndAndStaysPinned() {
        val activity = Robolectric.buildActivity(ComponentActivity::class.java).setup().get()
        activity.setTheme(com.google.android.material.R.style.Theme_Material3_DayNight_NoActionBar)
        val h = Harness(activity)
        activity.setContentView(h.surface)
        h.batch {
            create(1, ViewType.VIEW); insert(1, 0, 0); frame(1, 0f, 0f, 400f, 800f)
            create(2, ViewType.LIST); prop(2, Prop.LIST_ALIGNMENT, PropValue.i32(1)); insert(2, 1, 0); frame(2, 0f, 0f, 400f, 300f)
        }
        setRows(h, 100)
        idle(h)
        val lv = h.view(2) as ShellListView
        val lm = lv.layoutManager as LinearLayoutManager
        assertEquals(99, lm.findLastVisibleItemPosition())
        assertEquals(100, JSONObject(h.eventsOf(EventType.RANGE_CHANGE).last().payload).getInt("end"))

        // At the end: new rows keep the end in view.
        setRows(h, 103)
        idle(h)
        assertEquals(102, lm.findLastVisibleItemPosition())

        // The viewport shrinks (keyboard): the end stays in view.
        h.batch { frame(2, 0f, 0f, 400f, 150f) }
        idle(h)
        assertEquals(102, lm.findLastVisibleItemPosition())

        // Scrolled away (by the user): position kept when rows arrive.
        lm.scrollToPositionWithOffset(10, 0)
        idle(h)
        setRows(h, 110)
        idle(h)
        assertEquals(10, lm.findFirstVisibleItemPosition())
    }
}
