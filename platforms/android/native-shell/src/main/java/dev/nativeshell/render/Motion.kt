package dev.nativeshell.render

import android.animation.ValueAnimator
import android.content.Context
import android.os.Build
import android.provider.Settings

/**
 * Reduced motion: the user turned animations off ("Remove animations", or an
 * animator duration scale of 0). Movement (frames, page and sheet slides,
 * toast slides) then applies instantly or cross-fades; opacity fades stay, short.
 */
internal object Motion {
    /** Tests force a value; null reads the system setting. */
    @Volatile var override: Boolean? = null

    const val REDUCED_FADE_MS = 120L

    fun reduced(context: Context): Boolean {
        override?.let { return it }
        if (Build.VERSION.SDK_INT >= 26 && !ValueAnimator.areAnimatorsEnabled()) return true
        return Settings.Global.getFloat(context.contentResolver, Settings.Global.ANIMATOR_DURATION_SCALE, 1f) == 0f
    }
}
