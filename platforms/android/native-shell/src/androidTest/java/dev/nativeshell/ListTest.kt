package dev.nativeshell

import androidx.test.ext.junit.runners.AndroidJUnit4
import dev.nativeshell.protocol.Command
import dev.nativeshell.protocol.EventType
import dev.nativeshell.protocol.Prop
import dev.nativeshell.protocol.PropValue
import dev.nativeshell.protocol.ViewType
import dev.nativeshell.testing.Harness
import dev.nativeshell.views.ShellListView
import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertSame
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith

/** A 10k-row virtualized list: placeholders, rows hosted in cells, range reports while scrolling. */
@RunWith(AndroidJUnit4::class)
class ListTest : ActivityTest() {
    private val count = 10_000
    private val rowHeight = 48f
    private val list = 2

    private fun Harness.renderWindow(first: Int, last: Int) {
        // Like the core: rows outside the window are deleted, rows inside exist with their index.
        batch {
            for (i in first until last) {
                val id = 1000 + i
                if (mounting.node(id) == null) {
                    create(id, ViewType.LIST_ROW); prop(id, Prop.ROW_INDEX, PropValue.i32(i))
                    insert(id, list, 0); frame(id, 0f, 0f, 360f, rowHeight)
                    create(id + 100_000, ViewType.TEXT); insert(id + 100_000, id, 0); frame(id + 100_000, 16f, 12f, 200f, 24f)
                    prop(id + 100_000, Prop.ATTRIBUTED_TEXT, PropValue.json("""{"runs":[{"text":"Row $i","size":16}]}"""))
                    prop(id, Prop.TEST_ID, PropValue.str("row-$i"))
                }
            }
        }
    }

    private fun lastRange(h: Harness): Pair<Int, Int> {
        val e = h.eventsOf(EventType.RANGE_CHANGE).last()
        val o = JSONObject(e.payload)
        assertEquals(list, e.node)
        return o.getInt("first") to o.getInt("end")
    }

    @Test
    fun tenThousandRowsReportRangesWhileScrolling() {
        val h = harness()
        onMain {
            h.batch {
                create(1, ViewType.VIEW); insert(1, 0, 0); frame(1, 0f, 0f, 360f, 640f)
                create(list, ViewType.LIST); insert(list, 1, 0); frame(list, 0f, 0f, 360f, 600f)
                prop(list, Prop.ITEM_COUNT, PropValue.i32(count))
                prop(list, Prop.ROW_OFFSETS, PropValue.floats(FloatArray(count + 1) { it * rowHeight }))
                prop(list, Prop.PREFETCH, PropValue.f32(5f))
            }
            h.renderWindow(0, 20)
        }
        instrumentation.waitForIdleSync()
        waitFor(what = "first range") { h.eventsOf(EventType.RANGE_CHANGE).isNotEmpty() }
        val (f0, l0) = lastRange(h)
        assertEquals(0, f0)
        assertTrue("last=$l0", l0 in 13..25) // ~600/48 visible + prefetch, exclusive end
        val lv = onMain { h.view(list) as ShellListView }
        // Row 3 is hosted by a visible cell.
        val row3 = onMain { h.surface.findByTestId("row-3")!! }
        assertSame(lv, onMain { row3.parent.parent })

        onMain { h.batch { command(list, Command.SCROLL_TO_INDEX, """{"index":5000,"position":"top","animated":false}""") } }
        waitFor(what = "range at 5000") { lastRange(h).first in 4990..5000 }
        val (f1, l1) = lastRange(h)
        assertEquals(4995, f1)
        assertTrue(l1 in 5010..5025)
        // Unrendered positions are placeholders at the estimated height.
        onMain {
            val cell = lv.findViewHolderForAdapterPosition(5000)!!.itemView
            assertEquals(Math.round(rowHeight * lv.resources.displayMetrics.density), cell.height)
            assertEquals(0, (cell as android.view.ViewGroup).childCount)
        }
        onMain { h.renderWindow(f1, l1) }
        onMain {
            val cell = lv.findViewHolderForAdapterPosition(5000)!!.itemView as android.view.ViewGroup
            assertEquals(1, cell.childCount)
        }
        // Scroll by hand: ranges keep coming, deduplicated.
        val before = h.eventsOf(EventType.RANGE_CHANGE).size
        repeat(20) { onMain { lv.scrollBy(0, 400) }; instrumentation.waitForIdleSync() }
        val after = h.eventsOf(EventType.RANGE_CHANGE)
        assertTrue(after.size > before)
        assertTrue(after.zipWithNext().all { (a, b) -> a.payload != b.payload })
        val (f2, _) = lastRange(h)
        assertTrue("f2=$f2", f2 > f1)
        // Height corrections from measured rows move later cells.
        onMain {
            h.batch { prop(list, Prop.ROW_OFFSETS, PropValue.floats(FloatArray(count + 1) { it * 60f })) }
        }
        instrumentation.waitForIdleSync()
        onMain {
            val vh = lv.findViewHolderForAdapterPosition(lastRange(h).first + 6)
            if (vh != null) assertEquals(Math.round(60f * lv.resources.displayMetrics.density), vh.itemView.height)
        }
    }
}
