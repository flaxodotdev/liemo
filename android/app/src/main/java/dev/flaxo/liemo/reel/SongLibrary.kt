package dev.flaxo.liemo.reel

import android.media.MediaMetadataRetriever
import androidx.compose.runtime.mutableStateListOf
import dev.flaxo.liemo.core.Core
import kotlinx.serialization.Serializable
import kotlinx.serialization.builtins.ListSerializer
import java.io.File
import java.net.HttpURLConnection
import java.net.URL
import java.net.URLEncoder
import java.util.UUID
import kotlin.math.abs
import kotlin.math.max

/** One word of a line, with its own moment (from word-synced LRC). */
@Serializable
data class LyricWord(val time: Double, val text: String)

/** One line of a song at a moment in it, from synced lyrics (LRC). */
@Serializable
data class LyricLine(val time: Double, val text: String, val words: List<LyricWord>? = null)

/** A song you brought in, kept so it's one tap next time. Its lyrics ride along. */
@Serializable
data class SavedSong(
    val id: String,
    val title: String,
    val artist: String = "",
    val file: String,
    val synced: String? = null,
    val plain: String? = null,
    val added: Double = 0.0,
    /** Seconds to shift every lyric stamp by (later when positive). */
    val offset: Double = 0.0,
) {
    val path: File get() = File(SongLibrary.songsDir, file)
    val hasSynced: Boolean get() = !synced.isNullOrEmpty()
    val lines: List<LyricLine> get() = SongLibrary.parseLRC(synced ?: "")
    val wordSynced: Boolean get() = lines.firstOrNull()?.words?.isNotEmpty() == true
    val plainText: String get() = plain?.takeIf { it.isNotEmpty() } ?: lines.joinToString("\n") { it.text }
}

/** The songs you brought in, in songs.json next to their audio files. */
object SongLibrary {
    val songsDir: File get() = File(Core.dataDir, "songs").apply { mkdirs() }
    private val index: File get() = File(Core.dataDir, "songs.json")
    private val serializer = ListSerializer(SavedSong.serializer())

    val songs = mutableStateListOf<SavedSong>().also { list ->
        runCatching { list.addAll(Core.json.decodeFromString(serializer, index.readText())) }
    }

    private fun save() {
        runCatching { index.writeText(Core.json.encodeToString(serializer, songs.toList())) }
    }

    /** Add a song, copying its audio into the songs folder. */
    fun add(source: File, title: String, artist: String, synced: String?, plain: String?): SavedSong {
        val ext = source.extension.ifEmpty { "m4a" }.lowercase()
        val name = "${UUID.randomUUID().toString().take(8)}.$ext"
        val dst = File(songsDir, name)
        if (source.parentFile != songsDir) source.copyTo(dst, overwrite = true) else source.renameTo(dst)
        val s = SavedSong(name, title, artist, dst.name, synced, plain, System.currentTimeMillis() / 1000.0)
        songs.add(0, s)
        save()
        return s
    }

    fun update(s: SavedSong) {
        val i = songs.indexOfFirst { it.id == s.id }
        if (i >= 0) songs[i] = s
        save()
    }

    fun remove(s: SavedSong) {
        songs.removeAll { it.id == s.id }
        s.path.delete()
        save()
    }

    /** What the file says it is: its tags, else "Artist - Title" in its name, else the name. */
    fun nameOf(file: File, displayName: String): Pair<String, String> {
        val r = MediaMetadataRetriever()
        try {
            r.setDataSource(file.path)
            val t = r.extractMetadata(MediaMetadataRetriever.METADATA_KEY_TITLE)?.trim().orEmpty()
            val a = (r.extractMetadata(MediaMetadataRetriever.METADATA_KEY_ARTIST)
                ?: r.extractMetadata(MediaMetadataRetriever.METADATA_KEY_ALBUMARTIST))?.trim().orEmpty()
            if (t.isNotEmpty()) return t to a
        } catch (_: Exception) {
        } finally {
            r.release()
        }
        val base = displayName.substringBeforeLast('.').replace('_', ' ').trim()
        val parts = base.split(" - ", " – ", limit = 2)
        return if (parts.size == 2) parts[1].trim() to parts[0].trim() else base to ""
    }

    // LRC

    /**
     * `[01:23.45] words`, also `[01:23]` and several stamps before one line. An `[offset: ±ms]`
     * tag shifts every stamp after it. Word-synced lines carry `<00:13.50>` before each word.
     * Symbols the caption fonts can't draw (♪, ★…) are dropped.
     */
    fun parseLRC(text: String): List<LyricLine> {
        val out = ArrayList<LyricLine>()
        var offset = 0.0
        for (raw in text.lines()) {
            val stamps = ArrayList<Double>()
            var rest = raw
            while (rest.startsWith("[")) {
                val close = rest.indexOf(']')
                if (close < 0) break
                val tag = rest.substring(1, close)
                if (tag.startsWith("offset")) {
                    val ms = tag.removePrefix("offset").trim(':', ' ', '=').toDoubleOrNull() ?: break
                    offset = ms / 1000
                } else {
                    stamps += stamp(tag) ?: break
                }
                rest = rest.substring(close + 1)
            }
            val line = rest.trim()
            if (stamps.isEmpty() || line.isEmpty() || line == "♪") continue
            var words: List<LyricWord>? = null
            var display = line
            if ('<' in line) {
                var at = stamps[0]
                val parts = ArrayList<Pair<Double, String>>()
                var scan = line
                while (true) {
                    val open = scan.indexOf('<')
                    if (open < 0) break
                    val head = scan.substring(0, open).trim()
                    if (head.isNotEmpty()) parts += at to head
                    val close = scan.indexOf('>', open)
                    if (close < 0) break
                    stamp(scan.substring(open + 1, close))?.let { at = it }
                    scan = scan.substring(close + 1)
                }
                scan.trim().takeIf { it.isNotEmpty() }?.let { parts += at to it }
                if (parts.size > 1) {
                    words = parts.mapNotNull { (t, w) -> sanitize(w).takeIf { it.isNotEmpty() }?.let { LyricWord(max(0.0, t - offset), it) } }
                    display = parts.joinToString(" ") { it.second }
                }
            }
            display = sanitize(display)
            for (t in stamps) out += LyricLine(max(0.0, t - offset), display, words)
        }
        return out.sortedBy { it.time }
    }

    fun sanitize(s: String): String =
        s.filter { it.isLetterOrDigit() || it.isWhitespace() || it in ".,!?¡¿'’\"“”-–—:;()&%" }.trim()

    private fun stamp(tag: String): Double? {
        val parts = tag.split(':')
        if (parts.size > 2) return null
        val secs = parts.last().toDoubleOrNull() ?: return null
        val mins = if (parts.size == 2) parts[0].toDoubleOrNull() ?: 0.0 else 0.0
        return mins * 60 + secs
    }

    // Lyrics from LRCLIB (free, no key).

    data class Fetched(val synced: String?, val plain: String?)

    private data class LrcResult(val duration: Double?, val plain: String?, val synced: String?)

    /**
     * `get` for an exact artist and title, `search` when that misses. With the song's real
     * duration, the best match wins: synced beats plain, and a cut of another length loses.
     */
    fun fetchLyrics(artist: String, track: String, duration: Double?): Fetched {
        val got = ArrayList<LrcResult>()
        got += lrclib("get", listOf("artist_name" to artist, "track_name" to track))
        got += lrclib("search", listOf("track_name" to track, "artist_name" to artist))
        got += lrclib("search", listOf("q" to "$artist $track".trim()))
        fun score(r: LrcResult): Double {
            var s = 0.0
            if (!r.synced.isNullOrEmpty()) s += 10
            if (r.synced?.contains('<') == true) s += 3
            if (!r.plain.isNullOrEmpty()) s += 1
            if (duration != null && r.duration != null && r.duration > 0) s += 4 * max(0.0, 1 - abs(r.duration - duration) / 15)
            return s
        }
        val best = got.maxByOrNull { score(it) } ?: return Fetched(null, null)
        return if (best.synced.isNullOrEmpty() && best.plain.isNullOrEmpty()) Fetched(null, null) else Fetched(best.synced, best.plain)
    }

    private fun lrclib(endpoint: String, query: List<Pair<String, String>>): List<LrcResult> = runCatching {
        val q = query.joinToString("&") { (k, v) -> "$k=${URLEncoder.encode(v, "UTF-8")}" }
        val c = URL("https://lrclib.net/api/$endpoint?$q").openConnection() as HttpURLConnection
        c.setRequestProperty("User-Agent", "Liemo/${Core.engine.version()} (Android)")
        c.connectTimeout = 8000
        c.readTimeout = 12000
        if (c.responseCode != 200) return emptyList()
        val body = c.inputStream.bufferedReader().readText()
        val el = Core.json.parseToJsonElement(body)
        val items = if (el is kotlinx.serialization.json.JsonArray) el.toList() else listOf(el)
        items.mapNotNull { e ->
            val o = e as? kotlinx.serialization.json.JsonObject ?: return@mapNotNull null
            fun str(k: String) = (o[k] as? kotlinx.serialization.json.JsonPrimitive)?.takeIf { it.isString }?.content
            LrcResult(
                (o["duration"] as? kotlinx.serialization.json.JsonPrimitive)?.content?.toDoubleOrNull(),
                str("plainLyrics"), str("syncedLyrics"),
            )
        }
    }.getOrDefault(emptyList())
}
