# 0010 – macOS-Unterbau: zwei Backend-Grenzen, Core Audio dahinter

Stand: 2026-11-21 · **vorgeschlagen** · Plan: [../plan-macos.md](../plan-macos.md)
· berührt [ADR 0004](0004-pakete-flatpak-appimage.md) (Pakete) und
[ADR 0005](0005-echounterdrueckung-mit-aec3.md) (Echo-Referenz)

## Kontext

Wunsch des Nutzers, wörtlich: „Cool wäre dann noch eine Mac-Version, wenn das
geht, aber plane da erstmal den Unterbau – Mac nutzt ja nicht PipeWire."

`docs/architektur.md` führte macOS bis hierher als „nicht geplant, aber machbar"
mit 170–240 h. Die Begründung war dünn an drei Stellen:

1. Sie begann beim Core-Audio-Backend, ohne zu sagen, **wo** es im Code
   andocken soll. Heute dockt es nirgends an: `pw.rs` (2 342 Zeilen) und
   `lauf.rs` (879) sprechen PipeWire und JACK direkt, `crates/fenster`
   importiert `pw::{Graph, PipeWire}` in `geraete.rs`.
2. Sie nannte „Audio Taps ab macOS 13". Die Taps kamen mit **14.2**
   (`AudioHardwareCreateProcessTap`).
3. Sie rechnete den Ersatz für `echo.rs` nicht mit. ADR 0005 hängt die
   Echounterdrückung an die Monitor-Ports der Ausgabegeräte – ein
   PipeWire-Modell, das es auf macOS nicht gibt.

Zugleich ist die Lage günstig: der Kern ist bereits datenorientiert.
`EngineDsp::process(&Params, &Io, frames)` bekommt 18 rohe Pufferzeiger und
weiß nichts von einem Audio-Server. Ein zweites Backend braucht keine zweite
Engine.

## Entscheidung

1. **Zwei Grenzen, nicht eine.** Im Kern entstehen zwei Traits:
   - `AudioBackend` – ein Callback, 18 Puffer je Block, Abtastrate, Blockgröße.
     Linux-Umsetzung = der heutige JACK-Code, unverändert.
   - `GeraeteBackend` – welche Geräte es gibt, welcher Bus wohin gehört, woher
     die Echo-Referenz kommt. Linux-Umsetzung = `pw.rs`, verengt auf
     `Vec<Geraet>` statt `node:port`.
   Das vorhandene `Zugriff`-Trait (`pw.rs:418`) ist der Keim dafür: es wandert
   nach `core/src/backend/geraete.rs` und wird von PipeWire-Sprech auf
   Geräte-Sprech gehoben.
2. **Die Auswahl passiert in `src/`**, der Kompositionswurzel. `crates/fenster`
   bekommt kein `cfg(target_os)` (AGENTS.md §2.10), sondern eine `Umgebung` als
   Daten – `Plattform`, `tray`, `autostart`, `virtuelle_geraete`, `taps`. Die
   Ansichten rendern daraus. `cfg` bleibt nur in `Cargo.toml`, wo Cargo es
   erzwingt.
3. **macOS-Audio über Core Audio (HAL-API)**, gebunden mit `objc2-core-audio`
   (0.3) statt `cpal`. Begründung: cpal verbirgt Aggregat-Geräte und den
   IOProc, den wir für 18 Kanäle und einen Tap brauchen. Preis: das
   Callback-Modell ändert sich – auf macOS ein IOProc **je Gerät**, nicht ein
   Client mit 18 Ports. Der DSP wird deshalb von einem Mix-Thread gefüttert,
   der die Ringpuffer der Geräte sammelt. Zwei Puffer mehr Latenz als unter
   JACK; das steht als Zahl im Modell, nicht als Versprechen im README.
4. **Virtuelle Geräte: eigenes Audio-Server-Plugin**, kein Kernel-Treiber.
   Ein userspace-Bundle in `/Library/Audio/Plug-Ins/HAL`, gebaut mit `libASPL`
   (C++17, MIT), das mit der App über Shared Memory und zwei lock-freie
   Ringpuffer spricht. Kein `kext`, kein Dext, keine „reduzierte Sicherheit".
   **Zwischenschritt BlackHole** (fremder Treiber, vom Nutzer installiert) ist
   ausdrücklich erlaubt und wird vor dem eigenen Plugin gebaut – er beweist den
   Rest der App, bevor ein Fehler im `coreaudiod` den Ton eines ganzen Macs
   mitnimmt.
5. **Echo-Referenz und Eingänge über Core-Audio-Taps** (macOS 14.2+), nicht
   über Monitor-Geräte. Ein globaler Stereo-Tap ersetzt die Monitor-Suche aus
   `echo.rs`; Prozess-Taps ersetzen `pw-link` für „Programm X auf Streifen 3".
   Damit ist die Echo-Referenz auf macOS **näher an ADR 0005** als unter
   PipeWire, wo ein Gerät einen Monitor haben muss.
6. **Untergrenze macOS 14.2**, weil darunter keine Taps existieren.
7. **Pakete: `.app` + `.dmg`, Developer ID signiert und notarisiert**
   (ADR 0004 gilt weiter für Linux; macOS bekommt keinen vierten Linux-Weg).
   Universal Binary, CI-Job auf `macos-14`. Kosten: 99 €/Jahr
   Developer Program.
8. **Reihenfolge in sechs Schritten**, jeder einzeln lauffähig:
   (a) Grenzen einziehen, Linux grün · (b) Core-Audio-Backend ohne virtuelle
   Geräte · (c) BlackHole · (d) eigenes HAL-Plugin · (e) Taps · (f) Pakete.
   Nach (b) ist entscheidbar, ob der Rest das Geld wert ist. Summe
   **180–286 h**, ohne eigenes Plugin 120–186 h.

## Alternativen

| Alternative | Warum nicht |
|---|---|
| Kein macOS, wie bisher | Der Nutzer hat gefragt; die Grenze (a) ist auch ohne macOS nützlich (Tests ohne PipeWire, Windows später) |
| Portierung beginnen, wo `architektur.md` sie begann – beim Audio-Backend, ohne Traits | dann steht macOS-Code neben PipeWire-Code statt dahinter; zwei Wahrheiten im selben Modul |
| `cpal` als Audio-Schicht | bequem für „ein Stream je Gerät", verdeckt Aggregate und IOProc; unsere 18 Kanäle plus Tap sind kein cpal-Anwendungsfall |
| PipeWire-Emulation auf macOS (eigener Server) | 300 h+ und ein zweites Betriebssystem im Betriebssystem |
| AudioDriverKit statt HAL-Plugin | braucht Apples Freigabe für eine System Extension; Aufwand ~150 h+ für dasselbe Ergebnis |
| Nur BlackHole, kein eigenes Plugin | billiger (12–20 h), aber keine B1–B3 einzeln; die Gerätenamen weichen von Linux ab, Handbuch und Screenshots gelten nur halb |
| Nur Taps, kein virtuelles Gerät | Taps nehmen auf, sie leiten nicht um – „Programm X auf Kanal 3" bliebe unmöglich |
| Windows gleich mitplanen | eigenes Thema (signierter Kernel-Treiber, EV-Zertifikat 300–700 €/Jahr); die Grenze (a) macht es später leichter, aber dieser ADR entscheidet nichts darüber |

## Kosten

- **Arbeit:** 180–286 h nach [../plan-macos.md](../plan-macos.md) §5, davon
  24–40 h für (a) – und die fallen **auch ohne macOS** an, weil sie die
  Testbarkeit ohne PipeWire verbessern.
- **Geld:** 99 €/Jahr Developer Program; Testhardware (Mac mini M4 und
  USB-Interface, ~700 €) oder geliehen; macOS-CI-Minuten zählen 10-fach.
- **Risiko:** Das HAL-Plugin läuft im `coreaudiod`. Ein Absturz dort nimmt den
  Ton des ganzen Rechners mit. Das ist der Grund für den Zwischenschritt
  BlackHole und für die harten Regeln im Plugin (kein Sperren, kein Allokieren,
  kein Resampling).
- **Wartung:** eine zweite Backend-Familie, ein zweiter Tray-Weg, ein zweiter
  Paketweg. Gegenmittel: alles Neue liegt in `backend/*/macos.rs`,
  `tray/macos.rs` und `packaging/macos/` – die Linux-Seite bleibt unverändert
  und der ganze macOS-Teil ist mit einem `git rm -r` wieder weg.
- **Nicht enthalten:** Zeit für Notarisierungs-Wartezeiten, Fehlersuche im
  `coreaudiod`-Log, und die Gefahr, dass ein zweites Backend jede künftige
  Änderung doppelt kostet.

## Folgen

- `pw.rs` verliert `Zugriff`, `devices`/`inputs`/`echo`/`wache` sprechen
  `Geraet` statt Portnamen. Die Tests dieser Module prüfen danach den Trait,
  nicht PipeWire – sie laufen auf jeder Plattform.
- `crates/fenster::geraete` importiert keinen PipeWire-Typ mehr; die
  Oberflächentests (`egui_kittest`) bekommen einen zweiten Bestand geschenkt.
- `web/index.html` und `remote_api.rs` bekommen ein Feld `platform`; nichts
  bricht, weil es niemand auswertet.
- `docs/architektur.md` (Abschnitt Plattformen) verweist auf diesen ADR statt
  „macOS: nicht geplant" zu behaupten.
- `README.md` bekommt einen macOS-Abschnitt **erst**, wenn (b) auf echter
  Hardware läuft. Vorher steht dort nichts – ein Versprechen ohne Binary ist
  schlimmer als Schweigen.

## Offene Punkte (blockieren die Umsetzung, nicht diesen Beschluss)

1. macOS-Untergrenze bestätigen: 14.2 (Taps) – oder Verzicht auf Taps.
2. Apple Silicon allein oder Universal.
3. Entwicklerkonto vorhanden?
4. Mac zum Testen vorhanden (Modell, Audio-Interface)?
5. BlackHole-Zwischenschritt gewünscht oder direkt das eigene Plugin?
6. Gerätenamen identisch zu Linux (`CROSSPOINT-Input`,
   `CROSSPOINT-Output-B1..B3`) – bestätigen.

Solange 1–4 offen sind, wird **nichts** davon gebaut; Schritt (a) ist der
einzige, der ohne Antworten sinnvoll ist, und er ist auch ohne macOS nützlich.
