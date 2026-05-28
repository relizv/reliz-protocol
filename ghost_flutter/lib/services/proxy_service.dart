import 'dart:async';
import '../models/proxy_state.dart';

/// Сервис управления Ghost-прокси.
///
/// В реальном приложении методы вызывают Rust-ядро через flutter_rust_bridge.
/// Пока что — заглушки с эмуляцией для демонстрации UI.
class ProxyService {
  ProxyState _state = const ProxyState();
  final _stateController = StreamController<ProxyState>.broadcast();

  Stream<ProxyState> get stateStream => _stateController.stream;
  ProxyState get state => _state;

  void _update(ProxyState newState) {
    _state = newState;
    _stateController.add(_state);
  }

  /// Подключиться к Ghost-серверу.
  Future<void> connect({
    required String serverAddr,
    required String userId,
    required bool enablePadding,
    required bool enableFragmentation,
    required String maskDomain,
  }) async {
    _update(_state.copyWith(
      isConnected: false,
      serverAddr: serverAddr,
      userId: userId,
      enablePadding: enablePadding,
      enableFragmentation: enableFragmentation,
      maskDomain: maskDomain,
      statusText: 'Connecting...',
    ));

    // Эмуляция задержки подключения
    await Future.delayed(const Duration(seconds: 2));

    _update(_state.copyWith(
      isConnected: true,
      statusText: 'Connected',
    ));

    // Запускаем таймер сессии
    _startSessionTimer();
  }

  /// Отключиться.
  Future<void> disconnect() async {
    _update(_state.copyWith(
      isConnected: false,
      statusText: 'Disconnected',
      bytesIn: 0,
      bytesOut: 0,
      connectionTime: null,
    ));
  }

  Timer? _sessionTimer;
  DateTime? _connectedAt;

  void _startSessionTimer() {
    _connectedAt = DateTime.now();
    _sessionTimer?.cancel();
    _sessionTimer = Timer.periodic(const Duration(seconds: 1), (_) {
      if (_connectedAt != null && _state.isConnected) {
        final elapsed = DateTime.now().difference(_connectedAt!);
        // Эмуляция трафика
        final fakeIn = _state.bytesIn + 1024 + (elapsed.inSeconds % 512);
        final fakeOut = _state.bytesOut + 256 + (elapsed.inSeconds % 128);
        _update(_state.copyWith(
          connectionTime: elapsed,
          bytesIn: fakeIn,
          bytesOut: fakeOut,
        ));
      }
    });
  }

  void dispose() {
    _sessionTimer?.cancel();
    _stateController.close();
  }
}
