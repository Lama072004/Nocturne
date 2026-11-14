import 'package:flutter/material.dart';

import '../../app_state.dart';
import '../../downloads/auto_sync.dart';
import '../../l10n.dart';
import '../../src/rust/api/library.dart' as rust;
import '../../src/rust/api/settings.dart';
import '../../theme/app_theme.dart';
import '../actions.dart';
import 'equalizer_screen.dart';

/// Einstellungen: Design, Klangqualität, Offline und Wischgesten.
///
/// Jede Änderung wird sofort gespeichert (kein "Speichern"-Knopf). Die
/// Seite hört auf `AppState.settings` und zeigt deshalb immer den Stand, der
/// wirklich in Rust gespeichert ist.
class SettingsScreen extends StatelessWidget {
  const SettingsScreen({super.key});

  /// Bitraten, die zur Auswahl stehen. Feste Stufen statt freier Eingabe,
  /// weil Server-Transcoder nur mit üblichen Werten zuverlässig arbeiten.
  static const _bitrates = [64, 96, 128, 160, 192, 256, 320];

  /// Auswahl für das Download-Limit in MB (0 = unbegrenzt).
  static const _limits = [0, 1024, 2048, 5120, 10240, 20480, 51200];

  /// Auswahl für den Streaming-Cache in MB (0 = aus).
  static const _caches = [0, 256, 512, 1024, 2048, 5120];

  static String _size(int mb) => mb >= 1024 ? '${mb ~/ 1024} GB' : '$mb MB';

  @override
  Widget build(BuildContext context) {
    final l = L.of(context);
    final app = AppState.instance;

    return Scaffold(
      appBar: AppBar(title: Text(l.settings)),
      body: ValueListenableBuilder<AppSettings>(
        valueListenable: app.settings,
        builder: (context, s, _) {
          // Nach dem Speichern gleich abgleichen: Wer einen Auto-Sync-Schalter
          // umlegt, erwartet, dass sofort etwas passiert.
          void save(AppSettings next) =>
              app.updateSettings(next).then((_) => AutoSync.run());

          return ListView(
            children: [
              _Section(l.appearance),
              _Choice<ThemeChoice>(
                label: l.theme,
                value: s.theme,
                values: ThemeChoice.values,
                name: l.themeName,
                onChanged: (v) => save(s.copyWith(theme: v)),
              ),
              // Nur sinnvoll (und nur sichtbar), wenn "System" gewählt ist.
              if (s.theme == ThemeChoice.system)
                _Choice<DarkVariant>(
                  label: l.systemDark,
                  value: s.systemDarkVariant,
                  values: DarkVariant.values,
                  name: l.darkVariantName,
                  onChanged: (v) => save(s.copyWith(systemDarkVariant: v)),
                ),

              // Akzentfarbe: Tippen auf einen Kreis übernimmt sie sofort für
              // die ganze App.
              Padding(
                padding: const EdgeInsets.fromLTRB(16, 12, 16, 4),
                child: Text(l.accentColor),
              ),
              Padding(
                padding: const EdgeInsets.fromLTRB(16, 4, 16, 12),
                child: Wrap(
                  spacing: 12,
                  runSpacing: 12,
                  children: [
                    for (final c in kAccentChoices)
                      GestureDetector(
                        onTap: () =>
                            save(s.copyWith(accentColor: c.toARGB32())),
                        child: Container(
                          width: 40,
                          height: 40,
                          decoration: BoxDecoration(
                            color: c,
                            shape: BoxShape.circle,
                          ),
                          // Haken auf der gerade gewählten Farbe.
                          child: s.accentColor == c.toARGB32()
                              ? const Icon(Icons.check, color: Colors.black87)
                              : null,
                        ),
                      ),
                  ],
                ),
              ),

              // Klang hat eine eigene Seite: Equalizer, Kompressor und
              // ReplayGain brauchen mehr Platz als eine Zeile.
              ListTile(
                leading: const Icon(Icons.graphic_eq),
                title: Text(l.sound),
                subtitle: Text(
                  '${l.equalizer}, ${l.compressor}, ${l.replayGain}',
                ),
                trailing: const Icon(Icons.chevron_right),
                onTap: () => Navigator.of(context).push(
                  MaterialPageRoute(builder: (_) => const EqualizerScreen()),
                ),
              ),

              _Section(l.t('Wiedergabe', 'Playback')),
              const _CrossfadeTile(),

              _Section(l.quality),
              _QualityTile(
                label: l.qualityWifi,
                value: s.qualityWifi,
                onChanged: (q) => save(s.copyWith(qualityWifi: q)),
              ),
              _QualityTile(
                label: l.qualityMobile,
                value: s.qualityMobile,
                onChanged: (q) => save(s.copyWith(qualityMobile: q)),
              ),
              _QualityTile(
                label: l.qualityDownload,
                value: s.qualityDownload,
                onChanged: (q) => save(s.copyWith(qualityDownload: q)),
              ),

              _Section(l.offline),
              SwitchListTile(
                title: Text(l.offlineOnMobile),
                value: s.offlineOnMobile,
                onChanged: (v) => save(s.copyWith(offlineOnMobile: v)),
              ),
              SwitchListTile(
                title: Text(l.autoOffline),
                value: s.autoOffline,
                onChanged: (v) => save(s.copyWith(autoOffline: v)),
              ),
              SwitchListTile(
                title: Text(l.downloadWifiOnly),
                value: s.downloadWifiOnly,
                onChanged: (v) => save(s.copyWith(downloadWifiOnly: v)),
              ),
              _Choice<int>(
                label: l.downloadLimit,
                value: _limits.contains(s.downloadLimitMb)
                    ? s.downloadLimitMb
                    : 0,
                values: _limits,
                name: (mb) => mb == 0 ? l.unlimited : _size(mb),
                onChanged: (v) => save(s.copyWith(downloadLimitMb: v)),
              ),
              _Choice<int>(
                label: l.streamCache,
                value: _caches.contains(s.streamCacheMb)
                    ? s.streamCacheMb
                    : 512,
                values: _caches,
                name: (mb) => mb == 0 ? l.off : _size(mb),
                onChanged: (v) => save(s.copyWith(streamCacheMb: v)),
              ),
              SwitchListTile(
                title: Text(l.sdCard),
                // Der Speicherort wird beim Start festgelegt; das soll der
                // Nutzer wissen, bevor er sich wundert.
                subtitle: Text(l.sdCardHint),
                value: s.downloadsOnSdCard,
                onChanged: (v) => _changeSdCard(context, s, v, save),
              ),

              _Section(l.autoSync),
              // Jede Art einzeln schaltbar. Geladen wird bei App-Start und
              // beim Wechsel ins WLAN, nicht dauernd im Hintergrund.
              SwitchListTile(
                title: Text(l.autoSyncFavorites),
                value: s.autoSyncFavorites,
                onChanged: (v) => save(s.copyWith(autoSyncFavorites: v)),
              ),
              SwitchListTile(
                title: Text(l.autoSyncPlaylists),
                value: s.autoSyncPlaylists,
                onChanged: (v) => save(s.copyWith(autoSyncPlaylists: v)),
              ),
              SwitchListTile(
                title: Text(l.autoSyncSmart),
                value: s.autoSyncSmart,
                onChanged: (v) => save(s.copyWith(autoSyncSmart: v)),
              ),

              _Section(l.swipes),
              _SwipeSetTile(
                title: l.swipeTrackRows,
                value: s.swipes.trackRows,
                onChanged: (v) =>
                    save(s.copyWith(swipes: s.swipes.copyWith(trackRows: v))),
              ),
              _SwipeSetTile(
                title: l.swipeCollectionRows,
                value: s.swipes.collectionRows,
                onChanged: (v) => save(
                  s.copyWith(swipes: s.swipes.copyWith(collectionRows: v)),
                ),
              ),
              _SwipeSetTile(
                title: l.swipeQueueRows,
                value: s.swipes.queueRows,
                onChanged: (v) =>
                    save(s.copyWith(swipes: s.swipes.copyWith(queueRows: v))),
              ),
              ExpansionTile(
                title: Text(l.swipeMiniPlayer),
                children: [
                  for (final (label, value, apply)
                      in <
                        (
                          String,
                          PlayerSwipeAction,
                          PlayerSwipes Function(PlayerSwipeAction),
                        )
                      >[
                        (
                          l.swipeLeft,
                          s.swipes.miniPlayer.left,
                          (a) => PlayerSwipes(
                            left: a,
                            right: s.swipes.miniPlayer.right,
                            up: s.swipes.miniPlayer.up,
                          ),
                        ),
                        (
                          l.swipeRight,
                          s.swipes.miniPlayer.right,
                          (a) => PlayerSwipes(
                            left: s.swipes.miniPlayer.left,
                            right: a,
                            up: s.swipes.miniPlayer.up,
                          ),
                        ),
                        (
                          l.swipeUp,
                          s.swipes.miniPlayer.up,
                          (a) => PlayerSwipes(
                            left: s.swipes.miniPlayer.left,
                            right: s.swipes.miniPlayer.right,
                            up: a,
                          ),
                        ),
                      ])
                    _Choice<PlayerSwipeAction>(
                      label: label,
                      value: value,
                      values: PlayerSwipeAction.values,
                      name: l.playerSwipeName,
                      onChanged: (a) => save(
                        s.copyWith(
                          swipes: s.swipes.copyWith(miniPlayer: apply(a)),
                        ),
                      ),
                    ),
                ],
              ),

              const Divider(height: 32),
              ListTile(
                leading: const Icon(Icons.logout),
                title: Text(l.logout),
                onTap: () async {
                  final nav = Navigator.of(context, rootNavigator: true);
                  await rust.logout();
                  app.loggedIn.value = false;
                  // Zurück bis zur Wurzel, dort erscheint dann die Anmeldung.
                  nav.popUntil((r) => r.isFirst);
                },
              ),
              const SizedBox(height: 24),
            ],
          );
        },
      ),
    );
  }
}

/// Wechselt den Speicherort der Downloads (intern <-> SD-Karte).
///
/// WARUM nicht einfach den Schalter umlegen? Die vorhandenen Downloads
/// liegen am ALTEN Ort. Nach dem Wechsel fände die App sie nicht mehr, sie
/// belegten aber weiter Speicher, und in der Liste stünden sie als
/// "heruntergeladen", obwohl sie nicht abspielbar wären. Deshalb werden sie
/// (nach Rückfrage) entfernt; der Nutzer lädt am neuen Ort neu.
Future<void> _changeSdCard(
  BuildContext context,
  AppSettings s,
  bool toSdCard,
  void Function(AppSettings) save,
) async {
  final l = L.of(context);
  final downloads = AppState.instance.downloads;
  if (toSdCard && !await downloads.sdCardAvailable()) {
    if (context.mounted) {
      showToast(context, l.t('Keine SD-Karte gefunden', 'No SD card found'));
    }
    return;
  }
  final count = downloads.tracks.length;
  if (count > 0) {
    if (!context.mounted) return;
    final ok = await showDialog<bool>(
      context: context,
      builder: (context) => AlertDialog(
        title: Text(l.t('Speicherort wechseln?', 'Change storage location?')),
        content: Text(
          l.t(
            'Die $count vorhandenen Downloads liegen am bisherigen Ort und '
                'werden entfernt. Du kannst sie danach neu herunterladen.',
            'The $count existing downloads are stored in the current '
                'location and will be removed. You can download them again '
                'afterwards.',
          ),
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.of(context).pop(false),
            child: Text(l.cancel),
          ),
          FilledButton(
            onPressed: () => Navigator.of(context).pop(true),
            child: Text(l.t('Wechseln', 'Change')),
          ),
        ],
      ),
    );
    if (ok != true) return;
    await downloads.removeAll();
  }
  save(s.copyWith(downloadsOnSdCard: toSdCard));
  if (context.mounted) {
    showToast(
      context,
      l.t(
        'Gilt nach einem Neustart der App',
        'Takes effect after restarting the app',
      ),
    );
  }
}

/// Regler für das Überblenden zwischen Titeln (0 = aus, bis 12 Sekunden).
///
/// Eigener kleiner Zustand: Während des Ziehens soll nur die Anzeige
/// mitlaufen. Gespeichert und an den Dienst gegeben wird erst beim
/// Loslassen – sonst würde bei jedem Zwischenschritt der Audio-Pfad
/// umgestellt.
class _CrossfadeTile extends StatefulWidget {
  const _CrossfadeTile();

  @override
  State<_CrossfadeTile> createState() => _CrossfadeTileState();
}

class _CrossfadeTileState extends State<_CrossfadeTile> {
  late double _value = AppState.instance.player.crossfadeSeconds.toDouble();

  @override
  Widget build(BuildContext context) {
    final l = L.of(context);
    final theme = Theme.of(context);
    final secs = _value.round();
    return Padding(
      padding: const EdgeInsets.fromLTRB(16, 4, 16, 4),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Row(
            children: [
              Expanded(
                child: Text(
                  l.t('Überblenden (Crossfade)', 'Crossfade'),
                  style: theme.textTheme.bodyLarge,
                ),
              ),
              Text(
                secs == 0 ? l.off : '$secs s',
                style: theme.textTheme.bodyLarge?.copyWith(
                  fontWeight: FontWeight.bold,
                ),
              ),
            ],
          ),
          Slider(
            min: 0,
            max: 12,
            divisions: 12,
            value: _value,
            label: secs == 0 ? l.off : '$secs s',
            onChanged: (v) => setState(() {
              _value = v;
            }),
            onChangeEnd: (v) =>
                AppState.instance.player.setCrossfade(v.round()),
          ),
          // Ehrlich sagen, was es kostet (wie beim Equalizer).
          Text(
            l.t(
              'Das Ende eines Titels geht in den Anfang des nächsten über. '
                  'Braucht etwas mehr Akku.',
              'The end of a track blends into the start of the next. Uses a '
                  'bit more battery.',
            ),
            style: theme.textTheme.bodySmall?.copyWith(color: theme.hintColor),
          ),
        ],
      ),
    );
  }
}

class _Section extends StatelessWidget {
  const _Section(this.title);
  final String title;

  @override
  Widget build(BuildContext context) => Padding(
    padding: const EdgeInsets.fromLTRB(16, 24, 16, 8),
    child: Text(
      title,
      style: Theme.of(context).textTheme.titleSmall?.copyWith(
        color: Theme.of(context).colorScheme.primary,
        fontWeight: FontWeight.bold,
      ),
    ),
  );
}

/// Eine Zeile mit Auswahlliste. Generisch, damit dieselbe Zeile für Design,
/// Formate und alle Gesten-Aktionen verwendet werden kann.
class _Choice<T> extends StatelessWidget {
  const _Choice({
    required this.label,
    required this.value,
    required this.values,
    required this.name,
    required this.onChanged,
  });

  final String label;
  final T value;
  final List<T> values;
  final String Function(T) name;
  final void Function(T) onChanged;

  @override
  Widget build(BuildContext context) => ListTile(
    title: Text(label),
    trailing: DropdownButton<T>(
      value: value,
      underline: const SizedBox.shrink(),
      items: [
        for (final v in values)
          DropdownMenuItem(value: v, child: Text(name(v))),
      ],
      onChanged: (v) {
        if (v != null) onChanged(v);
      },
    ),
  );
}

/// Format + Bitrate für eine der drei Qualitätsstufen.
class _QualityTile extends StatelessWidget {
  const _QualityTile({
    required this.label,
    required this.value,
    required this.onChanged,
  });

  final String label;
  final StreamQuality value;
  final void Function(StreamQuality) onChanged;

  @override
  Widget build(BuildContext context) {
    final l = L.of(context);
    final isOriginal = value.format == AudioFormat.original;
    return ListTile(
      title: Text(label),
      trailing: Row(
        mainAxisSize: MainAxisSize.min,
        children: [
          DropdownButton<AudioFormat>(
            value: value.format,
            underline: const SizedBox.shrink(),
            items: [
              for (final f in AudioFormat.values)
                DropdownMenuItem(value: f, child: Text(l.formatName(f))),
            ],
            onChanged: (f) {
              if (f == null) return;
              // Beim Wechsel weg von "Original" eine sinnvolle Bitrate setzen.
              final bitrate = value.bitrateKbps == 0 ? 160 : value.bitrateKbps;
              onChanged(
                StreamQuality(
                  format: f,
                  bitrateKbps: f == AudioFormat.original ? 0 : bitrate,
                ),
              );
            },
          ),
          // Bei "Original" gibt es keine Bitrate zu wählen.
          if (!isOriginal) ...[
            const SizedBox(width: 8),
            DropdownButton<int>(
              value: SettingsScreen._bitrates.contains(value.bitrateKbps)
                  ? value.bitrateKbps
                  : 160,
              underline: const SizedBox.shrink(),
              items: [
                for (final b in SettingsScreen._bitrates)
                  DropdownMenuItem(value: b, child: Text('$b kbit/s')),
              ],
              onChanged: (b) {
                if (b != null) {
                  onChanged(
                    StreamQuality(format: value.format, bitrateKbps: b),
                  );
                }
              },
            ),
          ],
        ],
      ),
    );
  }
}

/// Aufklappbare Gruppe mit den vier Gesten einer Zeilenart.
class _SwipeSetTile extends StatelessWidget {
  const _SwipeSetTile({
    required this.title,
    required this.value,
    required this.onChanged,
  });

  final String title;
  final SwipeSet value;
  final void Function(SwipeSet) onChanged;

  @override
  Widget build(BuildContext context) {
    final l = L.of(context);
    Widget row(
      String label,
      SwipeAction current,
      SwipeSet Function(SwipeAction) apply,
    ) => _Choice<SwipeAction>(
      label: label,
      value: current,
      values: SwipeAction.values,
      name: l.swipeActionName,
      onChanged: (a) => onChanged(apply(a)),
    );
    return ExpansionTile(
      title: Text(title),
      children: [
        row(
          l.rightShort,
          value.rightShort,
          (a) => value.copyWith(rightShort: a),
        ),
        row(l.rightLong, value.rightLong, (a) => value.copyWith(rightLong: a)),
        row(l.leftShort, value.leftShort, (a) => value.copyWith(leftShort: a)),
        row(l.leftLong, value.leftLong, (a) => value.copyWith(leftLong: a)),
      ],
    );
  }
}
