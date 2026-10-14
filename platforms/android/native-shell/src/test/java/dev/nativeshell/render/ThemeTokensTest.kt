package dev.nativeshell.render

import android.content.res.ColorStateList
import androidx.test.core.app.ApplicationProvider
import com.google.android.material.materialswitch.MaterialSwitch
import dev.nativeshell.protocol.Prop
import dev.nativeshell.protocol.PropValue
import dev.nativeshell.protocol.Protocol
import dev.nativeshell.protocol.ViewType
import dev.nativeshell.testing.Harness
import dev.nativeshell.views.ValueField
import org.junit.Assert.assertEquals
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config

/** The root's `ThemeTokens` prop is the source of every color the renderer draws itself. */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [34])
class ThemeTokensTest {
    private val checked = intArrayOf(android.R.attr.state_checked)

    private fun MaterialSwitch.checkedTrack() = (trackTintList as ColorStateList).getColorForState(checked, 0)

    @Test
    fun rootTokensDriveChromeAndRetint() {
        val h = Harness(ApplicationProvider.getApplicationContext())
        h.batch {
            create(1, ViewType.SWITCH); insert(1, 0, 0); frame(1, 0f, 0f, 52f, 32f)
            create(2, ViewType.SELECT); insert(2, 0, 1); frame(2, 0f, 40f, 160f, 40f)
        }
        val sw = h.view(1) as MaterialSwitch
        // Before the core's tokens arrive: the defaults (light primary #171717).
        assertEquals(0xFF171717.toInt(), sw.checkedTrack())

        h.batch {
            prop(Protocol.ROOT_NODE, Prop.THEME_TOKENS, PropValue.json(
                """{"appearance":"light","background":4294967295,"foreground":4278848010,"primary":4280640491,"primary_foreground":4294967295,"muted":4294309365}""",
            ))
        }
        assertEquals(0xFF2563EB.toInt(), sw.checkedTrack())
        assertEquals(0xFF2563EB.toInt(), ThemeTokens.of(h.view(2).context).primary)
        assertEquals(0xFF2563EB.toInt(), ThemeTokens.of(h.surface.toastStack.context).primary)

        // A theme/appearance change re-tints existing controls; missing tokens fall back per appearance.
        h.batch {
            prop(Protocol.ROOT_NODE, Prop.THEME_TOKENS, PropValue.json("""{"appearance":"dark","primary":4294309365}"""))
        }
        assertEquals(0xFFF5F5F5.toInt(), sw.checkedTrack())
        val dark = ThemeTokens.of(h.view(2).context)
        assertEquals(0xFF0A0A0A.toInt(), dark.background)
        assertEquals(0xFF171717.toInt(), dark.primaryForeground)

        // Controls created afterwards get the current tokens; a new ValueField reads them too.
        h.batch { create(3, ViewType.SWITCH); insert(3, 0, 2); create(4, ViewType.DATE_PICKER); insert(4, 0, 3) }
        assertEquals(0xFFF5F5F5.toInt(), (h.view(3) as MaterialSwitch).checkedTrack())
        assertEquals(0xFFF5F5F5.toInt(), ThemeTokens.of((h.view(4) as ValueField).context).primary)

        // Null falls back to the provider/defaults.
        h.batch { prop(Protocol.ROOT_NODE, Prop.THEME_TOKENS, PropValue.NULL) }
        assertEquals(ThemeTokens.of(ThemeTokens.isDark(h.surface.context)).primary, sw.checkedTrack())
    }
}
