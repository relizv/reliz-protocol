import 'package:flutter/material.dart';

class RelizTheme {
  RelizTheme._();

  static ThemeData get darkTheme {
    return ThemeData(
      brightness: Brightness.dark,
      primaryColor: const Color(0xFF00E676),
      scaffoldBackgroundColor: const Color(0xFF0A0E27),
      colorScheme: const ColorScheme.dark(
        primary: Color(0xFF00E676),
        secondary: Color(0xFF00B0FF),
        surface: Color(0xFF111633),
        error: Color(0xFFFF5252),
        onPrimary: Color(0xFF0A0E27),
        onSurface: Colors.white,
      ),
      cardTheme: CardThemeData(
        color: const Color(0xFF111633),
        elevation: 4,
        shape: RoundedRectangleBorder(
          borderRadius: BorderRadius.circular(16),
        ),
      ),
      elevatedButtonTheme: ElevatedButtonThemeData(
        style: ElevatedButton.styleFrom(
          backgroundColor: const Color(0xFF00E676),
          foregroundColor: const Color(0xFF0A0E27),
          shape: RoundedRectangleBorder(
            borderRadius: BorderRadius.circular(12),
          ),
          padding: const EdgeInsets.symmetric(horizontal: 32, vertical: 16),
          textStyle: const TextStyle(
            fontSize: 18,
            fontWeight: FontWeight.bold,
          ),
        ),
      ),
      inputDecorationTheme: InputDecorationTheme(
        filled: true,
        fillColor: const Color(0xFF1A2040),
        border: OutlineInputBorder(
          borderRadius: BorderRadius.circular(12),
          borderSide: BorderSide.none,
        ),
        focusedBorder: OutlineInputBorder(
          borderRadius: BorderRadius.circular(12),
          borderSide: const BorderSide(color: Color(0xFF00E676), width: 2),
        ),
        hintStyle: TextStyle(color: Colors.white.withOpacity(0.4)),
      ),
      switchTheme: SwitchThemeData(
        thumbColor: WidgetStateProperty.resolveWith((states) {
          if (states.contains(WidgetState.selected)) {
            return const Color(0xFF00E676);
          }
          return Colors.grey;
        }),
        trackColor: WidgetStateProperty.resolveWith((states) {
          if (states.contains(WidgetState.selected)) {
            return const Color(0xFF00E676).withOpacity(0.4);
          }
          return Colors.grey.withOpacity(0.3);
        }),
      ),
    );
  }
}
