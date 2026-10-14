package dev.nativeshell

import android.content.res.Configuration
import android.graphics.Bitmap
import android.graphics.Canvas
import android.graphics.Color
import android.view.ContextThemeWrapper
import androidx.test.ext.junit.runners.AndroidJUnit4
import com.google.android.material.materialswitch.MaterialSwitch
import dev.nativeshell.protocol.Prop
import dev.nativeshell.protocol.PropValue
import dev.nativeshell.protocol.ViewType
import dev.nativeshell.render.ThemeTokens
import dev.nativeshell.testing.Harness
import org.junit.After
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith

/** Controls are tinted from the shell's `primary` token, not Material's baseline purple. */
@RunWith(AndroidJUnit4::class)
class ControlTintTest : ActivityTest() {

    @After fun clear() { NativeShell.themeProvider = null }

    private fun harness(dark: Boolean): Harness = onMain {
        val cfg = Configuration(activity().resources.configuration).apply {
            uiMode = (uiMode and Configuration.UI_MODE_NIGHT_MASK.inv()) or
                (if (dark) Configuration.UI_MODE_NIGHT_YES else Configuration.UI_MODE_NIGHT_NO)
        }
        Harness(ContextThemeWrapper(activity().createConfigurationContext(cfg), R.style.NativeShell_Theme)).also { activity().show(it.surface) }
    }

    private fun near(a: Int, b: Int) = Math.abs(Color.red(a) - Color.red(b)) < 12 &&
        Math.abs(Color.green(a) - Color.green(b)) < 12 && Math.abs(Color.blue(a) - Color.blue(b)) < 12

    /** The most common opaque color of a bitmap region that is not the background: the track. */
    private fun trackColor(b: Bitmap, background: Int): Int {
        val counts = HashMap<Int, Int>()
        for (y in b.height / 3 until b.height * 2 / 3) for (x in 0 until b.width / 2) {
            val p = b.getPixel(x, y)
            if (Color.alpha(p) == 255 && !near(p, background)) counts[p] = (counts[p] ?: 0) + 1
        }
        return counts.maxByOrNull { it.value }?.key ?: 0
    }

    private fun checkedSwitchTrack(dark: Boolean): Pair<Int, Int> {
        val h = harness(dark)
        val bg = if (dark) Color.BLACK else Color.WHITE
        onMain {
            h.batch {
                create(1, ViewType.SWITCH); insert(1, 0, 0); frame(1, 0f, 0f, 52f, 32f); prop(1, Prop.CHECKED, PropValue.bool(true))
            }
        }
        waitFor(what = "thumb settled") { (h.view(1) as MaterialSwitch).isChecked && h.view(1).width > 0 }
        Thread.sleep(400) // thumb animation
        val bmp = onMain {
            val v = h.view(1)
            Bitmap.createBitmap(v.width, v.height, Bitmap.Config.ARGB_8888).also { it.eraseColor(bg); v.draw(Canvas(it)) }
        }
        val primary = onMain { ThemeTokens.of(h.surface.context).primary }
        onMain { h.instance.destroy() }
        return trackColor(bmp, bg) to primary
    }

    @Test
    fun checkedSwitchTrackIsPrimaryInBothAppearances() {
        for (dark in listOf(false, true)) {
            val (track, primary) = checkedSwitchTrack(dark)
            assertTrue("dark=$dark track ${Integer.toHexString(track)} vs primary ${Integer.toHexString(primary)}", near(track, primary))
        }
        // A theme provider's primary is used too.
        NativeShell.themeProvider = ThemeProvider { mapOf("primary" to 0xFF2563EB.toInt(), "primary_foreground" to Color.WHITE) }
        val (track, primary) = checkedSwitchTrack(false)
        assertTrue("provider track ${Integer.toHexString(track)}", near(track, 0xFF2563EB.toInt()) && primary == 0xFF2563EB.toInt())
    }

    @Test
    fun tintColorOverridesAndResets() {
        val h = harness(false)
        onMain {
            h.batch {
                create(1, ViewType.SLIDER); insert(1, 0, 0); frame(1, 0f, 0f, 200f, 40f)
                prop(1, Prop.TINT_COLOR, PropValue.color(0xFF16A34A.toInt()))
            }
        }
        val slider = onMain { h.view(1) as com.google.android.material.slider.Slider }
        assertTrue(onMain { slider.trackActiveTintList.defaultColor } == 0xFF16A34A.toInt())
        onMain { h.batch { prop(1, Prop.TINT_COLOR, PropValue.NULL) } }
        assertTrue(onMain { slider.trackActiveTintList.defaultColor } == ThemeTokens.of(false).primary)
    }
}
