import 'dart:async';
import 'dart:math' as math;
import 'dart:typed_data';

import 'package:flutter/material.dart';

import '../../app_state.dart';
import '../../l10n.dart';
import '../../src/rust/api/dsp.dart';
import '../actions.dart';

/// Klang-Einstellungen: Equalizer (parametrisch oder 10-Band), AutoEQ,
/// Profil je Ausgabegerät, Kompressor und ReplayGain.
///
/// Gerechnet wird in Rust. Diese Seite hält nur eine Kopie der
/// Einstellungen, zeigt sie an und schickt jede Änderung zurück.
class EqualizerScreen extends StatefulWidget {
  const EqualizerScreen({super.key});

  @override
  State<EqualizerScreen> createState() => _EqualizerScreenState();
}

class _EqualizerScreenState extends State<EqualizerScreen>
    with WidgetsBindingObserver {
  late DspConfig _config = getDspConfig();
  OutputDevice? _device = currentOutputDevice();
  Timer? _saveTimer;

  @override
  void initState() {
    super.initState();
    WidgetsBinding.instance.addObserver(this);
  }

  @override
  void dispose() {
    WidgetsBinding.instance.removeObserver(this);
    // Noch nicht gespeicherte Änderung nicht verlieren.
    if (_saveTimer?.isActive ?? false) {
      _saveTimer!.cancel();
      _saveNow();
    }
    super.dispose();
  }

  /// Kommt die App wieder nach vorne, kann inzwischen ein anderer
  /// Kopfhörer verbunden sein -> Gerät neu abfragen.
  @override
  void didChangeAppLifecycleState(AppLifecycleState state) {
    if (state == AppLifecycleState.resumed) {
      setState(() => _device = currentOutputDevice());
    }
  }

  // -------------------------------------------------------------------------
  // Welches Profil wird gerade bearbeitet?
  // -------------------------------------------------------------------------

  /// Index des Profils für das aktuelle Gerät, oder -1, wenn es keins gibt.
  int get _deviceProfileIndex {
    final key = _device?.key;
    if (key == null) return -1;
    return _config.deviceProfiles.indexWhere((p) => p.deviceKey == key);
  }

  /// Das Profil, das gerade gilt UND bearbeitet wird: das des aktuellen
  /// Geräts, falls vorhanden, sonst das Standard-Profil. Dieselbe Regel
  /// wendet Rust beim Abspielen an.
  EqProfile get _eq {
    final i = _deviceProfileIndex;
    return i >= 0 ? _config.deviceProfiles[i].profile : _config.eq;
  }

  void _setEq(EqProfile eq) {
    final i = _deviceProfileIndex;
    if (i < 0) {
      _update(_copyConfig(eq: eq));
    } else {
      final profiles = [..._config.deviceProfiles];
      profiles[i] = DeviceProfile(
        deviceKey: profiles[i].deviceKey,
        deviceName: profiles[i].deviceName,
        profile: eq,
      );
      _update(_copyConfig(deviceProfiles: profiles));
    }
  }

  /// Legt für das aktuelle Gerät ein eigenes Profil an (als Kopie des
  /// gerade gültigen) oder entfernt es wieder.
  void _toggleDeviceProfile(bool on) {
    final device = _device;
    if (device == null) return;
    final profiles = [..._config.deviceProfiles];
    if (on) {
      profiles.add(
        DeviceProfile(
          deviceKey: device.key,
          deviceName: device.name,
          profile: _eq,
        ),
      );
    } else {
      profiles.removeWhere((p) => p.deviceKey == device.key);
    }
    _update(_copyConfig(deviceProfiles: profiles));
  }

  // -------------------------------------------------------------------------
  // Speichern
  // -------------------------------------------------------------------------

  DspConfig _copyConfig({
    EqProfile? eq,
    List<DeviceProfile>? deviceProfiles,
    CompressorConfig? compressor,
    ReplayGainConfig? replayGain,
  }) => DspConfig(
    eq: eq ?? _config.eq,
    deviceProfiles: deviceProfiles ?? _config.deviceProfiles,
    compressor: compressor ?? _config.compressor,
    replayGain: replayGain ?? _config.replayGain,
  );

  /// Übernimmt eine Änderung: Anzeige sofort, Speichern leicht verzögert.
  ///
  /// Beim Ziehen eines Reglers kommen dutzende Werte pro Sekunde. Würde
  /// jeder einzeln gespeichert, schriebe die App ständig auf den Speicher.
  /// So geht alle 60 ms höchstens ein Stand an Rust – für das Ohr trotzdem
  /// ohne spürbare Verzögerung.
  void _update(DspConfig config) {
    setState(() => _config = config);
    _saveTimer?.cancel();
    _saveTimer = Timer(const Duration(milliseconds: 60), _saveNow);
  }

  Future<void> _saveNow() async {
    try {
      await saveDspConfig(config: _config);
      // Der Wiedergabe-Dienst muss wissen, ob jetzt Effekte laufen
      // (Hardware-Offload an/aus).
      await AppState.instance.player.notifyDspChanged();
    } catch (e) {
      if (mounted) showToast(context, '$e');
    }
  }

  EqProfile _copyEq({
    bool? enabled,
    EqMode? mode,
    double? preampDb,
    List<EqBand>? bands,
    List<double>? graphicGainsDb,
  }) {
    final eq = _eq;
    return EqProfile(
      enabled: enabled ?? eq.enabled,
      mode: mode ?? eq.mode,
      preampDb: preampDb ?? eq.preampDb,
      bands: bands ?? eq.bands,
      // Die Brücke zu Rust erwartet hier ein Float32List (kompaktes
      // Zahlen-Array), keine normale Dart-Liste.
      graphicGainsDb: graphicGainsDb == null
          ? eq.graphicGainsDb
          : Float32List.fromList(graphicGainsDb),
    );
  }

  // -------------------------------------------------------------------------
  // AutoEQ
  // -------------------------------------------------------------------------

  Future<void> _openAutoEq() async {
    final profile = await showModalBottomSheet<EqProfile>(
      context: context,
      isScrollControlled: true,
      showDragHandle: true,
      builder: (_) => const _AutoEqSheet(),
    );
    if (profile != null) _setEq(profile);
  }

  @override
  Widget build(BuildContext context) {
    final l = L.of(context);
    final theme = Theme.of(context);
    final eq = _eq;
    final device = _device;
    final hasDeviceProfile = _deviceProfileIndex >= 0;

    // Zwei Reiter statt einer langen Seite. WARUM: Die zehn Regler des
    // Equalizers stehen senkrecht und füllen die ganze Breite. Auf einer
    // langen Scroll-Seite hat man beim Blättern zu Kompressor und ReplayGain
    // unweigerlich einen Regler erwischt und den Klang verstellt (im Test
    // passiert). Jetzt passt der Equalizer ohne Blättern auf den Schirm, und
    // der Rest liegt einen Tipp daneben.
    return DefaultTabController(
      length: 2,
      child: Scaffold(
        appBar: AppBar(
          title: Text(l.sound),
          bottom: TabBar(
            tabs: [
              Tab(text: l.equalizer),
              Tab(text: l.t('Lautstärke', 'Loudness')),
            ],
          ),
        ),
        body: TabBarView(
          // Kein seitliches Wischen zwischen den Reitern: Es würde mit den
          // waagrechten Reglern (Vorverstärkung, Kompressor) konkurrieren.
          physics: const NeverScrollableScrollPhysics(),
          children: [
            LayoutBuilder(
              builder: (context, box) => ListView(
                padding: const EdgeInsets.only(bottom: 8),
                children: [
                  // ---------------- Equalizer ----------------
                  SwitchListTile(
                    title: Text(
                      l.equalizer,
                      style: theme.textTheme.titleMedium?.copyWith(
                        fontWeight: FontWeight.bold,
                      ),
                    ),
                    // Ehrlicher Hinweis: Effekte kosten etwas Akku, weil dann das
                    // stromsparende Hardware-Offload nicht möglich ist.
                    subtitle: Text(l.effectsBatteryHint),
                    value: eq.enabled,
                    onChanged: (v) => _setEq(_copyEq(enabled: v)),
                  ),
                  if (device != null)
                    SwitchListTile(
                      dense: true,
                      title: Text(
                        l.deviceProfile(
                          l.outputDeviceName(device.key, device.name),
                        ),
                      ),
                      subtitle: Text(
                        hasDeviceProfile
                            ? l.deviceProfileOn
                            : l.deviceProfileOff,
                      ),
                      value: hasDeviceProfile,
                      onChanged: _toggleDeviceProfile,
                    ),
                  Padding(
                    padding: const EdgeInsets.fromLTRB(16, 8, 16, 0),
                    child: SizedBox(
                      height: 110,
                      // Kurve neu berechnen lassen (Rust), sobald sich das Profil ändert.
                      child: CustomPaint(
                        painter: _CurvePainter(
                          points: eqResponse(profile: eq, points: 96),
                          color: eq.enabled
                              ? theme.colorScheme.primary
                              : theme.hintColor,
                          gridColor: theme.dividerColor,
                        ),
                        child: const SizedBox.expand(),
                      ),
                    ),
                  ),
                  Padding(
                    padding: const EdgeInsets.fromLTRB(16, 12, 16, 4),
                    child: Row(
                      children: [
                        Expanded(
                          child: SegmentedButton<EqMode>(
                            segments: [
                              ButtonSegment(
                                value: EqMode.graphic,
                                label: Text(l.eqGraphic),
                              ),
                              ButtonSegment(
                                value: EqMode.parametric,
                                label: Text(l.eqParametric),
                              ),
                            ],
                            selected: {eq.mode},
                            onSelectionChanged: (s) =>
                                _setEq(_copyEq(mode: s.first)),
                          ),
                        ),
                        const SizedBox(width: 8),
                        IconButton(
                          tooltip: l.autoEq,
                          icon: const Icon(Icons.headphones),
                          onPressed: _openAutoEq,
                        ),
                        IconButton(
                          tooltip: l.reset,
                          icon: const Icon(Icons.restart_alt),
                          onPressed: () => _setEq(
                            _copyEq(
                              preampDb: 0,
                              graphicGainsDb: List.filled(10, 0.0),
                              bands: [
                                for (final b in eq.bands)
                                  EqBand(
                                    kind: b.kind,
                                    freqHz: b.freqHz,
                                    gainDb: 0,
                                    q: b.q,
                                  ),
                              ],
                            ),
                          ),
                        ),
                      ],
                    ),
                  ),
                  _SliderRow(
                    label: l.preamp,
                    value: eq.preampDb,
                    min: -24,
                    max: 12,
                    unit: 'dB',
                    onChanged: (v) => _setEq(_copyEq(preampDb: v)),
                  ),
                  if (eq.mode == EqMode.graphic)
                    _GraphicEq(
                      // Die Regler bekommen genau den Platz, der unter den
                      // festen Teilen (Schalter, Kurve, Modus, Vorverstärkung:
                      // zusammen rund 420 px) übrig bleibt. So passt der ganze
                      // Reiter ohne Blättern auf den Schirm, und niemand
                      // verstellt beim Scrollen versehentlich ein Band.
                      height: (box.maxHeight - 420).clamp(150.0, 320.0),
                      gains: eq.graphicGainsDb.toList(),
                      onChanged: (g) => _setEq(_copyEq(graphicGainsDb: g)),
                    )
                  else ...[
                    for (var i = 0; i < eq.bands.length; i++)
                      _BandEditor(
                        key: ValueKey(i),
                        index: i,
                        band: eq.bands[i],
                        onChanged: (b) =>
                            _setEq(_copyEq(bands: [...eq.bands]..[i] = b)),
                        onRemove: () =>
                            _setEq(_copyEq(bands: [...eq.bands]..removeAt(i))),
                      ),
                    // Rust verarbeitet höchstens 16 Bänder (MAX_BANDS).
                    if (eq.bands.length < 16)
                      Padding(
                        padding: const EdgeInsets.symmetric(horizontal: 16),
                        child: OutlinedButton.icon(
                          icon: const Icon(Icons.add),
                          label: Text(l.addBand),
                          onPressed: () => _setEq(
                            _copyEq(
                              bands: [
                                ...eq.bands,
                                const EqBand(
                                  kind: EqFilterKind.peaking,
                                  freqHz: 1000,
                                  gainDb: 0,
                                  q: 1,
                                ),
                              ],
                            ),
                          ),
                        ),
                      ),
                  ],
                ],
              ),
            ),
            ListView(
              padding: const EdgeInsets.only(top: 8, bottom: 32),
              children: [
                // ---------------- Kompressor ----------------
                SwitchListTile(
                  title: Text(
                    l.compressor,
                    style: theme.textTheme.titleMedium?.copyWith(
                      fontWeight: FontWeight.bold,
                    ),
                  ),
                  subtitle: Text(l.compressorHint),
                  value: _config.compressor.enabled,
                  onChanged: (v) => _setCompressor(enabled: v),
                ),
                if (_config.compressor.enabled) ...[
                  _SliderRow(
                    label: l.threshold,
                    value: _config.compressor.thresholdDb,
                    min: -60,
                    max: 0,
                    unit: 'dB',
                    onChanged: (v) => _setCompressor(thresholdDb: v),
                  ),
                  _SliderRow(
                    label: l.ratio,
                    value: _config.compressor.ratio,
                    min: 1,
                    max: 20,
                    unit: ':1',
                    onChanged: (v) => _setCompressor(ratio: v),
                  ),
                  _SliderRow(
                    label: l.attack,
                    value: _config.compressor.attackMs,
                    min: 1,
                    max: 200,
                    unit: 'ms',
                    onChanged: (v) => _setCompressor(attackMs: v),
                  ),
                  _SliderRow(
                    label: l.release,
                    value: _config.compressor.releaseMs,
                    min: 10,
                    max: 1000,
                    unit: 'ms',
                    onChanged: (v) => _setCompressor(releaseMs: v),
                  ),
                  _SliderRow(
                    label: l.makeup,
                    value: _config.compressor.makeupDb,
                    min: 0,
                    max: 24,
                    unit: 'dB',
                    onChanged: (v) => _setCompressor(makeupDb: v),
                  ),
                ],

                // ---------------- ReplayGain ----------------
                const Divider(height: 40),
                Padding(
                  padding: const EdgeInsets.fromLTRB(16, 0, 16, 4),
                  child: Text(
                    l.replayGain,
                    style: theme.textTheme.titleMedium?.copyWith(
                      fontWeight: FontWeight.bold,
                    ),
                  ),
                ),
                Padding(
                  padding: const EdgeInsets.fromLTRB(16, 0, 16, 8),
                  child: Text(
                    l.replayGainHint,
                    style: theme.textTheme.bodySmall?.copyWith(
                      color: theme.hintColor,
                    ),
                  ),
                ),
                Padding(
                  padding: const EdgeInsets.symmetric(horizontal: 16),
                  child: SegmentedButton<ReplayGainChoice>(
                    segments: [
                      ButtonSegment(
                        value: ReplayGainChoice.off,
                        label: Text(l.off),
                      ),
                      ButtonSegment(
                        value: ReplayGainChoice.track,
                        label: Text(l.rgTrack),
                      ),
                      ButtonSegment(
                        value: ReplayGainChoice.album,
                        label: Text(l.rgAlbum),
                      ),
                    ],
                    selected: {_config.replayGain.mode},
                    onSelectionChanged: (s) => _setReplayGain(mode: s.first),
                  ),
                ),
                if (_config.replayGain.mode != ReplayGainChoice.off) ...[
                  _SliderRow(
                    label: l.preamp,
                    value: _config.replayGain.preampDb,
                    min: -15,
                    max: 15,
                    unit: 'dB',
                    onChanged: (v) => _setReplayGain(preampDb: v),
                  ),
                  _SliderRow(
                    label: l.rgFallback,
                    value: _config.replayGain.fallbackDb,
                    min: -15,
                    max: 0,
                    unit: 'dB',
                    onChanged: (v) => _setReplayGain(fallbackDb: v),
                  ),
                  SwitchListTile(
                    title: Text(l.clippingProtection),
                    value: _config.replayGain.preventClipping,
                    onChanged: (v) => _setReplayGain(preventClipping: v),
                  ),
                ],
              ],
            ),
          ],
        ),
      ),
    );
  }

  void _setCompressor({
    bool? enabled,
    double? thresholdDb,
    double? ratio,
    double? attackMs,
    double? releaseMs,
    double? makeupDb,
  }) {
    final c = _config.compressor;
    _update(
      _copyConfig(
        compressor: CompressorConfig(
          enabled: enabled ?? c.enabled,
          thresholdDb: thresholdDb ?? c.thresholdDb,
          ratio: ratio ?? c.ratio,
          attackMs: attackMs ?? c.attackMs,
          releaseMs: releaseMs ?? c.releaseMs,
          makeupDb: makeupDb ?? c.makeupDb,
        ),
      ),
    );
  }

  void _setReplayGain({
    ReplayGainChoice? mode,
    double? preampDb,
    double? fallbackDb,
    bool? preventClipping,
  }) {
    final r = _config.replayGain;
    _update(
      _copyConfig(
        replayGain: ReplayGainConfig(
          mode: mode ?? r.mode,
          preampDb: preampDb ?? r.preampDb,
          fallbackDb: fallbackDb ?? r.fallbackDb,
          preventClipping: preventClipping ?? r.preventClipping,
        ),
      ),
    );
  }
}

/// "62", "1k", "16k" für die Beschriftung.
String _freqLabel(double hz) => hz >= 1000
    ? '${(hz / 1000).toStringAsFixed(hz % 1000 == 0 ? 0 : 1)}k'
    : hz.round().toString();

/// Beschrifteter Schieberegler mit aktuellem Wert rechts.
class _SliderRow extends StatelessWidget {
  const _SliderRow({
    required this.label,
    required this.value,
    required this.min,
    required this.max,
    required this.unit,
    required this.onChanged,
  });

  final String label;
  final double value;
  final double min;
  final double max;
  final String unit;
  final ValueChanged<double> onChanged;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return Padding(
      padding: const EdgeInsets.symmetric(horizontal: 16),
      child: Row(
        children: [
          SizedBox(
            width: 96,
            child: Text(label, style: theme.textTheme.bodyMedium),
          ),
          Expanded(
            child: Slider(
              min: min,
              max: max,
              // Werte außerhalb des Bereichs (z. B. aus einem importierten
              // Profil) würden den Slider sonst abstürzen lassen.
              value: value.clamp(min, max),
              onChanged: onChanged,
            ),
          ),
          SizedBox(
            width: 64,
            child: Text(
              '${value.toStringAsFixed(1)} $unit',
              textAlign: TextAlign.end,
              style: theme.textTheme.bodySmall,
            ),
          ),
        ],
      ),
    );
  }
}

/// Der einfache Modus: zehn senkrechte Regler.
class _GraphicEq extends StatelessWidget {
  const _GraphicEq({
    required this.height,
    required this.gains,
    required this.onChanged,
  });

  /// Gesamthöhe samt Beschriftung oben (Wert) und unten (Frequenz).
  final double height;
  final List<double> gains;
  final ValueChanged<List<double>> onChanged;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final freqs = graphicFrequencies();
    return SizedBox(
      height: height,
      child: Row(
        children: [
          for (var i = 0; i < freqs.length; i++)
            Expanded(
              child: Column(
                children: [
                  Text(
                    (i < gains.length ? gains[i] : 0).toStringAsFixed(0),
                    style: theme.textTheme.labelSmall,
                  ),
                  Expanded(
                    // Ein normaler Slider, um 90° gedreht: spart ein
                    // eigenes Widget für senkrechte Regler.
                    child: RotatedBox(
                      quarterTurns: 3,
                      child: Slider(
                        min: -12,
                        max: 12,
                        value: (i < gains.length ? gains[i] : 0.0).clamp(
                          -12.0,
                          12.0,
                        ),
                        onChanged: (v) {
                          final next = List<double>.generate(
                            10,
                            (j) => j < gains.length ? gains[j] : 0.0,
                          );
                          next[i] = v;
                          onChanged(next);
                        },
                      ),
                    ),
                  ),
                  Text(_freqLabel(freqs[i]), style: theme.textTheme.labelSmall),
                ],
              ),
            ),
        ],
      ),
    );
  }
}

/// Ein Band des parametrischen EQ: Typ, Frequenz, Gain, Güte.
class _BandEditor extends StatelessWidget {
  const _BandEditor({
    super.key,
    required this.index,
    required this.band,
    required this.onChanged,
    required this.onRemove,
  });

  final int index;
  final EqBand band;
  final ValueChanged<EqBand> onChanged;
  final VoidCallback onRemove;

  EqBand _copy({
    EqFilterKind? kind,
    double? freqHz,
    double? gainDb,
    double? q,
  }) => EqBand(
    kind: kind ?? band.kind,
    freqHz: freqHz ?? band.freqHz,
    gainDb: gainDb ?? band.gainDb,
    q: q ?? band.q,
  );

  @override
  Widget build(BuildContext context) {
    final l = L.of(context);
    final theme = Theme.of(context);
    return Card(
      margin: const EdgeInsets.fromLTRB(16, 6, 16, 6),
      color: theme.colorScheme.surfaceContainer,
      elevation: 0,
      child: Padding(
        padding: const EdgeInsets.symmetric(vertical: 4),
        child: Column(
          children: [
            Padding(
              padding: const EdgeInsets.only(left: 16, right: 4),
              child: Row(
                children: [
                  Text('${index + 1}', style: theme.textTheme.titleSmall),
                  const SizedBox(width: 12),
                  DropdownButton<EqFilterKind>(
                    value: band.kind,
                    underline: const SizedBox.shrink(),
                    items: [
                      DropdownMenuItem(
                        value: EqFilterKind.peaking,
                        child: Text(l.filterPeaking),
                      ),
                      DropdownMenuItem(
                        value: EqFilterKind.lowShelf,
                        child: Text(l.filterLowShelf),
                      ),
                      DropdownMenuItem(
                        value: EqFilterKind.highShelf,
                        child: Text(l.filterHighShelf),
                      ),
                    ],
                    onChanged: (k) => onChanged(_copy(kind: k)),
                  ),
                  const Spacer(),
                  IconButton(
                    icon: const Icon(Icons.close),
                    onPressed: onRemove,
                  ),
                ],
              ),
            ),
            // Frequenz logarithmisch: Der Regler arbeitet im Exponenten
            // (20 Hz · 1000^t). So bekommt jede Oktave gleich viel Weg,
            // statt dass die Bässe in den ersten Millimetern liegen.
            _SliderRow(
              label: l.frequency,
              value:
                  math.log(band.freqHz.clamp(20, 20000) / 20) / math.log(1000),
              min: 0,
              max: 1,
              unit: '',
              onChanged: (t) =>
                  onChanged(_copy(freqHz: 20 * math.pow(1000, t).toDouble())),
            ),
            Padding(
              padding: const EdgeInsets.only(right: 16, bottom: 2),
              child: Align(
                alignment: Alignment.centerRight,
                child: Text(
                  '${band.freqHz.round()} Hz',
                  style: theme.textTheme.bodySmall,
                ),
              ),
            ),
            _SliderRow(
              label: l.gain,
              value: band.gainDb,
              min: -15,
              max: 15,
              unit: 'dB',
              onChanged: (v) => onChanged(_copy(gainDb: v)),
            ),
            _SliderRow(
              label: l.qFactor,
              value: band.q,
              min: 0.2,
              max: 10,
              unit: 'Q',
              onChanged: (v) => onChanged(_copy(q: v)),
            ),
          ],
        ),
      ),
    );
  }
}

/// Zeichnet die Frequenzkurve des Equalizers.
class _CurvePainter extends CustomPainter {
  _CurvePainter({
    required this.points,
    required this.color,
    required this.gridColor,
  });

  final List<CurvePoint> points;
  final Color color;
  final Color gridColor;

  /// Dargestellter Bereich: ±18 dB.
  static const _range = 18.0;

  @override
  void paint(Canvas canvas, Size size) {
    final grid = Paint()
      ..color = gridColor
      ..strokeWidth = 1;
    // Waagrechte Hilfslinien bei -12, -6, 0, +6, +12 dB; 0 dB etwas kräftiger.
    for (final db in const [-12.0, -6.0, 0.0, 6.0, 12.0]) {
      final y = size.height / 2 - db / _range * size.height / 2;
      canvas.drawLine(
        Offset(0, y),
        Offset(size.width, y),
        grid..strokeWidth = db == 0 ? 1.5 : 0.5,
      );
    }
    if (points.length < 2) return;

    final path = Path();
    for (var i = 0; i < points.length; i++) {
      // Die Punkte sind schon logarithmisch verteilt -> gleichmäßig auftragen.
      final x = size.width * i / (points.length - 1);
      final db = points[i].gainDb.clamp(-_range, _range);
      final y = size.height / 2 - db / _range * size.height / 2;
      i == 0 ? path.moveTo(x, y) : path.lineTo(x, y);
    }
    canvas.drawPath(
      path,
      Paint()
        ..color = color
        ..style = PaintingStyle.stroke
        ..strokeWidth = 2.5
        ..strokeJoin = StrokeJoin.round,
    );
  }

  @override
  bool shouldRepaint(_CurvePainter old) =>
      old.points != points || old.color != color || old.gridColor != gridColor;
}

/// Suche in der AutoEQ-Datenbank oder Einfügen eines Profils als Text.
class _AutoEqSheet extends StatefulWidget {
  const _AutoEqSheet();

  @override
  State<_AutoEqSheet> createState() => _AutoEqSheetState();
}

class _AutoEqSheetState extends State<_AutoEqSheet> {
  final _controller = TextEditingController();
  Timer? _debounce;
  List<AutoEqEntry> _results = const [];
  bool _loading = false;
  String? _error;
  int _requestId = 0;

  @override
  void dispose() {
    _debounce?.cancel();
    _controller.dispose();
    super.dispose();
  }

  void _onChanged(String text) {
    _debounce?.cancel();
    // Enthält der Text "Filter", hat der Nutzer ein ganzes Profil eingefügt
    // und sucht nicht nach einem Kopfhörer.
    if (text.contains('Filter')) return;
    _debounce = Timer(
      const Duration(milliseconds: 400),
      () => _search(text.trim()),
    );
  }

  Future<void> _search(String query) async {
    final id = ++_requestId;
    if (query.length < 2) {
      setState(() => _results = const []);
      return;
    }
    setState(() {
      _loading = true;
      _error = null;
    });
    try {
      final r = await autoeqSearch(query: query);
      if (id != _requestId || !mounted) return;
      setState(() {
        _results = r;
        _loading = false;
      });
    } catch (e) {
      if (id != _requestId || !mounted) return;
      setState(() {
        _error = '$e';
        _loading = false;
      });
    }
  }

  Future<void> _pick(AutoEqEntry entry) async {
    setState(() => _loading = true);
    try {
      final profile = await autoeqFetch(entry: entry);
      if (mounted) Navigator.of(context).pop(profile);
    } catch (e) {
      if (mounted) {
        setState(() {
          _error = '$e';
          _loading = false;
        });
      }
    }
  }

  Future<void> _usePastedText() async {
    try {
      final profile = await parseAutoeq(text: _controller.text);
      if (mounted) Navigator.of(context).pop(profile);
    } catch (e) {
      if (mounted) setState(() => _error = '$e');
    }
  }

  @override
  Widget build(BuildContext context) {
    final l = L.of(context);
    final theme = Theme.of(context);
    final pasted = _controller.text.contains('Filter');
    return Padding(
      // Platz für die Tastatur lassen.
      padding: EdgeInsets.only(bottom: MediaQuery.viewInsetsOf(context).bottom),
      child: SizedBox(
        height: MediaQuery.sizeOf(context).height * 0.7,
        child: Column(
          children: [
            Padding(
              padding: const EdgeInsets.fromLTRB(16, 0, 16, 8),
              child: TextField(
                controller: _controller,
                autofocus: true,
                minLines: 1,
                maxLines: 4,
                onChanged: (t) {
                  setState(() {});
                  _onChanged(t);
                },
                decoration: InputDecoration(
                  labelText: l.autoEq,
                  hintText: l.autoEqHint,
                  prefixIcon: const Icon(Icons.search),
                ),
              ),
            ),
            if (_loading) const LinearProgressIndicator(minHeight: 2),
            if (_error != null)
              Padding(
                padding: const EdgeInsets.all(16),
                child: Text(
                  _error!,
                  style: TextStyle(color: theme.colorScheme.error),
                ),
              ),
            if (pasted)
              Padding(
                padding: const EdgeInsets.all(16),
                child: FilledButton(
                  onPressed: _usePastedText,
                  child: Text(l.autoEqUseText),
                ),
              )
            else
              Expanded(
                child: ListView.builder(
                  itemCount: _results.length,
                  itemBuilder: (context, i) {
                    final e = _results[i];
                    return ListTile(
                      title: Text(e.name),
                      // Dieselben Kopfhörer wurden oft von mehreren Leuten
                      // gemessen; die Quelle hilft bei der Auswahl.
                      subtitle: Text(e.source),
                      onTap: () => _pick(e),
                    );
                  },
                ),
              ),
          ],
        ),
      ),
    );
  }
}
