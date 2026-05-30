/// Статус VPN-соединения (зеркалит `ProxyStatus` из Rust-моста).
enum VpnStatus { disconnected, connecting, connected, error }

/// Модель состояния Reliz-VPN.
///
/// Конфигурация (сервер, токен, SNI) больше не хранится здесь — она зашита в
/// [RelizConfig], а stealth-опции всегда включены. Поэтому модель описывает
/// только то, что меняется в рантайме: статус и статистику сессии.
class ProxyState {
  final VpnStatus status;
  final String statusText;
  final int bytesIn;
  final int bytesOut;
  final Duration? connectionTime;

  const ProxyState({
    this.status = VpnStatus.disconnected,
    this.statusText = 'Disconnected',
    this.bytesIn = 0,
    this.bytesOut = 0,
    this.connectionTime,
  });

  bool get isConnected => status == VpnStatus.connected;
  bool get isConnecting => status == VpnStatus.connecting;

  ProxyState copyWith({
    VpnStatus? status,
    String? statusText,
    int? bytesIn,
    int? bytesOut,
    Duration? connectionTime,
  }) {
    return ProxyState(
      status: status ?? this.status,
      statusText: statusText ?? this.statusText,
      bytesIn: bytesIn ?? this.bytesIn,
      bytesOut: bytesOut ?? this.bytesOut,
      connectionTime: connectionTime ?? this.connectionTime,
    );
  }
}
