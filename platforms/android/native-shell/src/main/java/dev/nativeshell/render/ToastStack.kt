package dev.nativeshell.render

import android.annotation.SuppressLint
import android.content.Context
import android.graphics.Canvas
import android.graphics.Paint
import android.graphics.Path
import android.graphics.Typeface
import android.graphics.drawable.GradientDrawable
import android.util.TypedValue
import android.view.Gravity
import android.view.View
import android.view.accessibility.AccessibilityEvent
import android.widget.LinearLayout
import android.widget.TextView
import dev.nativeshell.NativeShell

/**
 * Toasts as gpui-shell shows them: up to [MAX_VISIBLE] stacked at the bottom of
 * the surface, the rest queued; a new toast never retracts an earlier one.
 * Each visible toast auto-dismisses after its duration (0 = until retracted);
 * the next queued one then takes its place.
 */
@SuppressLint("ViewConstructor")
internal class ToastStack(context: Context) : LinearLayout(context) {
    class Toast(val id: String, val title: String, val description: String?, val level: String, val durationMs: Int, val actionLabel: String?)

    private val visible = ArrayList<Pair<Toast, View>>()
    private val queue = ArrayDeque<Toast>()
    private var anonymous = 0
    private val dp = resources.displayMetrics.density

    val visibleIds: List<String> get() = visible.map { it.first.id }
    val queuedIds: List<String> get() = queue.map { it.id }

    /** Text of the last toast shown (tests). */
    var lastShown: String? = null
        private set

    init {
        orientation = VERTICAL
        gravity = Gravity.BOTTOM or Gravity.CENTER_HORIZONTAL
        clipChildren = false
        clipToPadding = false
    }

    /** Space to keep clear at the bottom (safe area or keyboard), in px. */
    fun setBottomInset(px: Int) {
        setPadding(paddingLeft, paddingTop, paddingRight, px + (16 * dp).toInt())
    }

    /** Called with the toast id when its action button is tapped (the toast is dismissed). */
    var onAction: ((String) -> Unit)? = null

    fun show(id: String?, title: String, description: String?, level: String, durationMs: Int, actionLabel: String? = null) {
        val t = Toast(id ?: "toast-${++anonymous}", title, description, level, durationMs.coerceAtLeast(0), actionLabel)
        if (visible.size < MAX_VISIBLE) present(t) else queue.addLast(t)
    }

    /** Retracts [id] (visible or queued), or every toast when null. Unknown ids are ignored. */
    fun dismiss(id: String?) {
        if (id == null) {
            queue.clear()
            visible.toList().forEach { remove(it.first) }
            return
        }
        if (queue.removeAll { it.id == id }) return
        visible.firstOrNull { it.first.id == id }?.let { remove(it.first) }
    }

    private fun present(t: Toast) {
        val card = card(t)
        visible += t to card
        addView(card, LayoutParams(LayoutParams.MATCH_PARENT, LayoutParams.WRAP_CONTENT).apply {
            setMargins((16 * dp).toInt(), (8 * dp).toInt(), (16 * dp).toInt(), 0)
        })
        lastShown = if (t.description.isNullOrEmpty()) t.title else "${t.title}\n${t.description}"
        val reduced = Motion.reduced(context)
        card.alpha = 0f
        card.translationY = if (reduced) 0f else 24 * dp
        card.animate().alpha(1f).translationY(0f).setDuration(if (reduced) Motion.REDUCED_FADE_MS else ANIM_MS).start()
        card.contentDescription = lastShown
        card.sendAccessibilityEvent(AccessibilityEvent.TYPE_VIEW_FOCUSED)
        announceForAccessibility(lastShown)
        if (t.durationMs > 0) card.postDelayed({ remove(t) }, t.durationMs.toLong())
        card.setOnClickListener { remove(t) }
    }

    private fun remove(t: Toast) {
        val i = visible.indexOfFirst { it.first === t }
        if (i < 0) return
        val (_, card) = visible.removeAt(i)
        card.animate().cancel()
        val reduced = Motion.reduced(context)
        card.animate().alpha(0f).translationY(if (reduced) 0f else 24 * dp)
            .setDuration(if (reduced) Motion.REDUCED_FADE_MS else ANIM_MS).withEndAction { removeView(card) }.start()
        queue.removeFirstOrNull()?.let { present(it) }
    }

    private fun card(t: Toast): View {
        val c = ThemeTokens.of(context)
        val box = LinearLayout(context).apply {
            orientation = HORIZONTAL
            val p = (14 * dp).toInt()
            setPadding(p, (12 * dp).toInt(), p, (12 * dp).toInt())
            // A neutral surface; the level shows as the icon's shape (and color), never as a filled card.
            background = GradientDrawable().apply {
                setColor(c.background)
                setStroke(dp.toInt().coerceAtLeast(1), c.border)
                cornerRadius = RADIUS * dp
            }
            elevation = 4 * dp
            isClickable = true
        }
        val iconColor = when (t.level) {
            "error" -> c.destructive
            "success" -> c.success
            "warning" -> c.warning
            else -> c.info
        }
        val iconSize = (20 * dp).toInt()
        box.addView(StatusIcon(context, t.level, iconColor), LayoutParams(iconSize, iconSize).apply {
            marginEnd = (10 * dp).toInt()
            topMargin = (1 * dp).toInt()
        })
        val texts = LinearLayout(context).apply { orientation = VERTICAL }
        texts.addView(TextView(context).apply {
            text = t.title
            setTextColor(c.foreground)
            setTextSize(TypedValue.COMPLEX_UNIT_SP, 15f)
            typeface = Typeface.create("sans-serif-medium", Typeface.NORMAL)
        })
        if (!t.description.isNullOrEmpty()) {
            texts.addView(TextView(context).apply {
                text = t.description
                setTextColor(c.mutedForeground)
                setTextSize(TypedValue.COMPLEX_UNIT_SP, 13f)
            })
        }
        box.addView(texts, LayoutParams(0, LayoutParams.WRAP_CONTENT, 1f))
        t.actionLabel?.let { label ->
            box.gravity = Gravity.CENTER_VERTICAL
            val min = (48 * dp).toInt()
            box.addView(TextView(context).apply {
                text = label
                setTextColor(c.foreground)
                setTextSize(TypedValue.COMPLEX_UNIT_SP, 14f)
                typeface = Typeface.create("sans-serif-medium", Typeface.NORMAL)
                gravity = Gravity.CENTER
                minWidth = min
                minHeight = min
                val p = (12 * dp).toInt()
                setPadding(p, 0, p, 0)
                isClickable = true
                isFocusable = true
                contentDescription = label
                val tv = android.util.TypedValue()
                if (context.theme.resolveAttribute(android.R.attr.selectableItemBackground, tv, true)) setBackgroundResource(tv.resourceId)
                setOnClickListener {
                    remove(t)
                    onAction?.invoke(t.id)
                }
            }, LayoutParams(LayoutParams.WRAP_CONTENT, LayoutParams.WRAP_CONTENT).apply {
                marginStart = (8 * dp).toInt()
                // The 48 dp target may extend into the card's padding.
                topMargin = -(12 * dp).toInt(); bottomMargin = -(12 * dp).toInt(); marginEnd = -(6 * dp).toInt()
            })
        }
        return box
    }

    /** The level as a shape: info ⓘ, success ✓, warning !, error ✕. */
    @SuppressLint("ViewConstructor")
    internal class StatusIcon(context: Context, val level: String, val color: Int) : View(context) {
        private val paint = Paint(Paint.ANTI_ALIAS_FLAG).apply {
            style = Paint.Style.STROKE
            strokeCap = Paint.Cap.ROUND
            strokeJoin = Paint.Join.ROUND
            this.color = this@StatusIcon.color
        }
        private val path = Path()

        override fun onDraw(canvas: Canvas) {
            val s = minOf(width, height).toFloat()
            val u = s / 20f
            paint.strokeWidth = 1.75f * u
            paint.style = Paint.Style.STROKE
            val c = s / 2f
            path.reset()
            when (level) {
                "warning" -> {
                    path.moveTo(c, 2.5f * u); path.lineTo(18f * u, 17f * u); path.lineTo(2f * u, 17f * u); path.close()
                    canvas.drawPath(path, paint)
                    canvas.drawLine(c, 8f * u, c, 11.5f * u, paint)
                    canvas.drawPoint(c, 14.2f * u, paint)
                }
                else -> {
                    canvas.drawCircle(c, c, 8.5f * u, paint)
                    when (level) {
                        "success" -> {
                            path.moveTo(6.5f * u, 10.2f * u); path.lineTo(9f * u, 12.7f * u); path.lineTo(13.7f * u, 7.6f * u)
                            canvas.drawPath(path, paint)
                        }
                        "error" -> {
                            canvas.drawLine(7.3f * u, 7.3f * u, 12.7f * u, 12.7f * u, paint)
                            canvas.drawLine(12.7f * u, 7.3f * u, 7.3f * u, 12.7f * u, paint)
                        }
                        else -> {
                            canvas.drawLine(c, 9f * u, c, 13.8f * u, paint)
                            canvas.drawPoint(c, 6.3f * u, paint)
                        }
                    }
                }
            }
        }
    }

    companion object {
        const val MAX_VISIBLE = 3
        const val ANIM_MS = 200L
        /** The theme's `lg` radius. */
        const val RADIUS = 8f
    }
}
