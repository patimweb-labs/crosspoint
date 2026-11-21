# 0002 – Rohsignal an den Eingängen, Filter nur an den virtuellen Ausgängen

Stand: 2026-11-21 · entschieden

## Kontext

Jeder Kanalzug hatte eine eigene Bearbeitungskette (Lowcut, TK, Gate, NR, EQ,
Kompressor, Denoiser), dazu Effektwege und Inserts; die Hardware-Busse EQ,
Limiter und C5. Wer das unbearbeitete Mikrofon brauchte (Kopfhörer,
Aufnahme), bekam es nicht mehr: die Kette saß vor dem Routing. Mit Boxen statt
Kopfhörer kam außerdem der Systemton über das Mikrofon zurück in Discord und
OBS.

Wunsch des Nutzers: „die ganzen Filter rausnehmen … nur an virtuellen
Ausgängen haben, so dass man das Raw-Signal irgendwo hat. Anfangs nur das
Noise Gate und einen Filter, der den Systemsound rausrechnet."

## Entscheidung

1. **Eingänge und A1–A5 tragen das Rohsignal.** Kanalzug: Mono, Fader, Pan,
   Routing. Hardware-Bus: Mono/Modus, Delay, Trim, Gain/Mute.
2. **B1–B3 haben einen `AusgangsFilter`** (`Option` im `BusParam`, nur virtuelle
   Busse): Echounterdrückung, dann Noise Gate. `Params::set` lehnt `gate`/`aec`
   an A1–A5 ab.
3. **Echounterdrückung in Rust** (`crates/core/src/aec.rs`): partitionierter
   adaptiver Filter im Frequenzbereich (Block 128, FFT 256, 96 Partitionen =
   256 ms), NLMS je Bin, zwei Filter (Hintergrund lernt, Vordergrund gibt aus)
   gegen Doppelsprechen. Allokationsfrei im Audio-Thread, Tests mit Raummodell:
   40 dB Echodämpfung, Stimme beim Doppelsprechen 28 dB über dem Rest,
   Neulernen nach Raumwechsel.
4. **Referenz ist der fertige Ausgang eines Hardware-Busses** (`aec_ref`, ab
   Werk A1): genau der Ton, den die Boxen spielen. Darum rechnet der Callback
   A1–A5 vor B1–B3.
5. Die Effektwege aus ADR 0001 (Hall/Echo, FX1/FX2, LV2-Inserts über mod-host, Programme,
   `vaio`) sind entfernt; sie stehen in der Historie (Commits vor diesem Umbau).

## Alternativen

| Alternative | Warum nicht |
|---|---|
| WebRTC AEC3 (C++) | beste Qualität, aber eine C++-Bibliothek in Flatpak und AppImage; erst wenn die eigene Umsetzung nicht reicht |
| PipeWire `module-echo-cancel` vor dem Mixer | außerhalb des Modells, Referenz wäre der Sink statt des Busses, schwer zu steuern |
| Filter je Bus wählbar (auch A1–A5) | widerspricht dem Ziel „irgendwo das Rohsignal“ |
| Kanalfilter behalten und abschaltbar machen | doppelte Wege, das Rohsignal hinge an einem Schalter |

## Kosten

- Alte Presets mit Kanalfiltern laden weiter, ihre Filterwerte werden übergangen.
- Echounterdrückung kostet ≈ 0,7 % eines Kerns je Kanal und 2,7 ms Latenz;
  ohne Restecho-Suppressor bleiben Verzerrungen kleiner Boxen leise hörbar
  (offen in `docs/TODO.md`).
- Ein Ton, der an Crosspoint vorbei auf die Boxen geht, ist nicht in der
  Referenz und wird nicht herausgerechnet.
