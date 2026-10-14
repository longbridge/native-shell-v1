package dev.nativeshell.protocol

/**
 * A property value as decoded from the stream. One class for every kind keeps
 * prop setters free of casts: a setter reads the field its kind fills.
 */
class PropValue private constructor(
    @JvmField val kind: Int,
    @JvmField val int: Int = 0,
    @JvmField val float: Float = 0f,
    @JvmField val string: String? = null,
    @JvmField val floats: FloatArray? = null,
) {
    val isNull: Boolean get() = kind == ValueKind.NULL
    val bool: Boolean get() = int != 0

    /** Numeric reading of I32 / F32 / Bool, for props that accept either. */
    fun number(): Float = when (kind) {
        ValueKind.F32 -> float
        ValueKind.I32, ValueKind.BOOL, ValueKind.COLOR -> int.toFloat()
        else -> 0f
    }

    fun intValue(): Int = when (kind) {
        ValueKind.F32 -> float.toInt()
        else -> int
    }

    override fun equals(other: Any?): Boolean = other is PropValue && other.kind == kind && other.int == int &&
        other.float.toRawBits() == float.toRawBits() && other.string == string &&
        (other.floats?.contentEquals(floats) ?: (floats == null))

    override fun hashCode(): Int = kind * 31 + int + (string?.hashCode() ?: 0)

    override fun toString(): String = when (kind) {
        ValueKind.NULL -> "Null"
        ValueKind.BOOL -> "Bool($bool)"
        ValueKind.I32 -> "I32($int)"
        ValueKind.F32 -> "F32($float)"
        ValueKind.COLOR -> "Color(0x${Integer.toHexString(int)})"
        ValueKind.STR -> "Str($string)"
        ValueKind.JSON -> "Json($string)"
        else -> "F32Array(${floats?.contentToString()})"
    }

    companion object {
        @JvmField val NULL = PropValue(ValueKind.NULL)
        @JvmStatic fun bool(b: Boolean) = PropValue(ValueKind.BOOL, int = if (b) 1 else 0)
        @JvmStatic fun i32(i: Int) = PropValue(ValueKind.I32, int = i)
        @JvmStatic fun f32(f: Float) = PropValue(ValueKind.F32, float = f)
        @JvmStatic fun color(argb: Int) = PropValue(ValueKind.COLOR, int = argb)
        @JvmStatic fun str(s: String) = PropValue(ValueKind.STR, string = s)
        @JvmStatic fun json(s: String) = PropValue(ValueKind.JSON, string = s)
        @JvmStatic fun floats(a: FloatArray) = PropValue(ValueKind.F32_ARRAY, floats = a)
    }
}

/** One decoded instruction. Mirrors `Mutation` in protocol.rs so tests compare structurally. */
sealed class Mutation {
    data class BeginBatch(val seq: Int, val flags: Int) : Mutation()
    data object EndBatch : Mutation()
    data class Create(val node: Int, val viewType: Int) : Mutation()
    data class Insert(val node: Int, val parent: Int, val index: Int) : Mutation()
    data class Remove(val node: Int, val parent: Int, val index: Int) : Mutation()
    data class Delete(val node: Int) : Mutation()
    data class Move(val node: Int, val parent: Int, val from: Int, val to: Int) : Mutation()
    data class SetProp(val node: Int, val prop: Int, val value: PropValue) : Mutation()
    data class SetFrame(val node: Int, val x: Float, val y: Float, val width: Float, val height: Float) : Mutation()
    data class SetEventMask(val node: Int, val mask: Int) : Mutation()
    data class Command(val node: Int, val command: Int, val payload: String) : Mutation()
}

/** A decoded batch: the instructions between `BeginBatch` and `EndBatch` plus the batch header. */
class Batch(val seq: Int, val flags: Int, val mutations: List<Mutation>) {
    val isAnimated: Boolean get() = flags and BatchFlags.ANIMATED != 0
    val isInitial: Boolean get() = flags and BatchFlags.INITIAL != 0
}
