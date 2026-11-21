# Datenfluss

Was ein Bedienschritt auslöst, bis das Bild steht. Ein Prozess: Fenster (egui)
und Engine laufen im selben Binary (`crosspoint` mit Feature `ui`).

```mermaid
flowchart LR
    Maus[Maus/Tastatur] --> Ansicht[ansichten/*: render → Aktion]
    Ansicht --> Logik[logik::ausfuehren]
    Logik --> Params[(Params: Atome)]
    Params --> Audio[Audio-Callback: EngineDsp]
    Audio --> Pegel[(Pegelbank)]
    Pegel --> Takt[app.rs: pegel_ticken]
    Takt --> Modell[(Oberflaeche)]
    Params --> Ansicht
    Modell --> Ansicht
```

## „Fader ziehen"

1. **Ansicht** (`crates/fenster/src/bausteine.rs`, `fader`): der Zug liefert einen
   neuen Wert; `logik::fader_ziehen` rechnet Pixel in dB um und hält die
   0-dB-Raste (`fmt::einrasten`).
2. **Aktion** `Setzen { ziel, param: "gain", wert }` geht an
   `logik::ausfuehren`, das `Params::set` aufruft – das Atom
   `StripParam::gain` (`crates/core/src/engine.rs`), kein Puffer, kein JSON.
3. **Audio-Thread** (`EngineDsp::process`): liest das Atom im nächsten Block,
   Fader und Pan, dann die Routing-Matrix über `StripParam::sends`.
4. **Pegel** (`crates/core/src/pegel.rs`): der Callback schreibt Spitzenwerte je
   Eingang und Bus.
5. **Anzeige** (`app.rs`, 33-ms-Takt): `logik::pegel_ticken` glättet und hält
   die Spitze im Modell (`Oberflaeche::pegel`), die Ansicht zeichnet daraus.

## „FILTER-Modal öffnen“

1. Klick (oder Rechtsklick) auf **FILTER** am Zug B1 →
   `Aktion::Aufklappen(Some(Aufklapp::Filter(5)))`; B1 ist ein virtueller Bus,
   also gibt es einen `AusgangsFilter`.
2. `logik::ausfuehren` schreibt den Zustand ins Modell
   (`Oberflaeche::offen`), nicht in die Ansicht. Im nächsten Bild zeichnet
   `ansichten::filter::modal` das Modal: Titel `FILTER · B1 <Name>`, Pegel
   **ROH** (`BusParam::roh`, Summe vor den Filtern) und **NACHHER**
   (`BusParam::level`, nach der Kette), darunter je Filter eine Karte.
3. Jede Zeile im Modal ist eine Aktion: Schalter →
   `Setzen { ziel: Bus(5), param: "lc" | "aec" | "nr" | … }`, Regler →
   `Setzen { …, param: "lc_freq" | "comp_schwelle" | … }`, ECHO-Geräte →
   `Aktion::EchoGeraet { name, an }`. Geschlossen wird per X
   (`Aktion::Aufklappen(None)`), Esc oder Klick daneben
   (`Modal::should_close`).

## „ECHO an B1“

1. Im Modal die ECHO-Karte einschalten →
   `Setzen { ziel: Bus(5), param: "aec" }`.
2. `Params::set` → `AusgangsFilter::set` (nur virtuelle Busse haben einen
   `AusgangsFilter`; an A1–A5 wird `aec` abgelehnt und gemeldet).
3. Audio-Thread: `reconfigure` sieht den Wechsel auf „an“ und leert die Puffer
   der Echounterdrücker; im Sample-Takt nimmt `process` das Sample von
   `crosspoint:echo_ref_L/R` als Referenz (die Monitore der gewählten
   Ausgabegeräte, siehe Abschnitt „Echo-Referenz“) und rechnet B1 damit
   (`crates/core/src/aec.rs`, AEC3 aus `sonora`). Die Ausgabe kommt 19 ms
   später (10 ms Rahmen + 9 ms AEC3); ist die Referenz still, geht das Mikrofon
   unverändert und nur verzögert durch.

## „Die Filterkette an B1“ (ADR 0006)

Ein Block, ein Sample, ein Bus – in dieser Reihenfolge
(`EngineDsp::process`, Nummern im Code):

1. **Summe**: die Routing-Matrix der Streifen füllt `acc[bus]`, `bus_mix` legt
   Mono/Modus/Cross darüber. Der Spitzenwert hier ist `BusParam::roh`
   („ROH“ im Modal).
2. **LC**: `Biquad::hochpass`, Koeffizienten nur bei geänderter Frequenz
   (`reconfigure`).
3. **ECHO → NR**: eine `sonora`-Instanz, beide Schalter. `set_filter` wird nur
   bei Änderung aufgerufen – das baut den Rechner neu und allokiert; im
   eingeschwungenen Zustand allokiert `process` nichts.
4. **GATE**: gekoppelter Detektor `max(|L|, |R|)`, wie bisher.
5. **COMP**: derselbe Detektor, harte Kennlinie, Make-up.
6. **AGC**: `agc::Pegelregler` (AGC2 + Zielpegel-Trim), 10-ms-Rahmen-Sammler –
   nur wenn an.
7. **Limiter**: Decke, sofort zu, langsam auf.
8. **Pegel nach den Filtern** (`FilterDsp::pegel_jetzt`) – die Grundlage fürs
   Ducking im **nächsten** Block.
9. **DUCK**: jeder Bus, dessen `duck` an ist und dessen `duck_ziel` dieser Bus
   ist, senkt ihn um `duck_tiefe` dB (Attack/Release je Sample). Die
   Entscheidung „spreche ich?“ fällt mit dem Pegel des vorigen Blocks, weil
   der Zielbus in derselben Schleife schon geschrieben sein kann.
10. **PTT**: `ptt` an und `ptt_offen` aus → 0.
11. **Delay → Trim → Gain/Mute** wie bisher, danach der Pegel für die Anzeige.

## „Echo-Referenz wählen“

1. Im FILTER-Modal unter ECHO ein Häkchen setzen; die Liste kommt aus
   `geraete::Bestand::senken` (plus die gemerkten, die gerade fehlen).
2. Klick auf ein Häkchen → `Aktion::EchoGeraet { name, an }` →
   `logik::ausfuehren` bildet die neue **Liste** (die anderen Geräte bleiben) und
   ruft `echo::setzen` über den `Zuordner` (`crates/fenster/src/geraete.rs`).
3. `echo::setzen` löst alle Links an `crosspoint:echo_ref_L/R` und legt für jedes
   gewählte Gerät `<sink>:monitor_FL/FR` darauf (dieselben pw-Wege wie
   `devices.rs`/`inputs.rs`), dann schreibt es `echo.json`.
4. Beim Start macht das `echo::beim_start`; kommt ein Gerät zurück, legt es der
   Wächter (`wache::abgleich`) nach. Der Audio-Thread liest die Ports und rechnet
   sie als Referenz – kein Neustart, kein Modul.
   `GET/POST /api/echo` ist derselbe Weg von außen.

## „Master bedienen"

1. SEL am Master → Auswahl → `Aktion::MasterBus(i)` (Fenstereinstellung,
   `fenster.json`).
2. Master-Fader, Mute, Mono und FILTER (wenn der Bus virtuell ist) schreiben
   **auf den gewählten Bus** – der Master ist eine Fernbedienung, keine zweite
   Summe.

## „Aufnahme (Tape)"

1. REC → `Aktion::TapeStart` → `TapeControl::recording` (`crates/core/src/tape_io.rs`);
   die Uhr steht im Modell (`Tape::Aufnahme { seit }`).
2. Der Audio-Callback schreibt den Ring, der Tape-Schreiber die WAV-Datei.

## „Fenster zu" mit Tray

1. Der Fensterknopf schließt das Fenster (`run_native` kehrt zurück).
2. `betrieb::danach` wartet auf „Fenster zeigen" (Tray), `POST /api/fenster`
   (zweiter Start) oder „Beenden"/Engine-Ende; dann neues Fenster oder Ende.

## Web-Oberfläche

`crates/core/src/http.rs` liefert unter `127.0.0.1:8770` dieselben Parameter als
JSON (`/api/state`, `/api/set`, `/api/levels`) und `web/index.html`. Sie ist
**keine** zweite Wahrheit: beide Wege schreiben in dieselben Atome.
