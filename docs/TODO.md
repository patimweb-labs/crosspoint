# Crosspoint — offene Punkte

Stand: 2026-10-29. Erledigt ist alles bis Release `v0.2.0` (natives Fenster,
Zustand merken, Autostart, Tray). Diese Liste ist die Arbeitsgrundlage für das,
was noch fehlt — sortiert nach Nutzen pro Aufwand.

## 0. Stand 2026-11-14 (Fehlerrunde)

Erledigt in dieser Runde:

- [x] **Autosave aus dem Fenster**: Änderungen in der GTK-Oberfläche wurden nie
      gespeichert (nur HTTP/OSC markierten). Der Autosave erkennt Änderungen
      jetzt selbst (`restore::Beobachter`) und sichert beim Beenden.
- [x] **Preset vollständig**: Namen (`label`), EQ-Bänder (`eq_cells`),
      Kompressor (`comp_*`) und Gate-Zeiten werden gespeichert **und** geladen.
- [x] **VBAN-Dialog** auf echten Daten (vorher Beispieldaten): Senden/Empfangen
      schalten, Host/Port/Stream, Incoming/Outgoing-Tabelle, Reset/Load/Save.
      `vban.json` wird beim Start geladen.
- [x] **MIDI-Dialog**: MIDI-Geräte aus dem ALSA-Sequencer statt Audio-Eingängen;
      erzeugte `midimonster.cfg` mit gültigem Instanznamen, `write` = Ausgang,
      Wertebereichen (sonst Fader bei 0..1 dB) und Lern-Weiterleitung
      `cc{0..127} > /midi/cc/{0..127}`; LEARN verwirft alte CC-Meldungen;
      Ziele `/strip/8..12` (gibt es nicht) ersetzt durch Bus-Gain bzw. Solo.
- [x] OSC `/midi/cc/0 <wert>` wurde verworfen (CC 0 = Fader 1).
- [x] **COMP** je Kanalzug (Rechtsklick: Schwelle, Ratio, Attack, Release,
      Ausgang), **T/D** am Master (Trim und Delay des gewählten Busses).
- [x] `cargo fmt`, `clippy -D warnings`, zwei veraltete Tests (Theme-Farben,
      WebSocket-Startframe).

Nachgezogen am 2026-11-15:

- [x] `crates/macros/target/` aus dem Index genommen (stand schon in `.gitignore`).
- [x] Web-Referenz: COMP- und GATE-Regler schalten die Stufe jetzt ein
      (Anschlag = aus) und folgen dem Zustand aus `/api/state`.
- [x] MIDIMonster: `[a-b]` ist dort eine Zeichenklasse, keine Zahlenspanne –
      CC-Muster daher `/midi/cc/*` (geprüft an `backends/osc.md`).
- [x] Screenshots in allen drei Themes (Pult, VBAN, MIDI), erzeugt mit
      `CROSSPOINT_NO_WIRE=1 CROSSPOINT_NO_RESTORE=1` auf eigenem Xwayland;
      dabei Schalter und Eingabefelder der Dialoge an die Tokens angebunden.
- [x] Speichern von Ende zu Ende belegt: Name, Comp, EQ-Band, Trim setzen,
      schließen, neu starten – Werte sind wieder da.

## 1. Läuft gerade

- [x] **VBAN-Dialog im Original-Look**: `IP Audio Network Configuration` mit
      VBAN an/aus, UDP-Port, IP, Main Stream 48 000 Hz, Incoming- und
      Outgoing-Tabelle (Streamname, IP, SR, Ch, Format, Quality, Destination
      bzw. Source, Net Quality, Errors), Reset/Load/Save Config.
      Abnahme: Dialog per Screenshot, `GET/POST /api/vban/config` per `curl`
      gesetzt und zurückgelesen.
- [x] **Eigene Namen für Ein- und Ausgänge**: Parameter `label` je Streifen und
      Bus, Anzeige im Kanalzug statt `HW IN 1` / `Bus A1`, gespeichert über den
      vorhandenen Autosave.
      Abnahme: `curl` setzt Label, UI zeigt es, nach Neustart noch da.

## 2. MIDI — über MIDIMonster, nicht selbst gebaut

- [x] **OSC-Eingang in der Engine**: `POST`/UDP-OSC auf `127.0.0.1:9000`,
      Nachrichten `/strip/<n>/<param> <wert>` und `/bus/<n>/<param> <wert>`,
      angewendet über dieselbe Funktion wie `/api/set`.
      Abnahme: `oscsend` setzt einen Fader, `/api/state` zeigt den Wert.
- [x] **MIDIMonster-Konfiguration** mitliefern (`data/midimonster.cfg`):
      MIDI-Backend auf das vorhandene Gerät, Mapping CC → OSC-Pfad, ein
      Beispiel-Mapping für acht Fader und Mute.
      Abnahme: CC `B0 07 64` bewegt Streifen 1 über den ganzen Weg.
- [x] **Learn-Hilfe im UI**: Zeile anklicken, Fader bewegen, Zuordnung wird in
      `midimonster.cfg` nachgetragen (die Map-Tabelle bleibt Anzeige + Editor,
      die Arbeit macht MIDIMonster).
      Abnahme: neuer Eintrag in der Datei, nach Neustart wirksam.

## 3. Effekte

- [ ] **LV2-Inserts über mod-host**: je Streifen und Bus ein Insert-Slot,
      mod-host als betreuter Kindprozess (Socket 5555), Plugin-Liste aus
      `lv2ls`, Zustand im Preset.
      Abnahme: Kompressor-Plugin geladen, Pegel vor/nach messbar.
- [ ] **Carla als VST-Brücke** (optional, nur wenn installiert): Erkennung,
      Start, Rack je Streifen, Parameter über OSC; im UI als „externer Host"
      gekennzeichnet. VST2/VST3 laufen dort, nicht in Crosspoint.
      Abnahme: Carla-Rack mit einem VST, Signal läuft durch den Streifen.

## 4. Parität zum Original (aus `docs/04-abgleich-original.md`)

- [ ] Intellipan / Voice Color (`paneltype`, `Panel3Dx/y`, `ColorPanel*`)
- [ ] Sends `R`, `D`, `Fx1`, `Fx2` mit Pre/Post-Schalter
- [ ] Bus-Returns `retR`, `retD`, `retFx1`, `retFx2`
- [ ] `SEL` und `monitor` je Bus
- [ ] FX-Preset-Programme `prg_c`, `prg_g`, `prg_d`
- [ ] `vaio` — Bus als eigenes virtuelles Gerät
- [ ] `muc`, `karaoke`, `stripmode`, `greenmode`

## 5. Verpackung und Vertrieb

- [ ] **macOS**: CoreAudio-Backend statt JACK, `.dmg`-Job, Signierung
      (ohne Signatur „Right-click → Open" dokumentieren)
- [ ] **Flathub-Einreichung**: Metainfo ist da, Screenshots im Repo,
      Einreichung als PR gegen `flathub/flathub`
- [x] **Host-Betrieb ohne Flatpak** ebenfalls nativ: `crosspoint-launch.sh`
      startet `crosspoint` (GTK4, `cargo build --release --features ui`)
- [ ] release-please-Kette einmal komplett durchlaufen lassen
      (Feature-Commit → Release-PR → Merge), bisher wird per Tag ausgelöst

## 6. Aufräumen

- [ ] `docs/04-abgleich-original.md` nach jedem Feature nachziehen
- [x] Brave-Reste des alten Fensterpfads entfernt: `crosspoint-launch.sh`
      startet das native Fenster, `crosspoint-browser-prefs.sh` und
      `tools/fenster_screenshot.py` (CDP gegen Brave) sind weg (2026-11-15)
- [x] Tray im nativen Fenster (`crosspoint --minimized`, `crates/ui-gtk/src/tray.rs`);
      `crosspoint-fenster` bleibt nur Rückfall (2026-11-15)

## Nicht geplant (bewusste Entscheidung)

- **Eigenes VST-Hosting**: VST3-SDK, Plugin-Scan, GUI-Einbettung, Sandboxing —
  Wochen Arbeit. Carla macht das seit Jahren; wir verdrahten es.
- **Windows-VSTs nativ**: braucht Wine. Nur über Carla, nicht in Crosspoint.
- **1:1-Kopie der Original-Oberfläche**: Marken- und Designrisiko. Layout und
  Bedienlogik bleiben vertraut, Farben, Logo und Namen sind eigen.
