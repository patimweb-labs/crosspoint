# 0012 – Kern und Fenster trennen (Vorschlag)

Stand: 2026-11-26 · entschieden · **Stufe 1 umgesetzt**

## Kontext

Das Fenster (`crates/fenster`) soll später auch über einem anderen Kern laufen:
ein Windows- oder macOS-Kern (WASAPI/CoreAudio statt PipeWire/JACK) unter
derselben egui-Oberfläche. Dafür muss das Fenster aufhören, die Linux-Arbeit
selbst zu machen.

Was heute schon gut ist: das Fenster schreibt **Zustand ausschließlich über
Atome** (`Arc<Params>`), liest Pegel aus einer `Pegelbank` und schickt Aktionen
durch `logik::ausfuehren`. Kein HTTP, kein JSON im Fenster (ADR 0003). Die
Gerätezuordnung läuft bereits über einen Trait (`geraete::Zuordner`).

Was noch klebt: **fünf Dateien** greifen direkt in Plattformmodule des Kerns.

| Datei | greift zu auf | wofür |
|---|---|---|
| `fenster/src/geraete.rs` | `pw::{PipeWire, Zugriff, Node}`, `devices`, `echo` | Geräteliste, Zuordnung, Echo-Auswahl |
| `fenster/src/betrieb.rs` | `lauf`, `http::fenster_gewuenscht`, `echo` | Start/Status der Engine, Fensterwunsch, Echo-Referenz |
| `fenster/src/tray.rs` | `lauf`, `autostart`, `restore` | Tray-Menü, Autostart, Sichern beim Beenden |
| `fenster/src/logik.rs` | `tape_io`, `vban_io`, `macros_io`, `osc`, `restore`, `autostart` | Bandsteuerung, VBAN, Makros, MIDIMonster-Karte, Presets |
| `fenster/src/vban.rs` | `vban_io` | VBAN-Dialog |

Alles andere (`ansichten/`, `bausteine`, `thema`, `state`, `modell`,
`konfig`) kennt nur das Modell.

Der Kern ist heute ein Crate (`crosspoint-core`) und mischt drei Dinge:
das **Modell** (`Params` und die Parameterstrukturen in `engine.rs`), die
**Linux-Backends** (PipeWire, JACK, VBAN, Tape, HTTP, OSC) und den **DSP**
(`dsp.rs`, `aec.rs`, `agc.rs`). Ein Windows-Kern könnte heute nicht einmal die
Parameterstrukturen mitbenutzen, ohne PipeWire mitzubauen.

## Vorschlag

Drei Schichten, in dieser Reihenfolge:

```
crates/modell        Params, StripParam/BusParam, AusgangsFilter, Namen,
                     FILTER_PARAMS, Pegelbank — nur std + serde_json.
                     Kein PipeWire, kein JACK, kein HTTP, keine Pfade.

crates/schnittstelle Traits, die das Fenster benutzt:
                       Geraete   (Liste, Zuordnen, Echo-Auswahl, Lautstärke)
                       Ablage    (Presets, Autostart, fenster.json, MIDI-Datei)
                       Betrieb   (Engine-Status, Fensterwunsch, Beenden)
                       Band, Netz, Makros (die vorhandenen Control-Handles)
                     `Kern` im Fenster hält `Box<dyn …>` statt konkreter Typen.

crates/core          Linux-Kern: implementiert die Traits über PipeWire/JACK,
                     behält den DSP. `src/` ist der Ort, an dem ein Kern
                     ausgewählt und eingehängt wird (Kompositionswurzel).
```

Damit hängt ein zweiter Kern nur noch an `modell` + `schnittstelle` und bringt
seinen eigenen DSP mit (oder denselben – `dsp.rs` ist plattformfrei und ließe
sich später ebenfalls herausziehen, wenn der Windows-Kern ihn braucht).

### Stufen

1. **`crates/modell` herausziehen.** ✅ *umgesetzt*: `Params`, die
   Parameterstrukturen, `bus_kind`, Namen, `FILTER_PARAMS`, `filter_get`,
   `to_json`, die Pegelbank (`pegel.rs`), die JSON-Wertform (`json.rs`) und die
   dB-Umrechnung. `engine.rs` und `lib.rs` re-exportieren (`pub use`), damit
   Fenster, OSC, HTTP und Tests unverändert bauen. Kein Verhalten hat sich
   geändert. Die CI hat dafür einen eigenen Job (`Schichten`): `crosspoint-modell`
   darf an keiner Plattform-Bibliothek hängen.
   **Nebenbei entfernt:** die toten Typen des Referenzprogramms
   (`FilterType`, `EqCell`, `Compressor`, `Gate`, `Denoiser`, `Pitch`, `Strip`,
   `Fader`, `Mixer`, `EngineConfig`, `EQ_CELLS`, `LAYERS`) – sie waren seit
   ADR 0002 nirgends mehr in Gebrauch (0 Treffer außerhalb ihrer Definition).
2. **Traits für Geräte und Ablage.** `geraete.rs` (PipeWire!) und `tray.rs`
   wandern als Implementierung in den Kern; das Fenster bekommt `dyn Geraete`
   und `dyn Ablage`. Das ist der größte Brocken, weil die Geräteliste heute
   mit `pw::Node` arbeitet.
3. **Band, Netz, Makros, Betrieb.** Die Control-Handles sind schon gekapselt
   (`Arc<TapeControl>`, `Arc<VbanControl>` …); hier reicht ein Trait je
   Bereich und ein `Betrieb` für `lauf`/`fenster_gewuenscht`.
4. *(später)* `dsp.rs` aus dem Kern lösen, falls der Windows-Kern denselben DSP
   verwenden soll statt eines eigenen.

Nach Stufe 2 darf `crates/fenster` **kein** `pipewire`, `jack` oder
`crosspoint::pw` mehr im Abhängigkeitsbaum haben. Das lässt sich in der CI
prüfen (`cargo tree -p crosspoint-fenster`).

## Alternativen

| Alternative | Warum nicht |
|---|---|
| Fenster redet mit dem Kern nur über HTTP/JSON | ein Umweg über Text für Dinge, die im selben Prozess liegen (Pegel bei 30 Hz, Fader); Latenz und Serialisierung ohne Nutzen |
| Prozessgrenze: Kern als eigener Dienst, Fenster als Client | sauberste Trennung, aber IPC, Fehlerfälle und Startreihenfolge; für Windows/macOS derselbe Aufwand wie Traits, nur später |
| Nur ein Trait für alles | ein 30-Methoden-Interface, das jeder neue Kern vollständig füllen muss, auch wenn er kein VBAN hat |
| Modell weiter im Kern lassen | der Windows-Kern müsste PipeWire mitbauen, um `Params` zu kennen – genau das soll weg |

## Kosten

- Zwei neue Crates, mehr `Cargo.toml`-Pflege.
- Stufe 1 ist mechanisch, aber ein großer Diff (≈ 900 Zeilen verschoben).
- Stufe 2 ändert die Geräteansicht: was heute eine `pw::Node`-Liste ist, muss
  als plattformneutraler `Eintrag` (gibt es schon in `geraete.rs`) durch den
  Trait gehen.
- Für das Fenster ändert sich nichts Sichtbares; Oberflächentests bleiben
  unverändert gültig (sie bauen ihr Modell selbst, ohne Kern).
