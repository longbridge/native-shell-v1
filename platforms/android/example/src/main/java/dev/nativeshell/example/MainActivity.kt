package dev.nativeshell.example

import android.content.Intent
import android.content.res.ColorStateList
import android.content.res.Configuration
import android.graphics.Typeface
import android.graphics.drawable.ColorDrawable
import android.graphics.drawable.GradientDrawable
import android.graphics.drawable.StateListDrawable
import android.os.Bundle
import android.util.TypedValue
import android.view.Gravity
import android.view.View
import android.view.ViewGroup.LayoutParams.MATCH_PARENT
import android.view.ViewGroup.LayoutParams.WRAP_CONTENT
import android.widget.FrameLayout
import android.widget.ImageView
import android.widget.LinearLayout
import android.widget.ScrollView
import android.widget.TextView
import androidx.activity.ComponentActivity
import androidx.activity.enableEdgeToEdge
import androidx.core.view.ViewCompat
import androidx.core.view.WindowInsetsCompat

/** One example in the launcher; [tint] is the tile color in light and dark. */
data class Example(val name: String, val title: String, val summary: String, val icon: Int, val tint: Pair<Int, Int>)

/**
 * The launcher: the repository's example programs as an iOS inset grouped
 * list under a large title, which hands over to a centered compact title as
 * the list scrolls. The framework's design language is iOS on both platforms,
 * so the launcher matches the iOS Example rather than Material. A tap opens
 * the program full screen in [ProgramActivity]; back returns here.
 * `--es program <name> [--ez reset true]` opens one directly.
 */
class MainActivity : ComponentActivity() {
    private val dark get() = resources.configuration.uiMode and Configuration.UI_MODE_NIGHT_MASK == Configuration.UI_MODE_NIGHT_YES

    // iOS system colors: grouped backgrounds, label hierarchy, separator, pressed fill.
    private val groupedBackground get() = if (dark) 0xFF000000.toInt() else 0xFFF2F2F7.toInt()
    private val cell get() = if (dark) 0xFF1C1C1E.toInt() else 0xFFFFFFFF.toInt()
    private val label get() = if (dark) 0xFFFFFFFF.toInt() else 0xFF000000.toInt()
    private val secondaryLabel get() = if (dark) 0x99EBEBF5.toInt() else 0x993C3C43.toInt()
    private val tertiaryLabel get() = if (dark) 0x4DEBEBF5 else 0x4D3C3C43
    private val separator get() = if (dark) 0xA6545458.toInt() else 0x4A3C3C43
    private val pressed get() = if (dark) 0xFF3A3A3C.toInt() else 0xFFD1D1D6.toInt()

    override fun onCreate(savedInstanceState: Bundle?) {
        enableEdgeToEdge()
        super.onCreate(savedInstanceState)
        val title = getString(R.string.examples_title)
        val largeTitle = TextView(this).apply {
            text = title
            setTextSize(TypedValue.COMPLEX_UNIT_SP, 34f)
            typeface = Typeface.DEFAULT_BOLD
            setTextColor(label)
            setPadding(dp(16), dp(4), dp(16), dp(8))
            ViewCompat.setAccessibilityHeading(this, true)
        }
        val column = LinearLayout(this).apply {
            orientation = LinearLayout.VERTICAL
            addView(largeTitle)
        }
        for ((header, examples) in SECTIONS) {
            column.addView(sectionHeader(header))
            column.addView(card(examples), LinearLayout.LayoutParams(MATCH_PARENT, WRAP_CONTENT).apply { setMargins(dp(16), 0, dp(16), 0) })
        }
        val scroll = ScrollView(this).apply {
            clipToPadding = false
            isVerticalScrollBarEnabled = false
            addView(column, MATCH_PARENT, WRAP_CONTENT)
        }

        // The compact bar: empty over the large title, then the centered title
        // and a hairline once the large title has scrolled under it.
        val compactTitle = TextView(this).apply {
            text = title
            setTextSize(TypedValue.COMPLEX_UNIT_SP, 17f)
            typeface = Typeface.create("sans-serif-medium", Typeface.NORMAL)
            setTextColor(label)
            gravity = Gravity.CENTER
            alpha = 0f
            importantForAccessibility = View.IMPORTANT_FOR_ACCESSIBILITY_NO
        }
        val hairline = View(this).apply {
            setBackgroundColor(separator)
            alpha = 0f
        }
        val bar = FrameLayout(this).apply {
            setBackgroundColor(groupedBackground)
            addView(compactTitle, FrameLayout.LayoutParams(MATCH_PARENT, dp(44), Gravity.BOTTOM))
            addView(hairline, FrameLayout.LayoutParams(MATCH_PARENT, maxOf(1, dp(0.5f)), Gravity.BOTTOM))
        }
        scroll.setOnScrollChangeListener { _, _, y, _, _ ->
            val collapsed = if (y > largeTitle.bottom - largeTitle.paddingBottom - dp(8)) 1f else 0f
            if (compactTitle.alpha != collapsed) {
                compactTitle.animate().alpha(collapsed).setDuration(150).start()
                hairline.animate().alpha(collapsed).setDuration(150).start()
            }
        }

        val root = FrameLayout(this).apply {
            setBackgroundColor(groupedBackground)
            addView(scroll, MATCH_PARENT, MATCH_PARENT)
            addView(bar, FrameLayout.LayoutParams(MATCH_PARENT, WRAP_CONTENT, Gravity.TOP))
        }
        ViewCompat.setOnApplyWindowInsetsListener(root) { _, insets ->
            val bars = insets.getInsets(WindowInsetsCompat.Type.systemBars() or WindowInsetsCompat.Type.displayCutout())
            bar.setPadding(bars.left, bars.top, bars.right, 0)
            bar.layoutParams = bar.layoutParams.apply { height = bars.top + dp(44) }
            scroll.setPadding(bars.left, bars.top + dp(44), bars.right, bars.bottom + dp(32))
            insets
        }
        setContentView(root)
        // `--es program <name>` opens one directly, as if tapped (tests and scripts use it).
        if (savedInstanceState == null) intent.getStringExtra(ProgramActivity.EXTRA_PROGRAM)?.let { open(it) }
    }

    /** Footnote, uppercase, secondary: an inset grouped section's header, aligned with the row text. */
    private fun sectionHeader(title: String) = TextView(this).apply {
        text = title.uppercase()
        setTextSize(TypedValue.COMPLEX_UNIT_SP, 13f)
        setTextColor(secondaryLabel)
        setPadding(dp(32), dp(24), dp(32), dp(7))
        contentDescription = title
        ViewCompat.setAccessibilityHeading(this, true)
    }

    /** The section's rows on one rounded cell background, separated by hairlines inset to the text. */
    private fun card(examples: List<Example>) = LinearLayout(this).apply {
        orientation = LinearLayout.VERTICAL
        background = GradientDrawable().apply {
            cornerRadius = dp(10).toFloat()
            setColor(cell)
        }
        clipToOutline = true
        examples.forEachIndexed { i, example ->
            if (i > 0) {
                addView(
                    View(context).apply { setBackgroundColor(separator) },
                    LinearLayout.LayoutParams(MATCH_PARENT, maxOf(1, dp(0.5f))).apply { marginStart = dp(16 + 32 + 14) },
                )
            }
            addView(row(example))
        }
    }

    private fun row(example: Example): View {
        val tile = ImageView(this).apply {
            setImageResource(example.icon)
            scaleType = ImageView.ScaleType.CENTER_INSIDE
            setPadding(dp(6), dp(6), dp(6), dp(6))
            background = GradientDrawable().apply {
                cornerRadius = dp(8).toFloat()
                setColor(if (dark) example.tint.second else example.tint.first)
            }
            importantForAccessibility = View.IMPORTANT_FOR_ACCESSIBILITY_NO
        }
        val texts = LinearLayout(this).apply {
            orientation = LinearLayout.VERTICAL
            addView(text(example.title, 17f, label))
            addView(text(example.summary, 15f, secondaryLabel).apply { setPadding(0, dp(2), 0, 0) })
        }
        val chevron = ImageView(this).apply {
            setImageResource(R.drawable.ic_chevron)
            imageTintList = ColorStateList.valueOf(tertiaryLabel)
            importantForAccessibility = View.IMPORTANT_FOR_ACCESSIBILITY_NO
        }
        return LinearLayout(this).apply {
            orientation = LinearLayout.HORIZONTAL
            gravity = Gravity.CENTER_VERTICAL
            minimumHeight = dp(44)
            setPadding(dp(16), dp(10), dp(16), dp(10))
            // iOS highlights a pressed row with a flat fill, not a ripple.
            background = StateListDrawable().apply {
                addState(intArrayOf(android.R.attr.state_pressed), ColorDrawable(pressed))
                addState(intArrayOf(), ColorDrawable(0))
                setExitFadeDuration(200)
            }
            isClickable = true
            tag = "example-${example.name}"
            contentDescription = "${example.title}. ${example.summary}"
            addView(tile, LinearLayout.LayoutParams(dp(32), dp(32)).apply { marginEnd = dp(14) })
            addView(texts, LinearLayout.LayoutParams(0, WRAP_CONTENT, 1f))
            addView(chevron, LinearLayout.LayoutParams(dp(8), dp(14)).apply { marginStart = dp(8) })
            setOnClickListener { open(example.name) }
        }
    }

    private fun text(value: String, size: Float, color: Int) = TextView(this).apply {
        text = value
        setTextSize(TypedValue.COMPLEX_UNIT_SP, size)
        setTextColor(color)
    }

    private fun open(name: String) {
        startActivity(
            Intent(this, ProgramActivity::class.java)
                .putExtra(ProgramActivity.EXTRA_PROGRAM, name)
                .putExtra(ProgramActivity.EXTRA_RESET, intent.getBooleanExtra(ProgramActivity.EXTRA_RESET, false)),
        )
    }

    private fun dp(v: Int) = (v * resources.displayMetrics.density).toInt()

    private fun dp(v: Float) = (v * resources.displayMetrics.density).toInt()

    companion object {
        // Tile colors are iOS system colors (light, dark), as the iOS Example uses.
        private val BLUE = 0xFF007AFF.toInt() to 0xFF0A84FF.toInt()
        private val YELLOW = 0xFFFFCC00.toInt() to 0xFFFFD60A.toInt()
        private val GREEN = 0xFF34C759.toInt() to 0xFF30D158.toInt()
        private val INDIGO = 0xFF5856D6.toInt() to 0xFF5E5CE6.toInt()
        private val GRAY = 0xFF8E8E93.toInt() to 0xFF8E8E93.toInt()
        private val ORANGE = 0xFFFF9500.toInt() to 0xFFFF9F0A.toInt()
        private val TEAL = 0xFF30B0C7.toInt() to 0xFF40C8E0.toInt()

        val SECTIONS = listOf(
            "Apps" to listOf(
                Example("tasks", "Tasks", "Lists, editing and undo", R.drawable.ic_tasks, BLUE),
                Example("notes", "Notes", "Folders, checklists and search", R.drawable.ic_notes, YELLOW),
                Example("chat", "Chat", "A thread and its composer", R.drawable.ic_chat, GREEN),
                Example("watchlist", "Watchlist", "Host modules and a native cell", R.drawable.ic_watchlist, INDIGO),
                Example("settings", "Settings", "Grouped preferences and pages", R.drawable.ic_settings, GRAY),
            ),
            "Reference" to listOf(
                Example("components", "Components", "Every control, in context", R.drawable.ic_components, ORANGE),
                Example("gpui-todolist", "gpui-shell To-Do List", "A desktop program, unchanged", R.drawable.ic_desktop, TEAL),
            ),
        )
        val PROGRAMS = SECTIONS.flatMap { (_, examples) -> examples.map { it.name } }
    }
}
