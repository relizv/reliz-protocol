plugins {
    id("com.android.application")
    id("kotlin-android")
    // The Flutter Gradle Plugin must be applied after the Android and Kotlin Gradle plugins.
    id("dev.flutter.flutter-gradle-plugin")
}

android {
    namespace = "com.example.reliz_protocol"
    compileSdk = flutter.compileSdkVersion
    // app_links / shared_preferences_android требуют NDK 27.x. Берём явно.
    ndkVersion = "27.0.12077973"

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }

    kotlinOptions {
        jvmTarget = JavaVersion.VERSION_17.toString()
    }

    defaultConfig {
        applicationId = "com.example.reliz_protocol"
        // You can update the following values to match your application needs.
        // For more information, see: https://flutter.dev/to/review-gradle-config.
        minSdk = flutter.minSdkVersion
        targetSdk = flutter.targetSdkVersion
        versionCode = flutter.versionCode
        versionName = flutter.versionName
    }

    buildTypes {
        release {
            // TODO: Add your own signing config for the release build.
            // Signing with the debug keys for now, so `flutter run --release` works.
            signingConfig = signingConfigs.getByName("debug")
        }
    }
}

flutter {
    source = "../.."
}

// Нативный tun2socks (libhev-socks5-tunnel.so) подключается через jniLibs.
// Положи собранные библиотеки в src/main/jniLibs/<abi>/ (см. Tun2Socks.kt).
// Архитектуры: arm64-v8a, armeabi-v7a, x86_64.
//
// android {
//     sourceSets["main"].jniLibs.srcDirs("src/main/jniLibs")
// }
