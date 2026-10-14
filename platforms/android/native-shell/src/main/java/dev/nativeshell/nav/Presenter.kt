package dev.nativeshell.nav

import android.app.Dialog
import android.content.Context
import android.view.View
import android.view.ViewGroup
import android.view.Window
import android.widget.FrameLayout
import androidx.appcompat.app.AlertDialog
import com.google.android.material.bottomsheet.BottomSheetBehavior
import com.google.android.material.bottomsheet.BottomSheetDialog
import com.google.android.material.dialog.MaterialAlertDialogBuilder
import dev.nativeshell.R
import dev.nativeshell.protocol.Command
import dev.nativeshell.protocol.EventType
import dev.nativeshell.protocol.ViewType
import dev.nativeshell.render.FrameApplier
import dev.nativeshell.render.Motion
import dev.nativeshell.render.MountHost
import dev.nativeshell.render.MountingManager
import dev.nativeshell.render.Node
import dev.nativeshell.render.SurfacePresenter
import dev.nativeshell.render.ThemeTokens
import dev.nativeshell.util.Json
import dev.nativeshell.views.surfaceProps
import org.json.JSONObject

/**
 * Presents surface nodes: pages in a stack above the root content with slide
 * transitions, sheets as bottom-sheet dialogs, dialogs and alerts as dialog
 * windows. The core owns the stack; user dismissals (back, swipe, backdrop)
 * are reported as `Dismiss`, and the core answers by deleting the node.
 */
internal class Presenter(
    private val pageLayer: FrameLayout,
    private val rootContent: () -> View,
    private val onStackChanged: () -> Unit,
) : SurfacePresenter {

    lateinit var host: MountHost

    private val pages = ArrayList<Node>()
    private val dismissing = HashSet<Node>()
    private val dialogs = HashMap<Node, Dialog>()
    /** Dialogs closed by the core: their dismiss listener must not report a user dismissal. */
    private val closing = HashSet<Dialog>()

    val pageCount: Int get() = pages.count { it !in dismissing }
    val topPage: Node? get() = pages.lastOrNull { it !in dismissing }
    val openDialogs: Collection<Dialog> get() = dialogs.values

    override fun attach(node: Node, mounting: MountingManager) {
        when (node.type) {
            ViewType.PAGE -> attachPage(node)
            ViewType.SHEET -> showSheet(node)
            ViewType.DIALOG -> showDialog(node)
        }
    }

    override fun detach(node: Node, mounting: MountingManager) {
        when (node.type) {
            ViewType.PAGE -> detachPage(node)
            else -> dialogs.remove(node)?.let { d ->
                closing.add(d)
                (node.view.parent as? ViewGroup)?.removeView(node.view)
                if (d.isShowing) d.dismiss()
            }
        }
    }

    override fun command(node: Node, command: Int, payload: JSONObject?): Boolean {
        if (node.type != ViewType.PAGE) return false
        val animated = Json.bool(payload, "animated", true)
        when (command) {
            Command.PRESENT_PAGE -> if (animated) slideIn(node)
            Command.DISMISS_PAGE -> {
                // Already popped locally by system back.
                if (node in dismissing) return true
                dismissing.add(node)
                onStackChanged()
                if (animated) slideOut(node) else finishDismiss(node)
            }
            else -> return false
        }
        return true
    }

    override fun surfaceChanged(node: Node) {
        val d = dialogs[node] ?: return
        val p = node.surfaceProps
        d.setCancelable(p.dismissable)
        d.setCanceledOnTouchOutside(p.dismissable)
        if (d is BottomSheetDialog) configureSheet(d, node)
    }

    // ---- Pages --------------------------------------------------------------------------

    private fun attachPage(node: Node) {
        val v = node.view
        (v.parent as? ViewGroup)?.removeView(v)
        pageLayer.addView(v, FrameLayout.LayoutParams(
            if (node.hasFrame) node.width else ViewGroup.LayoutParams.MATCH_PARENT,
            if (node.hasFrame) node.height else ViewGroup.LayoutParams.MATCH_PARENT,
        ))
        // Pages are opaque screens: a page without a background shows the theme's.
        if (v.background == null) v.setBackgroundColor(ThemeTokens.of(v.context).background)
        v.isClickable = true // swallow touches meant for pages below
        pages.add(node)
        onStackChanged()
    }

    private fun detachPage(node: Node) {
        pages.remove(node)
        // A page still sliding out is removed by finishDismiss when the animation ends.
        if (node !in sliding) {
            dismissing.remove(node)
            node.view.animate().cancel()
            pageLayer.removeView(node.view)
            restoreUnderlying()
        }
        onStackChanged()
    }

    private val sliding = HashSet<Node>()

    private fun isSliding(node: Node) = node in sliding

    private fun below(node: Node): View {
        val i = pages.indexOf(node)
        return if (i > 0) pages[i - 1].view else rootContent()
    }

    private fun slideIn(node: Node) {
        val v = node.view
        val w = pageLayer.width.toFloat().takeIf { it > 0f } ?: v.resources.displayMetrics.widthPixels.toFloat()
        val under = below(node)
        if (Motion.reduced(v.context)) {
            v.translationX = 0f
            v.alpha = 0f
            v.animate().alpha(1f).setDuration(Motion.REDUCED_FADE_MS).start()
            return
        }
        v.translationX = w
        v.animate().translationX(0f).setDuration(PAGE_MS).withEndAction(null).start()
        under.animate().translationX(-w * 0.3f).setDuration(PAGE_MS).start()
    }

    private fun slideOut(node: Node) {
        val v = node.view
        val w = pageLayer.width.toFloat().takeIf { it > 0f } ?: v.resources.displayMetrics.widthPixels.toFloat()
        val under = below(node)
        sliding.add(node)
        if (Motion.reduced(v.context)) {
            v.animate().alpha(0f).setDuration(Motion.REDUCED_FADE_MS).withEndAction {
                sliding.remove(node)
                finishDismiss(node)
            }.start()
            return
        }
        under.animate().translationX(0f).setDuration(PAGE_MS).start()
        v.animate().translationX(w).setDuration(PAGE_MS).withEndAction {
            sliding.remove(node)
            finishDismiss(node)
        }.start()
    }

    private fun finishDismiss(node: Node) {
        dismissing.remove(node)
        sliding.remove(node)
        // Deleted already (the usual case): remove the view now; otherwise detach does it.
        if (node !in pages) pageLayer.removeView(node.view) else node.view.visibility = View.INVISIBLE
        restoreUnderlying()
        onStackChanged()
    }

    private fun restoreUnderlying() {
        val top = topPage
        val under = if (top != null) top.view else rootContent()
        if (top == null || !isSliding(top)) under.animate().translationX(0f).setDuration(0).start()
    }

    /** System back: the top page pops if the script allows it, else the script is told. */
    fun handleBack(): Boolean {
        val top = topPage ?: return false
        if (top.surfaceProps.canGoBack) {
            // Pop locally right away; the core follows with DismissPage + Delete.
            dismissing.add(top)
            onStackChanged()
            slideOut(top)
            host.dispatch(top.id, EventType.DISMISS, "{}")
        } else {
            host.dispatch(top.id, EventType.BACK_PRESSED, "{}")
        }
        return true
    }

    // ---- Sheets -------------------------------------------------------------------------

    private fun showSheet(node: Node) {
        val ctx = host.context
        val d = BottomSheetDialog(ctx, R.style.NativeShell_BottomSheet)
        val tokens = ThemeTokens.of(ctx)
        val dp = ctx.resources.displayMetrics.density
        val holder = FrameLayout(ctx)
        (node.view.parent as? ViewGroup)?.removeView(node.view)
        holder.addView(node.view, FrameLayout.LayoutParams(
            ViewGroup.LayoutParams.MATCH_PARENT,
            if (node.hasFrame) node.height else ViewGroup.LayoutParams.WRAP_CONTENT,
        ))
        holder.addView(Grabber(ctx, tokens.mutedForeground), FrameLayout.LayoutParams(
            (GRABBER_W * dp).toInt(), (GRABBER_H * dp).toInt(), android.view.Gravity.TOP or android.view.Gravity.CENTER_HORIZONTAL,
        ).apply { topMargin = (GRABBER_TOP * dp).toInt() })
        // The sheet runs under the navigation bar; its content stays above it. Scripts already
        // pad by safe_area.bottom (the same inset when the surface reaches the window bottom),
        // so only the part the environment does not report is padded here.
        // The keyboard belongs to the sheet's window: the sheet rises above it (content padded by the IME).
        androidx.core.view.ViewCompat.setOnApplyWindowInsetsListener(holder) { v, insets ->
            val nav = insets.getInsets(androidx.core.view.WindowInsetsCompat.Type.navigationBars()).bottom
            val ime = insets.getInsets(androidx.core.view.WindowInsetsCompat.Type.ime()).bottom
            val reported = sheetSafeBottomPx()
            v.setPadding(0, 0, 0, maxOf((nav - reported).coerceAtLeast(0), ime))
            if (ime > 0) d.behavior.state = BottomSheetBehavior.STATE_EXPANDED
            insets
        }
        d.setContentView(holder)
        @Suppress("DEPRECATION")
        d.window?.setSoftInputMode(android.view.WindowManager.LayoutParams.SOFT_INPUT_ADJUST_RESIZE)
        // Opaque theme background (the behavior's 16 dp top-cornered shape), under the navigation bar too.
        d.findViewById<View>(com.google.android.material.R.id.design_bottom_sheet)?.backgroundTintList =
            android.content.res.ColorStateList.valueOf(tokens.background)
        d.window?.let { w ->
            val c = androidx.core.view.WindowCompat.getInsetsController(w, w.decorView)
            c.isAppearanceLightNavigationBars = !tokens.dark
            c.isAppearanceLightStatusBars = !tokens.dark
        }
        // Reduced motion: the sheet fades in and out instead of sliding.
        if (Motion.reduced(ctx)) d.window?.setWindowAnimations(android.R.style.Animation_Toast)
        configureSheet(d, node)
        trackUserDismiss(d, node)
        dialogs[node] = d
        d.show()
    }

    /** `safe_area.bottom` the script was told, in px (the surface's own bottom inset). */
    private fun sheetSafeBottomPx(): Int {
        val surface = host.anchor as? dev.nativeshell.ShellSurfaceView ?: return 0
        val d = surface.resources.displayMetrics.density
        return (surface.environmentJson().getJSONArray("safe_area").getDouble(2) * d).toInt()
    }

    /** The sheet's drag handle. */
    @android.annotation.SuppressLint("ViewConstructor")
    private class Grabber(context: Context, color: Int) : View(context) {
        init {
            background = android.graphics.drawable.GradientDrawable().apply {
                setColor((color and 0x00FFFFFF) or 0x66000000) // 40% alpha
                cornerRadius = 100f
            }
            importantForAccessibility = IMPORTANT_FOR_ACCESSIBILITY_NO
        }
    }

    private fun configureSheet(d: BottomSheetDialog, node: Node) {
        val p = node.surfaceProps
        d.setCancelable(p.dismissable)
        d.setCanceledOnTouchOutside(p.dismissable)
        val b = d.behavior
        b.isHideable = p.dismissable
        b.isDraggable = p.dismissable
        val screen = host.context.resources.displayMetrics.heightPixels
        val custom = p.detentHeight
        val peek = when {
            custom != null -> (custom * host.density).toInt()
            "medium" in p.detents -> screen / 2
            else -> null
        }
        if (peek != null) {
            b.skipCollapsed = false
            b.peekHeight = peek
            b.state = BottomSheetBehavior.STATE_COLLAPSED
        } else {
            b.skipCollapsed = true
            b.state = BottomSheetBehavior.STATE_EXPANDED
        }
    }

    // ---- Dialogs and alerts --------------------------------------------------------------

    private fun showDialog(node: Node) {
        val p = node.surfaceProps
        val spec = Json.obj(p.alertSpec)
        val d = if (spec != null) alert(node, spec) else customDialog(node)
        trackUserDismiss(d, node)
        dialogs[node] = d
        d.show()
    }

    private fun customDialog(node: Node): Dialog {
        val p = node.surfaceProps
        val d = Dialog(host.context, R.style.NativeShell_Dialog)
        d.requestWindowFeature(Window.FEATURE_NO_TITLE)
        val holder = FrameLayout(host.context)
        (node.view.parent as? ViewGroup)?.removeView(node.view)
        // The dialog node is the card (the core sizes it); it sits centered over the dimmed window.
        holder.addView(node.view, FrameLayout.LayoutParams(
            if (node.hasFrame) node.width else ViewGroup.LayoutParams.WRAP_CONTENT,
            if (node.hasFrame) node.height else ViewGroup.LayoutParams.WRAP_CONTENT,
            android.view.Gravity.CENTER,
        ))
        defaultSurfaceBackground(node, DIALOG_RADIUS)
        node.view.isClickable = true // taps on the card never reach the backdrop
        holder.setOnClickListener { if (node.surfaceProps.dismissable) d.cancel() }
        // The card stays centered in the space above the keyboard.
        androidx.core.view.ViewCompat.setOnApplyWindowInsetsListener(holder) { v, insets ->
            v.setPadding(0, 0, 0, insets.getInsets(androidx.core.view.WindowInsetsCompat.Type.ime()).bottom)
            insets
        }
        d.setContentView(holder)
        d.window?.setLayout(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.MATCH_PARENT)
        @Suppress("DEPRECATION")
        d.window?.setSoftInputMode(android.view.WindowManager.LayoutParams.SOFT_INPUT_ADJUST_RESIZE)
        d.setCancelable(p.dismissable)
        d.setCanceledOnTouchOutside(p.dismissable)
        if (node.hasFrame) FrameApplier.layoutNow(node)
        return d
    }

    private fun alert(node: Node, spec: JSONObject): AlertDialog {
        val b = MaterialAlertDialogBuilder(host.context)
        Json.string(spec, "title")?.let { b.setTitle(it) }
        Json.string(spec, "message")?.let { b.setMessage(it) }
        val actions = spec.optJSONArray("actions")
        val labels = ArrayList<String>()
        val styles = ArrayList<String>()
        for (i in 0 until (actions?.length() ?: 0)) {
            val a = actions!!.optJSONObject(i) ?: continue
            labels += Json.string(a, "label") ?: ""
            styles += Json.string(a, "style") ?: "default"
        }
        val pick = { index: Int -> host.dispatch(node.id, EventType.ACTION, "{\"index\":$index}") }
        val cancelIndex = styles.indexOf("cancel")
        if (labels.size <= 3) {
            // Cancel goes on the negative button; others fill positive then neutral.
            val others = labels.indices.filter { it != cancelIndex }
            others.getOrNull(0)?.let { i -> b.setPositiveButton(labels[i]) { _, _ -> pick(i) } }
            others.getOrNull(1)?.let { i -> b.setNeutralButton(labels[i]) { _, _ -> pick(i) } }
            if (cancelIndex >= 0) b.setNegativeButton(labels[cancelIndex]) { _, _ -> pick(cancelIndex) }
            else others.getOrNull(2)?.let { i -> b.setNegativeButton(labels[i]) { _, _ -> pick(i) } }
        } else {
            b.setItems(labels.toTypedArray()) { _, i -> pick(i) }
        }
        val dismissable = node.surfaceProps.dismissable && cancelIndex >= 0
        b.setCancelable(dismissable)
        val d = b.create()
        d.setCanceledOnTouchOutside(false)
        if (dismissable) d.setOnCancelListener { pick(cancelIndex) }
        val destructive = styles.indexOf("destructive")
        if (destructive >= 0) d.setOnShowListener {
            listOf(AlertDialog.BUTTON_POSITIVE, AlertDialog.BUTTON_NEUTRAL, AlertDialog.BUTTON_NEGATIVE).forEach { which ->
                val btn = d.getButton(which)
                if (btn != null && btn.text == labels[destructive]) btn.setTextColor(0xFFDC2626.toInt())
            }
        }
        return d
    }

    private fun trackUserDismiss(d: Dialog, node: Node) {
        d.setOnDismissListener {
            if (closing.remove(d)) return@setOnDismissListener
            dialogs.remove(node)
            // An alert answered by a button already reported its action.
            if (node.surfaceProps.alertSpec == null) host.dispatch(node.id, EventType.DISMISS, "{}")
        }
    }

    fun dismissAll() {
        for ((_, d) in dialogs) {
            closing.add(d)
            if (d.isShowing) d.dismiss()
        }
        dialogs.clear()
    }

    /** Gives a surface without its own background the theme's opaque background (rounded for dialogs). */
    private fun defaultSurfaceBackground(node: Node, radiusDp: Float) {
        val v = node.view
        if (v.background != null) return
        val dp = v.resources.displayMetrics.density
        v.background = android.graphics.drawable.GradientDrawable().apply {
            setColor(ThemeTokens.of(v.context).background)
            cornerRadius = radiusDp * dp
        }
        v.clipToOutline = true
    }

    companion object {
        const val PAGE_MS = 300L
        /** The theme's `xl` radius. */
        const val DIALOG_RADIUS = 12f
        const val GRABBER_W = 36f
        const val GRABBER_H = 5f
        const val GRABBER_TOP = 6f
    }
}
