package com.example.reliz_protocol

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.content.Intent
import android.content.pm.ServiceInfo
import android.net.VpnService
import android.os.Build
import android.os.ParcelFileDescriptor
import android.util.Log
import androidx.core.app.NotificationCompat
import java.io.File

/**
 * Foreground VpnService для Reliz Protocol.
 *
 * Зачем нужен: без foreground-сервиса Android агрессивно убивает фоновые
 * процессы. Этот сервис:
 *   1. Поднимает постоянное уведомление (startForeground) — процесс живёт,
 *      пока VPN активен.
 *   2. Создаёт TUN-интерфейс и маршрутизирует весь трафик через Rust tun2socks
 *      (ghost-tun) → локальный Reliz SOCKS5 (127.0.0.1:10808).
 *   3. Исключает своё приложение из VPN (addDisallowedApplication), чтобы
 *      исходящие соединения Rust-прокси к серверу не зацикливались в TUN.
 */
class RelizVpnService : VpnService() {

    companion object {
        const val ACTION_CONNECT = "com.example.reliz_protocol.CONNECT"
        const val ACTION_DISCONNECT = "com.example.reliz_protocol.DISCONNECT"

        private const val CHANNEL_ID = "reliz_vpn"
        private const val NOTIF_ID = 1001

        private const val SOCKS_HOST = "127.0.0.1"
        private const val SOCKS_PORT = 10808
        private const val MTU = 1500
    }

    private var tun: ParcelFileDescriptor? = null

    @Volatile
    private var running = false

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        when (intent?.action) {
            ACTION_DISCONNECT -> {
                stopVpn()
                return START_NOT_STICKY
            }
            else -> startVpn()
        }
        return START_STICKY
    }

    private fun startVpn() {
        if (running) return
        startForegroundCompat()

        val builder = Builder()
            .setSession("Reliz Protocol")
            .setMtu(MTU)
            .addAddress("10.0.0.2", 32)
            .addAddress("fd00::2", 128)
            .addDnsServer("1.1.1.1")
            .addDnsServer("8.8.8.8")
            .addRoute("0.0.0.0", 0)
            .addRoute("::", 0)

        try {
            builder.addDisallowedApplication(packageName)
        } catch (_: Exception) {
        }

        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) {
            builder.setMetered(false)
        }

        val fd = builder.establish()
        if (fd == null) {
            stopVpn()
            return
        }
        tun = fd
        running = true

        // Запускаем Rust tun2socks (ghost-tun) напрямую через JNI
        if (GhostTunBridge.ensureLoaded()) {
            val rc = GhostTunBridge.startTun(fd.fd)
            if (rc != 0) {
                Log.e("RelizVpnService", "ghost_tun_start failed with code $rc")
                stopVpn()
            } else {
                Log.i("RelizVpnService", "ghost-tun started successfully")
            }
        } else {
            Log.e("RelizVpnService", "Native library not loaded, cannot start TUN relay")
            stopVpn()
        }
    }

    private fun stopVpn() {
        running = false
        try {
            if (GhostTunBridge.ensureLoaded()) {
                GhostTunBridge.stopTun()
            }
        } catch (_: Throwable) {
        }
        try {
            tun?.close()
        } catch (_: Exception) {
        }
        tun = null

        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.N) {
            stopForeground(STOP_FOREGROUND_REMOVE)
        } else {
            @Suppress("DEPRECATION")
            stopForeground(true)
        }
        stopSelf()
    }

    override fun onRevoke() {
        stopVpn()
        super.onRevoke()
    }

    override fun onDestroy() {
        stopVpn()
        super.onDestroy()
    }

    private fun startForegroundCompat() {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
            val nm = getSystemService(NotificationManager::class.java)
            val channel = NotificationChannel(
                CHANNEL_ID,
                "Reliz VPN",
                NotificationManager.IMPORTANCE_LOW,
            )
            channel.description = "Статус VPN-соединения"
            nm.createNotificationChannel(channel)
        }

        val openIntent = PendingIntent.getActivity(
            this,
            0,
            Intent(this, MainActivity::class.java),
            PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT,
        )
        val stopIntent = PendingIntent.getService(
            this,
            1,
            Intent(this, RelizVpnService::class.java).setAction(ACTION_DISCONNECT),
            PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT,
        )

        val notif: Notification = NotificationCompat.Builder(this, CHANNEL_ID)
            .setContentTitle("Reliz Protocol")
            .setContentText("VPN активен — трафик защищён")
            .setSmallIcon(R.mipmap.ic_launcher)
            .setOngoing(true)
            .setContentIntent(openIntent)
            .addAction(0, "Отключить", stopIntent)
            .setPriority(NotificationCompat.PRIORITY_LOW)
            .build()

        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.UPSIDE_DOWN_CAKE) {
            startForeground(
                NOTIF_ID,
                notif,
                ServiceInfo.FOREGROUND_SERVICE_TYPE_SYSTEM_EXEMPTED,
            )
        } else {
            startForeground(NOTIF_ID, notif)
        }
    }
}
