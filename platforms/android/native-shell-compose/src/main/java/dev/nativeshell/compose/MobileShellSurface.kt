package dev.nativeshell.compose

import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.viewinterop.AndroidView
import dev.nativeshell.ShellInstance
import dev.nativeshell.ShellSurfaceView

/**
 * Shows [instance] inside a Compose hierarchy. The shell renders with Android
 * views; this only hosts its [ShellSurfaceView]. The instance outlives the
 * composable: create and destroy it where its lifetime belongs (a ViewModel,
 * the activity), and the surface detaches when it leaves composition.
 */
@Composable
fun NativeShellSurface(instance: ShellInstance, modifier: Modifier = Modifier) {
    AndroidView(
        factory = { context -> ShellSurfaceView(context).apply { attach(instance) } },
        modifier = modifier,
        update = { view -> if (view.instance !== instance) view.attach(instance) },
        onRelease = { view -> view.detach() },
    )
}
