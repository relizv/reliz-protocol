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
import androidx.core.app.NotificationCompat
import java.io.File

/**
 * Foreground VpnService для Reliz Protocol.
 *
 * Зачем нужен: без foreground-сервиса Android агрессивно убивает фоновые
 * процессы. Этот сервис:
 *   1. Поднимает постоянное уведомление (startForeground) — процесс живёт,
 *      пока VPN активен.
 *   2. Создаёт TUN-интерфейс и маршрутизирует весь трафик через tun2socks →
 *      локальный Reliz SOCKS5 (127.0.0.1:10808).
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
    private var tunnelThread: Thread? = null

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
        // START_STICKY: если система всё-таки убьёт сервис — она пересоздаст его.
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

        // Исключаем трафик самого приложения из VPN, чтобы соединения
        // Rust-прокси к удалённому серверу не уходили обратно в TUN (loop).
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

        val configPath = writeTun2socksConfig().absolutePath
        val tunFd = fd.fd
        tunnelThread = Thread({
            if (Tun2Socks.ensureLoaded()) {
                try {
                    Tun2Socks.tunnelStart(configPath, tunFd)
                } catch (_: Throwable) {
                    // Нативный tun2socks завершился / недоступен.
                }
            }
        }, "reliz-tun2socks")
        tunnelThread?.start()
    }

    private fun writeTun2socksConfig(): File {
        // Формат конфига hev-socks5-tunnel.
        val yaml = """
            tunnel:
              mtu: $MTU
            socks5:
              address: $SOCKS_HOST
              port: $SOCKS_PORT
              udp: udp
            misc:
              task-stack-size: 20480
        """.trimIndent()
        val f = File(cacheDir, "tun2socks.yaml")
        f.writeText(yaml)
        return f
    }

    private fun stopVpn() {
        running = false
        try {
            if (Tun2Socks.ensureLoaded()) Tun2Socks.tunnelStop()
        } catch (_: Throwable) {
        }
        try {
            tun?.close()
        } catch (_: Exception) {
        }
        tun = null
        tunnelThread = null

        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.N) {
            stopForeground(STOP_FOREGROUND_REMOVE)
        } else {
            @Suppress("DEPRECATION")
            stopForeground(true)
        }
        stopSelf()
    }

    override fun onRevoke() {
        // Пользователь отозвал VPN-разрешение из системных настроек.
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
            // Android 14+: обязателен тип foreground-сервиса.
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
