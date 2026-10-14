package dev.nativeshell

import android.app.Activity
import android.content.ComponentCallbacks2

/**
 * Wires an Activity's lifecycle to an instance: pauses its timers in the
 * background, forwards memory pressure, routes back presses (for activities
 * that are not a `ComponentActivity`; those get back handling automatically
 * through `OnBackPressedDispatcher`) and destroys it with the activity.
 *
 * ```kotlin
 * val delegate = ShellActivityDelegate(this, surface, instance)
 * override fun onResume() { super.onResume(); delegate.onResume() }
 * ```
 */
class ShellActivityDelegate(
    private val activity: Activity,
    val surface: ShellSurfaceView,
    val instance: ShellInstance,
) {
    init {
        surface.attach(instance)
    }

    fun onResume() = instance.setActive(true)

    fun onPause() = instance.setActive(false)

    fun onTrimMemory(level: Int) {
        @Suppress("DEPRECATION")
        if (level >= ComponentCallbacks2.TRIM_MEMORY_RUNNING_LOW) instance.trimMemory()
    }

    /** Returns true when the shell consumed the back press (a page was popped or the script was told). */
    fun onBackPressed(): Boolean = surface.handleBackPressed()

    fun onDestroy() {
        surface.detach()
        if (!activity.isChangingConfigurations) instance.destroy()
    }
}
