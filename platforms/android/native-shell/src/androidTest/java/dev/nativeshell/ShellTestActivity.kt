package dev.nativeshell

import android.os.Bundle
import android.view.View
import android.widget.FrameLayout
import androidx.activity.ComponentActivity
import androidx.activity.enableEdgeToEdge
import androidx.test.ext.junit.rules.ActivityScenarioRule
import androidx.test.platform.app.InstrumentationRegistry
import dev.nativeshell.testing.Harness
import org.junit.Rule

/** Host activity of the instrumented tests: an edge-to-edge window holding one surface. */
class ShellTestActivity : ComponentActivity() {
    lateinit var container: FrameLayout

    override fun onCreate(savedInstanceState: Bundle?) {
        enableEdgeToEdge()
        super.onCreate(savedInstanceState)
        container = FrameLayout(this)
        setContentView(container)
    }

    fun show(view: View) {
        container.removeAllViews()
        container.addView(view, FrameLayout.LayoutParams(FrameLayout.LayoutParams.MATCH_PARENT, FrameLayout.LayoutParams.MATCH_PARENT))
    }
}

abstract class ActivityTest {
    @get:Rule
    val rule = ActivityScenarioRule(ShellTestActivity::class.java)

    protected val instrumentation get() = InstrumentationRegistry.getInstrumentation()

    protected fun <T> onMain(block: () -> T): T {
        var result: Result<T>? = null
        instrumentation.runOnMainSync { result = runCatching(block) }
        return result!!.getOrThrow()
    }

    protected fun activity(): ShellTestActivity {
        var a: ShellTestActivity? = null
        rule.scenario.onActivity { a = it }
        return a!!
    }

    /** A fake-engine surface shown in the activity. */
    internal fun harness(): Harness {
        val h = onMain { Harness(activity()).also { activity().show(it.surface) } }
        instrumentation.waitForIdleSync()
        return h
    }

    /** Polls [condition] on the main thread until it holds or [timeoutMs] passes. */
    protected fun waitFor(timeoutMs: Long = 5000, what: String = "condition", condition: () -> Boolean) {
        val end = System.currentTimeMillis() + timeoutMs
        while (System.currentTimeMillis() < end) {
            if (onMain(condition)) return
            Thread.sleep(16)
        }
        throw AssertionError("timed out waiting for $what")
    }
}
