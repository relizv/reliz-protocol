// AUTO-GENERATED FILE — flutter_rust_bridge v2 shape.
//
// Этот файл является заглушкой, повторяющей публичный API сгенерированного
// flutter_rust_bridge кода. Чтобы получить настоящий код:
//
//     dart pub global activate flutter_rust_bridge_codegen
//     flutter_rust_bridge_codegen generate \
//         --rust-input crate::api \
//         --rust-root ghost_flutter/rust \
//         --dart-output ghost_flutter/lib/src/rust
//
// До этого момента приложение собирается, но реальный FFI-вызов уйдёт в
// заглушку — её можно использовать для UI-тестов без нативной библиотеки.

import 'dart:async';
import 'dart:ffi';
import 'dart:io';

/// Главная точка инициализации flutter_rust_bridge.
///
/// Реальная сгенерированная версия загружает `libghost_flutter_bridge` и
/// настраивает диспатчер. Здесь — no-op, чтобы Dart-код собирался без
/// нативной библиотеки.
class RustLib {
  RustLib._();

  static bool _initialized = false;
  static DynamicLibrary? _dylib;

  /// Инициализация моста. В реальной сборке загружает нативную библиотеку.
  static Future<void> init({DynamicLibrary? externalLibrary}) async {
    if (_initialized) return;
    _dylib = externalLibrary ?? _tryOpenDefaultLibrary();
    _initialized = true;
  }

  static DynamicLibrary? _tryOpenDefaultLibrary() {
    try {
      if (Platform.isAndroid || Platform.isLinux) {
        return DynamicLibrary.open('libghost_flutter_bridge.so');
      }
      if (Platform.isIOS || Platform.isMacOS) {
        return DynamicLibrary.process();
      }
      if (Platform.isWindows) {
        return DynamicLibrary.open('ghost_flutter_bridge.dll');
      }
    } catch (_) {
      // Нативная библиотека не собрана — оставляем null, заглушка отработает в Dart.
    }
    return null;
  }

  /// Возвращает дескриптор подгруженной библиотеки, если есть.
  static DynamicLibrary? get dylib => _dylib;

  /// Сбросить состояние (для тестов).
  static void dispose() {
    _initialized = false;
    _dylib = null;
  }
}
