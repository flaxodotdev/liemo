package dev.flaxo.liemo.core

import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import kotlinx.serialization.json.JsonElement

// What the engine's tools return (the same JSON the MCP server and the iPhone app read).

@Serializable
data class ProjectList(val projects: List<ProjectSummary> = emptyList())

@Serializable
data class ProjectSummary(
    val id: String,
    val name: String = "",
    val format: String = "",
    val aspect: String = "",
    val duration: Double = 0.0,
    val clips: Int = 0,
    val edited: String = "",
)

@Serializable
data class Created(val id: String)

@Serializable
data class CanvasSize(val width: Double = 1280.0, val height: Double = 720.0)

@Serializable
data class ProjectDoc(
    val id: String,
    val name: String = "",
    val format: String = "",
    val aspect: String = "",
    val fps: Int = 30,
    val exportResolution: Int = 1080,
    val background: String = "#000000",
    val canvas: CanvasSize = CanvasSize(),
    val duration: Double = 0.0,
    val tracks: List<Track> = emptyList(),
    val clips: List<Clip> = emptyList(),
)

@Serializable
data class Track(
    val id: String,
    val name: String = "",
    val kind: String = "",
    val hidden: Boolean = false,
    val muted: Boolean = false,
    val locked: Boolean = false,
)

@Serializable
data class Clip(
    val id: String,
    val asset: String = "",
    val type: String = "",
    val name: String = "",
    val track: String = "",
    val start: Double = 0.0,
    val dur: Double = 0.0,
    @SerialName("in") val inPoint: Double = 0.0,
    val x: Double = 0.0,
    val y: Double = 0.0,
    val scale: Double = 1.0,
    val rot: Double = 0.0,
    val opacity: Double = 1.0,
    val fadeIn: Double = 0.0,
    val fadeOut: Double = 0.0,
    val volume: Double = 1.0,
    val end: Double = 0.0,
    val transition: ClipTransition? = null,
    val effects: List<AppliedEffect> = emptyList(),
    val text: TextSpec? = null,
    val motion: Motion? = null,
    /** Keyframes by property (x, scale, text.size, fx.<uid>.amount…), times from the clip's start. */
    val keys: Map<String, List<ClipKey>>? = null,
) {
    val isVisual: Boolean get() = type != "music" && type != "audio"
    val hasSound: Boolean get() = type == "music" || type == "audio" || type == "video"
    val isMedia: Boolean get() = type == "video" || type == "audio" || type == "music"

    fun keyed(prop: String): Boolean = !(keys?.get(prop).isNullOrEmpty())

    fun hasKey(prop: String, t: Double): Boolean {
        val lt = (t - start).coerceIn(0.0, dur)
        return keys?.get(prop)?.any { kotlin.math.abs(it.t - lt) < 1.0 / 60 } ?: false
    }

    /** The property's keyed value at timeline time `t`, as the engine works it out. */
    fun keyedValue(prop: String, t: Double): Double? {
        val ks = keys?.get(prop)?.takeIf { it.isNotEmpty() } ?: return null
        val lt = t - start
        if (lt <= ks.first().t) return ks.first().v
        if (lt >= ks.last().t) return ks.last().v
        val i = ks.indexOfFirst { it.t > lt }
        if (i <= 0) return ks.last().v
        val a = ks[i - 1]
        val b = ks[i]
        val u = ((lt - a.t) / maxOf(b.t - a.t, 1e-9)).coerceIn(0.0, 1.0)
        return when (a.ease) {
            "hold" -> a.v
            "smooth" -> a.v + (b.v - a.v) * u * u * (3 - 2 * u)
            else -> a.v + (b.v - a.v) * u
        }
    }
}

@Serializable
data class ClipKey(val t: Double, val v: Double, val ease: String? = null)

@Serializable
data class ClipTransition(val id: String, val dur: Double = 0.5)

@Serializable
data class Motion(
    val speed: Double = 1.0,
    val reverse: Boolean = false,
    val flipH: Boolean = false,
    val flipV: Boolean = false,
    val animIn: String = "none",
    val animInDur: Double = 0.5,
    val animOut: String = "none",
    val animOutDur: Double = 0.5,
)

@Serializable
data class AppliedEffect(
    val uid: String,
    val id: String,
    val enabled: Boolean = true,
    val amount: Double = 1.0,
    val params: Map<String, JsonElement> = emptyMap(),
)

@Serializable
data class TextSpec(
    val content: String = "",
    val font: String = "Inter",
    val size: Double = 96.0,
    val weight: Double = 700.0,
    val color: String = "#ffffff",
    val align: String = "center",
    val anim: String = "none",
    val box: Boolean = false,
    val y: Double = 0.0,
    val outline: String? = null,
    val outlineWidth: Double? = null,
    val shadow: String? = null,
    val glow: String? = null,
    val highlight: String? = null,
    val blend: String? = null,
    val tiltX: Double? = null,
    val tiltY: Double? = null,
)

@Serializable
data class LibraryItem(
    val id: String,
    val name: String = "",
    val about: String? = null,
    val duration: Double? = null,
)

@Serializable
data class LibraryFont(val family: String, val role: String = "")

@Serializable
data class Library(
    val footage: List<LibraryItem> = emptyList(),
    @SerialName("shader_backgrounds") val shaderBackgrounds: List<LibraryItem> = emptyList(),
    val stills: List<LibraryItem> = emptyList(),
    val overlays: List<LibraryItem> = emptyList(),
    val music: List<LibraryItem> = emptyList(),
    val textStyles: List<LibraryItem> = emptyList(),
    val fonts: List<LibraryFont> = emptyList(),
    val installedFonts: List<String> = emptyList(),
)

@Serializable
data class FormatList(val formats: List<FormatDef> = emptyList())

@Serializable
data class FormatDef(
    val id: String,
    val name: String = "",
    val aspect: String = "",
    val canvas: List<Double> = listOf(16.0, 9.0),
    val bestFor: String = "",
)

@Serializable
data class EffectList(val effects: List<EffectDef> = emptyList())

@Serializable
data class EffectDef(val id: String, val name: String = "", val group: String = "")

@Serializable
data class TransitionList(val transitions: List<TransitionDef> = emptyList())

@Serializable
data class TransitionDef(val id: String, val name: String = "", val group: String = "")
