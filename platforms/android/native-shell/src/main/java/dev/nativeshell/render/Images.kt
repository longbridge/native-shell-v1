package dev.nativeshell.render

import android.content.Context
import android.graphics.Bitmap
import android.graphics.BitmapFactory
import android.os.Handler
import android.os.Looper
import android.util.LruCache
import android.widget.ImageView
import dev.nativeshell.ImageLoader
import dev.nativeshell.NativeShell
import dev.nativeshell.R
import java.io.File
import java.io.InputStream
import java.net.HttpURLConnection
import java.net.URL
import java.util.concurrent.ExecutorService
import java.util.concurrent.Executors

/** Turns an `ImageSource` into pixels in an [ImageView]. */
internal interface ImageResolver {
    fun load(source: String, view: ImageView)
    fun cancel(view: ImageView)
}

/**
 * Resolves image sources for one instance: `http(s)` URLs go to
 * [NativeShell.imageLoader]; `file://` and absolute paths are read from disk;
 * `asset://` from the APK; anything else is relative to the program's
 * resource base (its directory or asset folder).
 */
internal class DefaultImageResolver(private val context: Context, private val resourceBase: String?) : ImageResolver {
    override fun load(source: String, view: ImageView) {
        view.setTag(R.id.nsh_image_source, source)
        when {
            source.startsWith("http://") || source.startsWith("https://") -> NativeShell.imageLoader.load(source, view)
            else -> LocalImages.load(context, resolve(source), view, source)
        }
    }

    override fun cancel(view: ImageView) {
        view.setTag(R.id.nsh_image_source, null)
        view.setImageDrawable(null)
    }

    private fun resolve(source: String): String {
        if (source.startsWith("file://") || source.startsWith("asset://") || source.startsWith("/")) return source
        val base = resourceBase ?: return source
        val clean = source.removePrefix("./")
        if (clean.split('/').any { it == ".." }) return ""
        return if (base.endsWith("/")) base + clean else "$base/$clean"
    }
}

private object LocalImages {
    private val main = Handler(Looper.getMainLooper())

    fun load(context: Context, path: String, view: ImageView, tag: String) {
        HttpImageLoader.cache.get(path)?.let { view.setImageBitmap(it); return }
        HttpImageLoader.executor.execute {
            val bmp = try {
                when {
                    path.startsWith("asset://") -> context.assets.open(path.removePrefix("asset://").trimStart('/')).use(::decode)
                    path.startsWith("file://") -> File(path.removePrefix("file://")).inputStream().use(::decode)
                    path.startsWith("/") -> File(path).inputStream().use(::decode)
                    else -> null
                }
            } catch (e: Exception) {
                null
            }
            if (bmp != null) HttpImageLoader.cache.put(path, bmp)
            main.post { if (view.getTag(R.id.nsh_image_source) == tag && bmp != null) view.setImageBitmap(bmp) }
        }
    }

    private fun decode(s: InputStream): Bitmap? = BitmapFactory.decodeStream(s)
}

/** Default [ImageLoader]: HttpURLConnection on a small pool, decoded bitmaps in an [LruCache]. */
internal object HttpImageLoader : ImageLoader {
    val executor: ExecutorService = Executors.newFixedThreadPool(3) { r -> Thread(r, "native-shell-images").apply { isDaemon = true } }
    val cache = object : LruCache<String, Bitmap>((Runtime.getRuntime().maxMemory() / 8 / 1024).toInt()) {
        override fun sizeOf(key: String, value: Bitmap): Int = value.byteCount / 1024
    }
    private val main = Handler(Looper.getMainLooper())

    override fun load(url: String, imageView: ImageView) {
        imageView.setTag(R.id.nsh_image_source, url)
        cache.get(url)?.let { imageView.setImageBitmap(it); return }
        imageView.setImageDrawable(null)
        executor.execute {
            val bmp = try {
                val c = URL(url).openConnection() as HttpURLConnection
                c.connectTimeout = 15_000
                c.readTimeout = 30_000
                c.instanceFollowRedirects = true
                try {
                    if (c.responseCode in 200..299) c.inputStream.use { BitmapFactory.decodeStream(it) } else null
                } finally {
                    c.disconnect()
                }
            } catch (e: Exception) {
                null
            }
            if (bmp != null) cache.put(url, bmp)
            main.post { if (imageView.getTag(R.id.nsh_image_source) == url && bmp != null) imageView.setImageBitmap(bmp) }
        }
    }
}
