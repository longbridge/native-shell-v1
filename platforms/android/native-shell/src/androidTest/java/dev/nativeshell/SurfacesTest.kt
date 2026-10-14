package dev.nativeshell

import android.app.Dialog
import android.content.DialogInterface
import androidx.appcompat.app.AlertDialog
import androidx.test.ext.junit.runners.AndroidJUnit4
import com.google.android.material.bottomsheet.BottomSheetDialog
import dev.nativeshell.protocol.Command
import dev.nativeshell.protocol.EventType
import dev.nativeshell.protocol.Prop
import dev.nativeshell.protocol.PropValue
import dev.nativeshell.protocol.ViewType
import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith

/** Pages and system back, sheets, dialogs, alerts and toasts. */
@RunWith(AndroidJUnit4::class)
class SurfacesTest : ActivityTest() {

    @Test
    fun pagePushPopAndBackHandling() {
        val h = harness()
        onMain {
            h.batch {
                create(1, ViewType.VIEW); insert(1, 0, 0); frame(1, 0f, 0f, 400f, 800f)
                create(10, ViewType.PAGE); prop(10, Prop.TITLE, PropValue.str("Detail")); insert(10, 0, 1); frame(10, 0f, 0f, 400f, 800f)
                create(11, ViewType.TEXT); insert(11, 10, 0); frame(11, 16f, 16f, 100f, 20f)
                prop(11, Prop.ATTRIBUTED_TEXT, PropValue.json("""{"runs":[{"text":"Detail page"}]}"""))
                command(10, Command.PRESENT_PAGE)
            }
        }
        assertEquals(1, onMain { h.surface.pageCount })
        // The page is not a child of the root content view.
        assertTrue(onMain { h.view(10).parent !== h.view(0) })
        waitFor(what = "slide in") { h.view(10).translationX == 0f }

        // Back blocked by the script: it is told, nothing pops.
        onMain { h.batch { prop(10, Prop.CAN_GO_BACK, PropValue.bool(false)) } }
        onMain { activity().onBackPressedDispatcher.onBackPressed() }
        assertEquals(1, h.eventsOf(EventType.BACK_PRESSED).size)
        assertEquals(1, onMain { h.surface.pageCount })
        assertTrue(!activity().isFinishing)

        // Back allowed: the page pops locally at once and the script is told.
        val pageView = onMain { h.view(10) }
        onMain { h.batch { prop(10, Prop.CAN_GO_BACK, PropValue.bool(true)) } }
        onMain { activity().onBackPressedDispatcher.onBackPressed() }
        assertEquals(10, h.eventsOf(EventType.DISMISS).single().node)
        assertEquals(0, onMain { h.surface.pageCount })

        // The core follows with DismissPage + Delete; the exit transition is not restarted.
        onMain { h.batch { command(10, Command.DISMISS_PAGE, "{\"animated\":true}"); delete(10) } }
        waitFor(what = "page view detached", timeoutMs = 3000) { !pageView.isAttachedToWindow }
        assertEquals(0f, onMain { h.view(0).translationX })
    }

    @Test
    fun corePopRunsTheExitTransitionBeforeRemoving() {
        val h = harness()
        onMain {
            h.batch {
                create(10, ViewType.PAGE); insert(10, 0, 0); frame(10, 0f, 0f, 400f, 800f)
                command(10, Command.PRESENT_PAGE, "{\"animated\":true}")
            }
        }
        waitFor(what = "slide in") { h.view(10).translationX == 0f }
        val pageView = onMain { h.view(10) }
        onMain { h.batch { command(10, Command.DISMISS_PAGE, "{\"animated\":true}"); delete(10) } }
        assertEquals(0, onMain { h.surface.pageCount })
        assertTrue(onMain { pageView.isAttachedToWindow })
        waitFor(what = "page view detached", timeoutMs = 3000) { !pageView.isAttachedToWindow }
    }

    @Test
    fun keyboardInsetReachesPagesAndOverlaysAreOpaque() {
        val h = harness()
        onMain {
            h.batch {
                create(1, ViewType.VIEW); insert(1, 0, 0); frame(1, 0f, 0f, 400f, 800f)
                create(10, ViewType.PAGE); insert(10, 0, 1); frame(10, 0f, 0f, 400f, 800f)
                command(10, Command.PRESENT_PAGE, "{\"animated\":false}")
            }
        }
        val d = onMain { h.surface.resources.displayMetrics.density }
        fun keyboard() = JSONObject(h.engine.environments.last()).getDouble("keyboard_height")
        val ime = (300 * d).toInt()
        onMain {
            val insets = androidx.core.view.WindowInsetsCompat.Builder()
                .setInsets(androidx.core.view.WindowInsetsCompat.Type.ime(), androidx.core.graphics.Insets.of(0, 0, 0, ime)).build()
            androidx.core.view.ViewCompat.dispatchApplyWindowInsets(h.surface, insets)
        }
        // The surface reaches the window bottom here, so the whole IME overlaps it (pages share the surface's environment).
        assertEquals(300.0, keyboard(), 1.0)
        onMain { androidx.core.view.ViewCompat.dispatchApplyWindowInsets(h.surface, androidx.core.view.WindowInsetsCompat.Builder().build()) }
        assertEquals(0.0, keyboard(), 0.01)
        // Sheets and dialogs are opaque in the theme background.
        onMain {
            h.batch {
                create(20, ViewType.SHEET); insert(20, 0, 2); frame(20, 0f, 0f, 400f, 200f)
                create(30, ViewType.DIALOG); insert(30, 0, 3); frame(30, 0f, 0f, 352f, 160f)
            }
        }
        val dialogs = onMain { h.surface.presenter.openDialogs.toList() }
        waitFor(what = "overlays shown") { dialogs.all { it.isShowing } }
        val bg = onMain { dev.nativeshell.render.ThemeTokens.of(h.surface.context).background }
        val sheet = dialogs.filterIsInstance<BottomSheetDialog>().single()
        assertEquals(bg, onMain { sheet.findViewById<android.view.View>(com.google.android.material.R.id.design_bottom_sheet)!!.backgroundTintList!!.defaultColor })
        val card = onMain { h.view(30) }
        assertEquals(bg, onMain { (card.background as android.graphics.drawable.GradientDrawable).color!!.defaultColor })
        // The dialog card is centered in its window.
        waitFor(what = "card laid out") { card.width > 0 }
        onMain {
            // Centered in the safe area (the holder is padded by the system bars and the keyboard).
            val parent = card.parent as android.view.View
            val cx = parent.paddingLeft + (parent.width - parent.paddingLeft - parent.paddingRight) / 2f
            val cy = parent.paddingTop + (parent.height - parent.paddingTop - parent.paddingBottom) / 2f
            assertEquals(cx, card.left + card.width / 2f, 2f)
            assertEquals(cy, card.top + card.height / 2f, 2f)
        }
        onMain { h.batch { delete(20); delete(30) } }
    }

    @Test
    fun sheetShowsAndReportsUserDismissal() {
        val h = harness()
        onMain {
            h.batch {
                create(20, ViewType.SHEET); prop(20, Prop.DETENTS, PropValue.json("""["medium","large"]"""))
                insert(20, 0, 0); frame(20, 0f, 0f, 400f, 300f)
                create(21, ViewType.TEXT); insert(21, 20, 0); frame(21, 16f, 16f, 200f, 20f)
                prop(21, Prop.ATTRIBUTED_TEXT, PropValue.json("""{"runs":[{"text":"Sheet"}]}"""))
            }
        }
        val dialog = onMain { h.surface.presenter.openDialogs.single() } as BottomSheetDialog
        waitFor(what = "sheet shown") { dialog.isShowing && h.view(21).isAttachedToWindow }
        onMain { dialog.cancel() }
        waitFor(what = "dismiss event") { h.eventsOf(EventType.DISMISS).isNotEmpty() }
        assertEquals(20, h.eventsOf(EventType.DISMISS).single().node)
        // The core then deletes it; no second dismissal is reported.
        onMain { h.batch { delete(20) } }
        assertEquals(1, h.eventsOf(EventType.DISMISS).size)
    }

    @Test
    fun coreClosedSheetDoesNotReportDismiss() {
        val h = harness()
        onMain { h.batch { create(20, ViewType.SHEET); insert(20, 0, 0); frame(20, 0f, 0f, 400f, 200f) } }
        val dialog = onMain { h.surface.presenter.openDialogs.single() }
        waitFor(what = "shown") { dialog.isShowing }
        onMain { h.batch { delete(20) } }
        waitFor(what = "closed") { !dialog.isShowing }
        instrumentation.waitForIdleSync()
        assertTrue(h.eventsOf(EventType.DISMISS).isEmpty())
    }

    @Test
    fun alertReportsTheChosenAction() {
        val h = harness()
        onMain {
            h.batch {
                create(30, ViewType.DIALOG)
                prop(30, Prop.ALERT_SPEC, PropValue.json(
                    """{"title":"Delete?","message":"This cannot be undone","actions":[{"label":"Cancel","style":"cancel"},{"label":"Delete","style":"destructive"}]}""",
                ))
                insert(30, 0, 0)
            }
        }
        val dialog = onMain { h.surface.presenter.openDialogs.single() } as AlertDialog
        waitFor(what = "alert shown") { dialog.isShowing }
        onMain { dialog.getButton(DialogInterface.BUTTON_POSITIVE).performClick() }
        waitFor(what = "action") { h.eventsOf(EventType.ACTION).isNotEmpty() }
        assertEquals(1, JSONObject(h.eventsOf(EventType.ACTION).single().payload).getInt("index"))
        assertTrue(h.eventsOf(EventType.DISMISS).isEmpty())
    }

    @Test
    fun customDialogHostsContent() {
        val h = harness()
        onMain {
            h.batch {
                create(40, ViewType.DIALOG); prop(40, Prop.DISMISSABLE, PropValue.bool(true)); insert(40, 0, 0); frame(40, 0f, 0f, 400f, 800f)
                create(41, ViewType.VIEW); insert(41, 40, 0); frame(41, 50f, 300f, 300f, 200f)
                prop(41, Prop.BACKGROUND_COLOR, PropValue.color(0xFFFFFFFF.toInt()))
            }
        }
        val dialog: Dialog = onMain { h.surface.presenter.openDialogs.single() }
        waitFor(what = "dialog content") { dialog.isShowing && h.view(41).isAttachedToWindow }
        onMain { dialog.cancel() }
        waitFor(what = "dismiss") { h.eventsOf(EventType.DISMISS).isNotEmpty() }
    }

    @Test
    fun toastsStackThreeAndQueueTheRest() {
        val h = harness()
        val stack = onMain { h.surface.toastStack }
        onMain {
            h.batch {
                for (i in 1..4) command(0, Command.SHOW_TOAST, """{"id":"t$i","title":"Toast $i","level":"info","duration":0}""")
            }
        }
        assertEquals(listOf("t1", "t2", "t3"), onMain { stack.visibleIds })
        assertEquals(listOf("t4"), onMain { stack.queuedIds })
        // Unknown ids are ignored; a queued toast can be retracted before it shows.
        onMain { h.batch { command(0, Command.DISMISS_TOAST, """{"id":"nope"}""") } }
        assertEquals(listOf("t1", "t2", "t3"), onMain { stack.visibleIds })
        // Retracting a visible one lets the queued one in.
        onMain { h.batch { command(0, Command.DISMISS_TOAST, """{"id":"t2"}""") } }
        assertEquals(listOf("t1", "t3", "t4"), onMain { stack.visibleIds })
        assertTrue(onMain { stack.queuedIds }.isEmpty())
        onMain { h.batch { command(0, Command.SHOW_TOAST, """{"id":"t5","title":"Queued","level":"error","duration":0}""") } }
        onMain { h.batch { command(0, Command.DISMISS_TOAST, """{"id":"t5"}""") } }
        assertTrue(onMain { stack.queuedIds }.isEmpty())
        // Sticky toasts stay; timed ones expire and are replaced.
        onMain { h.batch { command(0, Command.DISMISS_TOAST, "{}") } }
        assertTrue(onMain { stack.visibleIds }.isEmpty())
        onMain {
            h.batch {
                command(0, Command.SHOW_TOAST, """{"id":"a","title":"Short","level":"success","duration":300}""")
                command(0, Command.SHOW_TOAST, """{"id":"b","title":"Sticky","level":"warning","duration":0}""")
            }
        }
        waitFor(what = "timed toast expired") { stack.visibleIds == listOf("b") }
        onMain { h.batch { command(0, Command.DISMISS_TOAST, "{}") } }
    }

    @Test
    fun toastActionDismissesAndReports() {
        val h = harness()
        onMain { h.batch { command(0, Command.SHOW_TOAST, """{"id":"undo-1","title":"Item deleted","level":"info","duration":0,"action":{"label":"Undo"}}""") } }
        val stack = onMain { h.surface.toastStack }
        val button = onMain {
            val card = stack.getChildAt(0) as android.view.ViewGroup
            card.getChildAt(card.childCount - 1) as android.widget.TextView
        }
        assertEquals("Undo", onMain { button.text.toString() })
        waitFor(what = "laid out") { button.height > 0 }
        val d = onMain { button.resources.displayMetrics.density }
        assertTrue(onMain { button.height >= (48 * d).toInt() && button.width >= (48 * d).toInt() })
        onMain { button.performClick() }
        val e = h.eventsOf(EventType.TOAST_ACTION).single()
        assertEquals(0, e.node)
        assertEquals("""{"id":"undo-1"}""", e.payload)
        assertTrue(onMain { stack.visibleIds }.isEmpty())
    }

    @Test
    fun toastsShowInTheStack() {
        val h = harness()
        onMain { h.batch { command(0, Command.SHOW_TOAST, """{"id":"s","title":"Saved","description":"All good","level":"info","duration":3000}""") } }
        assertEquals("Saved\nAll good", onMain { h.surface.toastStack.lastShown })
        onMain { h.batch { command(0, Command.SET_CLIPBOARD, """{"text":"copied"}""") } }
        val cm = activity().getSystemService(android.content.ClipboardManager::class.java)
        assertEquals("copied", onMain { cm.primaryClip?.getItemAt(0)?.text?.toString() })
        // Unknown command targets are ignored, not fatal.
        onMain { h.batch { command(12345, Command.HAPTIC, """{"haptic":"success"}""") } }
        assertNull(h.mounting.node(12345))
    }
}
