# Crosspoint

<p align="center">
  <img src="data/icons/hicolor/256x256/apps/de.weppelmann.Crosspoint.png" width="140" alt="Crosspoint: acht Eingänge, acht Busse, das Kreuz in der Mitte">
</p>

8×8-Mixer für PipeWire/JACK in Rust: 8 Eingänge, 8 Busse (A1–A5 Hardware,
B1–B3 virtuell), ohne Kernelmodul. Die Eingänge liefern ihr **Rohsignal**;
Filter gibt es nur an den virtuellen Ausgängen B1–B3 – dort, wo das Signal zu
Discord, OBS oder einer Aufnahme geht: **Low-Cut**, **Echounterdrückung**
(rechnet den Systemton heraus, den die Boxen zurück ins Mikrofon spielen),
**Rauschunterdrückung**, **Noise Gate**, **Kompressor mit Limiter**, **AGC**,
**Ducking** und **Push-to-Talk** – acht Filter, geschaltet und eingestellt in
einem **FILTER-Modal** je Bus. Engine und Fenster (egui) laufen in einem
Prozess; die Engine ist ein JACK-Client.

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

![FILTER-Modal an B1](docs/bilder/filter-modal-dark.png)

Die Bilder erzeugt der Oberflächentest (`CROSSPOINT_BILDER=$PWD/docs/bilder
cargo test -p crosspoint-fenster --test oberflaeche`), sie werden nicht von
Hand abgelegt.

Dunkel ist Cyan auf Nachtblau, hell ist Schwarz auf Kachelweiß, mono eine
Tinte — und in allen drei Themes sitzt der Kreuzungspunkt bernsteinfarben:
an der Zuordnung `A1 … B3` und an der Spitze der Pegel. Zeichen, Wortmarke und
Icons liegen als SVG in `assets/`, die Farben als Tokens in
`crates/fenster/tokens/` ([ADR 0008](docs/adr/0008-logo-und-farben.md)).

Das Pult **scrollt nicht**: alle 17 Züge mit allen neun Skalenzahlen (+12 …
−60) stehen immer vollständig da. Damit nichts abgeschnitten wird, hat das
Fenster eine Mindestgröße von 1642 × 660 Pixeln, die aus den Maßen des Pults
gerechnet wird ([ADR 0007](docs/adr/0007-pult-ohne-scrollleisten.md)); die
Makrofelder klappen bei knapper Höhe von selbst auf ihre Kopfzeile zusammen.

## Daten und Datenschutz

Crosspoint arbeitet ausschließlich lokal. Die Steuer-API hört auf
`127.0.0.1:8770`, VBAN nur auf dem eingestellten UDP-Port. Keine Telemetrie,
keine Konten, keine Cloud-Verbindung. Konfiguration und Presets liegen unter
`~/.config/crosspoint`, Aufnahmen im eingestellten Tape-Ordner.

## Signalkette

```
Eingänge 1..8:  Eingang → Mono → Fader → Pan → Routing (8 Busse)        – Rohsignal
Busse A1..A5:   Summe → Mono/Modus → Delay → Trim → Gain/Mute → Ausgang  – Rohsignal
Busse B1..B3:   Summe → Mono/Modus → LC → ECHO → NR → GATE → COMP → AGC
                → Limiter → DUCK → PTT → Delay → Trim → Gain/Mute → Ausgang
```

Ein Mikrofon auf A1 **und** B1 ergibt also das Rohsignal auf A1 (Kopfhörer,
Aufnahme) und das bereinigte auf B1 (`CROSSPOINT-Output-B1`, z. B. für Discord).

**ECHO** (Echounterdrückung): rechnet den Ton heraus, den die Boxen spielen und
das Mikrofon wieder aufnimmt – das, was Discord „Echounterdrückung“ und Windows
„Akustische Echounterdrückung“ nennen. Gerechnet wird mit AEC3 (WebRTC-Verfahren,
`sonora`, reines Rust; Begründung und Messung in
[ADR 0005](docs/adr/0005-echounterdrueckung-mit-aec3.md)).

Die **Referenz** kommt vom Ausgabegerät: die Monitore der gewählten Geräte
hängen am Mixer (`<sink>:monitor_FL/FR` → `crosspoint:echo_ref_L/R`), damit auch
Programme erfasst werden, die an Crosspoint vorbei direkt auf das Gerät spielen
(Chromium auf HDMI). Im FILTER-Modal steht unter **ECHO** ein Häkchen je
lokalem Ausgabegerät; mehrere gleichzeitig sind erlaubt (Boxen und Kopfhörer),
die Auswahl steht in `echo.json` und kommt beim Start zurück. Angeboten werden
nur Geräte, die im Raum spielen: nicht Crosspoints eigene Sinks (sonst schlösse
sich die Schleife Bus → Referenz → Bus) und keine AirPlay-/Netzwerkziele. Ohne gewähltes Gerät passiert nichts, das Mikrofon bleibt
unverändert.

Kosten: rund 0,4 % eines Kerns je Kanal, 19 ms Latenz (10 ms Rahmen + 9 ms
AEC3), keine Allokation im Audio-Callback. Spielt nichts, geht das Mikrofon
unverändert und nur verzögert durch. Gemessen auf dem nachgestellten Pfad mit
Nachhall und getrennten Uhren: 14 dB Echodämpfung (der frühere lineare Filter
schaffte dort 2,4 dB), die Stimme kommt beim Doppelsprechen in ihrer Lautstärke
durch.

**NR** (Rauschunterdrückung): Lüfter, Tastatur, Maus. Eigener Schalter,
unabhängig von ECHO, Stufe niedrig/mittel/hoch/sehr hoch (6/12/18/21 dB). Der
alte Parameter `aec_ns` – bis ADR 0005 das Häkchen „Restecho mit weg rechnen“
im ECHO-Menü – schaltet dieselbe Rauschunterdrückung ein, damit alte Presets
laden.

**LC** (Low-Cut): Hochpass 2. Ordnung, 40–200 Hz, ab Werk 80 Hz. Räumt
Rumpeln, Trittschall und Lüfterbrummen weg, bevor die Echounterdrückung
rechnet.

**GATE**: schließt in Pausen (Schwelle, Tiefe, Attack, Hold, Release); es
entscheidet nach der Echounterdrückung, also am echofreien Signal.

**COMP** (Kompressor) und **Limiter**: Schwelle, Ratio, Attack, Release,
Make-up; der Limiter hält die Decke (ab Werk −1 dBFS) und ist getrennt
schaltbar. Beide arbeiten mit gekoppeltem Stereo-Detektor – das Stereobild
wandert nicht, wenn eine Seite leiser wird.

**AGC**: zieht die Sprache auf den eingestellten Zielpegel (ab Werk −12 dBFS),
egal wie laut man spricht. AGC2 aus `sonora`, mit einem eigenen, langsamen
Trim für den Zielpegel; kostet 10 ms Latenz, solange er an ist.

**DUCK** (Ducking): spricht man auf diesem Bus, wird ein anderer Bus (A1–A5
oder B1–B3, z. B. der Spielton) um X dB abgesenkt – mit Schwelle, Tiefe,
Attack und Release. Die Entscheidung fällt mit dem Pegel des vorigen Blocks
(eine Blocklänge, bei 512 Frames 10,7 ms).

**PTT** (Push-to-Talk): ist PTT an, ist der Bus stumm, solange die Taste nicht
gedrückt ist (`ptt_offen`). Der Fusstaster hängt über MIDI/OSC am Bus:
`/bus/<n>/ptt` und `/bus/<n>/ptt_offen`; in der MIDI-Karte (Seite „Busse“)
stehen beide Ziele für B1–B3.

An B1–B3 (und am Master, wenn er einen B-Bus bedient) steht dafür ein Knopf
**FILTER** mit acht Lämpchen: sie zeigen, was an ist. Ein Klick (oder
Rechtsklick) öffnet das **FILTER-Modal** mit allen Schaltern, Reglern und der
ECHO-Geräteauswahl; die Karten stehen in einem Raster aus vier Spalten – acht
Karten, zwei volle Zeilen, auch im kleinsten Fenster – in der Reihenfolge der
Kette, unter dem Titel der Pegel **vor** und **nach** den Filtern. Alle Dialoge (auch PRESETS,
VBAN, MIDI und DARSTELLUNG) teilen sich denselben Rahmen mit dem Schließen-X
oben rechts ([ADR 0009](docs/adr/0009-ein-dialograhmen-fuer-alle.md)).
Esc, Klick daneben und das X schließen es. Reihenfolge und Begründung der
Kette: [ADR 0006](docs/adr/0006-filterkette-und-modal.md).
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

`crosspoint --version` (oder `-V`, auch bei `crosspoint-engine`) gibt die
Version aus und beendet sich sofort – ohne JACK, ohne Fenster; dieselbe Zahl
steht unten im DARSTELLUNG-Dialog.

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
`crosspoint:busB3_R`, `crosspoint:echo_ref_L/_R` (Referenz der
Echounterdrückung, hängt an den Monitoren der gewählten Ausgabegeräte) sowie
`crosspoint:vban_L/_R`.

## Konfiguration

Alles liegt in `~/.config/crosspoint` (bzw. `$XDG_CONFIG_HOME/crosspoint`):

| Datei | Inhalt |
|---|---|
| `preset.json` | aktueller Mischpultzustand |
| `devices.json` | Hardware-Bus → PipeWire-Gerät |
| `inputs.conf` | Streifen 1..5 → Aufnahmequelle |
| `echo.json` | Ausgabegeräte, deren Ton aus dem Mikrofon gerechnet wird |
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
curl http://127.0.0.1:8770/api/echo     # {"geraete":[…],"ausgabegeraete":[…],"links":[…]}
curl -X POST http://127.0.0.1:8770/api/echo \
     -d '{"geraete":["alsa_output.pci-0000_03_00.1.hdmi-stereo"]}'
curl http://127.0.0.1:8770/api/autostart # {"enabled":…,"path":…,"exec":…}
curl -X POST http://127.0.0.1:8770/api/autostart -d '{"enabled":true}'
curl http://127.0.0.1:8770/api/vban/config   # Sende-/Empfangsweg + beide Tabellen
curl -X POST http://127.0.0.1:8770/api/vban/config \
     -d '{"senden":{"an":true,"host":"192.168.1.255","port":6980,"stream":"CROSSPOINT"},"empfangen":{"an":true,"port":6980}}'
curl -X POST http://127.0.0.1:8770/api/vban/config -d '{"action":"save"}'   # auch "load" und "reset"
```

Parameter der Eingänge: `gain`, `pan`, `mute`, `solo`, `mono`, `send0..send7`.
Busse: `gain`, `mute`, `mono`, `mode`, `cross`, `trim`, `delay`; nur B1–B3
(Index 5–7) zusätzlich die Filter:

| Filter | Parameter |
|---|---|
| Low-Cut | `lc`, `lc_freq` (40–200 Hz) |
| Echounterdrückung | `aec` |
| Rauschunterdrückung | `nr`, `nr_stufe` (0–3), dazu `aec_ns` als alter Name |
| Noise Gate | `gate`, `gate_schwelle`, `gate_tiefe`, `gate_attack`, `gate_hold`, `gate_release` |
| Kompressor | `comp`, `comp_schwelle`, `comp_ratio`, `comp_attack`, `comp_release`, `comp_makeup` |
| Limiter | `limiter`, `limiter_decke` (−12…0 dBFS) |
| AGC | `agc`, `agc_ziel` (−40…0 dBFS) |
| Ducking | `duck`, `duck_ziel` (0–7), `duck_tiefe`, `duck_schwelle`, `duck_attack`, `duck_release` |
| Push-to-Talk | `ptt`, `ptt_offen` |

An einem Hardware-Bus antworten diese Namen mit einem Fehler. Ältere Presets
mit Filtern an den Eingängen, mit `aec_ref` oder mit `aec_ns` laden weiter;
`aec_ns` schaltet jetzt die Rauschunterdrückung.

```bash
# Ducking: B1 (Mikrofon) senkt A1 (Spielton) um 12 dB, ab −30 dBFS
curl -X POST http://127.0.0.1:8770/api/set -d '{"target":"bus","index":5,"param":"duck","value":1}'
curl -X POST http://127.0.0.1:8770/api/set -d '{"target":"bus","index":5,"param":"duck_ziel","value":0}'
curl -X POST http://127.0.0.1:8770/api/set -d '{"target":"bus","index":5,"param":"duck_tiefe","value":12}'
# Push-to-Talk: B2 stumm, bis die Taste kommt
curl -X POST http://127.0.0.1:8770/api/set -d '{"target":"bus","index":6,"param":"ptt","value":1}'
curl -X POST http://127.0.0.1:8770/api/set -d '{"target":"bus","index":6,"param":"ptt_offen","value":1}'
# dieselben Wege per OSC: /bus/5/duck, /bus/5/duck_tiefe, /bus/6/ptt_offen
```

Welche Ausgabegeräte als Echo-Referenz dienen, steht nicht im Preset (das ist
eine Eigenschaft des Rechners, nicht des Mischzustands), sondern in
`echo.json`; `/api/echo` liest und schreibt die Liste.
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

- **Echounterdrückung ohne Rauschunterdrückung:** AEC3 zieht das lineare Echo
  ab; Verzerrungen kleiner Boxen bleiben leise hörbar, solange NR aus ist. NR
  nimmt sie mit, kostet aber Mikrofonklang – deshalb ein eigener Schalter und
  keine Vorgabe.
- Die Referenz ist das, was Crosspoint an die Boxen schickt. Spielt eine
  Anwendung an Crosspoint vorbei direkt auf die Boxen, kann ECHO sie nicht
  herausrechnen.
- Bus-Mix-Modi Upmix 2.1/4.1/6.1 ergeben eine Monosumme auf beiden Kanälen
  (Busse sind stereo).
- Keine Latenz-Optimierung: die Engine erbt die PipeWire-Quantum (hier 1024 Frames ≈ 21 ms).
- Nicht getestet: PipeWire-Neustart im Betrieb, mehrere Instanzen, Ratenwechsel.
