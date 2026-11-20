package dev.flaxo.liemo.reel

import dev.flaxo.liemo.core.CoreError
import java.io.File
import java.io.RandomAccessFile
import java.nio.ByteBuffer
import java.nio.ByteOrder
import kotlin.math.exp
import kotlin.math.ln
import kotlin.math.max
import kotlin.math.min
import kotlin.math.pow
import kotlin.math.sqrt

/** Song work for quick reels, on 48 kHz interleaved stereo floats. */
object SongFX {
    /**
     * `length` seconds of output from `samples`, starting `start` seconds in, played at `rate`
     * (pitch follows, like a record), written as a 16-bit WAV at `out`. Reverb is the engine's,
     * added to the clip afterwards.
     */
    fun render(samples: FloatArray, start: Double, length: Double, rate: Double, out: File) {
        val frames = samples.size / 2
        val first = start * 48000
        if (first >= frames) throw CoreError("The song is shorter than that.")
        val n = (length * 48000).toInt()
        val pcm = ByteBuffer.allocate(n * 4).order(ByteOrder.LITTLE_ENDIAN)
        for (i in 0 until n) {
            val pos = first + i * rate
            val a = pos.toInt()
            if (a + 1 >= frames) {
                pcm.putShort(0); pcm.putShort(0)
                continue
            }
            val f = (pos - a).toFloat()
            for (c in 0..1) {
                val v = samples[a * 2 + c] + (samples[(a + 1) * 2 + c] - samples[a * 2 + c]) * f
                // A short fade at the very end so the cut doesn't click.
                val tail = min(1f, (n - i) / 2400f)
                pcm.putShort((v.coerceIn(-1f, 1f) * tail * 32767).toInt().toShort())
            }
        }
        writeWav(out, pcm.array())
    }

    fun writeWav(out: File, data: ByteArray, rate: Int = 48000, channels: Int = 2) {
        out.delete()
        RandomAccessFile(out, "rw").use { f ->
            val h = ByteBuffer.allocate(44).order(ByteOrder.LITTLE_ENDIAN)
            h.put("RIFF".toByteArray()); h.putInt(36 + data.size); h.put("WAVE".toByteArray())
            h.put("fmt ".toByteArray()); h.putInt(16); h.putShort(1); h.putShort(channels.toShort())
            h.putInt(rate); h.putInt(rate * channels * 2); h.putShort((channels * 2).toShort()); h.putShort(16)
            h.put("data".toByteArray()); h.putInt(data.size)
            f.write(h.array())
            f.write(data)
        }
    }

    /** Root-mean-square of `len` samples from `from`. */
    private fun rms(s: FloatArray, from: Int, len: Int): Float {
        var sum = 0.0
        for (i in from until min(s.size, from + len)) sum += s[i] * s[i]
        return sqrt(sum / len).toFloat()
    }

    /**
     * Where the song hits hardest: the start of the `window`-second stretch with the most energy,
     * favouring a jump up from what came before (the drop).
     */
    fun dropStart(samples: FloatArray, window: Double): Double {
        val block = 12000 // 0.25 s of stereo frames
        val n = samples.size / 2 / block
        if (n <= 8) return 0.0
        val e = FloatArray(n) { rms(samples, it * block * 2, block * 2) }
        val w = max(1, (window / 0.25).toInt())
        val look = 16 // 4 s either side of the cut
        if (n <= w) return 0.0
        fun mean(a: Int, b: Int): Float {
            val lo = max(0, a)
            val hi = min(n, b)
            if (hi <= lo) return 0f
            var s = 0f
            for (i in lo until hi) s += e[i]
            return s / (hi - lo)
        }
        var best = 0
        var score = Float.NEGATIVE_INFINITY
        for (s in 0..(n - w)) {
            // A jump only counts with real music before it to jump from.
            val jump = if (s >= look) mean(s, s + look) - mean(s - look, s) else 0f
            val v = mean(s, s + w) + 0.8f * jump
            if (v > score) { score = v; best = s }
        }
        return best * 0.25
    }

    /** Tempo and the first beat. With `near`, only tempos within a few percent of it count. */
    fun beats(samples: FloatArray, near: Double? = null): Pair<Double, Double> {
        val hop = 512
        val n = samples.size / 2 / hop
        if (n <= 200) return (near ?: 120.0) to 0.0
        val energy = FloatArray(n) { val r = rms(samples, it * hop * 2, hop * 2); ln(r * r + 1e-6f) }
        // Onset strength, centred: a curve that's always positive correlates with itself everywhere.
        val onset = FloatArray(n)
        for (i in 1 until n) onset[i] = max(0f, energy[i] - energy[i - 1])
        val avg = onset.average().toFloat()
        for (i in 0 until n) onset[i] -= avg

        fun ac(lag: Double): Double {
            val l = lag.toInt()
            val f = (lag - l).toFloat()
            val m = n - l - 1
            if (m <= 0) return 0.0
            var dot = 0f
            for (t in 0 until m) dot += onset[t] * (onset[t + l] * (1 - f) + onset[t + l + 1] * f)
            return dot.toDouble() / m
        }
        val rate = 48000.0 / hop
        val lo = near?.let { it * 0.96 } ?: 70.0
        val hi = near?.let { it * 1.04 } ?: 180.0
        var bestLag = 60 / (near ?: 120.0) * rate
        var bestScore = Double.NEGATIVE_INFINITY
        var bpm = lo
        while (bpm <= hi) {
            val lag = 60 / bpm * rate
            // The real beat also lines up at two and four beats; a 3:2 ghost of it doesn't.
            var s = ac(lag) + 0.5 * ac(2 * lag) + 0.25 * ac(4 * lag)
            if (near == null) s *= 0.85 + 0.15 * exp(-(ln(bpm / 125) / ln(2.0)).pow(2) / 0.5)
            if (s > bestScore) { bestScore = s; bestLag = lag }
            bpm += 0.25
        }
        // Phase: where a grid of that spacing lines up with the most onsets.
        var bestOff = 0
        var bestSum = Float.NEGATIVE_INFINITY
        for (off in 0 until max(1, bestLag.toInt())) {
            var sum = 0f
            var t = off.toDouble()
            while (t.toInt() < n) { sum += onset[t.toInt()]; t += bestLag }
            if (sum > bestSum) { bestSum = sum; bestOff = off }
        }
        return 60 * rate / bestLag to bestOff / rate
    }
}
