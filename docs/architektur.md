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

Signalweg (ADR 0002): Eingänge und Hardware-Busse tragen das Rohsignal;
Noise Gate und Echounterdrückung (`crates/core/src/aec.rs`, AEC3 aus `sonora`)
sitzen nur an den virtuellen Bussen B1–B3. Die Echounterdrückung nimmt den
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

## Darstellung: drei Themes, eine Dichte

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
  Breite der Züge folgt dem Fenster (`logik::zugbreiten`), bei Full HD passen
  alle 17 Züge. Wählbar bleibt nur die Dichte (kompakt, normal, breit) und die
  Zahl der Makrofelder – gespeichert in `~/.config/crosspoint/fenster.json`.

## Plattformen: nur Linux

Ausgeliefert wird als Flatpak, AppImage und tar.gz (ADR 0004) – damit ist
jede Distribution mit PipeWire abgedeckt, ohne je Distribution ein eigenes
Paket zu pflegen.

### Warum nicht Windows oder macOS

Entschieden am 2026-10-29 nach Aufwandsmessung (`wc -l` über den Code).

**Windows: nicht geplant.** Der Mixer selbst wäre portierbar (≈19 % des Kerns
ist plattformnah, DSP/VBAN/OSC/HTTP sind plattformneutral), aber virtuelle
Geräte — also genau das, was einen VoiceMeeter-Ersatz ausmacht — brauchen einen
**signierten Kernel-Treiber**: 400–800 h Arbeit plus EV-Zertifikat
(300–700 €/Jahr). Ein Pult ohne virtuelle Kabel (120–200 h) löst den
Anwendungsfall „Programm X auf Kanal 3" nicht und wäre damit kein Ersatz.

**macOS: nicht geplant, aber machbar.** Kein Treiber nötig (Audio Taps ab
macOS 13), aber CoreAudio-Backend, Taps, `.app`, Notarisierung (99 €/Jahr) und
Testhardware ergeben 170–240 h. Nur sinnvoll mit einem Mac zum Belegen und
einer Gruppe, die es vermisst.

**Regel für später:** Wer die Portierung wieder aufbringt, beginnt mit dem
Audio-Backend allein (60–80 h) und entscheidet erst danach über Oberfläche und
Geräte-Routing. Nicht mit dem Treiber anfangen — daran sterben solche Projekte.
