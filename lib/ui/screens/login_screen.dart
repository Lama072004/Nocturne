import 'package:flutter/material.dart';

import '../../app_state.dart';
import '../../l10n.dart';
import '../../l10n/detail_l10n.dart';
import '../../src/rust/api/library.dart' as rust;
import '../../src/rust/api/models.dart';
import '../widgets/logo.dart';

/// Anmeldung am eigenen Server (Navidrome/Subsonic, Jellyfin, Emby, Plex).
///
/// Das Passwort verlässt dieses Formular nur in Richtung Rust. Dort wird
/// daraus sofort ein Token berechnet; gespeichert wird nur das Token, nie
/// das Passwort selbst (siehe `server/subsonic.rs`).
class LoginScreen extends StatefulWidget {
  const LoginScreen({super.key});

  @override
  State<LoginScreen> createState() => _LoginScreenState();
}

class _LoginScreenState extends State<LoginScreen> {
  final _url = TextEditingController();
  final _user = TextEditingController();
  final _password = TextEditingController();
  ServerKind _kind = ServerKind.subsonic;
  bool _busy = false;
  bool _showPassword = false;
  String? _error;

  @override
  void dispose() {
    _url.dispose();
    _user.dispose();
    _password.dispose();
    super.dispose();
  }

  /// Füllt die Felder mit dem öffentlichen Navidrome-Demo-Server. Meldet
  /// bewusst NICHT an: Der Nutzer soll sehen, was eingetragen wurde.
  void _fillDemo() {
    setState(() {
      _kind = ServerKind.subsonic;
      _url.text = 'demo.navidrome.org';
      _user.text = 'demo';
      _password.text = 'demo';
      _error = null;
    });
  }

  /// Entfernt Internet-Adressen aus Fehlermeldungen: Dort können
  /// Anmeldedaten stecken (siehe UI_PLAN Abschnitt 10).
  String _clean(Object e, L l) {
    var text = '$e'
        .replaceAll(RegExp(r'^(Exception|Error|AnyhowException)[:(]?\s*'), '')
        .replaceAll(RegExp(r'https?://\S+', caseSensitive: false), '…')
        .trim();
    if (text.isEmpty) text = l.loginFailedGeneric;
    return text;
  }

  Future<void> _submit() async {
    final l = L.of(context);
    // Bei Plex darf der Benutzername leer sein (dann gilt das Passwortfeld
    // als Plex-Token); sonst müssen alle drei Felder gefüllt sein.
    final userOptional = _kind == ServerKind.plex;
    if (_url.text.trim().isEmpty ||
        _password.text.isEmpty ||
        (!userOptional && _user.text.trim().isEmpty)) {
      setState(
        () => _error = userOptional ? l.fillUrlAndSecret : l.fillRequiredFields,
      );
      return;
    }
    setState(() {
      _busy = true;
      _error = null;
    });
    try {
      await rust.login(
        kind: _kind,
        // Ohne "http://" ist erlaubt: Rust probiert HTTPS, dann HTTP.
        url: _url.text.trim(),
        username: _user.text.trim(),
        password: _password.text,
      );
      // Passwort sofort aus dem Eingabefeld (und damit aus dem RAM der UI) nehmen.
      _password.clear();
      AppState.instance.loggedIn.value = true;
    } catch (e) {
      if (mounted) setState(() => _error = _clean(e, l));
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  String _hint(L l) => switch (_kind) {
    ServerKind.subsonic => l.hintSubsonic,
    ServerKind.jellyfin => l.hintJellyfin,
    ServerKind.emby => l.hintEmby,
    ServerKind.plex => l.hintPlex,
  };

  @override
  Widget build(BuildContext context) {
    final l = L.of(context);
    final theme = Theme.of(context);
    final untested = _kind == ServerKind.emby || _kind == ServerKind.plex;
    return Scaffold(
      body: SafeArea(
        child: Center(
          child: SingleChildScrollView(
            padding: const EdgeInsets.all(24),
            child: ConstrainedBox(
              constraints: const BoxConstraints(maxWidth: 420),
              // AutofillGroup: Passwort-Manager können die Felder ausfüllen.
              child: AutofillGroup(
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.stretch,
                  children: [
                    const Center(child: NocturneLogo(size: 72)),
                    const SizedBox(height: 16),
                    Text(
                      l.loginTitle,
                      textAlign: TextAlign.center,
                      style: theme.textTheme.headlineSmall?.copyWith(
                        fontWeight: FontWeight.bold,
                      ),
                    ),
                    const SizedBox(height: 32),
                    // Alle vier Servertypen sind wählbar; Rust beherrscht sie.
                    DropdownButtonFormField<ServerKind>(
                      initialValue: _kind,
                      decoration: InputDecoration(labelText: l.serverType),
                      items: const [
                        DropdownMenuItem(
                          value: ServerKind.subsonic,
                          child: Text('Navidrome / Subsonic'),
                        ),
                        DropdownMenuItem(
                          value: ServerKind.jellyfin,
                          child: Text('Jellyfin'),
                        ),
                        DropdownMenuItem(
                          value: ServerKind.emby,
                          child: Text('Emby'),
                        ),
                        DropdownMenuItem(
                          value: ServerKind.plex,
                          child: Text('Plex'),
                        ),
                      ],
                      onChanged: _busy
                          ? null
                          : (v) => setState(() => _kind = v ?? _kind),
                    ),
                    // Kurzer Hinweis je Servertyp.
                    Padding(
                      padding: const EdgeInsets.fromLTRB(4, 8, 4, 0),
                      child: Text(
                        untested ? '${_hint(l)} ${l.hintUntested}' : _hint(l),
                        style: theme.textTheme.bodySmall?.copyWith(
                          color: theme.hintColor,
                        ),
                      ),
                    ),
                    const SizedBox(height: 16),
                    TextField(
                      controller: _url,
                      enabled: !_busy,
                      keyboardType: TextInputType.url,
                      autocorrect: false,
                      autofillHints: const [AutofillHints.url],
                      decoration: InputDecoration(
                        labelText: l.serverUrl,
                        hintText: l.serverUrlHint,
                      ),
                    ),
                    const SizedBox(height: 12),
                    TextField(
                      controller: _user,
                      enabled: !_busy,
                      autocorrect: false,
                      autofillHints: const [AutofillHints.username],
                      decoration: InputDecoration(labelText: l.username),
                    ),
                    const SizedBox(height: 12),
                    TextField(
                      controller: _password,
                      enabled: !_busy,
                      obscureText: !_showPassword,
                      autocorrect: false,
                      enableSuggestions: false,
                      autofillHints: const [AutofillHints.password],
                      onSubmitted: (_) => _busy ? null : _submit(),
                      decoration: InputDecoration(
                        labelText: l.password,
                        // Auge zum Ein-/Ausblenden (Tippfehler finden).
                        suffixIcon: IconButton(
                          tooltip: _showPassword
                              ? l.hidePassword
                              : l.showPassword,
                          icon: Icon(
                            _showPassword
                                ? Icons.visibility_off_outlined
                                : Icons.visibility_outlined,
                          ),
                          onPressed: () =>
                              setState(() => _showPassword = !_showPassword),
                        ),
                      ),
                    ),
                    // Fehler gut lesbar in einem Kasten über dem Knopf.
                    if (_error != null) ...[
                      const SizedBox(height: 16),
                      Semantics(
                        liveRegion: true,
                        child: Container(
                          padding: const EdgeInsets.all(12),
                          decoration: BoxDecoration(
                            color: theme.colorScheme.errorContainer,
                            borderRadius: BorderRadius.circular(8),
                          ),
                          child: Row(
                            crossAxisAlignment: CrossAxisAlignment.start,
                            children: [
                              Icon(
                                Icons.error_outline,
                                size: 20,
                                color: theme.colorScheme.onErrorContainer,
                              ),
                              const SizedBox(width: 10),
                              Expanded(
                                child: Text(
                                  _error!,
                                  style: TextStyle(
                                    color: theme.colorScheme.onErrorContainer,
                                  ),
                                ),
                              ),
                            ],
                          ),
                        ),
                      ),
                    ],
                    const SizedBox(height: 24),
                    FilledButton(
                      onPressed: _busy ? null : _submit,
                      child: _busy
                          ? Row(
                              mainAxisAlignment: MainAxisAlignment.center,
                              mainAxisSize: MainAxisSize.min,
                              children: [
                                const SizedBox(
                                  width: 18,
                                  height: 18,
                                  child: CircularProgressIndicator(
                                    strokeWidth: 2,
                                  ),
                                ),
                                const SizedBox(width: 12),
                                Text(l.connecting),
                              ],
                            )
                          : Text(l.connect),
                    ),
                    const SizedBox(height: 8),
                    Tooltip(
                      message: l.tryDemoHint,
                      child: TextButton(
                        onPressed: _busy ? null : _fillDemo,
                        child: Text(l.tryDemo),
                      ),
                    ),
                  ],
                ),
              ),
            ),
          ),
        ),
      ),
    );
  }
}
