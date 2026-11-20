package dev.flaxo.liemo.ui

import android.content.Context
import android.content.Intent
import android.net.Uri
import android.provider.OpenableColumns
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.PickVisualMediaRequest
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.grid.GridCells
import androidx.compose.foundation.lazy.grid.LazyVerticalGrid
import androidx.compose.foundation.lazy.grid.items
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.CheckCircle
import androidx.compose.material.icons.filled.Folder
import androidx.compose.material.icons.filled.MusicNote
import androidx.compose.material.icons.filled.PhotoLibrary
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.Icon
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.SegmentedButton
import androidx.compose.material3.SegmentedButtonDefaults
import androidx.compose.material3.SingleChoiceSegmentedButtonRow
import androidx.compose.material3.Slider
import androidx.compose.material3.SliderDefaults
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.key
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.core.content.FileProvider
import dev.flaxo.liemo.core.Clip
import dev.flaxo.liemo.core.Core
import dev.flaxo.liemo.core.EditorModel
import dev.flaxo.liemo.core.ExportSettings
import dev.flaxo.liemo.core.Exporter
import dev.flaxo.liemo.core.Library
import dev.flaxo.liemo.core.LibraryItem
import dev.flaxo.liemo.core.TextSpec
import dev.flaxo.liemo.core.formatTime
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import java.io.File

// MARK: Library

private val tabs = listOf("footage" to "Footage", "backgrounds" to "Backgrounds", "stills" to "Stills", "overlays" to "Overlays", "music" to "Music", "text" to "Text")

@Composable
fun LibraryPanel(model: EditorModel, startTab: String, done: () -> Unit) {
    val context = LocalContext.current
    val scope = rememberCoroutineScope()
    val library = remember { runCatching { Core.decode<Library>("list_library") }.getOrNull() }
    var tab by remember { mutableStateOf(startTab) }
    var importing by remember { mutableStateOf(false) }

    fun finish(uris: List<Uri>) {
        if (uris.isEmpty()) return
        importing = true
        scope.launch {
            val paths = withContext(Dispatchers.IO) { uris.mapNotNull { copyIn(context, it)?.path } }
            importing = false
            if (paths.isNotEmpty() && model.importFiles(paths)) done()
        }
    }
    val photos = rememberLauncherForActivityResult(ActivityResultContracts.PickMultipleVisualMedia(20)) { finish(it) }
    val files = rememberLauncherForActivityResult(ActivityResultContracts.OpenMultipleDocuments()) { finish(it) }

    Column(Modifier.heightIn(min = 420.dp)) {
        Row(Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 4.dp), verticalAlignment = Alignment.CenterVertically) {
            Text("Add", fontSize = 17.sp, fontWeight = FontWeight.SemiBold, modifier = Modifier.weight(1f))
            TextButton(onClick = { photos.launch(PickVisualMediaRequest(ActivityResultContracts.PickVisualMedia.ImageAndVideo)) }) {
                Icon(Icons.Default.PhotoLibrary, null, modifier = Modifier.size(18.dp)); Spacer(Modifier.width(6.dp)); Text("Photos")
            }
            TextButton(onClick = { files.launch(arrayOf("video/*", "audio/*", "image/*")) }) {
                Icon(Icons.Default.Folder, null, modifier = Modifier.size(18.dp)); Spacer(Modifier.width(6.dp)); Text("Files")
            }
        }
        Row(Modifier.horizontalScroll(rememberScrollState()).padding(horizontal = 16.dp), horizontalArrangement = Arrangement.spacedBy(6.dp)) {
            tabs.forEach { (id, name) ->
                Box(
                    Modifier.clip(CircleShape).background(if (tab == id) Color.White else Palette.surface2).clickable { tab = id }
                        .padding(horizontal = 12.dp, vertical = 6.dp),
                ) { Text(name, fontSize = 13.sp, fontWeight = FontWeight.Medium, color = if (tab == id) Color.Black else Palette.ink) }
            }
        }
        if (importing) {
            LinearProgressIndicator(Modifier.fillMaxWidth().padding(16.dp))
        }
        val items: List<LibraryItem> = library?.let {
            when (tab) {
                "backgrounds" -> it.shaderBackgrounds
                "stills" -> it.stills
                "overlays" -> it.overlays
                "music" -> it.music
                "text" -> it.textStyles
                else -> it.footage
            }
        } ?: emptyList()
        LazyVerticalGrid(
            GridCells.Adaptive(140.dp), contentPadding = PaddingValues(16.dp),
            horizontalArrangement = Arrangement.spacedBy(10.dp), verticalArrangement = Arrangement.spacedBy(14.dp),
            modifier = Modifier.heightIn(max = 560.dp),
        ) {
            items(items, key = { "$tab/${it.id}" }) { item ->
                Column(Modifier.clickable {
                    val ok = if (tab == "text") model.add("add_text", mapOf("style" to item.id, "content" to item.name, "start" to model.time))
                    else model.add("add_clip", mapOf("asset" to item.id, "start" to model.time))
                    if (ok) done()
                }) {
                    val m = Modifier.fillMaxWidth().aspectRatio(16f / 9f).clip(RoundedCornerShape(8.dp))
                    if (tab == "music") {
                        Box(m.background(Palette.surface2), contentAlignment = Alignment.Center) {
                            Icon(Icons.Default.MusicNote, null, tint = Palette.muted)
                        }
                    } else {
                        EngineImage("asset:${item.id}", m) { Core.engine.assetThumb(item.id) }
                    }
                    Spacer(Modifier.height(6.dp))
                    Text(item.name, fontSize = 13.sp, fontWeight = FontWeight.Medium, maxLines = 1, overflow = TextOverflow.Ellipsis)
                    item.about?.let { Text(it, fontSize = 11.sp, color = Palette.muted, maxLines = 1, overflow = TextOverflow.Ellipsis) }
                }
            }
        }
    }
}

/** A picked photo, video or sound, copied into Liemo's imports folder under its own name. */
private fun copyIn(context: Context, uri: Uri): File? = runCatching {
    val r = context.contentResolver
    var name = r.query(uri, arrayOf(OpenableColumns.DISPLAY_NAME), null, null, null)?.use { c ->
        if (c.moveToFirst()) c.getString(0) else null
    } ?: "media"
    if (!name.contains('.')) {
        val ext = android.webkit.MimeTypeMap.getSingleton().getExtensionFromMimeType(r.getType(uri)) ?: "mp4"
        name = "$name.$ext"
    }
    val dst = File(Core.mediaDir, "${java.util.UUID.randomUUID().toString().take(8)}-${name.replace('/', '-')}")
    r.openInputStream(uri)!!.use { input -> dst.outputStream().use { input.copyTo(it) } }
    dst
}.getOrNull()

// MARK: Inspector

@Composable
fun InspectorPanel(model: EditorModel) {
    val c = model.selectedClip
    Column(Modifier.verticalScroll(rememberScrollState()).padding(horizontal = 16.dp).padding(bottom = 32.dp)) {
        if (c == null) {
            Column(Modifier.fillMaxWidth().padding(vertical = 60.dp), horizontalAlignment = Alignment.CenterHorizontally) {
                Text("Nothing selected", fontSize = 16.sp, fontWeight = FontWeight.SemiBold)
                Text("Tap a clip on the timeline to edit it.", fontSize = 14.sp, color = Palette.muted)
            }
            return@Column
        }
        // Fresh drafts for another clip.
        key(c.id) {
            Text(c.name, fontSize = 17.sp, fontWeight = FontWeight.SemiBold)
            c.text?.let { TextSection(model, c, it) }
            if (c.isVisual) {
                SectionTitle("Transform")
                Keyed(model, c, "scale", "Scale", c.scale, 0.1f..4f, { "${(it * 100).toInt()}%" }) { model.update(c.id, mapOf("scale" to it)) }
                Keyed(model, c, "x", "X", c.x, -1500f..1500f, { "${it.toInt()}" }) { model.update(c.id, mapOf("x" to it)) }
                Keyed(model, c, "y", "Y", c.y, -1500f..1500f, { "${it.toInt()}" }) { model.update(c.id, mapOf("y" to it)) }
                Keyed(model, c, "rot", "Rotation", c.rot, -180f..180f, { "${it.toInt()}°" }) { model.update(c.id, mapOf("rotation" to it)) }
                Keyed(model, c, "opacity", "Opacity", c.opacity, 0f..1f, { "${(it * 100).toInt()}%" }) { model.update(c.id, mapOf("opacity" to it)) }
                if (c.type != "text") {
                    Toggle("Mirror", c.motion?.flipH ?: false) { model.update(c.id, mapOf("flipH" to it)) }
                    Toggle("Flip upside down", c.motion?.flipV ?: false) { model.update(c.id, mapOf("flipV" to it)) }
                }
            }
            if (c.hasSound) {
                SectionTitle("Sound")
                Keyed(model, c, "volume", "Volume", c.volume, 0f..1f, { "${(it * 100).toInt()}%" }) { model.update(c.id, mapOf("volume" to it)) }
            }
            SectionTitle("Timing")
            SliderRow("Duration", c.dur, 0.2f..maxOf(30f, c.dur.toFloat() * 2), { "%.1fs".format(it) }) { model.update(c.id, mapOf("duration" to it)) }
            val fadeMax = maxOf(0.1f, minOf(5f, c.dur.toFloat() / 2))
            SliderRow("Fade in", c.fadeIn, 0f..fadeMax, { "%.1fs".format(it) }) { model.update(c.id, mapOf("fadeIn" to it)) }
            SliderRow("Fade out", c.fadeOut, 0f..fadeMax, { "%.1fs".format(it) }) { model.update(c.id, mapOf("fadeOut" to it)) }
        }
    }
}

private val swatches = listOf("#ffffff", "#000000", "#ff5577", "#ffd400", "#5fd39a", "#0099ff", "#9b6bff", "#ff8a3d")
private val anims = listOf(
    "none" to "None", "fade" to "Fade", "rise" to "Rise", "slide" to "Slide", "type" to "Type", "caret" to "Typewriter",
    "pop" to "Pop", "bounce" to "Bounce", "zoom" to "Zoom", "settle" to "Settle", "blur" to "Blur", "reveal" to "Mask reveal",
    "wave" to "Wave", "scramble" to "Decode", "words" to "Word by word", "wordpop" to "Word pop", "marker" to "Marker",
    "shimmer" to "Shimmer", "glitch" to "Glitch", "flicker" to "Flicker", "fontswap" to "Font swap",
)

@Composable
private fun TextSection(model: EditorModel, c: Clip, t: TextSpec) {
    fun set(f: Map<String, Any?>) = model.update(c.id, mapOf("text" to f))
    val fonts = remember {
        runCatching { Core.decode<Library>("list_library") }.getOrNull()?.let { l -> l.fonts.map { it.family } + l.installedFonts } ?: emptyList()
    }
    var content by remember { mutableStateOf(t.content) }
    LaunchedEffect(content) {
        kotlinx.coroutines.delay(400)
        if (content != t.content) set(mapOf("content" to content))
    }
    SectionTitle("Text")
    OutlinedTextField(content, { content = it }, modifier = Modifier.fillMaxWidth(), minLines = 1, maxLines = 4)
    var fontOpen by remember { mutableStateOf(false) }
    Box {
        Row(Modifier.fillMaxWidth().clickable { fontOpen = true }.padding(vertical = 12.dp)) {
            Text("Font", modifier = Modifier.weight(1f), fontSize = 14.sp)
            Text(t.font, color = Palette.muted, fontSize = 14.sp)
        }
        DropdownMenu(expanded = fontOpen, onDismissRequest = { fontOpen = false }, containerColor = Palette.surface2, modifier = Modifier.heightIn(max = 420.dp)) {
            fonts.distinct().forEach { f ->
                DropdownMenuItem(text = { Text(f) }, onClick = { fontOpen = false; set(mapOf("font" to f)) })
            }
        }
    }
    Keyed(model, c, "text.size", "Size", t.size, 12f..300f, { "${it.toInt()}" }) { set(mapOf("size" to it)) }
    Text("Colour", fontSize = 14.sp, modifier = Modifier.padding(top = 8.dp, bottom = 8.dp))
    Row(horizontalArrangement = Arrangement.spacedBy(10.dp)) {
        swatches.forEach { s ->
            val on = t.color.equals(s, ignoreCase = true)
            Box(
                Modifier.size(30.dp).clip(CircleShape).background(cssColor(s) ?: Color.White)
                    .border(if (on) 2.dp else 1.dp, if (on) Palette.accent else Palette.border, CircleShape)
                    .clickable { set(mapOf("color" to s)) },
            )
        }
    }
    Text("Animation", fontSize = 14.sp, modifier = Modifier.padding(top = 14.dp, bottom = 8.dp))
    Row(Modifier.horizontalScroll(rememberScrollState()), horizontalArrangement = Arrangement.spacedBy(6.dp)) {
        anims.forEach { (id, name) ->
            val on = t.anim.equals(id, ignoreCase = true)
            Box(
                Modifier.clip(CircleShape).background(if (on) Color.White else Palette.surface2).clickable { set(mapOf("animation" to id)) }
                    .padding(horizontal = 12.dp, vertical = 6.dp),
            ) { Text(name, fontSize = 13.sp, color = if (on) Color.Black else Palette.ink) }
        }
    }
    Toggle("Backing box", t.box) { set(mapOf("box" to it)) }
    Keyed(model, c, "text.tiltX", "3D tilt ↕", t.tiltX ?: 0.0, -70f..70f, { "${it.toInt()}°" }) { set(mapOf("tiltX" to it)) }
    Keyed(model, c, "text.tiltY", "3D tilt ↔", t.tiltY ?: 0.0, -70f..70f, { "${it.toInt()}°" }) { set(mapOf("tiltY" to it)) }
}

/** A slider for a property that can be keyframed: it shows the value at the playhead, and once
 *  the property is animated, moving it sets a key there. The diamond adds or removes one. */
@Composable
private fun Keyed(
    model: EditorModel, c: Clip, prop: String, label: String, base: Double,
    range: ClosedFloatingPointRange<Float>, fmt: (Float) -> String, set: (Double) -> Unit,
) {
    val t = model.time
    val animated = c.keyed(prop)
    SliderRow(label, c.keyedValue(prop, t) ?: base, range, fmt, key = animated to c.hasKey(prop, t), onKey = {
        model.edit("keyframe", mapOf("clip" to c.id, "property" to prop, "time" to t))
    }) { v ->
        if (animated) model.edit("keyframe", mapOf("clip" to c.id, "property" to prop, "time" to model.time, "value" to v))
        else set(v)
    }
}

@Composable
private fun SliderRow(
    label: String, value: Double, range: ClosedFloatingPointRange<Float>, fmt: (Float) -> String,
    key: Pair<Boolean, Boolean>? = null, onKey: (() -> Unit)? = null, commit: (Double) -> Unit,
) {
    var draft by remember(value) { mutableFloatStateOf(value.toFloat().coerceIn(range)) }
    Column(Modifier.padding(vertical = 2.dp)) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            Text(label, fontSize = 14.sp, modifier = Modifier.weight(1f))
            Text(fmt(draft), fontSize = 13.sp, color = Palette.muted)
            if (key != null && onKey != null) {
                val (animated, here) = key
                Text(
                    if (here) "◆" else "◇", fontSize = 16.sp,
                    color = if (animated) Palette.accent else Palette.muted,
                    modifier = Modifier.clickable(onClick = onKey).padding(start = 10.dp, end = 2.dp),
                )
            }
        }
        Slider(
            draft, { draft = it }, valueRange = range, onValueChangeFinished = { commit(draft.toDouble()) },
            colors = SliderDefaults.colors(thumbColor = Color.White, activeTrackColor = Color.White, inactiveTrackColor = Palette.surface3),
            modifier = Modifier.height(32.dp),
        )
    }
}

@Composable
private fun Toggle(label: String, on: Boolean, set: (Boolean) -> Unit) {
    Row(Modifier.fillMaxWidth().padding(vertical = 4.dp), verticalAlignment = Alignment.CenterVertically) {
        Text(label, fontSize = 14.sp, modifier = Modifier.weight(1f))
        Switch(on, set)
    }
}

// MARK: Export

@Composable
fun ExportPanel(model: EditorModel) {
    val context = LocalContext.current
    val scope = rememberCoroutineScope()
    val doc = model.doc
    var settings by remember { mutableStateOf(ExportSettings(resolution = doc?.exportResolution ?: 1080, fps = doc?.fps ?: 30)) }
    var progress by remember { mutableStateOf<Double?>(null) }
    var result by remember { mutableStateOf<File?>(null) }
    var failure by remember { mutableStateOf<String?>(null) }
    var exporter by remember { mutableStateOf<Exporter?>(null) }

    Column(Modifier.padding(20.dp).padding(bottom = 24.dp), verticalArrangement = Arrangement.spacedBy(18.dp)) {
        Text("Export", fontSize = 17.sp, fontWeight = FontWeight.SemiBold)
        val p = progress
        val done = result
        when {
            p != null -> {
                LinearProgressIndicator(progress = { p.toFloat() }, modifier = Modifier.fillMaxWidth())
                Row(verticalAlignment = Alignment.CenterVertically) {
                    Text("Rendering ${(p * 100).toInt()}%", fontSize = 13.sp, color = Palette.muted, modifier = Modifier.weight(1f))
                    TextButton(onClick = { exporter?.cancel() }) { Text("Cancel") }
                }
            }
            done != null -> {
                Row(verticalAlignment = Alignment.CenterVertically) {
                    Icon(Icons.Default.CheckCircle, null, tint = Palette.wave)
                    Spacer(Modifier.width(8.dp))
                    Text("Saved to Movies › Liemo", color = Palette.wave)
                }
                Row {
                    TextButton(onClick = {
                        val uri = FileProvider.getUriForFile(context, "${context.packageName}.files", done)
                        val send = Intent(Intent.ACTION_SEND).setType("video/mp4").putExtra(Intent.EXTRA_STREAM, uri).addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
                        context.startActivity(Intent.createChooser(send, "Share video"))
                    }) { Text("Share") }
                    Spacer(Modifier.weight(1f))
                    TextButton(onClick = { result = null }) { Text("Export again") }
                }
            }
            else -> {
                Segmented(listOf(720 to "720p", 1080 to "1080p", 2160 to "4K"), settings.resolution) { settings = settings.copy(resolution = it) }
                Segmented(listOf(24 to "24 fps", 30 to "30 fps", 60 to "60 fps"), settings.fps) { settings = settings.copy(fps = it) }
                Text("${doc?.aspect ?: ""} · ${formatTime(model.duration)} · MP4, saved to Movies", fontSize = 13.sp, color = Palette.muted)
                failure?.let { Text(it, fontSize = 13.sp, color = Palette.danger) }
                Box(
                    Modifier.fillMaxWidth().height(44.dp).clip(RoundedCornerShape(10.dp)).background(Color.White).clickable {
                        val d = model.doc ?: return@clickable
                        model.pause()
                        failure = null
                        val ex = Exporter()
                        exporter = ex
                        progress = 0.0
                        scope.launch {
                            val r = withContext(Dispatchers.Default) {
                                runCatching { ex.run(context, d, settings) { v -> scope.launch { progress = v } } }
                            }
                            progress = null
                            exporter = null
                            r.onSuccess { result = it }.onFailure { failure = it.message }
                        }
                    },
                    contentAlignment = Alignment.Center,
                ) { Text("Export video", color = Color.Black, fontSize = 15.sp, fontWeight = FontWeight.SemiBold) }
            }
        }
    }
}

@Composable
private fun Segmented(options: List<Pair<Int, String>>, value: Int, set: (Int) -> Unit) {
    SingleChoiceSegmentedButtonRow(Modifier.fillMaxWidth()) {
        options.forEachIndexed { i, (v, name) ->
            SegmentedButton(
                selected = v == value, onClick = { set(v) }, shape = SegmentedButtonDefaults.itemShape(i, options.size),
                colors = SegmentedButtonDefaults.colors(activeContainerColor = Palette.surface3, inactiveContainerColor = Palette.surface1),
            ) { Text(name, fontSize = 13.sp) }
        }
    }
}
