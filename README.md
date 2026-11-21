# Crosspoint

8×8-Mixer für PipeWire/JACK in Rust: 8 Eingänge, 8 Busse (A1–A5 Hardware,
B1–B3 virtuell), ohne Kernelmodul. Die Eingänge liefern ihr **Rohsignal**;
Filter gibt es nur an den virtuellen Ausgängen B1–B3 – dort, wo das Signal zu
Discord, OBS oder einer Aufnahme geht: ein **Noise Gate** und eine
**Echounterdrückung**, die den Systemton herausrechnet, den die Boxen zurück
ins Mikrofon spielen. Engine und Fenster (egui) laufen in einem Prozess; die
Engine ist ein JACK-Client.

## Installation

Nur Linux. Fertige Pakete liegen unter [Releases](https://github.com/patimweb-labs/crosspoint/releases):

| Weg | Datei | Befehl |
| --- | --- | --- |
| Flatpak (jede Distribution) | `crosspoint-*.flatpak` | `flatpak install --user ./crosspoint-*.flatpak` |
| AppImage (jede Distribution) | `Crosspoint-*-x86_64.AppImage` | `chmod +x Crosspoint-*.AppImage && ./Crosspoint-*.AppImage` |
| Archiv | `crosspoint-*-linux-x86_64.tar.gz` | nach `~/.local` oder `/usr/local` entpacken |

Voraussetzung: PipeWire mit `pipewire-jack` und `pactl` (bei Fedora, Ubuntu,
Debian, Arch und openSUSE Standard). Das Flatpak bringt beides selbst mit.

## Screenshots

![Pult im dunklen Theme](docs/bilder/pult-dark-1920.png)

![Pult im hellen Theme](docs/bilder/pult-light-1920.png)

Die Bilder erzeugt der Oberflächentest (`CROSSPOINT_BILDER=$PWD/docs/bilder
cargo test -p crosspoint-fenster --test oberflaeche`), sie werden nicht von
Hand abgelegt.

## Daten und Datenschutz

Crosspoint arbeitet ausschließlich lokal. Die Steuer-API hört auf
`127.0.0.1:8770`, VBAN nur auf dem eingestellten UDP-Port. Keine Telemetrie,
keine Konten, keine Cloud-Verbindung. Konfiguration und Presets liegen unter
`~/.config/crosspoint`, Aufnahmen im eingestellten Tape-Ordner.

## Signalkette

```
Eingänge 1..8:  Eingang → Mono → Fader → Pan → Routing (8 Busse)        – Rohsignal
Busse A1..A5:   Summe → Mono/Modus → Delay → Trim → Gain/Mute → Ausgang  – Rohsignal
Busse B1..B3:   Summe → Mono/Modus → ECHO → GATE → Delay → Trim → Gain/Mute → Ausgang
```

Ein Mikrofon auf A1 **und** B1 ergibt also das Rohsignal auf A1 (Kopfhörer,
Aufnahme) und das bereinigte auf B1 (`CROSSPOINT-Output-B1`, z. B. für Discord).

**ECHO** (Echounterdrückung): ein adaptiver Filter im Frequenzbereich
(256 ms Filterlänge, zwei Filter gegen Doppelsprechen) lernt, wie der Ton der
Boxen im Mikrofon ankommt, und zieht ihn ab. Referenz ist der fertige Ausgang
eines Hardware-Busses – ab Werk A1, umstellbar per Rechtsklick auf ECHO. Kosten:
rund 0,7 % eines Kerns je Kanal, 2,7 ms Latenz. Gemessen im Test: 40 dB weniger
Echo, die Stimme beim Doppelsprechen bleibt 28 dB über dem Rest.

**GATE**: schließt in Pausen (Schwelle, Tiefe, Attack, Hold, Release per
Rechtsklick); es entscheidet nach der Echounterdrückung, also am echofreien
Signal.

Linksklick auf GATE oder ECHO schaltet, Rechtsklick öffnet die Einstellungen.
Herleitung der Parameternamen des Originals: `docs/02-signalmodell.md`,
`docs/03-ghidra-app.md`, `docs/04-abgleich-original.md`.

## Bauen und starten

Voraussetzungen: `pipewire` (mit `pipewire-jack`), `pactl`, `cargo`, `clang`
(bindgen). Keine `-devel`-Pakete für PipeWire/JACK nötig: `packaging/pkgconfig`
liefert eigene `.pc`-Stümpfe; das Fenster lädt Wayland/X11 und OpenGL zur
Laufzeit.

```bash
cargo build --release --features ui                # Engine und Fenster (egui)
./scripts/crosspoint-launch.sh                     # Fenster mit Engine, Ende bei Fensterschluss
./target/release/crosspoint-engine                 # nur die Engine, Web-UI auf http://127.0.0.1:8770/
packaging/linux/paket.sh                           # dist/: tar.gz und AppImage (braucht appimagetool)
```

`crosspoint-wire.sh up` legt `crosspoint_in_1..3` (Anwendungen spielen hinein →
Streifen 6..8, Anzeigename `CROSSPOINT-Input-1..3`) und `crosspoint_out_B1..3`
(Anwendungen nehmen auf ← Busse B1..B3, Anzeigename `CROSSPOINT-Output-B1..B3`)
als `null-sink` an; die Hardware-Busse A1..A5 kommen über `CROSSPOINT_A1` …
`CROSSPOINT_A5`. Die Engine ruft das Skript beim Start selbst auf
(`CROSSPOINT_NO_WIRE=1` schaltet das ab).

**Aufräumen hängt an keiner Uhr.** Beim Schließen des Fensters läuft
`wire down` (Skript, Merkliste) und danach ein ausdrücklicher Sweep, der ohne
Merkliste **jedes** `crosspoint_*`-Modul entlädt und jeden übrig gebliebenen
Knoten zerstört (`pw-cli destroy`). Fehler werden gemeldet, der Sweep läuft
trotzdem durch, und am Ende wird der Rest nachgezählt
(`Aufraeumen (Fenster zu): N Module, M Knoten entfernt, 0 Fehler; Rest: 0/0`).
Beim Start räumt derselbe Sweep Reste einer abgestürzten Instanz weg, bevor
neue Geräte entstehen. Ein Leerlauf-Wächter beendet im Fensterbetrieb nichts:
die Oberfläche läuft im selben Prozess und fragt den HTTP-Adapter nicht.
`./scripts/crosspoint-wire.sh sweep` macht dasselbe von der Kommandozeile.

Der JACK-Client heißt `crosspoint`, seine Ports sind `crosspoint:strip1_L` …
`crosspoint:busB3_R` sowie `crosspoint:vban_L/_R`.

## Konfiguration

Alles liegt in `~/.config/crosspoint` (bzw. `$XDG_CONFIG_HOME/crosspoint`):

| Datei | Inhalt |
|---|---|
| `preset.json` | aktueller Mischpultzustand |
| `devices.json` | Hardware-Bus → PipeWire-Gerät |
| `inputs.conf` | Streifen 1..5 → Aufnahmequelle |
| `macros.json` | Makro-Tasten |
| `tape/` | Mitschnitte der Bandmaschine |

Beim ersten Start wird ein vorhandenes `~/.config/vm8-linux` einmalig nach
`~/.config/crosspoint` umbenannt (nur wenn das neue Verzeichnis noch fehlt) –
Presets, Gerätewahl und Makros bleiben damit erhalten.

Im **Flatpak** zeigt `XDG_CONFIG_HOME` in den Sandbox-Ordner; die Konfiguration
liegt dort unter `~/.var/app/de.weppelmann.Crosspoint/config/crosspoint`. Der
Autostart-Eintrag dagegen gehört dem Host und landet immer in
`~/.config/autostart`.

## Zustand beim Start, Autosave, Autostart, Tray

Beim Start wird der gemerkte Zustand **wiederhergestellt**, nicht nur gelesen:

1. `devices.json` – jeder Bus wird über denselben Weg wie `POST /api/devices`
   neu verdrahtet (`pw-link` setzen, alte Links lösen).
2. `inputs.conf` – Streifen-Eingänge über `inputs::assign`.
3. `preset.json` – das vollständige Parameterbild.

Das läuft, nachdem die virtuellen Geräte stehen (`crosspoint-wire.sh up`), und
steht danach im Engine-Protokoll:
`Zustand wiederhergestellt: 1 Bus(se), 1 Streifen-Eingang, Preset geladen`.
Geräte, die gerade nicht da sind, werden gezählt und gemeldet, aber nicht als
Fehler behandelt.

**Autosave:** der Autosave-Thread vergleicht im Takt das Parameterbild und
bemerkt so jede Änderung – aus dem Fenster, über `POST /api/set`, OSC/MIDI oder
ein Makro. 2 s nach der letzten Änderung schreibt er das komplette Bild nach
`preset.json` (kein Lock, keine Datei-Arbeit im Audio-Callback); eine
Faderfahrt erzeugt so genau einen Schreibvorgang. Beim Beenden wird eine noch
offene Änderung sofort geschrieben. Gespeichert werden auch Namen und die
Filter von B1–B3. Die VBAN-Einstellungen („SAVE CONFIG", `vban.json`)
gelten ebenfalls nach dem Neustart.

`CROSSPOINT_NO_RESTORE=1` schaltet Wiederherstellen **und** Autosave ab.

**Autostart:** `POST /api/autostart {"enabled":true}` schreibt
`~/.config/autostart/de.weppelmann.Crosspoint.desktop`, `false` entfernt sie
wieder. Der Startbefehl ist im Flatpak
`flatpak run de.weppelmann.Crosspoint --minimized`, auf dem Host
`<repo>/scripts/crosspoint-launch.sh --minimized`. Beide starten ohne Fenster,
nur mit Tray-Icon.

**Tray:** `crosspoint --minimized` meldet ein StatusNotifierItem an (Crate
`ksni`, reines Rust):

| Menüpunkt | Wirkung |
|---|---|
| Fenster zeigen | Fenster öffnen |
| Alles stumm | alle Eingänge stumm schalten bzw. wieder hörbar |
| Letztes Preset laden | gespeichertes `preset.json` anwenden |
| Autostart | Autostart-Datei schreiben/entfernen, Haken zeigt den Zustand |
| Beenden | Programm und Engine beenden |

Mit Tray schließt der Fensterknopf nur das Fenster; Engine und Symbol laufen
weiter, „Fenster zeigen" öffnet es neu (unter Wayland lässt sich ein Fenster
nicht verstecken, deshalb wird es geschlossen und neu geöffnet). Ohne Tray
beendet das Schließen das Programm. Ein zweiter Start (Menü, Launcher) startet
keine zweite Engine, sondern holt das Fenster der laufenden Instanz
(`POST /api/fenster`).

## Steuer-API

```bash
curl -X POST http://127.0.0.1:8770/api/set \
     -d '{"target":"strip","index":0,"param":"gain","value":-12}'
curl http://127.0.0.1:8770/api/state     # kompletter Zustand
curl http://127.0.0.1:8770/api/levels    # Pegel in dB
curl -X POST http://127.0.0.1:8770/api/preset -d '{"action":"save"}'
curl http://127.0.0.1:8770/api/restore   # {"devices":n,"inputs":n,"preset":bool}
curl -X POST http://127.0.0.1:8770/api/restore -d '{"preset":true,"devices":true,"inputs":false}'
curl http://127.0.0.1:8770/api/autostart # {"enabled":…,"path":…,"exec":…}
curl -X POST http://127.0.0.1:8770/api/autostart -d '{"enabled":true}'
curl http://127.0.0.1:8770/api/vban/config   # Sende-/Empfangsweg + beide Tabellen
curl -X POST http://127.0.0.1:8770/api/vban/config \
     -d '{"senden":{"an":true,"host":"192.168.1.255","port":6980,"stream":"CROSSPOINT"},"empfangen":{"an":true,"port":6980}}'
curl -X POST http://127.0.0.1:8770/api/vban/config -d '{"action":"save"}'   # auch "load" und "reset"
```

Parameter der Eingänge: `gain`, `pan`, `mute`, `solo`, `mono`, `send0..send7`.
Busse: `gain`, `mute`, `mono`, `mode`, `cross`, `trim`, `delay`; nur B1–B3
(Index 5–7) zusätzlich `gate`, `gate_schwelle`, `gate_tiefe`, `gate_attack`,
`gate_hold`, `gate_release`, `aec`, `aec_ref` (0–4 = A1–A5). An einem
Hardware-Bus antwortet `gate`/`aec` mit einem Fehler. Ältere Presets mit
Filtern an den Eingängen laden weiter; diese Werte werden übergangen.
Die Engine hat keine Authentifizierung und lauscht nur auf `127.0.0.1`.

**Anzeigenamen:** Streifen und Busse tragen auf Wunsch einen eigenen Namen. Der
Wert ist Text, nicht Zahl; ein leerer Text setzt wieder `HW In 1`/`A1`:

```bash
curl -X POST http://127.0.0.1:8770/api/set \
     -d '{"target":"strip","index":0,"param":"label","value":"Gesang"}'
curl -X POST http://127.0.0.1:8770/api/set \
     -d '{"target":"bus","index":0,"param":"label","value":"Monitor"}'
```

In der Oberfläche genügt ein Doppelklick auf den Namen im Kanalzug. Der Name
steht im Preset (`label` je Streifen/Bus) und kommt beim Wiederherstellen zurück.
Erlaubt sind bis zu 24 Zeichen ohne Steuerzeichen.

## Pakete

`packaging/linux/paket.sh` baut tar.gz und AppImage, `flatpak/de.weppelmann.Crosspoint.yml`
das Flatpak aus denselben, fertig gebauten Programmen (`prebuilt/`, nicht
eingecheckt). Der Release-Workflow hängt alle drei an.

```bash
cargo build --release --features ui
mkdir -p prebuilt && cp target/release/crosspoint target/release/crosspoint-engine prebuilt/
flatpak-builder --user --install --force-clean build-dir flatpak/de.weppelmann.Crosspoint.yml
flatpak run de.weppelmann.Crosspoint
```

Das Flatpak nutzt die Freedesktop-Runtime 25.08; sie enthält `pactl`, die
`pw-*`-Werkzeuge und `libjack.so.0` (pipewire-jack). Die Cargo-Quellen sind in
`flatpak/new-sources.json` eingefroren (`python3 packaging/flatpak/quellen.py >
flatpak/new-sources.json` nach jeder Änderung an `Cargo.lock`).

Finish-Args und warum sie da sind:

| Recht | Grund |
|---|---|
| `--socket=pulseaudio` | `pactl` legt die virtuellen Knoten (`CROSSPOINT-Input-*`, `CROSSPOINT-Output-*`) an |
| `--filesystem=xdg-run/pipewire-0` | nativer PipeWire-Zugriff für JACK-Client und `pw-link`/`pw-dump` |
| `--share=ipc`, `--share=network` | Steuer-API auf `127.0.0.1:8770`, VBAN, OSC |
| `--socket=wayland`, `--socket=fallback-x11`, `--device=dri` | das Fenster (OpenGL) |
| `--socket=session-bus` | Tray-Icon (StatusNotifierItem) |
| `--filesystem=~/.config/crosspoint:create` | Konfiguration/Presets |
| `--filesystem=xdg-config/autostart:create` | Autostart-Datei schreiben/entfernen |

Im AppImage schreibt der Autostart den Pfad der AppImage-Datei (`$APPIMAGE`),
im Flatpak `flatpak run de.weppelmann.Crosspoint --minimized`.

## Grenzen (ehrlich)

- **Echounterdrückung ohne Restecho-Unterdrückung:** der adaptive Filter zieht
  das lineare Echo ab; Verzerrungen kleiner Boxen bleiben leise hörbar. Ein
  nachgeschalteter Suppressor (wie in WebRTC) fehlt noch.
- Die Referenz ist das, was Crosspoint an die Boxen schickt. Spielt eine
  Anwendung an Crosspoint vorbei direkt auf die Boxen, kann ECHO sie nicht
  herausrechnen.
- Bus-Mix-Modi Upmix 2.1/4.1/6.1 ergeben eine Monosumme auf beiden Kanälen
  (Busse sind stereo).
- Keine Latenz-Optimierung: die Engine erbt die PipeWire-Quantum (hier 1024 Frames ≈ 21 ms).
- Nicht getestet: PipeWire-Neustart im Betrieb, mehrere Instanzen, Ratenwechsel.
