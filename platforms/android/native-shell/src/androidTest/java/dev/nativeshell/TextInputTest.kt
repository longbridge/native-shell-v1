package dev.nativeshell

import android.view.inputmethod.EditorInfo
import android.view.inputmethod.InputConnection
import androidx.test.ext.junit.runners.AndroidJUnit4
import dev.nativeshell.protocol.Command
import dev.nativeshell.protocol.EventMask
import dev.nativeshell.protocol.EventType
import dev.nativeshell.protocol.Prop
import dev.nativeshell.protocol.PropValue
import dev.nativeshell.protocol.ViewType
import dev.nativeshell.testing.Harness
import dev.nativeshell.views.ShellEditText
import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith

/** The controlled-input event-count protocol, with IME composition driven through an InputConnection. */
@RunWith(AndroidJUnit4::class)
class TextInputTest : ActivityTest() {
    private lateinit var h: Harness
    private lateinit var field: ShellEditText
    private lateinit var ic: InputConnection

    @Before
    fun mount() {
        h = harness()
        onMain {
            h.batch {
                create(1, ViewType.TEXT_INPUT); insert(1, 0, 0); frame(1, 16f, 100f, 300f, 44f)
                mask(1, EventMask.CHANGE or EventMask.SUBMIT or EventMask.FOCUS or EventMask.BLUR)
            }
            field = h.view(1) as ShellEditText
            field.requestFocus()
            ic = field.onCreateInputConnection(EditorInfo())!!
        }
    }

    private fun changes() = h.eventsOf(EventType.CHANGE).map { JSONObject(it.payload) }

    private fun textValue(text: String, count: Int) = onMain {
        h.batch { prop(1, Prop.TEXT_VALUE, PropValue.json(JSONObject().put("text", text).put("count", count).toString())) }
    }

    @Test
    fun typingReportsTextSelectionAndCount() {
        onMain { ic.commitText("hi", 1) }
        val c = changes().last()
        assertEquals("hi", c.getString("text"))
        assertEquals(1, c.getInt("count"))
        assertFalse(c.getBoolean("composing"))
        assertEquals(2, c.getJSONArray("selection").getInt(0))
        // The script echoes the value it saw: no rewrite, no extra event.
        textValue("hi", 1)
        assertEquals(1, changes().size)
        assertEquals("hi", onMain { field.text.toString() })
    }

    @Test
    fun scriptTransformsAreAppliedWhenCurrent() {
        onMain { ic.commitText("abc", 1) }
        textValue("ABC", 1)
        assertEquals("ABC", onMain { field.text.toString() })
        // Writing the script's value is not a user edit.
        assertEquals(1, changes().size)
    }

    @Test
    fun staleValuesAreRejected() {
        onMain { ic.commitText("a", 1); ic.commitText("b", 1) }
        assertEquals(2, changes().last().getInt("count"))
        // The script answered the first edit only: applying it would drop "b".
        textValue("A", 1)
        assertEquals("ab", onMain { field.text.toString() })
        textValue("AB", 2)
        assertEquals("AB", onMain { field.text.toString() })
    }

    @Test
    fun compositionIsNeverInterrupted() {
        // Pinyin: "ni" composing, then the candidate "你" committed.
        onMain { ic.setComposingText("n", 1); ic.setComposingText("ni", 1) }
        val composing = changes().last()
        assertTrue(composing.getBoolean("composing"))
        assertEquals("ni", composing.getString("text"))
        val count = composing.getInt("count")
        // A current value arriving mid-composition waits.
        textValue("NI", count)
        assertEquals("ni", onMain { field.text.toString() })
        assertTrue(onMain { field.isComposing })
        onMain { ic.commitText("你", 1) }
        val committed = changes().last()
        assertFalse(committed.getBoolean("composing"))
        assertEquals("你", committed.getString("text"))
        // The deferred value is now stale (count moved on) and is dropped.
        assertEquals("你", onMain { field.text.toString() })
        textValue("你好", committed.getInt("count"))
        assertEquals("你好", onMain { field.text.toString() })
    }

    @Test
    fun compositionEndIsReportedAndSupersedesDeferredValues() {
        // Kana: "か" composing; the script's value with the current count waits for the end.
        onMain { ic.setComposingText("か", 1) }
        val count = changes().last().getInt("count")
        onMain { ic.finishComposingText() }
        // Ending composition reports a final, non-composing change.
        val fin = changes().last()
        assertFalse(fin.getBoolean("composing"))
        assertEquals(count + 1, fin.getInt("count"))
        onMain { ic.setComposingText("き", 1) }
        val c2 = changes().last().getInt("count")
        textValue("カキ", c2)
        assertEquals("かき", onMain { field.text.toString() })
        onMain { ic.finishComposingText() }
        // The pending value had count c2, the composition end bumped the counter: stale, dropped.
        assertEquals("かき", onMain { field.text.toString() })
        // The script answers the final change and its value applies.
        textValue("カキ", changes().last().getInt("count"))
        assertEquals("カキ", onMain { field.text.toString() })
    }

    @Test
    fun maxLengthAndAllowedCharsFilterInput() {
        onMain { h.batch { prop(1, Prop.MAX_LENGTH, PropValue.i32(4)) } }
        onMain { ic.commitText("abcdefgh", 1) }
        assertEquals("abcd", onMain { field.text.toString() })
        onMain {
            h.batch {
                prop(1, Prop.MAX_LENGTH, PropValue.NULL)
                prop(1, Prop.ALLOWED_CHARS, PropValue.str("0123456789"))
            }
            field.setText("")
        }
        onMain { ic.commitText("1a2b3", 1) }
        assertEquals("123", onMain { field.text.toString() })
    }

    @Test
    fun submitAndFocusEvents() {
        onMain { h.batch { prop(1, Prop.RETURN_KEY, PropValue.i32(1)) } }
        onMain { ic.commitText("go", 1); ic.performEditorAction(EditorInfo.IME_ACTION_DONE) }
        assertEquals(1, h.eventsOf(EventType.SUBMIT).size)
        onMain { h.batch { command(1, Command.BLUR) } }
        assertFalse(onMain { field.hasFocus() })
        assertTrue(h.eventsOf(EventType.BLUR).isNotEmpty())
        onMain { h.batch { command(1, Command.FOCUS) } }
        assertTrue(onMain { field.hasFocus() })
        assertTrue(h.eventsOf(EventType.FOCUS).isNotEmpty())
    }
}
