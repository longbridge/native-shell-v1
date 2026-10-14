package dev.nativeshell.render

import android.util.SparseArray
import android.view.View
import android.view.ViewGroup
import dev.nativeshell.LogLevel
import dev.nativeshell.R
import dev.nativeshell.protocol.Batch
import dev.nativeshell.protocol.Command
import dev.nativeshell.protocol.Mutation
import dev.nativeshell.protocol.Prop
import dev.nativeshell.protocol.PropValue
import dev.nativeshell.protocol.Protocol
import dev.nativeshell.protocol.ViewType
import dev.nativeshell.util.Json
import dev.nativeshell.views.ViewManagers

/**
 * Applies decoded batches to native views. Main thread only.
 *
 * Every node maps to one view in [nodes]. Views are never reparented by the
 * core (a flatten change recreates the subtree), so `Insert` of an attached
 * node only happens after a `Remove`. Released views go to a per-type pool
 * after every prop they had is reset to its default.
 */
internal class MountingManager(
    val host: MountHost,
    rootView: ShellViewGroup,
    private val managers: ViewManagers = ViewManagers(),
) {
    private val nodes = SparseArray<Node>()
    private val pools = SparseArray<ArrayDeque<Pair<View, Any?>>>()

    val root: Node = Node(Protocol.ROOT_NODE, ViewType.VIEW, managers.root, rootView)

    /** `seq` of the last batch applied, sent back with every event. */
    var lastSeq = 0
        private set

    /** Set while applying a batch with the ANIMATED flag. */
    var animating = false
        private set

    var batchesApplied = 0
        private set

    init {
        rootView.setTag(R.id.nsh_node_id, Protocol.ROOT_NODE)
        nodes.put(Protocol.ROOT_NODE, root)
    }

    fun node(id: Int): Node? = nodes.get(id)

    val nodeCount: Int get() = nodes.size()

    fun apply(batch: Batch) {
        animating = batch.isAnimated
        try {
            for (m in batch.mutations) {
                try {
                    applyOne(m)
                } catch (e: Exception) {
                    host.log(LogLevel.ERROR, "mutation $m failed: ${e.javaClass.simpleName}: ${e.message}")
                }
            }
        } finally {
            animating = false
        }
        lastSeq = batch.seq
        batchesApplied++
        RelayoutQueue.flush()
    }

    private fun applyOne(m: Mutation) {
        when (m) {
            is Mutation.BeginBatch, Mutation.EndBatch -> {}
            is Mutation.Create -> create(m.node, m.viewType)
            is Mutation.Insert -> insert(m.node, m.parent, m.index)
            is Mutation.Remove -> {
                val n = require(m.node) ?: return
                detach(n)
            }
            is Mutation.Delete -> {
                val n = require(m.node) ?: return
                if (n === root) return host.log(LogLevel.WARN, "cannot delete the root node")
                detach(n)
                release(n)
            }
            is Mutation.Move -> move(m.node, m.parent, m.to)
            is Mutation.SetProp -> setProp(m.node, m.prop, m.value)
            is Mutation.SetFrame -> {
                val n = require(m.node) ?: return
                if (n === root) return
                FrameApplier.setFrame(n, m.x, m.y, m.width, m.height, host.density, animating)
                n.manager.onFrame(n, host)
            }
            is Mutation.SetEventMask -> {
                val n = require(m.node) ?: return
                n.eventMask = m.mask
                TouchHandler.sync(n, host)
                Pointer.syncHover(n, host)
                n.manager.onEventMask(n, host)
            }
            is Mutation.Command -> command(m.node, m.command, m.payload)
        }
    }

    private fun require(id: Int): Node? {
        val n = nodes.get(id)
        if (n == null) host.log(LogLevel.WARN, "unknown node $id")
        return n
    }

    private fun create(id: Int, type: Int) {
        if (id == Protocol.ROOT_NODE) return host.log(LogLevel.WARN, "Create for the root node ignored")
        nodes.get(id)?.let {
            host.log(LogLevel.WARN, "node $id created twice; replacing")
            detach(it)
            release(it)
        }
        val manager = managers.forType(type)
        val pooled = pools.get(type)?.removeLastOrNull()
        val view = pooled?.first ?: manager.createView(host).also {
            if (dev.nativeshell.views.ControlTint.handles(it)) dev.nativeshell.views.ControlTint.apply(it)
        }
        val node = Node(id, type, manager, view)
        node.state = pooled?.second
        view.setTag(R.id.nsh_node_id, id)
        nodes.put(id, node)
        manager.onMount(node, host)
    }

    private fun insert(id: Int, parentId: Int, index: Int) {
        val child = require(id) ?: return
        val parent = require(parentId) ?: return
        if (child.parent != null) detach(child)
        if (isAncestor(child, parent)) return host.log(LogLevel.WARN, "insert of $child under its own descendant $parent")
        val at = index.coerceIn(0, parent.children.size)
        parent.children.add(at, child)
        child.parent = parent
        if (child.isSurface) {
            parent.surfaceChildren++
            host.presenter.attach(child, this)
        } else {
            parent.manager.insertChild(parent, child, parent.inlineIndexOf(child, at), host)
        }
    }

    private fun isAncestor(candidate: Node, of: Node): Boolean {
        var p: Node? = of
        while (p != null) {
            if (p === candidate) return true
            p = p.parent
        }
        return false
    }

    private fun move(id: Int, parentId: Int, to: Int) {
        val child = require(id) ?: return
        val parent = require(parentId) ?: return
        if (child.parent !== parent) return host.log(LogLevel.WARN, "move of $child: not a child of $parent")
        parent.children.remove(child)
        val at = to.coerceIn(0, parent.children.size)
        parent.children.add(at, child)
        if (!child.isSurface) parent.manager.moveChild(parent, child, parent.inlineIndexOf(child, at), host)
    }

    /** Detaches a node from its parent (logically and natively); it stays alive. */
    private fun detach(child: Node) {
        val parent = child.parent ?: return
        if (child.isSurface) {
            parent.surfaceChildren--
            host.presenter.detach(child, this)
        } else {
            parent.manager.removeChild(parent, child, host)
            (child.view.parent as? ViewGroup)?.removeView(child.view)
        }
        parent.children.remove(child)
        child.parent = null
    }

    /** Destroys a detached node and its subtree, resetting and pooling their views. */
    private fun release(node: Node) {
        for (c in node.children.toList()) {
            if (c.isSurface) host.presenter.detach(c, this)
            else node.manager.removeChild(node, c, host)
            c.parent = null
            release(c)
        }
        node.children.clear()
        nodes.remove(node.id)
        val view = node.view
        // A surface's view belongs to the presenter until its exit transition ends.
        if (!node.isSurface) {
            (view.parent as? ViewGroup)?.removeView(view)
            view.animate().cancel()
        }
        FrameApplier.cancelFrameAnimation(view)
        resetProps(node)
        if (node.eventMask != 0) {
            node.eventMask = 0
            TouchHandler.sync(node, host)
            Pointer.syncHover(node, host)
            node.manager.onEventMask(node, host)
        }
        node.manager.onRecycle(node, host)
        view.setTag(R.id.nsh_node_id, null)
        if (node.manager.poolable) {
            val pool = pools.get(node.type) ?: ArrayDeque<Pair<View, Any?>>().also { pools.put(node.type, it) }
            if (pool.size < POOL_LIMIT) pool.addLast(view to node.state)
        }
    }

    /** Sends `Null` for every prop the node had set: the reset contract. */
    fun resetProps(node: Node) {
        var p = node.setProps.nextSetBit(0)
        while (p >= 0) {
            applyProp(node, p, PropValue.NULL)
            p = node.setProps.nextSetBit(p + 1)
        }
        node.setProps.clear()
    }

    private fun setProp(id: Int, prop: Int, value: PropValue) {
        val node = require(id) ?: return
        if (value.isNull) node.setProps.clear(prop) else node.setProps.set(prop)
        applyProp(node, prop, value)
        if (node.isSurface && node.parent != null && prop in SURFACE_PROPS) host.presenter.surfaceChanged(node)
    }

    private fun applyProp(node: Node, prop: Int, value: PropValue) {
        if (node.manager.setProp(node, prop, value, host)) return
        if (CommonProps.set(node, prop, value, host, animating)) return
        host.log(LogLevel.DEBUG, "${ViewType.name(node.type)} ignores prop $prop")
    }

    private fun command(id: Int, command: Int, payload: String) {
        val node = require(id) ?: return
        val json = Json.obj(payload)
        if (command == Command.PRESENT_PAGE || command == Command.DISMISS_PAGE) {
            if (!host.presenter.command(node, command, json)) host.log(LogLevel.WARN, "page command on $node ignored")
            return
        }
        if (node.manager.command(node, command, json, host)) return
        if (RootCommands.run(command, json, host)) return
        host.log(LogLevel.WARN, "command $command on $node ignored")
    }

    /**
     * Routes a hardware key event: to the nearest node listening for `Key` on the
     * focus path, else to the root (app-wide bindings). Never consumes it.
     */
    fun dispatchKey(focused: View?, e: android.view.KeyEvent) {
        val payload = KeyEvents.payload(e) ?: return
        var v: Any? = focused
        while (v is View) {
            val id = v.getTag(R.id.nsh_node_id) as? Int
            val n = if (id != null) nodes.get(id) else null
            if (n != null && n.eventMask and dev.nativeshell.protocol.EventMask.KEY != 0) {
                return host.dispatch(n.id, dev.nativeshell.protocol.EventType.KEY, payload)
            }
            v = v.parent
        }
        host.dispatch(Protocol.ROOT_NODE, dev.nativeshell.protocol.EventType.KEY, payload)
    }

    /** Appearance or theme changed: re-tint native controls from the current tokens. */
    fun retintControls() {
        for (i in 0 until nodes.size()) {
            val v = nodes.valueAt(i).view
            if (dev.nativeshell.views.ControlTint.handles(v)) dev.nativeshell.views.ControlTint.apply(v)
            (v as? dev.nativeshell.views.ValueField)?.applyTokens()
        }
    }

    /**
     * The view of the node carrying `TestId` [testId] that is actually on screen:
     * attached to the window and reachable from the root (a list row parked
     * outside the visible cells, or a node awaiting deletion, does not count).
     */
    fun findByTestId(testId: String): View? = nodeForTestId(testId)?.view

    fun nodeForTestId(testId: String): Node? {
        var fallback: Node? = null
        for (i in 0 until nodes.size()) {
            val n = nodes.valueAt(i)
            if (n.testId != testId) continue
            if (n.view.isAttachedToWindow && isMounted(n)) return n
            // A surface not attached to a window (headless tests) resolves by tree membership alone.
            if (fallback == null && !root.view.isAttachedToWindow && isMounted(n) && n.view.parent != null) fallback = n
        }
        return fallback
    }

    private fun isMounted(n: Node): Boolean {
        var c: Node = n
        while (true) c = c.parent ?: return c === root
    }

    /** The native tree: types, frames in points, test ids; detached nodes listed after. Debugging and tests. */
    fun dump(): String {
        val sb = StringBuilder()
        val seen = HashSet<Int>()
        fun walk(n: Node, depth: Int) {
            seen += n.id
            sb.append("  ".repeat(depth)).append(n).append(" [")
                .append(n.left / host.density).append(',').append(n.top / host.density).append(' ')
                .append(n.widthPt).append('x').append(n.heightPt).append(']')
            n.testId?.let { sb.append(" #").append(it) }
            if (n.state is Int) sb.append(" row=").append(n.state)
            sb.append('\n')
            for (c in n.children) walk(c, depth + 1)
        }
        walk(root, 0)
        for (i in 0 until nodes.size()) {
            val n = nodes.valueAt(i)
            if (n.id !in seen && n.parent == null) { sb.append("(detached) "); walk(n, 0) }
        }
        return sb.toString()
    }

    fun pooledCount(type: Int): Int = pools.get(type)?.size ?: 0

    /** Destroys every node (instance teardown). */
    fun clear() {
        for (c in root.children.toList()) {
            detach(c)
            release(c)
        }
        pools.clear()
    }

    companion object {
        const val POOL_LIMIT = 128
        private val SURFACE_PROPS = setOf(
            Prop.TITLE, Prop.CAN_GO_BACK, Prop.NAV_BAR_HIDDEN, Prop.DETENTS, Prop.DISMISSABLE, Prop.ALERT_SPEC,
        )
    }
}
