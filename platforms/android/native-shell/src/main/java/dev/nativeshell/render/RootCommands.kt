package dev.nativeshell.render

import android.content.ClipData
import android.content.ClipboardManager
import android.content.Context
import android.os.Build
import android.view.HapticFeedbackConstants
import android.view.View
import android.widget.Toast
import dev.nativeshell.LogLevel
import dev.nativeshell.NativeShell
import dev.nativeshell.protocol.Command
import dev.nativeshell.util.Json
import org.json.JSONObject

/** Commands that are not about one view: toasts, URLs, clipboard, haptics. */
internal object RootCommands {

    fun run(command: Int, payload: JSONObject?, host: MountHost): Boolean {
        when (command) {
            Command.SHOW_TOAST -> showToast(payload, host)
            Command.OPEN_URL -> {
                val url = Json.string(payload, "url") ?: return true
                if (!NativeShell.urlOpener.open(host.context, url)) host.log(LogLevel.WARN, "could not open $url")
            }
            Command.SET_CLIPBOARD -> {
                val cm = host.context.getSystemService(Context.CLIPBOARD_SERVICE) as? ClipboardManager ?: return true
                cm.setPrimaryClip(ClipData.newPlainText("text", Json.string(payload, "text") ?: ""))
            }
            Command.DISMISS_TOAST -> host.toasts?.dismiss(Json.string(payload, "id"))
            Command.HAPTIC -> host.anchor?.let { haptic(it, Json.string(payload, "kind") ?: Json.string(payload, "haptic") ?: "light") }
            else -> return false
        }
        return true
    }

    private fun showToast(payload: JSONObject?, host: MountHost) {
        val title = Json.string(payload, "title") ?: ""
        val description = Json.string(payload, "description")
        // Absent duration: the default; 0: until retracted.
        val durationMs = if (payload?.has("duration") == true) Json.int(payload, "duration", 3000) else 3000
        val stack = host.toasts
        if (stack != null) {
            stack.show(Json.string(payload, "id"), title, description, Json.string(payload, "level") ?: "info", durationMs,
                payload?.optJSONObject("action")?.let { Json.string(it, "label") })
        } else {
            val text = if (description.isNullOrEmpty()) title else "$title\n$description"
            Toast.makeText(host.context, text, if (durationMs > 3000 || durationMs == 0) Toast.LENGTH_LONG else Toast.LENGTH_SHORT).show()
        }
    }

    fun haptic(view: View, kind: String) {
        val c = when (kind) {
            "light", "selection" -> HapticFeedbackConstants.CLOCK_TICK
            "medium" -> HapticFeedbackConstants.VIRTUAL_KEY
            "heavy" -> HapticFeedbackConstants.LONG_PRESS
            "success" -> if (Build.VERSION.SDK_INT >= 30) HapticFeedbackConstants.CONFIRM else HapticFeedbackConstants.VIRTUAL_KEY
            "warning", "error" -> if (Build.VERSION.SDK_INT >= 30) HapticFeedbackConstants.REJECT else HapticFeedbackConstants.LONG_PRESS
            else -> HapticFeedbackConstants.VIRTUAL_KEY
        }
        @Suppress("DEPRECATION")
        view.performHapticFeedback(c, HapticFeedbackConstants.FLAG_IGNORE_VIEW_SETTING)
    }
}
