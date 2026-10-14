package dev.nativeshell

import android.content.res.Configuration
import android.graphics.Bitmap
import android.graphics.Canvas
import android.graphics.Color
import android.view.ViewGroup
import androidx.test.ext.junit.runners.AndroidJUnit4
import dev.nativeshell.protocol.Command
import dev.nativeshell.render.ToastStack
import dev.nativeshell.testing.Harness
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import java.io.File

/**
 * Toasts are neutral cards (background + border tokens) whose level shows as
 * the icon's shape; checked in both appearances and saved as screenshots
 * (`files/screenshots/toast-{light,dark}.png` on the device).
 */
@RunWith(AndroidJUnit4::class)
class ToastDesignTest : ActivityTest() {

    private fun render(dark: Boolean): Bitmap {
        val h = onMain {
            val cfg = Configuration(activity().resources.configuration).apply {
                uiMode = (uiMode and Configuration.UI_MODE_NIGHT_MASK.inv()) or
                    (if (dark) Configuration.UI_MODE_NIGHT_YES else Configuration.UI_MODE_NIGHT_NO)
            }
            val ctx = android.view.ContextThemeWrapper(activity().createConfigurationContext(cfg), R.style.NativeShell_Theme)
            Harness(ctx).also { activity().show(it.surface) }
        }
        val levels = listOf("info" to "Draft saved", "success" to "Payment sent", "warning" to "Storage almost full", "error" to "Upload failed")
        onMain {
            h.batch {
                for ((i, l) in levels.withIndex()) {
                    command(0, Command.SHOW_TOAST, org.json.JSONObject().put("id", "t$i").put("title", l.second)
                        .put("description", if (i % 2 == 0) "Details for ${l.first}." else null).put("level", l.first).put("duration", 0).toString())
                }
            }
        }
        val stack = onMain { h.surface.toastStack }
        // Three visible, one queued: retract the first so every level is shown once.
        onMain { h.batch { command(0, Command.DISMISS_TOAST, """{"id":"t0"}""") } }
        waitFor(what = "toasts settled") { stack.visibleIds == listOf("t1", "t2", "t3") && (0 until stack.childCount).all { stack.getChildAt(it).alpha == 1f } && stack.childCount == 3 }
        val icons = onMain {
            (0 until stack.childCount).map { ((stack.getChildAt(it) as ViewGroup).getChildAt(0) as ToastStack.StatusIcon).let { i -> i.level to i.color } }
        }
        assertEquals(listOf("success", "warning", "error"), icons.map { it.first })
        assertEquals(3, icons.map { it.second }.toSet().size)
        val bmp = onMain {
            val b = Bitmap.createBitmap(stack.width, stack.height, Bitmap.Config.ARGB_8888)
            b.eraseColor(if (dark) Color.BLACK else Color.WHITE)
            stack.draw(Canvas(b))
            b
        }
        // Card fill is the background token, not a level color.
        val card = onMain { stack.getChildAt(0) }
        val d = onMain { card.resources.displayMetrics.density }
        val fill = bmp.getPixel(card.left + card.width - (8 * d).toInt(), card.top + card.height / 2)
        assertEquals(Integer.toHexString(if (dark) 0xFF0A0A0A.toInt() else 0xFFFFFFFF.toInt()), Integer.toHexString(fill))
        onMain { h.batch { command(0, Command.DISMISS_TOAST, "{}") } }
        return bmp
    }

    @Test
    fun neutralCardsWithShapedIconsInBothAppearances() {
        val dir = File(instrumentation.targetContext.getExternalFilesDir(null), "screenshots").apply { mkdirs() }
        val light = render(false)
        val dark = render(true)
        assertNotEquals(light.getPixel(light.width / 2, light.height - 20), dark.getPixel(dark.width / 2, dark.height - 20))
        for ((name, b) in listOf("toast-light" to light, "toast-dark" to dark)) {
            File(dir, "$name.png").outputStream().use { b.compress(Bitmap.CompressFormat.PNG, 100, it) }
        }
        assertTrue(File(dir, "toast-dark.png").length() > 0)
    }
}
