package dev.nativeshell.protocol

import java.nio.ByteBuffer
import java.nio.ByteOrder

/**
 * Builds a batch buffer exactly as `protocol::Encoder` does (string interning
 * included). The core is the only producer in production; this exists so tests
 * and tools can drive the renderer without the native library.
 */
class BatchEncoder {
    private val words = IntArrayList()
    private val strings = ArrayList<String>()
    private val stringIndex = HashMap<String, Int>()
    private val arrays = ArrayList<FloatArray>()

    private fun header(op: Int, node: Int) {
        if (node.toLong() and 0xFFFFFFFFL > Protocol.NODE_MASK) {
            words.add(op or Protocol.EXTENDED_NODE)
            words.add(node)
        } else {
            words.add(op or (node shl 8))
        }
    }

    private fun intern(s: String): Int = stringIndex.getOrPut(s) { strings.add(s); strings.size - 1 }

    fun push(m: Mutation): BatchEncoder {
        when (m) {
            is Mutation.BeginBatch -> { header(Op.BEGIN_BATCH, 0); words.add(m.seq); words.add(m.flags) }
            Mutation.EndBatch -> header(Op.END_BATCH, 0)
            is Mutation.Create -> { header(Op.CREATE, m.node); words.add(m.viewType) }
            is Mutation.Insert -> { header(Op.INSERT, m.node); words.add(m.parent); words.add(m.index) }
            is Mutation.Remove -> { header(Op.REMOVE, m.node); words.add(m.parent); words.add(m.index) }
            is Mutation.Delete -> header(Op.DELETE, m.node)
            is Mutation.Move -> { header(Op.MOVE, m.node); words.add(m.parent); words.add(m.from); words.add(m.to) }
            is Mutation.SetProp -> {
                header(Op.SET_PROP, m.node)
                val v = m.value
                words.add(m.prop or (v.kind shl 16))
                when (v.kind) {
                    ValueKind.NULL -> {}
                    ValueKind.BOOL, ValueKind.I32, ValueKind.COLOR -> words.add(v.int)
                    ValueKind.F32 -> words.add(v.float.toRawBits())
                    ValueKind.STR, ValueKind.JSON -> words.add(intern(v.string!!))
                    ValueKind.F32_ARRAY -> { words.add(arrays.size); arrays.add(v.floats!!) }
                }
            }
            is Mutation.SetFrame -> {
                header(Op.SET_FRAME, m.node)
                words.add(m.x.toRawBits()); words.add(m.y.toRawBits())
                words.add(m.width.toRawBits()); words.add(m.height.toRawBits())
            }
            is Mutation.SetEventMask -> { header(Op.SET_EVENT_MASK, m.node); words.add(m.mask) }
            is Mutation.Command -> {
                header(Op.COMMAND, m.node)
                val i = intern(m.payload)
                words.add(m.command); words.add(i)
            }
        }
        return this
    }

    fun pushAll(ms: Iterable<Mutation>): BatchEncoder { ms.forEach { push(it) }; return this }

    fun finish(): ByteArray {
        val out = IntArrayList()
        out.add(Protocol.MAGIC); out.add(Protocol.VERSION); out.add(words.size); out.add(strings.size); out.add(arrays.size)
        for (k in 0 until words.size) out.add(words[k])
        for (s in strings) {
            val bytes = s.toByteArray(Charsets.UTF_8)
            out.add(bytes.size)
            var k = 0
            while (k < bytes.size) {
                var word = 0
                for (b in 0 until 4) {
                    if (k + b < bytes.size) word = word or ((bytes[k + b].toInt() and 0xFF) shl (8 * b))
                }
                out.add(word)
                k += 4
            }
        }
        for (a in arrays) {
            out.add(a.size)
            a.forEach { out.add(it.toRawBits()) }
        }
        val bb = ByteBuffer.allocate(out.size * 4).order(ByteOrder.LITTLE_ENDIAN)
        for (k in 0 until out.size) bb.putInt(out[k])
        return bb.array()
    }

    companion object {
        @JvmStatic fun encode(ms: List<Mutation>): ByteArray = BatchEncoder().pushAll(ms).finish()
    }
}

internal class IntArrayList {
    private var data = IntArray(64)
    var size = 0
        private set

    fun add(v: Int) {
        if (size == data.size) data = data.copyOf(size * 2)
        data[size++] = v
    }

    operator fun get(i: Int) = data[i]
}
