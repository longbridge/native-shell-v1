package dev.nativeshell

import android.content.Context
import android.os.SystemClock
import android.view.View
import dev.nativeshell.protocol.Batch
import dev.nativeshell.protocol.BatchDecoder
import dev.nativeshell.protocol.DecodeException
import dev.nativeshell.render.DefaultImageResolver
import dev.nativeshell.render.ImageResolver
import dev.nativeshell.render.MountHost
import dev.nativeshell.render.MountingManager
import dev.nativeshell.render.ShellViewGroup
import dev.nativeshell.render.SurfacePresenter
import java.nio.ByteBuffer

/**
 * Connects one instance to one surface view: decodes batches, applies them
 * through the [MountingManager] and routes events back with the applied seq.
 */
internal class SurfaceRenderer(
    private val surfaceView: ShellSurfaceView,
    themed: Context,
    rootView: ShellViewGroup,
    override val presenter: SurfacePresenter,
    private val sink: EventSinkTarget,
    resourceBase: String?,
) : MountHost {

    /** Where events go: the instance in production, a recorder in tests. */
    interface EventSinkTarget {
        fun dispatch(node: Int, type: Int, payload: String, seq: Int)
        fun log(level: Int, message: String)
        fun detached() {}
    }

    override val context: Context = themed
    override val density: Float = themed.resources.displayMetrics.density
    override val images: ImageResolver = DefaultImageResolver(themed, resourceBase)
    override val anchor: View get() = surfaceView
    override val toasts get() = surfaceView.toastStack

    val mounting = MountingManager(this, rootView)

    /** True until the first batch is applied; the surface holds its first draw meanwhile. */
    var awaitingFirstBatch = true
        private set

    /** Wall-clock ms spent applying the last batch (decode + mount), for diagnostics. */
    var lastApplyMs = 0.0
        private set
    var lastDecodeMs = 0.0
        private set

    override fun dispatch(node: Int, eventType: Int, payload: String) {
        sink.dispatch(node, eventType, payload, mounting.lastSeq)
    }

    override fun log(level: Int, message: String) = sink.log(level, message)

    fun applyBuffer(buf: ByteBuffer) {
        val t0 = SystemClock.elapsedRealtimeNanos()
        val batch: Batch = try {
            BatchDecoder.decode(buf)
        } catch (e: DecodeException) {
            sink.log(LogLevel.ERROR, "rejected batch: ${e.message}")
            return
        }
        val t1 = SystemClock.elapsedRealtimeNanos()
        apply(batch)
        lastDecodeMs = (t1 - t0) / 1e6
        lastApplyMs = (SystemClock.elapsedRealtimeNanos() - t0) / 1e6
    }

    fun apply(batch: Batch) {
        if (NativeShell.traceBatches) {
            sink.log(LogLevel.DEBUG, "batch seq=${batch.seq} flags=${batch.flags} (${batch.mutations.size} mutations)")
            for (m in batch.mutations) sink.log(LogLevel.DEBUG, "  $m")
        }
        mounting.apply(batch)
        if (awaitingFirstBatch) {
            awaitingFirstBatch = false
            surfaceView.invalidate()
        }
    }

    fun detachInstance() {
        sink.detached()
    }
}
