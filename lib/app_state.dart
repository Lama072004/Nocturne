import 'package:flutter/foundation.dart';

import 'downloads/download_controller.dart';
import 'library/favorites.dart';
import 'player/player_controller.dart';
import 'src/rust/api/library.dart' as rust;
import 'src/rust/api/settings.dart';

/// Globaler Zustand der App.
///
/// WARUM globale Objekte statt Provider/Riverpod?
/// Es gibt nur eine Handvoll Dinge, die überall gebraucht werden
/// (Einstellungen, Anmeldung, Player, Favoriten, Downloads, Netz). Dafür
/// reichen [ValueNotifier] und [ChangeNotifier] aus Flutter selbst. Weniger
/// Pakete = kleinere App, schnellerer Start und weniger, das bei Updates
/// kaputtgehen kann. Die eigentliche Wahrheit liegt ohnehin im Rust-Kern;
/// Dart hält nur eine Kopie für die Anzeige.
class AppState {
  AppState._();
  static final AppState instance = AppState._();

  /// Kopie der Einstellungen aus Rust. Widgets hören darauf und bauen sich
  /// bei Änderungen neu (z. B. Theme-Wechsel).
  late final ValueNotifier<AppSettings> settings = ValueNotifier(getSettings());

  late final ValueNotifier<bool> loggedIn = ValueNotifier(rust.isLoggedIn());

  final PlayerController player = PlayerController();
  final FavoriteStore favorites = FavoriteStore();
  final DownloadController downloads = DownloadController();

  /// Aktuelle Netzart (wird in `main.dart` vom System-Ereignis gesetzt).
  final ValueNotifier<rust.NetworkKind> network = ValueNotifier(
    rust.NetworkKind.wifi,
  );

  /// `true` = die App zeigt nur heruntergeladene Inhalte.
  ///
  /// Das ist KEIN eigener Schalter, sondern ergibt sich aus Netz +
  /// Einstellungen. So kann der Zustand nie "hängen bleiben".
  final ValueNotifier<bool> offline = ValueNotifier(false);

  /// Berechnet [offline] neu. Aufrufen, wenn sich Netz oder Einstellungen ändern.
  void recomputeOffline() {
    final s = settings.value;
    offline.value = switch (network.value) {
      // Kein Netz: offline, außer der Nutzer hat die Automatik abgeschaltet.
      rust.NetworkKind.none => s.autoOffline,
      // Mobile Daten: nur offline, wenn der Nutzer das so eingestellt hat.
      rust.NetworkKind.mobile => s.offlineOnMobile,
      rust.NetworkKind.wifi => false,
    };
  }

  /// Speichert in Rust (Datei) und aktualisiert danach die Kopie.
  /// Reihenfolge bewusst so: Schlägt das Speichern fehl, zeigt die UI nicht
  /// fälschlich den neuen Wert.
  Future<void> updateSettings(AppSettings s) async {
    await saveSettings(settings: s);
    settings.value = s;
    recomputeOffline();
    // Android braucht die Download-Einstellungen ebenfalls (WLAN-Zwang usw.).
    await downloads.configure(s);
  }
}

/// Die von flutter_rust_bridge erzeugten Klassen haben kein `copyWith`.
/// Diese Erweiterung ergänzt es, damit man einzelne Felder ändern kann,
/// ohne jedes Mal alle anderen von Hand zu kopieren.
extension AppSettingsCopy on AppSettings {
  AppSettings copyWith({
    ThemeChoice? theme,
    DarkVariant? systemDarkVariant,
    int? accentColor,
    StreamQuality? qualityWifi,
    StreamQuality? qualityMobile,
    StreamQuality? qualityDownload,
    bool? offlineOnMobile,
    bool? downloadWifiOnly,
    bool? autoOffline,
    int? downloadLimitMb,
    int? streamCacheMb,
    bool? downloadsOnSdCard,
    bool? autoSyncFavorites,
    bool? autoSyncPlaylists,
    bool? autoSyncSmart,
    SwipeConfig? swipes,
  }) => AppSettings(
    theme: theme ?? this.theme,
    systemDarkVariant: systemDarkVariant ?? this.systemDarkVariant,
    accentColor: accentColor ?? this.accentColor,
    qualityWifi: qualityWifi ?? this.qualityWifi,
    qualityMobile: qualityMobile ?? this.qualityMobile,
    qualityDownload: qualityDownload ?? this.qualityDownload,
    offlineOnMobile: offlineOnMobile ?? this.offlineOnMobile,
    downloadWifiOnly: downloadWifiOnly ?? this.downloadWifiOnly,
    autoOffline: autoOffline ?? this.autoOffline,
    downloadLimitMb: downloadLimitMb ?? this.downloadLimitMb,
    streamCacheMb: streamCacheMb ?? this.streamCacheMb,
    downloadsOnSdCard: downloadsOnSdCard ?? this.downloadsOnSdCard,
    autoSyncFavorites: autoSyncFavorites ?? this.autoSyncFavorites,
    autoSyncPlaylists: autoSyncPlaylists ?? this.autoSyncPlaylists,
    autoSyncSmart: autoSyncSmart ?? this.autoSyncSmart,
    swipes: swipes ?? this.swipes,
  );
}

extension SwipeConfigCopy on SwipeConfig {
  SwipeConfig copyWith({
    SwipeSet? trackRows,
    SwipeSet? collectionRows,
    SwipeSet? queueRows,
    PlayerSwipes? miniPlayer,
  }) => SwipeConfig(
    trackRows: trackRows ?? this.trackRows,
    collectionRows: collectionRows ?? this.collectionRows,
    queueRows: queueRows ?? this.queueRows,
    miniPlayer: miniPlayer ?? this.miniPlayer,
  );
}

extension SwipeSetCopy on SwipeSet {
  SwipeSet copyWith({
    SwipeAction? rightShort,
    SwipeAction? rightLong,
    SwipeAction? leftShort,
    SwipeAction? leftLong,
  }) => SwipeSet(
    rightShort: rightShort ?? this.rightShort,
    rightLong: rightLong ?? this.rightLong,
    leftShort: leftShort ?? this.leftShort,
    leftLong: leftLong ?? this.leftLong,
  );
}
