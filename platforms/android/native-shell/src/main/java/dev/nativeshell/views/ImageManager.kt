package dev.nativeshell.views

import android.content.res.ColorStateList
import android.view.View
import android.view.ViewGroup
import android.widget.ImageView
import androidx.appcompat.widget.AppCompatImageView
import androidx.core.widget.ImageViewCompat
import dev.nativeshell.protocol.Prop
import dev.nativeshell.protocol.PropValue
import dev.nativeshell.protocol.ValueKind
import dev.nativeshell.protocol.ViewType
import dev.nativeshell.render.MountHost
import dev.nativeshell.render.Node
import dev.nativeshell.render.ViewManager

internal class ImageManager : ViewManager(ViewType.IMAGE) {
    override fun createView(host: MountHost): View = AppCompatImageView(host.context).apply {
        scaleType = ImageView.ScaleType.CENTER_CROP
    }

    override fun container(node: Node): ViewGroup? = null

    override fun setProp(node: Node, prop: Int, value: PropValue, host: MountHost): Boolean {
        val v = node.view as ImageView
        when (prop) {
            Prop.IMAGE_SOURCE -> {
                val src = value.string
                if (value.isNull || src.isNullOrEmpty()) host.images.cancel(v) else host.images.load(src, v)
            }
            Prop.CONTENT_MODE -> v.scaleType = when (if (value.isNull) 0 else value.intValue()) {
                1 -> ImageView.ScaleType.FIT_CENTER
                2 -> ImageView.ScaleType.FIT_XY
                3 -> ImageView.ScaleType.CENTER
                else -> ImageView.ScaleType.CENTER_CROP
            }
            Prop.TINT_COLOR -> ImageViewCompat.setImageTintList(
                v, if (value.kind == ValueKind.COLOR) ColorStateList.valueOf(value.int) else null,
            )
            else -> return false
        }
        return true
    }
}
