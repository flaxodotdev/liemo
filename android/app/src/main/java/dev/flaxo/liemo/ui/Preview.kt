package dev.flaxo.liemo.ui

import androidx.compose.foundation.Canvas
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.gestures.awaitEachGesture
import androidx.compose.foundation.gestures.awaitFirstDown
import androidx.compose.foundation.gestures.calculatePan
import androidx.compose.foundation.gestures.calculateRotation
import androidx.compose.foundation.gestures.calculateZoom
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.graphics.TransformOrigin
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.graphics.drawscope.rotate
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.input.pointer.positionChange
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.unit.dp
import dev.flaxo.liemo.core.Core
import dev.flaxo.liemo.core.EditorModel
import dev.flaxo.liemo.core.toImage
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

/** What a gesture shows while it runs: the frame without the clip, the clip alone, and how far it
 *  has moved, grown and turned. */
private data class Live(
    val clip: String,
    val anchor: Offset,
    val bg: ImageBitmap? = null,
    val layer: ImageBitmap? = null,
    val offset: Offset = Offset.Zero,
    val scale: Float = 1f,
    val rot: Float = 0f,
    /** The edit version that holds the result, once committed. */
    val committed: Int? = null,
)

/**
 * The picture. Tap a clip to select it (tap a selected title again to edit it); drag, pinch and
 * twist to move, size and turn the selected clip. While fingers move, only the clip's own
 * picture moves over a still of everything else, so it follows the finger at full speed; the
 * edit goes to the engine once, when they lift.
 */
@Composable
fun PreviewStage(model: EditorModel, editText: () -> Unit, modifier: Modifier = Modifier) {
    val scope = rememberCoroutineScope()
    var live by remember { mutableStateOf<Live?>(null) }
    var box by remember { mutableStateOf<List<Double>>(emptyList()) }
    val density = LocalDensity.current.density

    // The selection box follows the clip at the playhead.
    val sel = model.selected
    LaunchedEffect(sel, model.rev, model.time) {
        box = if (sel == null) emptyList() else withContext(Dispatchers.Default) { Core.engine.clipBox(sel, model.time) }
    }
    // Hand back to the engine's frame once it shows the committed edit (no flash between).
    LaunchedEffect(model.frameRev) {
        val c = live?.committed
        if (c != null && model.frameRev >= c) live = null
    }

    BoxWithConstraints(modifier.padding(12.dp), contentAlignment = Alignment.Center) {
        val a = model.aspect.toFloat()
        val gw = constraints.maxWidth.toFloat()
        val gh = constraints.maxHeight.toFloat()
        val fitW = if (gw / gh > a) gh * a else gw
        val fitH = if (gw / gh > a) gh else gw / a
        val canvasW = model.doc?.canvas?.width?.toFloat() ?: 1280f
        val k = fitW / canvasW // pixels per canvas unit
        LaunchedEffect(fitW, fitH) {
            val px = maxOf(fitW, fitH).toInt().coerceIn(320, 1440)
            if (px != model.previewMax) {
                model.previewMax = px
                model.requestFrame()
            }
        }
        Box(
            Modifier.size((fitW / density).dp, (fitH / density).dp).background(Color.Black)
                .border(1.dp, Palette.border)
                .pointerInput(k) {
                    awaitEachGesture {
                        val down = awaitFirstDown()
                        var moved = Offset.Zero
                        val tap = withTimeoutOrNull(400) {
                            while (true) {
                                val ev = awaitPointerEvent()
                                if (ev.changes.count { it.pressed } >= 2) return@withTimeoutOrNull false
                                val c = ev.changes.firstOrNull { it.id == down.id } ?: return@withTimeoutOrNull false
                                if (!c.pressed) return@withTimeoutOrNull true
                                moved += c.positionChange()
                                if (moved.getDistance() > viewConfiguration.touchSlop) return@withTimeoutOrNull false
                            }
                            @Suppress("UNREACHABLE_CODE") false
                        } ?: false
                        if (tap) {
                            val hit = Core.engine.hitTest(model.time, (down.position.x / k).toDouble(), (down.position.y / k).toDouble())
                            when {
                                hit != null -> {
                                    if (hit == model.selected && model.selectedClip?.type == "text") editText()
                                    model.selected = hit
                                }
                                model.selected != null -> model.selected = null
                                else -> model.togglePlay()
                            }
                            return@awaitEachGesture
                        }
                        // A move: the clip under the finger, else the selected one.
                        Core.engine.hitTest(model.time, (down.position.x / k).toDouble(), (down.position.y / k).toDouble())
                            ?.let { if (it != model.selected) model.selected = it }
                        val c = model.selectedClip
                        if (c == null || !c.isVisual || model.time < c.start || model.time >= c.end || live?.let { it.committed == null } == true) {
                            do { val ev = awaitPointerEvent() } while (ev.changes.any { it.pressed })
                            return@awaitEachGesture
                        }
                        model.pause()
                        model.beginGesture()
                        val fresh = Core.engine.clipBox(c.id, model.time)
                        val doc = model.doc
                        val au = if (fresh.size == 7) Offset(fresh[5].toFloat(), fresh[6].toFloat())
                        else Offset(((doc?.canvas?.width ?: 1280.0) / 2 + c.x).toFloat(), ((doc?.canvas?.height ?: 720.0) / 2 + c.y).toFloat())
                        live = Live(c.id, au * k, offset = moved)
                        val (t, max) = model.time to model.previewMax.toUInt()
                        scope.launch {
                            val (bg, layer) = withContext(Dispatchers.Default) {
                                Core.engine.renderFrameWithout(c.id, t, max)?.toImage() to Core.engine.renderClip(c.id, t, max)?.toImage()
                            }
                            live?.takeIf { it.clip == c.id && it.committed == null }?.let { live = it.copy(bg = bg, layer = layer) }
                        }
                        while (true) {
                            val ev = awaitPointerEvent()
                            if (ev.changes.none { it.pressed }) break
                            val l = live ?: break
                            live = l.copy(
                                offset = l.offset + ev.calculatePan(),
                                scale = l.scale * ev.calculateZoom(),
                                rot = l.rot + ev.calculateRotation(),
                            )
                            ev.changes.forEach { it.consume() }
                        }
                        // The fingers lifted: one edit with where the clip ended up.
                        val l = live ?: run { model.endGesture(); return@awaitEachGesture }
                        val f = mutableMapOf<String, Any?>()
                        if (l.offset != Offset.Zero) {
                            f["x"] = c.x + l.offset.x / k
                            f["y"] = c.y + l.offset.y / k
                        }
                        if (kotlin.math.abs(l.scale - 1) > 0.001f) f["scale"] = (c.scale * l.scale).coerceIn(0.05, 8.0)
                        if (kotlin.math.abs(l.rot) > 0.01f) f["rotation"] = c.rot + l.rot
                        if (f.isEmpty()) {
                            model.endGesture()
                            live = null
                        } else {
                            model.update(c.id, f) {
                                live = live?.copy(committed = model.rev)
                                model.endGesture()
                            }
                        }
                    }
                },
        ) {
            val l = live
            val pic = l?.bg ?: model.frame
            pic?.let { Image(it, null, contentScale = ContentScale.FillBounds, modifier = Modifier.fillMaxSize()) }
            val origin = l?.let { TransformOrigin(it.anchor.x / fitW, it.anchor.y / fitH) } ?: TransformOrigin.Center
            l?.layer?.let { layer ->
                Image(
                    layer, null, contentScale = ContentScale.FillBounds,
                    modifier = Modifier.fillMaxSize().graphicsLayer {
                        transformOrigin = origin
                        scaleX = l.scale; scaleY = l.scale
                        rotationZ = l.rot
                        translationX = l.offset.x; translationY = l.offset.y
                    },
                )
            }
            // The selected clip's outline, following the moving layer while fingers are down.
            val c = model.selectedClip
            if (box.size == 7 && c != null && model.time >= c.start && model.time < c.end) {
                val follow = l?.takeIf { it.committed == null }
                Canvas(
                    Modifier.fillMaxSize().graphicsLayer {
                        if (follow != null) {
                            transformOrigin = origin
                            scaleX = follow.scale; scaleY = follow.scale
                            rotationZ = follow.rot
                            translationX = follow.offset.x; translationY = follow.offset.y
                        }
                    },
                ) {
                    val cx = box[0].toFloat() * k
                    val cy = box[1].toFloat() * k
                    val bw = box[2].toFloat() * k
                    val bh = box[3].toFloat() * k
                    rotate(box[4].toFloat(), Offset(cx, cy)) {
                        drawRect(Color.White.copy(alpha = 0.9f), Offset(cx - bw / 2, cy - bh / 2), Size(bw, bh), style = Stroke(1.5f * density))
                    }
                }
            }
        }
    }
}
