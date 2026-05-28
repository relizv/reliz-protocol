import 'package:flutter/material.dart';
import 'screens/home_screen.dart';
import 'services/theme.dart';

void main() {
  runApp(const GhostProxyApp());
}

class GhostProxyApp extends StatelessWidget {
  const GhostProxyApp({super.key});

  @override
  Widget build(BuildContext context) {
    return MaterialApp(
      title: 'Ghost Proxy',
      theme: GhostTheme.darkTheme,
      home: const HomeScreen(),
      debugShowCheckedModeBanner: false,
    );
  }
}
