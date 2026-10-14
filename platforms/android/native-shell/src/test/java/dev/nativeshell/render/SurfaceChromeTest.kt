package dev.nativeshell.render

import android.os.Looper
import android.view.View
import android.view.ViewGroup
import androidx.activity.ComponentActivity
import com.google.android.material.bottomsheet.BottomSheetDialog
import dev.nativeshell.protocol.Prop
import dev.nativeshell.protocol.PropValue
import dev.nativeshell.protocol.ViewType
import dev.nativeshell.testing.Harness
import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertSame
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.Robolectric
import org.robolectric.RobolectricTestRunner
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config

/** Sheet chrome, on-screen-only test ids, and the clipboard_feedback environment field. */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [34], qualifiers = "w400dp-h800dp-xxhdpi")
class SurfaceChromeTest {
    private fun attached(): Harness {
        val activity = Robolectric.buildActivity(ComponentActivity::class.java).setup().get()
        activity.setTheme(com.google.android.material.R.style.Theme_Material3_DayNight_NoActionBar)
        val h = Harness(activity)
        activity.setContentView(h.surface)
        shadowOf(Looper.getMainLooper()).idle()
        return h
    }

    @Test
    fun sheetsHaveRoundedOpaqueSurfaceAndGrabber() {
        val h = attached()
        h.batch {
            create(20, ViewType.SHEET); insert(20, 0, 0); frame(20, 0f, 0f, 400f, 240f)
            create(21, ViewType.VIEW); insert(21, 20, 0); frame(21, 0f, 0f, 400f, 240f)
        }
        shadowOf(Looper.getMainLooper()).idle()
        val dialog = h.surface.presenter.openDialogs.single() as BottomSheetDialog
        val sheet = dialog.findViewById<View>(com.google.android.material.R.id.design_bottom_sheet)!!
        val d = h.surface.resources.displayMetrics.density
        assertEquals(ThemeTokens.of(h.surface.context).background, sheet.backgroundTintList!!.defaultColor)
        val shape = (sheet.background as com.google.android.material.shape.MaterialShapeDrawable).shapeAppearanceModel
        val bounds = android.graphics.RectF(0f, 0f, 400 * d, 240 * d)
        assertEquals(16 * d, shape.topLeftCornerSize.getCornerSize(bounds), 0.5f)
        assertEquals(16 * d, shape.topRightCornerSize.getCornerSize(bounds), 0.5f)
        assertEquals(0f, shape.bottomLeftCornerSize.getCornerSize(bounds), 0.01f)
        val holder = h.view(20).parent as ViewGroup
        val grabber = holder.getChildAt(1)
        assertEquals((36 * d).toInt(), grabber.layoutParams.width)
        assertEquals((5 * d).toInt(), grabber.layoutParams.height)
        assertEquals((6 * d).toInt(), (grabber.layoutParams as ViewGroup.MarginLayoutParams).topMargin)
        // The keyboard opens in the sheet's window: the content is padded above it and the sheet expands.
        val ime = (280 * d).toInt()
        androidx.core.view.ViewCompat.dispatchApplyWindowInsets(
            holder,
            androidx.core.view.WindowInsetsCompat.Builder()
                .setInsets(androidx.core.view.WindowInsetsCompat.Type.ime(), androidx.core.graphics.Insets.of(0, 0, 0, ime)).build(),
        )
        assertEquals(ime, holder.paddingBottom)
        assertEquals(com.google.android.material.bottomsheet.BottomSheetBehavior.STATE_EXPANDED, dialog.behavior.state)
    }

    @Test
    fun overlaysOpenWithoutFocusingAField() {
        val h = attached()
        h.batch {
            create(20, ViewType.SHEET); insert(20, 0, 0); frame(20, 0f, 0f, 400f, 200f)
            create(21, ViewType.TEXT_INPUT); insert(21, 20, 0); frame(21, 16f, 40f, 300f, 44f)
            create(30, ViewType.DIALOG); insert(30, 0, 1); frame(30, 0f, 0f, 352f, 160f)
            create(31, ViewType.TEXT_INPUT); insert(31, 30, 0); frame(31, 16f, 40f, 300f, 44f)
        }
        shadowOf(Looper.getMainLooper()).idle()
        for (field in listOf(21, 31)) {
            val v = h.view(field)
            assertTrue("field $field must not take focus on open", !v.hasFocus())
        }
        // Dialogs are edge to edge: transparent system bars under the scrim, card centered in the safe area.
        val dialog = h.surface.presenter.openDialogs.first { it !is BottomSheetDialog }
        val w = dialog.window!!
        assertEquals(android.graphics.Color.TRANSPARENT, w.statusBarColor)
        val holder = h.view(30).parent as ViewGroup
        androidx.core.view.ViewCompat.dispatchApplyWindowInsets(
            holder,
            androidx.core.view.WindowInsetsCompat.Builder()
                .setInsets(androidx.core.view.WindowInsetsCompat.Type.systemBars(), androidx.core.graphics.Insets.of(0, 70, 0, 40))
                .setInsets(androidx.core.view.WindowInsetsCompat.Type.ime(), androidx.core.graphics.Insets.of(0, 0, 0, 600)).build(),
        )
        assertEquals(70, holder.paddingTop)
        assertEquals(600, holder.paddingBottom)
    }

    @Test
    fun findByTestIdIgnoresRowsNotOnScreen() {
        val h = attached()
        h.batch {
            create(1, ViewType.VIEW); insert(1, 0, 0); frame(1, 0f, 0f, 400f, 800f)
            create(2, ViewType.LIST); insert(2, 1, 0); frame(2, 0f, 0f, 400f, 300f)
            prop(2, Prop.ITEM_COUNT, PropValue.i32(100))
            prop(2, Prop.ROW_OFFSETS, PropValue.floats(FloatArray(101) { it * 50f }))
            for (i in 0 until 20) {
                val id = 100 + i
                create(id, ViewType.LIST_ROW); prop(id, Prop.ROW_INDEX, PropValue.i32(i)); insert(id, 2, i); frame(id, 0f, 0f, 400f, 50f)
                prop(id, Prop.TEST_ID, PropValue.str("row-$i"))
            }
        }
        h.surface.measure(View.MeasureSpec.makeMeasureSpec(1200, View.MeasureSpec.EXACTLY), View.MeasureSpec.makeMeasureSpec(2400, View.MeasureSpec.EXACTLY))
        h.surface.layout(0, 0, 1200, 2400)
        shadowOf(Looper.getMainLooper()).idle()
        assertSame(h.view(100), h.surface.findByTestId("row-0"))
        // Row 19 exists as a node but sits in no visible cell: not findable.
        assertNull(h.surface.findByTestId("row-19"))
        // A released row is reset: its test id is gone even though the view is pooled.
        val row0 = h.view(100)
        h.batch { delete(100) }
        assertNull(row0.getTag(dev.nativeshell.R.id.nsh_test_id))
        assertNull(h.surface.findByTestId("row-0"))
        assertNotNull(h.surface.findByTestId("row-1"))
    }

    @Test
    fun environmentReportsClipboardFeedback() {
        val h = attached()
        assertTrue(JSONObject(h.engine.environments.last()).getBoolean("clipboard_feedback"))
    }

    @Test
    @Config(sdk = [32])
    fun noClipboardFeedbackBeforeAndroid13() {
        val h = attached()
        assertEquals(false, JSONObject(h.engine.environments.last()).getBoolean("clipboard_feedback"))
    }
}
