/// Модель состояния Reliz-прокси.
class ProxyState {
  final bool isConnected;
  final String serverAddr;
  final String userId;
  final bool enablePadding;
  final bool enableFragmentation;
  final String maskDomain;
  final String statusText;
  final int bytesIn;
  final int bytesOut;
  final Duration? connectionTime;

  const ProxyState({
    this.isConnected = false,
    this.serverAddr = '',
    this.userId = '',
    this.enablePadding = true,
    this.enableFragmentation = false,
    this.maskDomain = 'www.apple.com',
    this.statusText = 'Disconnected',
    this.bytesIn = 0,
    this.bytesOut = 0,
    this.connectionTime,
  });

  ProxyState copyWith({
    bool? isConnected,
    String? serverAddr,
    String? userId,
    bool? enablePadding,
    bool? enableFragmentation,
    String? maskDomain,
    String? statusText,
    int? bytesIn,
    int? bytesOut,
    Duration? connectionTime,
  }) {
    return ProxyState(
      isConnected: isConnected ?? this.isConnected,
      serverAddr: serverAddr ?? this.serverAddr,
      userId: userId ?? this.userId,
      enablePadding: enablePadding ?? this.enablePadding,
      enableFragmentation: enableFragmentation ?? this.enableFragmentation,
      maskDomain: maskDomain ?? this.maskDomain,
      statusText: statusText ?? this.statusText,
      bytesIn: bytesIn ?? this.bytesIn,
      bytesOut: bytesOut ?? this.bytesOut,
      connectionTime: connectionTime ?? this.connectionTime,
    );
  }
}
