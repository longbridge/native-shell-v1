package dev.nativeshell.protocol

import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Assert.fail
import org.junit.Test
import java.nio.ByteBuffer
import java.nio.ByteOrder

class BatchDecoderTest {

    /** The same instructions as `protocol::tests::sample` in crates/core/src/protocol.rs. */
    private fun sample(): List<Mutation> = listOf(
        Mutation.BeginBatch(7, BatchFlags.INITIAL),
        Mutation.Create(1, ViewType.VIEW),
        Mutation.SetProp(1, Prop.BACKGROUND_COLOR, PropValue.color(0xFF112233.toInt())),
        Mutation.SetProp(1, Prop.CORNER_RADII, PropValue.floats(floatArrayOf(4f, 4f, 0f, 0f))),
        Mutation.SetProp(1, Prop.A11Y_LABEL, PropValue.str("héllo 😀")),
        Mutation.SetProp(1, Prop.TEST_ID, PropValue.str("héllo 😀")),
        Mutation.SetProp(1, Prop.OPACITY, PropValue.NULL),
        Mutation.SetProp(1, Prop.MAX_LENGTH, PropValue.i32(-3)),
        Mutation.SetProp(1, Prop.HIDDEN, PropValue.bool(true)),
        Mutation.SetProp(1, Prop.VALUE, PropValue.f32(0.5f)),
        Mutation.SetProp(1, Prop.ATTRIBUTED_TEXT, PropValue.json("{\"runs\":[]}")),
        Mutation.Insert(1, Protocol.ROOT_NODE, 0),
        Mutation.Create(0x0100_0000, ViewType.TEXT),
        Mutation.Insert(0x0100_0000, 1, 0),
        Mutation.Move(2, 1, 3, 0),
        Mutation.Remove(2, 1, 0),
        Mutation.SetFrame(1, 0f, 1.5f, 320f, 44.25f),
        Mutation.SetEventMask(1, EventMask.CLICK or EventMask.LONG_PRESS),
        Mutation.Command(0, Command.SHOW_TOAST, "{\"title\":\"hi\"}"),
        Mutation.Delete(2),
        Mutation.EndBatch,
    )

    /** `encode(&sample())` printed by the Rust encoder (see the android README for how it was produced). */
    private val rustFixture = hex(
        "4e534231010000003c000000030000000100000001000000070000000200000003010000010000000801000001000400" +
            "332211ff08010000020007000000000008010000820005000000000008010000870005000000000008010000050000000801" +
            "000037000200fdffffff08010000090001000100000008010000500003000000003f08010000140006000100000004010000" +
            "00000000000000008300000000000001020000008400000000000001010000000000000007020000010000000300000000" +
            "00000005020000010000000000000009010000000000000000c03f0000a043000031420a010000050000000b0000000500" +
            "00000200000006020000020000000b00000068c3a96c6c6f20f09f9880000b0000007b2272756e73223a5b5d7d000e0000" +
            "007b227469746c65223a226869227d00000400000000008040000080400000000000000000",
    )

    @Test
    fun decodesTheRustEncoderOutput() {
        val batch = BatchDecoder.decode(rustFixture)
        assertEquals(7, batch.seq)
        assertTrue(batch.isInitial)
        assertEquals(sample(), batch.mutations)
    }

    @Test
    fun encoderMatchesTheRustEncoderByteForByte() {
        assertArrayEquals(rustFixture, BatchEncoder.encode(sample()))
    }

    @Test
    fun roundTripsEveryInstruction() {
        val bytes = BatchEncoder.encode(sample())
        assertEquals(0, bytes.size % 4)
        assertEquals(sample(), BatchDecoder.decode(bytes).mutations)
    }

    @Test
    fun decodesFromDirectLittleEndianBuffers() {
        val direct = ByteBuffer.allocateDirect(rustFixture.size).order(ByteOrder.BIG_ENDIAN)
        direct.put(rustFixture)
        direct.flip()
        assertEquals(sample(), BatchDecoder.decode(direct).mutations)
    }

    @Test
    fun stringsAreInternedOnce() {
        val bytes = BatchEncoder.encode(sample())
        val words = ByteBuffer.wrap(bytes).order(ByteOrder.LITTLE_ENDIAN)
        assertEquals(3, words.getInt(12))
    }

    @Test
    fun extendedNodeIdsCostOneWord() {
        val small = BatchEncoder.encode(listOf(Mutation.Delete(5)))
        val big = BatchEncoder.encode(listOf(Mutation.Delete(0x0100_0000)))
        assertEquals(small.size + 4, big.size)
        assertEquals(Mutation.Delete(0x0100_0000), BatchDecoder.decode(big).mutations.single())
    }

    @Test
    fun hostileBuffersAreRejectedWithoutCrashing() {
        val good = BatchEncoder.encode(sample())
        for (cut in good.indices) expectRejectedOrValid(good.copyOf(cut))
        val bad = good.clone()
        bad[20] = 0xEE.toByte()
        expectRejected(bad)
        var seed = 0x12345678
        repeat(5000) {
            val buf = good.clone()
            repeat(4) {
                seed = seed xor (seed shl 13)
                seed = seed xor (seed ushr 17)
                seed = seed xor (seed shl 5)
                val at = Protocol.HEADER_WORDS * 4 + Math.floorMod(seed, buf.size - Protocol.HEADER_WORDS * 4)
                buf[at] = (seed ushr 8).toByte()
            }
            expectRejectedOrValid(buf)
        }
        expectRejected(byteArrayOf(1, 2, 3))
    }

    @Test
    fun rejectsBadHeaders() {
        val good = BatchEncoder.encode(sample())
        val wrongVersion = good.clone().also { it[4] = 2 }
        expectRejected(wrongVersion)
        val hugeCount = good.clone().also { it[8] = 0xFF.toByte(); it[9] = 0xFF.toByte(); it[10] = 0xFF.toByte(); it[11] = 0x7F }
        expectRejected(hugeCount)
        val hugeStrings = good.clone().also { it[15] = 0x7F }
        expectRejected(hugeStrings)
    }

    @Test
    fun rejectsInvalidUtf8() {
        val bytes = BatchEncoder.encode(listOf(Mutation.SetProp(1, Prop.TITLE, PropValue.str("abcd"))))
        // The string bytes are the last word: make them an invalid UTF-8 sequence.
        bytes[bytes.size - 4] = 0xC3.toByte()
        bytes[bytes.size - 3] = 0x28
        expectRejected(bytes)
    }

    @Test
    fun rejectsUnknownEnumValues() {
        expectRejected(BatchEncoder.encode(listOf(Mutation.Create(1, 99))))
        expectRejected(BatchEncoder.encode(listOf(Mutation.Command(1, 77, ""))))
        expectRejected(BatchEncoder.encode(listOf(Mutation.SetProp(1, 12, PropValue.NULL))))
    }

    private fun expectRejected(bytes: ByteArray) {
        try {
            BatchDecoder.decode(bytes)
            fail("expected rejection")
        } catch (e: DecodeException) {
            // expected
        }
    }

    private fun expectRejectedOrValid(bytes: ByteArray) {
        try {
            BatchDecoder.decode(bytes)
        } catch (e: DecodeException) {
            // fine
        }
    }

    companion object {
        fun hex(s: String): ByteArray = ByteArray(s.length / 2) { s.substring(it * 2, it * 2 + 2).toInt(16).toByte() }
    }
}
