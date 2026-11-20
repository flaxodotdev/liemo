package dev.flaxo.liemo.ui

import android.graphics.BitmapFactory
import androidx.compose.foundation.ExperimentalFoundationApi
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.combinedClickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.grid.GridCells
import androidx.compose.foundation.lazy.grid.GridItemSpan
import androidx.compose.foundation.lazy.grid.LazyVerticalGrid
import androidx.compose.foundation.lazy.grid.items
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Add
import androidx.compose.material.icons.filled.AutoAwesome
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import dev.flaxo.liemo.core.Core
import dev.flaxo.liemo.core.Created
import dev.flaxo.liemo.core.FormatDef
import dev.flaxo.liemo.core.FormatList
import dev.flaxo.liemo.core.ProjectList
import dev.flaxo.liemo.core.ProjectSummary
import dev.flaxo.liemo.core.formatTime
import java.io.File

@Composable
fun ProjectsScreen(onOpen: (String) -> Unit, onReel: () -> Unit) {
    var projects by remember { mutableStateOf<List<ProjectSummary>>(emptyList()) }
    var stamp by remember { mutableIntStateOf(0) }
    var creating by remember { mutableStateOf(false) }
    var renaming by remember { mutableStateOf<ProjectSummary?>(null) }
    fun refresh() {
        projects = runCatching { Core.decode<ProjectList>("list_projects").projects }.getOrDefault(emptyList())
        stamp++
    }
    LaunchedEffect(Unit) { refresh() }

    LazyVerticalGrid(
        columns = GridCells.Adaptive(160.dp),
        contentPadding = PaddingValues(start = 16.dp, end = 16.dp, bottom = 32.dp),
        horizontalArrangement = Arrangement.spacedBy(14.dp),
        verticalArrangement = Arrangement.spacedBy(18.dp),
        modifier = Modifier.fillMaxSize(),
    ) {
        item(span = { GridItemSpan(maxLineSpan) }) {
            Row(Modifier.padding(top = 12.dp, bottom = 4.dp), verticalAlignment = Alignment.CenterVertically) {
                LiemoMark(26.dp)
                Spacer(Modifier.width(10.dp))
                Text("Liemo", fontSize = 22.sp, fontWeight = FontWeight.SemiBold)
                Spacer(Modifier.weight(1f))
                Row(
                    Modifier.clip(CircleShape).background(Palette.surface2).clickable(onClick = onReel)
                        .padding(horizontal = 14.dp).height(36.dp),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    Icon(Icons.Default.AutoAwesome, null, modifier = Modifier.size(17.dp))
                    Spacer(Modifier.width(6.dp))
                    Text("Quick reel", fontSize = 15.sp, fontWeight = FontWeight.SemiBold)
                }
                Spacer(Modifier.width(8.dp))
                Row(
                    Modifier.clip(CircleShape).background(Color.White).clickable { creating = true }
                        .padding(horizontal = 14.dp).height(36.dp),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    Icon(Icons.Default.Add, null, tint = Color.Black, modifier = Modifier.size(18.dp))
                    Spacer(Modifier.width(4.dp))
                    Text("New", color = Color.Black, fontSize = 15.sp, fontWeight = FontWeight.SemiBold)
                }
            }
        }
        if (projects.isEmpty()) {
            item(span = { GridItemSpan(maxLineSpan) }) {
                Column(Modifier.fillMaxWidth().padding(vertical = 80.dp), horizontalAlignment = Alignment.CenterHorizontally) {
                    Text("No projects yet", fontSize = 20.sp, fontWeight = FontWeight.SemiBold)
                    Spacer(Modifier.height(14.dp))
                    Text(
                        "Start from a format, or open the sample reel to see what Liemo does.",
                        color = Palette.muted, fontSize = 15.sp, textAlign = TextAlign.Center,
                    )
                    Spacer(Modifier.height(14.dp))
                    OutlinedButton(onClick = {
                        runCatching { Core.decode<Created>("create_project", mapOf("template" to "sample")) }.getOrNull()?.let { onOpen(it.id) }
                    }) { Text("Open the sample") }
                }
            }
        }
        items(projects, key = { it.id }) { p ->
            ProjectCard(
                p, stamp,
                onOpen = { onOpen(p.id) },
                onRename = { renaming = p },
                onDuplicate = { Core.engine.duplicateProject(p.id); refresh() },
                onDelete = { Core.engine.deleteProject(p.id); refresh() },
            )
        }
    }

    if (creating) {
        NewProjectSheet { id ->
            creating = false
            refresh()
            if (id != null) onOpen(id)
        }
    }
    renaming?.let { p ->
        RenameDialog(p.name, onDismiss = { renaming = null }) { name ->
            Core.call("set_project", mapOf("project" to p.id, "name" to name))
            renaming = null
            refresh()
        }
    }
}

@OptIn(ExperimentalFoundationApi::class)
@Composable
private fun ProjectCard(
    p: ProjectSummary,
    stamp: Int,
    onOpen: () -> Unit,
    onRename: () -> Unit,
    onDuplicate: () -> Unit,
    onDelete: () -> Unit,
) {
    var menu by remember { mutableStateOf(false) }
    val thumb = remember(p.id, stamp) {
        val f = File(Core.engine.projectThumbPath(p.id))
        if (f.exists()) BitmapFactory.decodeFile(f.path)?.asImageBitmap() else null
    }
    Column(Modifier.combinedClickable(onClick = onOpen, onLongClick = { menu = true })) {
        // Every card is the same box; the picture inside keeps the project's real shape.
        Box(
            Modifier.fillMaxWidth().aspectRatio(4f / 3f).clip(RoundedCornerShape(10.dp))
                .background(Palette.surface2).border(1.dp, Palette.border, RoundedCornerShape(10.dp)),
            contentAlignment = Alignment.Center,
        ) {
            if (thumb != null) {
                Image(thumb, null, modifier = Modifier.padding(8.dp).clip(RoundedCornerShape(4.dp)))
            } else {
                Box(Modifier.alpha(0.5f)) { LiemoMark(28.dp) }
            }
        }
        Spacer(Modifier.height(8.dp))
        Text(p.name, fontSize = 15.sp, fontWeight = FontWeight.Medium, maxLines = 1, overflow = TextOverflow.Ellipsis)
        Text(
            "${p.aspect} · ${formatTime(p.duration)} · ${p.edited}",
            fontSize = 12.sp, color = Palette.muted, maxLines = 1, overflow = TextOverflow.Ellipsis,
        )
        DropdownMenu(expanded = menu, onDismissRequest = { menu = false }) {
            DropdownMenuItem(text = { Text("Rename") }, onClick = { menu = false; onRename() })
            DropdownMenuItem(text = { Text("Duplicate") }, onClick = { menu = false; onDuplicate() })
            DropdownMenuItem(text = { Text("Delete", color = Palette.danger) }, onClick = { menu = false; onDelete() })
        }
    }
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun NewProjectSheet(done: (String?) -> Unit) {
    val formats = remember { runCatching { Core.decode<FormatList>("list_formats").formats }.getOrDefault(emptyList()) }
    fun create(args: Map<String, Any?>) = done(runCatching { Core.decode<Created>("create_project", args).id }.getOrNull())
    ModalBottomSheet(onDismissRequest = { done(null) }, containerColor = Palette.surface1) {
        LazyColumn(Modifier.padding(bottom = 24.dp)) {
            item {
                Text("New project", fontSize = 17.sp, fontWeight = FontWeight.SemiBold, modifier = Modifier.padding(16.dp))
                FormatRow("Nature reel", "The sample: footage, titles, music, effects", 16.0, 9.0) { create(mapOf("template" to "sample")) }
                Text("FORMATS", fontSize = 11.sp, fontWeight = FontWeight.SemiBold, color = Palette.muted, modifier = Modifier.padding(start = 16.dp, top = 16.dp, bottom = 4.dp))
            }
            items(formats) { f: FormatDef ->
                FormatRow(f.name, "${f.aspect} · ${f.bestFor}", f.canvas.firstOrNull() ?: 16.0, f.canvas.lastOrNull() ?: 9.0) {
                    create(mapOf("format" to f.id))
                }
            }
        }
    }
}

@Composable
private fun FormatRow(title: String, sub: String, w: Double, h: Double, onClick: () -> Unit) {
    Row(
        Modifier.fillMaxWidth().clickable(onClick = onClick).padding(horizontal = 16.dp, vertical = 10.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Box(Modifier.size(30.dp), contentAlignment = Alignment.Center) {
            val k = 26f / maxOf(w, h).toFloat()
            Box(Modifier.size((w.toFloat() * k).dp, (h.toFloat() * k).dp).border(1.5.dp, Palette.muted, RoundedCornerShape(3.dp)))
        }
        Spacer(Modifier.width(14.dp))
        Column {
            Text(title, color = Palette.ink, fontSize = 15.sp)
            Text(sub, color = Palette.muted, fontSize = 13.sp)
        }
    }
}

@Composable
fun RenameDialog(current: String, onDismiss: () -> Unit, onRename: (String) -> Unit) {
    var name by remember { mutableStateOf(current) }
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text("Rename project") },
        text = { OutlinedTextField(name, { name = it }, singleLine = true) },
        confirmButton = { TextButton(onClick = { onRename(name) }) { Text("Rename") } },
        dismissButton = { TextButton(onClick = onDismiss) { Text("Cancel") } },
        containerColor = Palette.surface2,
    )
}
