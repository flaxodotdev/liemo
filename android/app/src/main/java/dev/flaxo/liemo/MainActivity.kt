package dev.flaxo.liemo

import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.SystemBarStyle
import androidx.activity.compose.BackHandler
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.compose.material3.Surface
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.safeDrawingPadding
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import dev.flaxo.liemo.core.Core
import dev.flaxo.liemo.ui.EditorScreen
import dev.flaxo.liemo.ui.LiemoTheme
import dev.flaxo.liemo.ui.Palette
import dev.flaxo.liemo.ui.ProjectsScreen
import dev.flaxo.liemo.ui.QuickReelScreen

class MainActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        enableEdgeToEdge(SystemBarStyle.dark(android.graphics.Color.TRANSPARENT), SystemBarStyle.dark(android.graphics.Color.TRANSPARENT))
        Core.init(this)
        // For checks: `adb shell am start -n dev.flaxo.liemo/.MainActivity --es project <id>`.
        val launchProject = intent.getStringExtra("project")
        setContent {
            LiemoTheme {
                var open by rememberSaveable { mutableStateOf(launchProject) }
                var reeling by rememberSaveable { mutableStateOf(false) }
                Surface(Modifier.fillMaxSize(), color = Palette.bg, contentColor = Palette.ink) {
                Box(Modifier.fillMaxSize().safeDrawingPadding()) {
                    val id = open
                    if (reeling) {
                        QuickReelScreen { made ->
                            reeling = false
                            if (made != null) open = made
                        }
                    } else if (id == null) {
                        ProjectsScreen(onOpen = { open = it }, onReel = { reeling = true })
                    } else {
                        BackHandler { open = null }
                        EditorScreen(id = id, onClose = { open = null })
                    }
                }
                }
            }
        }
    }
}
