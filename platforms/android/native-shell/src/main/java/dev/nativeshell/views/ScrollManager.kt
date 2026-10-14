package dev.nativeshell.views

import android.annotation.SuppressLint
import android.content.Context
import android.os.SystemClock
import android.view.View
import android.view.ViewGroup
import android.widget.FrameLayout
import android.widget.HorizontalScrollView
import androidx.core.widget.NestedScrollView
import androidx.swiperefreshlayout.widget.SwipeRefreshLayout
import dev.nativeshell.protocol.Command
import dev.nativeshell.protocol.EventMask
import dev.nativeshell.protocol.EventType
import dev.nativeshell.protocol.Prop
import dev.nativeshell.protocol.PropValue
import dev.nativeshell.protocol.ViewType
import dev.nativeshell.render.MountHost
import dev.nativeshell.render.Node
import dev.nativeshell.render.ShellViewGroup
import dev.nativeshell.render.ViewManager
import dev.nativeshell.util.Json
import org.json.JSONObject
import kotlin.math.roundToInt

/**
 * A native scroll container: (pull-to-refresh) → scroll view → content
 * [ShellViewGroup] of the size the core computed (`ContentSize`). The scroll
 * view is vertical or horizontal; switching recreates it around the same content.
 */
@SuppressLint("ViewConstructor")
internal class ShellScrollView(context: Context) : FrameLayout(context) {
    val refresh = SwipeRefreshLayout(context)
    val content = ShellViewGroup(context)
    private var indicators = true
    var scroller: ViewGroup = newScroller(false)
        private set
    var horizontal = false
        set(value) {
            if (field == value) return
            field = value
            val old = scroller
            old.removeView(content)
            if (old.parent === refresh) refresh.removeView(old) else removeView(old)
            scroller = newScroller(value)
            attachScroller()
        }
    var showsIndicators: Boolean
        get() = indicators
        set(value) {
            indicators = value
            scroller.isVerticalScrollBarEnabled = value && !horizontal
            scroller.isHorizontalScrollBarEnabled = value && horizontal
        }
    var contentWidth = 0
    var contentHeight = 0
    var onScroll: (() -> Unit)? = null

    init {
        refresh.isEnabled = false
        addView(refresh, LayoutParams(LayoutParams.MATCH_PARENT, LayoutParams.MATCH_PARENT))
        attachScroller()
    }

    private fun newScroller(horizontal: Boolean): ViewGroup {
        val s: ViewGroup = if (horizontal) HorizontalScrollView(context) else NestedScrollView(context)
        s.isVerticalScrollBarEnabled = indicators && !horizontal
        s.isHorizontalScrollBarEnabled = indicators && horizontal
        s.clipToPadding = false
        s.setOnScrollChangeListener { _, _, _, _, _ -> onScroll?.invoke() }
        return s
    }

    private fun attachScroller() {
        scroller.addView(content, LayoutParams(contentWidth, contentHeight))
        // Pull-to-refresh only wraps a vertical scroller.
        if (horizontal) {
            refresh.isEnabled = false
            addView(scroller, LayoutParams(LayoutParams.MATCH_PARENT, LayoutParams.MATCH_PARENT))
        } else {
            refresh.addView(scroller, LayoutParams(LayoutParams.MATCH_PARENT, LayoutParams.MATCH_PARENT))
        }
        showsIndicators = showsIndicators
    }

    fun setContentSize(w: Int, h: Int) {
        contentWidth = w
        contentHeight = h
        val lp = content.layoutParams
        if (lp.width != w || lp.height != h) {
            lp.width = w
            lp.height = h
            content.layoutParams = lp
        }
    }

    fun scrollToPx(x: Int, y: Int, animated: Boolean) {
        when (val s = scroller) {
            is NestedScrollView -> if (animated) s.smoothScrollTo(x, y) else s.scrollTo(x, y)
            is HorizontalScrollView -> if (animated) s.smoothScrollTo(x, y) else s.scrollTo(x, y)
        }
    }
}

internal class ScrollManager : ViewManager(ViewType.SCROLL) {
    private class State {
        var lastSent = 0L
        var pending: Runnable? = null
    }

    override fun createView(host: MountHost): View = ShellScrollView(host.context)

    override fun container(node: Node): ViewGroup = (node.view as ShellScrollView).content

    override fun onMount(node: Node, host: MountHost) {
        val v = node.view as ShellScrollView
        if (node.state !is State) node.state = State()
        v.onScroll = { scrolled(node, host) }
        v.refresh.setOnRefreshListener { host.dispatch(node.id, EventType.REFRESH, "{}") }
    }

    override fun setProp(node: Node, prop: Int, value: PropValue, host: MountHost): Boolean {
        val v = node.view as ShellScrollView
        when (prop) {
            Prop.HORIZONTAL -> v.horizontal = !value.isNull && value.bool
            Prop.CONTENT_SIZE -> {
                val a = value.floats
                val d = host.density
                if (a != null && a.size >= 2) v.setContentSize((a[0] * d).roundToInt().coerceAtLeast(0), (a[1] * d).roundToInt().coerceAtLeast(0))
                else v.setContentSize(0, 0)
            }
            Prop.REFRESHING -> v.refresh.isRefreshing = !value.isNull && value.bool
            Prop.REFRESH_ENABLED -> v.refresh.isEnabled = !value.isNull && value.bool && !v.horizontal
            Prop.SHOWS_INDICATORS -> v.showsIndicators = value.isNull || value.bool
            else -> return false
        }
        return true
    }

    override fun command(node: Node, command: Int, payload: JSONObject?, host: MountHost): Boolean {
        if (command != Command.SCROLL_TO) return false
        val v = node.view as ShellScrollView
        val d = host.density
        v.scrollToPx((Json.float(payload, "x", 0f) * d).roundToInt(), (Json.float(payload, "y", 0f) * d).roundToInt(), Json.bool(payload, "animated", true))
        return true
    }

    private fun scrolled(node: Node, host: MountHost) {
        if (node.eventMask and EventMask.SCROLL == 0) return
        val st = node.state as? State ?: return
        val v = node.view as ShellScrollView
        val now = SystemClock.uptimeMillis()
        val wait = THROTTLE_MS - (now - st.lastSent)
        if (wait <= 0) {
            st.lastSent = now
            send(node, v, host)
        } else if (st.pending == null) {
            val r = Runnable {
                st.pending = null
                st.lastSent = SystemClock.uptimeMillis()
                send(node, v, host)
            }
            st.pending = r
            v.postDelayed(r, wait)
        }
    }

    private fun send(node: Node, v: ShellScrollView, host: MountHost) {
        val d = host.density
        val s = v.scroller
        host.dispatch(
            node.id, EventType.SCROLL,
            "{\"x\":${s.scrollX / d},\"y\":${s.scrollY / d},\"content_width\":${v.contentWidth / d}," +
                "\"content_height\":${v.contentHeight / d},\"viewport_width\":${v.width / d},\"viewport_height\":${v.height / d}}",
        )
    }

    override fun onRecycle(node: Node, host: MountHost) {
        val v = node.view as ShellScrollView
        (node.state as? State)?.pending?.let { v.removeCallbacks(it) }
        node.state = State()
        v.scrollToPx(0, 0, false)
    }

    companion object {
        const val THROTTLE_MS = 16L
    }
}
