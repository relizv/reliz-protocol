import 'package:flutter/material.dart';

import '../config/reliz_config.dart';
import '../models/proxy_state.dart';
import '../services/proxy_service.dart';

class HomeScreen extends StatefulWidget {
  const HomeScreen({super.key});

  @override
  State<HomeScreen> createState() => _HomeScreenState();
}

class _HomeScreenState extends State<HomeScreen>
    with SingleTickerProviderStateMixin {
  final ProxyService _service = ProxyService();

  late AnimationController _pulseController;
  late Animation<double> _pulseAnimation;

  @override
  void initState() {
    super.initState();
    _pulseController = AnimationController(
      vsync: this,
      duration: const Duration(seconds: 2),
    )..repeat(reverse: true);

    _pulseAnimation = Tween<double>(begin: 0.85, end: 1.0).animate(
      CurvedAnimation(parent: _pulseController, curve: Curves.easeInOut),
    );
  }

  @override
  void dispose() {
    _pulseController.dispose();
    _service.dispose();
    super.dispose();
  }

  Future<void> _toggleConnection(ProxyState state) async {
    if (state.isConnected) {
      await _service.disconnect();
    } else {
      await _service.connect();
    }
  }

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      body: SafeArea(
        child: StreamBuilder<ProxyState>(
          stream: _service.stateStream,
          initialData: _service.state,
          builder: (context, snapshot) {
            final state = snapshot.data ?? const ProxyState();
            return SingleChildScrollView(
              padding: const EdgeInsets.all(24),
              child: Column(
                children: [
                  _buildHeader(),
                  const SizedBox(height: 32),
                  _buildConnectButton(state),
                  const SizedBox(height: 8),
                  _buildStatusLabel(state),
                  const SizedBox(height: 32),
                  if (state.isConnected) ...[
                    _buildStatsCard(state),
                    const SizedBox(height: 24),
                  ],
                  _buildServerCard(),
                  const SizedBox(height: 24),
                  _buildStealthCard(),
                ],
              ),
            );
          },
        ),
      ),
    );
  }

  Widget _buildHeader() {
    return Column(
      children: [
        Icon(
          Icons.shield_rounded,
          size: 48,
          color: Theme.of(context).colorScheme.primary,
        ),
        const SizedBox(height: 12),
        Text(
          'RELIZ PROTOCOL',
          style: Theme.of(context).textTheme.headlineMedium?.copyWith(
                fontWeight: FontWeight.bold,
                letterSpacing: 4,
                color: Theme.of(context).colorScheme.primary,
              ),
        ),
        const SizedBox(height: 4),
        Text(
          'Stealth VPN • Protocol v1',
          style: Theme.of(context).textTheme.bodyMedium?.copyWith(
                color: Colors.white54,
              ),
        ),
      ],
    );
  }

  Widget _buildConnectButton(ProxyState state) {
    final isConnected = state.isConnected;
    final isConnecting = state.isConnecting;

    return AnimatedBuilder(
      animation: _pulseAnimation,
      builder: (context, child) {
        final scale = isConnected ? _pulseAnimation.value : 1.0;
        return Transform.scale(
          scale: scale,
          child: GestureDetector(
            onTap: isConnecting ? null : () => _toggleConnection(state),
            child: Container(
              width: 180,
              height: 180,
              decoration: BoxDecoration(
                shape: BoxShape.circle,
                gradient: LinearGradient(
                  begin: Alignment.topLeft,
                  end: Alignment.bottomRight,
                  colors: isConnected
                      ? [const Color(0xFF00E676), const Color(0xFF00C853)]
                      : isConnecting
                          ? [const Color(0xFFFFB74D), const Color(0xFFFF9800)]
                          : [const Color(0xFF424242), const Color(0xFF212121)],
                ),
                boxShadow: isConnected
                    ? [
                        BoxShadow(
                          color: const Color(0xFF00E676).withOpacity(0.4),
                          blurRadius: 30,
                          spreadRadius: 5,
                        ),
                      ]
                    : null,
              ),
              child: Column(
                mainAxisAlignment: MainAxisAlignment.center,
                children: [
                  Icon(
                    isConnected
                        ? Icons.vpn_lock
                        : isConnecting
                            ? Icons.hourglass_empty
                            : Icons.power_settings_new,
                    size: 56,
                    color: isConnected
                        ? const Color(0xFF0A0E27)
                        : Colors.white70,
                  ),
                  const SizedBox(height: 8),
                  Text(
                    isConnected
                        ? 'CONNECTED'
                        : isConnecting
                            ? 'CONNECTING'
                            : 'TAP TO\nCONNECT',
                    textAlign: TextAlign.center,
                    style: TextStyle(
                      fontSize: 14,
                      fontWeight: FontWeight.bold,
                      color: isConnected
                          ? const Color(0xFF0A0E27)
                          : Colors.white70,
                      letterSpacing: 1,
                    ),
                  ),
                ],
              ),
            ),
          ),
        );
      },
    );
  }

  Widget _buildStatusLabel(ProxyState state) {
    return Padding(
      padding: const EdgeInsets.only(top: 12),
      child: Row(
        mainAxisAlignment: MainAxisAlignment.center,
        children: [
          Container(
            width: 8,
            height: 8,
            decoration: BoxDecoration(
              shape: BoxShape.circle,
              color: state.isConnected
                  ? const Color(0xFF00E676)
                  : state.isConnecting
                      ? Colors.orange
                      : Colors.grey,
            ),
          ),
          const SizedBox(width: 8),
          Text(
            state.statusText,
            style: TextStyle(
              color: state.isConnected ? const Color(0xFF00E676) : Colors.grey,
              fontWeight: FontWeight.w500,
            ),
          ),
        ],
      ),
    );
  }

  Widget _buildStatsCard(ProxyState state) {
    return Card(
      child: Padding(
        padding: const EdgeInsets.all(20),
        child: Column(
          children: [
            Text(
              'SESSION STATS',
              style: Theme.of(context).textTheme.titleSmall?.copyWith(
                    color: Colors.white54,
                    letterSpacing: 2,
                  ),
            ),
            const SizedBox(height: 16),
            Row(
              mainAxisAlignment: MainAxisAlignment.spaceEvenly,
              children: [
                _buildStatItem(
                  icon: Icons.arrow_downward,
                  label: 'Download',
                  value: _formatBytes(state.bytesIn),
                  color: const Color(0xFF00B0FF),
                ),
                _buildStatItem(
                  icon: Icons.arrow_upward,
                  label: 'Upload',
                  value: _formatBytes(state.bytesOut),
                  color: const Color(0xFF00E676),
                ),
                _buildStatItem(
                  icon: Icons.timer,
                  label: 'Duration',
                  value: _formatDuration(state.connectionTime),
                  color: const Color(0xFFFFB74D),
                ),
              ],
            ),
          ],
        ),
      ),
    );
  }

  Widget _buildStatItem({
    required IconData icon,
    required String label,
    required String value,
    required Color color,
  }) {
    return Column(
      children: [
        Icon(icon, color: color, size: 24),
        const SizedBox(height: 4),
        Text(
          value,
          style: const TextStyle(fontSize: 16, fontWeight: FontWeight.bold),
        ),
        Text(
          label,
          style: TextStyle(fontSize: 11, color: Colors.white.withOpacity(0.5)),
        ),
      ],
    );
  }

  /// Карточка с преднастроенным сервером (вместо полей ввода).
  Widget _buildServerCard() {
    return Card(
      child: Padding(
        padding: const EdgeInsets.all(20),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Text(
              'SERVER',
              style: Theme.of(context).textTheme.titleSmall?.copyWith(
                    color: Colors.white54,
                    letterSpacing: 2,
                  ),
            ),
            const SizedBox(height: 12),
            _buildInfoRow(Icons.dns, 'Endpoint', RelizConfig.serverAddr),
            const SizedBox(height: 10),
            _buildInfoRow(
              Icons.visibility_off,
              'Mask (SNI)',
              RelizConfig.maskDomain,
            ),
          ],
        ),
      ),
    );
  }

  Widget _buildInfoRow(IconData icon, String label, String value) {
    return Row(
      children: [
        Icon(icon, size: 20, color: Colors.white38),
        const SizedBox(width: 12),
        Text(
          label,
          style: const TextStyle(color: Colors.white54, fontSize: 13),
        ),
        const Spacer(),
        Flexible(
          child: Text(
            value,
            textAlign: TextAlign.right,
            overflow: TextOverflow.ellipsis,
            style: const TextStyle(
              fontWeight: FontWeight.w600,
              fontSize: 13,
            ),
          ),
        ),
      ],
    );
  }

  /// Stealth-опции всегда включены — тумблеры убраны, показываем статус.
  Widget _buildStealthCard() {
    return Card(
      child: Padding(
        padding: const EdgeInsets.all(20),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Text(
              'STEALTH • ALWAYS ON',
              style: Theme.of(context).textTheme.titleSmall?.copyWith(
                    color: Colors.white54,
                    letterSpacing: 2,
                  ),
            ),
            const SizedBox(height: 8),
            _buildStealthRow(
              Icons.blur_on,
              'Dynamic Padding',
              'Adds random padding to break size-based traffic analysis',
            ),
            _buildStealthRow(
              Icons.call_split,
              'TCP Fragmentation',
              'Splits TLS ClientHello to bypass DPI signatures (ByeDPI)',
            ),
            _buildStealthRow(
              Icons.verified_user,
              'Reality Masking',
              'Server masquerades as mask domain when scanned',
            ),
            _buildStealthRow(
              Icons.fingerprint,
              'JA4 Fingerprint Spoofing',
              'Mimics Chrome 131 TLS fingerprint',
            ),
          ],
        ),
      ),
    );
  }

  Widget _buildStealthRow(IconData icon, String title, String subtitle) {
    return ListTile(
      contentPadding: EdgeInsets.zero,
      leading: Icon(icon, color: const Color(0xFF00B0FF)),
      title: Text(title),
      subtitle: Text(
        subtitle,
        style: const TextStyle(fontSize: 12, color: Colors.white54),
      ),
      trailing: const Icon(Icons.check_circle, color: Color(0xFF00E676)),
    );
  }

  String _formatBytes(int bytes) {
    if (bytes < 1024) return '$bytes B';
    if (bytes < 1024 * 1024) return '${(bytes / 1024).toStringAsFixed(1)} KB';
    return '${(bytes / (1024 * 1024)).toStringAsFixed(1)} MB';
  }

  String _formatDuration(Duration? d) {
    if (d == null) return '--:--';
    final h = d.inHours.toString().padLeft(2, '0');
    final m = (d.inMinutes % 60).toString().padLeft(2, '0');
    final s = (d.inSeconds % 60).toString().padLeft(2, '0');
    return '$h:$m:$s';
  }
}
