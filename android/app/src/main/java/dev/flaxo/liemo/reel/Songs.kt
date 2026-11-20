package dev.flaxo.liemo.reel

import android.content.Context
import dev.flaxo.liemo.core.Core
import java.io.File

/**
 * Tracks bundled with Liemo (in assets/songs). All by Kevin MacLeod (incompetech.com), CC BY 4.0:
 * free to use in reels as long as the post credits him (the quick reel screen shows the line).
 */
data class Song(val id: String, val title: String, val mood: String) {
    val credit: String get() = "“$title” Kevin MacLeod (incompetech.com), licensed under CC BY 4.0"

    /** The song as a file (the engine and the decoders read paths), copied out of the app once. */
    fun file(context: Context): File {
        val f = File(File(Core.dataDir, "bundled-songs").apply { mkdirs() }, "$id.m4a")
        if (!f.exists()) {
            context.assets.open("songs/$id.m4a").use { i -> f.outputStream().use { i.copyTo(it) } }
        }
        return f
    }

    companion object {
        val library = listOf(
            Song("hitman", "Hitman", "Dark · Hip-hop"),
            Song("volatile-reaction", "Volatile Reaction", "Heavy · Electronic"),
            Song("exit-the-premises", "Exit the Premises", "Driving · Electro"),
            Song("raving-energy", "Raving Energy", "EDM · Hype"),
            Song("funkorama", "Funkorama", "Funk · Groove"),
            Song("pump", "Pump", "Short · Punchy"),
            Song("take-a-chance", "Take a Chance", "Short · Upbeat"),
        )
    }
}

/** How the song is reworked: its speed (pitch follows, like a record) and how much reverb. */
enum class Vibe(val label: String, val rate: Double, val reverb: Double, val room: Double) {
    SpedUp("Sped up", 1.25, 0.18, 0.5),
    Slowed("Slowed + reverb", 0.82, 0.5, 0.85),
    Original("Original", 1.0, 0.0, 0.5),
}

enum class CaptionPlacement(val label: String, val y: Double) { Top("Top", -250.0), Middle("Middle", 0.0), Low("Low", 210.0) }
enum class CaptionAlign(val label: String, val id: String) { Left("Left", "left"), Center("Center", "center"), Right("Right", "right") }
enum class CaptionSize(val label: String, val factor: Double) { Small("Small", 0.72), Medium("Medium", 0.88), Big("Big", 1.12) }
enum class CaptionStyle(val label: String) { Poster("Poster"), Clean("Clean"), Neon("Neon"), Glass("Liquid glass"), Invert("Invert"), Marker("Marker") }

val captionFonts = listOf("Bebas Neue", "Mona Sans", "Inter", "Instrument Serif", "JetBrains Mono", "Caveat")

/** Everything the quick reel screen collects. */
data class ReelPlan(
    val media: List<String> = emptyList(),
    val song: String? = null,
    val songName: String = "",
    val vibe: Vibe = Vibe.SpedUp,
    val length: Double = 15.0,
    val words: String = "",
    /** Synced lyrics, in the song's own time. Empty falls back to `words`, spread on the beat. */
    val lyrics: List<LyricLine> = emptyList(),
    /** Captions at all: the song's lyrics when it has them, else `words`. */
    val captions: Boolean = true,
    /** Words pop in one at a time rather than whole lines. */
    val wordByWord: Boolean = true,
    val font: String = "Bebas Neue",
    val style: CaptionStyle = CaptionStyle.Poster,
    /** Where the song starts in its own file: null lets the drop finder pick. */
    val songStart: Double? = null,
    /** Where each piece of footage starts in its own file; null picks moments for you. */
    val videoStarts: List<Double?> = emptyList(),
    val placement: CaptionPlacement = CaptionPlacement.Low,
    val align: CaptionAlign = CaptionAlign.Center,
    val size: CaptionSize = CaptionSize.Medium,
)
