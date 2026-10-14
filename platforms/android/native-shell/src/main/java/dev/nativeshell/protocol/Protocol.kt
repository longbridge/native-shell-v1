package dev.nativeshell.protocol

/**
 * Constants of the binary mutation stream, mirrored from `crates/core/src/protocol.rs`.
 *
 * The values are the wire contract; names follow the Rust enums so a reader can
 * grep across both sides. Everything is `const val Int` so the decoder and the
 * mounting code switch on plain ints without boxing.
 */
object Protocol {
    const val MAGIC: Int = 0x3142534E // "NSB1" little-endian
    const val VERSION: Int = 1
    const val HEADER_WORDS: Int = 5
    const val EXTENDED_NODE: Int = 0x80
    const val NODE_MASK: Int = 0x00FF_FFFF
    const val ROOT_NODE: Int = 0
}

object Op {
    const val BEGIN_BATCH = 1
    const val END_BATCH = 2
    const val CREATE = 3
    const val INSERT = 4
    const val REMOVE = 5
    const val DELETE = 6
    const val MOVE = 7
    const val SET_PROP = 8
    const val SET_FRAME = 9
    const val SET_EVENT_MASK = 10
    const val COMMAND = 11

    fun isValid(op: Int) = op in BEGIN_BATCH..COMMAND
}

object BatchFlags {
    const val ANIMATED = 1
    const val INITIAL = 2
}

object ViewType {
    const val VIEW = 1
    const val TEXT = 2
    const val IMAGE = 3
    const val SCROLL = 4
    const val TEXT_INPUT = 5
    const val SWITCH = 6
    const val CHECKBOX = 7
    const val SLIDER = 8
    const val PROGRESS = 9
    const val LIST = 10
    const val LIST_ROW = 11
    const val PAGE = 12
    const val SHEET = 13
    const val DIALOG = 14
    const val SEGMENTED = 15
    const val DATE_PICKER = 16
    const val SELECT = 17
    const val SHAPE = 18
    const val SPINNER = 19
    const val HOST_COMPONENT = 20

    fun isValid(t: Int) = t in VIEW..HOST_COMPONENT

    fun name(t: Int): String = when (t) {
        VIEW -> "View"; TEXT -> "Text"; IMAGE -> "Image"; SCROLL -> "Scroll"; TEXT_INPUT -> "TextInput"
        SWITCH -> "Switch"; CHECKBOX -> "Checkbox"; SLIDER -> "Slider"; PROGRESS -> "Progress"; LIST -> "List"
        LIST_ROW -> "ListRow"; PAGE -> "Page"; SHEET -> "Sheet"; DIALOG -> "Dialog"; SEGMENTED -> "Segmented"
        DATE_PICKER -> "DatePicker"; SELECT -> "Select"; SHAPE -> "Shape"; SPINNER -> "Spinner"
        HOST_COMPONENT -> "HostComponent"; else -> "Unknown($t)"
    }
}

object ValueKind {
    const val NULL = 0
    const val BOOL = 1
    const val I32 = 2
    const val F32 = 3
    const val COLOR = 4
    const val STR = 5
    const val JSON = 6
    const val F32_ARRAY = 7

    fun isValid(k: Int) = k in NULL..F32_ARRAY

    /** Value words that follow the prop word. */
    fun words(k: Int) = if (k == NULL) 0 else 1
}

object Prop {
    const val BACKGROUND_COLOR = 1
    const val CORNER_RADII = 2
    const val BORDER_WIDTHS = 3
    const val BORDER_COLOR = 4
    const val OPACITY = 5
    const val SHADOW = 6
    const val GRADIENT = 7
    const val CLIPS_TO_BOUNDS = 8
    const val HIDDEN = 9
    const val TRANSFORM = 10
    const val ANIMATION = 11

    const val ATTRIBUTED_TEXT = 20
    const val SELECTABLE = 21
    const val LINKS_ENABLED = 22

    const val IMAGE_SOURCE = 30
    const val CONTENT_MODE = 31
    const val TINT_COLOR = 32

    const val HORIZONTAL = 40
    const val CONTENT_SIZE = 41
    const val REFRESHING = 42
    const val SHOWS_INDICATORS = 43
    const val REFRESH_ENABLED = 44
    const val PAGING = 45

    const val TEXT_VALUE = 50
    const val PLACEHOLDER = 51
    const val MULTILINE = 52
    const val SECURE = 53
    const val KEYBOARD_TYPE = 54
    const val MAX_LENGTH = 55
    const val ALLOWED_CHARS = 56
    const val EDITABLE = 57
    const val RETURN_KEY = 58
    const val TEXT_STYLE = 59
    const val CONTENT_TYPE = 60
    const val PLACEHOLDER_COLOR = 61
    const val AUTO_CAPITALIZE = 62
    /** F32 array `[top, right, bottom, left]`: the field's padding, applied as text insets. */
    const val CONTENT_INSETS = 63

    const val CHECKED = 70
    const val ENABLED = 71
    const val CHECK_STYLE = 72

    const val VALUE = 80
    const val MIN = 81
    const val MAX = 82
    const val STEP = 83
    const val INDETERMINATE = 84

    const val ITEM_COUNT = 90
    const val ROW_OFFSETS = 91
    const val ROW_INDEX = 92
    const val PREFETCH = 93
    /** I32, GPUI's `ListAlignment`: 0 top, 1 bottom (opens at the end, keeps the end in view). */
    const val LIST_ALIGNMENT = 94

    const val TITLE = 100
    const val CAN_GO_BACK = 101
    const val NAV_BAR_HIDDEN = 102
    const val DETENTS = 103
    const val DISMISSABLE = 104
    const val ALERT_SPEC = 105

    const val ITEMS = 110
    const val SELECTED_INDEX = 111
    const val DATE = 112
    const val DATE_MODE = 113
    const val MENU = 114

    const val PATH_DATA = 120
    const val FILL_COLOR = 121
    const val STROKE_COLOR = 122
    const val STROKE_WIDTH = 123
    const val VIEW_BOX = 124
    /** I32: 0 nonzero (default), 1 even-odd. */
    const val FILL_RULE = 125

    const val A11Y_LABEL = 130
    const val A11Y_ROLE = 131
    const val A11Y_HINT = 132
    const val A11Y_VALUE = 133
    const val A11Y_STATE = 134
    const val TEST_ID = 135
    const val A11Y_HIDDEN = 136

    const val ACTIVE_OPACITY = 140
    const val ACTIVE_BACKGROUND = 141
    const val HIT_SLOP = 142
    const val FOCUSABLE = 143
    const val HOVER_OPACITY = 144
    const val HOVER_BACKGROUND = 145
    /** I32: GPUI's `CursorStyle`. */
    const val CURSOR = 146
    const val TOOLTIP = 147

    const val HOST_COMPONENT = 150
    const val HOST_PROPS = 151
    /** JSON on ROOT_NODE: every resolved color token (ARGB ints) plus `appearance`. */
    const val THEME_TOKENS = 152

    /** Highest prop id. */
    const val LAST = 152

    private val known: BooleanArray = BooleanArray(LAST + 1).also { k ->
        intArrayOf(
            1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 20, 21, 22, 30, 31, 32, 40, 41, 42, 43, 44,
            50, 51, 52, 53, 54, 55, 56, 57, 58, 59, 60, 61, 62, 63, 70, 71, 72, 80, 81, 82, 83, 84,
            90, 91, 92, 93, 94, 100, 101, 102, 103, 104, 105, 110, 111, 112, 113, 114,
            120, 121, 122, 123, 124, 125, 130, 131, 132, 133, 134, 135, 136, 140, 141, 142, 143, 144, 145, 146, 147, 150, 151, 152,
        ).forEach { k[it] = true }
    }

    fun isValid(p: Int) = p in 0..LAST && known[p]
}

object A11yRole {
    const val NONE = 0
    const val BUTTON = 1
    const val LINK = 2
    const val HEADER = 3
    const val IMAGE = 4
    const val TEXT = 5
    const val TEXT_FIELD = 6
    const val SWITCH = 7
    const val CHECKBOX = 8
    const val RADIO = 9
    const val SLIDER = 10
    const val PROGRESS_BAR = 11
    const val TAB = 12
    const val TAB_LIST = 13
    const val LIST = 14
    const val LIST_ITEM = 15
    const val MENU = 16
    const val ALERT = 17
    const val SEARCH = 18
}

object Command {
    const val FOCUS = 1
    const val BLUR = 2
    const val SCROLL_TO = 3
    const val SCROLL_TO_INDEX = 4
    const val SHOW_TOAST = 5
    const val OPEN_URL = 6
    const val SET_CLIPBOARD = 7
    const val PRESENT_PAGE = 8
    const val DISMISS_PAGE = 9
    const val HAPTIC = 10
    /** Payload `{id}` retracts one toast, `{}` all of them. */
    const val DISMISS_TOAST = 11

    fun isValid(c: Int) = c in FOCUS..DISMISS_TOAST
}

object EventMask {
    const val CLICK = 1 shl 0
    const val DOUBLE_CLICK = 1 shl 1
    const val LONG_PRESS = 1 shl 2
    const val CHANGE = 1 shl 3
    const val SUBMIT = 1 shl 4
    const val FOCUS = 1 shl 5
    const val BLUR = 1 shl 6
    const val SCROLL = 1 shl 7
    const val REFRESH = 1 shl 8
    const val PAN = 1 shl 9
    const val PINCH = 1 shl 10
    const val LINK = 1 shl 11
    const val KEY = 1 shl 12
    const val DISMISS = 1 shl 13
    const val ACTION = 1 shl 14
    const val PRESS_IN = 1 shl 15
    const val PRESS_OUT = 1 shl 16
    const val HOST_EVENT = 1 shl 17
    /** Pointer platforms: entry and exit of the pointer. */
    const val HOVER = 1 shl 18

    /** Bits that need a touch listener on a plain view. */
    const val TOUCH = CLICK or DOUBLE_CLICK or LONG_PRESS or PAN or PINCH or PRESS_IN or PRESS_OUT
}

object EventType {
    const val CLICK = 1
    const val DOUBLE_CLICK = 2
    const val LONG_PRESS = 3
    const val CHANGE = 4
    const val SUBMIT = 5
    const val FOCUS = 6
    const val BLUR = 7
    const val SCROLL = 8
    const val REFRESH = 9
    const val PAN = 10
    const val PINCH = 11
    const val LINK = 12
    const val KEY = 13
    const val DISMISS = 14
    const val ACTION = 15
    const val RANGE_CHANGE = 16
    const val ROW_HEIGHT = 17
    const val BACK_PRESSED = 18
    const val PRESS_IN = 19
    const val PRESS_OUT = 20
    const val HOST_EVENT = 21
    /** `{id}` on the root: a toast's action button was tapped. */
    const val TOAST_ACTION = 22
    /** `{entered, x, y}`: the pointer entered or left the node. */
    const val HOVER = 23
}
