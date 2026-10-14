package dev.nativeshell.views

import android.view.View
import dev.nativeshell.protocol.ViewType
import dev.nativeshell.protocol.Prop
import dev.nativeshell.protocol.PropValue
import dev.nativeshell.render.MountHost
import dev.nativeshell.render.Node
import dev.nativeshell.render.ShellViewGroup
import dev.nativeshell.render.ViewManager

/** One manager per view type. */
internal class ViewManagers {
    val root: ViewManager = object : ViewManager(ViewType.VIEW) {
        override fun createView(host: MountHost): View = error("the root view is created by the surface")
        override fun setProp(node: Node, prop: Int, value: PropValue, host: MountHost): Boolean {
            if (prop != Prop.THEME_TOKENS) return false
            host.setThemeTokens(if (value.isNull) null else value.string)
            return true
        }
        override val poolable get() = false
    }

    private val byType: Array<ViewManager?> = arrayOfNulls<ViewManager>(ViewType.HOST_COMPONENT + 1).also {
        it[ViewType.VIEW] = BoxManager(ViewType.VIEW)
        it[ViewType.LIST_ROW] = ListRowManager()
        it[ViewType.TEXT] = TextManager()
        it[ViewType.IMAGE] = ImageManager()
        it[ViewType.SCROLL] = ScrollManager()
        it[ViewType.TEXT_INPUT] = TextInputManager()
        it[ViewType.SWITCH] = SwitchManager()
        it[ViewType.CHECKBOX] = CheckboxManager()
        it[ViewType.SLIDER] = SliderManager()
        it[ViewType.PROGRESS] = ProgressManager()
        it[ViewType.SPINNER] = SpinnerManager()
        it[ViewType.LIST] = ListManager()
        it[ViewType.PAGE] = SurfaceManager(ViewType.PAGE)
        it[ViewType.SHEET] = SurfaceManager(ViewType.SHEET)
        it[ViewType.DIALOG] = SurfaceManager(ViewType.DIALOG)
        it[ViewType.SEGMENTED] = SegmentedManager()
        it[ViewType.DATE_PICKER] = DatePickerManager()
        it[ViewType.SELECT] = SelectManager()
        it[ViewType.SHAPE] = ShapeManager()
        it[ViewType.HOST_COMPONENT] = HostComponentManager()
    }

    fun forType(type: Int): ViewManager = byType.getOrNull(type) ?: error("no manager for view type $type")
}

/** `View` and `ListRow`: a plain container. */
internal class BoxManager(type: Int) : ViewManager(type) {
    override fun createView(host: MountHost): View = ShellViewGroup(host.context)
}
