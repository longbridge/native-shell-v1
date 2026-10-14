package dev.nativeshell.views

import android.annotation.SuppressLint
import android.content.Context
import android.view.View
import android.view.ViewGroup
import android.widget.FrameLayout
import dev.nativeshell.EventSink
import dev.nativeshell.HostComponent
import dev.nativeshell.LogLevel
import dev.nativeshell.NativeShell
import dev.nativeshell.protocol.EventMask
import dev.nativeshell.protocol.EventType
import dev.nativeshell.protocol.Prop
import dev.nativeshell.protocol.PropValue
import dev.nativeshell.protocol.ViewType
import dev.nativeshell.render.MountHost
import dev.nativeshell.render.Node
import dev.nativeshell.render.ViewManager
import dev.nativeshell.util.Json
import org.json.JSONObject

/** Hosts one app-registered component view, filling the node's frame. */
@SuppressLint("ViewConstructor")
internal class HostComponentContainer(context: Context) : FrameLayout(context) {
    var name: String? = null
    var component: HostComponent? = null
    var componentView: View? = null
}

/**
 * `HostComponent` nodes. The registered name arrives right after `Create`; the
 * component (one object per native view) comes from a pool per name, and goes
 * back to it after [HostComponent.reset] when the node is released.
 */
internal class HostComponentManager : ViewManager(ViewType.HOST_COMPONENT) {
    private val pools = HashMap<String, ArrayDeque<Pair<HostComponent, View>>>()

    override fun createView(host: MountHost): View = HostComponentContainer(host.context)

    override fun container(node: Node): ViewGroup? = null

    override fun setProp(node: Node, prop: Int, value: PropValue, host: MountHost): Boolean {
        val c = node.view as HostComponentContainer
        when (prop) {
            Prop.HOST_COMPONENT -> {
                val name = if (value.isNull) null else value.string
                if (name != c.name) {
                    release(c)
                    if (name != null) acquire(node, c, name, host)
                }
            }
            Prop.HOST_PROPS -> {
                val comp = c.component ?: return true
                val view = c.componentView ?: return true
                val props = (if (value.isNull) null else Json.obj(value.string)) ?: JSONObject()
                try {
                    comp.update(view, props)
                } catch (e: Exception) {
                    host.log(LogLevel.ERROR, "host component ${c.name}: update failed: ${e.message}")
                }
            }
            else -> return false
        }
        return true
    }

    private fun acquire(node: Node, c: HostComponentContainer, name: String, host: MountHost) {
        val spec = NativeShell.component(name)
        if (spec == null) {
            host.log(LogLevel.ERROR, "host component `$name` is not registered")
            return
        }
        val (comp, view) = pools[name]?.removeLastOrNull() ?: try {
            val comp = spec.factory.create()
            comp to comp.createView(host.context)
        } catch (e: Exception) {
            host.log(LogLevel.ERROR, "host component `$name`: create failed: ${e.message}")
            return
        }
        c.name = name
        c.component = comp
        c.componentView = view
        (view.parent as? ViewGroup)?.removeView(view)
        c.addView(view, FrameLayout.LayoutParams(FrameLayout.LayoutParams.MATCH_PARENT, FrameLayout.LayoutParams.MATCH_PARENT))
        comp.setEventSink(view, EventSink { event, payload ->
            if (node.eventMask and EventMask.HOST_EVENT != 0 && c.component === comp) {
                val json = "{\"name\":${Json.stringify(event)},\"payload\":${Json.stringify(payload)}}"
                // Components may emit from any thread; events are dispatched on the main thread.
                if (android.os.Looper.myLooper() == android.os.Looper.getMainLooper()) host.dispatch(node.id, EventType.HOST_EVENT, json)
                else c.post { host.dispatch(node.id, EventType.HOST_EVENT, json) }
            }
        })
    }

    private fun release(c: HostComponentContainer) {
        val name = c.name ?: return
        val comp = c.component
        val view = c.componentView
        c.removeAllViews()
        c.name = null
        c.component = null
        c.componentView = null
        if (comp != null && view != null) {
            try {
                comp.reset(view)
                comp.setEventSink(view) { _, _ -> }
                val pool = pools.getOrPut(name) { ArrayDeque() }
                if (pool.size < 32) pool.addLast(comp to view)
            } catch (e: Exception) {
                // A component that cannot reset is not reused.
            }
        }
    }

    override fun onRecycle(node: Node, host: MountHost) {
        release(node.view as HostComponentContainer)
    }

    /** Pooled component views per name (tests). */
    fun pooled(name: String): Int = pools[name]?.size ?: 0
}
