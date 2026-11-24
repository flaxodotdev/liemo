package dev.flaxo.liemo

import android.os.Bundle
import android.content.Intent
import android.net.Uri
import android.os.Build
import android.provider.Settings
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.remember
import androidx.compose.ui.platform.LocalContext
import androidx.core.content.FileProvider
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import org.json.JSONObject
import java.io.File
import java.net.HttpURLConnection
import java.net.URL
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
                val context = LocalContext.current
                var update by remember { mutableStateOf<AndroidUpdate?>(null) }
                var installFile by remember { mutableStateOf<File?>(null) }
                val settings = rememberLauncherForActivityResult(ActivityResultContracts.StartActivityForResult()) {
                    if (Build.VERSION.SDK_INT < 26 || packageManager.canRequestPackageInstalls()) {
                        installFile?.let { promptInstaller(it) }
                    }
                }
                LaunchedEffect(Unit) {
                    update = withContext(Dispatchers.IO) { AndroidUpdate.check(context) }
                }
                update?.let { available ->
                    AlertDialog(
                        onDismissRequest = { update = null },
                        title = { Text("Liemo ${available.version} is available") },
                        text = { Text("Download and install the latest version from GitHub?") },
                        confirmButton = {
                            TextButton(onClick = {
                                update = null
                                installFile = null
                                Thread {
                                    val file = runCatching { AndroidUpdate.download(context, available.apkUrl) }.getOrNull()
                                    runOnUiThread {
                                        if (file != null) {
                                            installFile = file
                                            if (Build.VERSION.SDK_INT >= 26 && !packageManager.canRequestPackageInstalls()) {
                                                settings.launch(Intent(Settings.ACTION_MANAGE_UNKNOWN_APP_SOURCES, Uri.parse("package:$packageName")))
                                            } else promptInstaller(file)
                                        }
                                    }
                                }.start()
                            }) { Text("Download update") }
                        },
                        dismissButton = { TextButton(onClick = { update = null }) { Text("Later") } }
                    )
                }
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

    private fun promptInstaller(file: File) {
        val uri = FileProvider.getUriForFile(this, "$packageName.files", file)
        startActivity(Intent(Intent.ACTION_INSTALL_PACKAGE).setData(uri).addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION))
    }
}

private data class AndroidUpdate(val version: String, val apkUrl: String) {
    companion object {
        private const val API = "https://api.github.com/repos/flaxodotdev/liemo/releases/latest"
        fun check(context: android.content.Context): AndroidUpdate? = runCatching {
            val connection = (URL(API).openConnection() as HttpURLConnection).apply {
                connectTimeout = 5000; readTimeout = 5000; setRequestProperty("Accept", "application/vnd.github+json")
            }
            val release = JSONObject(connection.inputStream.bufferedReader().use { it.readText() })
            val tag = release.getString("tag_name").removePrefix("v")
            val current = context.packageManager.getPackageInfo(context.packageName, 0).versionName ?: return null
            if (compareVersions(tag, current) <= 0) return null
            val assets = release.getJSONArray("assets")
            val asset = (0 until assets.length()).map { assets.getJSONObject(it) }
                .firstOrNull { it.getString("name").endsWith("android-universal.apk") } ?: return null
            AndroidUpdate(tag, asset.getString("browser_download_url"))
        }.getOrNull()

        fun download(context: android.content.Context, apkUrl: String): File {
            val connection = (URL(apkUrl).openConnection() as HttpURLConnection).apply { connectTimeout = 15000; readTimeout = 30000 }
            val output = File(context.cacheDir, "updates/liemo-update.apk").apply { parentFile?.mkdirs() }
            connection.inputStream.use { input -> output.outputStream().use { stream -> input.copyTo(stream) } }
            return output
        }

        private fun compareVersions(a: String, b: String): Int {
            val av = a.split('.').map { it.toIntOrNull() ?: 0 }
            val bv = b.split('.').map { it.toIntOrNull() ?: 0 }
            for (i in 0 until maxOf(av.size, bv.size)) {
                val d = (av.getOrElse(i) { 0 }).compareTo(bv.getOrElse(i) { 0 })
                if (d != 0) return d
            }
            return 0
        }
    }
}
