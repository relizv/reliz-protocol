import 'package:flutter/material.dart';
import 'screens/home_screen.dart';
import 'services/theme.dart';

void main() {
  runApp(const RelizProtocolApp());
}

class RelizProtocolApp extends StatelessWidget {
  const RelizProtocolApp({super.key});

  @override
  Widget build(BuildContext context) {
    return MaterialApp(
      title: 'Reliz Protocol',
      theme: RelizTheme.darkTheme,
      home: const HomeScreen(),
      debugShowCheckedModeBanner: false,
    );
  }
}
