# Crosspoint — offene Punkte

Stand: 2026-10-29. Erledigt ist alles bis Release `v0.2.0` (natives Fenster,
Zustand merken, Autostart, Tray). Diese Liste ist die Arbeitsgrundlage für das,
was noch fehlt — sortiert nach Nutzen pro Aufwand.

## Stand 2026-11-21 (Umbau)

Erledigt:

- [x] **Rohsignal an den Eingängen**: EQ, Kompressor, Gate, NR, TK, Lowcut,
      Effektwege und Inserts sind aus den Kanalzügen und den Hardware-Bussen
      entfernt (ADR 0002).
- [x] **Noise Gate und Echounterdrückung an B1–B3** (`crates/core/src/aec.rs`),
      Referenz = Ausgang eines Hardware-Busses (ab Werk A1).
- [x] **Fenster in egui** statt GTK4 (`crates/fenster`, ADR 0003), Bilder aus
      dem Oberflächentest, alle drei Themes.
- [x] **Pakete**: Flatpak (Freedesktop-Runtime), AppImage, tar.gz (ADR 0004).
- [x] Tray mit `ksni`; mit Tray schließt der Fensterknopf nur das Fenster,
      ein zweiter Start holt das Fenster der laufenden Instanz.

- [x] **Echo-Referenz vom Ausgabegerät** statt vom A-Bus: Monitore der
      gewählten Geräte auf `echo_ref_L/R`, Auswahl in `echo.json`,
      Häkchen je Gerät im ECHO-Menü (ADR 0005).
- [x] **AEC3 statt eigenem Filter** (`sonora`, reines Rust): 21 dB statt 2,4 dB
      bei getrennten Uhren, 0 Allokationen im Callback (ADR 0005).
- [x] **Filterkette an B1–B3**: LC, ECHO, NR (eigener Schalter, Stufe), GATE,
      COMP + Limiter, AGC, DUCK, PTT, in einem FILTER-Modal statt acht Knöpfen
      (ADR 0006). `aec_ns` schaltet jetzt NR, alte Presets laden weiter.

- [x] **Filterkette am Kanalzug** (ADR 0011): dieselbe Kette wie an B1–B3, aber
      vor dem Fader; Presets tragen sie als `filter` am Streifen.

Offen:

- [ ] **Web-Fernbedienung an die Kette anpassen**: `web/index.html` zeigt am
      Kanalzug noch EQ/COMP/GATE/NR, die Engine lehnt sie ab. Die Kette am Kanal
      (`/api/set` mit `ziel:strip`, `param:lc` …) ist dort noch nicht bedienbar.
- [ ] **Cheat aus ADR 0009, abzubauen:** die ECHO-Karte im FILTER-Modal zeigt
      höchstens sieben lokale Ausgabegeräte und darunter „… n weitere Geräte“
      (der Rest nur im Tooltip). Grund: kein Scrollen (ADR 0007) und eine
      feste Kartenhöhe im Raster. Sauber wäre eine zweite Spalte innerhalb der
      Karte oder ein Aufklapper; erst nötig, wenn jemand mehr als sieben lokale
      Sinks hat.
- [ ] **Globale Tastenkürzel für PTT** (Fusstaster ohne MIDI, z. B. F13 oder
      Maustaste 4): unter Wayland darf ein Programm ohne Portal keine Tasten
      außerhalb seines Fensters abgreifen. Sauber wäre das
      `GlobalShortcuts`-Portal (`org.freedesktop.portal.GlobalShortcuts`,
      Wayland 1.4+/xdg-desktop-portal 1.19) – das gibt es nicht auf jedem
      Schreibtisch, und GNOME hat es lange nicht angeboten. Bis dahin: MIDI
      (`/bus/<n>/ptt_offen` in der MIDI-Karte) oder OSC. Nicht halb bauen:
      ein X11-Weg über `XGrabKey` wäre auf Wayland wirkungslos und auf X11
      ein zweiter Codepfad.
- [ ] **RNNoise als Alternative zur NR:** `sonora`s NS ist WebRTCs
      Spektralverfahren (6/12/18/21 dB) und frisst bei hohen Stufen
      Sprachanteile. RNNoise (BSD-3, GRU-basiert, 48 kHz) klingt bei gleicher
      Dämpfung natürlicher, braucht aber ein Modell (~90 KiB) und eine
      C-Bindung (`nnnoiseless` ist reines Rust) – als *zusätzliche* Stufe
      hinter NR denkbar, nicht als Ersatz, weil die Stufen dann zwei
      Bedeutungen hätten. Messen: Sprache bei 48 kHz, SNR-Gewinn und
      Verständlichkeit, im Vergleich zu `nr_stufe = 2/3`.
- [ ] **Einzelne Programme als Echo-Referenz wählbar:** heute hängt die
      Referenz am Ausgabegerät (Monitor des Sinks) – alles, was dort landet,
      ist in der Referenz. Wer nur den Browser herausrechnen will und den
      Rest (Spiel, Musik) behalten möchte, braucht den Ton je Anwendung statt
      je Gerät: `pw-dump` liefert Streams (`media.class: Stream/Output/Audio`)
      mit eigenem `node.name`; sie ließen sich wie Sinks behandeln
      (Monitor gibt es dort nicht, dafür den Stream-Ausgang direkt abgreifen
      und auf `echo_ref_L/R` legen). Offene Fragen: welche Streams sind
      stabil benannt, wie wird die Auswahl gespeichert, was passiert bei
      einem Stream-Neustart.
- [x] **Restecho-Unterdrückung als Schalter je Bus**: `aec_ns` an B1–B3,
      Häkchen „RESTECHO MIT WEGRECHNEN“ im ECHO-Menü, Vorgabe aus. Messung:
      +12–13 dB auf allen Pfaden (`echo_verfahren`), im eingeschwungenen
      Zustand weiter 0 Allokationen im Callback. Offen bleibt die Abnahme am
      echten Mikrofon: ob der rauere Klang bei Sprache akzeptabel ist, lässt
      sich nur mit Aufnahme beurteilen.
- [ ] Echounterdrückung mit echten Boxen und Mikrofon abhören: die
      Entscheidung stützt sich auf den nachgebauten Pfad und die frühere
      Feldmessung (1,5 dB gegen 30 dB), nicht auf eine eigene Aufnahme mit
      AEC3 im Raum. Der Restecho-Schalter gehört in diese Abnahme.
- [ ] Flatpak einmal vollständig mit `flatpak-builder` bauen (lokal fehlt
      `org.freedesktop.Sdk//25.08`; die CI baut es).
- [ ] AppImage lokal bauen und auf einer zweiten Distribution starten.
- [ ] `web/index.html` (Web-Fernbedienung) zeigt noch EQ/COMP/GATE/NR an den
      Eingängen; die Engine lehnt diese Werte ab. Auf Rohsignal + Filter an
      B1–B3 umstellen.

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

- [ ] **Flathub-Einreichung**: Metainfo ist da, Screenshots im Repo,
      Einreichung als PR gegen `flathub/flathub`
- [x] **Host-Betrieb ohne Flatpak** ebenfalls nativ: `crosspoint-launch.sh`
      startet `crosspoint` (`cargo build --release --features ui`)
- [ ] release-please-Kette einmal komplett durchlaufen lassen
      (Feature-Commit → Release-PR → Merge), bisher wird per Tag ausgelöst

## 6. Aufräumen

- [ ] `docs/04-abgleich-original.md` nach jedem Feature nachziehen
- [x] Brave-Reste des alten Fensterpfads entfernt: `crosspoint-launch.sh`
      startet das native Fenster, `crosspoint-browser-prefs.sh` und
      `tools/fenster_screenshot.py` (CDP gegen Brave) sind weg (2026-11-15)
- [x] Tray im nativen Fenster (`crosspoint --minimized`, seit 2026-11-21
      `crates/fenster/src/tray.rs`); der WebKit-Rückfall `crosspoint-fenster`
      ist entfernt

## Nicht geplant (bewusste Entscheidung)

- **Windows und macOS**: ausgeliefert wird nur für Linux (Flatpak, AppImage,
  tar.gz), siehe `docs/architektur.md`.

- **Eigenes VST-Hosting**: VST3-SDK, Plugin-Scan, GUI-Einbettung, Sandboxing —
  Wochen Arbeit. Carla macht das seit Jahren; wir verdrahten es.
- **Windows-VSTs nativ**: braucht Wine. Nur über Carla, nicht in Crosspoint.
- **1:1-Kopie der Original-Oberfläche**: Marken- und Designrisiko. Layout und
  Bedienlogik bleiben vertraut, Farben, Logo und Namen sind eigen.
