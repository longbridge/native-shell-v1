package dev.nativeshell.views

import android.view.View
import dev.nativeshell.protocol.Prop
import dev.nativeshell.protocol.PropValue
import dev.nativeshell.render.MountHost
import dev.nativeshell.render.Node
import dev.nativeshell.render.ShellViewGroup
import dev.nativeshell.render.ViewManager
import dev.nativeshell.util.Json

/** Presentation props of a page, sheet or dialog node. */
internal class SurfaceProps {
    var title: String = ""
    var canGoBack = true
    var navBarHidden = false
    var detents: List<String> = emptyList()
    /** A `{"height": h}` detent, in points. */
    var detentHeight: Float? = null
    var dismissable = true
    var alertSpec: String? = null
}

internal val Node.surfaceProps: SurfaceProps
    get() = state as? SurfaceProps ?: SurfaceProps().also { state = it }

/**
 * Page, Sheet and Dialog nodes: a container whose presentation (page stack,
 * bottom sheet, dialog window) is up to the surface presenter. Not pooled: a
 * dismissed page may still be animating out when it is deleted.
 */
internal class SurfaceManager(type: Int) : ViewManager(type) {
    override fun createView(host: MountHost): View = ShellViewGroup(host.context)

    override val poolable: Boolean get() = false

    override fun setProp(node: Node, prop: Int, value: PropValue, host: MountHost): Boolean {
        val p = node.surfaceProps
        when (prop) {
            Prop.TITLE -> {
                p.title = if (value.isNull) "" else value.string ?: ""
                androidx.core.view.ViewCompat.setAccessibilityPaneTitle(node.view, p.title.ifEmpty { null })
            }
            Prop.CAN_GO_BACK -> p.canGoBack = value.isNull || value.bool
            Prop.NAV_BAR_HIDDEN -> p.navBarHidden = !value.isNull && value.bool
            Prop.DETENTS -> {
                val a = if (value.isNull) null else Json.arr(value.string)
                p.detents = (0 until (a?.length() ?: 0)).mapNotNull { a!!.opt(it) as? String }
                p.detentHeight = (0 until (a?.length() ?: 0)).mapNotNull { a!!.optJSONObject(it)?.optDouble("height")?.toFloat() }
                    .firstOrNull { it.isFinite() && it > 0f }
            }
            Prop.DISMISSABLE -> p.dismissable = value.isNull || value.bool
            Prop.ALERT_SPEC -> p.alertSpec = if (value.isNull) null else value.string
            else -> return false
        }
        return true
    }
}
