import 'dart:async';

import '../config/reliz_config.dart';
import '../models/proxy_state.dart';
import '../src/rust/api.dart' as api;
import '../src/rust/frb_generated.dart';
import 'vpn_controller.dart';

/// Сервис управления Reliz-VPN.
///
/// Оркестрирует три слоя:
///   1. Системный `VpnService` (Android) — TUN-интерфейс + foreground, чтобы
///      ОС не убивала процесс в фоне ([VpnController]).
///   2. Локальный SOCKS5-прокси на Rust ([api.startRelizProxy]).
///   3. tun2socks внутри сервиса заворачивает пакеты TUN → SOCKS5.
///
/// Конфигурация берётся из [RelizConfig] (один токен на сервере), поэтому
/// метод [connect] больше не принимает параметров.
class ProxyService {
  ProxyService();

  final VpnController _vpn = VpnController();

  ProxyState _state = const ProxyState();
  final _stateController = StreamController<ProxyState>.broadcast();

  Timer? _statusPoller;
  Timer? _sessionTimer;
  DateTime? _connectedAt;

  static bool _rustInitialized = false;
  static Future<void> _ensureRustInit() async {
    if (_rustInitialized) return;
    await RustLib.init();
    _rustInitialized = true;
  }

  Stream<ProxyState> get stateStream => _stateController.stream;
  ProxyState get state => _state;

  void _update(ProxyState newState) {
    _state = newState;
    _stateController.add(_state);
  }

  /// Подключиться: разрешение → SOCKS5-прокси → VpnService.
  Future<void> connect() async {
    await _ensureRustInit();
    _update(_state.copyWith(
      status: VpnStatus.connecting,
      statusText: 'Connecting...',
    ));

    // 1. Системный диалог согласия на VPN.
    final granted = await _vpn.prepare();
    if (!granted) {
      _update(const ProxyState(
        status: VpnStatus.error,
        statusText: 'VPN permission denied',
      ));
      return;
    }

    // 2. Поднимаем локальный SOCKS5-прокси (Rust). Конфиг — из RelizConfig.
    final rc = await api.startRelizProxy(
      serverAddr: RelizConfig.serverAddr,
      userId: RelizConfig.userId,
      enablePadding: RelizConfig.enablePadding,
      enableFragmentation: RelizConfig.enableFragmentation,
      maskDomain: RelizConfig.maskDomain,
    );
    if (rc != 0) {
      _update(_state.copyWith(
        status: VpnStatus.error,
        statusText: 'Proxy error (code $rc)',
      ));
      return;
    }

    // 3. Запускаем системный VpnService (foreground + TUN + tun2socks).
    try {
      await _vpn.start();
    } catch (_) {
      await api.stopProxy();
      _update(_state.copyWith(
        status: VpnStatus.error,
        statusText: 'VPN start failed',
      ));
      return;
    }

    _startStatusPolling();
  }

  /// Отключиться: гасим VpnService, затем прокси.
  Future<void> disconnect() async {
    await _ensureRustInit();
    try {
      await _vpn.stop();
    } catch (_) {
      // Сервис мог уже умереть — игнорируем.
    }
    await api.stopProxy();
    _stopStatusPolling();
    _stopSessionTimer();
    _update(const ProxyState(
      status: VpnStatus.disconnected,
      statusText: 'Disconnected',
    ));
  }

  VpnStatus _mapStatus(int code) {
    switch (code) {
      case 1:
        return VpnStatus.connecting;
      case 2:
        return VpnStatus.connected;
      case 3:
        return VpnStatus.error;
      default:
        return VpnStatus.disconnected;
    }
  }

  String _statusText(VpnStatus s) {
    switch (s) {
      case VpnStatus.connecting:
        return 'Connecting...';
      case VpnStatus.connected:
        return 'Connected';
      case VpnStatus.error:
        return 'Error';
      case VpnStatus.disconnected:
        return 'Disconnected';
    }
  }

  void _startStatusPolling() {
    _statusPoller?.cancel();
    _statusPoller = Timer.periodic(const Duration(milliseconds: 500), (_) async {
      final code = await api.getProxyStatus();
      final s = _mapStatus(code);

      if (s == VpnStatus.connected && _state.status != VpnStatus.connected) {
        _startSessionTimer();
      }
      if (s != VpnStatus.connected) {
        _stopSessionTimer();
      }

      _update(_state.copyWith(status: s, statusText: _statusText(s)));

      if (s == VpnStatus.disconnected || s == VpnStatus.error) {
        _stopStatusPolling();
      }
    });
  }

  void _stopStatusPolling() {
    _statusPoller?.cancel();
    _statusPoller = null;
  }

  void _startSessionTimer() {
    _connectedAt = DateTime.now();
    _sessionTimer?.cancel();
    _sessionTimer = Timer.periodic(const Duration(seconds: 1), (_) {
      if (_connectedAt != null && _state.status == VpnStatus.connected) {
        final elapsed = DateTime.now().difference(_connectedAt!);
        _update(_state.copyWith(connectionTime: elapsed));
      }
    });
  }

  void _stopSessionTimer() {
    _sessionTimer?.cancel();
    _sessionTimer = null;
    _connectedAt = null;
  }

  void dispose() {
    _stopStatusPolling();
    _stopSessionTimer();
    _stateController.close();
  }
}
