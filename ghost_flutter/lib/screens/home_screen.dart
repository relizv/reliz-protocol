import 'package:flutter/material.dart';
import '../services/proxy_service.dart';
import '../services/theme.dart';
import '../models/proxy_state.dart';

class HomeScreen extends StatefulWidget {
  const HomeScreen({super.key});

  @override
  State<HomeScreen> createState() => _HomeScreenState();
}

class _HomeScreenState extends State<HomeScreen> with SingleTickerProviderStateMixin {
  final ProxyService _service = ProxyService();

  final _serverController = TextEditingController(text: 'ghost.example.com:443');
  final _userIdController = TextEditingController(text: '00000000000000000000000000000001');
  final _maskDomainController = TextEditingController(text: 'www.apple.com');

  bool _enablePadding = true;
  bool _enableFragmentation = false;
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
                  _buildConfigCard(state),
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
          'GHOST PROXY',
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
    final isConnecting = state.statusText == 'Connecting...';

    return AnimatedBuilder(
      animation: _pulseAnimation,
      builder: (context, child) {
        final scale = isConnected ? _pulseAnimation.value : 1.0;
        return Transform.scale(
          scale: scale,
          child: GestureDetector(
            onTap: isConnecting
                ? null
                : () => _toggleConnection(state),
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
                  : state.statusText == 'Connecting...'
                      ? Colors.orange
                      : Colors.grey,
            ),
          ),
          const SizedBox(width: 8),
          Text(
            state.statusText,
            style: TextStyle(
              color: state.isConnected
                  ? const Color(0xFF00E676)
                  : Colors.grey,
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
          style: const TextStyle(
            fontSize: 16,
            fontWeight: FontWeight.bold,
          ),
        ),
        Text(
          label,
          style: TextStyle(
            fontSize: 11,
            color: Colors.white.withOpacity(0.5),
          ),
        ),
      ],
    );
  }

  Widget _buildConfigCard(ProxyState state) {
    return Card(
      child: Padding(
        padding: const EdgeInsets.all(20),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Text(
              'CONFIGURATION',
              style: Theme.of(context).textTheme.titleSmall?.copyWith(
                    color: Colors.white54,
                    letterSpacing: 2,
                  ),
            ),
            const SizedBox(height: 16),
            TextField(
              controller: _serverController,
              enabled: !state.isConnected,
              decoration: const InputDecoration(
                labelText: 'Ghost Server',
                hintText: 'ghost.example.com:443',
                prefixIcon: Icon(Icons.dns),
              ),
            ),
            const SizedBox(height: 12),
            TextField(
              controller: _userIdController,
              enabled: !state.isConnected,
              decoration: const InputDecoration(
                labelText: 'User ID (UUID)',
                hintText: '00000000-0000-0000-0000-000000000001',
                prefixIcon: Icon(Icons.person),
              ),
            ),
            const SizedBox(height: 12),
            TextField(
              controller: _maskDomainController,
              enabled: !state.isConnected,
              decoration: const InputDecoration(
                labelText: 'Mask Domain (SNI)',
                hintText: 'www.apple.com',
                prefixIcon: Icon(Icons.visibility_off),
              ),
            ),
          ],
        ),
      ),
    );
  }

  Widget _buildStealthCard() {
    return Card(
      child: Padding(
        padding: const EdgeInsets.all(20),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Text(
              'STEALTH OPTIONS',
              style: Theme.of(context).textTheme.titleSmall?.copyWith(
                    color: Colors.white54,
                    letterSpacing: 2,
                  ),
            ),
            const SizedBox(height: 16),
            SwitchListTile(
              title: const Text('Dynamic Padding'),
              subtitle: const Text(
                'Adds random padding to break size-based traffic analysis',
                style: TextStyle(fontSize: 12, color: Colors.white54),
              ),
              value: _enablePadding,
              onChanged: (v) => setState(() => _enablePadding = v),
            ),
            SwitchListTile(
              title: const Text('TCP Fragmentation'),
              subtitle: const Text(
                'Splits TLS ClientHello to bypass DPI signature detection (ByeDPI)',
                style: TextStyle(fontSize: 12, color: Colors.white54),
              ),
              value: _enableFragmentation,
              onChanged: (v) => setState(() => _enableFragmentation = v),
            ),
            const ListTile(
              leading: Icon(Icons.verified_user, color: Color(0xFF00E676)),
              title: Text('Reality Masking'),
              subtitle: Text(
                'Server masquerades as mask domain when scanned',
                style: TextStyle(fontSize: 12, color: Colors.white54),
              ),
              trailing: Icon(Icons.check_circle, color: Color(0xFF00E676)),
            ),
            const ListTile(
              leading: Icon(Icons.fingerprint, color: Color(0xFF00B0FF)),
              title: Text('JA4 Fingerprint Spoofing'),
              subtitle: Text(
                'Mimics Chrome 131 TLS fingerprint',
                style: TextStyle(fontSize: 12, color: Colors.white54),
              ),
              trailing: Icon(Icons.check_circle, color: Color(0xFF00E676)),
            ),
          ],
        ),
      ),
    );
  }

  Future<void> _toggleConnection(ProxyState state) async {
    if (state.isConnected) {
      await _service.disconnect();
    } else {
      await _service.connect(
        serverAddr: _serverController.text,
        userId: _userIdController.text,
        enablePadding: _enablePadding,
        enableFragmentation: _enableFragmentation,
        maskDomain: _maskDomainController.text,
      );
    }
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
