package com.example.reliz_protocol

/**
 * JNI-обёртка над нативным tun2socks (`libhev-socks5-tunnel.so`).
 *
 * tun2socks превращает IP-пакеты из TUN-интерфейса VpnService в SOCKS5-
 * соединения к локальному Reliz-прокси (127.0.0.1:10808) и обратно.
 *
 * ⚠️ ТРЕБУЕТ НАТИВНОЙ БИБЛИОТЕКИ.
 * Положи собранные `.so` в `android/app/src/main/jniLibs/<abi>/`:
 *     jniLibs/arm64-v8a/libhev-socks5-tunnel.so
 *     jniLibs/armeabi-v7a/libhev-socks5-tunnel.so
 *     jniLibs/x86_64/libhev-socks5-tunnel.so
 * Исходники и сборка: https://github.com/heiher/hev-socks5-tunnel
 * (собирается через Android NDK; JNI-символы должны соответствовать
 * методам ниже: класс com/example/reliz_protocol/Tun2Socks).
 *
 * Сборка Kotlin не зависит от наличия .so (external-методы линкуются
 * в рантайме); без библиотеки будет UnsatisfiedLinkError при старте туннеля.
 */
object Tun2Socks {
    @Volatile
    private var loaded = false

    @Synchronized
    fun ensureLoaded(): Boolean {
        if (loaded) return true
        return try {
            System.loadLibrary("hev-socks5-tunnel")
            loaded = true
            true
        } catch (t: Throwable) {
            loaded = false
            false
        }
    }

    /**
     * Запустить туннель. Блокирует вызывающий поток, поэтому вызывается
     * в отдельном Thread. configPath — YAML для hev-socks5-tunnel, tunFd — fd из
     * ParcelFileDescriptor туннеля.
     */
    external fun tunnelStart(configPath: String, tunFd: Int): Int

    /** Остановить туннель (разблокирует tunnelStart). */
    external fun tunnelStop()
}
