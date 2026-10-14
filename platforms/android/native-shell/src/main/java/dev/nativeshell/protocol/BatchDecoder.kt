package dev.nativeshell.protocol

import java.nio.ByteBuffer
import java.nio.ByteOrder

/** Why a buffer was rejected. The whole batch is dropped; nothing of it is applied. */
class DecodeException(message: String) : Exception(message)

/**
 * Validating decoder of one batch buffer, the Kotlin twin of `protocol::decode`.
 *
 * It validates everything the wire format can get wrong (alignment, header,
 * table bounds, opcodes, enum values, UTF-8) before returning, so the mounting
 * code never sees a half-valid batch. It never throws anything but
 * [DecodeException].
 */
object BatchDecoder {

    @JvmStatic
    fun decode(bytes: ByteArray): Batch = decode(ByteBuffer.wrap(bytes))

    @JvmStatic
    fun decode(buffer: ByteBuffer): Batch {
        try {
            return decodeUnchecked(buffer)
        } catch (e: DecodeException) {
            throw e
        } catch (e: RuntimeException) {
            // Defensive: any index mistake below is a bad buffer, not a crash.
            throw DecodeException("malformed batch: ${e.javaClass.simpleName}: ${e.message}")
        }
    }

    private fun decodeUnchecked(source: ByteBuffer): Batch {
        val buf = source.duplicate().order(ByteOrder.LITTLE_ENDIAN)
        buf.position(0)
        val byteLen = buf.limit()
        if (byteLen % 4 != 0) throw DecodeException("buffer is not a whole number of words")
        val words = byteLen / 4
        if (words < Protocol.HEADER_WORDS) throw DecodeException("truncated at word $words")
        // One bulk copy: the loops below index a plain int array.
        val ws = IntArray(words)
        buf.asIntBuffer().get(ws)
        if (ws[0] != Protocol.MAGIC || ws[1] != Protocol.VERSION) {
            throw DecodeException("bad magic or unsupported protocol version ${ws[1]}")
        }
        val n = ws[2].toLong() and 0xFFFFFFFFL
        val stringCount = ws[3].toLong() and 0xFFFFFFFFL
        val arrayCount = ws[4].toLong() and 0xFFFFFFFFL
        val bodyEnd = Protocol.HEADER_WORDS + n
        if (bodyEnd > words) throw DecodeException("truncated at word $words")
        val end = bodyEnd.toInt()

        // String table. Bytes are validated as UTF-8 before decoding (JNI strings are never "modified UTF-8").
        var at = end
        if (stringCount > words) throw DecodeException("string count $stringCount out of range")
        val strings = arrayOfNulls<String>(stringCount.toInt())
        var bytes: ByteArray? = null
        var bytesBase = 0
        for (s in strings.indices) {
            if (at >= words) throw DecodeException("truncated at word $at")
            val len = ws[at].toLong() and 0xFFFFFFFFL
            at += 1
            val wc = (len + 3) / 4
            if (at + wc > words) throw DecodeException("truncated at word $at")
            if (bytes == null) {
                // Copy the tables once (direct buffers have no backing array).
                bytesBase = at * 4
                bytes = ByteArray(byteLen - bytesBase)
                val b = buf.duplicate()
                b.position(bytesBase)
                b.get(bytes)
            }
            val off = at * 4 - bytesBase
            val l = len.toInt()
            if (!Utf8.isValid(bytes, off, l)) throw DecodeException("string table entry is not UTF-8")
            strings[s] = String(bytes, off, l, Charsets.UTF_8)
            at += wc.toInt()
        }

        // Float-array table.
        if (arrayCount > words) throw DecodeException("array count $arrayCount out of range")
        val arrays = arrayOfNulls<FloatArray>(arrayCount.toInt())
        for (a in arrays.indices) {
            if (at >= words) throw DecodeException("truncated at word $at")
            val len = ws[at].toLong() and 0xFFFFFFFFL
            at += 1
            if (at + len > words) throw DecodeException("truncated at word $at")
            val arr = FloatArray(len.toInt())
            for (k in arr.indices) arr[k] = Float.fromBits(ws[at + k])
            arrays[a] = arr
            at += len.toInt()
        }

        // Instructions.
        val out = ArrayList<Mutation>(maxOf(16, (end - Protocol.HEADER_WORDS) / 3))
        var i = Protocol.HEADER_WORDS
        var seq = 0
        var flags = 0
        fun take(): Int {
            if (i >= end) throw DecodeException("truncated at word $i")
            return ws[i++]
        }
        fun string(idx: Int): String {
            if (idx < 0 || idx >= strings.size) throw DecodeException("table index ${idx.toUInt()} out of range")
            return strings[idx]!!
        }
        while (i < end) {
            val at0 = i
            val head = take()
            val rawOp = head and 0xFF
            val node = if (rawOp and Protocol.EXTENDED_NODE != 0) take() else head ushr 8
            val op = rawOp and Protocol.EXTENDED_NODE.inv()
            out += when (op) {
                Op.BEGIN_BATCH -> {
                    seq = take(); flags = take()
                    Mutation.BeginBatch(seq, flags)
                }
                Op.END_BATCH -> Mutation.EndBatch
                Op.CREATE -> {
                    val t = take()
                    if (!ViewType.isValid(t)) throw DecodeException("unknown view type value ${t.toUInt()}")
                    Mutation.Create(node, t)
                }
                Op.INSERT -> Mutation.Insert(node, take(), take())
                Op.REMOVE -> Mutation.Remove(node, take(), take())
                Op.DELETE -> Mutation.Delete(node)
                Op.MOVE -> Mutation.Move(node, take(), take(), take())
                Op.SET_PROP -> {
                    val pw = take()
                    val prop = pw and 0xFFFF
                    if (!Prop.isValid(prop)) throw DecodeException("unknown prop value $prop")
                    val kind = (pw ushr 16) and 0xFF
                    val value = when (kind) {
                        ValueKind.NULL -> PropValue.NULL
                        ValueKind.BOOL -> PropValue.bool(take() != 0)
                        ValueKind.I32 -> PropValue.i32(take())
                        ValueKind.F32 -> PropValue.f32(Float.fromBits(take()))
                        ValueKind.COLOR -> PropValue.color(take())
                        ValueKind.STR -> PropValue.str(string(take()))
                        ValueKind.JSON -> PropValue.json(string(take()))
                        ValueKind.F32_ARRAY -> {
                            val idx = take()
                            if (idx < 0 || idx >= arrays.size) {
                                throw DecodeException("table index ${idx.toUInt()} out of range")
                            }
                            PropValue.floats(arrays[idx]!!)
                        }
                        else -> throw DecodeException("unknown value kind value $kind")
                    }
                    Mutation.SetProp(node, prop, value)
                }
                Op.SET_FRAME -> Mutation.SetFrame(
                    node,
                    Float.fromBits(take()), Float.fromBits(take()), Float.fromBits(take()), Float.fromBits(take()),
                )
                Op.SET_EVENT_MASK -> Mutation.SetEventMask(node, take())
                Op.COMMAND -> {
                    val c = take()
                    if (!Command.isValid(c)) throw DecodeException("unknown command value ${c.toUInt()}")
                    Mutation.Command(node, c, string(take()))
                }
                else -> throw DecodeException("unknown opcode $rawOp at word $at0")
            }
        }
        return Batch(seq, flags, out)
    }
}

/** Strict UTF-8 validation (no overlongs, no surrogates, max U+10FFFF), as Rust's `str::from_utf8`. */
internal object Utf8 {
    fun isValid(b: ByteArray, off: Int, len: Int): Boolean {
        var i = off
        val end = off + len
        while (i < end) {
            val c = b[i].toInt() and 0xFF
            if (c < 0x80) { i++; continue }
            val need: Int
            val min: Int
            var cp: Int
            when {
                c in 0xC2..0xDF -> { need = 1; min = 0x80; cp = c and 0x1F }
                c in 0xE0..0xEF -> { need = 2; min = 0x800; cp = c and 0x0F }
                c in 0xF0..0xF4 -> { need = 3; min = 0x10000; cp = c and 0x07 }
                else -> return false
            }
            if (i + need >= end) return false
            for (k in 1..need) {
                val cc = b[i + k].toInt() and 0xFF
                if (cc and 0xC0 != 0x80) return false
                cp = (cp shl 6) or (cc and 0x3F)
            }
            if (cp < min || cp > 0x10FFFF || cp in 0xD800..0xDFFF) return false
            i += need + 1
        }
        return true
    }
}
