import 'dart:async';

import 'package:connectivity_plus/connectivity_plus.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_localizations/flutter_localizations.dart';
import 'package:path_provider/path_provider.dart';

import 'app_state.dart';
import 'downloads/auto_sync.dart';
import 'prefs.dart';
import 'src/rust/api/app.dart';
import 'src/rust/api/library.dart' as rust;
import 'src/rust/api/settings.dart';
import 'src/rust/api/stats.dart' as stats;
import 'src/rust/frb_generated.dart';
import 'theme/app_theme.dart';
import 'ui/screens/login_screen.dart';
import 'ui/shell.dart';

/// Start der App. Reihenfolge ist wichtig:
///  1. Flutter-Bindung, sonst gehen Plattform-Kanäle noch nicht.
///  2. Rust-Bibliothek laden (`RustLib.init`).
///  3. Rust-Kern einrichten: Datenverzeichnis übergeben, Einstellungen und
///     gespeicherte Anmeldung laden. Das passiert OHNE Netzwerk, damit die
///     App auch offline sofort startet.
///  4. Erst dann die UI starten. So kennt sie beim ersten Bild schon Theme
///     und Anmeldestatus und flackert nicht.
Future<void> main() async {
  WidgetsFlutterBinding.ensureInitialized();

  // WICHTIG: Schlägt die Einrichtung fehl, MUSS trotzdem etwas angezeigt
  // werden. Ohne dieses try/catch würde `runApp` nie erreicht, und die App
  // bliebe für immer auf dem Startbild stehen – ohne jeden Hinweis, warum.
  // (Genau das ist auf einem Gerät mit 16-KB-Speicherseiten passiert, als
  // die Rust-Bibliothek nicht geladen werden konnte.)
  try {
    await _startUp();
  } catch (error, stack) {
    runApp(StartupErrorApp(error: error, stack: stack));
    return;
  }

  runApp(const MusicApp());
}

Future<void> _startUp() async {
  // Zeitlimits: Lieber nach wenigen Sekunden eine klare Fehlermeldung als
  // ein endloses Startbild, falls ein Schritt wider Erwarten hängt.
  await RustLib.init().timeout(const Duration(seconds: 20));

  // "Application Support" = privater App-Ordner, der NICHT im Dateimanager
  // sichtbar ist und bei Deinstallation gelöscht wird. Richtig für
  // Einstellungen und Anmeldedaten.
  final dir = await getApplicationSupportDirectory();
  await initCore(
    dataDir: dir.path,
    storageKey: await _storageKey(),
  ).timeout(const Duration(seconds: 20));
  // Merkzettel der Oberfläche vorladen (Ansicht, Sortierung …), damit die
  // erste Seite gleich in der gewohnten Darstellung erscheint.
  await Prefs.instance.init();

  AppState.instance.player.init();
  // Download-Einstellungen an Android geben und den Stand der Downloads laden.
  // Nicht mit `await`: Der App-Start soll darauf nicht warten müssen. Fehler
  // hier (z. B. Speicher nicht lesbar) dürfen den Start nicht verhindern.
  AppState.instance.downloads
      .init(AppState.instance.settings.value)
      // Danach einmal abgleichen, was automatisch heruntergeladen sein soll.
      .then((_) => AutoSync.run())
      .catchError((_) {});
  _watchNetwork();
}

/// Holt den Schlüssel, mit dem Rust die Anmeldung verschlüsselt speichert.
///
/// Der Schlüssel liegt im Android-Keystore (`SecretStore.kt`) und damit
/// außerhalb des App-Ordners. Schlägt der Abruf fehl (andere Plattform,
/// alter Android-Teil), geben wir einen leeren Schlüssel zurück: Rust
/// speichert dann unverschlüsselt, die App bleibt aber benutzbar.
Future<Uint8List> _storageKey() async {
  try {
    const channel = MethodChannel('musicplayer/system');
    final key = await channel.invokeMethod<Uint8List>('storageKey');
    return key ?? Uint8List(0);
  } catch (_) {
    return Uint8List(0);
  }
}

/// Wird statt der App gezeigt, wenn die Einrichtung fehlschlägt.
///
/// Zeigt die echte Fehlermeldung an (zum Markieren und Kopieren), damit man
/// sie weitergeben kann, statt nur "geht nicht" sagen zu können.
class StartupErrorApp extends StatelessWidget {
  const StartupErrorApp({super.key, required this.error, required this.stack});

  final Object error;
  final StackTrace stack;

  @override
  Widget build(BuildContext context) {
    return MaterialApp(
      debugShowCheckedModeBanner: false,
      theme: AppTheme.dark(),
      home: Scaffold(
        body: SafeArea(
          child: Padding(
            padding: const EdgeInsets.all(24),
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                const Icon(
                  Icons.error_outline,
                  size: 48,
                  color: Colors.redAccent,
                ),
                const SizedBox(height: 16),
                const Text(
                  'Nocturne konnte nicht starten\nNocturne could not start',
                  style: TextStyle(fontSize: 20, fontWeight: FontWeight.bold),
                ),
                const SizedBox(height: 16),
                Expanded(
                  child: SingleChildScrollView(
                    child: SelectableText('$error\n\n$stack'),
                  ),
                ),
              ],
            ),
          ),
        ),
      ),
    );
  }
}

/// Meldet Rust, ob gerade WLAN oder mobile Daten aktiv sind. Davon hängt
/// ab, in welcher Qualität neue Titel angefordert werden.
///
/// AKKU: `onConnectivityChanged` ist ein System-Ereignis. Es wird nichts
/// regelmäßig abgefragt; Code läuft nur, wenn sich das Netz wirklich ändert.
void _watchNetwork() {
  void apply(List<ConnectivityResult> results) {
    final kind =
        results.contains(ConnectivityResult.wifi) ||
            results.contains(ConnectivityResult.ethernet)
        ? rust.NetworkKind.wifi
        : results.contains(ConnectivityResult.mobile)
        ? rust.NetworkKind.mobile
        : rust.NetworkKind.none;
    rust.setNetworkKind(kind: kind);
    final app = AppState.instance;
    final wasOffline = app.network.value == rust.NetworkKind.none;
    final before = app.network.value;
    app.network.value = kind;
    app.recomputeOffline();
    // Netz ist zurück: offline Gehörtes jetzt an den Server nachreichen.
    // Passiert nur bei diesem Übergang, nicht in einer Dauerschleife.
    // Neu im WLAN: guter Moment, automatisch Vorgemerktes nachzuladen.
    if (kind == rust.NetworkKind.wifi && before != rust.NetworkKind.wifi) {
      AutoSync.run();
    }
    if (wasOffline && kind != rust.NetworkKind.none) {
      stats.flushScrobbles().catchError((_) {});
    }
  }

  final connectivity = Connectivity();
  connectivity.checkConnectivity().then(apply).catchError((_) {});
  connectivity.onConnectivityChanged.listen(apply);
}

class MusicApp extends StatelessWidget {
  const MusicApp({super.key});

  @override
  Widget build(BuildContext context) {
    final app = AppState.instance;
    // Hört auf die Einstellungen: Ändert der Nutzer das Design, baut sich
    // die App sofort mit dem neuen Theme neu.
    return ValueListenableBuilder<AppSettings>(
      valueListenable: app.settings,
      builder: (context, settings, _) {
        final theme = AppTheme.resolve(settings);
        return MaterialApp(
          title: 'Nocturne',
          debugShowCheckedModeBanner: false,
          themeMode: theme.mode,
          theme: theme.light,
          darkTheme: theme.dark,
          // Deutsch und Englisch. Ist das Handy auf eine andere Sprache
          // gestellt, nimmt Flutter die erste der Liste (Englisch).
          supportedLocales: const [Locale('en'), Locale('de')],
          localizationsDelegates: const [
            GlobalMaterialLocalizations.delegate,
            GlobalWidgetsLocalizations.delegate,
            GlobalCupertinoLocalizations.delegate,
          ],
          // Angemeldet -> App, sonst Anmeldung. Wechselt automatisch beim
          // An- und Abmelden.
          home: ValueListenableBuilder<bool>(
            valueListenable: app.loggedIn,
            builder: (_, loggedIn, __) =>
                loggedIn ? const AppShell() : const LoginScreen(),
          ),
        );
      },
    );
  }
}
