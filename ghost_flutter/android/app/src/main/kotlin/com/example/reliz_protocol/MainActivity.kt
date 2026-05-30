package com.example.reliz_protocol

import android.app.Activity
import android.content.Intent
import android.net.VpnService
import android.os.Build
import io.flutter.embedding.android.FlutterActivity
import io.flutter.embedding.engine.FlutterEngine
import io.flutter.plugin.common.MethodChannel

/**
 * Связь Flutter ↔ нативный VpnService через MethodChannel `reliz/vpn`.
 *
 *   prepare — системный диалог согласия на VPN (VpnService.prepare).
 *   start   — запуск foreground RelizVpnService.
 *   stop    — остановка сервиса.
 */
class MainActivity : FlutterActivity() {

    private val channelName = "reliz/vpn"
    private val vpnRequestCode = 0x1A2B
    private var pendingResult: MethodChannel.Result? = null

    override fun configureFlutterEngine(flutterEngine: FlutterEngine) {
        super.configureFlutterEngine(flutterEngine)
        MethodChannel(flutterEngine.dartExecutor.binaryMessenger, channelName)
            .setMethodCallHandler { call, result ->
                when (call.method) {
                    "prepare" -> prepareVpn(result)
                    "start" -> {
                        startVpn()
                        result.success(true)
                    }
                    "stop" -> {
                        stopVpn()
                        result.success(true)
                    }
                    else -> result.notImplemented()
                }
            }
    }

    private fun prepareVpn(result: MethodChannel.Result) {
        val intent = VpnService.prepare(this)
        if (intent != null) {
            // Разрешение ещё не выдано — показываем системный диалог.
            pendingResult = result
            startActivityForResult(intent, vpnRequestCode)
        } else {
            // Согласие уже есть.
            result.success(true)
        }
    }

    private fun startVpn() {
        val intent = Intent(this, RelizVpnService::class.java)
            .setAction(RelizVpnService.ACTION_CONNECT)
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
            startForegroundService(intent)
        } else {
            startService(intent)
        }
    }

    private fun stopVpn() {
        val intent = Intent(this, RelizVpnService::class.java)
            .setAction(RelizVpnService.ACTION_DISCONNECT)
        startService(intent)
    }

    override fun onActivityResult(requestCode: Int, resultCode: Int, data: Intent?) {
        super.onActivityResult(requestCode, resultCode, data)
        if (requestCode == vpnRequestCode) {
            pendingResult?.success(resultCode == Activity.RESULT_OK)
            pendingResult = null
        }
    }
}
