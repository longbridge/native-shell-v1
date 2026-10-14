package dev.nativeshell.views

import android.annotation.SuppressLint
import android.content.Context
import android.util.DisplayMetrics
import android.util.SparseArray
import android.view.View
import android.view.ViewGroup
import android.widget.FrameLayout
import androidx.recyclerview.widget.LinearLayoutManager
import androidx.recyclerview.widget.LinearSmoothScroller
import androidx.recyclerview.widget.RecyclerView
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
import dev.nativeshell.render.layoutParamsFor
import dev.nativeshell.util.Json
import org.json.JSONObject
import kotlin.math.roundToInt

/**
 * A virtualized list. The core renders only the window this view reports
 * (`RangeChange`), as `ListRow` children carrying their `RowIndex`; every other
 * position is an empty cell at the estimated height from `RowOffsets`. Cells
 * host whichever row node currently has their index.
 */
@SuppressLint("ViewConstructor")
internal class ShellListView(context: Context) : RecyclerView(context) {
    var count = 0
        private set
    private var offsets: FloatArray? = null
    var prefetch = 0
    private val rows = SparseArray<Node>()
    private var lastFirst = -1
    private var lastLast = -1
    var onRange: ((first: Int, end: Int) -> Unit)? = null

    /**
     * GPUI's `ListAlignment::Bottom` (a chat thread): the list opens at its end and,
     * while the user is at the end, keeps the end in view as rows arrive or the
     * viewport shrinks (the keyboard). Scrolled away, the position is kept.
     */
    var bottomAligned = false
        set(value) {
            if (field == value) return
            field = value
            lm.stackFromEnd = value
            pinnedToEnd = value
            if (value && count > 0) lm.scrollToPositionWithOffset(count - 1, 0)
        }

    /** The end is in view (bottom-aligned lists only). */
    private var pinnedToEnd = false

    private fun keepEndInView() {
        if (bottomAligned && pinnedToEnd && count > 0) lm.scrollToPosition(count - 1)
    }
    var onScrolled: (() -> Unit)? = null
    private val density = context.resources.displayMetrics.density
    private val lm = LinearLayoutManager(context)

    private inner class Cell(val frame: FrameLayout) : ViewHolder(frame)

    private val cellAdapter = object : Adapter<Cell>() {
        override fun getItemCount(): Int = count
        override fun getItemId(position: Int): Long = position.toLong()
        override fun onCreateViewHolder(parent: ViewGroup, viewType: Int): Cell =
            Cell(FrameLayout(parent.context).apply { layoutParams = RecyclerView.LayoutParams(LayoutParams.MATCH_PARENT, 0) })
        override fun onBindViewHolder(holder: Cell, position: Int) = bind(holder.frame, position)
        override fun onViewRecycled(holder: Cell) {
            holder.frame.removeAllViews()
        }
    }

    init {
        layoutManager = lm
        itemAnimator = null
        cellAdapter.setHasStableIds(true)
        adapter = cellAdapter
        clipToPadding = false
        addOnScrollListener(object : OnScrollListener() {
            override fun onScrolled(recyclerView: RecyclerView, dx: Int, dy: Int) {
                // Whether the end is in view after any scroll or relayout; appends and
                // shrinking keep it there (stackFromEnd anchors layout at the end).
                if (bottomAligned) pinnedToEnd = !canScrollVertically(1)
                reportRange()
                onScrolled?.invoke()
            }
        })
    }

    private fun rowHeightPx(position: Int): Int {
        val o = offsets
        return if (o != null && position + 1 < o.size) ((o[position + 1] - o[position]) * density).roundToInt().coerceAtLeast(0)
        else (DEFAULT_ROW_HEIGHT * density).roundToInt()
    }

    private fun bind(cell: FrameLayout, position: Int) {
        val lp = cell.layoutParams
        val h = rowHeightPx(position)
        if (lp.height != h) {
            lp.height = h
            cell.layoutParams = lp
        }
        val row = rows.get(position)
        if (row != null && row.view.parent === cell) return
        cell.removeAllViews()
        if (row != null) {
            (row.view.parent as? ViewGroup)?.removeView(row.view)
            cell.addView(row.view, row.layoutParamsFor(cell))
        }
    }

    private fun rebindPosition(position: Int) {
        for (i in 0 until childCount) {
            val holder = getChildViewHolder(getChildAt(i)) as Cell
            if (holder.bindingAdapterPosition == position) bind(holder.frame, position)
        }
    }

    // The list sits in a container that lets children overflow (shadows, transforms);
    // its own rows never draw outside its bounds (over a header above it).
    override fun dispatchDraw(canvas: android.graphics.Canvas) {
        val save = canvas.save()
        canvas.clipRect(scrollX, scrollY, scrollX + width, scrollY + height)
        super.dispatchDraw(canvas)
        canvas.restoreToCount(save)
    }

    fun setCount(n: Int) {
        if (n == count) return
        count = n.coerceAtLeast(0)
        cellAdapter.notifyDataSetChanged()
        keepEndInView()
    }

    override fun onSizeChanged(w: Int, h: Int, oldw: Int, oldh: Int) {
        super.onSizeChanged(w, h, oldw, oldh)
        // The viewport shrank (keyboard) or grew: a pinned end stays in view.
        if (h != oldh) post { keepEndInView() }
    }

    fun setOffsets(o: FloatArray?) {
        offsets = o
        post { keepEndInView() }
        for (i in 0 until childCount) {
            val holder = getChildViewHolder(getChildAt(i)) as Cell
            val pos = holder.bindingAdapterPosition
            if (pos != NO_POSITION) bind(holder.frame, pos)
        }
    }

    fun addRow(node: Node, index: Int) {
        if (index < 0) return
        val prev = rows.get(index)
        if (prev != null && prev !== node) (prev.view.parent as? ViewGroup)?.removeView(prev.view)
        rows.put(index, node)
        rebindPosition(index)
    }

    fun removeRow(node: Node) {
        val i = rows.indexOfValue(node)
        if (i >= 0) rows.removeAt(i)
        (node.view.parent as? ViewGroup)?.removeView(node.view)
    }

    fun rowForIndex(index: Int): Node? = rows.get(index)

    /** Rows currently hosted by visible cells (tests). */
    val rowCount: Int get() = rows.size()

    override fun onLayout(changed: Boolean, l: Int, t: Int, r: Int, b: Int) {
        super.onLayout(changed, l, t, r, b)
        reportRange()
    }

    fun reportRange() {
        if (count == 0) return
        val first = lm.findFirstVisibleItemPosition()
        val last = lm.findLastVisibleItemPosition()
        if (first == NO_POSITION || last == NO_POSITION) return
        val f = (first - prefetch).coerceAtLeast(0)
        val l = (last + 1 + prefetch).coerceAtMost(count)
        if (f == lastFirst && l == lastLast) return
        lastFirst = f
        lastLast = l
        onRange?.invoke(f, l)
    }

    fun scrollToIndex(index: Int, position: String, animated: Boolean) {
        if (count == 0) return
        val i = index.coerceIn(0, count - 1)
        val rowH = rowHeightPx(i)
        val offset = when (position) {
            "center" -> (height - rowH) / 2
            "bottom" -> height - rowH
            else -> 0
        }
        if (!animated) {
            lm.scrollToPositionWithOffset(i, offset)
            return
        }
        val scroller = object : LinearSmoothScroller(context) {
            override fun calculateDtToFit(viewStart: Int, viewEnd: Int, boxStart: Int, boxEnd: Int, snapPreference: Int): Int =
                boxStart + offset - viewStart

            override fun calculateSpeedPerPixel(displayMetrics: DisplayMetrics): Float = 50f / displayMetrics.densityDpi
        }
        scroller.targetPosition = i
        lm.startSmoothScroll(scroller)
    }

    fun resetForReuse() {
        rows.clear()
        bottomAligned = false
        offsets = null
        prefetch = 0
        lastFirst = -1
        lastLast = -1
        count = 0
        cellAdapter.notifyDataSetChanged()
        lm.scrollToPositionWithOffset(0, 0)
    }

    companion object {
        const val DEFAULT_ROW_HEIGHT = 44f
    }
}

internal class ListManager : ViewManager(ViewType.LIST) {
    override fun createView(host: MountHost): View = ShellListView(host.context)

    override fun container(node: Node): ViewGroup? = null

    override fun onMount(node: Node, host: MountHost) {
        val v = node.view as ShellListView
        v.onRange = { first, end -> host.dispatch(node.id, EventType.RANGE_CHANGE, "{\"first\":$first,\"end\":$end}") }
        v.onScrolled = {
            if (node.eventMask and EventMask.SCROLL != 0) {
                val d = host.density
                host.dispatch(
                    node.id, EventType.SCROLL,
                    "{\"x\":0,\"y\":${v.computeVerticalScrollOffset() / d},\"content_width\":${v.width / d}," +
                        "\"content_height\":${v.computeVerticalScrollRange() / d},\"viewport_width\":${v.width / d},\"viewport_height\":${v.height / d}}",
                )
            }
        }
    }

    override fun insertChild(node: Node, child: Node, index: Int, host: MountHost) {
        (node.view as ShellListView).addRow(child, ListRowIndex.of(child))
    }

    override fun removeChild(node: Node, child: Node, host: MountHost) {
        (node.view as ShellListView).removeRow(child)
    }

    override fun moveChild(node: Node, child: Node, to: Int, host: MountHost) {}

    override fun setProp(node: Node, prop: Int, value: PropValue, host: MountHost): Boolean {
        val v = node.view as ShellListView
        when (prop) {
            Prop.ITEM_COUNT -> v.setCount(if (value.isNull) 0 else value.intValue())
            Prop.ROW_OFFSETS -> v.setOffsets(if (value.isNull) null else value.floats)
            Prop.PREFETCH -> { v.prefetch = if (value.isNull) 0 else value.number().roundToInt().coerceIn(0, 1000); v.reportRange() }
            Prop.SHOWS_INDICATORS -> v.isVerticalScrollBarEnabled = value.isNull || value.bool
            Prop.LIST_ALIGNMENT -> v.bottomAligned = !value.isNull && value.intValue() == 1
            else -> return false
        }
        return true
    }

    override fun command(node: Node, command: Int, payload: JSONObject?, host: MountHost): Boolean {
        val v = node.view as ShellListView
        when (command) {
            Command.SCROLL_TO_INDEX -> v.scrollToIndex(Json.int(payload, "index", 0), Json.string(payload, "position") ?: "top", Json.bool(payload, "animated", true))
            Command.SCROLL_TO -> {
                val d = host.density
                val y = (Json.float(payload, "y", 0f) * d).roundToInt()
                if (Json.bool(payload, "animated", true)) v.smoothScrollBy(0, y - v.computeVerticalScrollOffset())
                else v.scrollBy(0, y - v.computeVerticalScrollOffset())
            }
            else -> return false
        }
        return true
    }

    override fun onRecycle(node: Node, host: MountHost) {
        val v = node.view as ShellListView
        v.resetForReuse()
        v.onRange = null
        v.onScrolled = null
    }
}

/** `ListRow`: a container that knows its row index and tells its list when it changes. */
internal class ListRowManager : ViewManager(ViewType.LIST_ROW) {
    override fun createView(host: MountHost): View = ShellViewGroup(host.context)

    override fun setProp(node: Node, prop: Int, value: PropValue, host: MountHost): Boolean {
        if (prop != Prop.ROW_INDEX) return false
        val index = if (value.isNull) -1 else value.intValue()
        val list = node.parent?.view as? ShellListView
        if (list != null && ListRowIndex.of(node) != index) list.removeRow(node)
        node.state = index
        if (list != null) list.addRow(node, index)
        return true
    }

    override fun onRecycle(node: Node, host: MountHost) {
        node.state = null
    }
}

internal object ListRowIndex {
    fun of(node: Node): Int = node.state as? Int ?: -1
}
