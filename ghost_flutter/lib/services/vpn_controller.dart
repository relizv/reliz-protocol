import 'package:flutter/services.dart';

/// Тонкая обёртка над нативным Android `VpnService` через [MethodChannel].
///
/// Канал `reliz/vpn` реализован в `MainActivity.kt`:
/// - `prepare` — показывает системный диалог согласия на VPN и возвращает
///   `true`, если разрешение выдано;
/// - `start`   — поднимает foreground [RelizVpnService] (TUN + tun2socks);
/// - `stop`    — останавливает сервис и закрывает TUN.
class VpnController {
  static const MethodChannel _channel = MethodChannel('reliz/vpn');

  /// Запросить разрешение на создание VPN-туннеля.
  /// Возвращает `true`, если пользователь согласился (или согласие уже есть).
  Future<bool> prepare() async {
    final granted = await _channel.invokeMethod<bool>('prepare');
    return granted ?? false;
  }

  /// Запустить системный VpnService (foreground).
  Future<void> start() => _channel.invokeMethod<void>('start');

  /// Остановить VpnService.
  Future<void> stop() => _channel.invokeMethod<void>('stop');
}
