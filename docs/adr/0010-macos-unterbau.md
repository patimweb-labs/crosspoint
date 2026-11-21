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
   Daten – `Plattform`, `tray`, `autostart`, `virtuelle_geraete`. Die
   Ansichten rendern daraus. Ein Feld `taps` gibt es nicht: die Untergrenze ist
   die jeweils aktuelle macOS-Version, die Taps (14.2) können nicht fehlen.
   `cfg` bleibt nur in `Cargo.toml`, wo Cargo es erzwingt.

3. **macOS-Audio über Core Audio (HAL-API)**, gebunden mit `objc2-core-audio`
   (0.3) statt `cpal`. Begründung: cpal verbirgt Aggregat-Geräte und den
   IOProc, den wir für 18 Kanäle und einen Tap brauchen. Preis: das
   Callback-Modell ändert sich – auf macOS ein IOProc **je Gerät**, nicht ein
   Client mit 18 Ports. Der DSP wird deshalb von einem Mix-Thread gefüttert,
   der die Ringpuffer der Geräte sammelt. Zwei Puffer mehr Latenz als unter
   JACK; das steht als Zahl im Modell, nicht als Versprechen im README.
4. **Virtuelle Geräte: BlackHole, vom Nutzer installiert.** Kein eigener
   Treiber. Grund: ohne Entwicklerkonto (Frage 3) gibt es keine Developer ID,
   und nach den vorliegenden Berichten weist AMFI ein nur ad-hoc signiertes
   Audio-Server-Plugin mit `AppleMobileFileIntegrityError -423` ab – ein
   Gegenbericht zeigt einen von Hand installierten ad-hoc-Build, der läuft
   (Einzelheiten und Quellen in [../plan-macos.md](../plan-macos.md) §3.2). Der
   Befund ist **unsicher**; deshalb wird die sichere Seite gewählt. BlackHole
   ist kostenlos, signiert und notarisiert, wird per
   `brew install blackhole-2ch` installiert und bildet die Kanäle auf B1–B3 ab.
   Ein **eigenes HAL-Plugin** (`libASPL`, userspace, kein `kext`) bleibt als
   Option, wird aber nur gebaut, wenn ein Machbarkeitstest auf echter Hardware
   zeigt, dass ein ad-hoc signiertes Bundle lädt. Bis dahin gilt: es kostet
   einen Treiber im `coreaudiod`, den wir nicht signieren können.
5. **Echo-Referenz und Eingänge über Core-Audio-Taps** (seit macOS 14.2, also
   auf jeder unterstützten Version), nicht
   über Monitor-Geräte. Ein globaler Stereo-Tap ersetzt die Monitor-Suche aus
   `echo.rs`; Prozess-Taps ersetzen `pw-link` für „Programm X auf Streifen 3".
   Damit ist die Echo-Referenz auf macOS **näher an ADR 0005** als unter
   PipeWire, wo ein Gerät einen Monitor haben muss.
6. **Untergrenze: die jeweils aktuelle macOS-Version** – keine
   Rückwärtskompatibilität (Frage 1, beantwortet am 2026-11-21). Damit sind die
   Taps immer vorhanden. Folge: **nur Apple Silicon**, weil macOS 26 (Tahoe)
   die letzte Version für Intel-Macs war und macOS ab 27 ausschließlich auf
   Apple Silicon läuft. Kein Universal Binary, kein `lipo`, kein Intel-CI-Job
   (`macos-13` entfällt), HAL-Plugin nur `arm64`.
7. **Pakete: `.app` + `.dmg`, ad hoc signiert** (ADR 0004 gilt weiter für
   Linux; macOS bekommt keinen vierten Linux-Weg). Kein Entwicklerkonto, also
   **keine Developer ID, keine Notarisierung**: `codesign --force --deep -s -`,
   kostenlos und im CI machbar. Preis: Gatekeeper blockiert den ersten Start
   einer heruntergeladenen App; der Weg führt über *Systemeinstellungen ›
   Datenschutz & Sicherheit › Trotzdem öffnen* oder
   `xattr -dr com.apple.quarantine /Applications/Crosspoint.app` – **nicht**
   mehr über Rechtsklick → Öffnen, das macOS 15 entfernt hat. Verteilung
   kostenlos über GitHub-Releases. Ein Homebrew-Cask ist kein Ersatzweg: seit
   Homebrew 5.0.0 müssen Casks signiert und notarisiert sein. CI-Job auf
   `macos-14`, arm64 allein. Kosten: **0 €**, nur Testhardware.
8. **Reihenfolge in sechs Schritten**, jeder einzeln lauffähig:
   (a) Grenzen einziehen, Linux grün · (b) Core-Audio-Backend ohne virtuelle
   Geräte · (c) BlackHole · (d) eigenes HAL-Plugin (**optional**, nur nach
   Machbarkeitstest) · (e) Taps · (f) Pakete.
   Nach (b) ist entscheidbar, ob der Rest die Arbeit wert ist. Summe ohne (d)
   **104–164 h**, mit (d) 164–264 h.

## Alternativen

| Alternative | Warum nicht |
|---|---|
| Kein macOS, wie bisher | Der Nutzer hat gefragt; die Grenze (a) ist auch ohne macOS nützlich (Tests ohne PipeWire, Windows später) |
| Portierung beginnen, wo `architektur.md` sie begann – beim Audio-Backend, ohne Traits | dann steht macOS-Code neben PipeWire-Code statt dahinter; zwei Wahrheiten im selben Modul |
| `cpal` als Audio-Schicht | bequem für „ein Stream je Gerät", verdeckt Aggregate und IOProc; unsere 18 Kanäle plus Tap sind kein cpal-Anwendungsfall |
| PipeWire-Emulation auf macOS (eigener Server) | 300 h+ und ein zweites Betriebssystem im Betriebssystem |
| AudioDriverKit statt HAL-Plugin | braucht Apples Freigabe für eine System Extension; Aufwand ~150 h+ für dasselbe Ergebnis |
| Nur BlackHole, kein eigenes Plugin | **das ist die Entscheidung** (siehe Entscheidung 4) – keine B1–B3 einzeln, die Gerätenamen weichen von Linux ab, Handbuch und Screenshots gelten nur halb. Der Preis dafür ist kleiner als ein Treiber, den wir nicht signieren können |
| Eigenes HAL-Plugin trotzdem, ohne Entwicklerkonto | nach den vorliegenden Berichten weist AMFI ein ad-hoc signiertes Bundle mit `-423` ab; ob der Gegenbericht verallgemeinerbar ist, ist offen |
| Nur Taps, kein virtuelles Gerät | Taps nehmen auf, sie leiten nicht um – „Programm X auf Kanal 3" bliebe unmöglich |
| Windows gleich mitplanen | eigenes Thema (signierter Kernel-Treiber, EV-Zertifikat 300–700 €/Jahr); die Grenze (a) macht es später leichter, aber dieser ADR entscheidet nichts darüber |

## Kosten

- **Arbeit:** 104–164 h ohne optionales (d), 164–264 h mit, nach
  [../plan-macos.md](../plan-macos.md) §5, davon 24–40 h für (a) – und die
  fallen **auch ohne macOS** an, weil sie die Testbarkeit ohne PipeWire
  verbessern.
- **Geld:** **0 €.** Kein Apple Developer Program, keine Zertifikate, keine
  Notarisierung. Bleibt: Testhardware (Mac mini M4 und USB-Interface, ~700 €)
  oder geliehen; macOS-CI-Minuten zählen 10-fach.
- **Preis beim Nutzer:** eine Gatekeeper-Warnung beim ersten Start und nach
  jedem Update neu erteilte TCC-Berechtigungen (ad-hoc signiert = für macOS
  jedes Mal eine andere App). Das steht im README, nicht im Kleingedruckten.
- **Risiko:** (d) läuft im `coreaudiod` – ein Absturz dort nimmt den Ton des
  ganzen Rechners mit. Deshalb ist (d) optional und kommt nur nach einem
  Machbarkeitstest; getragen wird das Ganze von BlackHole.
- **Wartung:** eine zweite Backend-Familie, ein zweiter Tray-Weg, ein zweiter
  Paketweg. Gegenmittel: alles Neue liegt in `backend/*/macos.rs`,
  `tray/macos.rs` und `packaging/macos/` – die Linux-Seite bleibt unverändert
  und der ganze macOS-Teil ist mit einem `git rm -r` wieder weg.
- **Nicht enthalten:** Fehlersuche im `coreaudiod`-Log, Support für die
  Gatekeeper-Anleitung, und die Gefahr, dass ein zweites Backend jede künftige
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
  schlimmer als Schweigen. Dann aber mit der Anleitung aus
  [../plan-macos.md](../plan-macos.md) §3.7: *Trotzdem öffnen* in den
  Systemeinstellungen oder `xattr -dr com.apple.quarantine …`, plus
  `brew install blackhole-2ch` für die virtuellen Geräte.

## Offene Punkte (blockieren die Umsetzung, nicht diesen Beschluss)

1. ~~macOS-Untergrenze?~~ **Beantwortet am 2026-11-21**: die jeweils aktuelle
   macOS-Version, keine Rückwärtskompatibilität – siehe Entscheidung 6.
2. ~~Apple Silicon allein oder Universal?~~ **Beantwortet am 2026-11-21**: nur
   Apple Silicon, siehe Entscheidung 6.
3. ~~Entwicklerkonto vorhanden?~~ **Beantwortet am 2026-11-21**: keins – „ne
   habe ich nicht ist ja auch ein kostenloses tool was sich die leute über
   github ziehen können". Also ad hoc signiert, keine Notarisierung, und (d)
   nicht auslieferbar – siehe Entscheidungen 4 und 7.
4. Mac zum Testen vorhanden (Modell, Audio-Interface)?
5. BlackHole-Zwischenschritt gewünscht oder direkt das eigene Plugin?
   **Empfehlung nach Frage 3: BlackHole zuerst und dauerhaft.**
6. Gerätenamen identisch zu Linux (`CROSSPOINT-Input`,
   `CROSSPOINT-Output-B1..B3`) – bestätigen.

Solange 4–6 offen sind, wird **nichts** davon gebaut; Schritt (a) ist der
einzige, der ohne Antworten sinnvoll ist, und er ist auch ohne macOS nützlich.
