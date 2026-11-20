package dev.flaxo.liemo.core

import android.media.AudioAttributes
import android.media.AudioFormat
import android.media.AudioTrack
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableDoubleStateOf
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateMapOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.runtime.withFrameNanos
import androidx.compose.ui.graphics.ImageBitmap
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import java.io.File
import java.nio.ByteBuffer
import java.nio.ByteOrder

/** What a drag on a selected clip does. Normal moves it and trims its edges; the others are
 *  Resolve's Trim tool edits. */
enum class TrimMode(val label: String, val about: String) {
    Normal("Move & trim", "Drag to move, edges to trim"),
    Ripple("Ripple", "Edges trim and the rest of the track follows"),
    Roll("Roll", "Edges move the cut with the clip beside"),
    Slip("Slip", "Drag to show other footage, in place"),
    Slide("Slide", "Drag between the clips either side"),
}

/**
 * One open project: its state as the engine reports it, the preview frame, playback. Lives on
 * the main thread; engine work runs on background dispatchers and lands back here.
 *
 * `scope` must come from the composition (rememberCoroutineScope), so playback can follow the
 * display's frames.
 */
class EditorModel(val id: String, private val scope: CoroutineScope) {
    var doc by mutableStateOf<ProjectDoc?>(null)
        private set
    private var timeState by mutableDoubleStateOf(0.0)
    var time: Double
        get() = timeState
        set(v) {
            timeState = v
            if (!playing) requestFrame()
        }
    var playing by mutableStateOf(false)
        private set
    var selected by mutableStateOf<String?>(null)
    var frame by mutableStateOf<ImageBitmap?>(null)
        private set
    /** Which version of the edit (`rev`) the frame on screen was drawn from. */
    var frameRev by mutableIntStateOf(-1)
        private set
    var canUndo by mutableStateOf(false)
        private set
    var canRedo by mutableStateOf(false)
        private set
    var error by mutableStateOf<String?>(null)
    var rev by mutableIntStateOf(0)
        private set
    var trimMode by mutableStateOf(TrimMode.Normal)

    /** Filmstrip frames by index (frame i shows time (i + ½) × tileSpan). Old frames stay up
     *  until fresh ones replace them. */
    val tiles = mutableStateMapOf<Int, ImageBitmap>()
    private val tileRev = HashMap<Int, Pair<Double, Int>>()
    private val tilesBusy = HashSet<Int>()
    var tileSpan by mutableDoubleStateOf(1.0)
        private set
    /** Waveform levels per sound clip. */
    val peaks = mutableStateMapOf<String, List<Float>>()

    /** Long side of the preview render, in pixels (set from the stage's size). */
    var previewMax = 960

    private val engine get() = Core.engine
    private var rendering = false
    private var pending = false
    private var playJob: Job? = null
    private val audio = AudioOut()

    val duration: Double get() = doc?.duration ?: 0.0
    val selectedClip: Clip? get() = doc?.clips?.firstOrNull { it.id == selected }
    val aspect: Double get() = doc?.let { it.canvas.width / it.canvas.height } ?: (16.0 / 9.0)

    fun open() {
        val r = engine.openProject(id)
        if (!r.ok) error = r.text
        reload()
    }

    fun close() {
        pause()
        engine.closeProject()
    }

    /** Run an edit tool on this project; the view updates from what the engine then reports. */
    fun edit(tool: String, args: Map<String, Any?> = emptyMap()): Boolean {
        val r = Core.call(tool, args + ("project" to id))
        if (!r.ok) error = r.text
        reload()
        return r.ok
    }

    /** Add a clip (add_clip, add_text) and select it. */
    fun add(tool: String, args: Map<String, Any?>): Boolean {
        val r = Core.call(tool, args + ("project" to id))
        if (r.ok) {
            runCatching {
                Core.json.parseToJsonElement(r.text).jsonObject["clip"]?.jsonObject?.get("id")?.jsonPrimitive?.content
            }.getOrNull()?.let { selected = it }
        } else {
            error = r.text
        }
        reload()
        return r.ok
    }

    /** Bring files into the project at the playhead (later clips move along); selects the first. */
    fun importFiles(paths: List<String>): Boolean {
        val r = Core.call("import_media", mapOf("project" to id, "paths" to paths, "start" to time))
        if (!r.ok) error = r.text
        else runCatching {
            val j = Core.json.parseToJsonElement(r.text).jsonObject
            j["clips"]?.jsonArray?.firstOrNull()?.jsonObject?.get("id")?.jsonPrimitive?.content
        }.getOrNull()?.let { selected = it }
        reload()
        return r.ok
    }

    /** Hold the selected clip's picture at the playhead: the clip splits there and a still of
     *  that frame goes between the halves. */
    fun freeze(c: Clip, hold: Double = 1.5) {
        if (!c.isVisual || c.type == "text" || time <= c.start + 0.04 || time >= c.end - 0.04) {
            error = "Put the playhead over the clip, then freeze."
            return
        }
        pause()
        val t = time
        val bmp = engine.renderClip(c.id, t, 1920u)?.toBitmap() ?: return
        val f = File(Core.mediaDir, "Freeze ${java.util.UUID.randomUUID().toString().take(4)}.png")
        f.outputStream().use { bmp.compress(android.graphics.Bitmap.CompressFormat.PNG, 100, it) }
        edit("split_clip", mapOf("clip" to c.id, "time" to t))
        val r = Core.call("import_media", mapOf("project" to id, "paths" to listOf(f.path), "addToTimeline" to false))
        val media = runCatching {
            Core.json.parseToJsonElement(r.text).jsonObject["media"]!!.jsonArray[0].jsonObject["id"]!!.jsonPrimitive.content
        }.getOrNull() ?: return
        add("add_clip", mapOf("asset" to media, "track" to c.track, "start" to t, "duration" to hold))
    }

    // Gestures on the preview: the edits until endGesture are one undo step.
    fun beginGesture() = engine.beginGesture()

    /** An edit during a gesture that only the moving clip's own picture needs to show. */
    fun liveUpdate(clip: String, fields: Map<String, Any?>) {
        Core.call("update_clip", fields + mapOf("clip" to clip, "project" to id))
    }

    fun endGesture() {
        engine.endGesture()
        canUndo = engine.canUndo()
    }

    /** The hot edit path (drags, trims, sliders): the engine call, the save and the re-read run
     *  off the main thread; `done` runs on the main thread once the new state is in. */
    fun update(clip: String, fields: Map<String, Any?>, done: (() -> Unit)? = null) {
        val a = fields + mapOf("clip" to clip, "project" to id)
        scope.launch {
            val (r, d) = withContext(Dispatchers.Default) {
                val r = Core.call("update_clip", a)
                r to (if (r.ok) runCatching { Core.decode<ProjectDoc>("get_project", mapOf("project" to id)) }.getOrNull() else null)
            }
            if (!r.ok) error = r.text
            d?.let { apply(it) }
            done?.invoke()
        }
    }

    private fun apply(d: ProjectDoc) {
        doc = d
        if (selected != null && d.clips.none { it.id == selected }) selected = null
        rev += 1
        canUndo = engine.canUndo()
        canRedo = engine.canRedo()
        requestFrame()
    }

    fun reload() {
        val d = runCatching { Core.decode<ProjectDoc>("get_project", mapOf("project" to id)) }.getOrNull()
        if (d != null) apply(d) else {
            canUndo = engine.canUndo()
            canRedo = engine.canRedo()
        }
    }

    fun undo() = edit("undo")
    fun redo() = edit("redo")

    /** Zoom lands coarsely, so a pinch doesn't queue a whole filmstrip re-render every tick. */
    fun setTileSpan(s: Double, exactly: Boolean = false) {
        if (!exactly && kotlin.math.abs(s - tileSpan) <= tileSpan * 0.04) return
        tileSpan = s
    }

    /** Draw filmstrip frame `i` if it's missing or older than the edit or the zoom. */
    fun loadTile(i: Int) {
        val span = tileSpan
        val r = rev
        tileRev[i]?.let { if (it.first == span && it.second == r) return }
        if (!tilesBusy.add(i)) return
        val t = (i + 0.5) * span
        scope.launch {
            val img = withContext(Dispatchers.Default) { engine.renderFrame(t, 140u)?.toImage() }
            tilesBusy.remove(i)
            if (img != null) tiles[i] = img
            tileRev[i] = span to r
            if (r != rev || span != tileSpan) loadTile(i)
        }
    }

    private val peaksAsked = HashSet<String>()
    fun loadPeaks(c: Clip) {
        val key = "${c.id}|${c.dur}|${c.inPoint}"
        if (!peaksAsked.add(key)) return
        val buckets = (c.dur * 24).coerceIn(16.0, 2000.0).toInt().toUInt()
        scope.launch {
            val p = withContext(Dispatchers.Default) { engine.audioPeaks(c.id, buckets) }
            peaks[c.id] = p
        }
    }

    // Preview

    /** Render the frame at `time` off the main thread; while one renders only the latest
     *  request waits behind it. */
    fun requestFrame() {
        if (rendering) {
            pending = true
            return
        }
        rendering = true
        val t = time
        val max = previewMax.toUInt()
        val r = rev
        scope.launch {
            val img = withContext(Dispatchers.Default) { engine.renderFrame(t, max)?.toImage() }
            if (img != null) frame = img
            frameRev = r
            rendering = false
            if (pending) {
                pending = false
                requestFrame()
            }
        }
    }

    // Playback

    fun togglePlay() = if (playing) pause() else play()

    fun play() {
        if (duration <= 0) return
        if (time >= duration - 0.05) time = 0.0
        playing = true
        val from = time
        audio.start(from)
        playJob = scope.launch {
            val t0 = System.nanoTime()
            while (isActive && playing) {
                withFrameNanos { }
                val t = from + (System.nanoTime() - t0) / 1e9
                if (t >= duration) {
                    timeState = duration
                    pause()
                    break
                }
                timeState = t
                requestFrame()
            }
        }
    }

    fun pause() {
        if (!playing) return
        playing = false
        playJob?.cancel()
        playJob = null
        audio.stop()
        requestFrame()
    }
}

/** Plays the engine's mix: renders 100 ms blocks ahead on its own thread into an AudioTrack. */
class AudioOut {
    @Volatile private var generation = 0
    private var thread: Thread? = null

    fun start(from: Double) {
        stop()
        val gen = ++generation
        thread = Thread({
            val block = 4800
            val track = AudioTrack.Builder()
                .setAudioAttributes(AudioAttributes.Builder().setUsage(AudioAttributes.USAGE_MEDIA).setContentType(AudioAttributes.CONTENT_TYPE_MOVIE).build())
                .setAudioFormat(AudioFormat.Builder().setEncoding(AudioFormat.ENCODING_PCM_FLOAT).setSampleRate(48000).setChannelMask(AudioFormat.CHANNEL_OUT_STEREO).build())
                .setBufferSizeInBytes(block * 2 * 4 * 3)
                .setTransferMode(AudioTrack.MODE_STREAM)
                .build()
            track.play()
            var t = from
            val samples = FloatArray(block * 2)
            try {
                while (gen == generation) {
                    val bytes = Core.engine.renderAudioBytes(t, block.toUInt())
                    ByteBuffer.wrap(bytes).order(ByteOrder.LITTLE_ENDIAN).asFloatBuffer().get(samples, 0, minOf(samples.size, bytes.size / 4))
                    t += block / 48000.0
                    if (gen != generation) break
                    track.write(samples, 0, samples.size, AudioTrack.WRITE_BLOCKING)
                }
            } finally {
                track.pause()
                track.flush()
                track.release()
            }
        }, "liemo-audio").apply {
            priority = Thread.MAX_PRIORITY
            start()
        }
    }

    fun stop() {
        generation++
        thread = null
    }
}
