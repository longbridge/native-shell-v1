package dev.nativeshell

import android.util.Base64
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config

/** Program files bundled in the APK reach the engine: text as is, binaries as `data:` URLs. */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [34])
class ProgramFilesTest {
    @Test
    fun textAndBinaryFiles() {
        assertEquals("export default 1;", ShellInstance.programFile("main.js", "export default 1;".toByteArray()))
        assertEquals("<svg/>", ShellInstance.programFile("icons/x.svg", "<svg/>".toByteArray()))
        val jpeg = byteArrayOf(0xFF.toByte(), 0xD8.toByte(), 0xFF.toByte(), 0xE0.toByte(), 0, 1, 2, 3)
        val url = ShellInstance.programFile("assets/ridge-route.JPG", jpeg)
        assertTrue(url, url.startsWith("data:image/jpeg;base64,"))
        assertTrue(Base64.decode(url.substringAfter(";base64,"), Base64.DEFAULT).contentEquals(jpeg))
        assertTrue(ShellInstance.programFile("a.png", byteArrayOf(1)).startsWith("data:image/png;base64,"))
        assertTrue(ShellInstance.programFile("blob.bin", byteArrayOf(1)).startsWith("data:application/octet-stream;base64,"))
    }
}
