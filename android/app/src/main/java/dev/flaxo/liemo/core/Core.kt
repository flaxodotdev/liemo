package dev.flaxo.liemo.core

import android.content.Context
import android.graphics.Bitmap
import android.util.LruCache
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.graphics.asImageBitmap
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import uniffi.liemo_mobile.Engine
import uniffi.liemo_mobile.MediaFrame
import uniffi.liemo_mobile.ToolResult
import java.io.File
import java.nio.ByteBuffer

/**
 * The Rust engine, shared by every screen: the same engine as the iPhone app and the desktop's
 * renderer. Its calls are synchronous and thread-safe (the engine serialises them), so slow ones
 * (rendering, export) run off the main thread.
 */
object Core {
    lateinit var engine: Engine
        private set
    lateinit var host: AndroidMediaHost
        private set
    lateinit var dataDir: File
        private set
    lateinit var cacheDir: File
        private set

    val json = Json { ignoreUnknownKeys = true; coerceInputValues = true; explicitNulls = false }

    fun init(context: Context) {
        if (::engine.isInitialized) return
        val app = context.applicationContext
        dataDir = File(app.filesDir, "Liemo").apply { mkdirs() }
        cacheDir = app.cacheDir
        host = AndroidMediaHost(app)
        engine = Engine(dataDir.path, host)
    }

    /** Where imported photos, videos and sounds are copied. */
    val mediaDir: File get() = File(dataDir, "imports").apply { mkdirs() }

    fun call(tool: String, args: Map<String, Any?> = emptyMap()): ToolResult =
        engine.call(tool, toJson(args).toString())

    inline fun <reified T> decode(tool: String, args: Map<String, Any?> = emptyMap()): T {
        val r = call(tool, args)
        if (!r.ok) throw CoreError(r.text)
        return json.decodeFromString(r.text)
    }

    // Library pictures are drawn by the engine once and kept.
    private val thumbs = LruCache<String, ImageBitmap>(300)
    fun thumb(key: String, make: () -> MediaFrame?): ImageBitmap? {
        thumbs.get(key)?.let { return it }
        val img = make()?.toBitmap()?.asImageBitmap() ?: return null
        thumbs.put(key, img)
        return img
    }
}

class CoreError(message: String) : Exception(message)

/** Plain Kotlin values (maps, lists, numbers, strings, booleans) as JSON for the engine's tools. */
fun toJson(v: Any?): JsonElement = when (v) {
    null -> JsonNull
    is JsonElement -> v
    is String -> JsonPrimitive(v)
    is Number -> JsonPrimitive(v)
    is Boolean -> JsonPrimitive(v)
    is Map<*, *> -> JsonObject(v.entries.associate { (k, x) -> k.toString() to toJson(x) })
    is Iterable<*> -> JsonArray(v.map { toJson(it) })
    is Array<*> -> JsonArray(v.map { toJson(it) })
    else -> JsonPrimitive(v.toString())
}

/**
 * The engine's straight-alpha RGBA as a Bitmap. Android draws premultiplied pictures, so any
 * see-through pixels are premultiplied first (frames of the edit are opaque and skip that).
 */
fun MediaFrame.toBitmap(): Bitmap? {
    val w = width.toInt()
    val h = height.toInt()
    if (w <= 0 || h <= 0 || rgba.size < w * h * 4) return null
    val px = rgba
    var i = 3
    while (i < px.size) {
        val a = px[i].toInt() and 0xff
        if (a != 255) {
            for (c in 1..3) px[i - c] = (((px[i - c].toInt() and 0xff) * a + 127) / 255).toByte()
        }
        i += 4
    }
    val bmp = Bitmap.createBitmap(w, h, Bitmap.Config.ARGB_8888)
    bmp.copyPixelsFromBuffer(ByteBuffer.wrap(px, 0, w * h * 4))
    return bmp
}

fun MediaFrame.toImage(): ImageBitmap? = toBitmap()?.asImageBitmap()

/** 0:03.4 */
fun formatTime(t: Double): String {
    val s = t.coerceAtLeast(0.0)
    val m = s.toInt() / 60
    val sec = s.toInt() % 60
    val f = ((s - kotlin.math.floor(s)) * 10).toInt()
    return "%d:%02d.%d".format(m, sec, f)
}
