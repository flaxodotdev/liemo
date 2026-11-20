package dev.flaxo.liemo.core

import android.content.Context
import android.graphics.Bitmap
import android.graphics.ImageDecoder
import android.media.MediaCodec
import android.media.MediaExtractor
import android.media.MediaFormat
import android.media.MediaMetadataRetriever
import android.media.AudioFormat
import uniffi.liemo_mobile.MediaFrame
import uniffi.liemo_mobile.MediaHost
import uniffi.liemo_mobile.MediaProbe
import java.io.File
import java.nio.ByteBuffer
import java.nio.ByteOrder

/**
 * What the engine asks of Android's media stack: the desktop app does these with FFmpeg and the
 * iPhone app with AVFoundation. The engine calls in from its own threads and waits for the answer.
 */
class AndroidMediaHost(private val context: Context) : MediaHost {
    /** One retriever per video, kept open: opening a file is the slow part. */
    private val retrievers = HashMap<String, MediaMetadataRetriever>()

    override fun probe(path: String): MediaProbe? {
        val r = MediaMetadataRetriever()
        return try {
            r.setDataSource(path)
            val ms = r.extractMetadata(MediaMetadataRetriever.METADATA_KEY_DURATION)?.toLongOrNull() ?: return null
            val hasVideo = r.extractMetadata(MediaMetadataRetriever.METADATA_KEY_HAS_VIDEO) == "yes"
            val hasAudio = r.extractMetadata(MediaMetadataRetriever.METADATA_KEY_HAS_AUDIO) == "yes"
            var w = r.extractMetadata(MediaMetadataRetriever.METADATA_KEY_VIDEO_WIDTH)?.toIntOrNull() ?: 0
            var h = r.extractMetadata(MediaMetadataRetriever.METADATA_KEY_VIDEO_HEIGHT)?.toIntOrNull() ?: 0
            // Phones record sideways and say so: the engine wants the size as it's seen.
            val rot = r.extractMetadata(MediaMetadataRetriever.METADATA_KEY_VIDEO_ROTATION)?.toIntOrNull() ?: 0
            if (rot == 90 || rot == 270) w = h.also { h = w }
            MediaProbe(ms / 1000.0, w.toUInt(), h.toUInt(), hasVideo, hasAudio)
        } catch (_: Exception) {
            null
        } finally {
            r.release()
        }
    }

    override fun videoFrame(path: String, time: Double, maxW: UInt, maxH: UInt): MediaFrame? {
        val r = synchronized(retrievers) {
            retrievers.getOrPut(path) { MediaMetadataRetriever().apply { setDataSource(path) } }
        }
        val bmp = try {
            synchronized(r) {
                r.getScaledFrameAtTime(
                    (time * 1_000_000).toLong().coerceAtLeast(0),
                    MediaMetadataRetriever.OPTION_CLOSEST,
                    maxW.toInt().coerceAtLeast(2),
                    maxH.toInt().coerceAtLeast(2),
                )
            }
        } catch (_: Exception) {
            null
        } ?: return null
        return frameOf(bmp)
    }

    override fun decodeImage(path: String, max: UInt): MediaFrame? = try {
        val src = ImageDecoder.createSource(File(path))
        val bmp = ImageDecoder.decodeBitmap(src) { d, info, _ ->
            d.allocator = ImageDecoder.ALLOCATOR_SOFTWARE
            val long = maxOf(info.size.width, info.size.height)
            if (long > max.toInt() && max.toInt() > 0) {
                val k = max.toDouble() / long
                d.setTargetSize((info.size.width * k).toInt().coerceAtLeast(1), (info.size.height * k).toInt().coerceAtLeast(1))
            }
        }
        frameOf(bmp)
    } catch (_: Exception) {
        null
    }

    override fun decodeAudio(path: String): ByteArray? {
        val pcm = decodePcm(path) ?: return null
        val out = ByteBuffer.allocate(pcm.size * 4).order(ByteOrder.LITTLE_ENDIAN)
        out.asFloatBuffer().put(pcm)
        return out.array()
    }

    /** Cut-outs need a subject finder; Android's comes in a later version. */
    override fun segment(frame: MediaFrame): MediaFrame? = null

    /** A Bitmap as the engine's straight-alpha RGBA. */
    private fun frameOf(src: Bitmap): MediaFrame {
        val bmp = if (src.config == Bitmap.Config.ARGB_8888) src else src.copy(Bitmap.Config.ARGB_8888, false)
        val w = bmp.width
        val h = bmp.height
        val buf = ByteBuffer.allocate(w * h * 4)
        bmp.copyPixelsToBuffer(buf)
        val px = buf.array()
        // Android keeps pixels premultiplied; the engine takes them straight.
        if (bmp.hasAlpha()) {
            var i = 3
            while (i < px.size) {
                val a = px[i].toInt() and 0xff
                if (a in 1..254) {
                    for (c in 1..3) px[i - c] = minOf(255, ((px[i - c].toInt() and 0xff) * 255 + a / 2) / a).toByte()
                }
                i += 4
            }
        }
        return MediaFrame(w.toUInt(), h.toUInt(), px)
    }
}

/** A file's sound as 48 kHz interleaved stereo floats, decoded with MediaCodec. */
fun decodePcm(path: String): FloatArray? {
    val ex = MediaExtractor()
    try {
        ex.setDataSource(path)
        val track = (0 until ex.trackCount).firstOrNull {
            ex.getTrackFormat(it).getString(MediaFormat.KEY_MIME)?.startsWith("audio/") == true
        } ?: return null
        ex.selectTrack(track)
        val fmt = ex.getTrackFormat(track)
        val codec = MediaCodec.createDecoderByType(fmt.getString(MediaFormat.KEY_MIME)!!)
        codec.configure(fmt, null, null, 0)
        codec.start()
        var rate = fmt.getInteger(MediaFormat.KEY_SAMPLE_RATE)
        var channels = fmt.getInteger(MediaFormat.KEY_CHANNEL_COUNT)
        var float = false
        val raw = Floats()
        val info = MediaCodec.BufferInfo()
        var inputDone = false
        val shorts = ShortArray(16384)
        try {
            // Feed every free input buffer and drain every ready output each turn; only wait
            // when the codec has nothing for us, so decoding runs at the codec's own speed.
            while (true) {
                while (!inputDone) {
                    val i = codec.dequeueInputBuffer(0)
                    if (i < 0) break
                    val n = ex.readSampleData(codec.getInputBuffer(i)!!, 0)
                    if (n < 0) {
                        codec.queueInputBuffer(i, 0, 0, 0, MediaCodec.BUFFER_FLAG_END_OF_STREAM)
                        inputDone = true
                    } else {
                        codec.queueInputBuffer(i, 0, n, ex.sampleTime, 0)
                        ex.advance()
                    }
                }
                val o = codec.dequeueOutputBuffer(info, 5_000)
                if (o == MediaCodec.INFO_OUTPUT_FORMAT_CHANGED) {
                    val f = codec.outputFormat
                    rate = f.getInteger(MediaFormat.KEY_SAMPLE_RATE)
                    channels = f.getInteger(MediaFormat.KEY_CHANNEL_COUNT)
                    float = f.containsKey(MediaFormat.KEY_PCM_ENCODING) &&
                        f.getInteger(MediaFormat.KEY_PCM_ENCODING) == AudioFormat.ENCODING_PCM_FLOAT
                } else if (o >= 0) {
                    val b = codec.getOutputBuffer(o)!!.order(ByteOrder.nativeOrder())
                    b.position(info.offset).limit(info.offset + info.size)
                    if (float) {
                        raw.add(b.asFloatBuffer())
                    } else {
                        val sb = b.asShortBuffer()
                        while (sb.hasRemaining()) {
                            val n = minOf(shorts.size, sb.remaining())
                            sb.get(shorts, 0, n)
                            raw.add(shorts, n)
                        }
                    }
                    codec.releaseOutputBuffer(o, false)
                    if (info.flags and MediaCodec.BUFFER_FLAG_END_OF_STREAM != 0) break
                }
            }
        } finally {
            codec.stop()
            codec.release()
        }
        return toStereo48k(raw.toArray(), rate, channels)
    } catch (_: Exception) {
        return null
    } finally {
        ex.release()
    }
}

/** Any rate and channel count to 48 kHz stereo (linear resampling; mono plays on both sides). */
private fun toStereo48k(src: FloatArray, rate: Int, channels: Int): FloatArray {
    val ch = channels.coerceAtLeast(1)
    val frames = src.size / ch
    if (frames == 0) return FloatArray(0)
    val outFrames = (frames.toLong() * 48000 / rate).toInt()
    val out = FloatArray(outFrames * 2)
    val step = rate.toDouble() / 48000
    for (i in 0 until outFrames) {
        val pos = i * step
        val a = pos.toInt().coerceAtMost(frames - 1)
        val b = (a + 1).coerceAtMost(frames - 1)
        val f = (pos - a).toFloat()
        val la = src[a * ch]
        val lb = src[b * ch]
        val ra = if (ch > 1) src[a * ch + 1] else la
        val rb = if (ch > 1) src[b * ch + 1] else lb
        out[i * 2] = la + (lb - la) * f
        out[i * 2 + 1] = ra + (rb - ra) * f
    }
    return out
}

/** A growing float array (a song is millions of samples; boxed lists would be too big). */
class Floats(capacity: Int = 1 shl 16) {
    private var a = FloatArray(capacity)
    var size = 0
        private set

    fun add(v: Float) {
        if (size == a.size) a = a.copyOf(a.size * 2)
        a[size++] = v
    }

    private fun room(n: Int) {
        if (size + n > a.size) a = a.copyOf(maxOf(a.size * 2, size + n))
    }

    fun add(b: java.nio.FloatBuffer) {
        val n = b.remaining()
        room(n)
        b.get(a, size, n)
        size += n
    }

    fun add(s: ShortArray, n: Int) {
        room(n)
        for (i in 0 until n) a[size + i] = s[i] / 32768f
        size += n
    }

    fun toArray(): FloatArray = a.copyOf(size)
}
