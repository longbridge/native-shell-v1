package dev.nativeshell.example

import android.content.Intent
import android.os.SystemClock
import android.util.Log
import android.view.View
import androidx.test.core.app.ActivityScenario
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import androidx.test.uiautomator.By
import androidx.test.uiautomator.UiDevice
import androidx.test.uiautomator.Until
import dev.nativeshell.NativeShell
import org.junit.After
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertTrue
import org.junit.Assume.assumeTrue
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith

/**
 * Drives every example program through its key screens in light and dark
 * appearance and saves `<example>-<screen>-<light|dark>.png` to [DIR] on the
 * device; `adb pull /data/local/tmp/nsh-screenshots/.` copies them to
 * `platforms/android/example/screenshots/`.
 */
@RunWith(AndroidJUnit4::class)
class ScreenshotTest {
    private val instrumentation = InstrumentationRegistry.getInstrumentation()
    private val device = UiDevice.getInstance(instrumentation)
    private var appearance = "light"

    @Before
    fun available() {
        assumeTrue(NativeShell.isNativeAvailable)
    }

    @After
    fun restoreAppearance() {
        device.executeShellCommand("cmd uimode night no")
    }

    // `screencap` runs as the shell user, which also captures the sheet and
    // dialog windows UiAutomation sometimes refuses; /data/local/tmp is its own.
    private fun save(name: String) {
        val path = "$DIR/$name.png"
        device.executeShellCommand("mkdir -p $DIR")
        device.executeShellCommand("screencap -p $path")
        Log.i("NSH-SHOT", "saved $path")
    }

    /** Runs [body] once per appearance, in a fresh program with empty storage. */
    private fun inBothAppearances(program: String, body: Driver.() -> Unit) {
        for (mode in listOf("light", "dark")) {
            appearance = mode
            device.executeShellCommand("cmd uimode night ${if (mode == "dark") "yes" else "no"}")
            device.executeShellCommand("cmd statusbar collapse")
            val intent = Intent(instrumentation.targetContext, ProgramActivity::class.java)
                .putExtra(ProgramActivity.EXTRA_PROGRAM, program)
                .putExtra(ProgramActivity.EXTRA_RESET, true)
            ActivityScenario.launch<ProgramActivity>(intent).use { Driver(it).body() }
        }
    }

    private inner class Driver(val scenario: ActivityScenario<ProgramActivity>) {
        fun view(id: String): View? {
            var found: View? = null
            scenario.onActivity { found = it.currentSurface.findByTestId(id)?.takeIf { v -> v.isShown && v.width > 0 } }
            return found
        }

        fun wait(id: String, timeoutMs: Long = 15_000): View {
            val end = SystemClock.uptimeMillis() + timeoutMs
            while (SystemClock.uptimeMillis() < end) {
                view(id)?.let { return it }
                SystemClock.sleep(100)
            }
            save("failed-$id-$appearance")
            throw AssertionError("$id did not appear")
        }

        /** A real touch at the center of the element, so press states and gestures run as on a device. */
        fun tap(id: String) {
            val v = wait(id)
            val loc = IntArray(2)
            instrumentation.runOnMainSync { v.getLocationOnScreen(loc) }
            device.click(loc[0] + v.width / 2, loc[1] + v.height / 2)
            instrumentation.waitForIdleSync()
        }

        /**
         * Taps the element whose accessible name contains [description]. List rows are recycled,
         * so inside lists the accessible name is the stable handle.
         */
        fun tapDescription(description: String) {
            val target = device.wait(Until.findObject(By.descContains(description)), 10_000)
            assertNotNull("element described as $description", target)
            target.click()
            instrumentation.waitForIdleSync()
        }

        fun type(id: String, text: String) {
            tap(id)
            SystemClock.sleep(400)
            instrumentation.sendStringSync(text)
            instrumentation.waitForIdleSync()
        }

        /** Closes the soft keyboard if it is up (back would otherwise pop the page). */
        fun hideKeyboard() {
            if (device.executeShellCommand("dumpsys input_method").contains("mInputShown=true")) {
                device.pressBack()
                SystemClock.sleep(500)
            }
        }

        /** Taps a row that pushes a page and lets the push transition finish. */
        fun push(id: String) {
            tap(id)
            SystemClock.sleep(800)
        }

        /** Returns to the previous page with the page's own back button. */
        fun back() {
            hideKeyboard()
            tap("back")
            // Let the pop transition finish before the next tap lands on the page below.
            SystemClock.sleep(800)
        }

        fun alertButton(label: String) {
            val button = device.wait(Until.findObject(By.text(label)), 5_000)
            assertNotNull("alert button $label", button)
            button.click()
            instrumentation.waitForIdleSync()
        }

        fun snap(name: String) {
            // Let transitions, the keyboard and pushed data settle.
            SystemClock.sleep(900)
            instrumentation.waitForIdleSync()
            save("$name-$appearance")
        }
    }

    @Test
    fun launcher() {
        for (mode in listOf("light", "dark")) {
            appearance = mode
            device.executeShellCommand("cmd uimode night ${if (mode == "dark") "yes" else "no"}")
            ActivityScenario.launch(MainActivity::class.java).use {
                assertTrue(device.wait(Until.hasObject(By.text("Examples")), 10_000))
                SystemClock.sleep(600)
                save("launcher-$mode")
            }
        }
    }

    @Test
    fun tasks() = inBothAppearances("tasks") {
        wait("task-1")
        tap("toggle-completed")
        snap("tasks")
        tapDescription("Edit “Book a dentist appointment”")
        wait("editor")
        snap("tasks-edit")
        tap("editor-delete")
        wait("undo-bar")
        snap("tasks-undo")
        tap("undo")
        type("draft", "Water the plants")
        snap("tasks-compose")
    }

    @Test
    fun settings() = inBothAppearances("settings") {
        wait("account")
        snap("settings")
        push("account")
        type("email", "@")
        tap("save")
        wait("email-error")
        snap("settings-account-error")
        back()
        alertButton("Discard changes")
        wait("settings")
        device.swipe(device.displayWidth / 2, device.displayHeight * 3 / 4, device.displayWidth / 2, device.displayHeight / 4, 20)
        device.swipe(device.displayWidth / 2, device.displayHeight * 3 / 4, device.displayWidth / 2, device.displayHeight / 4, 20)
        snap("settings-bottom")
        tap("clear-cache")
        snap("settings-clear-cache")
        alertButton("Clear cache")
    }

    @Test
    fun chat() = inBothAppearances("chat") {
        wait("bubble-m10")
        snap("chat")
        tapDescription("Called the ranger station")
        wait("actions")
        snap("chat-actions")
        device.pressBack()
        type("draft", "Great, see you at the station")
        snap("chat-compose")
        tap("send")
        wait("delivery")
    }

    @Test
    fun watchlist() = inBothAppearances("watchlist") {
        wait("cell-HALO.US")
        SystemClock.sleep(2_000)
        snap("watchlist")
        tapDescription("KITE.US")
        wait("detail")
        snap("watchlist-detail")
        device.pressBack()
        tap("add")
        wait("add-sheet")
        snap("watchlist-add")
    }

    @Test
    fun components() = inBothAppearances("components") {
        wait("open-buttons")
        snap("components")

        push("open-buttons")
        type("post", "The tomatoes finally came in.")
        hideKeyboard()
        tap("publish")
        snap("components-buttons")
        back()

        push("open-fields")
        for ((id, text) in listOf("name" to "Ada Lovelace", "street" to "12 Harbour Street", "city" to "Springfield", "postal" to "123")) type(id, text)
        tap("save-address")
        wait("postal-error")
        snap("components-fields")
        back()

        push("open-selection")
        tap("email-digest")
        tap("speed-express")
        snap("components-selection")
        back()

        push("open-progress")
        tap("upload-start")
        SystemClock.sleep(2_000)
        snap("components-progress")
        back()

        push("open-pickers")
        wait("summary")
        snap("components-pickers")
        back()

        push("open-overlays")
        snap("components-overlays")
        tap("share")
        wait("share-sheet")
        snap("components-overlays-sheet")
        device.pressBack()
        tap("rename")
        wait("rename-dialog")
        snap("components-overlays-dialog")
        tap("rename-cancel")
    }

    @Test
    fun gpuiTodolist() = inBothAppearances("gpui-todolist") {
        val field = device.wait(Until.findObject(By.clazz("android.widget.EditText")), 15_000)
        assertNotNull(field)
        for (item in listOf("Buy milk", "Walk the dog")) {
            field.click()
            instrumentation.sendStringSync(item)
            device.pressEnter()
        }
        snap("gpui-todolist")
    }

    companion object {
        const val DIR = "/data/local/tmp/nsh-screenshots"
    }
}
