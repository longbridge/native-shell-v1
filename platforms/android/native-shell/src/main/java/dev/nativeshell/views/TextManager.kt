package dev.nativeshell.views

import android.annotation.SuppressLint
import android.content.Context
import android.graphics.Canvas
import android.os.Build
import android.text.Layout
import android.text.Spanned
import android.text.StaticLayout
import android.text.TextUtils
import android.util.TypedValue
import android.view.Gravity
import android.view.MotionEvent
import android.view.View
import android.view.ViewGroup
import androidx.appcompat.widget.AppCompatTextView
import dev.nativeshell.NativeShell
import dev.nativeshell.protocol.EventMask
import dev.nativeshell.protocol.EventType
import dev.nativeshell.protocol.Prop
import dev.nativeshell.protocol.PropValue
import dev.nativeshell.protocol.ViewType
import dev.nativeshell.render.LinkTarget
import dev.nativeshell.render.MountHost
import dev.nativeshell.render.Node
import dev.nativeshell.render.ViewManager
import dev.nativeshell.text.LinkHandler
import dev.nativeshell.text.LinkSpan
import dev.nativeshell.text.TextLayouts
import dev.nativeshell.util.Json

/**
 * A text node. Non-selectable text draws the measured [StaticLayout] itself;
 * selectable text hosts a [AppCompatTextView] configured with the same
 * parameters, so both lay out exactly as measured.
 */
@SuppressLint("ViewConstructor")
internal class ShellTextView(context: Context) : ViewGroup(context), LinkTarget, LinkHandler {
    var json: String? = null
        set(value) {
            if (field == value) return
            field = value
            refresh()
        }
    var selectable = false
        set(value) {
            if (field == value) return
            field = value
            refresh()
        }
    var linksEnabled = false
    /** The script listens for `Link` events. */
    var linkListening = false
    var widthPt = 0f
        set(value) {
            if (field == value) return
            field = value
            invalidate()
        }
    var onLink: ((String) -> Unit)? = null

    /** Highlight of selected text (the `selection` token). */
    var selectionHighlight: Int = 0x33007AFF
        set(value) {
            field = value
            textView?.highlightColor = value
        }

    private var textView: AppCompatTextView? = null
    private var pressedLink: LinkSpan? = null

    init {
        setWillNotDraw(false)
    }

    /** The layout drawn for the current text and width (null when empty). */
    fun currentLayout(): Layout? {
        val j = json ?: return null
        return TextLayouts.layoutFor(j, widthPt)
    }

    private fun refresh() {
        val j = json
        if (selectable && j != null) {
            val tv = textView ?: AppCompatTextView(context).also {
                it.includeFontPadding = false
                it.setPadding(0, 0, 0, 0)
                it.setLineSpacing(0f, 1f)
                it.breakStrategy = Layout.BREAK_STRATEGY_SIMPLE
                it.hyphenationFrequency = Layout.HYPHENATION_FREQUENCY_NONE
                if (Build.VERSION.SDK_INT >= 28) it.isFallbackLineSpacing = true
                textView = it
                addView(it)
            }
            val a = TextLayouts.attributed(j)
            tv.setTextSize(TypedValue.COMPLEX_UNIT_PX, a.baseSizePx)
            tv.typeface = a.baseTypeface
            tv.gravity = when (a.alignment) {
                Layout.Alignment.ALIGN_CENTER -> Gravity.CENTER_HORIZONTAL
                Layout.Alignment.ALIGN_OPPOSITE -> Gravity.END
                else -> Gravity.START
            } or Gravity.TOP
            if (a.justify && Build.VERSION.SDK_INT >= 26) tv.justificationMode = Layout.JUSTIFICATION_MODE_INTER_WORD
            tv.maxLines = if (a.maxLines > 0) a.maxLines else Int.MAX_VALUE
            tv.ellipsize = if (a.ellipsize) TextUtils.TruncateAt.END else null
            tv.highlightColor = selectionHighlight
            tv.setTextIsSelectable(true)
            tv.text = a.text
            tv.visibility = VISIBLE
        } else {
            textView?.let {
                it.text = ""
                it.visibility = GONE
            }
        }
        contentDescription = null
        invalidate()
    }

    /** Text as the user sees it (tests, accessibility). */
    val text: CharSequence get() = json?.let { TextLayouts.attributed(it).text } ?: ""

    override fun onMeasure(widthMeasureSpec: Int, heightMeasureSpec: Int) {
        val w = MeasureSpec.getSize(widthMeasureSpec)
        val h = MeasureSpec.getSize(heightMeasureSpec)
        setMeasuredDimension(w, h)
        textView?.measure(MeasureSpec.makeMeasureSpec(w, MeasureSpec.EXACTLY), MeasureSpec.makeMeasureSpec(h, MeasureSpec.EXACTLY))
    }

    override fun onLayout(changed: Boolean, l: Int, t: Int, r: Int, b: Int) {
        textView?.layout(0, 0, r - l, b - t)
    }

    override fun onDraw(canvas: Canvas) {
        if (selectable) return
        val layout = currentLayout() ?: return
        layout.draw(canvas)
    }

    override fun onInitializeAccessibilityNodeInfo(info: android.view.accessibility.AccessibilityNodeInfo) {
        super.onInitializeAccessibilityNodeInfo(info)
        if (contentDescription == null) info.text = text
        info.className = "android.widget.TextView"
    }

    private fun linkAt(x: Float, y: Float): LinkSpan? {
        if (!linksEnabled && !linkListening) return null
        val layout = currentLayout() ?: return null
        val text = layout.text as? Spanned ?: return null
        val line = layout.getLineForVertical(y.toInt())
        if (x < layout.getLineLeft(line) || x > layout.getLineRight(line)) return null
        val off = layout.getOffsetForHorizontal(line, x)
        return text.getSpans(off, off, LinkSpan::class.java).firstOrNull()
    }

    override fun hasLinkAt(x: Float, y: Float): Boolean = !selectable && linkAt(x, y) != null

    @SuppressLint("ClickableViewAccessibility")
    override fun onTouchEvent(event: MotionEvent): Boolean {
        if (selectable) return super.onTouchEvent(event)
        when (event.actionMasked) {
            MotionEvent.ACTION_DOWN -> {
                pressedLink = linkAt(event.x, event.y)
                return pressedLink != null
            }
            MotionEvent.ACTION_UP -> {
                val link = pressedLink
                pressedLink = null
                if (link != null && linkAt(event.x, event.y) === link) {
                    onLink(link.url)
                    return true
                }
            }
            MotionEvent.ACTION_CANCEL -> pressedLink = null
        }
        return pressedLink != null
    }

    override fun onLink(url: String) {
        onLink?.invoke(url)
    }
}

internal class TextManager : ViewManager(ViewType.TEXT) {
    override fun createView(host: MountHost): View = ShellTextView(host.context)

    override fun container(node: Node): ViewGroup? = null

    override fun onMount(node: Node, host: MountHost) {
        val v = node.view as ShellTextView
        v.onLink = { url ->
            if (node.eventMask and EventMask.LINK != 0) {
                host.dispatch(node.id, EventType.LINK, "{\"url\":${Json.stringify(url)}}")
            } else if (v.linksEnabled) {
                NativeShell.urlOpener.open(v.context, url)
            }
        }
    }

    override fun setProp(node: Node, prop: Int, value: PropValue, host: MountHost): Boolean {
        val v = node.view as ShellTextView
        when (prop) {
            Prop.ATTRIBUTED_TEXT -> v.json = value.string
            Prop.SELECTABLE -> v.selectable = !value.isNull && value.bool
            Prop.LINKS_ENABLED -> v.linksEnabled = !value.isNull && value.bool
            else -> return false
        }
        return true
    }

    override fun onFrame(node: Node, host: MountHost) {
        (node.view as ShellTextView).widthPt = node.widthPt
    }

    override fun onEventMask(node: Node, host: MountHost) {
        (node.view as ShellTextView).linkListening = node.eventMask and EventMask.LINK != 0
    }

    override fun onRecycle(node: Node, host: MountHost) {
        val v = node.view as ShellTextView
        v.widthPt = 0f
    }
}
