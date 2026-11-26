# 0011 – Die Filterkette auch am Kanalzug

Stand: 2026-11-26 · entschieden · erweitert [ADR 0006](0006-filterkette-und-modal.md),
löst Punkt 2 aus [ADR 0002](0002-rohsignal-und-ausgangsfilter.md) teilweise ab

## Kontext

Seit ADR 0002 tragen die Eingänge das Rohsignal, und die Filterkette aus ADR 0006
sitzt ausschließlich an den virtuellen Bussen B1–B3. Damit gab es genau einen
bearbeiteten Punkt je Kanal: hinter der Bus-Summe. Wer sein Mikrofon schon am
Eingang aufräumen wollte – Lüfter weg, Trittschall weg, Pegel bändigen –, musste
das über einen Bus tun und den Kanal dorthin routen; die Bearbeitung landete also
automatisch auf dem Weg zu Discord oder OBS, nicht auf A1.

Wunsch des Nutzers: „die Filter direkt an den HW In 1 hängen zu können statt nur
an den virtuellen Ausgang."

## Entscheidung

1. **Jeder Kanalzug hat eine eigene Filterkette.** Sie ist dieselbe wie an den
   Bussen (`AusgangsFilter` in `crates/core/src/engine.rs`): LC, ECHO, NR, GATE,
   COMP, AGC, LIMITER, DUCK, PTT – dieselben Parameter, dasselbe Modal. Kein
   zweiter Satz Regler, keine zweite Bedeutung.
2. **Ort in der Signalkette:**

   ```
   Eingang → Mono → FILTERKETTE → Fader/Pan → Sends/Routing
   ```

   Die Kette sitzt **vor** dem Fader, aus demselben Grund wie am Bus: der Fader
   darf die Limiter-Decke nicht aushebeln. Und sie sitzt vor dem Routing, weil
   sie den Kanal bearbeitet und nicht einen seiner Wege – das ist der Unterschied
   zu ADR 0002, der hier bewusst aufgegeben wird.
3. **Das Rohsignal bleibt die Vorgabe.** Kein Filter ist von sich aus an; eine
   Kette ohne eingeschalteten Schalter klingt wie vorher (Test
   `kette_am_kanal_laesst_ohne_schalter_alles_durch`). A1–A5 behalten keine
   Filter (ADR 0002, Punkt 2 gilt dort weiter).
4. **Ducking darf jetzt auch von einem Kanal kommen** (`duck_ziel` = Bus): „wenn
   das Mikrofon spricht, wird B1 leiser" ist damit ein Sidechain, ohne Extra-Weg.
   Die Hüllkurve und die eine Blocklänge Verzögerung sind dieselben wie in
   ADR 0006.
5. **Die Echounterdrückung am Kanal** nimmt dieselbe Referenz wie am Bus
   (`echo_ref_L/R`, ADR 0005) – den Ton der gewählten Ausgabegeräte.
6. **Oberfläche:** Der FILTER-Knopf steht im Kanalzug bei M/S und MONO, mit
   derselben Lämpchenreihe; das Modal heißt „FILTER · HW In 1". Sein
   zugänglicher Name nennt den Zug („FILTER HW In 1"), sonst wären es elf
   gleichnamige Knöpfe. Die Mindesthöhe des Fensters folgt mit
   (`logik::pult_mindesthoehe` kennt jetzt den Kanal mit Lämpchenzeile).
7. **Presets** bekommen am Streifen den Schlüssel `filter`; alte Presets ohne ihn
   laden unverändert, die Kette bleibt aus. `aec_ref` bleibt ein stillschweigend
   übergehender alter Name.

## Alternativen

| Alternative | Warum nicht |
|---|---|
| Nur die halbe Kette am Kanal (LC, GATE, COMP) | zwei Sätze Regler mit zwei Bedeutungen; das Modal müsste je Ziel anders aussehen |
| Filter hinter dem Fader | der Fader könnte die Decke des Limiters überschreiten – dieselbe Begründung wie ADR 0006 |
| Zweiter Kanalzug nur für bearbeitete Signale | doppelte Züge, doppeltes Routing, kein Platz im Pult (ADR 0007) |
| Filter an den Hardware-Bussen A1–A5 freigeben | widerspricht dem Ziel „irgendwo das Rohsignal" (ADR 0002) |

## Kosten

- Acht weitere Filterblöcke im DSP. Sie werden **beim Start** gebaut, nicht im
  Callback (AEC3 und AGC2 allokieren beim Bauen), auch wenn nichts eingeschaltet
  ist. Gemessen (`EngineDsp::new`, Release, Fedora 45): Bau 0,7 ms → 2,1 ms,
  Prozess-RSS 5,0 MB → 8,0 MB.
- Die Kette rechnet je Kanal und Sample mit – die Stufen selbst sind billig, die
  Echounterdrückung nur dann, wenn sie an ist.
- Das Pult braucht eine Zeile mehr (Lämpchen am FILTER-Knopf im Kanalzug): die
  Mindesthöhe des Fensters steigt entsprechend.
- `web/index.html` (die Fernbedienung) zeigt am Kanalzug weiterhin die alten
  Felder EQ/COMP/GATE/NR; die Kette am Kanal ist dort noch nicht bedienbar
  (`docs/TODO.md`).
