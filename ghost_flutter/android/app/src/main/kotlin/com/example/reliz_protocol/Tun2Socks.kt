package com.example.reliz_protocol

import android.util.Log
import java.io.File

/**
 * JNI-обёртка над нативным tun2socks (`libhev-socks5-tunnel.so`).
 *
 * tun2socks превращает IP-пакеты из TUN-интерфейса VpnService в SOCKS5-
 * соединения к локальному Reliz-прокси (127.0.0.1:10808) и обратно.
 *
 * ⚠️ ТРЕБУЕТ НАТИВНОЙ БИБЛИОТЕКИ.
 *
 * ## Как получить .so:
 *
 * ### Вариант 1: Скачать готовые (быстрее)
 * ```powershell
 * .\scripts\fetch-prebuilt-tun2socks.ps1
 * ```
 * Если скрипт не найдёт релизов, используй вариант 2.
 *
 * ### Вариант 2: Собрать из исходников
 * ```bash
 * export ANDROID_NDK_HOME=$HOME/Android/Sdk/ndk/27.0.12077973
 * bash scripts/build-tun2socks-android.sh
 * ```
 *
 * ### Вариант 3: Вручную
 * Положи собранные `.so` в `android/app/src/main/jniLibs/<abi>/`:
 * ```
 * jniLibs/arm64-v8a/libhev-socks5-tunnel.so
 * jniLibs/armeabi-v7a/libhev-socks5-tunnel.so
 * jniLibs/x86_64/libhev-socks5-tunnel.so
 * ```
 *
 * Исходники: https://github.com/heiher/hev-socks5-tunnel
 *
 * ## Важно про JNI символы
 * Этот класс ожидает нативные методы:
 *   `Java_com_example_reliz_protocol_Tun2Socks_tunnelStart`
 *   `Java_com_example_reliz_protocol_Tun2Socks_tunnelStop`
 *
 * Если оригинальный hev-socks5-tunnel экспортирует другие имена, используй
 * скрипт `scripts/patch-jni-names.sh` (или собери с кастомным JNI wrapper).
 */
object Tun2Socks {
    private const val TAG = "Tun2Socks"

    @Volatile
    private var loaded = false

    @Synchronized
    fun ensureLoaded(): Boolean {
        if (loaded) return true
        return try {
            System.loadLibrary("hev-socks5-tunnel")
            loaded = true
            Log.i(TAG, "Native library loaded successfully")
            true
        } catch (t: Throwable) {
            loaded = false
            Log.e(TAG, "Failed to load libhev-socks5-tunnel.so: ${t.message}")
            Log.e(TAG, "Make sure .so files are in jniLibs/<abi>/")
            false
        }
    }

    /**
     * Запустить туннель. Блокирует вызывающий поток, поэтому вызывается
     * в отдельном Thread. configPath — YAML для hev-socks5-tunnel, tunFd — fd из
     * ParcelFileDescriptor туннеля.
     *
     * Возвращает 0 при успехе, иначе ненулевой код ошибки.
     */
    external fun tunnelStart(configPath: String, tunFd: Int): Int

    /** Остановить туннель (разблокирует tunnelStart). */
    external fun tunnelStop()
}
