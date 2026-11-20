package dev.flaxo.liemo.ui

import android.content.Context
import android.media.MediaMetadataRetriever
import android.media.MediaPlayer
import android.net.Uri
import android.provider.OpenableColumns
import androidx.activity.compose.BackHandler
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.PickVisualMediaRequest
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.ExperimentalFoundationApi
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.combinedClickable
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material.icons.filled.Add
import androidx.compose.material.icons.filled.AutoAwesome
import androidx.compose.material.icons.filled.Check
import androidx.compose.material.icons.filled.Movie
import androidx.compose.material.icons.filled.MusicNote
import androidx.compose.material.icons.filled.PlayArrow
import androidx.compose.material.icons.filled.Schedule
import androidx.compose.material.icons.filled.Stop
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
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
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableDoubleStateOf
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import dev.flaxo.liemo.core.Core
import dev.flaxo.liemo.reel.CaptionAlign
import dev.flaxo.liemo.reel.CaptionPlacement
import dev.flaxo.liemo.reel.CaptionSize
import dev.flaxo.liemo.reel.CaptionStyle
import dev.flaxo.liemo.reel.LyricLine
import dev.flaxo.liemo.reel.ReelBuilder
import dev.flaxo.liemo.reel.ReelPlan
import dev.flaxo.liemo.reel.SavedSong
import dev.flaxo.liemo.reel.Song
import dev.flaxo.liemo.reel.SongLibrary
import dev.flaxo.liemo.reel.Vibe
import dev.flaxo.liemo.reel.captionFonts
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import java.io.File

/** The song a reel uses: one of Liemo's, or one you brought in. */
private sealed class Pick {
    abstract val id: String
    abstract val name: String
    abstract fun path(context: Context): String?

    data class Bundled(val song: Song) : Pick() {
        override val id get() = song.id
        override val name get() = song.title
        override fun path(context: Context) = song.file(context).path
    }

    data class Saved(val song: SavedSong) : Pick() {
        override val id get() = song.id
        override val name get() = song.title
        override fun path(context: Context) = song.path.takeIf { it.exists() }?.path
    }
}

/**
 * Pick footage, a song and a vibe, add some words: Liemo cuts the reel. Songs you bring in get
 * their real lyrics from the web, so the words land on their moments.
 */
@OptIn(ExperimentalFoundationApi::class)
@Composable
fun QuickReelScreen(done: (String?) -> Unit) {
    val context = LocalContext.current
    val scope = rememberCoroutineScope()
    var plan by remember { mutableStateOf(ReelPlan()) }
    var thumbs by remember { mutableStateOf<List<ImageBitmap?>>(emptyList()) }
    var loadingMedia by remember { mutableStateOf(false) }
    var pick by remember { mutableStateOf<Pick?>(Pick.Bundled(Song.library.first())) }
    var progress by remember { mutableStateOf<String?>(null) }
    var failure by remember { mutableStateOf<String?>(null) }
    var editing by remember { mutableStateOf<SavedSong?>(null) }
    var trimming by remember { mutableStateOf<Int?>(null) }
    // Where the song starts: 0 the drop, 1 the start, 2 a moment you pick.
    var startChoice by remember { mutableIntStateOf(0) }
    var customStart by remember { mutableDoubleStateOf(0.0) }
    var songDuration by remember { mutableDoubleStateOf(0.0) }
    val player = remember { mutableStateOf<MediaPlayer?>(null) }
    var playing by remember { mutableStateOf<String?>(null) }

    fun stopPreview() {
        player.value?.release()
        player.value = null
        playing = null
    }
    DisposableEffect(Unit) { onDispose { stopPreview() } }
    BackHandler(enabled = progress == null) { stopPreview(); done(null) }

    fun play(key: String, path: String?, at: Double) {
        if (playing == key) { stopPreview(); return }
        stopPreview()
        path ?: return
        runCatching {
            player.value = MediaPlayer().apply {
                setDataSource(path)
                prepare()
                seekTo((at * 1000).toInt().coerceAtMost(maxOf(0, duration - 1000)))
                start()
            }
            playing = key
        }
    }

    fun fillWords() {
        (pick as? Pick.Saved)?.let { plan = plan.copy(words = it.song.plainText) }
    }

    LaunchedEffect(pick?.id) {
        startChoice = 0
        customStart = 0.0
        songDuration = withContext(Dispatchers.IO) {
            pick?.path(context)?.let { p ->
                val r = MediaMetadataRetriever()
                try { r.setDataSource(p); (r.extractMetadata(MediaMetadataRetriever.METADATA_KEY_DURATION)?.toLongOrNull() ?: 0) / 1000.0 }
                catch (_: Exception) { 0.0 } finally { r.release() }
            } ?: 0.0
        }
    }

    /** A song you brought in (a sound file, or the sound of a video): named, given lyrics, kept. */
    fun own(uri: Uri) {
        stopPreview()
        progress = "Finding the lyrics…"
        scope.launch {
            val saved = withContext(Dispatchers.IO) {
                val (file, display) = copyToCache(context, uri) ?: return@withContext null
                val (title, artist) = SongLibrary.nameOf(file, display)
                val dur = runCatching {
                    val r = MediaMetadataRetriever(); r.setDataSource(file.path)
                    (r.extractMetadata(MediaMetadataRetriever.METADATA_KEY_DURATION)?.toLongOrNull() ?: 0).also { r.release() } / 1000.0
                }.getOrNull()
                val lyrics = SongLibrary.fetchLyrics(artist, title, dur)
                SongLibrary.add(file, title, artist, lyrics.synced, lyrics.plain).also { file.delete() }
            }
            progress = null
            if (saved == null) failure = "That song couldn't be read."
            else {
                pick = Pick.Saved(saved)
                fillWords()
                // Names from tags are often wrong or missing: let them check it.
                if (!saved.hasSynced && saved.plain.isNullOrEmpty()) editing = saved
            }
        }
    }

    val footage = rememberLauncherForActivityResult(ActivityResultContracts.PickMultipleVisualMedia(30)) { uris ->
        if (uris.isEmpty()) return@rememberLauncherForActivityResult
        loadingMedia = true
        scope.launch {
            val got = withContext(Dispatchers.IO) {
                uris.mapNotNull { u -> copyToCache(context, u, Core.mediaDir)?.first }.map { it.path to thumbOf(it) }
            }
            plan = plan.copy(media = got.map { it.first }, videoStarts = List(got.size) { null })
            thumbs = got.map { it.second }
            loadingMedia = false
        }
    }
    val songFile = rememberLauncherForActivityResult(ActivityResultContracts.OpenDocument()) { it?.let(::own) }
    val songVideo = rememberLauncherForActivityResult(ActivityResultContracts.PickVisualMedia()) { it?.let(::own) }

    fun make() {
        stopPreview()
        failure = null
        val p = pick ?: return
        val path = p.path(context) ?: return
        val start = when (startChoice) { 1 -> 0.0; 2 -> customStart; else -> null }
        val lyrics = (p as? Pick.Saved)?.song?.let { s -> s.lines.map { LyricLine(it.time + s.offset, it.text, it.words?.map { w -> w.copy(time = w.time + s.offset) }) } } ?: emptyList()
        val full = plan.copy(song = path, songName = p.name, songStart = start, lyrics = lyrics)
        progress = "Starting…"
        scope.launch {
            val r = withContext(Dispatchers.Default) {
                runCatching { ReelBuilder().build(full) { s -> scope.launch { progress = s } } }
            }
            progress = null
            r.onSuccess { done(it) }.onFailure { failure = it.message }
        }
    }

    val ready = pick != null && plan.media.isNotEmpty() && !loadingMedia

    Box(Modifier.fillMaxSize()) {
        Column(Modifier.fillMaxSize()) {
            Row(Modifier.fillMaxWidth().height(52.dp).padding(horizontal = 4.dp), verticalAlignment = Alignment.CenterVertically) {
                IconButton(onClick = { stopPreview(); done(null) }) { Icon(Icons.AutoMirrored.Filled.ArrowBack, "Cancel") }
                Text("Quick reel", fontSize = 17.sp, fontWeight = FontWeight.SemiBold)
            }
            Column(Modifier.weight(1f).verticalScroll(rememberScrollState()).padding(16.dp), verticalArrangement = Arrangement.spacedBy(26.dp)) {
                // Footage
                Column(verticalArrangement = Arrangement.spacedBy(12.dp)) {
                    Title("Footage", "In the order they'll play · tap a video to choose where it starts")
                    Row(Modifier.horizontalScroll(rememberScrollState()), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                        Column(
                            Modifier.size(72.dp, 110.dp).clip(RoundedCornerShape(10.dp)).background(Palette.surface2)
                                .clickable { footage.launch(PickVisualMediaRequest(ActivityResultContracts.PickVisualMedia.ImageAndVideo)) },
                            horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.Center,
                        ) {
                            if (loadingMedia) CircularProgressIndicator(Modifier.size(20.dp), strokeWidth = 2.dp, color = Palette.ink)
                            else Icon(Icons.Default.Add, null)
                            Spacer(Modifier.height(6.dp))
                            Text(if (plan.media.isEmpty()) "Add" else "Change", fontSize = 12.sp)
                        }
                        thumbs.forEachIndexed { i, img ->
                            Box(Modifier.size(62.dp, 110.dp).clip(RoundedCornerShape(10.dp)).background(Palette.surface3).clickable { trimming = i }) {
                                img?.let { Image(it, null, contentScale = ContentScale.Crop, modifier = Modifier.fillMaxSize()) }
                                if (plan.videoStarts.getOrNull(i) != null) {
                                    Icon(
                                        Icons.Default.Schedule, null, tint = Color.White,
                                        modifier = Modifier.align(Alignment.BottomEnd).padding(4.dp).size(16.dp).background(Color.Black.copy(alpha = 0.65f), CircleShape).padding(2.dp),
                                    )
                                }
                            }
                        }
                    }
                }

                // Song
                Column(verticalArrangement = Arrangement.spacedBy(10.dp)) {
                    Title("Song", "Yours pick up their lyrics from the web")
                    Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                        Pill("Your song", false, Icons.Default.MusicNote) { songFile.launch(arrayOf("audio/*")) }
                        Pill("From a video", false, Icons.Default.Movie) {
                            songVideo.launch(PickVisualMediaRequest(ActivityResultContracts.PickVisualMedia.VideoOnly))
                        }
                    }
                    if (SongLibrary.songs.isNotEmpty()) {
                        Column(Modifier.clip(RoundedCornerShape(12.dp)).background(Palette.surface1)) {
                            SongLibrary.songs.forEach { s ->
                                var menu by remember(s.id) { mutableStateOf(false) }
                                Box {
                                    SongRow(
                                        s.title, s.artist.ifEmpty { "Your song" }, pick?.id == s.id, playing == s.id,
                                        onPlay = { pick = Pick.Saved(s); fillWords(); play(s.id, s.path.path, 30.0) },
                                        onSelect = { pick = Pick.Saved(s); fillWords() },
                                        onLong = { menu = true },
                                    )
                                    DropdownMenu(menu, { menu = false }, containerColor = Palette.surface2) {
                                        DropdownMenuItem(text = { Text("Edit name and lyrics") }, onClick = { menu = false; editing = s })
                                        DropdownMenuItem(text = { Text("Remove", color = Palette.danger) }, onClick = {
                                            menu = false
                                            if (pick?.id == s.id) pick = Pick.Bundled(Song.library.first())
                                            SongLibrary.remove(s)
                                        })
                                    }
                                }
                            }
                        }
                    }
                    Column(Modifier.clip(RoundedCornerShape(12.dp)).background(Palette.surface1)) {
                        Song.library.forEach { s ->
                            SongRow(
                                s.title, s.mood, pick?.id == s.id, playing == s.id,
                                onPlay = { pick = Pick.Bundled(s); play(s.id, s.file(context).path, 30.0) },
                                onSelect = { pick = Pick.Bundled(s) },
                            )
                        }
                    }
                    when (val p = pick) {
                        is Pick.Bundled -> Text("Credit when you post: ${p.song.credit}", fontSize = 12.sp, color = Palette.muted)
                        is Pick.Saved -> Row(verticalAlignment = Alignment.CenterVertically) {
                            val s = p.song
                            Text(
                                when {
                                    s.wordSynced -> "Word-by-word synced"
                                    s.hasSynced && s.offset != 0.0 -> "Synced · timing fixed (%+.1fs)".format(s.offset)
                                    s.hasSynced -> "Lyrics synced to the music"
                                    s.plainText.isEmpty() -> "No lyrics yet"
                                    else -> "Lyrics on the beat"
                                },
                                fontSize = 12.sp, color = Palette.muted, modifier = Modifier.weight(1f),
                            )
                            if (!s.hasSynced) {
                                TextButton(onClick = {
                                    progress = "Finding the lyrics…"
                                    scope.launch {
                                        val got = withContext(Dispatchers.IO) { SongLibrary.fetchLyrics(s.artist, s.title, songDuration.takeIf { it > 0 }) }
                                        val updated = s.copy(synced = got.synced ?: s.synced, plain = got.plain ?: s.plain)
                                        SongLibrary.update(updated)
                                        if (pick?.id == s.id) { pick = Pick.Saved(updated); fillWords() }
                                        progress = null
                                        if (got.synced == null && got.plain == null) failure = "No lyrics found for “${s.title}”. Check its name and artist with Edit."
                                    }
                                }) { Text("Find lyrics", fontSize = 12.sp, color = Palette.accent) }
                            }
                            TextButton(onClick = { editing = s }) { Text("Edit", fontSize = 12.sp, color = Palette.accent) }
                        }
                        null -> {}
                    }
                    if (songDuration > plan.length) {
                        Column(verticalArrangement = Arrangement.spacedBy(10.dp)) {
                            Title("Start at")
                            Segments(listOf("The drop", "The start", "Pick"), startChoice) { startChoice = it }
                            if (startChoice == 2) {
                                val maxStart = maxOf(0.1, songDuration - plan.length)
                                Row(verticalAlignment = Alignment.CenterVertically) {
                                    IconButton(onClick = { play("scrub", pick?.path(context), customStart) }) {
                                        Icon(if (playing == "scrub") Icons.Default.Stop else Icons.Default.PlayArrow, "Hear it")
                                    }
                                    Column(Modifier.weight(1f)) {
                                        Slider(
                                            customStart.toFloat().coerceIn(0f, maxStart.toFloat()), { customStart = it.toDouble() }, valueRange = 0f..maxStart.toFloat(),
                                            colors = SliderDefaults.colors(thumbColor = Color.White, activeTrackColor = Color.White, inactiveTrackColor = Palette.surface3),
                                        )
                                        Text("${tc(customStart)} of ${tc(songDuration)}", fontSize = 11.sp, color = Palette.muted)
                                    }
                                }
                            }
                        }
                    }
                }

                // Vibe
                Column(verticalArrangement = Arrangement.spacedBy(12.dp)) {
                    Title("Vibe")
                    Segments(Vibe.entries.map { it.label }, plan.vibe.ordinal) { plan = plan.copy(vibe = Vibe.entries[it]) }
                    val lengths = listOf(10.0, 15.0, 20.0, 30.0)
                    Segments(lengths.map { "${it.toInt()}s" }, lengths.indexOf(plan.length)) { plan = plan.copy(length = lengths[it]) }
                }

                // Words
                Column(verticalArrangement = Arrangement.spacedBy(10.dp)) {
                    Title("Words", if (pick is Pick.Saved) "From the song's lyrics, or your own" else null)
                    OutlinedTextField(
                        plan.words, { plan = plan.copy(words = it) }, modifier = Modifier.fillMaxWidth(), minLines = 2, maxLines = 6,
                        placeholder = { Text("pov: you finally stopped overthinking", color = Palette.muted) },
                    )
                }

                // Captions
                Column(verticalArrangement = Arrangement.spacedBy(12.dp)) {
                    Title("Captions", "How the words on the reel look")
                    val saved = (pick as? Pick.Saved)?.song
                    if (saved?.hasSynced == true) {
                        Row(verticalAlignment = Alignment.CenterVertically) {
                            Column(Modifier.weight(1f)) {
                                Text("Use the song's lyrics", fontSize = 15.sp)
                                Text("Off keeps just the words you typed", fontSize = 12.sp, color = Palette.muted)
                            }
                            Switch(plan.captions, { plan = plan.copy(captions = it) })
                        }
                        if (plan.captions) {
                            Row(verticalAlignment = Alignment.CenterVertically) {
                                Text("Word by word", fontSize = 15.sp, modifier = Modifier.weight(1f))
                                Switch(plan.wordByWord, { plan = plan.copy(wordByWord = it) })
                            }
                        }
                    }
                    Label("Font")
                    Row(Modifier.horizontalScroll(rememberScrollState()), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                        captionFonts.forEach { f -> Pill(f, plan.font == f) { plan = plan.copy(font = f) } }
                    }
                    Label("Style")
                    Row(Modifier.horizontalScroll(rememberScrollState()), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                        CaptionStyle.entries.forEach { s -> Pill(s.label, plan.style == s) { plan = plan.copy(style = s) } }
                    }
                    Label("Placement")
                    Segments(CaptionPlacement.entries.map { it.label }, plan.placement.ordinal) { plan = plan.copy(placement = CaptionPlacement.entries[it]) }
                    Label("Size")
                    Segments(CaptionSize.entries.map { it.label }, plan.size.ordinal) { plan = plan.copy(size = CaptionSize.entries[it]) }
                    Label("Align")
                    Segments(CaptionAlign.entries.map { it.label }, plan.align.ordinal) { plan = plan.copy(align = CaptionAlign.entries[it]) }
                }
                failure?.let { Text(it, fontSize = 14.sp, color = Palette.danger) }
                Spacer(Modifier.height(8.dp))
            }
            Box(Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 10.dp)) {
                Row(
                    Modifier.fillMaxWidth().height(50.dp).clip(RoundedCornerShape(14.dp))
                        .background(if (ready) Color.White else Palette.surface3).clickable(enabled = ready) { make() },
                    horizontalArrangement = Arrangement.Center, verticalAlignment = Alignment.CenterVertically,
                ) {
                    val c = if (ready) Color.Black else Palette.muted
                    Icon(Icons.Default.AutoAwesome, null, tint = c, modifier = Modifier.size(18.dp))
                    Spacer(Modifier.width(8.dp))
                    Text("Make reel", color = c, fontSize = 16.sp, fontWeight = FontWeight.SemiBold)
                }
            }
        }

        progress?.let { p ->
            Box(
                Modifier.fillMaxSize().background(Color.Black.copy(alpha = 0.75f)).clickable(enabled = true, onClick = {}),
                contentAlignment = Alignment.Center,
            ) {
                Column(horizontalAlignment = Alignment.CenterHorizontally) {
                    CircularProgressIndicator(color = Color.White)
                    Spacer(Modifier.height(14.dp))
                    Text(p, fontSize = 15.sp, fontWeight = FontWeight.Medium)
                }
            }
        }
    }

    editing?.let { s ->
        SongEditor(s, onDismiss = { editing = null }) { updated ->
            SongLibrary.update(updated)
            if (pick?.id == updated.id) { pick = Pick.Saved(updated); fillWords() }
            editing = null
        }
    }
    trimming?.let { i ->
        plan.media.getOrNull(i)?.let { path ->
            ClipStartDialog(path, plan.videoStarts.getOrNull(i) ?: 0.0, onDismiss = { trimming = null }) { v ->
                plan = plan.copy(videoStarts = plan.videoStarts.toMutableList().also { it[i] = v })
                trimming = null
            }
        }
    }
}

@Composable
private fun Title(t: String, sub: String? = null) {
    Column(verticalArrangement = Arrangement.spacedBy(3.dp)) {
        Text(t, fontSize = 17.sp, fontWeight = FontWeight.SemiBold)
        sub?.let { Text(it, fontSize = 13.sp, color = Palette.muted) }
    }
}

@Composable
private fun Label(t: String) = Text(t, fontSize = 12.sp, color = Palette.muted)

@Composable
private fun Pill(t: String, on: Boolean, icon: androidx.compose.ui.graphics.vector.ImageVector? = null, onClick: () -> Unit) {
    Row(
        Modifier.height(32.dp).clip(CircleShape).background(if (on) Color.White else Palette.surface2).clickable(onClick = onClick).padding(horizontal = 12.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        val c = if (on) Color.Black else Palette.ink
        icon?.let { Icon(it, null, tint = c, modifier = Modifier.size(15.dp)); Spacer(Modifier.width(6.dp)) }
        Text(t, fontSize = 13.sp, fontWeight = FontWeight.Medium, color = c)
    }
}

@Composable
private fun Segments(options: List<String>, selected: Int, set: (Int) -> Unit) {
    SingleChoiceSegmentedButtonRow(Modifier.fillMaxWidth()) {
        options.forEachIndexed { i, name ->
            SegmentedButton(
                selected = i == selected, onClick = { set(i) }, shape = SegmentedButtonDefaults.itemShape(i, options.size),
                colors = SegmentedButtonDefaults.colors(activeContainerColor = Palette.surface3, inactiveContainerColor = Palette.surface1),
                icon = {},
            ) { Text(name, fontSize = 12.sp, maxLines = 1, overflow = TextOverflow.Ellipsis) }
        }
    }
}

@OptIn(ExperimentalFoundationApi::class)
@Composable
private fun SongRow(main: String, sub: String, picked: Boolean, playing: Boolean, onPlay: () -> Unit, onSelect: () -> Unit, onLong: (() -> Unit)? = null) {
    Row(
        Modifier.fillMaxWidth().combinedClickable(onClick = onSelect, onLongClick = onLong).padding(horizontal = 12.dp, vertical = 8.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Box(Modifier.size(34.dp).clip(CircleShape).background(Palette.surface3).clickable(onClick = onPlay), contentAlignment = Alignment.Center) {
            Icon(if (playing) Icons.Default.Stop else Icons.Default.PlayArrow, if (playing) "Stop" else "Play", modifier = Modifier.size(16.dp))
        }
        Spacer(Modifier.width(12.dp))
        Column(Modifier.weight(1f)) {
            Text(main, fontSize = 15.sp, fontWeight = FontWeight.Medium, maxLines = 1, overflow = TextOverflow.Ellipsis)
            Text(sub, fontSize = 12.sp, color = Palette.muted, maxLines = 1, overflow = TextOverflow.Ellipsis)
        }
        if (picked) Icon(Icons.Default.Check, null, tint = Palette.accent)
    }
}

/** Fix a song's name, artist and lyrics (paste LRC for synced words), or nudge its timing. */
@Composable
private fun SongEditor(song: SavedSong, onDismiss: () -> Unit, onSave: (SavedSong) -> Unit) {
    var title by remember { mutableStateOf(song.title) }
    var artist by remember { mutableStateOf(song.artist) }
    var lyrics by remember { mutableStateOf(song.synced?.takeIf { it.isNotEmpty() } ?: song.plain.orEmpty()) }
    var offset by remember { mutableDoubleStateOf(song.offset) }
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text("Song") },
        text = {
            Column(Modifier.verticalScroll(rememberScrollState()), verticalArrangement = Arrangement.spacedBy(10.dp)) {
                OutlinedTextField(title, { title = it }, label = { Text("Title") }, singleLine = true, modifier = Modifier.fillMaxWidth())
                OutlinedTextField(artist, { artist = it }, label = { Text("Artist") }, singleLine = true, modifier = Modifier.fillMaxWidth())
                OutlinedTextField(
                    lyrics, { lyrics = it }, label = { Text("Lyrics (plain, or synced LRC)") },
                    minLines = 4, maxLines = 10, modifier = Modifier.fillMaxWidth().heightIn(max = 260.dp),
                )
                if (lyrics.trimStart().startsWith("[")) {
                    Text("Timing %+.1fs".format(offset), fontSize = 13.sp, color = Palette.muted)
                    Slider(
                        offset.toFloat(), { offset = (it * 10).toInt() / 10.0 }, valueRange = -3f..3f,
                        colors = SliderDefaults.colors(thumbColor = Color.White, activeTrackColor = Color.White, inactiveTrackColor = Palette.surface3),
                    )
                }
            }
        },
        confirmButton = {
            TextButton(onClick = {
                val synced = lyrics.trimStart().startsWith("[")
                onSave(song.copy(title = title.trim(), artist = artist.trim(), synced = if (synced) lyrics else null, plain = if (synced) song.plain else lyrics, offset = offset))
            }) { Text("Save") }
        },
        dismissButton = { TextButton(onClick = onDismiss) { Text("Cancel") } },
        containerColor = Palette.surface2,
    )
}

/** Where a piece of footage starts in its own file: scrub and see the frame, or let Liemo pick. */
@Composable
private fun ClipStartDialog(path: String, initial: Double, onDismiss: () -> Unit, onDone: (Double?) -> Unit) {
    val retriever = remember { MediaMetadataRetriever().apply { runCatching { setDataSource(path) } } }
    DisposableEffect(Unit) { onDispose { retriever.release() } }
    val duration = remember { (retriever.extractMetadata(MediaMetadataRetriever.METADATA_KEY_DURATION)?.toLongOrNull() ?: 0) / 1000.0 }
    val isVideo = remember { retriever.extractMetadata(MediaMetadataRetriever.METADATA_KEY_HAS_VIDEO) == "yes" }
    var start by remember { mutableDoubleStateOf(initial.coerceIn(0.0, maxOf(0.0, duration - 0.5))) }
    var frame by remember { mutableStateOf<ImageBitmap?>(null) }
    LaunchedEffect(start) {
        if (!isVideo) return@LaunchedEffect
        frame = withContext(Dispatchers.IO) {
            synchronized(retriever) {
                retriever.getScaledFrameAtTime((start * 1e6).toLong(), MediaMetadataRetriever.OPTION_CLOSEST_SYNC, 360, 640)?.asImageBitmap()
            }
        } ?: frame
    }
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text("Where it starts") },
        text = {
            if (!isVideo || duration < 0.2) {
                Text("This one's a photo — it plays the whole time.", color = Palette.muted)
            } else {
                Column(horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(10.dp)) {
                    Box(Modifier.fillMaxWidth().height(300.dp), contentAlignment = Alignment.Center) {
                        frame?.let { Image(it, null, contentScale = ContentScale.Fit, modifier = Modifier.clip(RoundedCornerShape(10.dp))) }
                            ?: CircularProgressIndicator(color = Color.White)
                    }
                    Slider(
                        start.toFloat(), { start = it.toDouble() }, valueRange = 0f..maxOf(0.1f, (duration - 0.5).toFloat()),
                        colors = SliderDefaults.colors(thumbColor = Color.White, activeTrackColor = Color.White, inactiveTrackColor = Palette.surface3),
                    )
                    Text("${tc(start)} of ${tc(duration)}", fontSize = 12.sp, color = Palette.muted)
                }
            }
        },
        confirmButton = { TextButton(onClick = { onDone(start) }, enabled = isVideo && duration >= 0.2) { Text("Use this") } },
        dismissButton = { TextButton(onClick = { onDone(null) }) { Text("Liemo picks") } },
        containerColor = Palette.surface2,
    )
}

private fun tc(t: Double) = "%d:%02d".format(t.toInt() / 60, t.toInt() % 60)

/** A picked file copied into `dir` (Liemo's imports, or the cache), with its original name. */
private fun copyToCache(context: Context, uri: Uri, dir: File = Core.cacheDir): Pair<File, String>? = runCatching {
    val r = context.contentResolver
    var name = r.query(uri, arrayOf(OpenableColumns.DISPLAY_NAME), null, null, null)?.use { c -> if (c.moveToFirst()) c.getString(0) else null } ?: "media"
    if (!name.contains('.')) name += "." + (android.webkit.MimeTypeMap.getSingleton().getExtensionFromMimeType(r.getType(uri)) ?: "mp4")
    val dst = File(dir, "${java.util.UUID.randomUUID().toString().take(8)}-${name.replace('/', '-')}")
    r.openInputStream(uri)!!.use { i -> dst.outputStream().use { i.copyTo(it) } }
    dst to name
}.getOrNull()

private fun thumbOf(f: File): ImageBitmap? = runCatching {
    android.graphics.BitmapFactory.decodeFile(f.path, android.graphics.BitmapFactory.Options().apply { inSampleSize = 8 })?.asImageBitmap()
        ?: MediaMetadataRetriever().let { r ->
            try { r.setDataSource(f.path); r.getScaledFrameAtTime(0, MediaMetadataRetriever.OPTION_CLOSEST_SYNC, 180, 320)?.asImageBitmap() } finally { r.release() }
        }
}.getOrNull()
