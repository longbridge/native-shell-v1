package dev.nativeshell.render

import android.view.KeyEvent
import dev.nativeshell.util.Json

/** Hardware key events as `Key` payloads, with gpui's key names (`s`, `enter`, `escape`, `up`, `f5`). */
internal object KeyEvents {

    fun payload(e: KeyEvent): String? {
        val key = name(e.keyCode) ?: return null
        val ch = e.getUnicodeChar(e.metaState and (KeyEvent.META_SHIFT_MASK or KeyEvent.META_CAPS_LOCK_ON))
            .takeIf { it > 0 && !Character.isISOControl(it) }?.let { String(Character.toChars(it)) } ?: ""
        val phase = if (e.action == KeyEvent.ACTION_UP) "up" else "down"
        return "{\"key\":${Json.stringify(key)},\"key_char\":${Json.stringify(ch)},\"modifiers\":{\"shift\":${e.isShiftPressed}," +
            "\"ctrl\":${e.isCtrlPressed},\"alt\":${e.isAltPressed},\"cmd\":${e.isMetaPressed}},\"phase\":\"$phase\"}"
    }

    /** gpui key name, or null for keys that are only modifiers or system keys. */
    fun name(code: Int): String? = when (code) {
        in KeyEvent.KEYCODE_A..KeyEvent.KEYCODE_Z -> ('a' + (code - KeyEvent.KEYCODE_A)).toString()
        in KeyEvent.KEYCODE_0..KeyEvent.KEYCODE_9 -> ('0' + (code - KeyEvent.KEYCODE_0)).toString()
        in KeyEvent.KEYCODE_F1..KeyEvent.KEYCODE_F12 -> "f${code - KeyEvent.KEYCODE_F1 + 1}"
        KeyEvent.KEYCODE_ENTER, KeyEvent.KEYCODE_NUMPAD_ENTER -> "enter"
        KeyEvent.KEYCODE_ESCAPE -> "escape"
        KeyEvent.KEYCODE_TAB -> "tab"
        KeyEvent.KEYCODE_SPACE -> "space"
        KeyEvent.KEYCODE_DEL -> "backspace"
        KeyEvent.KEYCODE_FORWARD_DEL -> "delete"
        KeyEvent.KEYCODE_DPAD_UP -> "up"
        KeyEvent.KEYCODE_DPAD_DOWN -> "down"
        KeyEvent.KEYCODE_DPAD_LEFT -> "left"
        KeyEvent.KEYCODE_DPAD_RIGHT -> "right"
        KeyEvent.KEYCODE_MOVE_HOME -> "home"
        KeyEvent.KEYCODE_MOVE_END -> "end"
        KeyEvent.KEYCODE_PAGE_UP -> "pageup"
        KeyEvent.KEYCODE_PAGE_DOWN -> "pagedown"
        KeyEvent.KEYCODE_INSERT -> "insert"
        KeyEvent.KEYCODE_MINUS -> "-"
        KeyEvent.KEYCODE_EQUALS -> "="
        KeyEvent.KEYCODE_LEFT_BRACKET -> "["
        KeyEvent.KEYCODE_RIGHT_BRACKET -> "]"
        KeyEvent.KEYCODE_BACKSLASH -> "\\"
        KeyEvent.KEYCODE_SEMICOLON -> ";"
        KeyEvent.KEYCODE_APOSTROPHE -> "'"
        KeyEvent.KEYCODE_COMMA -> ","
        KeyEvent.KEYCODE_PERIOD -> "."
        KeyEvent.KEYCODE_SLASH -> "/"
        KeyEvent.KEYCODE_GRAVE -> "`"
        else -> null
    }
}
