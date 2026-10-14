package dev.nativeshell.views

import android.annotation.SuppressLint
import android.app.DatePickerDialog
import android.app.TimePickerDialog
import android.content.Context
import android.os.SystemClock
import android.text.format.DateFormat
import android.view.Gravity
import android.view.Menu
import android.view.View
import android.view.ViewGroup
import android.widget.CompoundButton
import android.widget.FrameLayout
import android.widget.LinearLayout
import androidx.appcompat.widget.PopupMenu
import com.google.android.material.button.MaterialButton
import com.google.android.material.button.MaterialButtonToggleGroup
import com.google.android.material.checkbox.MaterialCheckBox
import com.google.android.material.materialswitch.MaterialSwitch
import com.google.android.material.progressindicator.CircularProgressIndicator
import com.google.android.material.progressindicator.LinearProgressIndicator
import com.google.android.material.radiobutton.MaterialRadioButton
import com.google.android.material.slider.Slider
import dev.nativeshell.protocol.EventMask
import dev.nativeshell.protocol.EventType
import dev.nativeshell.protocol.Prop
import dev.nativeshell.protocol.PropValue
import dev.nativeshell.protocol.ValueKind
import dev.nativeshell.protocol.ViewType
import dev.nativeshell.render.MountHost
import dev.nativeshell.render.Node
import dev.nativeshell.render.ViewManager
import dev.nativeshell.util.Json
import java.util.Calendar
import kotlin.math.abs
import kotlin.math.roundToInt

private fun changeEnabled(node: Node) = node.eventMask and EventMask.CHANGE != 0

/** Base of controls: leaves that take `Enabled`. */
internal abstract class ControlManager(type: Int) : ViewManager(type) {
    override fun container(node: Node): ViewGroup? = null

    protected fun setEnabled(view: View, value: PropValue) {
        view.isEnabled = value.isNull || value.bool
    }
}

// ---- Switch -------------------------------------------------------------------------------

internal class SwitchManager : ControlManager(ViewType.SWITCH) {
    override fun createView(host: MountHost): View = MaterialSwitch(host.context).apply {
        minWidth = 0
        minHeight = 0
        setPadding(0, 0, 0, 0)
    }

    override fun onMount(node: Node, host: MountHost) {
        (node.view as CompoundButton).setOnCheckedChangeListener { b, checked ->
            if (b.tag == MUTED) return@setOnCheckedChangeListener
            if (changeEnabled(node)) host.dispatch(node.id, EventType.CHANGE, "{\"checked\":$checked}")
        }
    }

    override fun setProp(node: Node, prop: Int, value: PropValue, host: MountHost): Boolean {
        val v = node.view as CompoundButton
        when (prop) {
            Prop.CHECKED -> v.setCheckedSilently(!value.isNull && value.bool)
            Prop.ENABLED -> setEnabled(v, value)
            Prop.TINT_COLOR -> ControlTint.setOverride(v, if (value.kind == ValueKind.COLOR) value.int else null)
            else -> return false
        }
        return true
    }

    override fun onRecycle(node: Node, host: MountHost) {
        (node.view as CompoundButton).setOnCheckedChangeListener(null)
    }
}

private const val MUTED = "nsh-muted"

private fun CompoundButton.setCheckedSilently(checked: Boolean) {
    if (isChecked == checked) return
    tag = MUTED
    isChecked = checked
    jumpDrawablesToCurrentState()
    tag = null
}


// ---- Checkbox / radio ---------------------------------------------------------------------

/** Holds a checkbox or a radio button; `CheckStyle` swaps which one. */
@SuppressLint("ViewConstructor")
internal class CheckContainer(context: Context) : FrameLayout(context) {
    var button: CompoundButton = make(0)
        private set
    var style = 0
        set(value) {
            if (field == value) return
            field = value
            val old = button
            removeView(old)
            button = make(value).also {
                it.isChecked = old.isChecked
                it.isEnabled = old.isEnabled
            }
            addView(button, LayoutParams(LayoutParams.MATCH_PARENT, LayoutParams.MATCH_PARENT))
            listener?.let { button.setOnCheckedChangeListener(it) }
            ControlTint.apply(this)
        }
    var listener: CompoundButton.OnCheckedChangeListener? = null
        set(value) {
            field = value
            button.setOnCheckedChangeListener(value)
        }

    init {
        addView(button, LayoutParams(LayoutParams.MATCH_PARENT, LayoutParams.MATCH_PARENT))
    }

    private fun make(style: Int): CompoundButton {
        val b = if (style == 1) MaterialRadioButton(context) else MaterialCheckBox(context)
        b.minWidth = 0
        b.minHeight = 0
        b.setPadding(0, 0, 0, 0)
        b.gravity = Gravity.CENTER
        return b
    }
}

internal class CheckboxManager : ControlManager(ViewType.CHECKBOX) {
    override fun createView(host: MountHost): View = CheckContainer(host.context)

    override fun onMount(node: Node, host: MountHost) {
        (node.view as CheckContainer).listener = CompoundButton.OnCheckedChangeListener { b, checked ->
            if (b.tag == MUTED) return@OnCheckedChangeListener
            if (changeEnabled(node)) host.dispatch(node.id, EventType.CHANGE, "{\"checked\":$checked}")
        }
    }

    override fun setProp(node: Node, prop: Int, value: PropValue, host: MountHost): Boolean {
        val v = node.view as CheckContainer
        when (prop) {
            Prop.CHECKED -> v.button.setCheckedSilently(!value.isNull && value.bool)
            Prop.ENABLED -> setEnabled(v.button, value)
            Prop.CHECK_STYLE -> v.style = if (value.isNull) 0 else value.intValue().coerceIn(0, 1)
            Prop.TINT_COLOR -> ControlTint.setOverride(v, if (value.kind == ValueKind.COLOR) value.int else null)
            else -> return false
        }
        return true
    }

    override fun onRecycle(node: Node, host: MountHost) {
        (node.view as CheckContainer).listener = null
    }
}

// ---- Slider -------------------------------------------------------------------------------

internal class SliderManager : ControlManager(ViewType.SLIDER) {
    private class State {
        var min = 0f
        var max = 1f
        var step = 0f
        var value = 0f
        var lastSent = 0L
        var muted = false
    }

    override fun createView(host: MountHost): View = Slider(host.context).apply {
        valueFrom = 0f
        valueTo = 1f
        labelBehavior = com.google.android.material.slider.LabelFormatter.LABEL_GONE
        setPadding(0, 0, 0, 0)
    }

    override fun onMount(node: Node, host: MountHost) {
        val s = node.view as Slider
        if (node.state !is State) node.state = State()
        val st = node.state as State
        s.clearOnChangeListeners()
        s.clearOnSliderTouchListeners()
        s.addOnChangeListener { _, value, fromUser ->
            if (!fromUser || st.muted) return@addOnChangeListener
            st.value = value
            val now = SystemClock.uptimeMillis()
            if (now - st.lastSent >= 16 && changeEnabled(node)) {
                st.lastSent = now
                host.dispatch(node.id, EventType.CHANGE, "{\"value\":$value,\"final\":false}")
            }
        }
        s.addOnSliderTouchListener(object : Slider.OnSliderTouchListener {
            override fun onStartTrackingTouch(slider: Slider) {}
            override fun onStopTrackingTouch(slider: Slider) {
                if (changeEnabled(node)) host.dispatch(node.id, EventType.CHANGE, "{\"value\":${slider.value},\"final\":true}")
            }
        })
    }

    override fun setProp(node: Node, prop: Int, value: PropValue, host: MountHost): Boolean {
        val s = node.view as Slider
        val st = node.state as State
        when (prop) {
            Prop.VALUE -> st.value = if (value.isNull) 0f else value.number()
            Prop.MIN -> st.min = if (value.isNull) 0f else value.number()
            Prop.MAX -> st.max = if (value.isNull) 1f else value.number()
            Prop.STEP -> st.step = if (value.isNull) 0f else value.number()
            Prop.ENABLED -> { setEnabled(s, value); return true }
            Prop.TINT_COLOR -> {
                ControlTint.setOverride(s, if (value.kind == ValueKind.COLOR) value.int else null)
                return true
            }
            else -> return false
        }
        apply(s, st)
        return true
    }


    /** Material's slider throws on inconsistent ranges; sanitize before handing values over. */
    private fun apply(s: Slider, st: State) {
        var from = if (st.min.isFinite()) st.min else 0f
        var to = if (st.max.isFinite()) st.max else 1f
        if (to <= from) to = from + 1f
        var step = if (st.step.isFinite() && st.step > 0f) st.step else 0f
        if (step > 0f) {
            val n = (to - from) / step
            if (abs(n - n.roundToInt()) > 1e-3f) step = 0f
        }
        var v = st.value.coerceIn(from, to)
        if (step > 0f) v = from + ((v - from) / step).roundToInt() * step
        v = v.coerceIn(from, to)
        st.muted = true
        try {
            // Order matters: widen the range before narrowing it.
            if (from < s.valueFrom) { s.valueFrom = from; s.valueTo = to } else { s.valueTo = to; s.valueFrom = from }
            s.stepSize = step
            s.value = v
        } finally {
            st.muted = false
        }
    }

    override fun onRecycle(node: Node, host: MountHost) {
        val s = node.view as Slider
        s.clearOnChangeListeners()
        s.clearOnSliderTouchListeners()
        node.state = State()
        apply(s, node.state as State)
    }
}

// ---- Progress / spinner -------------------------------------------------------------------

internal class ProgressManager : ControlManager(ViewType.PROGRESS) {
    override fun createView(host: MountHost): View = LinearProgressIndicator(host.context).apply {
        max = 1000
        trackCornerRadius = 1000
    }

    override fun setProp(node: Node, prop: Int, value: PropValue, host: MountHost): Boolean {
        val p = node.view as LinearProgressIndicator
        when (prop) {
            Prop.VALUE -> p.setProgressCompat(((if (value.isNull) 0f else value.number()).coerceIn(0f, 1f) * 1000).roundToInt(), false)
            Prop.INDETERMINATE -> setIndeterminate(p, !value.isNull && value.bool)
            Prop.TINT_COLOR -> ControlTint.setOverride(p, if (value.kind == ValueKind.COLOR) value.int else null)
            else -> return false
        }
        return true
    }
}

/** Material indicators refuse to switch mode while visible. */
private fun setIndeterminate(p: com.google.android.material.progressindicator.BaseProgressIndicator<*>, on: Boolean) {
    if (p.isIndeterminate == on) return
    val vis = p.visibility
    p.visibility = View.INVISIBLE
    p.isIndeterminate = on
    p.visibility = vis
}


internal class SpinnerManager : ControlManager(ViewType.SPINNER) {
    override fun createView(host: MountHost): View = CircularProgressIndicator(host.context).apply {
        isIndeterminate = true
    }

    override fun onFrame(node: Node, host: MountHost) {
        val p = node.view as CircularProgressIndicator
        val size = minOf(node.width, node.height)
        if (size > 0) {
            p.indicatorSize = size
            p.trackThickness = (size / 10).coerceAtLeast(2)
        }
    }

    override fun setProp(node: Node, prop: Int, value: PropValue, host: MountHost): Boolean {
        val p = node.view as CircularProgressIndicator
        when (prop) {
            Prop.TINT_COLOR -> ControlTint.setOverride(p, if (value.kind == ValueKind.COLOR) value.int else null)
            Prop.VALUE -> {
                if (value.isNull) setIndeterminate(p, true) else {
                    setIndeterminate(p, false)
                    p.setProgressCompat((value.number().coerceIn(0f, 1f) * 100).roundToInt(), false)
                }
            }
            Prop.INDETERMINATE -> setIndeterminate(p, value.isNull || value.bool)
            else -> return false
        }
        return true
    }
}

// ---- Segmented ----------------------------------------------------------------------------

internal class SegmentedManager : ControlManager(ViewType.SEGMENTED) {
    private class State {
        var items: List<String> = emptyList()
        var selected = -1
        var muted = false
    }

    override fun createView(host: MountHost): View = MaterialButtonToggleGroup(host.context).apply {
        isSingleSelection = true
        isSelectionRequired = true
        orientation = LinearLayout.HORIZONTAL
    }

    override fun onMount(node: Node, host: MountHost) {
        if (node.state !is State) node.state = State()
        val st = node.state as State
        val g = node.view as MaterialButtonToggleGroup
        g.clearOnButtonCheckedListeners()
        g.addOnButtonCheckedListener { group, checkedId, isChecked ->
            if (!isChecked || st.muted) return@addOnButtonCheckedListener
            val index = group.indexOfChild(group.findViewById(checkedId))
            if (index < 0 || index == st.selected) return@addOnButtonCheckedListener
            st.selected = index
            if (changeEnabled(node)) {
                host.dispatch(node.id, EventType.CHANGE, "{\"index\":$index,\"value\":${Json.stringify(st.items.getOrNull(index))}}")
            }
        }
    }

    override fun setProp(node: Node, prop: Int, value: PropValue, host: MountHost): Boolean {
        val g = node.view as MaterialButtonToggleGroup
        val st = node.state as State
        when (prop) {
            Prop.ITEMS -> {
                st.items = if (value.isNull) emptyList() else Json.strings(Json.arr(value.string))
                rebuild(g, st)
            }
            Prop.SELECTED_INDEX -> {
                st.selected = if (value.isNull) -1 else value.intValue()
                select(g, st)
            }
            Prop.ENABLED -> {
                setEnabled(g, value)
                for (i in 0 until g.childCount) g.getChildAt(i).isEnabled = g.isEnabled
            }
            else -> return false
        }
        return true
    }

    private fun rebuild(g: MaterialButtonToggleGroup, st: State) {
        st.muted = true
        try {
            g.removeAllViews()
            for (label in st.items) {
                val b = MaterialButton(g.context, null, com.google.android.material.R.attr.materialButtonOutlinedStyle)
                b.id = View.generateViewId()
                b.text = label
                b.isAllCaps = false
                b.insetTop = 0
                b.insetBottom = 0
                b.minWidth = 0
                b.minHeight = 0
                b.minimumWidth = 0
                b.minimumHeight = 0
                b.setPadding(0, 0, 0, 0)
                b.isEnabled = g.isEnabled
                g.addView(b, LinearLayout.LayoutParams(0, ViewGroup.LayoutParams.MATCH_PARENT, 1f))
            }
        } finally {
            st.muted = false
        }
        ControlTint.apply(g)
        select(g, st)
    }

    private fun select(g: MaterialButtonToggleGroup, st: State) {
        st.muted = true
        try {
            val child = g.getChildAt(st.selected)
            if (child != null) g.check(child.id) else g.clearChecked()
        } finally {
            st.muted = false
        }
    }

    override fun onRecycle(node: Node, host: MountHost) {
        (node.view as MaterialButtonToggleGroup).clearOnButtonCheckedListeners()
        node.state = State()
    }
}

// ---- Date picker --------------------------------------------------------------------------

/**
 * A value field showing the localized date/time; tapping opens the platform
 * date/time dialogs (they work in any Activity, unlike MaterialDatePicker which
 * needs fragments). Values stay `YYYY-MM-DD`, `HH:mm`, `YYYY-MM-DDTHH:mm`.
 */
internal class DatePickerManager : ControlManager(ViewType.DATE_PICKER) {
    private class State {
        var date: String = ""
        var mode = 0
    }

    override fun createView(host: MountHost): View = ValueField(host.context)

    override fun onMount(node: Node, host: MountHost) {
        if (node.state !is State) node.state = State()
        val st = node.state as State
        val f = node.view as ValueField
        f.setOnClickListener { open(node, st, f, host) }
        refresh(f, st)
    }

    override fun setProp(node: Node, prop: Int, value: PropValue, host: MountHost): Boolean {
        val f = node.view as ValueField
        val st = node.state as State
        when (prop) {
            Prop.DATE -> st.date = if (value.isNull) "" else value.string ?: ""
            Prop.DATE_MODE -> st.mode = if (value.isNull) 0 else value.intValue().coerceIn(0, 2)
            Prop.ENABLED -> { setEnabled(f, value); f.alpha = if (f.isEnabled) 1f else 0.5f; return true }
            Prop.PLACEHOLDER -> f.placeholder = if (value.isNull) null else value.string
            else -> return false
        }
        refresh(f, st)
        return true
    }

    private fun refresh(f: ValueField, st: State) {
        f.value = ValueFieldMetrics(f.context).displayDate(st.date, st.mode)
    }

    private fun open(node: Node, st: State, f: ValueField, host: MountHost) {
        val cal = parse(st.date)
        val ctx = host.context
        val emit = { c: Calendar ->
            val s = format(c, st.mode)
            st.date = s
            refresh(f, st)
            if (changeEnabled(node)) host.dispatch(node.id, EventType.CHANGE, "{\"date\":${Json.stringify(s)}}")
        }
        val time = {
            TimePickerDialog(ctx, { _, h, m ->
                cal.set(Calendar.HOUR_OF_DAY, h); cal.set(Calendar.MINUTE, m); emit(cal)
            }, cal.get(Calendar.HOUR_OF_DAY), cal.get(Calendar.MINUTE), DateFormat.is24HourFormat(ctx)).show()
        }
        if (st.mode == 1) {
            time()
        } else {
            DatePickerDialog(ctx, { _, y, mo, d ->
                cal.set(y, mo, d)
                if (st.mode == 2) time() else emit(cal)
            }, cal.get(Calendar.YEAR), cal.get(Calendar.MONTH), cal.get(Calendar.DAY_OF_MONTH)).show()
        }
    }

    override fun onRecycle(node: Node, host: MountHost) {
        val f = node.view as ValueField
        f.setOnClickListener(null)
        f.alpha = 1f
        node.state = State()
        f.placeholder = null
        refresh(f, node.state as State)
    }

    companion object {
        fun parse(s: String): Calendar {
            val c = Calendar.getInstance()
            val m = Regex("""^(\d{4})-(\d{2})-(\d{2})(?:[T ](\d{2}):(\d{2}))?""").find(s)
            val t = Regex("""^(\d{2}):(\d{2})""").find(s)
            if (m != null) {
                c.set(m.groupValues[1].toInt(), m.groupValues[2].toInt() - 1, m.groupValues[3].toInt())
                if (m.groupValues[4].isNotEmpty()) {
                    c.set(Calendar.HOUR_OF_DAY, m.groupValues[4].toInt()); c.set(Calendar.MINUTE, m.groupValues[5].toInt())
                }
            } else if (t != null) {
                c.set(Calendar.HOUR_OF_DAY, t.groupValues[1].toInt()); c.set(Calendar.MINUTE, t.groupValues[2].toInt())
            }
            return c
        }

        fun format(c: Calendar, mode: Int): String {
            val date = "%04d-%02d-%02d".format(c.get(Calendar.YEAR), c.get(Calendar.MONTH) + 1, c.get(Calendar.DAY_OF_MONTH))
            val time = "%02d:%02d".format(c.get(Calendar.HOUR_OF_DAY), c.get(Calendar.MINUTE))
            return when (mode) {
                1 -> time
                2 -> "${date}T$time"
                else -> date
            }
        }
    }
}

// ---- Select -------------------------------------------------------------------------------

internal class SelectManager : ControlManager(ViewType.SELECT) {
    private class Item(val label: String, val value: String?, val destructive: Boolean, val disabled: Boolean)
    private class State {
        var title: String? = null
        var items: List<Item> = emptyList()
        var selected = -1
        var placeholder: String? = null
    }

    override fun createView(host: MountHost): View = ValueField(host.context)

    override fun onMount(node: Node, host: MountHost) {
        if (node.state !is State) node.state = State()
        val st = node.state as State
        val b = node.view as ValueField
        b.setOnClickListener { open(node, st, b, host) }
        refresh(b, st)
    }

    override fun setProp(node: Node, prop: Int, value: PropValue, host: MountHost): Boolean {
        val b = node.view as ValueField
        val st = node.state as State
        when (prop) {
            Prop.MENU -> {
                val o = if (value.isNull) null else Json.obj(value.string)
                st.title = Json.string(o, "title")
                Json.string(o, "placeholder")?.let { st.placeholder = it }
                val arr = o?.optJSONArray("items")
                st.items = (0 until (arr?.length() ?: 0)).mapNotNull { i ->
                    val it = arr!!.optJSONObject(i) ?: return@mapNotNull null
                    Item(Json.string(it, "label") ?: "", Json.string(it, "value"), Json.bool(it, "destructive", false), Json.bool(it, "disabled", false))
                }
            }
            Prop.ITEMS -> st.items = if (value.isNull) emptyList() else Json.strings(Json.arr(value.string)).map { Item(it, it, false, false) }
            Prop.SELECTED_INDEX -> st.selected = if (value.isNull) -1 else value.intValue()
            Prop.PLACEHOLDER -> st.placeholder = if (value.isNull) null else value.string
            Prop.ENABLED -> { setEnabled(b, value); b.alpha = if (b.isEnabled) 1f else 0.5f }
            else -> return false
        }
        refresh(b, st)
        return true
    }

    private fun refresh(b: ValueField, st: State) {
        b.value = st.items.getOrNull(st.selected)?.label
        b.placeholder = st.placeholder
    }

    private fun open(node: Node, st: State, anchor: View, host: MountHost) {
        val menu = PopupMenu(host.context, anchor)
        st.items.forEachIndexed { i, item ->
            val label = if (item.destructive) android.text.SpannableString(item.label).apply {
                setSpan(android.text.style.ForegroundColorSpan(0xFFDC2626.toInt()), 0, length, 0)
            } else item.label
            menu.menu.add(Menu.NONE, i, i, label).isEnabled = !item.disabled
        }
        menu.setOnMenuItemClickListener { mi ->
            val i = mi.itemId
            if (changeEnabled(node)) {
                host.dispatch(node.id, EventType.CHANGE, "{\"index\":$i,\"value\":${Json.stringify(st.items.getOrNull(i)?.value)}}")
            }
            true
        }
        menu.show()
    }

    override fun onRecycle(node: Node, host: MountHost) {
        node.view.setOnClickListener(null)
        node.state = State()
        (node.view as ValueField).alpha = 1f
        refresh(node.view as ValueField, node.state as State)
    }
}
