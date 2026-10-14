package dev.nativeshell.example

import android.os.Bundle
import android.widget.Toast
import androidx.activity.ComponentActivity
import androidx.activity.enableEdgeToEdge
import dev.nativeshell.ShellConfig
import dev.nativeshell.ShellException
import dev.nativeshell.ShellInstance
import dev.nativeshell.ShellSurfaceView
import java.io.File

/**
 * Runs one bundled example program (assets/<program>/) full screen, edge to
 * edge; the insets reach the script as its safe area. System back pops the
 * program's own pages first, then returns to the launcher.
 */
class ProgramActivity : ComponentActivity() {
    private lateinit var surface: ShellSurfaceView
    private var instance: ShellInstance? = null

    override fun onCreate(savedInstanceState: Bundle?) {
        enableEdgeToEdge()
        super.onCreate(savedInstanceState)
        val program = intent.getStringExtra(EXTRA_PROGRAM)?.takeIf { it in MainActivity.PROGRAMS }
        if (program == null) {
            finish()
            return
        }
        surface = ShellSurfaceView(this)
        setContentView(surface)
        val appId = "dev.nativeshell.example.$program"
        val data = File(filesDir, "programs/$appId")
        if (intent.getBooleanExtra(EXTRA_RESET, false)) data.deleteRecursively()
        instance = try {
            ShellInstance.create(this, ShellConfig().appId(appId).dataDir(data)).also { i ->
                i.errorListener = ShellInstance.ErrorListener { msg -> Toast.makeText(this, msg, Toast.LENGTH_LONG).show() }
                surface.attach(i)
                i.loadSourcesFromAssets(this, program)
            }
        } catch (e: ShellException) {
            Toast.makeText(this, "Couldn’t open $program: ${e.message}", Toast.LENGTH_LONG).show()
            null
        }
    }

    /** The surface the program draws into (screenshot tests drive it). */
    val currentSurface: ShellSurfaceView get() = surface

    override fun onResume() {
        super.onResume()
        instance?.setActive(true)
    }

    override fun onPause() {
        instance?.setActive(false)
        super.onPause()
    }

    override fun onDestroy() {
        if (::surface.isInitialized) surface.detach()
        instance?.destroy()
        super.onDestroy()
    }

    companion object {
        const val EXTRA_PROGRAM = "program"
        /** Starts the program with empty storage (screenshot tests). */
        const val EXTRA_RESET = "reset"
    }
}
