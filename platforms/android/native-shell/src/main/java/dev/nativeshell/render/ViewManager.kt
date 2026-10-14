package dev.nativeshell.render

import android.content.Context
import android.view.View
import android.view.ViewGroup
import dev.nativeshell.protocol.PropValue
import org.json.JSONObject

/**
 * What mounting needs from the surface it renders into. Implemented by
 * [dev.nativeshell.ShellSurfaceView]'s renderer and by test fakes.
 */
internal interface MountHost {
    /** Context wrapped in the shell's Material theme: every view is created from it. */
    val context: Context
    val density: Float

    /** Sends an event to the script with the seq of the last applied batch. */
    fun dispatch(node: Int, eventType: Int, payload: String)

    /** Presents pages, sheets and dialogs. */
    val presenter: SurfacePresenter

    /** Resolves image sources (bundle-relative paths) and loads them. */
    val images: ImageResolver

    fun log(level: Int, message: String)

    /** The surface view, for commands that need a window (haptics, IME). */
    val anchor: View?

    /** The surface's toast stack, or null without a surface (a platform toast is used then). */
    val toasts: ToastStack? get() = null
}

/** Presentation of surface nodes inserted under the root. */
internal interface SurfacePresenter {
    fun attach(node: Node, mounting: MountingManager)
    fun detach(node: Node, mounting: MountingManager)
    fun command(node: Node, command: Int, payload: JSONObject?): Boolean
    /** A surface prop that changes presentation (title, detents, alert spec…) changed. */
    fun surfaceChanged(node: Node) {}
}

/**
 * Behavior of one view type: creating its view, setting its props, hosting
 * children. Managers are stateless; per-node state lives in [Node.state].
 *
 * The reset contract: [setProp] with a `Null` value restores that prop's
 * default exactly. Pooling relies on it: a released view gets `Null` for every
 * prop it had set and must then be indistinguishable from a new one.
 */
internal abstract class ViewManager(val type: Int) {

    abstract fun createView(host: MountHost): View

    /** Called once per node after creation or reuse from the pool. */
    open fun onMount(node: Node, host: MountHost) {}

    /** Handles a type-specific prop; returns false to fall back to the common props. */
    open fun setProp(node: Node, prop: Int, value: PropValue, host: MountHost): Boolean = false

    /** Where native children go, or null for leaves. */
    open fun container(node: Node): ViewGroup? = node.view as? ViewGroup

    open fun insertChild(node: Node, child: Node, index: Int, host: MountHost) {
        val c = container(node) ?: run {
            host.log(2, "$node cannot have children (inserting $child)")
            return
        }
        (child.view.parent as? ViewGroup)?.removeView(child.view)
        c.addView(child.view, index.coerceIn(0, c.childCount), child.layoutParamsFor(c))
        if (child.hasFrame) FrameApplier.layoutNow(child)
    }

    open fun removeChild(node: Node, child: Node, host: MountHost) {
        container(node)?.removeView(child.view)
    }

    open fun moveChild(node: Node, child: Node, to: Int, host: MountHost) {
        val c = container(node) ?: return
        c.removeView(child.view)
        c.addView(child.view, to.coerceIn(0, c.childCount), child.layoutParamsFor(c))
        if (child.hasFrame) FrameApplier.layoutNow(child)
    }

    /** The event mask changed: attach or detach listeners for the new bits. */
    open fun onEventMask(node: Node, host: MountHost) {}

    open fun command(node: Node, command: Int, payload: JSONObject?, host: MountHost): Boolean = false

    /** The node's frame changed (already applied to the view). */
    open fun onFrame(node: Node, host: MountHost) {}

    /** Before the view goes back to the pool, after every set prop was reset. */
    open fun onRecycle(node: Node, host: MountHost) {}

    /** Whether released views of this type may be reused. */
    open val poolable: Boolean get() = true
}

internal fun Node.layoutParamsFor(container: ViewGroup): ViewGroup.LayoutParams =
    if (container is ShellViewGroup) ShellLayoutParams(left, top, width, height)
    else ViewGroup.LayoutParams(if (hasFrame) width else ViewGroup.LayoutParams.MATCH_PARENT, if (hasFrame) height else ViewGroup.LayoutParams.MATCH_PARENT)
