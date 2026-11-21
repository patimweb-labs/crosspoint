# 0005 – Echounterdrückung mit AEC3, Referenz vom Ausgabegerät

Stand: 2026-11-21 · entschieden · löst Teile von [ADR 0002](0002-rohsignal-und-ausgangsfilter.md) ab

## Kontext

ADR 0002 hat die Echounterdrückung an B1–B3 gehängt und als Referenz den fertigen
Ausgang eines Hardware-Busses genommen (`aec_ref`, ab Werk A1). Im Betrieb griff
das nicht: Discord, Browser und der Systemton spielen **an Crosspoint vorbei**
direkt auf das Ausgabegerät (HDMI-Monitor, USB-Boxen). Der A-Bus blieb still,
die Referenz war leer, und der Filter rechnete nichts heraus.

Dazu kam die Qualität. Der eigene Filter (`crates/core/src/aec.rs`, PBFDAF mit
zwei Filtern gegen Doppelsprechen, 256 ms) ist ein **linearer** Filter. Im
echten Raum – Monitorlautsprecher, USB-Mikro, getrennte Uhren von Ausgabe- und
Aufnahmegerät, DSP im Lautsprecher – blieben davon 1,5 dB übrig, mit
PipeWires `module-echo-cancel` (WebRTC AEC3) waren es rund 30 dB.

Wunsch des Nutzers: „Der gesamte Ausgabeton am PC soll per Knopfdruck aus dem
Mikro herausgefiltert werden können.“

## Messung

Alles offline, mit `crates/core/examples/echo_verfahren.rs` und
`echo_probe.rs`; der alte Filter stammt aus Commit `87ed2e9`. Nachgebauter, aber
realistischer Pfad: 47 ms Laufzeit, Nachhall RT60 0,3 s, Echo 20 dB unter der
Referenz, Rauschen im Mikrofon. Gemessen im eingeschwungenen Zustand (4–8 s).

| Echopfad | eigener Filter (PBFDAF) | sonora AEC3 | AEC3 + NS |
|---|---|---|---|
| linear | +28,6 dB | +37,3 dB | +49,6 dB |
| + Nachhall | +28,6 dB | +23,6 dB | +35,9 dB |
| + Taktversatz 50 ppm | **+2,4 dB** | +21,3 dB | +33,6 dB |
| + Begrenzung (kleiner Lautsprecher) | +2,4 dB | +21,1 dB | +33,4 dB |

Der Sprung liegt am **Taktversatz**: schon 50 ppm (getrennte Quarzuhren, im
echten Betrieb der Normalfall) verschieben das Echo um 2,4 ms pro Sekunde. Ein
fester, lernender Filter verliert damit seine Verzögerung; AEC3 schätzt sie
laufend nach. Das erklärt die 1,5 dB im Raum und die 30 dB von
`module-echo-cancel`.

Mit echten Aufnahmen (Referenz = Monitor des HDMI-Sinks, Mikro = Sennheiser)
waren beide Verfahren nicht zu trennen: die beiden Spuren sind nicht
sample-genau zueinander (die Laufzeit wandert zwischen 46 und 55 ms, die
Korrelation bleibt unter 0,2), es gibt dort also kein verwertbares Echo zu
messen. Die Zahlen oben sind deshalb die Entscheidungsgrundlage, die
Feldmessung (1,5 dB gegen 30 dB) die Bestätigung.

Weitere Messungen (alle mit `sonora` 0.3, 48 kHz, mono):

* **Latenz**: 19 ms von Mikrofon zu Ausgang (10 ms Sammler + 9 ms AEC3),
  unabhängig von Inhalt und Raum; schwankt nicht.
* **Kosten**: 0,4 % eines Kerns je Kanal (AEC3), der alte PBFDAF brauchte
  0,7 %.
* **Allokationen im Callback**: keine. 100 Paare `process_render_f32` +
  `process_capture_f32` ergeben 0 Allokationen, und
  `crates/core/tests/allokation.rs` ist mit Echounterdrückung an allen
  virtuellen Bussen grün.
* **Doppelsprechen**: mit einer echten Sprachaufnahme über einem Rausch-Echo
  kommt die Stimme beim Doppelsprechen in ihrer eigenen Lautstärke durch
  (±5 dB), während das Echo um 43 dB gesenkt wird.
* **Stille Referenz**: AEC3 verbiegt ein Mikrofon ohne Echo (Pegel gleich,
  Wellenform nicht). Deshalb schaltet der Echounterdrücker nach 200 ms stiller
  Referenz auf eine reine Verzögerung um – dann kommt das Mikrofon
  unverändert durch. Ohne diesen Weg wäre der Klang bei Stille schlechter als
  ohne Echounterdrückung.

## Entscheidung

1. **AEC3 statt eigenem Filter.** `crates/core/src/aec.rs` ist jetzt ein
   schmaler Adapter auf [`sonora`](https://crates.io/crates/sonora) (0.3,
   BSD-3, reines Rust) – dasselbe Verfahren wie WebRTC und
   `module-echo-cancel`. Der alte PBFDAF ist entfernt (Historie: Commit
   `87ed2e9`), nicht versteckt.
2. **Referenz vom Ausgabegerät.** Neue JACK-Eingänge `echo_ref_L/R`. Die
   gewählten Ausgabegeräte hängen mit ihrem Monitor daran
   (`<sink>:monitor_FL/FR` → `crosspoint:echo_ref_L/R`); mehrere Geräte
   gleichzeitig sind erlaubt, JACK summiert sie. Gespeichert wird die Auswahl in
   `echo.json` (`~/.config/crosspoint`), wiederhergestellt beim Start und vom
   Wächter nachgelegt, wenn ein Gerät zurückkommt (`crates/core/src/echo.rs`).
3. **`aec_ref` entfällt.** Alte Presets laden weiter, der Wert wird
   stillschweigend übergangen (`AusgangsFilter::set` nimmt ihn an und tut
   nichts).
4. **Menü statt Busauswahl.** Rechtsklick auf ECHO öffnet
   „ECHOUNTERDRÜCKUNG – Was spielt auf:“ mit einem Häkchen je Ausgabegerät
   (Mehrfachauswahl). Anzeige und Schreiben laufen über
   `logik::ausfuehren`, die Auswahl steht im Kern, nicht in der Darstellung.

## Alternativen

| Alternative | Warum nicht |
|---|---|
| Eigener linearer Filter (ADR 0002) | 2,4 dB bei getrennten Uhren – im Raum nicht hörbar als Verbesserung |
| `sonora` mit Rauschunterdrückung (AEC3+NS) | gewinnt in der Messung 12 dB, frisst aber am Mikrofonklang; ECHO soll herausrechnen, nicht aufräumen. Abschaltbar nachrüstbar (`noise_suppression`). |
| PipeWire `module-echo-cancel` benutzen | außerhalb des Modells, ein zusätzlicher Knoten je Gerät, Referenz und Schalter nicht im Mixer; genau das soll Crosspoint ersetzen |
| AEC3 selbst schreiben | Wochen Arbeit für ein Ergebnis, das schlechter wäre als eine gepflegte Übersetzung |

## Kosten

- **Neue Abhängigkeit**: `sonora` 0.3 (BSD-3) mit `sonora-aec3`,
  `sonora-ns`, `sonora-agc2`, `sonora-common-audio`, `sonora-fft`,
  `sonora-simd`. Reines Rust, keine Systembibliothek – Flatpak und AppImage
  brauchen nichts Zusätzliches. `packaging/flatpak/new-sources.json` muss nach
  `Cargo.lock` nachgezogen werden.
- **Latenz**: B1–B3 kommen 19 ms später als A1–A5. Für Discord, OBS und
  Aufnahmen unkritisch, für einen Monitor-Mix auf dem Kopfhörer hörbar.
- **Rahmenbindung**: AEC3 rechnet in 10-ms-Rahmen. Andere Abtastraten als
  48 kHz gehen (Rahmen = Rate/100), aber nur mit glattem Teiler; alles andere
  wird auf 48 kHz gerechnet.
- **Doppelsprechen**: AEC3 dämpft die eigene Stimme beim Doppelsprechen um
  einige dB (gemessen im Bereich der eigenen Lautstärke). Der alte Filterwar
  hier zurückhaltender, rechnete dafür das Echo nicht weg.
- Ein Ton, der an allen gewählten Ausgabegeräten vorbei geht (z. B. direkt auf
  einen Kopfhörer, der nicht in der Liste steht), ist nicht in der Referenz und
  wird nicht herausgerechnet.
