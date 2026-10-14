package dev.nativeshell.render

import android.annotation.SuppressLint
import android.os.SystemClock
import android.view.MotionEvent
import android.view.ScaleGestureDetector
import android.view.VelocityTracker
import android.view.View
import android.view.ViewConfiguration
import dev.nativeshell.protocol.EventMask
import dev.nativeshell.protocol.EventType
import dev.nativeshell.protocol.ViewType
import kotlin.math.hypot

/** Views that handle taps on parts of themselves (text links) before the node's gestures. */
internal interface LinkTarget {
    fun hasLinkAt(x: Float, y: Float): Boolean
}

/**
 * Gestures and press feedback of a plain node, attached only when its event
 * mask asks for touch events or it declares `ActiveOpacity`/`ActiveBackground`,
 * so views without handlers never consume touches.
 */
@SuppressLint("ClickableViewAccessibility")
internal class TouchHandler private constructor(private val node: Node, private val host: MountHost) : View.OnTouchListener {

    private val view = node.view
    private val config = ViewConfiguration.get(view.context)
    private val slop = config.scaledTouchSlop
    private var downX = 0f
    private var downY = 0f
    private var pressed = false
    private var longPressFired = false
    private var panning = false
    private var pinching = false
    private var velocity: VelocityTracker? = null
    private var lastTapTime = 0L
    private var lastTapX = 0f
    private var lastTapY = 0f
    private var tapCount = 0
    private val longPress = Runnable {
        longPressFired = true
        send(EventType.LONG_PRESS, "{\"x\":${pt(downX)},\"y\":${pt(downY)}}")
        view.performHapticFeedback(android.view.HapticFeedbackConstants.LONG_PRESS)
    }
    private var scaleDetector: ScaleGestureDetector? = null
    private var scaleTotal = 1f

    private val mask get() = node.eventMask

    override fun onTouch(v: View, ev: MotionEvent): Boolean {
        if (ev.actionMasked == MotionEvent.ACTION_DOWN && v is LinkTarget && v.hasLinkAt(ev.x, ev.y)) return false
        if (mask and EventMask.PINCH != 0) pinchDetector().onTouchEvent(ev)
        when (ev.actionMasked) {
            MotionEvent.ACTION_DOWN -> {
                downX = ev.x; downY = ev.y
                longPressFired = false; panning = false
                velocity?.recycle()
                velocity = VelocityTracker.obtain().also { it.addMovement(ev) }
                setPressed(true)
                if (mask and EventMask.PRESS_IN != 0) send(EventType.PRESS_IN, point(ev.x, ev.y))
                if (mask and EventMask.LONG_PRESS != 0) view.postDelayed(longPress, ViewConfiguration.getLongPressTimeout().toLong())
                if (mask and (EventMask.PAN or EventMask.PINCH) != 0) view.parent?.requestDisallowInterceptTouchEvent(true)
            }
            MotionEvent.ACTION_MOVE -> {
                velocity?.addMovement(ev)
                val dx = ev.x - downX
                val dy = ev.y - downY
                if (!panning && hypot(dx, dy) > slop) {
                    view.removeCallbacks(longPress)
                    if (mask and EventMask.PAN != 0 && !pinching) {
                        panning = true
                        sendPan("began", dx, dy)
                    } else if (mask and EventMask.PAN == 0) {
                        setPressed(false)
                    }
                } else if (panning && !pinching) {
                    sendPan("changed", dx, dy)
                }
            }
            MotionEvent.ACTION_UP -> {
                view.removeCallbacks(longPress)
                velocity?.addMovement(ev)
                val wasPressed = pressed
                setPressed(false)
                if (mask and EventMask.PRESS_OUT != 0) send(EventType.PRESS_OUT, point(ev.x, ev.y))
                if (panning) {
                    sendPan("ended", ev.x - downX, ev.y - downY)
                } else if (wasPressed && !longPressFired && !pinching) {
                    tap(ev)
                }
                panning = false
                velocity?.recycle(); velocity = null
            }
            MotionEvent.ACTION_CANCEL -> {
                view.removeCallbacks(longPress)
                if (pressed && mask and EventMask.PRESS_OUT != 0) send(EventType.PRESS_OUT, point(ev.x, ev.y))
                setPressed(false)
                if (panning) sendPan("cancelled", ev.x - downX, ev.y - downY)
                panning = false
                velocity?.recycle(); velocity = null
            }
        }
        return true
    }

    private fun tap(ev: MotionEvent) {
        val now = SystemClock.uptimeMillis()
        tapCount = if (now - lastTapTime <= ViewConfiguration.getDoubleTapTimeout() &&
            hypot(ev.x - lastTapX, ev.y - lastTapY) < config.scaledDoubleTapSlop
        ) tapCount + 1 else 1
        lastTapTime = now; lastTapX = ev.x; lastTapY = ev.y
        if (mask and EventMask.CLICK != 0) {
            val mods = Pointer.modifiers(ev)?.let { ",\"modifiers\":$it" } ?: ""
            send(EventType.CLICK, "{\"x\":${pt(ev.x)},\"y\":${pt(ev.y)},\"count\":$tapCount$mods}")
        }
        if (tapCount == 2 && mask and EventMask.DOUBLE_CLICK != 0) {
            send(EventType.DOUBLE_CLICK, point(ev.x, ev.y))
        }
    }

    private fun sendPan(phase: String, dx: Float, dy: Float) {
        val vt = velocity
        var vx = 0f
        var vy = 0f
        if (vt != null) {
            vt.computeCurrentVelocity(1000)
            vx = vt.xVelocity; vy = vt.yVelocity
        }
        send(EventType.PAN, "{\"phase\":\"$phase\",\"dx\":${pt(dx)},\"dy\":${pt(dy)},\"vx\":${pt(vx)},\"vy\":${pt(vy)}}")
    }

    private fun pinchDetector(): ScaleGestureDetector = scaleDetector ?: ScaleGestureDetector(
        view.context,
        object : ScaleGestureDetector.SimpleOnScaleGestureListener() {
            override fun onScaleBegin(detector: ScaleGestureDetector): Boolean {
                pinching = true
                scaleTotal = 1f
                view.removeCallbacks(longPress)
                send(EventType.PINCH, "{\"phase\":\"began\",\"scale\":1}")
                return true
            }

            override fun onScale(detector: ScaleGestureDetector): Boolean {
                scaleTotal *= detector.scaleFactor
                send(EventType.PINCH, "{\"phase\":\"changed\",\"scale\":$scaleTotal}")
                return true
            }

            override fun onScaleEnd(detector: ScaleGestureDetector) {
                send(EventType.PINCH, "{\"phase\":\"ended\",\"scale\":$scaleTotal}")
                pinching = false
            }
        },
    ).also { scaleDetector = it }

    private fun setPressed(p: Boolean) {
        if (pressed == p) return
        pressed = p
        node.pressed = p
        Pointer.applyFeedback(node)
    }

    private fun point(x: Float, y: Float) = "{\"x\":${pt(x)},\"y\":${pt(y)}}"

    private fun send(type: Int, payload: String) = host.dispatch(node.id, type, payload)

    private fun pt(px: Float): Float = px / host.density

    companion object {
        /** Attaches or removes the handler to match the node's mask and press props. */
        fun sync(node: Node, host: MountHost) {
            if (!supportsGestures(node.type)) return
            val wants = node.eventMask and EventMask.TOUCH != 0 || !node.activeOpacity.isNaN() || node.activeBackground != null
            val view = node.view
            if (wants && node.touch == null) {
                val h = TouchHandler(node, host)
                node.touch = h
                view.setOnTouchListener(h)
            } else if (!wants && node.touch != null) {
                node.touch = null
                view.setOnTouchListener(null)
                view.isPressed = false
            }
            val clickable = node.eventMask and EventMask.CLICK != 0
            if (clickable) {
                // Accessibility services activate a view through performClick.
                view.setOnClickListener {
                    host.dispatch(node.id, EventType.CLICK, "{\"x\":${it.width / 2f / host.density},\"y\":${it.height / 2f / host.density},\"count\":1}")
                }
            } else if (view.hasOnClickListeners()) {
                view.setOnClickListener(null)
                view.isClickable = false
            }
            if (node.eventMask and EventMask.LONG_PRESS != 0) {
                view.setOnLongClickListener { host.dispatch(node.id, EventType.LONG_PRESS, "{\"x\":${it.width / 2f / host.density},\"y\":${it.height / 2f / host.density}}"); true }
            } else {
                view.setOnLongClickListener(null)
                view.isLongClickable = false
            }
        }

        fun supportsGestures(type: Int): Boolean = when (type) {
            ViewType.VIEW, ViewType.TEXT, ViewType.IMAGE, ViewType.SHAPE, ViewType.LIST_ROW, ViewType.HOST_COMPONENT -> true
            else -> false
        }
    }
}
