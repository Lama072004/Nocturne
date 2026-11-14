import 'package:flutter/material.dart';

import '../../l10n.dart';

/// Zeigt je nach Stand eines [Future] Ladekreis, Fehler mit "Erneut
/// versuchen" oder den Inhalt.
///
/// Fast jede Seite lädt etwas vom Server. Damit nicht jede Seite dieselben
/// drei Fälle neu schreibt (und einer davon vergessen wird), gibt es dieses
/// eine Widget.
class AsyncView<T> extends StatelessWidget {
  const AsyncView({
    super.key,
    required this.future,
    required this.builder,
    required this.onRetry,
  });

  final Future<T> future;
  final Widget Function(BuildContext context, T data) builder;
  final VoidCallback onRetry;

  @override
  Widget build(BuildContext context) {
    return FutureBuilder<T>(
      future: future,
      builder: (context, snap) {
        if (snap.hasError) {
          return Center(
            child: Column(
              mainAxisSize: MainAxisSize.min,
              children: [
                Padding(
                  padding: const EdgeInsets.all(24),
                  child: Text('${snap.error}', textAlign: TextAlign.center),
                ),
                FilledButton(
                  onPressed: onRetry,
                  child: Text(L.of(context).retry),
                ),
              ],
            ),
          );
        }
        // `connectionState` statt `hasData` prüfen: T darf auch `null` sein
        // (z. B. "kein Songtext vorhanden").
        if (snap.connectionState != ConnectionState.done) {
          return const Center(child: CircularProgressIndicator());
        }
        return builder(context, snap.data as T);
      },
    );
  }
}

/// Hinweis für leere Listen.
///
/// Ein leerer Bildschirm soll nie eine Sackgasse sein: [action] ist der Knopf
/// für den naheliegenden nächsten Schritt (z. B. "Zur Bibliothek").
class EmptyHint extends StatelessWidget {
  const EmptyHint({
    super.key,
    required this.icon,
    required this.text,
    this.detail,
    this.action,
  });

  final IconData icon;
  final String text;
  final String? detail;
  final Widget? action;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return Center(
      child: Padding(
        padding: const EdgeInsets.all(32),
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            Icon(icon, size: 56, color: theme.hintColor),
            const SizedBox(height: 16),
            Text(
              text,
              textAlign: TextAlign.center,
              style: theme.textTheme.titleMedium,
            ),
            if (detail != null) ...[
              const SizedBox(height: 8),
              Text(
                detail!,
                textAlign: TextAlign.center,
                style: theme.textTheme.bodyMedium?.copyWith(
                  color: theme.hintColor,
                ),
              ),
            ],
            if (action != null) ...[const SizedBox(height: 20), action!],
          ],
        ),
      ),
    );
  }
}
