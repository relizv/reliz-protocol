import 'dart:async';

import '../models/proxy_state.dart';
import '../src/rust/api.dart' as api;
import '../src/rust/frb_generated.dart';

/// Сервис управления Reliz-прокси.
///
/// Все методы — это тонкая обёртка вокруг сгенерированных
/// flutter_rust_bridge-биндингов в `lib/src/rust/`. Никаких фейковых
/// таймеров и `Future.delayed` здесь больше нет: реальный статус
/// соединения берётся из Rust через [api.getProxyStatus].
class ProxyService {
  ProxyService();

  ProxyState _state = const ProxyState();
  final _stateController = StreamController<ProxyState>.broadcast();

  /// Поллинг статуса из Rust (Stopped/Connecting/Connected/Error).
  Timer? _statusPoller;

  /// Таймер обновления длительности сессии.
  Timer? _sessionTimer;
  DateTime? _connectedAt;

  /// Гарантируем единственную инициализацию rust-моста на процесс.
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

  /// Подключиться к Reliz-серверу.
  ///
  /// Делегирует в нативный `start_reliz_proxy(...)` через flutter_rust_bridge,
  /// затем стартует поллер `get_proxy_status()` для обновления UI.
  Future<void> connect({
    required String serverAddr,
    required String userId,
    required bool enablePadding,
    required bool enableFragmentation,
    required String maskDomain,
  }) async {
    await _ensureRustInit();

    _update(_state.copyWith(
      isConnected: false,
      serverAddr: serverAddr,
      userId: userId,
      enablePadding: enablePadding,
      enableFragmentation: enableFragmentation,
      maskDomain: maskDomain,
      statusText: 'Connecting...',
    ));

    final rc = await api.startRelizProxy(
      serverAddr: serverAddr,
      userId: userId,
      enablePadding: enablePadding,
      enableFragmentation: enableFragmentation,
      maskDomain: maskDomain,
    );

    if (rc != 0) {
      _update(_state.copyWith(
        isConnected: false,
        statusText: 'Error (code $rc)',
      ));
      return;
    }

    _startStatusPolling();
  }

  /// Отключиться.
  Future<void> disconnect() async {
    await _ensureRustInit();
    await api.stopProxy();
    _stopStatusPolling();
    _stopSessionTimer();
    _update(_state.copyWith(
      isConnected: false,
      statusText: 'Disconnected',
      bytesIn: 0,
      bytesOut: 0,
      connectionTime: null,
    ));
  }

  /// Маппинг i32-кода из Rust (см. `ProxyStatus` в `api.rs`) на UI-строку.
  String _statusText(int code) {
    switch (code) {
      case 0:
        return 'Disconnected';
      case 1:
        return 'Connecting...';
      case 2:
        return 'Connected';
      case 3:
        return 'Error';
      default:
        return 'Unknown';
    }
  }

  void _startStatusPolling() {
    _statusPoller?.cancel();
    _statusPoller = Timer.periodic(const Duration(milliseconds: 500), (_) async {
      final code = await api.getProxyStatus();
      final connected = code == 2;
      final text = _statusText(code);

      if (connected && !_state.isConnected) {
        _startSessionTimer();
      }
      if (!connected) {
        _stopSessionTimer();
      }

      _update(_state.copyWith(
        isConnected: connected,
        statusText: text,
      ));

      if (code == 0 || code == 3) {
        // финальное состояние — поллить дальше нет смысла
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
      if (_connectedAt != null && _state.isConnected) {
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
