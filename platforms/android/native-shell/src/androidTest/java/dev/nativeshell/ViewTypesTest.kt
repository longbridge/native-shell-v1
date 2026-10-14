package dev.nativeshell

import android.view.View
import android.widget.CompoundButton
import android.widget.ImageView
import android.widget.TextView
import androidx.test.ext.junit.runners.AndroidJUnit4
import com.google.android.material.button.MaterialButtonToggleGroup
import com.google.android.material.materialswitch.MaterialSwitch
import com.google.android.material.progressindicator.CircularProgressIndicator
import com.google.android.material.progressindicator.LinearProgressIndicator
import com.google.android.material.slider.Slider
import dev.nativeshell.protocol.EventType
import dev.nativeshell.protocol.Prop
import dev.nativeshell.protocol.PropValue
import dev.nativeshell.protocol.PropValue.Companion.bool
import dev.nativeshell.protocol.PropValue.Companion.color
import dev.nativeshell.protocol.PropValue.Companion.f32
import dev.nativeshell.protocol.PropValue.Companion.floats
import dev.nativeshell.protocol.PropValue.Companion.i32
import dev.nativeshell.protocol.PropValue.Companion.json
import dev.nativeshell.protocol.PropValue.Companion.str
import dev.nativeshell.protocol.ViewType
import dev.nativeshell.render.BoxDrawable
import dev.nativeshell.views.CheckContainer
import dev.nativeshell.views.HostComponentContainer
import dev.nativeshell.views.ShapeView
import dev.nativeshell.views.ShellEditText
import dev.nativeshell.views.ShellListView
import dev.nativeshell.views.ShellScrollView
import dev.nativeshell.views.ShellTextView
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertSame
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith

/**
 * Every pooled view type: mounting with its props applied, then the reset
 * contract — a released view, reused for a new node, is indistinguishable
 * from a fresh one.
 */
@RunWith(AndroidJUnit4::class)
class ViewTypesTest : ActivityTest() {

    private val common: List<Pair<Int, PropValue>> = listOf(
        Prop.BACKGROUND_COLOR to color(0xFF3366FF.toInt()),
        Prop.CORNER_RADII to floats(floatArrayOf(8f, 8f, 2f, 2f)),
        Prop.BORDER_WIDTHS to floats(floatArrayOf(1f, 2f, 1f, 2f)),
        Prop.BORDER_COLOR to color(0xFF000000.toInt()),
        Prop.OPACITY to f32(0.7f),
        Prop.SHADOW to json("""{"color":1073741824,"x":0,"y":2,"blur":6,"spread":0}"""),
        Prop.CLIPS_TO_BOUNDS to bool(true),
        Prop.TRANSFORM to json("""{"translate_x":3,"translate_y":4,"scale":1.1,"rotate":5}"""),
        Prop.A11Y_LABEL to str("label"),
        Prop.A11Y_ROLE to i32(1),
        Prop.A11Y_VALUE to str("value"),
        Prop.TEST_ID to str("tid"),
        Prop.HIT_SLOP to floats(floatArrayOf(4f, 4f, 4f, 4f)),
    )

    private val specific: Map<Int, List<Pair<Int, PropValue>>> = mapOf(
        ViewType.VIEW to listOf(Prop.ACTIVE_OPACITY to f32(0.5f), Prop.HIDDEN to bool(true)),
        ViewType.TEXT to listOf(
            Prop.ATTRIBUTED_TEXT to json("""{"runs":[{"text":"Hello","size":16,"weight":700,"color":4278190080}]}"""),
            Prop.SELECTABLE to bool(true),
            Prop.LINKS_ENABLED to bool(true),
        ),
        ViewType.IMAGE to listOf(
            Prop.IMAGE_SOURCE to str("asset://missing.png"),
            Prop.CONTENT_MODE to i32(1),
            Prop.TINT_COLOR to color(0xFFFF0000.toInt()),
        ),
        ViewType.SCROLL to listOf(
            Prop.HORIZONTAL to bool(true),
            Prop.CONTENT_SIZE to floats(floatArrayOf(800f, 100f)),
            Prop.SHOWS_INDICATORS to bool(false),
            Prop.REFRESH_ENABLED to bool(true),
        ),
        ViewType.TEXT_INPUT to listOf(
            Prop.TEXT_VALUE to json("""{"text":"abc","count":0}"""),
            Prop.PLACEHOLDER to str("Type"),
            Prop.MULTILINE to bool(true),
            Prop.SECURE to bool(true),
            Prop.KEYBOARD_TYPE to i32(1),
            Prop.MAX_LENGTH to i32(10),
            Prop.ALLOWED_CHARS to str("abc"),
            Prop.EDITABLE to bool(false),
            Prop.RETURN_KEY to i32(5),
            Prop.TEXT_STYLE to json("""{"size":20,"weight":600,"color":4294901760,"align":"center"}"""),
            Prop.CONTENT_TYPE to i32(1),
            Prop.PLACEHOLDER_COLOR to color(0xFF00FF00.toInt()),
            Prop.AUTO_CAPITALIZE to i32(1),
        ),
        ViewType.SWITCH to listOf(Prop.CHECKED to bool(true), Prop.ENABLED to bool(false)),
        ViewType.CHECKBOX to listOf(Prop.CHECKED to bool(true), Prop.CHECK_STYLE to i32(1), Prop.ENABLED to bool(false)),
        ViewType.SLIDER to listOf(Prop.MIN to f32(10f), Prop.MAX to f32(20f), Prop.STEP to f32(1f), Prop.VALUE to f32(15f)),
        ViewType.PROGRESS to listOf(Prop.VALUE to f32(0.4f), Prop.INDETERMINATE to bool(true)),
        ViewType.SPINNER to listOf(Prop.TINT_COLOR to color(0xFFFF0000.toInt())),
        ViewType.SEGMENTED to listOf(Prop.ITEMS to json("""["One","Two","Three"]"""), Prop.SELECTED_INDEX to i32(2)),
        ViewType.DATE_PICKER to listOf(Prop.DATE to str("2026-10-14"), Prop.DATE_MODE to i32(2)),
        ViewType.SELECT to listOf(
            Prop.MENU to json("""{"title":"Pick","items":[{"label":"A","value":"a"},{"label":"B","value":"b"}]}"""),
            Prop.SELECTED_INDEX to i32(1),
            Prop.PLACEHOLDER to str("Choose"),
        ),
        ViewType.SHAPE to listOf(
            Prop.PATH_DATA to str("M0 0 L10 10 Z"),
            Prop.FILL_COLOR to color(0xFF00FF00.toInt()),
            Prop.STROKE_COLOR to color(0xFF0000FF.toInt()),
            Prop.STROKE_WIDTH to f32(2f),
            Prop.VIEW_BOX to floats(floatArrayOf(0f, 0f, 10f, 10f)),
        ),
        ViewType.LIST to listOf(Prop.ITEM_COUNT to i32(50), Prop.ROW_OFFSETS to floats(FloatArray(51) { it * 40f }), Prop.PREFETCH to f32(3f)),
        ViewType.LIST_ROW to listOf(Prop.ROW_INDEX to i32(3)),
        ViewType.HOST_COMPONENT to listOf(Prop.HOST_COMPONENT to str("test-label"), Prop.HOST_PROPS to json("{\"text\":\"hi\"}")),
    )

    init {
        NativeShell.registerComponent(HostComponentSpec("test-label") {
            object : HostComponent {
                override fun createView(context: android.content.Context): View = TextView(context)
                override fun update(view: View, props: org.json.JSONObject) { (view as TextView).text = props.optString("text") }
                override fun reset(view: View) { (view as TextView).text = "" }
            }
        })
    }

    /** Observable state of a view that its props can change. */
    private fun signature(v: View): List<Any?> {
        val s = mutableListOf<Any?>(
            v.javaClass.name, v.alpha, v.visibility, v.translationX, v.translationY, v.scaleX, v.scaleY, v.rotation,
            v.elevation, (v.background as? BoxDrawable)?.isEmpty ?: (v.background == null), v.clipToOutline,
            v.contentDescription, v.isEnabled, v.getTag(R.id.nsh_test_id), v.getTag(R.id.nsh_hit_slop), v.hasOnClickListeners(),
        )
        when (v) {
            is ShellTextView -> s.addAll(listOf(v.json, v.selectable, v.linksEnabled))
            is ImageView -> s.addAll(listOf(v.scaleType, v.drawable, v.imageTintList))
            is ShellScrollView -> s.addAll(listOf(v.horizontal, v.contentWidth, v.contentHeight, v.showsIndicators, v.refresh.isEnabled, v.refresh.isRefreshing))
            is ShellEditText -> s.addAll(listOf(
                v.text.toString(), v.hint, v.inputType, v.imeOptions, v.textSize, v.currentTextColor, v.gravity, v.filters.size,
                v.isFocusable, v.hintTextColors.defaultColor, v.typeface, v.nativeEventCount,
            ))
            is MaterialSwitch -> s.addAll(listOf(v.isChecked))
            is CheckContainer -> s.addAll(listOf(v.style, v.button.isChecked, v.button.isEnabled))
            is Slider -> s.addAll(listOf(v.value, v.valueFrom, v.valueTo, v.stepSize))
            is LinearProgressIndicator -> s.addAll(listOf(v.progress, v.isIndeterminate))
            is CircularProgressIndicator -> s.addAll(listOf(v.isIndeterminate))
            is MaterialButtonToggleGroup -> s.addAll(listOf(v.childCount, v.checkedButtonId))
            is TextView -> s.addAll(listOf(v.text.toString(), v.hint))
            is dev.nativeshell.views.ValueField -> s.addAll(listOf(v.value, v.placeholder, v.hasOnClickListeners()))
            is ShapeView -> s.addAll(listOf(v.pathData, v.fill, v.stroke, v.strokeWidth, v.viewBox?.toList()))
            is ShellListView -> s.addAll(listOf(v.count, v.prefetch, v.rowCount))
            is HostComponentContainer -> s.addAll(listOf(v.name, v.childCount))
        }
        return s
    }

    @Test
    fun everyPooledTypeMountsAndResetsToDefaults() {
        val h = harness()
        var id = 1
        val types = specific.keys.toList()
        for (type in types) {
            val fresh = ++id
            onMain {
                h.batch {
                    create(fresh, type)
                    if (type == ViewType.LIST_ROW) {
                        create(fresh + 1000, ViewType.LIST); insert(fresh + 1000, 0, 0); frame(fresh + 1000, 0f, 0f, 300f, 400f)
                        insert(fresh, fresh + 1000, 0)
                    } else {
                        insert(fresh, 0, 0)
                    }
                    frame(fresh, 10f, 10f, 200f, 60f)
                }
            }
            val view = onMain { h.view(fresh) }
            val before = onMain { signature(view) }
            onMain {
                h.batch {
                    for ((p, v) in common + specific.getValue(type)) prop(fresh, p, v)
                    mask(fresh, 0x3FFFF)
                }
            }
            val during = onMain { signature(view) }
            assertNotEquals("${ViewType.name(type)} props had no visible effect", before, during)
            onMain { h.batch { delete(fresh); if (type == ViewType.LIST_ROW) delete(fresh + 1000) } }
            val reuse = ++id
            onMain {
                h.batch {
                    create(reuse, type)
                    if (type == ViewType.LIST_ROW) {
                        create(reuse + 1000, ViewType.LIST); insert(reuse + 1000, 0, 0); frame(reuse + 1000, 0f, 0f, 300f, 400f)
                        insert(reuse, reuse + 1000, 0)
                    } else {
                        insert(reuse, 0, 0)
                    }
                    frame(reuse, 10f, 10f, 200f, 60f)
                }
            }
            val reused = onMain { h.view(reuse) }
            assertSame("${ViewType.name(type)} view was not pooled", view, reused)
            assertEquals("${ViewType.name(type)} did not reset", before, onMain { signature(reused) })
            onMain { h.batch { delete(reuse); if (type == ViewType.LIST_ROW) delete(reuse + 1000) } }
        }
    }

    @Test
    fun controlsReportChanges() {
        val h = harness()
        onMain {
            h.batch {
                create(2, ViewType.SWITCH); insert(2, 0, 0); frame(2, 0f, 0f, 60f, 40f); mask(2, 1 shl 3)
                create(3, ViewType.CHECKBOX); insert(3, 0, 1); frame(3, 0f, 50f, 40f, 40f); mask(3, 1 shl 3)
                create(4, ViewType.SEGMENTED); insert(4, 0, 2); frame(4, 0f, 100f, 300f, 40f); mask(4, 1 shl 3)
                prop(4, Prop.ITEMS, json("""["A","B"]""")); prop(4, Prop.SELECTED_INDEX, i32(0))
                create(5, ViewType.SLIDER); insert(5, 0, 3); frame(5, 0f, 150f, 300f, 40f); mask(5, 1 shl 3)
            }
        }
        onMain {
            (h.view(2) as CompoundButton).performClick()
            (h.view(3) as CheckContainer).button.performClick()
            (h.view(4) as MaterialButtonToggleGroup).getChildAt(1).performClick()
        }
        val changes = h.eventsOf(EventType.CHANGE)
        assertEquals("""{"checked":true}""", changes[0].payload)
        assertEquals(2, changes[0].node)
        assertEquals("""{"checked":true}""", changes[1].payload)
        assertEquals("""{"index":1,"value":"B"}""", changes[2].payload)
        // Programmatic updates never echo back.
        onMain { h.batch { prop(2, Prop.CHECKED, bool(false)); prop(4, Prop.SELECTED_INDEX, i32(0)) } }
        assertEquals(3, h.eventsOf(EventType.CHANGE).size)
        assertTrue(!(h.view(2) as CompoundButton).isChecked)
    }
}
