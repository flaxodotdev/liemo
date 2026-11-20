package dev.flaxo.liemo.ui

import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.gestures.awaitEachGesture
import androidx.compose.foundation.gestures.awaitFirstDown
import androidx.compose.foundation.gestures.calculateZoom
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Add
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.SideEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.clipToBounds
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Rect
import androidx.compose.ui.geometry.RoundRect
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.drawscope.DrawScope
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.graphics.drawscope.clipPath
import androidx.compose.ui.graphics.drawscope.clipRect
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.input.pointer.positionChange
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.drawText
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.rememberTextMeasurer
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.IntSize
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import dev.flaxo.liemo.core.Clip
import dev.flaxo.liemo.core.EditorModel
import dev.flaxo.liemo.core.ProjectDoc
import dev.flaxo.liemo.core.Track
import dev.flaxo.liemo.core.TrimMode
import dev.flaxo.liemo.core.formatTime
import kotlin.math.abs
import kotlin.math.ceil
import kotlin.math.floor
import kotlin.math.max
import kotlin.math.min
import kotlin.math.roundToInt

/** One lane on screen, bottom-up: main video, then sound, then layers. Sizes in pixels. */
data class Lane(val track: String, val height: Float, val rows: Int, val rowH: Float, val y: Float = 0f)

/** Rows for clips that share moments (they stack): first fit by start time. */
fun stackRows(clips: List<Clip>): Pair<Map<String, Int>, Int> {
    val ends = ArrayList<Double>()
    val row = HashMap<String, Int>()
    for (c in clips.sortedBy { it.start }) {
        val r = ends.indexOfFirst { it <= c.start + 1e-6 }
        if (r >= 0) {
            ends[r] = c.end
            row[c.id] = r
        } else {
            ends.add(c.end)
            row[c.id] = ends.size - 1
        }
    }
    return row to max(1, ends.size)
}

/**
 * Bottom-up: the main video, every sound track, then picture layers and titles, each only when
 * it has clips. Every lane grows with its stacked rows: a lane that didn't would hide its own
 * clips under the lanes below it, where no scrolling reaches. `scroll` lifts them all.
 */
private fun lanes(doc: ProjectDoc, height: Float, scroll: Float, u: Float): List<Lane> {
    val used = doc.clips.map { it.track }.toSet()
    val gap = 6 * u
    fun grown(id: String, rowH: Float): Lane {
        val n = stackRows(doc.clips.filter { it.track == id }).second
        return Lane(id, n * rowH + (n - 1) * 4 * u, n, rowH)
    }
    val stack = ArrayList<Lane>()
    stack += grown("V1", 58 * u)
    val sounding = doc.tracks.filter { it.kind == "audio" && it.id in used }.map { it.id }
    if (sounding.isEmpty()) stack += Lane("A1", 34 * u, 1, 34 * u)
    else sounding.forEach { stack += grown(it, 34 * u) }
    // Picture layers from the back up (the track list runs front to back), then titles.
    for (t in doc.tracks.reversed()) {
        if (t.id != "V1" && t.kind != "audio" && t.id in used) stack += grown(t.id, 30 * u)
    }
    var y = height - 18 * u + scroll
    return stack.map { l ->
        y -= l.height
        val placed = l.copy(y = y)
        y -= gap
        placed
    }
}

private enum class Part { Move, Left, Right }

/** The clip being dragged, and how far (pixels). */
private data class ClipDrag(val clip: String, val part: Part, val dx: Float, val gen: Int)

private class Placed(val clip: Clip, val lane: Lane, val rect: Rect)

/**
 * The timeline, phone-editor style: everything scrolls under a fixed playhead in the middle.
 * The main video sits at the bottom as a filmstrip of real frames, sound above it with its
 * waveform, and titles and overlays above that. One gesture handler decides what each touch is
 * (tap to select, drag sideways to scrub, up and down to reach more layers, drag a selected clip
 * to move it or its edges to trim, pinch to zoom, press and hold a lane for its track's menu),
 * so no two of them fight over a finger.
 */
@Composable
fun Timeline(model: EditorModel, addMedia: () -> Unit, addAudio: () -> Unit, modifier: Modifier = Modifier) {
    val u = LocalDensity.current.density
    var pps by remember { mutableFloatStateOf(60f) } // dp per second
    var vScroll by remember { mutableFloatStateOf(0f) }
    var drag by remember { mutableStateOf<ClipDrag?>(null) }
    var dragGen by remember { mutableIntStateOf(0) }
    var menu by remember { mutableStateOf<Pair<Track, Offset>?>(null) }
    val measurer = rememberTextMeasurer(cacheSize = 96)
    // What the gesture handler reads: the layout as last drawn.
    val geo = remember { object { var placed = emptyList<Placed>(); var lanes = emptyList<Lane>(); var origin = 0f; var scrollMax = 0f; var lift = 0f } }

    val aspect = model.aspect
    val tileSpan = 58.0 * min(max(aspect, 0.56), 1.0) / pps
    LaunchedEffect(tileSpan) { model.setTileSpan(tileSpan) }

    BoxWithConstraints(modifier.background(Palette.bg).clipToBounds()) {
        val w = constraints.maxWidth.toFloat()
        val h = constraints.maxHeight.toFloat()
        val ruler = 30 * u
        val doc = model.doc
        val time = model.time
        val ppx = pps * u
        val mid = w / 2
        val origin = mid - time.toFloat() * ppx
        val top = doc?.let { lanes(it, h, 0f, u).lastOrNull() }
        val scrollMax = top?.let { max(0f, ruler + 6 * u - it.y) } ?: 0f
        val lift = min(vScroll, scrollMax)
        val laneList = doc?.let { lanes(it, h, lift, u) } ?: emptyList()
        val placed = ArrayList<Placed>()
        if (doc != null) for (lane in laneList) {
            val cs = doc.clips.filter { it.track == lane.track }
            val rows = stackRows(cs).first
            for (c in cs) {
                val r = rows[c.id] ?: 0
                val y = lane.y + r * (lane.rowH + 4 * u)
                placed += Placed(c, lane, Rect(c.start.toFloat() * ppx, y, (c.start + c.dur).toFloat() * ppx, y + lane.rowH))
            }
        }
        geo.placed = placed; geo.lanes = laneList; geo.origin = origin; geo.scrollMax = scrollMax; geo.lift = lift

        // Filmstrip frames and waveforms that are on screen.
        val span = model.tileSpan
        SideEffect {
            for (p in placed) {
                if (p.lane.track == "V1") {
                    val a = max(floor(p.clip.start / span).toInt(), floor(-origin / ppx / span).toInt())
                    val b = min(ceil(p.clip.end / span).toInt(), ceil((w - origin) / ppx / span).toInt())
                    for (i in a until max(a, b)) model.loadTile(i)
                } else if (!p.clip.isVisual) {
                    model.loadPeaks(p.clip)
                }
            }
        }

        val selected = model.selected
        val mode = model.trimMode
        val tiles = model.tiles
        val peaks = model.peaks
        Canvas(
            Modifier.fillMaxSize().pointerInput(Unit) {
                awaitEachGesture {
                    val down = awaitFirstDown()
                    val g = geo
                    val px = (down.position.x - g.origin)
                    val hit = g.placed.lastOrNull { it.rect.contains(Offset(px, down.position.y)) }
                    val handleW = 22 * u
                    val part = when {
                        hit == null || hit.clip.id != model.selected -> Part.Move
                        px - hit.rect.left < handleW && hit.rect.width > handleW -> Part.Left
                        hit.rect.right - px < handleW && hit.rect.width > handleW -> Part.Right
                        else -> Part.Move
                    }
                    var total = Offset.Zero
                    val first = withTimeoutOrNull(viewConfiguration.longPressTimeoutMillis) {
                        var r = ""
                        while (r.isEmpty()) {
                            val ev = awaitPointerEvent()
                            val c = ev.changes.firstOrNull { it.id == down.id }
                            r = when {
                                ev.changes.count { it.pressed } >= 2 -> "multi"
                                c == null || !c.pressed -> "up"
                                else -> {
                                    total += c.positionChange()
                                    if (total.getDistance() > viewConfiguration.touchSlop) "move" else ""
                                }
                            }
                        }
                        r
                    }
                    when (first) {
                        null -> {
                            // Press and hold: the track's menu.
                            val lane = g.lanes.firstOrNull { down.position.y >= it.y && down.position.y < it.y + it.height }
                            model.doc?.tracks?.firstOrNull { it.id == lane?.track }?.let { menu = it to down.position }
                            do {
                                val ev = awaitPointerEvent()
                                ev.changes.forEach { it.consume() }
                            } while (ev.changes.any { it.pressed })
                            return@awaitEachGesture
                        }
                        "up" -> {
                            model.selected = if (hit == null || hit.clip.id == model.selected) null else hit.clip.id
                            return@awaitEachGesture
                        }
                    }
                    // 0 scrub, 1 scroll, 2 clip, 3 zoom
                    var kind = when {
                        first == "multi" -> 3
                        hit != null && hit.clip.id == model.selected && abs(total.y) <= abs(total.x) * 0.75f -> 2
                        abs(total.y) > abs(total.x) * 0.75f && g.scrollMax > 0 -> 1
                        else -> 0
                    }
                    val startTime = model.time
                    val startLift = g.lift
                    var zoomBase = pps
                    var zoom = 1f
                    if (kind == 2) {
                        dragGen++
                        drag = ClipDrag(hit!!.clip.id, part, total.x, dragGen)
                    }
                    if (kind == 0 && model.playing) model.pause()
                    while (true) {
                        val ev = awaitPointerEvent()
                        val pressed = ev.changes.filter { it.pressed }
                        if (pressed.isEmpty()) break
                        if (pressed.size >= 2 && kind != 2) {
                            if (kind != 3) { kind = 3; zoomBase = pps; zoom = 1f }
                            zoom *= ev.calculateZoom()
                            pps = (zoomBase * zoom).coerceIn(8f, 400f)
                            ev.changes.forEach { it.consume() }
                            continue
                        }
                        val c = ev.changes.firstOrNull { it.id == down.id } ?: continue
                        total += c.positionChange()
                        when (kind) {
                            0 -> model.time = (startTime - total.x / (pps * u)).coerceIn(0.0, max(model.duration, 0.0))
                            1 -> vScroll = (startLift + total.y).coerceIn(0f, g.scrollMax)
                            2 -> drag = drag?.copy(dx = total.x)
                        }
                        ev.changes.forEach { it.consume() }
                    }
                    when (kind) {
                        2 -> drag?.let { d ->
                            val c = model.doc?.clips?.firstOrNull { it.id == d.clip }
                            if (c == null) drag = null
                            else commit(model, c, d.part, (d.dx / (pps * u)).toDouble()) { if (drag?.gen == d.gen) drag = null }
                        }
                        3 -> model.setTileSpan(58.0 * min(max(model.aspect, 0.56), 1.0) / pps, exactly = true)
                    }
                }
            },
        ) {
            ruler(origin, ppx, pps, w, ruler, measurer)
            clipRect(top = ruler) {
                for (p in placed) {
                    val d = drag?.takeIf { it.clip == p.clip.id }
                    drawClip(p, d, mode, selected == p.clip.id, origin, ppx, span, tiles, peaks, measurer, u)
                }
            }
            if (scrollMax > 0) {
                // A thin bar on the right says there's more above or below.
                val room = h - ruler
                val barH = max(24 * u, room * room / (room + scrollMax))
                drawRoundRect(
                    Color.White.copy(alpha = 0.35f), Offset(w - 5 * u, ruler + (room - barH) * (1 - lift / scrollMax)),
                    Size(3 * u, barH), CornerRadius(2 * u),
                )
            }
            // The playhead.
            drawRect(Color.White, Offset(mid - u, 0f), Size(2 * u, h))
        }

        val noSound = doc?.clips?.none { it.track == "A1" || it.track == "A2" } ?: false
        laneList.firstOrNull { it.track == "A1" }?.takeIf { noSound }?.let { lane ->
            Row(
                Modifier.offset { IntOffset((8 * u).roundToInt(), lane.y.roundToInt()) }
                    .size(((w - 16 * u) / u).dp, (lane.height / u).dp)
                    .clip(RoundedCornerShape(6.dp)).background(Palette.surface1).clickable(onClick = addAudio)
                    .padding(start = 12.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Icon(Icons.Default.Add, null, tint = Palette.muted, modifier = Modifier.size(14.dp))
                Spacer(Modifier.width(4.dp))
                Text("Add audio", color = Palette.muted, fontSize = 12.sp, fontWeight = FontWeight.Medium)
            }
        }
        laneList.firstOrNull()?.let { main ->
            Box(
                Modifier.offset { IntOffset((w - 48 * u).roundToInt(), (main.y + (main.rowH - 36 * u) / 2).roundToInt()) }
                    .size(36.dp).clip(CircleShape).background(Color.White).clickable(onClick = addMedia),
                contentAlignment = Alignment.Center,
            ) { Icon(Icons.Default.Add, "Add media", tint = Color.Black, modifier = Modifier.size(20.dp)) }
        }
        menu?.let { (t, at) ->
            Box(Modifier.offset { IntOffset(at.x.roundToInt(), at.y.roundToInt()) }) {
                TrackMenu(model, t) { menu = null }
            }
        }
    }
}

@Composable
private fun TrackMenu(model: EditorModel, t: Track, close: () -> Unit) {
    val audio = t.kind == "audio"
    DropdownMenu(expanded = true, onDismissRequest = close, containerColor = Palette.surface2) {
        Text(t.name, color = Palette.muted, fontSize = 12.sp, modifier = Modifier.padding(horizontal = 16.dp, vertical = 6.dp))
        if (audio) {
            DropdownMenuItem(text = { Text(if (t.muted) "Unmute track" else "Mute track") }, onClick = {
                model.edit("set_track", mapOf("track" to t.id, "muted" to !t.muted)); close()
            })
        } else {
            DropdownMenuItem(text = { Text(if (t.hidden) "Show track" else "Hide track") }, onClick = {
                model.edit("set_track", mapOf("track" to t.id, "hidden" to !t.hidden)); close()
            })
        }
        DropdownMenuItem(text = { Text(if (t.locked) "Unlock track" else "Lock track") }, onClick = {
            model.edit("set_track", mapOf("track" to t.id, "locked" to !t.locked)); close()
        })
        HorizontalDivider(color = Palette.border)
        // Higher tracks are in front: titles can go between two overlays.
        DropdownMenuItem(text = { Text(if (audio) "Move up" else "Move up (in front)") }, onClick = {
            model.edit("set_track", mapOf("track" to t.id, "move" to "up")); close()
        })
        DropdownMenuItem(text = { Text(if (audio) "Move down" else "Move down (behind)") }, onClick = {
            model.edit("set_track", mapOf("track" to t.id, "move" to "down")); close()
        })
    }
}

/** The finger lifted: one edit with where the clip (or its edge) ended up. */
private fun commit(model: EditorModel, c: Clip, part: Part, d: Double, settle: () -> Unit) {
    if (abs(d) < 1e-9) { settle(); return }
    val trimTo = when (model.trimMode to part) {
        TrimMode.Ripple to Part.Left -> "ripple-head"
        TrimMode.Ripple to Part.Right -> "ripple-tail"
        TrimMode.Roll to Part.Left -> "roll-head"
        TrimMode.Roll to Part.Right -> "roll-tail"
        TrimMode.Slip to Part.Move -> "slip"
        TrimMode.Slide to Part.Move -> "slide"
        else -> null
    }
    if (trimTo != null) {
        model.edit("trim_clip", mapOf("clip" to c.id, "mode" to trimTo, "by" to d))
        settle()
        return
    }
    when (part) {
        Part.Move -> model.update(c.id, mapOf("start" to max(0.0, c.start + d)), settle)
        Part.Right -> model.update(c.id, mapOf("duration" to max(0.2, c.dur + d)), settle)
        Part.Left -> {
            var dd = min(d, c.dur - 0.2)
            dd = max(dd, -c.start)
            if (c.isMedia) dd = max(dd, -c.inPoint)
            val f = mutableMapOf<String, Any?>("start" to c.start + dd, "duration" to c.dur - dd)
            if (c.isMedia) f["in"] = c.inPoint + dd
            model.update(c.id, f, settle)
        }
    }
}

private fun DrawScope.ruler(origin: Float, ppx: Float, pps: Float, width: Float, height: Float, m: androidx.compose.ui.text.TextMeasurer) {
    val step = if (pps > 120) 0.5 else if (pps > 40) 1.0 else if (pps > 15) 5.0 else 10.0
    val u = density
    var t = max(0.0, floor(-origin / ppx / step) * step)
    val last = (width - origin) / ppx
    val style = TextStyle(color = Palette.muted, fontSize = 9.sp, fontFamily = FontFamily.Monospace)
    while (t <= last) {
        val x = origin + t.toFloat() * ppx
        val major = (t / step).roundToInt() % 2 == 0
        drawRect(Palette.muted.copy(alpha = 0.6f), Offset(x, 0f), Size(u, (if (major) 8 else 4) * u))
        if (major) drawText(m, formatTime(t).replace(".0", ""), Offset(x, 10 * u), style)
        t += step
    }
}

private fun DrawScope.drawClip(
    p: Placed,
    d: ClipDrag?,
    mode: TrimMode,
    selected: Boolean,
    origin: Float,
    ppx: Float,
    span: Double,
    tiles: Map<Int, androidx.compose.ui.graphics.ImageBitmap>,
    peaks: Map<String, List<Float>>,
    m: androidx.compose.ui.text.TextMeasurer,
    u: Float,
) {
    val c = p.clip
    val dx = d?.dx ?: 0f
    // While dragging, how the clip looks: a ripple keeps the head in place, a slip the whole clip.
    val holdsStart = d == null || d.part == Part.Right || (d.part == Part.Left && mode == TrimMode.Ripple) ||
        (d.part == Part.Move && mode == TrimMode.Slip)
    val x = origin + p.rect.left + (if (holdsStart) 0f else dx)
    val w = max(6 * u, p.rect.width + when (d?.part) { Part.Left -> -dx; Part.Right -> dx; else -> 0f })
    val top = p.rect.top
    val h = p.rect.height
    if (x > size.width || x + w < 0) return
    val shape = RoundRect(x + 0.75f * u, top, x + w - 0.75f * u, top + h, CornerRadius(6 * u))
    val path = Path().apply { addRoundRect(shape) }
    clipPath(path) {
        if (p.lane.track == "V1") {
            drawRect(Palette.surface2, Offset(x, top), Size(w, h))
            // Frames of the edit as it will look, edge to edge.
            val tileW = (span * ppx).toFloat()
            val first = floor(c.start / span).toInt()
            val last = ceil(c.end / span).toInt()
            for (i in first until max(first + 1, last)) {
                val tx = origin + (i * span).toFloat() * ppx + (if (holdsStart) 0f else dx)
                if (tx > size.width || tx + tileW < 0) continue
                val img = tiles[i] ?: continue
                // Fill the tile box, cropping the frame's overflow.
                val k = max(tileW / img.width, h / img.height)
                val sw = (tileW / k).coerceAtMost(img.width.toFloat())
                val sh = (h / k).coerceAtMost(img.height.toFloat())
                drawImage(
                    img,
                    srcOffset = IntOffset(((img.width - sw) / 2).roundToInt(), ((img.height - sh) / 2).roundToInt()),
                    srcSize = IntSize(sw.roundToInt().coerceAtLeast(1), sh.roundToInt().coerceAtLeast(1)),
                    dstOffset = IntOffset(tx.roundToInt(), top.roundToInt()),
                    dstSize = IntSize(ceil(tileW).toInt(), h.roundToInt()),
                )
                drawRect(Color.Black.copy(alpha = 0.25f), Offset(tx + tileW - 0.5f * u, top), Size(0.5f * u, h))
            }
        } else if (!c.isVisual) {
            drawRect(Palette.clip(c.type), Offset(x, top), Size(w, h))
            val pk = peaks[c.id]
            if (!pk.isNullOrEmpty()) {
                val bar = max(1f, w / pk.size)
                for ((i, v) in pk.withIndex()) {
                    val bh = max(1.5f * u, v * (h - 10 * u))
                    drawRect(Palette.wave.copy(alpha = 0.75f), Offset(x + i * bar, top + (h - bh) / 2), Size(max(1f, bar - 1), bh))
                }
            }
            if (w >= 60 * u) label(m, c.name, x + 7 * u, top, w - 14 * u, h, u)
        } else {
            drawRect(Palette.clip(c.type), Offset(x, top), Size(w, h))
            if (w >= 30 * u) {
                val name = if (c.type == "text") c.text?.content?.replace("\n", " ") ?: c.name else c.name
                label(m, (if (c.type == "text") "T  " else "▣  ") + name, x + 7 * u, top, w - 14 * u, h, u)
            }
        }
    }
    if (selected) {
        drawRoundRect(Color.White, Offset(shape.left, top), Size(shape.width, h), CornerRadius(6 * u), style = Stroke(2 * u))
        val hh = max(10 * u, h - 12 * u)
        for (hx in listOf(x + 6 * u, x + w - 16 * u)) {
            drawRoundRect(Color.White, Offset(hx, top + (h - hh) / 2), Size(10 * u, hh), CornerRadius(3 * u))
        }
    }
}

private fun DrawScope.label(m: androidx.compose.ui.text.TextMeasurer, s: String, x: Float, top: Float, maxW: Float, h: Float, u: Float) {
    if (maxW <= 4) return
    val r = m.measure(
        s, TextStyle(color = Palette.ink, fontSize = 11.sp, fontWeight = FontWeight.Medium),
        maxLines = 1, overflow = TextOverflow.Ellipsis,
        constraints = androidx.compose.ui.unit.Constraints(maxWidth = maxW.roundToInt()),
    )
    drawText(r, topLeft = Offset(x, top + (h - r.size.height) / 2))
}
