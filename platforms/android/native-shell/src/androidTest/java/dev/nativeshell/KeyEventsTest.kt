package dev.nativeshell

import android.view.KeyEvent
import androidx.test.ext.junit.runners.AndroidJUnit4
import dev.nativeshell.protocol.EventMask
import dev.nativeshell.protocol.EventType
import dev.nativeshell.protocol.Protocol
import dev.nativeshell.protocol.ViewType
import dev.nativeshell.views.ShellEditText
import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith

/** Hardware keys reach the focused listening node, else the surface root. */
@RunWith(AndroidJUnit4::class)
class KeyEventsTest : ActivityTest() {

    private fun ctrlS(action: Int) = KeyEvent(0, 0, action, KeyEvent.KEYCODE_S, 0, KeyEvent.META_CTRL_ON or KeyEvent.META_CTRL_LEFT_ON)

    @Test
    fun ctrlSGoesToTheRootWithoutFocusedListener() {
        val h = harness()
        onMain { h.batch { create(1, ViewType.VIEW); insert(1, 0, 0) } }
        waitFor(what = "surface focus") { h.surface.hasFocus() }
        instrumentation.sendKeySync(ctrlS(KeyEvent.ACTION_DOWN))
        instrumentation.sendKeySync(ctrlS(KeyEvent.ACTION_UP))
        instrumentation.waitForIdleSync()
        val keys = h.eventsOf(EventType.KEY)
        assertEquals(2, keys.size)
        assertEquals(Protocol.ROOT_NODE, keys[0].node)
        val down = JSONObject(keys[0].payload)
        assertEquals("s", down.getString("key"))
        assertEquals("down", down.getString("phase"))
        val m = down.getJSONObject("modifiers")
        assertTrue(m.getBoolean("ctrl"))
        assertFalse(m.getBoolean("shift") || m.getBoolean("alt") || m.getBoolean("cmd"))
        assertEquals("up", JSONObject(keys[1].payload).getString("phase"))
    }

    @Test
    fun focusedFieldWithKeyMaskReceivesKeysAndStillTypes() {
        val h = harness()
        onMain {
            h.batch {
                create(2, ViewType.TEXT_INPUT); insert(2, 0, 0); frame(2, 0f, 100f, 300f, 44f)
                mask(2, EventMask.KEY or EventMask.CHANGE)
            }
            (h.view(2) as ShellEditText).requestFocus()
        }
        instrumentation.sendKeySync(KeyEvent(KeyEvent.ACTION_DOWN, KeyEvent.KEYCODE_A))
        instrumentation.sendKeySync(KeyEvent(KeyEvent.ACTION_UP, KeyEvent.KEYCODE_A))
        instrumentation.sendKeySync(ctrlS(KeyEvent.ACTION_DOWN))
        instrumentation.waitForIdleSync()
        val keys = h.eventsOf(EventType.KEY)
        assertTrue(keys.all { it.node == 2 })
        val a = JSONObject(keys[0].payload)
        assertEquals("a", a.getString("key"))
        assertEquals("a", a.getString("key_char"))
        assertEquals("s", JSONObject(keys[2].payload).getString("key"))
        // Keys are not consumed: the field still received the letter.
        assertEquals("a", onMain { (h.view(2) as ShellEditText).text.toString() })
    }
}
