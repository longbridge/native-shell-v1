package dev.nativeshell.render

import android.animation.Animator
import android.animation.AnimatorListenerAdapter
import android.animation.TimeInterpolator
import android.animation.ValueAnimator
import android.view.View
import android.view.ViewGroup
import android.view.animation.AccelerateDecelerateInterpolator
import android.view.animation.AccelerateInterpolator
import android.view.animation.DecelerateInterpolator
import android.view.animation.LinearInterpolator
import androidx.dynamicanimation.animation.DynamicAnimation
import androidx.dynamicanimation.animation.FloatValueHolder
import androidx.dynamicanimation.animation.SpringAnimation
import androidx.dynamicanimation.animation.SpringForce
import dev.nativeshell.R
import dev.nativeshell.util.Json
import org.json.JSONObject
import kotlin.math.PI
import kotlin.math.roundToInt

/** One declared transition (`.transition(prop, …)` / `.spring(prop, …)`). Durations in ms, response in s. */
internal class Anim(
    val spring: Boolean,
    val durationMs: Long,
    val delayMs: Long,
    val easing: String,
    val response: Float,
    val damping: Float,
) {
    fun interpolator(): TimeInterpolator = when (easing) {
        "linear" -> LinearInterpolator()
        "ease_in" -> AccelerateInterpolator()
        "ease_out" -> DecelerateInterpolator()
        else -> AccelerateDecelerateInterpolator()
    }

    companion object {
        fun parse(o: JSONObject?): Anim? {
            if (o == null) return null
            return Anim(
                spring = Json.string(o, "kind") == "spring",
                durationMs = Json.float(o, "duration", 250f).toLong().coerceIn(0, 10_000),
                delayMs = Json.float(o, "delay", 0f).toLong().coerceIn(0, 10_000),
                easing = Json.string(o, "easing") ?: "ease_in_out",
                response = Json.float(o, "response", 0.35f).coerceIn(0.01f, 10f),
                damping = Json.float(o, "damping", 0.825f).coerceIn(0.01f, 10f),
            )
        }
    }
}

/** The `Animation` prop: which of a node's frame and opacity animate in ANIMATED batches. */
internal class AnimationSpec(val frame: Anim?, val opacity: Anim?) {
    companion object {
        fun parse(json: String?): AnimationSpec? {
            val o = Json.obj(json) ?: return null
            val f = Anim.parse(o.optJSONObject("frame"))
            val a = Anim.parse(o.optJSONObject("opacity"))
            return if (f == null && a == null) null else AnimationSpec(f, a)
        }
    }
}

/** Places node views at their frames, immediately or animated. */
internal object FrameApplier {

    fun setFrame(node: Node, x: Float, y: Float, w: Float, h: Float, density: Float, animated: Boolean) {
        val sx = if (x.isFinite()) x else 0f
        val sy = if (y.isFinite()) y else 0f
        val sw = if (w.isFinite()) w.coerceAtLeast(0f) else 0f
        val sh = if (h.isFinite()) h.coerceAtLeast(0f) else 0f
        val l = (sx * density).roundToInt()
        val t = (sy * density).roundToInt()
        val r = ((sx + sw) * density).roundToInt()
        val b = ((sy + sh) * density).roundToInt()
        val anim = node.animation?.frame
        val wasPlaced = node.hasFrame && node.view.parent != null
        val from = intArrayOf(node.left, node.top, node.left + node.width, node.top + node.height)
        node.left = l; node.top = t; node.width = r - l; node.height = b - t
        node.widthPt = sw; node.heightPt = sh
        node.hasFrame = true
        val parent = node.view.parent
        if (animated && anim != null && wasPlaced && parent is ShellViewGroup && !Motion.reduced(node.view.context) &&
            (from[0] != l || from[1] != t || from[2] != r || from[3] != b)
        ) {
            updateParams(node)
            animateFrame(node, from, intArrayOf(l, t, r, b), anim)
        } else {
            cancelFrameAnimation(node.view)
            layoutNow(node)
        }
    }

    private fun updateParams(node: Node) {
        val lp = node.view.layoutParams
        if (lp is ShellLayoutParams) {
            lp.left = node.left; lp.top = node.top; lp.width = node.width; lp.height = node.height
        }
    }

    /** Applies the node's frame to its view without waiting for a layout pass. */
    fun layoutNow(node: Node) {
        val v = node.view
        when (val lp = v.layoutParams) {
            is ShellLayoutParams -> {
                lp.left = node.left; lp.top = node.top; lp.width = node.width; lp.height = node.height
                if (v.parent is ShellViewGroup && !v.isAnimatingFrame()) {
                    ShellViewGroup.measureExactly(v, node.width, node.height)
                    v.layout(node.left, node.top, node.left + node.width, node.top + node.height)
                }
            }
            null -> {}
            else -> if (v.parent != null && (lp.width != node.width || lp.height != node.height)) {
                // Presented surfaces and list cells: their container lays them out.
                lp.width = node.width
                lp.height = node.height
                v.layoutParams = lp
            }
        }
    }

    private fun animateFrame(node: Node, from: IntArray, to: IntArray, anim: Anim) {
        val v = node.view
        cancelFrameAnimation(v)
        val step: (Float) -> Unit = { f ->
            val l = lerp(from[0], to[0], f); val t = lerp(from[1], to[1], f)
            val r = lerp(from[2], to[2], f); val b = lerp(from[3], to[3], f)
            ShellViewGroup.measureExactly(v, r - l, b - t)
            v.layout(l, t, r, b)
        }
        val end = {
            v.setTag(R.id.nsh_frame_animator, null)
            layoutNow(node)
        }
        if (anim.spring) {
            val holder = FloatValueHolder(0f)
            val spring = SpringAnimation(holder).setSpring(springForce(anim))
            spring.addUpdateListener { _, value, _ -> step(value) }
            spring.addEndListener { _, canceled, _, _ -> if (!canceled) end() }
            v.setTag(R.id.nsh_frame_animator, Cancelable { spring.cancel(); v.setTag(R.id.nsh_frame_animator, null) })
            spring.setStartValue(0f)
            if (anim.delayMs > 0) v.postDelayed({ spring.animateToFinalPosition(1f) }, anim.delayMs) else spring.animateToFinalPosition(1f)
        } else {
            val va = ValueAnimator.ofFloat(0f, 1f)
            va.duration = anim.durationMs
            va.startDelay = anim.delayMs
            va.interpolator = anim.interpolator()
            va.addUpdateListener { step(it.animatedValue as Float) }
            va.addListener(object : AnimatorListenerAdapter() {
                var canceled = false
                override fun onAnimationCancel(animation: Animator) { canceled = true }
                override fun onAnimationEnd(animation: Animator) { if (!canceled) end() }
            })
            v.setTag(R.id.nsh_frame_animator, Cancelable { va.cancel(); v.setTag(R.id.nsh_frame_animator, null) })
            va.start()
        }
    }

    fun cancelFrameAnimation(v: View) {
        (v.getTag(R.id.nsh_frame_animator) as? Cancelable)?.cancel()
    }

    /** Sets alpha, animating when the batch is animated and the node declares an opacity transition. */
    fun setAlpha(node: Node, alpha: Float, animated: Boolean) {
        val v = node.view
        val anim = node.animation?.opacity
        node.baseAlpha = alpha
        if (!animated || anim == null || v.parent == null) {
            v.animate().cancel()
            v.alpha = alpha
            return
        }
        if (Motion.reduced(v.context)) {
            // Fades still explain the change; keep them short and never springy.
            v.animate().alpha(alpha).setDuration(minOf(anim.durationMs, Motion.REDUCED_FADE_MS)).setStartDelay(0)
                .setInterpolator(LinearInterpolator()).start()
        } else if (anim.spring) {
            SpringAnimation(v, DynamicAnimation.ALPHA).setSpring(springForce(anim).setFinalPosition(alpha)).start()
        } else {
            v.animate().alpha(alpha).setDuration(anim.durationMs).setStartDelay(anim.delayMs)
                .setInterpolator(anim.interpolator()).start()
        }
    }

    private fun springForce(anim: Anim): SpringForce {
        // SwiftUI-style response (period, s) → stiffness for unit mass.
        val omega = 2 * PI / anim.response
        return SpringForce(1f).setStiffness((omega * omega).toFloat().coerceAtLeast(1f)).setDampingRatio(anim.damping)
    }

    private fun lerp(a: Int, b: Int, f: Float) = (a + (b - a) * f).roundToInt()
}

internal fun interface Cancelable {
    fun cancel()
}

internal fun ViewGroup.childIndex(v: View) = indexOfChild(v)
