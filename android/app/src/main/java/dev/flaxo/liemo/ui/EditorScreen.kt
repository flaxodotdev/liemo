package dev.flaxo.liemo.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.gestures.detectDragGestures
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material.icons.automirrored.filled.Redo
import androidx.compose.material.icons.automirrored.filled.Undo
import androidx.compose.material.icons.filled.AcUnit
import androidx.compose.material.icons.filled.Check
import androidx.compose.material.icons.filled.ContentCopy
import androidx.compose.material.icons.filled.ContentCut
import androidx.compose.material.icons.filled.Delete
import androidx.compose.material.icons.filled.Flip
import androidx.compose.material.icons.filled.MusicNote
import androidx.compose.material.icons.filled.Pause
import androidx.compose.material.icons.filled.PhotoLibrary
import androidx.compose.material.icons.filled.PlayArrow
import androidx.compose.material.icons.filled.Tune
import androidx.compose.material.icons.filled.SwapHoriz
import androidx.compose.material.icons.filled.TextFields
import androidx.compose.material.icons.filled.KeyboardArrowDown
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.rememberModalBottomSheetState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import dev.flaxo.liemo.core.EditorModel
import dev.flaxo.liemo.core.TrimMode
import dev.flaxo.liemo.core.formatTime

enum class Panel { Library, Audio, Inspector, Export }

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun EditorScreen(id: String, onClose: () -> Unit) {
    val scope = rememberCoroutineScope()
    val model = remember(id) { EditorModel(id, scope) }
    var sheet by remember { mutableStateOf<Panel?>(null) }
    var renaming by remember { mutableStateOf(false) }
    // The preview's height; drag the grip under the transport to change it.
    var previewHeight by remember { mutableFloatStateOf(360f) }
    DisposableEffect(model) {
        model.open()
        onDispose { model.close() }
    }

    BoxWithConstraints(Modifier.fillMaxSize()) {
        // Bars, transport, grip and toolbar always show; the preview takes what's asked of it
        // up to what leaves the timeline room, and the timeline gets the rest.
        val room = maxOf(160f, maxHeight.value - (48 + 52 + 18 + 64 + 200))
        Column(Modifier.fillMaxSize()) {
            TopBar(model, onClose, rename = { renaming = true }, export = { sheet = Panel.Export })
            PreviewStage(model, editText = { sheet = Panel.Inspector }, modifier = Modifier.fillMaxWidth().height(minOf(previewHeight, room).dp))
            Transport(model)
            Grip(onDrag = { previewHeight = (minOf(previewHeight, room) + it).coerceIn(160f, room) }) {
                previewHeight = if (previewHeight < room - 40) room else 360f
            }
            Timeline(model, addMedia = { sheet = Panel.Library }, addAudio = { sheet = Panel.Audio }, modifier = Modifier.fillMaxWidth().weight(1f))
            Toolbar(model) { sheet = it }
        }
    }

    sheet?.let { p ->
        val state = rememberModalBottomSheetState(skipPartiallyExpanded = p == Panel.Export)
        ModalBottomSheet(
            onDismissRequest = { sheet = null }, sheetState = state,
            containerColor = Palette.surface1, scrimColor = Color.Black.copy(alpha = 0.3f),
        ) {
            val close = { sheet = null }
            when (p) {
                Panel.Library -> LibraryPanel(model, "footage", close)
                Panel.Audio -> LibraryPanel(model, "music", close)
                Panel.Inspector -> InspectorPanel(model)
                Panel.Export -> ExportPanel(model)
            }
        }
    }
    model.error?.let { e ->
        AlertDialog(
            onDismissRequest = { model.error = null }, title = { Text("Liemo") }, text = { Text(e) },
            confirmButton = { TextButton(onClick = { model.error = null }) { Text("OK") } },
            containerColor = Palette.surface2,
        )
    }
    if (renaming) {
        RenameDialog(model.doc?.name ?: "", onDismiss = { renaming = false }) {
            model.edit("set_project", mapOf("name" to it))
            renaming = false
        }
    }
}

@Composable
private fun TopBar(model: EditorModel, onClose: () -> Unit, rename: () -> Unit, export: () -> Unit) {
    var menu by remember { mutableStateOf(false) }
    Row(Modifier.fillMaxWidth().height(48.dp).padding(horizontal = 6.dp), verticalAlignment = Alignment.CenterVertically) {
        IconButton(onClick = onClose) { Icon(Icons.AutoMirrored.Filled.ArrowBack, "Back") }
        Box(Modifier.weight(1f)) {
            Row(Modifier.clickable { menu = true }.padding(vertical = 6.dp), verticalAlignment = Alignment.CenterVertically) {
                Text(model.doc?.name ?: "", fontSize = 16.sp, fontWeight = FontWeight.SemiBold, maxLines = 1, overflow = TextOverflow.Ellipsis, modifier = Modifier.widthIn(max = 200.dp))
                Icon(Icons.Default.KeyboardArrowDown, null, tint = Palette.muted, modifier = Modifier.size(18.dp))
            }
            DropdownMenu(expanded = menu, onDismissRequest = { menu = false }, containerColor = Palette.surface2) {
                DropdownMenuItem(text = { Text("Rename") }, onClick = { menu = false; rename() })
                model.doc?.let { d ->
                    Text("${d.format} · ${d.aspect} · ${d.fps} fps", color = Palette.muted, fontSize = 13.sp, modifier = Modifier.padding(horizontal = 16.dp, vertical = 8.dp))
                }
            }
        }
        val res = model.doc?.exportResolution ?: 1080
        Text(
            if (res < 1000) "SD" else if (res >= 2000) "4K" else "HD", color = Palette.muted, fontSize = 13.sp,
            fontWeight = FontWeight.SemiBold, modifier = Modifier.clickable(onClick = export).padding(8.dp),
        )
        Box(
            Modifier.clip(CircleShape).background(Color.White).clickable(onClick = export).padding(horizontal = 14.dp, vertical = 7.dp),
        ) { Text("Export", color = Color.Black, fontSize = 14.sp, fontWeight = FontWeight.SemiBold) }
    }
}

@Composable
private fun Transport(model: EditorModel) {
    Row(Modifier.fillMaxWidth().height(52.dp).padding(horizontal = 8.dp), verticalAlignment = Alignment.CenterVertically) {
        Box(Modifier.weight(1f)) {
            IconButton(onClick = { model.togglePlay() }) {
                Icon(if (model.playing) Icons.Default.Pause else Icons.Default.PlayArrow, if (model.playing) "Pause" else "Play", modifier = Modifier.size(28.dp))
            }
        }
        Column(horizontalAlignment = Alignment.CenterHorizontally) {
            Text(formatTime(model.time), fontSize = 13.sp, fontWeight = FontWeight.Medium, fontFamily = FontFamily.Monospace)
            Text(formatTime(model.duration), fontSize = 12.sp, color = Palette.muted, fontFamily = FontFamily.Monospace)
        }
        Row(Modifier.weight(1f), horizontalArrangement = Arrangement.End) {
            IconButton(onClick = { model.undo() }, enabled = model.canUndo) { Icon(Icons.AutoMirrored.Filled.Undo, "Undo") }
            IconButton(onClick = { model.redo() }, enabled = model.canRedo) { Icon(Icons.AutoMirrored.Filled.Redo, "Redo") }
        }
    }
}

/** Drag to make the preview bigger (a tall reel) or smaller (more timeline); double-tap to
 *  switch between big and normal. */
@Composable
private fun Grip(onDrag: (Float) -> Unit, onDoubleTap: () -> Unit) {
    val density = LocalDensity.current.density
    var last by remember { mutableStateOf(0L) }
    Box(
        Modifier.fillMaxWidth().height(18.dp)
            .pointerInput(Unit) { detectDragGestures { _, d -> onDrag(d.y / density) } }
            .clickable(indication = null, interactionSource = null) {
                val now = System.currentTimeMillis()
                if (now - last < 350) onDoubleTap()
                last = now
            },
        contentAlignment = Alignment.Center,
    ) { Box(Modifier.size(40.dp, 5.dp).clip(CircleShape).background(Palette.surface3)) }
}

@Composable
private fun Toolbar(model: EditorModel, show: (Panel) -> Unit) {
    Row(
        Modifier.fillMaxWidth().height(64.dp).background(Palette.surface1).horizontalScroll(rememberScrollState()).padding(horizontal = 8.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        val c = model.selectedClip
        if (c != null) {
            Tool(if (c.type == "text") "Edit text" else "Edit", Icons.Default.Tune) { show(Panel.Inspector) }
            Tool("Split", Icons.Default.ContentCut) { model.edit("split_clip", mapOf("clip" to c.id, "time" to model.time)) }
            TrimTool(model)
            if (c.isVisual && c.type != "text") {
                Tool("Mirror", Icons.Default.Flip) { model.update(c.id, mapOf("flipH" to !(c.motion?.flipH ?: false))) }
                Tool("Freeze", Icons.Default.AcUnit) { model.freeze(c) }
            }
            Tool("Duplicate", Icons.Default.ContentCopy) { model.edit("duplicate_clip", mapOf("clip" to c.id)) }
            Tool("Delete", Icons.Default.Delete) { model.edit("remove_clip", mapOf("clip" to c.id)) }
            Tool("Done", Icons.Default.Check) { model.selected = null }
        } else {
            Tool("Media", Icons.Default.PhotoLibrary) { show(Panel.Library) }
            Tool("Audio", Icons.Default.MusicNote) { show(Panel.Audio) }
            Tool("Text", Icons.Default.TextFields) {
                if (model.add("add_text", mapOf("content" to "Your title", "start" to model.time))) show(Panel.Inspector)
            }
            Tool("Split", Icons.Default.ContentCut) { model.edit("split_clip", mapOf("time" to model.time)) }
        }
    }
}

/** What a drag on the selected clip does: move and trim, or Resolve's ripple, roll, slip, slide. */
@Composable
private fun TrimTool(model: EditorModel) {
    var open by remember { mutableStateOf(false) }
    Box {
        Tool(
            if (model.trimMode == TrimMode.Normal) "Trim" else model.trimMode.label, Icons.Default.SwapHoriz,
            tint = if (model.trimMode == TrimMode.Normal) Palette.ink else Palette.accent,
        ) { open = true }
        DropdownMenu(expanded = open, onDismissRequest = { open = false }, containerColor = Palette.surface2) {
            TrimMode.entries.forEach { m ->
                DropdownMenuItem(
                    text = {
                        Column {
                            Text(m.label, color = if (m == model.trimMode) Palette.accent else Palette.ink)
                            Text(m.about, color = Palette.muted, fontSize = 12.sp)
                        }
                    },
                    onClick = { model.trimMode = m; open = false },
                )
            }
        }
    }
}

@Composable
private fun Tool(title: String, icon: ImageVector, tint: Color = Palette.ink, enabled: Boolean = true, action: () -> Unit) {
    Column(
        Modifier.widthIn(min = 64.dp).height(56.dp).clip(androidx.compose.foundation.shape.RoundedCornerShape(8.dp))
            .clickable(enabled = enabled, onClick = action).alpha(if (enabled) 1f else 0.4f).padding(horizontal = 6.dp),
        horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.Center,
    ) {
        Icon(icon, null, tint = tint, modifier = Modifier.size(20.dp))
        Spacer(Modifier.height(4.dp))
        Text(title, fontSize = 11.sp, color = tint, maxLines = 1)
    }
}

@Composable
fun SectionTitle(s: String) {
    Text(s.uppercase(), fontSize = 11.sp, fontWeight = FontWeight.SemiBold, color = Palette.muted, modifier = Modifier.padding(top = 8.dp, bottom = 4.dp))
}

@Composable
fun Spacer8() = Spacer(Modifier.width(8.dp))
