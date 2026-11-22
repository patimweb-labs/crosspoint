# Architektur: ein Prozess, ein Zustand, dünne Ränder

Stand: 2026-10-29. Diese Notiz beantwortet die Frage „was ist der sauberste Weg,
ohne technische Brüche" — und zwar als Zielbild, nicht als Wunschliste.

## Was heute bricht

Ein Bruch ist jede Stelle, an der eine Information neu verpackt oder ein
Zustand doppelt geführt wird. Gezählt am aktuellen Stand:

| Bruch | Wo | Symptom, das schon aufgetreten ist |
|---|---|---|
| Oberfläche ↔ Engine über HTTP + JSON | `web/` → `src/http.rs` | 2 445 Byte JSON je Abfrage, WS-Frames 60×/s, weiße `<select>`-Felder, Fenster- und Gerätezustand laufen auseinander |
| Launcher-Skripte ↔ echter Prozess | `scripts/crosspoint-launch.sh`, Fensterbeobachtung | Engine wurde vom Aufräum-Trap gekillt, Geräte blieben liegen, Brave-Fenster tauchten wieder auf |
| Host-Weg und Flatpak-Weg | Launcher-Verzweigung, SDK-Abhängigkeit | zwei Startpfade, zwei Fehlerquellen |
| Eigenbau-Infrastruktur | JSON, HTTP, WS, JACK-ABI, Ringpuffer | behoben (serde, tiny_http/tungstenite, jack, rtrb/hound) |
| Textausgaben von `pactl`/`pw-link` | `pw.rs`, `devices.rs`, `inputs.rs` | Mono-Mikro nicht verbunden, Monitore als Geräte, Beschreibungen abgeschnitten — in Arbeit (pipewire-Crate) |
| Sechs Konfigurationsdateien in drei Formaten | `devices.json`, `inputs.conf`, `preset.json`, `macros.json`, `vban.json`, `fx.json` | Reihenfolge und Besitz unklar, Autosave schreibt quer |
| Fremdprozesse ohne gemeinsame Grenze | `mod-host`, `Carla`, `MIDIMonster`, VBAN, OSC | jeder mit eigenem Protokoll direkt im Kern verdrahtet |

## Zielbild

```
                     ┌──────────────────────────────────┐
   Tastatur/Maus ───▶│  egui-Fenster (im Prozess)       │
   MIDI ────────────▶│                                  │
                     │        Arc<Params>  (Atomics)    │──▶ Audio-Thread (JACK/PipeWire)
                     │        Befehlskanal (mpsc)       │
                     │        Pegelring (lock-frei)     │
                     └───────────┬──────────────────────┘
                                 │ dieselben Befehle
        ┌────────────────────────┼─────────────────────────┐
        ▼                        ▼                         ▼
  HTTP-Adapter (127.0.0.1)  OSC-Adapter (UDP)      VBAN-Adapter (UDP)
   Fernbedienung/Tests       MIDI über MIDIMonster   Netzwerk-Audio
```

Signalweg (ADR 0002/0006): Eingänge und Hardware-Busse tragen das Rohsignal;
die Filterkette sitzt nur an den virtuellen Bussen B1–B3 – Low-Cut,
Echounterdrückung (`crates/core/src/aec.rs`, AEC3 aus `sonora`),
Rauschunterdrückung (dieselbe Instanz), Noise Gate, Kompressor und Limiter
(`crates/core/src/dsp.rs`), AGC (`crates/core/src/agc.rs`, AGC2 aus `sonora`),
Ducking und Push-to-Talk. Die Echounterdrückung nimmt den
Ton der gewählten Ausgabegeräte (`crates/core/src/echo.rs` legt deren Monitore
auf `crosspoint:echo_ref_L/R`) und rechnet ihn aus dem Mikrofon. Der frühere
Weg – der fertige Ausgang eines Hardware-Busses als Referenz – ist mit
ADR 0005 entfallen: Programme, die an Crosspoint vorbei spielen, waren darin
nicht enthalten.

Fenster (ADR 0003): `crates/fenster` ist egui wie in sprechwerk –
`modell.rs` (Summentypen), `logik.rs` (Aktionen ausführen, getestet),
`ansichten/` (`render(modell) → Aktionen`), `bausteine.rs` (gezeichnete Fader,
Pegel, Schalter), `thema.rs` (drei Themes aus `tokens/*.tokens.json`,
erzeugt in `build.rs`). Oberflächentests mit `egui_kittest` bedienen das
Fenster über AccessKit und erzeugen die Bilder in `docs/bilder/`.

Vier Regeln, aus denen alles folgt:

1. **Ein Prozess für Bedienung und Audio.** Die Oberfläche liest und schreibt
   `Params` direkt. Kein JSON, kein Socket, keine Serialisierung auf dem Weg
   vom Fader zum Ton.
2. **Ein Befehlskanal für alles, was nicht Atomics ist.** Gerätezuordnung,
   Datei-Zugriff, Fremdprozesse laufen über `mpsc` an einen Worker; die
   Oberfläche blockiert nie. Die HTTP-Routen rufen **dieselbe** Funktion.
3. **Protokolle sind Ränder, nicht Wege.** HTTP, OSC und VBAN sind Adapter auf
   dem Befehlskanal. Fällt ein Adapter weg, ändert sich am Kern nichts.
4. **Ein Zustand, ein Schreiber.** Eine Datei `state.json` mit Abschnitten
   (`devices`, `inputs`, `preset`, `macros`, `vban`, `fx`, `labels`), ein
   Autosave, eine Migration beim ersten Start. Alte Dateien werden gelesen und
   danach nicht mehr geschrieben.

## Was das konkret bedeutet

- **Kein Launcher-Skript mehr als Startweg.** Das Flatpak führt `crosspoint`
  aus; Tray, Autostart, Fenster und Engine sind derselbe Prozess. Skripte
  bleiben nur als Entwicklerhilfen.
- **Kein zweiter Startpfad.** Der Host-Betrieb ist derselbe Prozess mit anderem
  Datenverzeichnis, nicht ein anderer Weg über einen Browser.
- **Fremdprozesse hinter einer Grenze.** Ein Modul `exthost` mit Start,
  Health-Check und Abschaltung; `mod-host` und `Carla` sind Implementierungen
  davon. Fehlt der Host, ist das Feature „aus" und wird als solches gemeldet —
  kein Sonderpfad im Kern.
- **PipeWire nativ, in-process.** Die Registry läuft im selben Prozess, die
  Oberfläche liest den Snapshot; keine Textausgaben, keine Shell.

## Was ausdrücklich ein Bruch bleibt (und warum)

- **Flatpak-Sandbox ↔ Host-PipeWire**: eine OS-Grenze. Nicht auflösbar, nur
  sauber dokumentiert (`--socket=pulseaudio`, `--filesystem=xdg-run/pipewire-0`).
- **Plugin-Hosts in eigenem Prozess**: ein abstürzendes VST darf den Mixer
  nicht mitnehmen. Das ist Absicht, kein Provisorium.
- **HTTP-Adapter**: bleibt für Fernbedienung, Tests und die `VBVMR_*`-Schnitt-
  stelle. Er ist Rand, nicht Bedienweg — abschaltbar mit `CROSSPOINT_HTTP=0`.

## Reihenfolge der Umsetzung

1. **Oberfläche in den Prozess** (erst GTK4, seit 2026-11-21 egui;
   `Arc<Params>`, Pegelbank) — erledigt, HTTP ist nur noch Adapter.
2. **Zustand zusammenführen** (`state.json`, ein Autosave, Migration).
3. **Launcher und Startpfade einebnen** (ein Binary, ein Weg).
4. **`exthost`-Grenze** für Effekte, danach MIDIMonster als Adapter dokumentieren.
5. **PipeWire-Umbau abschließen** (läuft bereits).

Jeder Schritt muss einzeln lauffähig sein; nach jedem Schritt Tests grün und
die API-Antworten unverändert. Kein Schritt darf einen neuen Adapter
einführen, ohne dass ein alter wegfällt.

## Darstellung: drei Themes, feste Anordnung

Seit dem Wechsel auf egui (ADR 0003) gilt AGENTS.md §2.7 wörtlich: drei
Themes (`dark`, `light`, `mono`), keine Nutzer-Themes, kein Theme-Editor.

- Farben, Abstände und Größen stehen in `crates/fenster/tokens/*.tokens.json`;
  `build.rs` erzeugt daraus Konstanten. Im Code stehen nur Namen
  (`f.accent_default`, `SIZE_FADER_BAHN`).
- **Auch selbst gezeichnete Fader und Pegel** lesen ihre Farben aus dem Theme.
- **Icons sind Dateien, keine Glyphen.** `crates/fenster/icons/*.svg` sind eine
  kuratierte Auswahl aus Lucide (ISC, Beilage `LUCIDE-LICENSE.txt`), in
  `bausteine::Icon` eingebettet und über `tint` mit der Textfarbe eingefärbt.
  Ein Icon steht **vor** dem Wort, nie statt seiner: der Name bleibt der
  zugängliche Name des Knopfs. Die Schrift (IBM Plex Mono) hat keine
  geometrischen Formen; statt Ersatzzeichen zu setzen, wird gezeichnet.
- Die Anordnung ist fest (Kopf, Tape, Eingänge | Busse | Master, Makros); die
  17 Züge füllen die Fläche zwischen Kopf- und Makroleiste genau
  (`logik::zugbreiten`). **Es scrollt nichts** (ADR 0007): Statt einer
  Scrollleiste hat das Fenster eine Mindestgröße, die aus den Maßen des Pults
  gerechnet wird (`logik::mindestfenster`, 1642 × 660). Alle neun
  Skalenzahlen stehen immer da; reicht die Höhe nicht für die Makrofelder,
  zeigt die Leiste nur ihre Kopfzeile. Wählbar bleiben Theme und Zahl der
  Makrofelder – gespeichert in `~/.config/crosspoint/fenster.json`.
- **Die Version ist eine Zeile, keine Einstellung.** Unten im
  DARSTELLUNG-Dialog steht gedämpft `Crosspoint <version>`. Die Zahl kommt
  nicht aus dem Fenster-Crate – dort stünde die Version der Bibliothek –,
  sondern aus dem Wurzelpaket: `src/bin/crosspoint.rs` liest
  `env!("CARGO_PKG_VERSION")`, gibt sie an `betrieb::starten` weiter, das sie
  ins Feld `Oberflaeche::version` legt. Dieselbe Zahl gibt
  `crosspoint --version` aus und beendet sich dabei, **bevor** eine Instanz
  geprüft oder JACK geöffnet wird.

## Plattformen: Linux ausgeliefert, macOS im Unterbau

Ausgeliefert wird heute als Flatpak, AppImage und tar.gz (ADR 0004) – damit ist
jede Distribution mit PipeWire abgedeckt, ohne je Distribution ein eigenes
Paket zu pflegen. macOS ist seit 2026-11-21 **geplant und beschlossen**
([ADR 0010](adr/0010-macos-unterbau.md), [plan-macos.md](plan-macos.md)), aber
noch nicht gebaut: entschieden ist der Unterbau, nicht das Ausliefern.

### Zwei Grenzen statt einer Portierung

PipeWire lässt sich nicht portieren, der Mixer schon. Der Kern bekommt deshalb
zwei Traits, hinter denen je eine Plattform sitzt:

```
crates/fenster     kennt nur Modell, Aktionen und Umgebung (als Daten)
     │
crates/core
     ├── AudioBackend    ein Callback, 18 Puffer je Block
     └── GeraeteBackend  welche Geräte, welcher Bus wohin, Echo-Referenz
              │                       │
      JACK + PipeWire            Core Audio
      (heute, Linux)             (neu, macOS)
```

Gewählt wird in `src/` (Kompositionswurzel), **nie** im Fenster: AGENTS.md §2.10
gilt hier wörtlich, `crates/fenster` bekommt kein `cfg(target_os)`, sondern eine
`Umgebung` im Modell. `cfg` bleibt nur in `Cargo.toml`, wo Cargo es erzwingt.

Die Naht ist schon da: `EngineDsp::process(&Params, &Io, frames)` bekommt 18
rohe Pufferzeiger und weiß nichts von einem Audio-Server. `Zugriff`
(`crates/core/src/pw.rs`) ist der Keim des Geräte-Backends und wandert nach
`core/src/backend/geraete.rs`. Zusammen sind das 24–40 h – und sie lohnen auch
ohne macOS, weil danach Tests ohne PipeWire laufen.

### Warum nicht Windows

Entschieden am 2026-10-29 nach Aufwandsmessung (`wc -l` über den Code).

**Windows: nicht geplant.** Der Mixer selbst wäre portierbar (≈20 % des Kerns
ist plattformnah, DSP/VBAN/OSC/HTTP sind plattformneutral), aber virtuelle
Geräte — also genau das, was einen VoiceMeeter-Ersatz ausmacht — brauchen einen
**signierten Kernel-Treiber**: 400–800 h Arbeit plus EV-Zertifikat
(300–700 €/Jahr). Ein Pult ohne virtuelle Kabel (120–200 h) löst den
Anwendungsfall „Programm X auf Kanal 3" nicht und wäre damit kein Ersatz. Die
Grenze aus ADR 0010 macht es später leichter, entscheidet aber nichts darüber.

### macOS (ADR 0010)

Kein Kernel-Treiber: virtuelle Geräte kommen von **BlackHole** (kostenlos,
signiert, vom Nutzer per `brew install blackhole-2ch` installiert) und werden
im Modell auf B1–B3 abgebildet. Ein eigenes *Audio Server Plug-in* in
`/Library/Audio/Plug-Ins/HAL` (userspace, `libASPL`, Shared Memory und zwei
lock-freie Ringpuffer) bleibt als Option – aber ohne Entwicklerkonto gibt es
keine Developer ID, und ein nur ad-hoc signiertes Plugin lädt nach den
vorliegenden Berichten nicht zuverlässig (AMFI, `-423`; der Befund ist
unsicher, siehe [plan-macos.md](plan-macos.md) §3.2). Audio läuft über die
Core-Audio-HAL-API (`objc2-core-audio`, nicht `cpal` – cpal verbirgt Aggregat-
Geräte und IOProc). Die **Echo-Referenz** aus ADR 0005 wird auf macOS nicht aus
Monitor-Ports gebaut, sondern aus einem globalen **Core-Audio-Tap**
(`AudioHardwareCreateProcessTap`, seit **macOS 14.2**) – das ist näher am
Original als der PipeWire-Weg, weil ein Gerät keinen Monitor haben muss. Die
frühere Notiz „Audio Taps ab macOS 13" war zu früh. Unterstützt wird nur die
jeweils **aktuelle macOS-Version** (Frage 1, beantwortet am 2026-11-21), die
Taps sind damit immer vorhanden – und damit **nur Apple Silicon**: macOS 26
(Tahoe) war die letzte Version für Intel-Macs.

Aufwand **104–164 h** in sechs Schritten, jeder einzeln lauffähig; mit dem
optionalen eigenen Plugin 164–264 h. **Geld: 0 €** – kein Apple Developer
Program, kein Zertifikat. Die App wird **ad hoc** signiert und über
GitHub-Releases verteilt; der Preis dafür ist eine Gatekeeper-Warnung beim
ersten Start (seit macOS 15 führt kein Rechtsklick mehr daran vorbei, sondern
nur *Systemeinstellungen › Datenschutz & Sicherheit › Trotzdem öffnen*) und
damit eine Anleitung im README. Die Zahl liegt über den früher genannten
170–240 h, weil dort die Trait-Grenze (a) und der Ersatz für `echo.rs` (e)
fehlten. Risiko: ein eigenes Plugin liefe im `coreaudiod`, ein Absturz dort
nimmt den Ton des ganzen Macs mit – ein weiterer Grund, warum BlackHole
trägt.

### Regel für später

Wer die Portierung aufbringt, beginnt **nicht** mit dem Treiber und nicht mit
der Oberfläche, sondern mit der Grenze: erst `AudioBackend` und
`GeraeteBackend` einziehen, Linux unverändert grün, dann ein zweites Backend
dahinter. Erst danach virtuelle Geräte. Daran sterben solche Projekte.
