package dev.flaxo.liemo.ui

import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.darkColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import dev.flaxo.liemo.R
import dev.flaxo.liemo.core.Core
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import uniffi.liemo_mobile.MediaFrame

/** The desktop app's palette. */
object Palette {
    val bg = Color(0xFF090909)
    val surface1 = Color(0xFF141414)
    val surface2 = Color(0xFF1C1C1C)
    val surface3 = Color(0xFF242424)
    val border = Color(0xFF262626)
    val ink = Color.White
    val muted = Color(0xFF999999)
    val accent = Color(0xFF0099FF)
    val danger = Color(0xFFFF5577)
    val wave = Color(0xFF5FD39A)

    fun clip(type: String): Color = when (type) {
        "text" -> Color(0xFF3B2F55)
        "music", "audio" -> Color(0xFF1D3D31)
        "overlay" -> Color(0xFF333333)
        else -> Color(0xFF223552)
    }
}

@Composable
fun LiemoTheme(content: @Composable () -> Unit) {
    MaterialTheme(
        colorScheme = darkColorScheme(
            primary = Color.White,
            onPrimary = Color.Black,
            secondary = Palette.accent,
            background = Palette.bg,
            surface = Palette.surface1,
            surfaceContainer = Palette.surface1,
            surfaceContainerHigh = Palette.surface2,
            surfaceContainerHighest = Palette.surface3,
            surfaceContainerLow = Palette.surface1,
            onSurface = Palette.ink,
            onSurfaceVariant = Palette.muted,
            outline = Palette.border,
            outlineVariant = Palette.border,
            error = Palette.danger,
        ),
        content = content,
    )
}

/** "#ff5577" as a colour. */
fun cssColor(s: String?): Color? {
    val h = s?.removePrefix("#") ?: return null
    if (h.length != 6) return null
    return h.toLongOrNull(16)?.let { Color(0xFF000000 or it) }
}

/** The L mark. */
@Composable
fun LiemoMark(size: Dp = 22.dp) {
    Image(
        painterResource(R.drawable.liemo_mark), contentDescription = null,
        modifier = Modifier.size(size).clip(RoundedCornerShape(size * 0.24f)),
    )
}

/** An engine-drawn picture (library cards, effect previews), drawn once and kept. */
@Composable
fun EngineImage(key: String, modifier: Modifier = Modifier, make: () -> MediaFrame?) {
    var img by remember(key) { mutableStateOf<ImageBitmap?>(null) }
    LaunchedEffect(key) {
        img = withContext(Dispatchers.Default) { Core.thumb(key, make) }
    }
    Box(modifier.background(Palette.surface3)) {
        img?.let {
            Image(it, contentDescription = null, contentScale = ContentScale.Crop, modifier = Modifier.matchParentSize())
        }
    }
}
