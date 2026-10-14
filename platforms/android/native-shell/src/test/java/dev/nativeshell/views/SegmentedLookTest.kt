package dev.nativeshell.views

import android.content.res.ColorStateList
import androidx.test.core.app.ApplicationProvider
import com.google.android.material.button.MaterialButton
import com.google.android.material.button.MaterialButtonToggleGroup
import dev.nativeshell.protocol.Prop
import dev.nativeshell.protocol.PropValue
import dev.nativeshell.protocol.ViewType
import dev.nativeshell.render.ThemeTokens
import dev.nativeshell.testing.Harness
import org.junit.Assert.assertEquals
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config

/** The segmented control looks like Material 3's segmented button unless the script tints it. */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [34])
class SegmentedLookTest {
    private val checked = intArrayOf(android.R.attr.state_checked)

    private fun MaterialButton.fill(on: Boolean) =
        (backgroundTintList as ColorStateList).getColorForState(if (on) checked else intArrayOf(), 1)

    @Test
    fun selectedSegmentUsesSecondaryContainerByDefaultAndTintWhenSet() {
        val h = Harness(ApplicationProvider.getApplicationContext())
        h.batch {
            create(1, ViewType.SEGMENTED); insert(1, 0, 0); frame(1, 0f, 0f, 300f, 40f)
            prop(1, Prop.ITEMS, PropValue.json("""["Day","Week","Month"]""")); prop(1, Prop.SELECTED_INDEX, PropValue.i32(1))
        }
        val g = h.view(1) as MaterialButtonToggleGroup
        val b = g.getChildAt(1) as MaterialButton
        val t = ThemeTokens.of(h.surface.context)
        assertEquals(t.secondary, b.fill(true))
        assertEquals(0, b.fill(false))
        assertEquals(t.secondaryForeground, b.textColors.getColorForState(checked, 0))

        h.batch { prop(1, Prop.TINT_COLOR, PropValue.color(0xFF16A34A.toInt())) }
        assertEquals(0xFF16A34A.toInt(), b.fill(true))
        assertEquals(t.primaryForeground, b.textColors.getColorForState(checked, 0))

        h.batch { prop(1, Prop.TINT_COLOR, PropValue.NULL) }
        assertEquals(t.secondary, b.fill(true))
    }
}
