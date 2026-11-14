import 'package:flutter/material.dart';

import '../../l10n.dart';
import '../../src/rust/api/session.dart';
import '../screens/profile_screen.dart';

/// Runder Knopf mit dem Anfangsbuchstaben des Benutzers, oben links auf
/// Home, Suche und Bibliothek. Er öffnet die Profilseite.
///
/// Immer an derselben Stelle auf allen drei Tabs: So findet man Statistik,
/// Downloads und Einstellungen, ohne zu überlegen, in welchem Tab man ist.
class ProfileButton extends StatelessWidget {
  const ProfileButton({super.key});

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final l = L.of(context);
    final name = sessionInfo()?.username ?? '';
    final letter = name.isEmpty ? null : name.characters.first.toUpperCase();

    return Padding(
      padding: const EdgeInsets.only(left: 12),
      child: Center(
        child: Semantics(
          button: true,
          label: l.profile,
          child: InkWell(
            customBorder: const CircleBorder(),
            onTap: () => Navigator.of(context).push(
              MaterialPageRoute(builder: (_) => const ProfileScreen()),
            ),
            child: Padding(
              // Sichtbar 32 px, Berührungsfläche 48 px: klein im Bild, aber
              // sicher zu treffen.
              padding: const EdgeInsets.all(8),
              child: CircleAvatar(
                radius: 16,
                backgroundColor: theme.colorScheme.primary,
                child: letter == null
                    ? Icon(Icons.person, size: 18, color: theme.colorScheme.onPrimary)
                    : Text(
                        letter,
                        style: TextStyle(
                          color: theme.colorScheme.onPrimary,
                          fontWeight: FontWeight.bold,
                          fontSize: 15,
                        ),
                      ),
              ),
            ),
          ),
        ),
      ),
    );
  }
}
