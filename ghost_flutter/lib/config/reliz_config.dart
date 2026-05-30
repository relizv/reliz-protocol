/// Захардкоженная конфигурация Reliz-клиента.
///
/// На сервере используется единственный токен (UUID), поэтому вводить
/// сервер / ключ / SNI вручную не нужно — всё зашито здесь. Чтобы выпустить
/// сборку под другой сервер или токен, поменяй значения ниже и пересобери APK.
class RelizConfig {
  RelizConfig._();

  /// Адрес Reliz-сервера в формате `host:port`.
  static const String serverAddr = 'reliz.example.com:443';

  /// Единственный токен пользователя (UUID, 32 hex-символа без дефисов).
  static const String userId = '00000000000000000000000000000001';

  /// Домен маскировки SNI (Reality).
  static const String maskDomain = 'www.apple.com';

  /// Локальный SOCKS5-эндпоинт, на который смотрит tun2socks.
  static const String socksHost = '127.0.0.1';
  static const int socksPort = 10808;

  /// Stealth-функции включены всегда (тумблеры убраны из UI).
  static const bool enablePadding = true;
  static const bool enableFragmentation = true;
}
