package dev.nativeshell.render

import android.view.MotionEvent
import android.view.View
import android.widget.ImageView
import androidx.test.core.app.ApplicationProvider
import dev.nativeshell.protocol.EventMask
import dev.nativeshell.protocol.EventType
import dev.nativeshell.protocol.Prop
import dev.nativeshell.protocol.PropValue
import dev.nativeshell.protocol.Protocol
import dev.nativeshell.protocol.ViewType
import dev.nativeshell.testing.Harness
import dev.nativeshell.views.ShellEditText
import dev.nativeshell.views.ShellTextView
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertSame
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import kotlin.math.roundToInt

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [34], qualifiers = "w400dp-h800dp-xxhdpi")
class MountingTest {
    private lateinit var h: Harness
    private val d get() = h.surface.resources.displayMetrics.density

    @Before
    fun setUp() {
        org.robolectric.shadows.ShadowLog.stream = System.out
        h = Harness(ApplicationProvider.getApplicationContext())
    }

    @Test
    fun createsInsertsAndPlacesViews() {
        h.batch {
            create(1, ViewType.VIEW); insert(1, Protocol.ROOT_NODE, 0); frame(1, 0f, 0f, 400f, 800f)
            create(2, ViewType.VIEW); insert(2, 1, 0); frame(2, 10f, 20f, 100f, 50.5f)
            create(3, ViewType.TEXT); insert(3, 1, 1); frame(3, 10f, 80f, 100f, 20f)
        }
        val box = h.view(2)
        assertSame(h.view(1), box.parent)
        assertEquals((10 * d).roundToInt(), box.left)
        assertEquals((20 * d).roundToInt(), box.top)
        assertEquals((100 * d).roundToInt(), box.width)
        assertEquals(((20 + 50.5f) * d).roundToInt() - (20 * d).roundToInt(), box.height)
        assertTrue(h.view(3) is ShellTextView)
        assertEquals(4, h.mounting.nodeCount) // root + 3
    }

    @Test
    fun movesReorderNativeChildren() {
        h.batch {
            create(1, ViewType.VIEW); insert(1, 0, 0)
            for (i in 2..4) { create(i, ViewType.VIEW); insert(i, 1, i - 2) }
            move(4, 1, 2, 0)
        }
        val parent = h.view(1) as android.view.ViewGroup
        assertSame(h.view(4), parent.getChildAt(0))
        assertSame(h.view(2), parent.getChildAt(1))
        assertSame(h.view(3), parent.getChildAt(2))
    }

    @Test
    fun releasedViewsAreResetAndPooled() {
        h.batch {
            create(1, ViewType.VIEW); insert(1, 0, 0); frame(1, 0f, 0f, 100f, 100f)
            prop(1, Prop.BACKGROUND_COLOR, PropValue.color(0xFF00FF00.toInt()))
            prop(1, Prop.CORNER_RADII, PropValue.floats(floatArrayOf(4f, 4f, 4f, 4f)))
            prop(1, Prop.OPACITY, PropValue.f32(0.5f))
            prop(1, Prop.HIDDEN, PropValue.bool(true))
            prop(1, Prop.TRANSFORM, PropValue.json("""{"translate_x":5,"scale":2,"rotate":45}"""))
            prop(1, Prop.SHADOW, PropValue.json("""{"color":4278190080,"x":0,"y":2,"blur":8,"spread":0}"""))
            prop(1, Prop.CLIPS_TO_BOUNDS, PropValue.bool(true))
            prop(1, Prop.A11Y_LABEL, PropValue.str("Box"))
            prop(1, Prop.TEST_ID, PropValue.str("box"))
            prop(1, Prop.HIT_SLOP, PropValue.floats(floatArrayOf(8f, 8f, 8f, 8f)))
            mask(1, EventMask.CLICK)
        }
        val v = h.view(1)
        assertNotNull(v.background)
        assertEquals(0.5f, v.alpha)
        assertEquals(View.INVISIBLE, v.visibility)
        assertTrue(v.elevation > 0f)
        assertEquals("Box", v.contentDescription)
        assertTrue(v.hasOnClickListeners())
        h.batch { delete(1) }
        assertEquals(1, h.mounting.pooledCount(ViewType.VIEW))
        h.batch { create(9, ViewType.VIEW); insert(9, 0, 0) }
        val reused = h.view(9)
        assertSame(v, reused)
        assertNull(reused.background)
        assertEquals(1f, reused.alpha)
        assertEquals(View.VISIBLE, reused.visibility)
        assertEquals(0f, reused.translationX)
        assertEquals(1f, reused.scaleX)
        assertEquals(0f, reused.rotation)
        assertEquals(0f, reused.elevation)
        assertFalse(reused.clipToOutline)
        assertNull(reused.contentDescription)
        assertNull(reused.getTag(dev.nativeshell.R.id.nsh_test_id))
        assertNull(reused.getTag(dev.nativeshell.R.id.nsh_hit_slop))
        assertFalse(reused.hasOnClickListeners())
        assertEquals(0, h.mounting.pooledCount(ViewType.VIEW))
    }

    @Test
    fun deleteReleasesTheWholeSubtree() {
        h.batch {
            create(1, ViewType.VIEW); insert(1, 0, 0)
            create(2, ViewType.VIEW); insert(2, 1, 0)
            create(3, ViewType.IMAGE); insert(3, 2, 0)
            delete(1)
        }
        assertEquals(1, h.mounting.nodeCount)
        assertEquals(2, h.mounting.pooledCount(ViewType.VIEW))
        assertEquals(1, h.mounting.pooledCount(ViewType.IMAGE))
    }

    @Test
    fun badBatchesAreRejectedWhole() {
        h.batch { create(1, ViewType.VIEW); insert(1, 0, 0) }
        val good = dev.nativeshell.testing.BatchBuilder(2, 0).create(2, ViewType.VIEW).insert(2, 0, 1).bytes()
        val truncated = good.copyOf(good.size - 4)
        h.applyBytes(truncated)
        assertNull(h.mounting.node(2))
        h.applyBytes(byteArrayOf(1, 2, 3, 4, 5))
        h.applyBytes(good)
        assertNotNull(h.mounting.node(2))
    }

    @Test
    fun invalidMutationsAreSkippedNotFatal() {
        h.batch {
            insert(42, 0, 0)                // unknown node
            create(1, ViewType.VIEW)
            insert(1, 77, 0)                // unknown parent
            insert(1, 0, 99)                // index clamped
            create(2, ViewType.TEXT)
            insert(2, 1, 0)                 // into a view: fine
            create(3, ViewType.VIEW)
            insert(3, 2, 0)                 // text has no children: logged
            prop(1, Prop.ITEM_COUNT, PropValue.i32(3)) // not a View prop: ignored
            frame(1, Float.NaN, 0f, Float.POSITIVE_INFINITY, -5f)
        }
        val v = h.view(1)
        assertSame(h.surface.renderer!!.mounting.root.view, v.parent)
        assertEquals(0, v.width)
    }

    @Test
    fun clicksAreSentWithTheAppliedSeq() {
        h.batch {
            create(1, ViewType.VIEW); insert(1, 0, 0); frame(1, 0f, 0f, 100f, 100f)
            mask(1, EventMask.CLICK or EventMask.PRESS_IN or EventMask.PRESS_OUT)
            prop(1, Prop.ACTIVE_OPACITY, PropValue.f32(0.5f))
        }
        val v = h.view(1)
        val down = MotionEvent.obtain(0, 0, MotionEvent.ACTION_DOWN, 30f, 60f, 0)
        v.dispatchTouchEvent(down)
        assertEquals(0.5f, v.alpha)
        v.dispatchTouchEvent(MotionEvent.obtain(0, 10, MotionEvent.ACTION_UP, 30f, 60f, 0))
        assertEquals(1f, v.alpha)
        val types = h.engine.events.map { it.type }
        assertEquals(listOf(EventType.PRESS_IN, EventType.PRESS_OUT, EventType.CLICK), types)
        val click = h.engine.events.last()
        assertEquals(1, click.node)
        assertEquals(1, click.seq)
        assertTrue(click.payload, click.payload.contains("\"count\":1"))
        assertTrue(click.payload, click.payload.contains("\"x\":${30f / d}"))
    }

    @Test
    fun viewsWithoutHandlersDoNotConsumeTouches() {
        h.batch { create(1, ViewType.VIEW); insert(1, 0, 0); frame(1, 0f, 0f, 100f, 100f) }
        val v = h.view(1)
        assertFalse(v.dispatchTouchEvent(MotionEvent.obtain(0, 0, MotionEvent.ACTION_DOWN, 5f, 5f, 0)))
    }

    @Test
    fun testIdLookup() {
        h.batch { create(1, ViewType.IMAGE); insert(1, 0, 0); prop(1, Prop.TEST_ID, PropValue.str("avatar")) }
        assertTrue(h.surface.findByTestId("avatar") is ImageView)
        assertNull(h.surface.findByTestId("nope"))
    }

    @Test
    fun textInputPropsResetToDefaults() {
        h.batch {
            create(1, ViewType.TEXT_INPUT); insert(1, 0, 0); frame(1, 0f, 0f, 200f, 40f)
        }
        val fresh = h.view(1) as ShellEditText
        val freshType = fresh.inputType
        val freshIme = fresh.imeOptions
        val freshSize = fresh.textSize
        val freshFilters = fresh.filters.size
        h.batch {
            prop(1, Prop.PLACEHOLDER, PropValue.str("Name"))
            prop(1, Prop.SECURE, PropValue.bool(true))
            prop(1, Prop.KEYBOARD_TYPE, PropValue.i32(3))
            prop(1, Prop.MAX_LENGTH, PropValue.i32(4))
            prop(1, Prop.RETURN_KEY, PropValue.i32(4))
            prop(1, Prop.TEXT_STYLE, PropValue.json("""{"size":24,"color":4294901760}"""))
            prop(1, Prop.TEXT_VALUE, PropValue.json("""{"text":"hello","count":0}"""))
        }
        assertEquals("Name", fresh.hint)
        assertEquals("hell", fresh.text.toString())
        h.batch {
            for (p in listOf(Prop.PLACEHOLDER, Prop.SECURE, Prop.KEYBOARD_TYPE, Prop.MAX_LENGTH, Prop.RETURN_KEY, Prop.TEXT_STYLE)) {
                prop(1, p, PropValue.NULL)
            }
        }
        assertNull(fresh.hint)
        assertEquals(freshType, fresh.inputType)
        assertEquals(freshIme, fresh.imeOptions)
        assertEquals(freshSize, fresh.textSize)
        assertEquals(freshFilters, fresh.filters.size)
    }
}
