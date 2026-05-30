package com.example.reliz_protocol

import android.util.Log

/**
 * Прямой JNI-мост к Rust tun2socks (ghost-tun).
 *
 * Заменяет внешний `libhev-socks5-tunnel.so`. Вся логика TUN → SOCKS5
 * теперь внутри Rust shared library (`libghost_flutter_bridge.so`),
 * которую уже загружает flutter_rust_bridge.
 *
 * JNI символы:
 *   - `ghost_tun_start(fd: i32) -> i32`
 *   - `ghost_tun_stop() -> i32`
 */
object GhostTunBridge {
    private const val TAG = "GhostTunBridge"

    @Volatile
    private var loaded = false

    @Synchronized
    fun ensureLoaded(): Boolean {
        if (loaded) return true
        return try {
            // Имя .so соответствует crate name в Cargo.toml: ghost-flutter-bridge
            System.loadLibrary("ghost_flutter_bridge")
            loaded = true
            Log.i(TAG, "Native library ghost_flutter_bridge loaded")
            true
        } catch (t: Throwable) {
            loaded = false
            Log.e(TAG, "Failed to load libghost_flutter_bridge.so: ${t.message}")
            false
        }
    }

    /**
     * Запустить userspace tun2socks.
     * @param tunFd fd из `ParcelFileDescriptor` VpnService.
     * @return 0 при успехе.
     */
    external fun startTun(tunFd: Int): Int

    /** Остановить tun2socks (прерывает внутренний цикл). */
    external fun stopTun(): Int
}
