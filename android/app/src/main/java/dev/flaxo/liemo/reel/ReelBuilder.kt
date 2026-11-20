package dev.flaxo.liemo.reel

import dev.flaxo.liemo.core.Core
import dev.flaxo.liemo.core.CoreError
import dev.flaxo.liemo.core.Created
import dev.flaxo.liemo.core.decodePcm
import kotlinx.serialization.Serializable
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import java.io.File
import java.util.UUID
import kotlin.math.max
import kotlin.math.min
import kotlin.random.Random

/**
 * Builds a 9:16 reel from a plan with the engine's tools: the song reworked and cut to its drop,
 * the footage cut on the beat with hard transitions and hits, and the words popping in on the
 * beat. The same reel the iPhone app makes.
 */
class ReelBuilder {
    @Serializable
    private data class Imported(val media: List<Media> = emptyList(), val failed: List<Failed>? = null) {
        @Serializable data class Media(val id: String, val type: String, val duration: Double = 0.0)
        @Serializable data class Failed(val error: String = "")
    }

    private fun call(tool: String, args: Map<String, Any?>) = Core.call(tool, args)

    fun build(plan: ReelPlan, report: (String) -> Unit): String {
        val t0 = System.nanoTime()
        val step = { s: String -> android.util.Log.i("Liemo", "reel: $s (%.1fs)".format((System.nanoTime() - t0) / 1e9)); report(s) }
        val song = plan.song ?: throw CoreError("Pick a song.")
        if (plan.media.isEmpty()) throw CoreError("Pick some photos or videos.")

        // 1. The song: find the drop and rework from there, unless the user picked the moment.
        step(if (plan.vibe == Vibe.Original) "Finding the drop…" else "Making it ${plan.vibe.label.lowercase()}…")
        val raw = decodePcm(song) ?: throw CoreError("That song couldn't be read.")
        val rawDuration = raw.size / 2 / 48000.0
        val window = plan.length * plan.vibe.rate
        val start = plan.songStart?.let { min(it, max(0.0, rawDuration - window)) } ?: SongFX.dropStart(raw, window)
        // Named for the timeline ("Hitman (sped up)"), in a folder of its own so names never clash.
        val folder = File(Core.mediaDir, "reel-${UUID.randomUUID().toString().take(8)}").apply { mkdirs() }
        val label = if (plan.vibe == Vibe.Original) plan.songName else "${plan.songName} (${plan.vibe.label.lowercase()})"
        val audio = File(folder, label.replace('/', '-') + ".wav")
        SongFX.render(raw, start, plan.length, plan.vibe.rate, audio)

        step("Finding the beat…")
        // Tempo from a minute of the original around the drop (short clips fool it), scaled by the
        // vibe's speed and kept in cutting range; the beat grid's phase from the result.
        val from = (start * 48000).toInt() * 2
        val minute = if (raw.size > from) raw.copyOfRange(from, min(raw.size, from + 60 * 48000 * 2)) else raw
        var tempo = SongFX.beats(minute).first * plan.vibe.rate
        while (tempo > 175) tempo /= 2
        while (tempo < 80) tempo *= 2
        val mix = decodePcm(audio.path) ?: FloatArray(0)
        val (bpm, first) = SongFX.beats(mix, tempo)
        val beat = 60 / bpm
        step("Cutting at %.0f BPM…".format(bpm))

        // 2. A project with the media in its pool.
        val name = plan.words.lines().firstOrNull()?.trim().orEmpty()
        val id = Core.decode<Created>("create_project", mapOf(
            "format" to "reel", "name" to if (name.isEmpty()) "Reel · ${plan.songName}" else name.take(40),
        )).id
        val opened = Core.engine.openProject(id)
        if (!opened.ok) throw CoreError(opened.text)
        try {
            val imported = Core.decode<Imported>("import_media", mapOf(
                "project" to id, "paths" to plan.media + audio.path, "addToTimeline" to false,
            ))
            val visuals = imported.media.filter { it.type != "audio" }
            val sound = imported.media.firstOrNull { it.type == "audio" }
            if (sound == null || visuals.isEmpty()) {
                throw CoreError(imported.failed?.firstOrNull()?.error?.takeIf { it.isNotEmpty() } ?: "Those files couldn't be read.")
            }

            // 3. Cuts on the beat: mostly every two beats, with one-beat flurries before the hits.
            val cuts = arrayListOf(0.0)
            var t = if (first > 0.08) first else first + beat
            var i = 0
            val fast = bpm > 140
            while (t < plan.length - 0.05) {
                cuts += t
                val flurry = i % 8 >= 6
                t += beat * (if (fast) (if (flurry) 1.0 else 2.0) else (if (flurry) 0.5 else 1.0)) * (if (bpm < 95) 2 else 1)
                i++
            }
            cuts += plan.length

            val look = Looks.of(plan.vibe)
            val cursor = HashMap<String, Double>() // how far into each video we've used
            for (k in 0 until cuts.size - 1) {
                val a = cuts[k]
                val b = cuts[k + 1]
                if (b - a <= 0.04) continue
                val m = visuals[k % visuals.size]
                val args = mutableMapOf<String, Any?>("project" to id, "asset" to m.id, "track" to "V1", "start" to a, "duration" to b - a)
                if (m.type == "video") {
                    val wanted = plan.videoStarts.getOrNull(k % plan.media.size)
                    val room = max(0.0, m.duration - (b - a))
                    val inPoint = if (wanted != null && wanted >= 0) min(wanted + (cursor[m.id] ?: 0.0), room)
                    else if (room > 0) ((cursor[m.id] ?: 0.0) + Random.nextDouble(0.0, 1.5)) % room else 0.0
                    args["in"] = inPoint
                    cursor[m.id] = if (wanted != null) (cursor[m.id] ?: 0.0) + (b - a) else inPoint + (b - a)
                }
                val r = call("add_clip", args)
                val cid = clipId(r.text) ?: continue
                call("apply_effect", mapOf("project" to id, "clip" to cid, "effect" to look.grade.first, "amount" to look.grade.second))
                if (k % 4 == 3) {
                    val hit = look.hits[(k / 4) % look.hits.size]
                    call("apply_effect", mapOf("project" to id, "clip" to cid, "effect" to hit.first, "amount" to hit.second))
                }
                if (k > 0 && k % 2 == 0) {
                    val tr = look.transitions[(k / 2) % look.transitions.size]
                    call("set_transition", mapOf("project" to id, "clip" to cid, "transition" to tr, "duration" to min(look.transitionLength, (b - a) * 0.6)))
                }
            }

            // 4. The song, with the vibe's reverb from the engine.
            val s = call("add_clip", mapOf("project" to id, "asset" to sound.id, "track" to "A1", "start" to 0, "duration" to plan.length))
            clipId(s.text)?.let { sid ->
                val f = mutableMapOf<String, Any?>("project" to id, "clip" to sid, "fadeIn" to 0.05, "fadeOut" to 0.6)
                if (plan.vibe.reverb > 0) f["sound"] = mapOf("reverb" to plan.vibe.reverb, "room" to plan.vibe.room)
                call("update_clip", f)
            }

            // 5. Words popping in on the beat.
            captions(plan, id, start, raw.size / 2 / 48000.0, first, beat, bpm, step)
            step("Done")
            return id
        } finally {
            Core.engine.closeProject()
        }
    }

    private fun captions(plan: ReelPlan, id: String, start: Double, songDuration: Double, first: Double, beat: Double, bpm: Double, step: (String) -> Unit) {
        val sizeF = sizeFactor(plan.font)
        fun add(text: String, size: Double, at: Double, dur: Double, accent: Boolean) =
            call("add_text", lyric(text, id, size, at, dur, accent, plan))

        if (plan.lyrics.isNotEmpty() && plan.captions) {
            // Synced lyrics on their real moments: the song starts at `start` and plays at the
            // vibe's rate, so a line at time t lands at (t - start) / rate.
            step("Adding the lyrics…")
            val r = plan.vibe.rate
            val lines = plan.lyrics.map { l ->
                LyricLine((l.time - start) / r, l.text, l.words?.map { LyricWord((it.time - start) / r, it.text) })
            }
            // Lines sung before the drop never show; one that runs over it starts at the top.
            val shown = lines.indices.filter { i -> (lines.getOrNull(i + 1)?.time ?: Double.MAX_VALUE) > 0.15 }.map { lines[it] }.toMutableList()
            if (shown.isNotEmpty()) shown[0] = shown[0].copy(time = max(shown[0].time, 0.05))
            var last = 0.0
            for ((n, l) in shown.withIndex()) {
                val at = max(l.time, last + 0.05)
                val next = if (n + 1 < shown.size) max(shown[n + 1].time, at + 0.05) else plan.length
                val until = min(next, plan.length) - 0.05
                if (until - at <= 0.3) continue
                // One size for the whole line so word-by-word captions don't bounce around.
                val size = min(118.0, 1150.0 / max(4, l.text.length)) * sizeF
                var pops = emptyList<LyricWord>()
                if (plan.wordByWord) {
                    val stamped = l.words
                    if (stamped != null && stamped.size > 1) pops = stamped
                    else {
                        val toks = l.text.split(' ').filter { it.isNotEmpty() }
                        if (toks.size > 1 && until - at > 0.6) {
                            // Spread across the line, longer words held longer.
                            val total = toks.sumOf { max(1, it.length) }.toDouble()
                            var acc = at
                            pops = toks.map { tok -> LyricWord(acc, tok).also { acc += (until - at) * max(1, tok.length) / total } }
                        }
                    }
                }
                if (pops.size > 1) {
                    val longest = pops.maxOf { it.text.length }
                    val wSize = min(150.0, 1600.0 / max(3, longest)) * sizeF
                    for ((w, word) in pops.withIndex()) {
                        val wAt = max(word.time, at)
                        if (wAt >= until) break
                        val wNext = if (w + 1 < pops.size) max(pops[w + 1].time, wAt + 0.08) else until
                        add(word.text, wSize, wAt, min(wNext, until) - wAt, n % 3 == 2)
                    }
                } else {
                    add(l.text, size, at, until - at, n % 3 == 2)
                }
                last = at
            }
            return
        }
        val text = plan.words.trim()
        if (text.isEmpty()) return
        val lines = text.lines().map { it.trim() }.filter { it.isNotEmpty() }
        if (lines.size > 1) {
            // Pasted plain lyrics: the reel starts partway into the song, so start the words
            // partway into the lyrics too (songs sing in roughly the 8–90% stretch).
            step("Adding the lyrics…")
            val f = ((start / max(songDuration, 1.0)) - 0.08) / 0.82
            val skip = min((lines.size - 1).toDouble(), max(0.0, f) * lines.size).toInt()
            val shown = lines.drop(skip)
            var at = max(if (first > 0.08) first else first + beat, 0.05)
            val each = min(beat * 4, max(beat, (plan.length - at) / shown.size))
            for ((n, w) in shown.withIndex()) {
                if (at >= plan.length - 0.1) break
                add(w, min(140.0, 1400.0 / max(4, w.length)) * sizeF, at, min(each, plan.length - at), n % 3 == 2)
                at += each
            }
        } else {
            // One caption: its words spread over the reel, each held one to four beats.
            val words = chunks(text)
            var at = if (first > 0.08) first else first + beat
            val room = (plan.length - at) / beat
            val perWord = min(4, max(if (bpm > 140) 2 else 1, (room / words.size).toInt())).toDouble()
            val each = beat * perWord
            for ((n, w) in words.withIndex()) {
                if (at >= plan.length - 0.1) break
                add(w, min(140.0, 1400.0 / max(4, w.length)) * sizeF, at, min(each, plan.length - at), n % 3 == 2)
                at += each
            }
        }
    }

    /** One or two words per caption: short words ride along with the next one. */
    private fun chunks(text: String): List<String> {
        val words = text.split(Regex("\\s+")).filter { it.isNotEmpty() }
        val out = ArrayList<String>()
        var i = 0
        while (i < words.size) {
            if (words[i].length <= 3 && i + 1 < words.size && words[i + 1].length <= 7) {
                out += words[i] + " " + words[i + 1]
                i += 2
            } else {
                out += words[i]
                i++
            }
        }
        return out
    }

    /** A lyric caption in the face and look the user picked; every third one is accented. */
    private fun lyric(content: String, project: String, size: Double, at: Double, dur: Double, accent: Boolean, plan: ReelPlan): Map<String, Any?> {
        val a = mutableMapOf<String, Any?>(
            "project" to project, "content" to content.uppercase(), "start" to at, "duration" to dur,
            "font" to plan.font, "weight" to weight(plan.font), "size" to size * plan.size.factor, "y" to plan.placement.y,
            "align" to plan.align.id, "animation" to "pop",
        )
        when (plan.style) {
            CaptionStyle.Poster -> { a["color"] = if (accent) "#ffd400" else "#ffffff"; a["outline"] = "#000000"; a["outlineWidth"] = 5 }
            CaptionStyle.Clean -> { a["color"] = "#ffffff"; a["shadow"] = "#000000" }
            CaptionStyle.Neon -> { a["color"] = "#ffffff"; a["glow"] = if (accent) "#00e5ff" else "#ff2bd6" }
            CaptionStyle.Glass -> { a["color"] = "#ffffff"; a["glassText"] = "#ffffff" }
            CaptionStyle.Invert -> { a["color"] = "#ffffff"; a["blend"] = "invert" }
            CaptionStyle.Marker -> { a["color"] = "#111111"; a["highlight"] = if (accent) "#ff7a3d" else "#ffd400" }
        }
        return a
    }

    /** Captions are sized for Bebas's condensed face; wider faces are scaled down to fit. */
    private fun sizeFactor(font: String) = when (font) {
        "Bebas Neue" -> 1.0
        "Instrument Serif" -> 0.88
        "Caveat" -> 0.92
        "JetBrains Mono" -> 0.72
        else -> 0.8
    }

    private fun weight(font: String) = when (font) {
        "Bebas Neue", "Instrument Serif" -> 400
        "JetBrains Mono" -> 500
        "Caveat" -> 700
        else -> 800
    }

    private fun clipId(json: String): String? = runCatching {
        Core.json.parseToJsonElement(json).jsonObject["clip"]?.jsonObject?.get("id")?.jsonPrimitive?.content
    }.getOrNull()
}

/** The grade, the hits and the transitions for each vibe. */
private class Looks(val grade: Pair<String, Double>, val hits: List<Pair<String, Double>>, val transitions: List<String>, val transitionLength: Double) {
    companion object {
        fun of(v: Vibe) = when (v) {
            Vibe.SpedUp -> Looks(
                "ig-punchy" to 0.7,
                listOf("rgbsplit" to 0.8, "shake" to 0.7, "zoom-blur" to 0.6, "glitch" to 0.7),
                listOf("whip", "remocn-glitch-cut", "grid-flash-4x4-random", "zoom", "spin", "remocn-zoom-blur", "push-left"), 0.25,
            )
            Vibe.Slowed -> Looks(
                "ig-moody" to 0.75,
                listOf("glow" to 0.6, "vhs" to 0.6, "rgbsplit" to 0.5, "dream" to 0.6),
                listOf("blur", "remocn-smear-pan", "dissolve", "remocn-focus-pull", "remocn-warp-dissolve"), 0.5,
            )
            Vibe.Original -> Looks(
                "ig-teal-orange" to 0.6,
                listOf("rgbsplit" to 0.7, "shake" to 0.6, "zoom-blur" to 0.5),
                listOf("whip", "zoom", "grid-flash", "push-up"), 0.3,
            )
        }
    }
}
