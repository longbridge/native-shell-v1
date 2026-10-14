package dev.nativeshell.protocol

import org.junit.Assert.assertEquals
import org.junit.Test

class Utf8Test {
    private fun strict(b: ByteArray): Boolean = try {
        Charsets.UTF_8.newDecoder().decode(java.nio.ByteBuffer.wrap(b)); true
    } catch (e: java.nio.charset.CharacterCodingException) { false }

    @Test
    fun agreesWithTheStrictJdkDecoder() {
        val samples = listOf(
            "héllo 😀 中文 ✓".toByteArray(), byteArrayOf(), byteArrayOf(0xC3.toByte(), 0x28), byteArrayOf(0xC0.toByte(), 0x80.toByte()),
            byteArrayOf(0xED.toByte(), 0xA0.toByte(), 0x80.toByte()), byteArrayOf(0xF4.toByte(), 0x90.toByte(), 0x80.toByte(), 0x80.toByte()),
            byteArrayOf(0xE2.toByte(), 0x82.toByte()), byteArrayOf(0xF0.toByte(), 0x9F.toByte(), 0x98.toByte()),
        )
        for (s in samples) assertEquals(s.contentToString(), strict(s), Utf8.isValid(s, 0, s.size))
        var seed = 42L
        repeat(20000) {
            val b = ByteArray((seed % 7).toInt().let { if (it < 0) -it else it } + 1) {
                seed = seed * 6364136223846793005L + 1442695040888963407L
                (seed ushr 33).toByte()
            }
            assertEquals(b.contentToString(), strict(b), Utf8.isValid(b, 0, b.size))
        }
    }
}
