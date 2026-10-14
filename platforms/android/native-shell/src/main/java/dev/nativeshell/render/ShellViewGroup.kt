package dev.nativeshell.render

import android.annotation.SuppressLint
import android.content.Context
import android.graphics.Rect
import android.os.Handler
import android.os.Looper
import android.view.MotionEvent
import android.view.View
import android.view.ViewGroup

/** A child's frame in pixels relative to its [ShellViewGroup] parent, as the core computed it. */
class ShellLayoutParams(@JvmField var left: Int, @JvmField var top: Int, width: Int, height: Int) :
    ViewGroup.LayoutParams(width, height)

/**
 * The container of every node that has native children.
 *
 * Layout is entirely the core's: `onMeasure` takes the size the parent imposes
 * (or its own fixed layout params) and measures each child at exactly its frame;
 * `onLayout` places children at their frames. `requestLayout` does not bubble
 * (as React Native's view groups): a child that asks for layout is re-measured
 * and re-laid out at its unchanged frame on the next main-thread turn, so a
 * widget changing its text never triggers a traversal of the whole surface.
 */
open class ShellViewGroup(context: Context) : ViewGroup(context) {

    /** Touch-target expansion per child, `[top, right, bottom, left]` in px, set from `HitSlop`. */
    private var slopTarget: View? = null

    init {
        clipChildren = false
        clipToPadding = false
        // A container that draws nothing skips onDraw; the decoration is a background drawable.
        setWillNotDraw(true)
    }

    override fun onMeasure(widthMeasureSpec: Int, heightMeasureSpec: Int) {
        val lp = layoutParams
        val w = if (lp != null && lp.width >= 0) lp.width else MeasureSpec.getSize(widthMeasureSpec)
        val h = if (lp != null && lp.height >= 0) lp.height else MeasureSpec.getSize(heightMeasureSpec)
        setMeasuredDimension(w, h)
        for (i in 0 until childCount) {
            val child = getChildAt(i)
            val clp = child.layoutParams as? ShellLayoutParams ?: continue
            measureExactly(child, clp.width, clp.height)
        }
    }

    override fun onLayout(changed: Boolean, l: Int, t: Int, r: Int, b: Int) {
        for (i in 0 until childCount) {
            val child = getChildAt(i)
            val clp = child.layoutParams as? ShellLayoutParams ?: continue
            if (child.isAnimatingFrame()) continue
            child.layout(clp.left, clp.top, clp.left + clp.width, clp.top + clp.height)
        }
    }

    override fun generateDefaultLayoutParams(): LayoutParams = ShellLayoutParams(0, 0, 0, 0)
    override fun checkLayoutParams(p: LayoutParams?): Boolean = p is ShellLayoutParams
    override fun generateLayoutParams(p: LayoutParams?): LayoutParams =
        ShellLayoutParams(0, 0, p?.width?.coerceAtLeast(0) ?: 0, p?.height?.coerceAtLeast(0) ?: 0)

    override fun shouldDelayChildPressedState(): Boolean = false

    @SuppressLint("MissingSuperCall")
    override fun requestLayout() {
        // Before attachment the framework's own first traversal lays everything out.
        if (!isAttachedToWindow || isRootContainer) {
            super.requestLayout()
            return
        }
        RelayoutQueue.schedule(this)
    }

    /** The surface root takes part in normal Android layout (its size comes from the host). */
    internal var isRootContainer = false

    /** Re-measures and re-lays out children that asked for layout, each at its unchanged frame. */
    internal fun relayoutRequestedChildren() {
        for (i in 0 until childCount) {
            val child = getChildAt(i)
            if (!child.isLayoutRequested) continue
            val clp = child.layoutParams as? ShellLayoutParams ?: continue
            measureExactly(child, clp.width, clp.height)
            child.layout(clp.left, clp.top, clp.left + clp.width, clp.top + clp.height)
        }
    }

    // Hit slop: a touch that lands outside every child but inside a child's expanded
    // rect goes to that child for the whole gesture.
    override fun dispatchTouchEvent(ev: MotionEvent): Boolean {
        if (ev.actionMasked == MotionEvent.ACTION_DOWN) {
            slopTarget = findSlopTarget(ev.x, ev.y)
        }
        val target = slopTarget
        if (target != null) {
            val e = MotionEvent.obtain(ev)
            e.offsetLocation((scrollX - target.left).toFloat(), (scrollY - target.top).toFloat())
            val handled = target.dispatchTouchEvent(e)
            e.recycle()
            if (ev.actionMasked == MotionEvent.ACTION_UP || ev.actionMasked == MotionEvent.ACTION_CANCEL) slopTarget = null
            return handled
        }
        return super.dispatchTouchEvent(ev)
    }

    private val tmp = Rect()

    private fun findSlopTarget(x: Float, y: Float): View? {
        var candidate: View? = null
        for (i in childCount - 1 downTo 0) {
            val c = getChildAt(i)
            if (c.visibility != VISIBLE) continue
            if (x >= c.left && x < c.right && y >= c.top && y < c.bottom) return null
            if (candidate != null) continue
            val slop = c.getTag(dev.nativeshell.R.id.nsh_hit_slop) as? IntArray ?: continue
            tmp.set(c.left - slop[3], c.top - slop[0], c.right + slop[1], c.bottom + slop[2])
            if (tmp.contains(x.toInt(), y.toInt())) candidate = c
        }
        return candidate
    }

    companion object {
        fun measureExactly(child: View, w: Int, h: Int) {
            child.measure(
                MeasureSpec.makeMeasureSpec(w.coerceAtLeast(0), MeasureSpec.EXACTLY),
                MeasureSpec.makeMeasureSpec(h.coerceAtLeast(0), MeasureSpec.EXACTLY),
            )
        }
    }
}

internal fun View.isAnimatingFrame(): Boolean = getTag(dev.nativeshell.R.id.nsh_frame_animator) != null

/**
 * Collects shell containers whose children asked for layout and lays them out
 * once on the next main-thread turn (or right away at the end of a batch).
 */
internal object RelayoutQueue {
    private val pending = LinkedHashSet<ShellViewGroup>()
    private val handler = Handler(Looper.getMainLooper())
    private var posted = false
    private val runnable = Runnable { flush() }

    fun schedule(group: ShellViewGroup) {
        pending.add(group)
        if (!posted) {
            posted = true
            handler.post(runnable)
        }
    }

    fun flush() {
        posted = false
        handler.removeCallbacks(runnable)
        // Laying out may schedule more groups (nested widgets); drain until stable.
        var guard = 0
        while (pending.isNotEmpty() && guard++ < 8) {
            val batch = pending.toList()
            pending.clear()
            for (g in batch) if (g.isAttachedToWindow) g.relayoutRequestedChildren()
        }
    }
}
