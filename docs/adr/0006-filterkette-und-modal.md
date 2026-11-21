# 0006 – Filterkette an B1–B3 und ein Modal statt acht Knöpfe

Stand: 2026-11-21 · entschieden · erweitert [ADR 0002](0002-rohsignal-und-ausgangsfilter.md)
und [ADR 0005](0005-echounterdrueckung-mit-aec3.md)

## Kontext

An den virtuellen Bussen B1–B3 standen bisher zwei Schalter: **ECHO**
(Echounterdrückung, AEC3 aus `sonora`) und **GATE** (Noise Gate) – plus die
Rauschunterdrückung als Häkchen „Restecho mit weg rechnen“ im ECHO-Menü.
Für den Alltag mit Spiel, Discord und OBS reicht das nicht:

* **Lüfter, Tastatur, Maus** will man loswerden, ohne die Echounterdrückung
  einzuschalten – die NS hing aber an ECHO.
* **Laute und leise Stellen** brauchen Kompressor, Limiter und eine
  automatische Pegelanpassung; ein Gate schneidet nur Pausen.
* **Rumpeln und Trittschall** unter 80 Hz sollen vor allem anderen weg.
* **Ducking**: spricht man, soll der Spielton leiser werden.
* **Push-to-Talk**: ein Fusstaster soll den Bus stumm halten, bis man tritt.

Wunsch des Nutzers: „Wir brauchen eine andere Variante als die Knöpfe,
vielleicht ein Modal. Bau das alles noch ein, ohne den Rest kaputt zu machen.“

## Entscheidung

### 1. Die Kette je virtuellem Bus

```
Summe → Mono/Modus → LC → ECHO → NR → GATE → COMP → AGC → Limiter
      → DUCK → PTT → Delay → Trim → Gain/Mute → Ausgang
```

Begründung der Reihenfolge:

| Stufe | Warum **hier** |
|---|---|
| **LC** (Hochpass 2. Ordnung, 40–200 Hz, ab Werk 80 Hz) | Vor AEC3: was der Filter wegnimmt (Rumpeln, Trittschall, Lüfterbrummen), muss die Echounterdrückung nicht mehr modellieren, und der Filter kann den Echo-Pfad nicht verstimmen. Ein Hochpass hinter AEC3 würde den gelernten Filter nicht stören, aber die Echo-Schätzung mit tieffrequentem Müll füttern. |
| **ECHO** (AEC3) | Braucht das trockene Signal; alles, was danach kommt, würde die Referenz verfälschen (der Filter säße im Regelkreis). |
| **NR** (Rauschunterdrückung, Stufe 0–3) | Hinter AEC3, weil sie sonst den Systemton als „Rauschen“ wegrechnet und die Echounterdrückung nichts mehr zu tun hätte. Eigener Schalter, nicht mehr an ECHO gekoppelt: `sonora` rechnet NS auch ohne AEC (`konfiguration(false, true, …)`), dann ohne Referenz. |
| **GATE** | Am echofreien, rauschreduzierten Signal entscheidet es über Pausen; davor würde der Systemton das Gate offen halten. |
| **COMP** | Hinter dem Gate: es soll Sprache verdichten, nicht Pausen. Ein Kompressor vor dem Gate würde dessen Schwelle mit dem Material mitziehen. |
| **AGC** (AGC2) | Hinter dem Kompressor: der Regler soll den Pegel glätten, den der Kompressor schon zusammengezogen hat, nicht gegen ihn arbeiten. |
| **Limiter** (Decke ab Werk −1 dBFS) | Ganz am Ende der Filter, **vor** Delay/Trim/Gain: der Fader darf den Limiter nicht aushebeln (ein Faderzug nach dem Limiter wäre lauter als die Decke). |
| **DUCK** | Reine Pegelsenkung hinter dem Limiter: sie nimmt nur weg, der Limiter soll nicht gegen eine Absenkung regeln. |
| **PTT** | Letzte Stufe vor Delay/Trim/Gain: stumm ist stumm. |

Reihenfolge im Code: `EngineDsp::process` in `crates/core/src/engine.rs`,
Nummern (1)–(8) im Kommentar.

### 2. Was neu gerechnet wird – und wo

| Filter | Herkunft | Ort |
|---|---|---|
| LC | eigener Biquad (RBJ, Q = 0,707) | `dsp::Biquad`, `Biquad::hochpass` |
| NR | `sonora`-NS, Stufe 0–3 (`Low … VeryHigh`, 6/12/18/21 dB) | `aec::Echounterdruecker::set_filter` |
| GATE | wie bisher | `dsp::Gate` |
| COMP + Limiter | eigene Implementierung, gekoppelter Stereo-Detektor wie beim Gate | `dsp::Kompressor`, `dsp::Limiter` |
| AGC | AGC2 aus `sonora` + eigener Zielpegel-Trim | `crates/core/src/agc.rs` |
| DUCK | eigener Ducker | `dsp::Ducker` |

**AGC2 und der Zielpegel.** AGC2 regelt die *Sprachlautstärke* gegen seine
eigene Vorgabe (Headroom des Sättigungsschutzes) und kennt keinen Zielpegel.
Der Zielpegel (`agc_ziel`, ab Werk −12 dBFS) ist deshalb eine eigene, langsame
Regelung dahinter: eine Spitzenhüllkurve misst den Reglerausgang, ein Trim
zieht die Spitzen auf den Zielwert (500 ms Zeitkonstante, Bereich ±30 dB).
Gemessen mit nachgeahmter Sprache (300 ms Ton, 200 ms Pause, 48 kHz):

| Eingang | Spitzen am Ausgang (Ziel −12 dBFS) |
|---|---|
| −39 dBFS | −10,9 dBFS |
| −11 dBFS | −10,9 dBFS |

Beide Eingänge landen also am selben Ziel (1,1 dB darüber – die Hüllkurve liegt
einen Hauch unter der Spitze). Ein reiner Dauerton wird von AGC2 **nicht** als
Sprache erkannt und bleibt, wo er ist; das ist WebRTC-Verhalten, nicht unsere
Wahl.

**Ducking, ein Block Verzögerung.** Der Pegel, der über die Absenkung
entscheidet, ist der Pegel des Quellbusses **nach den Filtern** aus dem
*vorigen* Block (`FilterDsp::pegel_alt`). Grund: die Busse laufen in einer
Schleife 0..8; liegt der Zielbus vor dem Quellbus, wäre sein Signal für diesen
Block schon geschrieben. Gemessen bei 512 Frames: eine Blocklänge = 10,7 ms
Verzögerung, mit Attack 5 ms/Release 100 ms unhörbar. Der erste Block nach dem
Sprechbeginn bleibt offen (Test `ducking_senkt_den_zielbus_im_naechsten_block`).

**PTT** ist kein Filter, sondern ein Pegel: ist `ptt` an und `ptt_offen` aus,
wird der Bus auf 0 gesetzt. Steuerbar über HTTP/OSC (`/bus/<n>/ptt`,
`/bus/<n>/ptt_offen`) und über die MIDI-Karte (Seite „Busse“, Fusstaster auf
`/bus/<n>/ptt_offen`).

### 3. Kompatibilität

* `aec_ns` (bis ADR 0005 der Restecho-Schalter an ECHO) wird weiter
  angenommen und schaltet **NR** – alte Presets laden, `filter_get("aec_ns")`
  liefert den Stand von `nr`, und `to_json` schreibt beide Namen.
* `aec_ref` bleibt der stille No-op aus ADR 0005.
* Neue Parameter: `lc`, `lc_freq`, `nr`, `nr_stufe`, `comp`, `comp_schwelle`,
  `comp_ratio`, `comp_attack`, `comp_release`, `comp_makeup`, `limiter`,
  `limiter_decke`, `agc`, `agc_ziel`, `duck`, `duck_ziel`, `duck_tiefe`,
  `duck_schwelle`, `duck_attack`, `duck_release`, `ptt`, `ptt_offen`.
  Alle stehen in `FILTER_PARAMS` und damit in Preset, `/api/state` und OSC;
  an A1–A5 lehnt `Params::set` sie ab wie bisher `gate`/`aec`.
* Neuer Messwert `BusParam::roh` (Summe vor den Filtern) für die Anzeige
  „ROH“ im Modal – kein Parameter, kein Preset-Feld.

### 4. Oberfläche: FILTER-Knopf + Modal

Acht Knöpfe passen nicht in eine Zugbreite (rund 95 px für einen Bus), und
Fehlbedienung fällt in einer Reihe Lämpchen schneller auf als in acht Popups.
Deshalb:

* Ein Knopf **FILTER** (Icon `sliders-horizontal`) ersetzt GATE und ECHO;
  darunter vier Zeilen Kürzel (`LC ECHO / NR GATE / COMP AGC / DUCK PTT`),
  an in `accent_default`, aus in `text_faint` – nur Anzeige, kein Klickziel.
* Links- wie Rechtsklick öffnen dasselbe Modal: `egui::Modal` (zentriert,
  abgedunkelter Hintergrund, Esc/Klick daneben/X schließen), Titel
  `FILTER · B1` – mit eigenem Namen `FILTER · B1 · <Name>`, ohne Dopplung.
  Das Modal steht im gemeinsamen Dialograhmen (`bausteine::dialog_rahmen`,
  ADR 0009): Titel links, Schließen-X rechts in der Titelleiste.
* Unter dem Titel steht der Pegel **ROH** (Summe vor den Filtern) und
  **NACHHER** (Bus-Pegel) als eigene Zeile über die volle Innenbreite, je eine
  Hälfte mit Beschriftung, Balken und Wert.
* Darunter ein **Raster**: die acht Karten in der Reihenfolge der Kette, links
  nach rechts. Die Spaltenzahl folgt aus der Breite (drei im kleinsten
  Fenster, vier bei breiterem; reicht die Höhe nicht, kommt eine Spalte dazu).
  Alle Spalten sind gleich breit und füllen die Innenbreite, die Karten einer
  Zeile sind gleich hoch (die höchste der Zeile). In jeder Karte: Kopf mit
  Kürzel, Titel und An/Aus-Schalter (rechts, überall gleich groß), darunter
  die Kurzbeschreibung und die Regler. Beschriftung und Wert stehen in festen
  Spalten, der Schieber nimmt den Rest – so stehen alle Zeilen bündig.
* Die ECHO-Auswahl bietet nur **lokale** Ausgabegeräte an: keine eigenen
  `crosspoint_*`-Sinks (die Schleife Bus → Referenz → Bus) und keine
  Netzwerk-/AirPlay-Ziele (was dort hinausgeht, spielt in einem anderen Raum).
  Lange Namen stehen einzeilig gekürzt da, der volle Name im Tooltip.
* Der Zustand steht im Modell (`Oberflaeche::offen = Some(Aufklapp::Filter(bus))`),
  jede Bedienung ist eine `Aktion` durch `logik::ausfuehren` (AGENTS.md §2).
  Die alten Popups (`Aufklapp::Gate`, `Aufklapp::Echo`) entfallen.
* Die ECHO-Auswahl der Ausgabegeräte wandert in die ECHO-Karte des Modals
  (Häkchen je Gerät, unverändert `Aktion::EchoGeraet`).

## Messungen (Tests, 48 kHz, Debug-Build)

| Prüfung | Ergebnis |
|---|---|
| LC, 80 Hz Grenze, 30 Hz | −17,4 dB (Test fordert < −12 dB) |
| LC, 80 Hz Grenze, 1 kHz | ±0,00 dB |
| COMP: −6 dBFS bei Schwelle −20 dBFS, Ratio 4:1 | −16,3 dBFS (Soll −16,5) |
| Limiter, Decke −1 dBFS, Eingang 1,5× Vollaussteuerung | Spitze ≤ −1 dBFS, alle Samples unter der Decke |
| DUCK, Tiefe 12 dB, Quellbus −6 dBFS | Zielbus −18,0 dBFS |
| AGC, Ziel −12 dBFS | Spitzen beider Eingänge bei −10,9 dBFS |
| Rohsignal A1 neben B1 | unverändert (0,5 → 0,5) |
| Allokationen im Callback mit der ganzen Kette | 0 (`crates/core/tests/allokation.rs`) |

Latenz: ECHO 19 ms (10 ms Sammler + 9 ms AEC3) und AGC 10 ms (eigener
Rahmen-Sammler), zusammen **29 ms**, wenn beide an sind. AGC aus = keine
Verzögerung, kein Rechenschritt. NR allein kostet keinen Aufschlag (derselbe
Sammler wie ECHO).

## Alternativen

| Alternative | Warum nicht |
|---|---|
| Acht Knöpfe am Zug | passen nicht in 95 px, und der Stand ist nicht auf einen Blick zu lesen; ein Modal zeigt alle Werte mit Namen und Beschreibung |
| Popup je Filter (wie GATE/ECHO bisher) | acht Popups, jedes schließt das andere; kein gemeinsamer Blick auf die Kette |
| AGC selbst schreiben | AGC2 aus `sonora` ist erprobt (WebRTC), kostet nichts extra – nur der Zielpegel fehlt und ist als Trim ergänzt |
| `sonora`-AGC2 um den Zielpegel willen patchen | fremde Crate forken für einen Trim von 20 Zeilen |
| Ducking im selben Block lösen | bräuchte eine zweite Schleife über alle Busse (Summe → Filter → Absenkung) und damit den doppelten Rechenweg; 10,7 ms Verzögerung sind billiger |
| NR weiter an ECHO koppeln | widerspricht dem Zweck: Lüfter weg, Echo nicht |
| Globaler Hotkey für PTT | unter Wayland ohne Portal nicht sauber möglich (siehe `docs/TODO.md`) |

## Kosten

* Drei weitere Filter-Stufen je Bus und Kanal; gemessen kostet die ganze Kette
  im Callback nichts an Allokation, die Rechenlast ist im Vergleich zu AEC3
  klein (Biquad, zwei Hüllkurven, ein Ducker).
* AGC bringt 10 ms Latenz, wenn eingeschaltet; das ist der Preis für einen
  Rahmen-Sammler wie bei AEC3.
* Ein Filter-Ausfall durch falsche Bedienung ist wahrscheinlicher geworden:
  acht Schalter statt zwei. Dagegen stehen die Lämpchen (man sieht, was an ist)
  und die Kurzbeschreibung je Karte.
* Das Modal braucht Platz: im kleinsten Fenster (1642 × 660) nimmt es fast die
  ganze Höhe ein; mehr Geräte als in die ECHO-Karte passen, stehen dort nur
  gekürzt (siehe `docs/TODO.md`).
* Das Raster rechnet mit festen Zeilenhöhen (Kopf, Beschreibung, Reglerzeile).
  Wer die Beschreibungen verlängert, verschiebt die Zeilenhöhen mit – die
  Tests halten fest, dass alles im kleinsten Fenster bleibt.
