package dev.nativeshell

import android.content.res.Configuration
import android.graphics.Bitmap
import android.graphics.Canvas
import android.text.format.DateFormat
import android.view.ContextThemeWrapper
import androidx.test.ext.junit.runners.AndroidJUnit4
import dev.nativeshell.protocol.Prop
import dev.nativeshell.protocol.PropValue
import dev.nativeshell.protocol.ViewType
import dev.nativeshell.testing.Harness
import dev.nativeshell.text.TextLayouts
import dev.nativeshell.views.ValueField
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import java.io.File
import java.util.Calendar
import kotlin.math.abs

/** Select and DatePicker as quiet value fields: localized text, built-in measurement, both appearances. */
@RunWith(AndroidJUnit4::class)
class PickerFieldTest : ActivityTest() {

    @After fun restore() { onMain { TextLayouts.onConfigurationChanged(activity()) } }

    private fun cal(y: Int, mo: Int, d: Int, h: Int = 0, mi: Int = 0) = Calendar.getInstance().apply { set(y, mo - 1, d, h, mi, 0) }

    @Test
    fun datesAreLocalizedAndValuesStayIso() {
        val h = harness()
        val ctx = onMain { activity() }
        onMain {
            h.batch {
                create(1, ViewType.DATE_PICKER); insert(1, 0, 0); frame(1, 0f, 0f, 240f, 40f); prop(1, Prop.DATE, PropValue.str("2026-10-14"))
                create(2, ViewType.DATE_PICKER); insert(2, 0, 1); frame(2, 0f, 50f, 240f, 40f)
                prop(2, Prop.DATE_MODE, PropValue.i32(1)); prop(2, Prop.DATE, PropValue.str("09:30"))
                create(3, ViewType.DATE_PICKER); insert(3, 0, 2); frame(3, 0f, 100f, 240f, 40f)
                prop(3, Prop.DATE_MODE, PropValue.i32(2)); prop(3, Prop.DATE, PropValue.str("2026-10-14T21:05"))
                create(4, ViewType.DATE_PICKER); insert(4, 0, 3); prop(4, Prop.PLACEHOLDER, PropValue.str("Pick a date"))
            }
        }
        val date = java.text.DateFormat.getDateInstance(java.text.DateFormat.MEDIUM).format(cal(2026, 10, 14).time)
        val time = DateFormat.getTimeFormat(ctx).format(cal(2026, 1, 1, 9, 30).time)
        val both = "$date, ${DateFormat.getTimeFormat(ctx).format(cal(2026, 10, 14, 21, 5).time)}"
        assertEquals(date, onMain { (h.view(1) as ValueField).value })
        assertEquals(time, onMain { (h.view(2) as ValueField).value })
        assertEquals(both, onMain { (h.view(3) as ValueField).value })
        assertNull(onMain { (h.view(4) as ValueField).value })
        assertEquals("Pick a date", onMain { (h.view(4) as ValueField).placeholder })
        assertTrue(!date.contains("2026-10-14"))
    }

    @Test
    fun selectShowsTheSelectedLabelOrPlaceholder() {
        val h = harness()
        onMain {
            h.batch {
                create(1, ViewType.SELECT); insert(1, 0, 0)
                prop(1, Prop.MENU, PropValue.json("""{"placeholder":"Choose…","items":[{"label":"Daily","value":"d"},{"label":"Weekly","value":"w"}]}"""))
                prop(1, Prop.SELECTED_INDEX, PropValue.i32(1))
            }
        }
        assertEquals("Weekly", onMain { (h.view(1) as ValueField).value })
        onMain { h.batch { prop(1, Prop.SELECTED_INDEX, PropValue.i32(-1)) } }
        assertNull(onMain { (h.view(1) as ValueField).value })
        assertEquals("Choose…", onMain { (h.view(1) as ValueField).placeholder })
    }

    @Test
    fun builtInMeasurementMatchesTheFieldAndFollowsFontScale() {
        val h = harness()
        val cb = h.engine.callbacks
        val d = onMain { activity().resources.displayMetrics.density }
        onMain { TextLayouts.onConfigurationChanged(activity()) }
        val m = cb.measureComponent("nsh/select", """{"label":"Weekly","placeholder":"Choose…"}""", Float.POSITIVE_INFINITY, 0)!!
        val field = onMain {
            ValueField(activity()).apply {
                value = "Weekly"
                measure(android.view.View.MeasureSpec.UNSPECIFIED, android.view.View.MeasureSpec.UNSPECIFIED)
            }
        }
        assertTrue("select ${m.toList()} vs ${field.measuredWidth}x${field.measuredHeight}px",
            abs(m[0] * d - field.measuredWidth) <= 1f && abs(m[1] * d - field.measuredHeight) <= 1f)
        val date = cb.measureComponent("nsh/date-picker", """{"value":"2026-10-14","mode":"date"}""", Float.POSITIVE_INFINITY, 0)!!
        val empty = cb.measureComponent("nsh/date-picker", """{"value":"","mode":"datetime"}""", Float.POSITIVE_INFINITY, 0)!!
        assertTrue(date[0] > 0f && empty[0] > date[0])
        // Constrained: never wider than the available width.
        assertTrue(cb.measureComponent("nsh/select", """{"label":"A very long label that does not fit"}""", 100f, 1)!![0] <= 100f)
        // Font scale 2.0: larger.
        onMain {
            val cfg = Configuration(activity().resources.configuration).apply { fontScale = 2f }
            TextLayouts.onConfigurationChanged(activity().createConfigurationContext(cfg))
        }
        val big = cb.measureComponent("nsh/select", """{"label":"Weekly"}""", Float.POSITIVE_INFINITY, 0)!!
        assertTrue("${big.toList()} vs ${m.toList()}", big[0] > m[0] && big[1] > m[1])
    }

    @Test
    fun screenshotsInBothAppearances() {
        val dir = File(instrumentation.targetContext.getExternalFilesDir(null), "screenshots").apply { mkdirs() }
        for (dark in listOf(false, true)) {
            val h = onMain {
                val cfg = Configuration(activity().resources.configuration).apply {
                    uiMode = (uiMode and Configuration.UI_MODE_NIGHT_MASK.inv()) or
                        (if (dark) Configuration.UI_MODE_NIGHT_YES else Configuration.UI_MODE_NIGHT_NO)
                }
                Harness(ContextThemeWrapper(activity().createConfigurationContext(cfg), R.style.NativeShell_Theme)).also { activity().show(it.surface) }
            }
            onMain {
                h.batch {
                    create(1, ViewType.VIEW); insert(1, 0, 0); frame(1, 0f, 0f, 360f, 240f)
                    prop(1, Prop.BACKGROUND_COLOR, PropValue.color(if (dark) 0xFF0A0A0A.toInt() else 0xFFFFFFFF.toInt()))
                    create(2, ViewType.SELECT); insert(2, 1, 0); frame(2, 16f, 16f, 160f, 40f)
                    prop(2, Prop.MENU, PropValue.json("""{"items":[{"label":"Weekly","value":"w"}]}""")); prop(2, Prop.SELECTED_INDEX, PropValue.i32(0))
                    create(3, ViewType.SELECT); insert(3, 1, 1); frame(3, 16f, 72f, 160f, 40f)
                    prop(3, Prop.MENU, PropValue.json("""{"placeholder":"Choose…","items":[]}"""))
                    create(4, ViewType.DATE_PICKER); insert(4, 1, 2); frame(4, 16f, 128f, 200f, 40f); prop(4, Prop.DATE, PropValue.str("2026-10-14"))
                    create(5, ViewType.DATE_PICKER); insert(5, 1, 3); frame(5, 16f, 184f, 240f, 40f)
                    prop(5, Prop.DATE_MODE, PropValue.i32(2)); prop(5, Prop.DATE, PropValue.str("2026-10-14T21:05"))
                }
            }
            instrumentation.waitForIdleSync()
            val bmp = onMain {
                val v = h.view(1)
                Bitmap.createBitmap(v.width, v.height, Bitmap.Config.ARGB_8888).also { v.draw(Canvas(it)) }
            }
            File(dir, "pickers-${if (dark) "dark" else "light"}.png").outputStream().use { bmp.compress(Bitmap.CompressFormat.PNG, 100, it) }
        }
    }
}
