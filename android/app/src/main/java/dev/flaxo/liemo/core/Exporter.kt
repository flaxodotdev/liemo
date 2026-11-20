package dev.flaxo.liemo.core

import android.content.ContentValues
import android.content.Context
import android.media.MediaCodec
import android.media.MediaCodecInfo
import android.media.MediaFormat
import android.media.MediaMuxer
import android.provider.MediaStore
import java.io.File
import java.nio.ByteBuffer
import java.nio.ByteOrder
import kotlin.math.abs
import kotlin.math.ceil
import kotlin.math.roundToInt

data class ExportSettings(
    /** Short side, in pixels. */
    val resolution: Int = 1080,
    val fps: Int = 30,
)

/**
 * Renders the open project with the engine and encodes it with MediaCodec (H.264, the engine's
 * mix as AAC) into an MP4, then saves it to Movies › Liemo. Returns the file, for sharing.
 */
class Exporter {
    @Volatile private var cancelled = false
    fun cancel() { cancelled = true }

    private class Enc(val codec: MediaCodec) {
        var track = -1
        var done = false
    }

    fun run(context: Context, doc: ProjectDoc, settings: ExportSettings, progress: (Double) -> Unit): File {
        val duration = doc.duration
        if (duration <= 0.05) throw CoreError("There's nothing on the timeline to export yet.")
        val engine = Core.engine
        val (cw, ch) = doc.canvas.width to doc.canvas.height
        val k = settings.resolution / minOf(cw, ch)
        val w = (cw * k / 2).roundToInt() * 2
        val h = (ch * k / 2).roundToInt() * 2
        val long = maxOf(w, h).toUInt()
        val fps = settings.fps

        val dir = File(Core.cacheDir, "exports").apply { mkdirs() }
        dir.listFiles()?.forEach { it.delete() }
        val name = (doc.name.ifBlank { "Liemo" }).replace(Regex("[/\\\\:*?\"<>|]"), "-")
        val out = File(dir, "$name.mp4")

        val vf = MediaFormat.createVideoFormat(MediaFormat.MIMETYPE_VIDEO_AVC, w, h).apply {
            setInteger(MediaFormat.KEY_COLOR_FORMAT, MediaCodecInfo.CodecCapabilities.COLOR_FormatYUV420Flexible)
            setInteger(MediaFormat.KEY_BIT_RATE, (w.toLong() * h * fps * 0.1).toLong().coerceAtMost(Int.MAX_VALUE.toLong()).toInt())
            setInteger(MediaFormat.KEY_FRAME_RATE, fps)
            setInteger(MediaFormat.KEY_I_FRAME_INTERVAL, 1)
            setInteger(MediaFormat.KEY_COLOR_STANDARD, MediaFormat.COLOR_STANDARD_BT709)
            setInteger(MediaFormat.KEY_COLOR_RANGE, MediaFormat.COLOR_RANGE_LIMITED)
            setInteger(MediaFormat.KEY_COLOR_TRANSFER, MediaFormat.COLOR_TRANSFER_SDR_VIDEO)
        }
        val video = Enc(MediaCodec.createEncoderByType(MediaFormat.MIMETYPE_VIDEO_AVC))
        try {
            video.codec.configure(vf, null, null, MediaCodec.CONFIGURE_FLAG_ENCODE)
        } catch (e: Exception) {
            video.codec.release()
            throw CoreError("This phone can't encode ${settings.resolution}p video. Try a lower resolution.")
        }

        val mixBytes = engine.renderAudioRangeBytes(0.0, duration)
        val mix = FloatArray(mixBytes.size / 4).also { ByteBuffer.wrap(mixBytes).order(ByteOrder.LITTLE_ENDIAN).asFloatBuffer().get(it) }
        val audio = if (mix.any { abs(it) > 1e-5f }) {
            val af = MediaFormat.createAudioFormat(MediaFormat.MIMETYPE_AUDIO_AAC, 48000, 2).apply {
                setInteger(MediaFormat.KEY_AAC_PROFILE, MediaCodecInfo.CodecProfileLevel.AACObjectLC)
                setInteger(MediaFormat.KEY_BIT_RATE, 192_000)
                setInteger(MediaFormat.KEY_MAX_INPUT_SIZE, 4800 * 4)
            }
            Enc(MediaCodec.createEncoderByType(MediaFormat.MIMETYPE_AUDIO_AAC)).also { it.codec.configure(af, null, null, MediaCodec.CONFIGURE_FLAG_ENCODE) }
        } else null

        val muxer = MediaMuxer(out.path, MediaMuxer.OutputFormat.MUXER_OUTPUT_MPEG_4)
        var started = false
        val waiting = ArrayList<Triple<Enc, ByteArray, MediaCodec.BufferInfo>>()
        val info = MediaCodec.BufferInfo()

        fun drain(e: Enc) {
            while (!e.done) {
                val i = e.codec.dequeueOutputBuffer(info, 0)
                when {
                    i == MediaCodec.INFO_TRY_AGAIN_LATER -> return
                    i == MediaCodec.INFO_OUTPUT_FORMAT_CHANGED -> {
                        e.track = muxer.addTrack(e.codec.outputFormat)
                        if (video.track >= 0 && (audio == null || audio.track >= 0)) {
                            muxer.start()
                            started = true
                            for ((we, bytes, wi) in waiting) muxer.writeSampleData(we.track, ByteBuffer.wrap(bytes), wi)
                            waiting.clear()
                        }
                    }
                    i >= 0 -> {
                        val buf = e.codec.getOutputBuffer(i)!!
                        if (info.flags and MediaCodec.BUFFER_FLAG_CODEC_CONFIG != 0) info.size = 0
                        if (info.size > 0) {
                            buf.position(info.offset).limit(info.offset + info.size)
                            if (started) muxer.writeSampleData(e.track, buf, info)
                            else {
                                val copy = ByteArray(info.size).also { buf.get(it) }
                                val ci = MediaCodec.BufferInfo().apply { set(0, info.size, info.presentationTimeUs, info.flags) }
                                waiting += Triple(e, copy, ci)
                            }
                        }
                        val eos = info.flags and MediaCodec.BUFFER_FLAG_END_OF_STREAM != 0
                        e.codec.releaseOutputBuffer(i, false)
                        if (eos) e.done = true
                    }
                }
            }
        }

        video.codec.start()
        audio?.codec?.start()
        val frames = ceil(duration * fps).toInt()
        val audioFrames = mix.size / 2
        var audioPos = 0
        val chunk = 1024
        var audioEnded = audio == null
        try {
            var f = 0
            while (f < frames || !video.done || (audio != null && !audio.done)) {
                if (cancelled) throw CoreError("Export cancelled.")
                // One video frame, when the encoder has room.
                if (f <= frames) {
                    val i = video.codec.dequeueInputBuffer(10_000)
                    if (i >= 0) {
                        if (f == frames) {
                            video.codec.queueInputBuffer(i, 0, 0, f * 1_000_000L / fps, MediaCodec.BUFFER_FLAG_END_OF_STREAM)
                        } else {
                            val t = f.toDouble() / fps
                            val frame = engine.renderFrame(t, long)
                            val img = video.codec.getInputImage(i)!!
                            if (frame != null) toYuv(frame.rgba, frame.width.toInt(), frame.height.toInt(), img, w, h)
                            video.codec.queueInputBuffer(i, 0, w * h * 3 / 2, f * 1_000_000L / fps, 0)
                            progress(f.toDouble() / frames)
                        }
                        f++
                    }
                }
                // Sound up to the picture.
                if (audio != null && !audioEnded) {
                    val until = minOf(audioFrames, ((f.toDouble() / fps) * 48000).toInt() + chunk)
                    while (audioPos < until || (f > frames && !audioEnded)) {
                        val i = audio.codec.dequeueInputBuffer(0)
                        if (i < 0) break
                        val buf = audio.codec.getInputBuffer(i)!!.order(ByteOrder.LITTLE_ENDIAN)
                        if (audioPos >= audioFrames) {
                            audio.codec.queueInputBuffer(i, 0, 0, audioPos * 1_000_000L / 48000, MediaCodec.BUFFER_FLAG_END_OF_STREAM)
                            audioEnded = true
                            break
                        }
                        val n = minOf(chunk, audioFrames - audioPos, buf.capacity() / 4)
                        buf.clear()
                        for (j in 0 until n * 2) {
                            buf.putShort((mix[audioPos * 2 + j].coerceIn(-1f, 1f) * 32767).toInt().toShort())
                        }
                        audio.codec.queueInputBuffer(i, 0, n * 4, audioPos * 1_000_000L / 48000, 0)
                        audioPos += n
                    }
                }
                drain(video)
                audio?.let { drain(it) }
            }
        } catch (e: Exception) {
            runCatching { video.codec.stop() }; video.codec.release()
            audio?.let { runCatching { it.codec.stop() }; it.codec.release() }
            runCatching { if (started) muxer.stop() }; muxer.release()
            out.delete()
            throw if (e is CoreError) e else CoreError("The video couldn't be written: ${e.message}")
        }
        video.codec.stop(); video.codec.release()
        audio?.let { it.codec.stop(); it.codec.release() }
        muxer.stop()
        muxer.release()
        progress(1.0)
        saveToMovies(context, out)
        return out
    }

    /** Into the phone's Movies › Liemo, where the gallery finds it. */
    private fun saveToMovies(context: Context, file: File) {
        val r = context.contentResolver
        val values = ContentValues().apply {
            put(MediaStore.Video.Media.DISPLAY_NAME, file.name)
            put(MediaStore.Video.Media.MIME_TYPE, "video/mp4")
            put(MediaStore.Video.Media.RELATIVE_PATH, "Movies/Liemo")
            put(MediaStore.Video.Media.IS_PENDING, 1)
        }
        val uri = r.insert(MediaStore.Video.Media.EXTERNAL_CONTENT_URI, values) ?: return
        r.openOutputStream(uri)?.use { o -> file.inputStream().use { it.copyTo(o) } }
        values.clear()
        values.put(MediaStore.Video.Media.IS_PENDING, 0)
        r.update(uri, values, null, null)
    }
}

/**
 * The engine's RGBA (sw×sh) into the encoder's YUV 4:2:0 picture (w×h, BT.709, limited range),
 * whatever its layout: planar or interleaved chroma, padded rows.
 */
private fun toYuv(rgba: ByteArray, sw: Int, sh: Int, img: android.media.Image, w: Int, h: Int) {
    val planes = img.planes
    val yp = planes[0]
    val yBuf = yp.buffer
    val yRow = yp.rowStride
    val row = ByteArray(w)
    val sx = IntArray(w) { minOf(sw - 1, it * sw / w) }
    for (y in 0 until h) {
        val srcY = minOf(sh - 1, y * sh / h) * sw
        for (x in 0 until w) {
            val p = (srcY + sx[x]) * 4
            val r = rgba[p].toInt() and 0xff
            val g = rgba[p + 1].toInt() and 0xff
            val b = rgba[p + 2].toInt() and 0xff
            row[x] = (((47 * r + 157 * g + 16 * b) shr 8) + 16).toByte()
        }
        yBuf.position(y * yRow)
        yBuf.put(row, 0, w)
    }
    val up = planes[1]
    val vp = planes[2]
    val uBuf = up.buffer
    val vBuf = vp.buffer
    for (y in 0 until h / 2) {
        val srcY = minOf(sh - 1, (y * 2) * sh / h) * sw
        for (x in 0 until w / 2) {
            val p = (srcY + sx[x * 2]) * 4
            val r = rgba[p].toInt() and 0xff
            val g = rgba[p + 1].toInt() and 0xff
            val b = rgba[p + 2].toInt() and 0xff
            val u = ((-26 * r - 87 * g + 112 * b) shr 8) + 128
            val v = ((112 * r - 102 * g - 10 * b) shr 8) + 128
            uBuf.put(y * up.rowStride + x * up.pixelStride, u.coerceIn(0, 255).toByte())
            vBuf.put(y * vp.rowStride + x * vp.pixelStride, v.coerceIn(0, 255).toByte())
        }
    }
}
