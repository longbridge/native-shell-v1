package dev.nativeshell

import android.content.Context
import android.content.res.Configuration
import android.os.SystemClock
import android.util.AttributeSet
import android.view.View
import android.view.ViewTreeObserver
import android.widget.FrameLayout
import androidx.activity.OnBackPressedCallback
import androidx.activity.OnBackPressedDispatcher
import androidx.activity.findViewTreeOnBackPressedDispatcherOwner
import androidx.appcompat.view.ContextThemeWrapper
import androidx.core.view.ViewCompat
import androidx.core.view.WindowInsetsCompat
import dev.nativeshell.nav.Presenter
import dev.nativeshell.render.ShellViewGroup
import dev.nativeshell.text.TextLayouts
import org.json.JSONArray
import org.json.JSONObject
import java.util.Locale

/**
 * The view an instance renders into. Add it to any layout, then [attach] an
 * instance. It reports its size, safe-area and keyboard insets, font scale,
 * appearance and locale to the script as the environment, presents pages,
 * sheets and dialogs, and handles system back for the page stack.
 *
 * The surface is edge to edge: it does not pad itself for system bars; the
 * script receives the insets as `safe_area` and lays out around them.
 */
open class ShellSurfaceView @JvmOverloads constructor(
    context: Context,
    attrs: AttributeSet? = null,
) : FrameLayout(context, attrs) {

    private val themed: Context = ContextThemeWrapper(context, R.style.NativeShell_Theme)
    private val rootGroup = ShellViewGroup(themed).apply { isRootContainer = true }
    private val pageLayer = FrameLayout(themed)
    internal val toastStack = dev.nativeshell.render.ToastStack(themed)
    internal val presenter = Presenter(pageLayer, { rootGroup }, { syncBackCallback() })

    var instance: ShellInstance? = null
        private set
    internal var renderer: SurfaceRenderer? = null
        private set

    private var insets: WindowInsetsCompat? = null
    private var lastEnvironment: String? = null
    private var attachTime = 0L
    private var backDispatcher: OnBackPressedDispatcher? = null

    /** Longest time the first draw waits for the first batch. */
    var firstFrameTimeoutMs = 1000L

    private val backCallback = object : OnBackPressedCallback(false) {
        override fun handleOnBackPressed() {
            presenter.handleBack()
        }
    }

    private val preDraw = ViewTreeObserver.OnPreDrawListener {
        val r = renderer ?: return@OnPreDrawListener true
        val inst = instance ?: return@OnPreDrawListener true
        if (!r.awaitingFirstBatch) return@OnPreDrawListener true
        // INITIAL batch: apply synchronously before the first draw when it is ready.
        inst.drainNow()
        !r.awaitingFirstBatch || SystemClock.uptimeMillis() - attachTime > firstFrameTimeoutMs
    }

    init {
        // Focusable so hardware keys reach the surface when no field has focus; fields still win.
        isFocusable = true
        isFocusableInTouchMode = true
        descendantFocusability = FOCUS_AFTER_DESCENDANTS
        addView(rootGroup, LayoutParams(LayoutParams.MATCH_PARENT, LayoutParams.MATCH_PARENT))
        addView(pageLayer, LayoutParams(LayoutParams.MATCH_PARENT, LayoutParams.MATCH_PARENT))
        toastStack.onAction = { id ->
            renderer?.dispatch(dev.nativeshell.protocol.Protocol.ROOT_NODE, dev.nativeshell.protocol.EventType.TOAST_ACTION, "{\"id\":${dev.nativeshell.util.Json.stringify(id)}}")
        }
        addView(toastStack, LayoutParams(LayoutParams.MATCH_PARENT, LayoutParams.WRAP_CONTENT, android.view.Gravity.BOTTOM))
        ViewCompat.setOnApplyWindowInsetsListener(this) { _, i ->
            insets = i
            sendEnvironment()
            i
        }
    }

    /** Renders [instance] in this surface. A surface shows one instance at a time. */
    fun attach(instance: ShellInstance) {
        detach()
        this.instance = instance
        val sink = object : SurfaceRenderer.EventSinkTarget {
            override fun dispatch(node: Int, type: Int, payload: String, seq: Int) = instance.dispatchEvent(node, type, payload, seq)
            override fun log(level: Int, message: String) = NativeShell.logger.log(level, "native-shell", message)
            override fun detached() {
                clearRenderer()
            }
        }
        attachRenderer(sink, instance.resourceBase)
        instance.surface = renderer
        lastEnvironment = null
        sendEnvironment()
    }

    internal fun attachRenderer(sink: SurfaceRenderer.EventSinkTarget, resourceBase: String?): SurfaceRenderer {
        val r = SurfaceRenderer(this, themed, rootGroup, presenter, sink, resourceBase)
        presenter.host = r
        renderer = r
        attachTime = SystemClock.uptimeMillis()
        return r
    }

    /** Detaches the instance; the instance keeps running and can be attached elsewhere. */
    fun detach() {
        val inst = instance ?: return
        inst.surface = null
        instance = null
        clearRenderer()
    }

    private fun clearRenderer() {
        presenter.dismissAll()
        renderer?.mounting?.clear()
        renderer = null
        instance = null
    }

    /** Moves focus to the surface itself (blur), so a field does not get it back. */
    internal fun takeFocus() {
        descendantFocusability = FOCUS_BLOCK_DESCENDANTS
        requestFocus()
        descendantFocusability = FOCUS_AFTER_DESCENDANTS
    }

    override fun dispatchKeyEvent(event: android.view.KeyEvent): Boolean {
        if (event.action != android.view.KeyEvent.ACTION_MULTIPLE) renderer?.mounting?.dispatchKey(findFocus(), event)
        return super.dispatchKeyEvent(event)
    }

    /** The view whose node carries `TestId` [testId], or null. */
    fun findByTestId(testId: String): View? = renderer?.mounting?.findByTestId(testId)

    /** Number of pages pushed (excluding pages being dismissed). */
    val pageCount: Int get() = presenter.pageCount

    // ---- Environment --------------------------------------------------------------------

    override fun onSizeChanged(w: Int, h: Int, oldw: Int, oldh: Int) {
        super.onSizeChanged(w, h, oldw, oldh)
        sendEnvironment()
    }

    override fun onLayout(changed: Boolean, left: Int, top: Int, right: Int, bottom: Int) {
        super.onLayout(changed, left, top, right, bottom)
        // The window position decides which insets overlap the surface.
        if (changed) sendEnvironment()
    }

    override fun onConfigurationChanged(newConfig: Configuration?) {
        super.onConfigurationChanged(newConfig)
        TextLayouts.onConfigurationChanged(context)
        sendEnvironment()
    }

    override fun onAttachedToWindow() {
        super.onAttachedToWindow()
        viewTreeObserver.addOnPreDrawListener(preDraw)
        backDispatcher = findViewTreeOnBackPressedDispatcherOwner()?.onBackPressedDispatcher
            ?: (context as? androidx.activity.ComponentActivity)?.onBackPressedDispatcher
        backDispatcher?.addCallback(backCallback)
        syncBackCallback()
        ViewCompat.requestApplyInsets(this)
        sendEnvironment()
        // Take key focus unless something in the window already has it.
        post { if (rootView.findFocus() == null) requestFocus() }
    }

    override fun onDetachedFromWindow() {
        viewTreeObserver.removeOnPreDrawListener(preDraw)
        backCallback.remove()
        backDispatcher = null
        super.onDetachedFromWindow()
    }

    private fun syncBackCallback() {
        backCallback.isEnabled = presenter.pageCount > 0
    }

    /** Back handling for hosts that are not a ComponentActivity: returns true when consumed. */
    fun handleBackPressed(): Boolean = presenter.handleBack()

    /** The environment JSON for the current state (`Environment` in crates/core/src/host.rs). */
    fun environmentJson(): JSONObject {
        val d = resources.displayMetrics.density
        val cfg = resources.configuration
        val i = insets
        val bars = i?.getInsets(WindowInsetsCompat.Type.systemBars() or WindowInsetsCompat.Type.displayCutout())
        val ime = i?.getInsets(WindowInsetsCompat.Type.ime())
        val dark = cfg.uiMode and Configuration.UI_MODE_NIGHT_MASK == Configuration.UI_MODE_NIGHT_YES
        val w = if (width > 0) width else resources.displayMetrics.widthPixels
        val h = if (height > 0) height else resources.displayMetrics.heightPixels
        return JSONObject()
            .put("width", (w / d).toDouble())
            .put("height", (h / d).toDouble())
            .put("scale", d.toDouble())
            .put("font_scale", cfg.fontScale.toDouble())
            .put("safe_area", JSONArray(safeArea(bars, w, h).map { (it / d).toDouble() }))
            .put("keyboard_height", (overlapBottom(ime?.bottom ?: 0, h) / d).toDouble())
            .put("appearance", if (dark) "dark" else "light")
            .put("platform", "android")
            .put("locale", (if (android.os.Build.VERSION.SDK_INT >= 24) cfg.locales[0] else Locale.getDefault()).toLanguageTag())
    }

    private val loc = IntArray(2)

    /**
     * Window insets clipped to this view: a surface below a toolbar gets no top
     * inset, one that ends above the navigation bar no bottom inset.
     */
    private fun safeArea(bars: androidx.core.graphics.Insets?, w: Int, h: Int): List<Int> {
        if (bars == null) return listOf(0, 0, 0, 0)
        val root = rootView
        getLocationInWindow(loc)
        val top = (bars.top - loc[1]).coerceIn(0, h)
        val left = (bars.left - loc[0]).coerceIn(0, w)
        val right = (bars.right - (root.width - loc[0] - w)).coerceIn(0, w)
        val bottom = overlapBottom(bars.bottom, h)
        return listOf(top, right, bottom, left)
    }

    private fun overlapBottom(inset: Int, h: Int): Int {
        getLocationInWindow(loc)
        return (inset - (rootView.height - loc[1] - h)).coerceIn(0, h)
    }

    private fun sendEnvironment() {
        val inst = instance ?: return
        if (width == 0 || height == 0) return
        val env = environmentJson()
        // Toasts sit above the keyboard and the bottom safe area.
        val d = resources.displayMetrics.density
        toastStack.setBottomInset((maxOf(env.getJSONArray("safe_area").getDouble(2), env.getDouble("keyboard_height")) * d).toInt())
        val json = env.toString()
        if (json == lastEnvironment) return
        lastEnvironment = json
        inst.setEnvironment(json)
    }
}
