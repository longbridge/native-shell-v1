package dev.nativeshell.views

import android.annotation.SuppressLint
import android.content.Context
import android.os.Build
import android.text.Editable
import android.text.InputFilter
import android.text.InputType
import android.text.Spanned
import android.text.TextWatcher
import android.util.TypedValue
import android.view.Gravity
import android.view.KeyEvent
import android.view.View
import android.view.ViewGroup
import android.view.inputmethod.BaseInputConnection
import android.view.inputmethod.EditorInfo
import android.view.inputmethod.InputConnection
import android.view.inputmethod.InputConnectionWrapper
import android.view.inputmethod.InputMethodManager
import androidx.appcompat.widget.AppCompatEditText
import dev.nativeshell.protocol.Command
import dev.nativeshell.protocol.EventMask
import dev.nativeshell.protocol.EventType
import dev.nativeshell.protocol.Prop
import dev.nativeshell.protocol.PropValue
import dev.nativeshell.protocol.ValueKind
import dev.nativeshell.protocol.ViewType
import dev.nativeshell.render.MountHost
import dev.nativeshell.render.Node
import dev.nativeshell.render.ViewManager
import dev.nativeshell.text.AttributedText
import dev.nativeshell.util.Json
import org.json.JSONObject
import kotlin.math.roundToInt

/**
 * A controlled text field, after React Native's ReactEditText event-count protocol.
 *
 * The EditText owns its text. Every user edit increments [nativeEventCount]
 * and reports `{text, selection, composing, count}`. A `TextValue` from the
 * script is applied only when its count is at least the native counter (it
 * reflects every edit the user made) and the IME is not composing (replacing
 * the text mid-composition breaks pinyin/kana input; the value waits until the
 * composition ends). Equal text is skipped, and the watcher is muted while the
 * script's value is written so it does not echo back.
 */
@SuppressLint("ViewConstructor")
internal class ShellEditText(context: Context) : AppCompatEditText(context) {
    var nativeEventCount = 0
        private set
    private var muted = 0
    private var pending: Pair<String, Int>? = null
    private var lastSentComposing = false
    /** Text as last reported or written; config changes re-set the same text and must not count as edits. */
    private var lastText = ""
    var onChange: ((text: String, start: Int, end: Int, composing: Boolean, count: Int) -> Unit)? = null
    var onSubmit: (() -> Unit)? = null
    var multiline = false

    var maxLength = -1
        set(value) { field = value; updateFilters() }
    var allowedChars: String = ""
        set(value) { field = value; updateFilters() }

    init {
        background = null
        setPadding(0, 0, 0, 0)
        includeFontPadding = false
        // Same filter chain shape as after a reset (AppCompat may wrap filters).
        updateFilters()
        addTextChangedListener(object : TextWatcher {
            override fun beforeTextChanged(s: CharSequence?, start: Int, count: Int, after: Int) {}
            override fun onTextChanged(s: CharSequence?, start: Int, before: Int, count: Int) {}
            override fun afterTextChanged(s: Editable?) {
                if (muted > 0) {
                    lastText = s?.toString() ?: ""
                    return
                }
                if ((s?.toString() ?: "") == lastText && isComposing == lastSentComposing) return
                reportChange()
            }
        })
        setOnEditorActionListener { _, actionId, event ->
            val isEnter = event != null && event.keyCode == KeyEvent.KEYCODE_ENTER && event.action == KeyEvent.ACTION_DOWN
            if (actionId != EditorInfo.IME_NULL || (isEnter && !multiline)) {
                onSubmit?.invoke()
                // Let "next" move focus and "done" close the keyboard as usual.
                actionId == EditorInfo.IME_NULL
            } else false
        }
    }

    val isComposing: Boolean
        get() {
            val e = text ?: return false
            return BaseInputConnection.getComposingSpanStart(e) != -1
        }

    private fun reportChange() {
        nativeEventCount++
        val composing = isComposing
        lastSentComposing = composing
        lastText = text?.toString() ?: ""
        onChange?.invoke(lastText, selectionStart.coerceAtLeast(0), selectionEnd.coerceAtLeast(0), composing, nativeEventCount)
    }

    /** Applies a `TextValue` from the script per the event-count protocol. Returns whether the text was written. */
    fun applyTextValue(value: String, count: Int): Boolean {
        if (count < nativeEventCount) {
            pending = null
            return false
        }
        if (isComposing) {
            pending = value to count
            return false
        }
        pending = null
        val current = text?.toString() ?: ""
        if (current == value) return false
        val start = selectionStart
        val end = selectionEnd
        muted++
        try {
            setText(value)
            // Filters (max length, allowed chars) may have shortened the value.
            val len = text?.length ?: 0
            if (start >= 0 && hasFocus()) setSelection(start.coerceIn(0, len), end.coerceIn(0, len)) else setSelection(len)
        } finally {
            muted--
        }
        return true
    }

    /** Called after every IME call: a composition may have ended. */
    private fun afterImeCall() {
        if (lastSentComposing && !isComposing) {
            // The script saw the text as composing; tell it the composition is final.
            reportChange()
        }
        val p = pending
        if (p != null && !isComposing) applyTextValue(p.first, p.second)
    }

    fun resetForReuse() {
        muted++
        try { setText("") } finally { muted-- }
        nativeEventCount = 0
        lastText = ""
        pending = null
        lastSentComposing = false
    }

    private fun updateFilters() {
        val list = ArrayList<InputFilter>()
        if (maxLength >= 0) list += InputFilter.LengthFilter(maxLength)
        if (allowedChars.isNotEmpty()) {
            val allowed = allowedChars
            list += InputFilter { source, start, end, _, _, _ ->
                var ok = true
                for (i in start until end) if (allowed.indexOf(source[i]) < 0) { ok = false; break }
                if (ok) null else {
                    val sb = StringBuilder()
                    for (i in start until end) if (allowed.indexOf(source[i]) >= 0) sb.append(source[i])
                    if (source is Spanned) android.text.SpannableString(sb) else sb
                }
            }
        }
        filters = list.toTypedArray()
    }

    override fun onCreateInputConnection(outAttrs: EditorInfo): InputConnection? {
        val ic = super.onCreateInputConnection(outAttrs) ?: return null
        return object : InputConnectionWrapper(ic, true) {
            override fun setComposingText(text: CharSequence?, newCursorPosition: Int) = super.setComposingText(text, newCursorPosition).also { afterImeCall() }
            override fun commitText(text: CharSequence?, newCursorPosition: Int) = super.commitText(text, newCursorPosition).also { afterImeCall() }
            override fun finishComposingText() = super.finishComposingText().also { afterImeCall() }
            override fun setComposingRegion(start: Int, end: Int) = super.setComposingRegion(start, end).also { afterImeCall() }
            override fun deleteSurroundingText(beforeLength: Int, afterLength: Int) = super.deleteSurroundingText(beforeLength, afterLength).also { afterImeCall() }
            override fun endBatchEdit() = super.endBatchEdit().also { afterImeCall() }
        }
    }
}

internal class TextInputManager : ViewManager(ViewType.TEXT_INPUT) {
    /** Field configuration that maps onto one `inputType`. */
    private class State {
        var keyboard = 0
        var secure = false
        var multiline = false
        var autoCap = 0
        var editable = true
        var returnKey = 0
    }

    override fun createView(host: MountHost): View = ShellEditText(host.context).also { applyDefaults(it) }

    private fun applyDefaults(v: ShellEditText) {
        v.setTextSize(TypedValue.COMPLEX_UNIT_SP, AttributedText.DEFAULT_SIZE)
        v.setTextColor(AttributedText.DEFAULT_COLOR)
        v.typeface = android.graphics.Typeface.DEFAULT
        v.gravity = Gravity.CENTER_VERTICAL or Gravity.START
        v.hint = null
        v.setHintTextColor(0xFF737373.toInt())
    }

    override fun container(node: Node): ViewGroup? = null

    override fun onMount(node: Node, host: MountHost) {
        val v = node.view as ShellEditText
        if (node.state !is State) node.state = State()
        applyInputType(v, node.state as State)
        v.onChange = { text, start, end, composing, count ->
            if (node.eventMask and EventMask.CHANGE != 0) {
                host.dispatch(
                    node.id, EventType.CHANGE,
                    "{\"text\":${Json.stringify(text)},\"selection\":[$start,$end],\"composing\":$composing,\"count\":$count}",
                )
            }
        }
        v.onSubmit = {
            if (node.eventMask and EventMask.SUBMIT != 0) {
                host.dispatch(node.id, EventType.SUBMIT, "{\"text\":${Json.stringify(v.text?.toString() ?: "")}}")
            }
        }
        v.setOnFocusChangeListener { _, focused ->
            val bit = if (focused) EventMask.FOCUS else EventMask.BLUR
            if (node.eventMask and bit != 0) host.dispatch(node.id, if (focused) EventType.FOCUS else EventType.BLUR, "{}")
        }
    }

    override fun setProp(node: Node, prop: Int, value: PropValue, host: MountHost): Boolean {
        val v = node.view as ShellEditText
        val st = node.state as State
        when (prop) {
            Prop.TEXT_VALUE -> {
                val o = if (value.isNull) null else Json.obj(value.string)
                v.applyTextValue(Json.string(o, "text") ?: "", Json.int(o, "count", 0))
            }
            Prop.PLACEHOLDER -> v.hint = if (value.isNull) null else value.string
            Prop.PLACEHOLDER_COLOR -> v.setHintTextColor(if (value.kind == ValueKind.COLOR) value.int else 0xFF737373.toInt())
            Prop.MULTILINE -> { st.multiline = !value.isNull && value.bool; applyInputType(v, st) }
            Prop.SECURE -> { st.secure = !value.isNull && value.bool; applyInputType(v, st) }
            Prop.KEYBOARD_TYPE -> { st.keyboard = if (value.isNull) 0 else value.intValue(); applyInputType(v, st) }
            Prop.AUTO_CAPITALIZE -> { st.autoCap = if (value.isNull) 0 else value.intValue(); applyInputType(v, st) }
            Prop.EDITABLE -> { st.editable = value.isNull || value.bool; applyInputType(v, st) }
            Prop.RETURN_KEY -> { st.returnKey = if (value.isNull) 0 else value.intValue(); applyInputType(v, st) }
            Prop.CONTENT_INSETS -> {
                val a = value.floats
                val d = host.density
                fun px(i: Int) = if (a != null && i < a.size && a[i].isFinite()) (a[i] * d).roundToInt().coerceAtLeast(0) else 0
                v.setPadding(px(3), px(0), px(1), px(2))
            }
            Prop.MAX_LENGTH -> v.maxLength = if (value.isNull) -1 else value.intValue().let { if (it < 0) -1 else it }
            Prop.ALLOWED_CHARS -> v.allowedChars = if (value.isNull) "" else value.string ?: ""
            Prop.TEXT_STYLE -> applyTextStyle(v, if (value.isNull) null else Json.obj(value.string))
            Prop.CONTENT_TYPE -> if (Build.VERSION.SDK_INT >= 26) {
                val hint = when (if (value.isNull) 0 else value.intValue()) {
                    1 -> "smsOTPCode"
                    2 -> View.AUTOFILL_HINT_USERNAME
                    3 -> View.AUTOFILL_HINT_PASSWORD
                    4 -> View.AUTOFILL_HINT_EMAIL_ADDRESS
                    else -> null
                }
                if (hint == null) {
                    v.setAutofillHints(*arrayOf<String>())
                    v.importantForAutofill = View.IMPORTANT_FOR_AUTOFILL_AUTO
                } else {
                    v.setAutofillHints(hint)
                    v.importantForAutofill = View.IMPORTANT_FOR_AUTOFILL_YES
                }
            }
            else -> return false
        }
        return true
    }

    private fun applyTextStyle(v: ShellEditText, o: JSONObject?) {
        v.setTextSize(TypedValue.COMPLEX_UNIT_SP, Json.float(o, "size", AttributedText.DEFAULT_SIZE).coerceIn(1f, 1000f))
        v.setTextColor(Json.color(o, "color", AttributedText.DEFAULT_COLOR))
        v.typeface = AttributedText.typeface(Json.string(o, "family"), Json.int(o, "weight", 400), Json.bool(o, "italic", false))
        val h = when (Json.string(o, "align")) {
            "center" -> Gravity.CENTER_HORIZONTAL
            "right", "end" -> Gravity.END
            else -> Gravity.START
        }
        v.gravity = h or (v.gravity and Gravity.VERTICAL_GRAVITY_MASK)
        val lh = Json.float(o, "line_height", 0f)
        if (lh > 0f) {
            androidx.core.widget.TextViewCompat.setLineHeight(v, TypedValue.applyDimension(TypedValue.COMPLEX_UNIT_SP, lh, v.resources.displayMetrics).roundToInt())
        } else {
            v.setLineSpacing(0f, 1f)
        }
    }

    private fun applyInputType(v: ShellEditText, st: State) {
        var type = when (st.keyboard) {
            1 -> InputType.TYPE_CLASS_NUMBER
            2 -> InputType.TYPE_CLASS_NUMBER or InputType.TYPE_NUMBER_FLAG_DECIMAL
            3 -> InputType.TYPE_CLASS_TEXT or InputType.TYPE_TEXT_VARIATION_EMAIL_ADDRESS
            4 -> InputType.TYPE_CLASS_PHONE
            5 -> InputType.TYPE_CLASS_TEXT or InputType.TYPE_TEXT_VARIATION_URI
            else -> InputType.TYPE_CLASS_TEXT
        }
        if (type and InputType.TYPE_MASK_CLASS == InputType.TYPE_CLASS_TEXT) {
            type = type or when (st.autoCap) {
                1 -> InputType.TYPE_TEXT_FLAG_CAP_WORDS
                2 -> InputType.TYPE_TEXT_FLAG_CAP_SENTENCES
                3 -> InputType.TYPE_TEXT_FLAG_CAP_CHARACTERS
                else -> 0
            }
            if (st.multiline) type = type or InputType.TYPE_TEXT_FLAG_MULTI_LINE
            if (st.secure) type = (type and InputType.TYPE_MASK_VARIATION.inv()) or InputType.TYPE_TEXT_VARIATION_PASSWORD
        } else if (st.secure && type and InputType.TYPE_MASK_CLASS == InputType.TYPE_CLASS_NUMBER) {
            type = type or InputType.TYPE_NUMBER_VARIATION_PASSWORD
        }
        val start = v.selectionStart
        val end = v.selectionEnd
        v.multiline = st.multiline
        if (v.inputType != type) v.inputType = type
        v.isSingleLine = !st.multiline
        if (st.multiline) v.maxLines = Int.MAX_VALUE
        v.gravity = (v.gravity and Gravity.HORIZONTAL_GRAVITY_MASK) or (if (st.multiline) Gravity.TOP else Gravity.CENTER_VERTICAL)
        v.imeOptions = when (st.returnKey) {
            1 -> EditorInfo.IME_ACTION_DONE
            2 -> EditorInfo.IME_ACTION_GO
            3 -> EditorInfo.IME_ACTION_NEXT
            4 -> EditorInfo.IME_ACTION_SEARCH
            5 -> EditorInfo.IME_ACTION_SEND
            else -> if (st.multiline) EditorInfo.IME_ACTION_UNSPECIFIED else EditorInfo.IME_ACTION_DONE
        }
        v.isFocusable = st.editable
        v.isFocusableInTouchMode = st.editable
        v.isCursorVisible = st.editable
        val len = v.text?.length ?: 0
        if (start >= 0) v.setSelection(start.coerceIn(0, len), end.coerceIn(0, len))
    }

    override fun command(node: Node, command: Int, payload: JSONObject?, host: MountHost): Boolean {
        val v = node.view as ShellEditText
        val imm = v.context.getSystemService(Context.INPUT_METHOD_SERVICE) as? InputMethodManager
        when (command) {
            Command.FOCUS -> {
                v.requestFocus()
                v.post { imm?.showSoftInput(v, InputMethodManager.SHOW_IMPLICIT) }
            }
            Command.BLUR -> {
                imm?.hideSoftInputFromWindow(v.windowToken, 0)
                var p: Any? = v.parent
                while (p != null && p !is dev.nativeshell.ShellSurfaceView) p = (p as? android.view.View)?.parent
                (p as? dev.nativeshell.ShellSurfaceView)?.takeFocus() ?: v.clearFocus()
            }
            else -> return false
        }
        return true
    }

    override fun onRecycle(node: Node, host: MountHost) {
        val v = node.view as ShellEditText
        v.clearFocus()
        v.resetForReuse()
        applyDefaults(v)
        node.state = State()
        applyInputType(v, node.state as State)
        v.onChange = null
        v.onSubmit = null
        v.onFocusChangeListener = null
    }
}
