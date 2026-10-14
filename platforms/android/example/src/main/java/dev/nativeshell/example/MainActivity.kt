package dev.nativeshell.example

import android.content.Intent
import android.graphics.drawable.GradientDrawable
import android.os.Bundle
import android.text.TextUtils
import android.util.TypedValue
import android.view.Gravity
import android.view.View
import android.view.ViewGroup.LayoutParams.MATCH_PARENT
import android.view.ViewGroup.LayoutParams.WRAP_CONTENT
import android.widget.ImageView
import android.widget.LinearLayout
import android.widget.ScrollView
import android.widget.TextView
import androidx.activity.ComponentActivity
import androidx.activity.enableEdgeToEdge
import androidx.core.view.ViewCompat
import androidx.core.view.WindowInsetsCompat
import androidx.core.widget.TextViewCompat
import com.google.android.material.color.MaterialColors

/** One example in the launcher. */
data class Example(val name: String, val title: String, val summary: String, val icon: Int, val tint: Int)

/**
 * The launcher: the repository's example programs as a Material list. A tap
 * opens the program full screen in [ProgramActivity]; system back returns here.
 * `--es program <name> [--ez reset true]` opens one directly.
 */
class MainActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        enableEdgeToEdge()
        super.onCreate(savedInstanceState)
        val column = LinearLayout(this).apply { orientation = LinearLayout.VERTICAL }
        column.addView(
            TextView(this).apply {
                text = getString(R.string.examples_title)
                TextViewCompat.setTextAppearance(this, com.google.android.material.R.style.TextAppearance_Material3_HeadlineMedium)
                setTextColor(MaterialColors.getColor(this, com.google.android.material.R.attr.colorOnSurface))
                setPadding(dp(16), dp(24), dp(16), dp(8))
                ViewCompat.setAccessibilityHeading(this, true)
            },
        )
        for ((title, examples) in SECTIONS) {
            column.addView(subheader(title))
            for (example in examples) column.addView(row(example))
        }
        val scroll = ScrollView(this).apply {
            clipToPadding = false
            addView(column, MATCH_PARENT, WRAP_CONTENT)
        }
        ViewCompat.setOnApplyWindowInsetsListener(scroll) { v, insets ->
            val bars = insets.getInsets(WindowInsetsCompat.Type.systemBars() or WindowInsetsCompat.Type.displayCutout())
            v.setPadding(bars.left, bars.top, bars.right, bars.bottom + dp(16))
            insets
        }
        setContentView(scroll)
        // `--es program <name>` opens one directly, as if tapped (tests and scripts use it).
        if (savedInstanceState == null) intent.getStringExtra(ProgramActivity.EXTRA_PROGRAM)?.let { open(it) }
    }

    private fun subheader(title: String) = TextView(this).apply {
        text = title
        TextViewCompat.setTextAppearance(this, com.google.android.material.R.style.TextAppearance_Material3_TitleSmall)
        setTextColor(MaterialColors.getColor(this, androidx.appcompat.R.attr.colorPrimary))
        gravity = Gravity.CENTER_VERTICAL
        minHeight = dp(48)
        setPadding(dp(16), dp(8), dp(16), 0)
        ViewCompat.setAccessibilityHeading(this, true)
    }

    private fun row(example: Example): View {
        val tile = ImageView(this).apply {
            setImageResource(example.icon)
            scaleType = ImageView.ScaleType.CENTER
            background = GradientDrawable().apply {
                cornerRadius = dp(12).toFloat()
                setColor(example.tint)
            }
            importantForAccessibility = View.IMPORTANT_FOR_ACCESSIBILITY_NO
        }
        val texts = LinearLayout(this).apply {
            orientation = LinearLayout.VERTICAL
            addView(text(example.title, com.google.android.material.R.style.TextAppearance_Material3_BodyLarge, com.google.android.material.R.attr.colorOnSurface))
            addView(text(example.summary, com.google.android.material.R.style.TextAppearance_Material3_BodyMedium, com.google.android.material.R.attr.colorOnSurfaceVariant))
        }
        return LinearLayout(this).apply {
            orientation = LinearLayout.HORIZONTAL
            gravity = Gravity.CENTER_VERTICAL
            minimumHeight = dp(72)
            setPadding(dp(16), dp(12), dp(24), dp(12))
            background = TypedValue().let { tv ->
                context.theme.resolveAttribute(android.R.attr.selectableItemBackground, tv, true)
                context.getDrawable(tv.resourceId)
            }
            tag = "example-${example.name}"
            contentDescription = "${example.title}. ${example.summary}"
            addView(tile, LinearLayout.LayoutParams(dp(40), dp(40)).apply { marginEnd = dp(16) })
            addView(texts, LinearLayout.LayoutParams(0, WRAP_CONTENT, 1f))
            setOnClickListener { open(example.name) }
        }
    }

    private fun text(value: String, appearance: Int, color: Int) = TextView(this).apply {
        text = value
        TextViewCompat.setTextAppearance(this, appearance)
        setTextColor(MaterialColors.getColor(this, color))
        maxLines = 2
        ellipsize = TextUtils.TruncateAt.END
    }

    private fun open(name: String) {
        startActivity(
            Intent(this, ProgramActivity::class.java)
                .putExtra(ProgramActivity.EXTRA_PROGRAM, name)
                .putExtra(ProgramActivity.EXTRA_RESET, intent.getBooleanExtra(ProgramActivity.EXTRA_RESET, false)),
        )
    }

    private fun dp(v: Int) = (v * resources.displayMetrics.density).toInt()

    companion object {
        val SECTIONS = listOf(
            "Apps" to listOf(
                Example("tasks", "Tasks", "Lists, editing and undo", R.drawable.ic_tasks, 0xFF1E88E5.toInt()),
                Example("chat", "Chat", "A thread and its composer", R.drawable.ic_chat, 0xFF43A047.toInt()),
                Example("watchlist", "Watchlist", "Host modules and a native cell", R.drawable.ic_watchlist, 0xFF5E35B1.toInt()),
                Example("settings", "Settings", "Grouped preferences and pages", R.drawable.ic_settings, 0xFF757575.toInt()),
            ),
            "Reference" to listOf(
                Example("components", "Components", "Every control, in context", R.drawable.ic_components, 0xFFFB8C00.toInt()),
                Example("gpui-todolist", "gpui-shell to-do list", "A desktop program, unchanged", R.drawable.ic_desktop, 0xFF00897B.toInt()),
            ),
        )
        val PROGRAMS = SECTIONS.flatMap { (_, examples) -> examples.map { it.name } }
    }
}
