plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.android")
    id("org.jetbrains.kotlin.plugin.compose")
    id("org.jetbrains.kotlin.plugin.serialization")
}

// One version for every Liemo: the engine's (native/mobile/Cargo.toml), like the iOS app's.
val liemoVersion: String = rootDir.resolve("../native/mobile/Cargo.toml").readLines()
    .first { it.startsWith("version") }.substringAfter('"').substringBefore('"')
// 0.9.1 -> 901: a growing number for the Play Store.
val liemoCode: Int = liemoVersion.split('.').map { it.toInt() }.let { (a, b, c) -> a * 10000 + b * 100 + c }

android {
    namespace = "dev.flaxo.liemo"
    compileSdk = 36

    defaultConfig {
        applicationId = "dev.flaxo.liemo"
        minSdk = 29
        targetSdk = 36
        versionCode = liemoCode
        versionName = liemoVersion
        ndk { abiFilters += listOf("arm64-v8a", "x86_64") }
    }

    buildTypes {
        release {
            isMinifyEnabled = false
            // Until there's a Play upload key, release builds are signed with the debug key so
            // they install on a phone.
            signingConfig = signingConfigs.getByName("debug")
        }
    }
    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }
    buildFeatures { compose = true }
}

kotlin {
    compilerOptions { jvmTarget.set(org.jetbrains.kotlin.gradle.dsl.JvmTarget.JVM_17) }
}

dependencies {
    val compose = platform("androidx.compose:compose-bom:2025.07.00")
    implementation(compose)
    implementation("androidx.compose.ui:ui")
    implementation("androidx.compose.foundation:foundation")
    implementation("androidx.compose.material3:material3")
    implementation("androidx.compose.material:material-icons-extended")
    implementation("androidx.activity:activity-compose:1.10.1")
    implementation("androidx.lifecycle:lifecycle-runtime-compose:2.9.2")
    implementation("org.jetbrains.kotlinx:kotlinx-serialization-json:1.9.0")
    implementation("org.jetbrains.kotlinx:kotlinx-coroutines-android:1.10.2")
    // UniFFI's Kotlin bindings reach the Rust engine through JNA.
    implementation("net.java.dev.jna:jna:5.17.0@aar")
}
