package dev.nativeshell

import androidx.test.ext.junit.runners.AndroidJUnit4
import dev.nativeshell.protocol.BatchFlags
import dev.nativeshell.protocol.Command
import dev.nativeshell.protocol.Prop
import dev.nativeshell.protocol.PropValue
import dev.nativeshell.protocol.ViewType
import dev.nativeshell.render.Motion
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith
import kotlin.math.roundToInt

/** With animations turned off, movement applies instantly and fades stay short. */
@RunWith(AndroidJUnit4::class)
class ReducedMotionTest : ActivityTest() {
    private val anim = """{"frame":{"kind":"spring","response":0.5,"damping":0.8,"delay":0},"opacity":{"kind":"timing","duration":800,"delay":0,"easing":"linear"}}"""

    @Before fun reduce() { Motion.override = true }
    @After fun restore() { Motion.override = null }

    @Test
    fun animatedBatchesMoveInstantlyAndFadeShort() {
        val h = harness()
        onMain {
            h.batch {
                create(1, ViewType.VIEW); insert(1, 0, 0); frame(1, 0f, 0f, 100f, 100f)
                prop(1, Prop.ANIMATION, PropValue.json(anim))
            }
        }
        val v = onMain { h.view(1) }
        val d = onMain { v.resources.displayMetrics.density }
        onMain {
            h.batch(BatchFlags.ANIMATED) { frame(1, 50f, 60f, 200f, 120f); prop(1, Prop.OPACITY, PropValue.f32(0.2f)) }
            // The frame is final in the same main-thread turn: no frame animator.
            assertEquals((50 * d).roundToInt(), v.left)
            assertEquals((200 * d).roundToInt(), v.width)
            assertNull(v.getTag(R.id.nsh_frame_animator))
        }
        // The fade still runs, but within the short reduced-motion duration.
        val t0 = System.currentTimeMillis()
        waitFor(timeoutMs = 2000, what = "short fade (alpha=${onMain { v.alpha }})") { kotlin.math.abs(v.alpha - 0.2f) < 1e-3f }
        assertTrue("fade took ${System.currentTimeMillis() - t0} ms", System.currentTimeMillis() - t0 < 600)
    }

    @Test
    fun pagesCrossFadeInsteadOfSliding() {
        val h = harness()
        onMain {
            h.batch {
                create(10, ViewType.PAGE); insert(10, 0, 0); frame(10, 0f, 0f, 400f, 800f)
                command(10, Command.PRESENT_PAGE, """{"animated":true}""")
            }
            assertEquals(0f, h.view(10).translationX)
            assertEquals(0f, h.view(0).translationX)
        }
        waitFor(timeoutMs = 1000, what = "page faded in") { h.view(10).alpha == 1f }
        val page = onMain { h.view(10) }
        onMain { h.batch { command(10, Command.DISMISS_PAGE, """{"animated":true}"""); delete(10) } }
        assertEquals(0f, onMain { page.translationX })
        waitFor(timeoutMs = 1000, what = "page removed") { !page.isAttachedToWindow }
    }
}
