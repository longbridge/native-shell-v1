package dev.nativeshell.render

import android.view.InputDevice
import android.view.KeyEvent
import android.view.MotionEvent
import androidx.test.core.app.ApplicationProvider
import dev.nativeshell.protocol.EventMask
import dev.nativeshell.protocol.EventType
import dev.nativeshell.protocol.Prop
import dev.nativeshell.protocol.PropValue
import dev.nativeshell.protocol.ViewType
import dev.nativeshell.testing.Harness
import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config

/** Hover feedback and events, cursor, tooltip and click modifiers from a mouse. */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [34])
class PointerTest {
    private fun hover(action: Int, x: Float = 10f, y: Float = 10f): MotionEvent =
        MotionEvent.obtain(0, 0, action, x, y, 0).apply { source = InputDevice.SOURCE_MOUSE }

    @Test
    fun hoverFeedbackEventsCursorAndTooltip() {
        val h = Harness(ApplicationProvider.getApplicationContext())
        h.batch {
            create(1, ViewType.VIEW); insert(1, 0, 0); frame(1, 0f, 0f, 100f, 100f)
            prop(1, Prop.HOVER_OPACITY, PropValue.f32(0.8f))
            prop(1, Prop.HOVER_BACKGROUND, PropValue.color(0xFF112233.toInt()))
            prop(1, Prop.ACTIVE_OPACITY, PropValue.f32(0.5f))
            prop(1, Prop.CURSOR, PropValue.i32(5))
            prop(1, Prop.TOOLTIP, PropValue.str("Open settings"))
            mask(1, EventMask.HOVER or EventMask.CLICK)
        }
        val v = h.view(1)
        assertNotNull(v.pointerIcon)
        assertEquals("Open settings", v.tooltipText)

        v.dispatchGenericMotionEvent(hover(MotionEvent.ACTION_HOVER_ENTER))
        assertEquals(0.8f, v.alpha, 1e-4f)
        assertEquals(0xFF112233.toInt(), h.mounting.node(1)!!.box!!.pressedFill)
        val e = h.eventsOf(EventType.HOVER).single()
        assertTrue(JSONObject(e.payload).getBoolean("entered"))

        // Pressed wins over hover, and hover comes back on release.
        v.dispatchTouchEvent(MotionEvent.obtain(0, 0, MotionEvent.ACTION_DOWN, 10f, 10f, 0))
        assertEquals(0.5f, v.alpha, 1e-4f)
        val up = MotionEvent.obtain(0, 10, MotionEvent.ACTION_UP, 10f, 10f, KeyEvent.META_CTRL_ON or KeyEvent.META_SHIFT_ON)
        v.dispatchTouchEvent(up)
        assertEquals(0.8f, v.alpha, 1e-4f)
        val click = JSONObject(h.eventsOf(EventType.CLICK).single().payload)
        val mods = click.getJSONObject("modifiers")
        assertTrue(mods.getBoolean("ctrl") && mods.getBoolean("shift") && !mods.getBoolean("cmd"))

        v.dispatchGenericMotionEvent(hover(MotionEvent.ACTION_HOVER_EXIT))
        assertEquals(1f, v.alpha, 1e-4f)
        assertNull(h.mounting.node(1)!!.box!!.pressedFill)
        assertFalse(JSONObject(h.eventsOf(EventType.HOVER).last().payload).getBoolean("entered"))

        // Reset contract.
        h.batch {
            for (p in listOf(Prop.HOVER_OPACITY, Prop.HOVER_BACKGROUND, Prop.CURSOR, Prop.TOOLTIP)) prop(1, p, PropValue.NULL)
            mask(1, 0)
        }
        assertNull(v.pointerIcon)
        assertNull(v.tooltipText)
        v.dispatchGenericMotionEvent(hover(MotionEvent.ACTION_HOVER_ENTER))
        assertEquals(1f, v.alpha, 1e-4f)
        assertEquals(2, h.eventsOf(EventType.HOVER).size)
    }

    @Test
    fun cursorStylesMapToSystemIcons() {
        assertEquals(android.view.PointerIcon.TYPE_HAND, Pointer.iconType(5))
        assertEquals(android.view.PointerIcon.TYPE_TEXT, Pointer.iconType(1))
        assertEquals(android.view.PointerIcon.TYPE_HORIZONTAL_DOUBLE_ARROW, Pointer.iconType(8))
        assertEquals(android.view.PointerIcon.TYPE_NO_DROP, Pointer.iconType(17))
        assertEquals(android.view.PointerIcon.TYPE_ARROW, Pointer.iconType(99))
    }
}
