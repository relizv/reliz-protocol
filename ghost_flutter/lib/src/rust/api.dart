// AUTO-GENERATED FILE — flutter_rust_bridge v2 shape.
//
// Зеркалит публичные функции из `ghost_flutter/rust/src/api.rs`. Когда
// будет настроен Flutter SDK и установлен flutter_rust_bridge_codegen,
// этот файл следует пересоздать через:
//
//     flutter_rust_bridge_codegen generate \
//         --rust-input crate::api \
//         --rust-root ghost_flutter/rust \
//         --dart-output ghost_flutter/lib/src/rust
//
// Сигнатуры функций совпадают с тем, что генерирует FRB v2: snake_case
// Rust → camelCase Dart, аргументы по именам, возврат через Future<T>.

import 'frb_generated.dart';

/// Статус прокси-соединения.
/// Совпадает с `enum ProxyStatus` в Rust (см. `api.rs`).
enum ProxyStatus {
  stopped,
  connecting,
  connected,
  error,
}

ProxyStatus proxyStatusFromCode(int code) {
  switch (code) {
    case 0:
      return ProxyStatus.stopped;
    case 1:
      return ProxyStatus.connecting;
    case 2:
      return ProxyStatus.connected;
    case 3:
      return ProxyStatus.error;
    default:
      return ProxyStatus.error;
  }
}

/// Запустить Reliz-прокси.
///
/// Соответствует Rust `start_reliz_proxy(...)`.
Future<int> startRelizProxy({
  required String serverAddr,
  required String userId,
  required bool enablePadding,
  required bool enableFragmentation,
  required String maskDomain,
}) async {
  await RustLib.init();
  // Реальная реализация: вызов FFI в libghost_flutter_bridge.
  // Здесь — заглушка, возвращающая 0 (success), чтобы UI был работоспособен
  // до того, как пользователь сгенерирует настоящие биндинги.
  return 0;
}

/// Остановить прокси. Rust `stop_proxy()`.
Future<int> stopProxy() async {
  await RustLib.init();
  return 0;
}

/// Получить текущий статус прокси (i32). Rust `get_proxy_status()`.
Future<int> getProxyStatus() async {
  await RustLib.init();
  // Без нативной библиотеки честно сообщаем «отключено».
  return 0;
}

/// Проверить, запущен ли прокси. Rust `is_proxy_running()`.
Future<bool> isProxyRunning() async {
  await RustLib.init();
  return false;
}

/// Протестировать подключение к серверу. Rust `test_connection(server_addr)`.
Future<int> testConnection({required String serverAddr}) async {
  await RustLib.init();
  return 0;
}

/// Версия протокола. Rust `get_protocol_version()`.
Future<int> getProtocolVersion() async {
  await RustLib.init();
  return 1;
}
