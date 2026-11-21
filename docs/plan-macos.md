# Plan: macOS-Unterbau für Crosspoint

Stand: 2026-11-21 · **nur Plan, kein Produktionscode** · Entscheidungsvorlage zu
[ADR 0010](adr/0010-macos-unterbau.md)

Nutzerwunsch, wörtlich: „Cool wäre dann noch eine Mac-Version, wenn das geht,
aber plane da erstmal den Unterbau – Mac nutzt ja nicht PipeWire."

Genau da liegt der Punkt: **PipeWire ist nicht portierbar.** Portierbar ist der
Mixer. Dieser Plan trennt beides: erst die Grenze (Schritte a), dann ein
zweites Backend dahinter (b–f). Nichts hier wird gebaut, bevor nicht jemand mit
einem Mac daneben sitzt.

Fragen 1 und 2 aus §7 sind **am 2026-11-21 beantwortet**: unterstützt wird nur
die jeweils **aktuelle macOS-Version** – keine Rückwärtskompatibilität – und
damit **nur Apple Silicon**. Taps sind damit immer vorhanden, der
Universal-Binary-Weg entfällt ersatzlos.

Frage 3 ist **am 2026-11-21** ebenfalls beantwortet: **kein Entwicklerkonto.**
Also keine Developer ID, keine Notarisierung, nur **ad hoc** signiert – dafür
kostenlos über GitHub-Releases. Zwei Folgen: Gatekeeper blockiert den ersten
Start (Anleitung in §3.7), und ein eigenes HAL-Plugin ist ohne
Apple-Signatur nicht belastbar (Recherche in §3.2). **BlackHole wird damit der
dauerhafte Weg für die virtuellen Geräte**, das eigene Plugin nur noch Option.
Fragen 4–6 sind offen.

## 1. Ist-Analyse am Code

Gemessen mit `wc -l` über `crates/*/src` und `src` (ohne Tests, Stand
`feat/rohsignal-und-ausgangsfilter`): **32 652 Zeilen** Rust.

### 1.1 Plattformneutral – bleibt unangetastet

| Teil | Dateien | Zeilen | Warum portierbar |
|---|---|---|---|
| Parametermodell + DSP | `engine.rs` (Params/`EngineDsp`), `dsp.rs`, `aec.rs`, `agc.rs` | 3 443 | reine Arithmetik auf `f32`-Puffern und Atomics |
| VBAN | `vban_io.rs` + `crates/vban` | 2 110 + 1 100 | UDP, kein Gerätezugriff |
| OSC | `osc.rs` | 1 228 | UDP |
| HTTP/WebSocket | `http.rs` + `remote_api.rs` | 2 090 | `tiny_http`/`tungstenite`, reines Rust |
| Tape | `tape_io.rs` + `crates/tape` | 969 + 700 | `hound`, Dateien |
| Makros | `macros_io.rs` + `crates/macros` | 667 + 1 300 | Datei + `Params` |
| Zustand/Autosave | `restore.rs`, `json.rs` | 1 223 | Datei + `serde` |
| Pegelbank | `pegel.rs` | 179 | Ringpuffer |
| Oberfläche | `crates/fenster` **ohne** `geraete.rs`/`tray.rs` | 7 609 | egui/eframe laufen auf macOS |

Der Beweis steht schon im Code: `EngineDsp::process(&Params, &Io, frames)`
bekommt `Io` – 18 rohe `*const f32`/`*mut f32`-Paare, mehr nicht
(`engine.rs:786`). Der DSP weiß nicht, dass es JACK gibt. Das ist die Naht, an
der ein zweites Backend andocken kann, ohne den Kern anzufassen.

### 1.2 Linux-spezifisch – muss hinter eine Grenze

| Datei | Zeilen | Gebunden an | Art der Bindung |
|---|---|---|---|
| `core/src/pw.rs` | 2 342 | Crate `pipewire`, `libpipewire-0.3` | API, nicht nur Modell |
| `core/src/lauf.rs` | 879 | `jack`-Crate, `Port<AudioIn>`, `ProcessScope` | API im Audio-Thread |
| `core/src/devices.rs` | 979 | `Graph` (PipeWire-Knoten) | **Modell**, nicht API |
| `core/src/inputs.rs` | 679 | `Graph` + Portnamen | Modell |
| `core/src/wache.rs` | 551 | `Graph` | Modell |
| `core/src/echo.rs` | 489 | Monitor-Ports (`*_monitor`) | Modell |
| `core/src/aufraeumen.rs` | 289 | `pactl`, `pw-cli` per `Command` | Fremdprozess |
| `core/src/autostart.rs` | 278 | XDG-`.desktop`, Flatpak-Erkennung | OS-Konvention |
| `core/src/pfade.rs` | 109 | `$XDG_CONFIG_HOME` | OS-Konvention |
| `core/src/jack.rs` | 92 | `libjack.so.0` | API |
| `fenster/src/geraete.rs` | 628 | `crosspoint::pw::{Graph, PipeWire}` direkt | **Bruch zu AGENTS.md §2.10** |
| `fenster/src/tray.rs` | 227 | `ksni` (D-Bus StatusNotifierItem) | OS-Konvention |
| `scripts/crosspoint-wire.sh` | 142 | `pactl` (module-null-sink), `pw-link` | Shell |
| `scripts/crosspoint-launch.sh` | 49 | XDG, Flatpak | Shell |

Summe: **6 687 Zeilen im Kern** (20,5 %) plus 855 im Fenster plus 191 Zeilen
Shell.

Wichtig für den Aufwand: nur **2 434 Zeilen** (`pw.rs` + `jack.rs`) hängen an
einer C-API, die es auf macOS nicht gibt. Die restlichen 4 253 Zeilen –
`devices`, `inputs`, `echo`, `wache` – sind **Konsumenten eines Geräte-Modells**
(`Graph`/`Node`/`Port`/`Link`). Sie laufen unverändert weiter, wenn dieses Modell
neutral wird und ein zweites Backend es füllt. Deshalb ist Schritt (a) kein
Umbau, sondern ein Ausschneiden.

### 1.3 Was im Fenster schon richtig ist

`crates/fenster` fasst PipeWire an genau **einer** Stelle an:
`geraete.rs` importiert `pw::{Graph, PipeWire, Zugriff}` und liest den Bestand
über `Zugriff::graph(&PipeWire)` (Zeile 227). Ein Trait gibt es also bereits –
`Zugriff` in `pw.rs:418` mit `graph`, `verbinden`, `trennen`,
`lautstaerke_setzen`. Es liegt nur im falschen Modul und spricht PipeWire
(`"node:port"`). Überall sonst steht PipeWire in Kommentaren, nicht im Code.

## 2. Zielarchitektur

### 2.1 Zwei Grenzen, nicht eine

```
   crates/fenster   (egui, kennt nur Modell und Aktionen)
        │  Arc<Params>, Befehlskanal, Pegelbank, Umgebung als Daten
        ▼
   crates/core
        │
        ├── Audio-Backend    ← ein Callback, 18 Puffer, kein Wissen vom Gerät
        └── Geräte-Backend   ← welche Geräte, welcher Bus wohin, Echo-Referenz
                 │                        │
        ┌────────┴────────┐      ┌────────┴─────────┐
        │ JACK + PipeWire │      │ Core Audio        │   ← src/ wählt
        │ (heute)         │      │ (neu, macOS)      │
        └─────────────────┘      └───────────────────┘
```

Die Auswahl passiert in `src/bin/crosspoint.rs` – der Kompositionswurzel.
`crates/fenster` bekommt **kein** `cfg`, sondern eine `Umgebung` als Daten
(AGENTS.md §2.10). `cfg` erlaubt bleibt nur dort, wo Cargo es erzwingt: in
`Cargo.toml` (`[target.'cfg(target_os = "linux")'.dependencies]`).

### 2.2 Trait „Audio-Backend" (Skizze)

```rust
// crates/core/src/backend/audio.rs
/// Ein Block, so wie ihn der Kern braucht: was reinkommt, was rausgeht.
pub struct Block<'a> {
    pub frames: usize,
    pub ins:      [[&'a [f32]; 2]; STRIPS],   // 8 Streifen stereo
    pub outs:     [[&'a mut [f32]; 2]; BUSES],// 8 Busse stereo
    pub echo_ref: [&'a [f32]; 2],             // Referenz der Echounterdrückung
}

pub trait AudioBackend: Send {
    fn abtastrate(&self) -> u32;
    fn block(&self) -> u32;
    /// Client/Geräte aufmachen und die 18 Puffer anmelden.
    fn oeffnen(&mut self) -> Result<(), String>;
    /// Ab hier ruft das Backend `f` je Block auf – im Realtime-Thread.
    /// `f` sperrt nicht, allokiert nicht, schreibt in `Block`.
    fn starten(&mut self, f: Box<dyn FnMut(&mut Block) + Send + 'static>) -> Result<(), String>;
    fn stoppen(&mut self);
    fn laeuft(&self) -> bool;
}
```

Heute: `Rt::run(&ProcessScope)` (`lauf.rs:325`) **zieht** sich die Puffer über
`p.as_slice(ps)` selbst aus den JACK-Ports. Künftig **bekommt** es sie. Das ist
der einzige Eingriff in den Audio-Thread und mechanisch: 40 Zeilen in
`lauf.rs:325–350` verschwinden, `Io` bleibt, wie es ist.

Linux-Umsetzung: `struct JackBackend { jack: Jack, rt: Rt }` – der heutige Code,
unverändert. Zusätzlich zu den 18 Mixer-Puffern braucht die JACK-Variante ihre
zwei VBAN-Ausgangsports (`vban_L/R`); die kommen als optionales Feld in `Block`
oder als zwei weitere Einträge in `outs` – beim Bauen entscheiden, beides ist
ein Zeilenumbau.

### 2.3 Trait „Geräte-Backend" (Skizze)

`Zugriff` wird aus `pw.rs` nach `backend/geraete.rs` gezogen und von
PipeWire-Sprech auf Geräte-Sprech gehoben. Der Unterschied ist wesentlich:
`verbinden("node:port", "node:port")` gibt es auf macOS nicht – dort ist die
Frage „Bus A1 auf welche Box", nicht „Port 34 nach Port 61".

```rust
pub enum Art { Ausgang, Eingang, Strom }  // Strom = ein Programm, das spielt

pub struct Geraet {
    pub id: String,           // stabil, gilt nur innerhalb eines Backends
    pub name: String,
    pub beschreibung: String,
    pub art: Art,
    pub lokal: bool,          // Ton ist hier im Raum hörbar (heute: !Bluetooth, !Netz)
    pub lautstaerke: Option<Lautstaerke>,
}

pub trait GeraeteBackend: Send + Sync {
    fn geraete(&self, art: Art) -> Result<Vec<Geraet>, String>;
    /// Bus (A1..A5 = Hardware, B1..B3 = virtuell) an ein Gerät hängen.
    fn bus_haengen(&self, bus: &str, geraet: &str) -> Result<(), String>;
    fn bus_loesen(&self, bus: &str) -> Result<(), String>;
    /// Streifen 1..5 an eine Aufnahmequelle.
    fn streifen_haengen(&self, strip: usize, quelle: Option<&str>) -> Result<(), String>;
    /// Echo-Referenz: der Ton, den die Boxen spielen. Linux: Monitor-Ports,
    /// macOS: ein Tap auf die Ausgabe.
    fn referenz_setzen(&self, geraete: &[String]) -> Result<(), String>;
    fn lautstaerke_setzen(&self, geraet: &str, v: Option<f32>, stumm: Option<bool>) -> Result<(), String>;
}
```

`devices.rs`, `inputs.rs`, `echo.rs`, `wache.rs` rufen künftig diese fünf
Methoden statt `Zugriff::verbinden` mit Portnamen. `Graph`/`Node`/`Port` bleiben
als **PipeWire-Interna** in `pw.rs`; was nach außen geht, ist `Vec<Geraet>`.
Der Testaufwand bleibt gleich – die Testdoppel (`KeinZugriff`) implementieren
den neuen Trait genauso wie den alten.

### 2.4 Plattform als Daten (AGENTS.md §2.10)

```rust
pub enum Plattform { Linux, MacOs }

pub struct Umgebung {
    pub plattform: Plattform,
    /// Gibt es ein Tray? Auf beiden ja, nur anders verdrahtet.
    pub tray: bool,
    pub autostart: AutostartArt,      // XdgDesktop | LaunchAgent | Flatpak
    pub virtuelle_geraete: Virtuell,  // PwModule { skript: PathBuf } | HalPlugin { installiert: bool } | BlackHole | Keine
}
```

Das Fenster rendert daraus: „Virtuelle Geräte: HAL-Plugin, nicht installiert –
installieren" statt einer Zeile mit `cfg!(target_os = "macos")`. Der
Web-Fernbedienung reicht ein Feld mehr im JSON (`"platform": "macos"`), das
`web/index.html` heute schon nicht auswertet und deshalb nicht bricht.

Ein Feld `taps` gibt es **nicht mehr**: die Untergrenze ist die jeweils aktuelle
macOS-Version (Frage 1, beantwortet), und die Taps sind seit 14.2 dabei – sie
können also nicht fehlen. Fehlt der Zugriff, ist das die TCC-Berechtigung, ein
Fehlerzustand im Modell, kein Plattformmerkmal.

## 3. macOS im Einzelnen

### 3.1 Audio: Core Audio

| Frage | Antwort |
|---|---|
| Welche API | **AudioObject-API (HAL)** – `AudioDeviceCreateIOProcID` je Gerät. Das ist die Ebene, auf der JACK unter Linux sitzt; gleiche Denkweise, gleiche Puffergrößen. |
| Crate | `objc2-core-audio` 0.3 (nüchterne FFI-Bindings, aktiv gepflegt) **oder** `coreaudio-rs` 0.14 (bequemer, dünner). Nicht `cpal`: cpal ist für „ein Stream je Gerät, ein Callback je Stream" gebaut und versteckt die Aggregat-Geräte, die wir für 18 Kanäle brauchen. |
| Callback-Modell | **Ein IOProc je Gerät.** Das ist der Bruch zum JACK-Modell: dort ist es *ein* Client mit 18 Ports und *ein* Callback. Auf macOS gibt es keinen zentralen Graphen, in den man Ports hängt, sondern Geräte mit eigenen Takten. |
| Konsequenz | Der DSP wird aus dem IOProc heraus gefüttert: ein **Mix-Thread** (Priorität `kAudioDeviceProcessorOverload`-Nähe, `pthread_setschedparam` oder `audio_thread_priority`-Crate) sammelt die Eingangsblöcke aus Ringen ein und verteilt die Ausgangsblöcke. Ringe sind im Haus (`rtrb`, schon für Tape/VBAN benutzt). |
| Latenz | Zwei Puffer mehr als unter JACK: Gerät → Ring → Mix → Ring → Gerät. Bei 64 Frames @ 48 kHz sind das 1,3 ms je Weg. Messbar, nicht schlimm – aber es ist **mehr** als heute, und das muss in der Oberfläche stehen, nicht im Kleingedruckten. |
| Blockgröße | Geräte melden `kAudioDevicePropertyBufferFrameSize`; wir nehmen, was das Gerät gibt (üblich 128–512), und lassen `EngineDsp` mit `max_block` arbeiten, wie heute. |
| Abtastrate | Ein Gerät gibt sie vor. Bei zwei Geräten mit 44,1 und 48 kHz braucht es ein Aggregat-Gerät mit Drift-Korrektur – von Hand nachbauen ist die häufigste Fehlerquelle, deshalb: Aggregate. |
| Allokationsfreiheit | Machbar, aber **nur mit Test**: `crates/core/tests/allokation.rs` prüft heute den DSP-Pfad. Ein zweiter Test muss den macOS-Pfad prüfen (Ring, keine `Vec`-Neuanlage im IOProc). Auf einem Mac ist `malloc_logger`/Instruments dafür das Werkzeug. |

### 3.2 Virtuelle Geräte (CROSSPOINT-Input, CROSSPOINT-Output-B1..B3)

Das ist der Teil, der einen VoiceMeeter-Ersatz ausmacht – ohne ihn hört kein
Programm auf Kanal 3. Auf macOS geht das **ohne Kernel-Treiber**: ein
*Audio Server Plug-in* ist ein userspace-Bundle, das im `coreaudiod` läuft.

| Punkt | Befund |
|---|---|
| Ort | `/Library/Audio/Plug-Ins/HAL/<Name>.driver` (Verzeichnis mit `Contents/Info.plist` + Binary) |
| Sprache | C++17 mit `libASPL` (gavv/libASPL, MIT) – nimmt die Boilerplate von `AudioServerPlugIn.h` ab: `aspl::Driver`, `aspl::Device`, `aspl::Stream` |
| Kein Kernel | richtig, kein kext, kein „Reduzierte Sicherheit" wie bei manchen Audio-Treibern. Kostet trotzdem Admin-Rechte (`sudo`) und einen Neustart von `coreaudiod` |
| Stabilität | **Der IO läuft im `coreaudiod`.** Ein Absturz im Plugin nimmt den Ton des **ganzen Macs** mit. Das ist der Grund, warum BlackHole so zurückhaltend programmiert ist |
| Kommunikation Plugin ↔ App | Shared Memory + zwei lock-freie Ringpuffer (Eingang: App schreibt, Plugin liest; Ausgang umgekehrt) + POSIX-Semaphore oder `os_unfair_lock`-freie Zähler. Format ist fest 32 bit float, verschränkt, 48 kHz – **kein** Resampling im Plugin (das wäre im coreaudiod der sichere Weg zum Aussetzer) |
| Anmeldung | `Info.plist` mit `CFBundlePackageType` `BNDL`, `AudioServerPlugIn_...`-Einträgen; Neustart `sudo launchctl kickstart -k system/com.apple.audio.coreaudiod` |
| Signatur | **Nur ad hoc** (`codesign --force --deep -s - Crosspoint.app`) – kostenlos, im CI machbar. Keine Developer ID, weil kein Entwicklerkonto (Frage 3). Ein HAL-Plugin braucht dagegen nach den vorliegenden Berichten eine Apple-Signatur (Recherche unten) |
| Apple Silicon | **Nur `arm64`.** macOS 26 (Tahoe) war die letzte Version für Intel-Macs, ab macOS 27 läuft macOS ausschließlich auf Apple Silicon. Ein `x86_64`-Slice wäre toter Ballast |

**Alternativen, bewertet:**

| Weg | Aufwand | Urteil |
|---|---|---|
| **BlackHole als Weg für die virtuellen Geräte** | 12–20 h | Zwei BlackHole-Instanzen (8 Kanal) als CROSSPOINT-Input/Output; Nutzer installiert sie selbst (`brew install blackhole-2ch` / Installer). Kostenlos, signiert und notarisiert – sonst gäbe es den Cask seit Homebrew 5.0.0 nicht mehr. Kostet Genauigkeit: keine B1–B3 einzeln, sondern ein 8-Kanal-Gerät, dessen Kanäle wir im Modell auf B1–B3 abbilden. **Empfohlen als Schritt (c) und als Endzustand** – es macht die App benutzbar, ohne dass wir einen Treiber signieren und verteidigen müssen |
| **Eigenes HAL-Plugin** | 60–100 h | Der schönere Weg, aber **nur optional**: vier Geräte mit den Linux-Namen (`CROSSPOINT-Input`, `CROSSPOINT-Output-B1..B3`), Risiko coreaudiod, und ohne Developer ID nach den vorliegenden Berichten nicht zuverlässig ladbar (Recherche unten). Erst nach einem Machbarkeitstest auf echter Hardware |
| **AudioDriverKit (Dext)** | 150 h+ | Apples „moderner" Weg, braucht aber eine `SystemExtension`-Freigabe durch Apple und einen eigenen Antrag. Für ein Hobby-Projekt unverhältnismäßig |
| **Nur Taps, kein virtuelles Gerät** | 20–30 h | Taps können **aufnehmen**, nicht **umleiten**. Damit ginge „Programm X auf Kanal 3" nur halb: man könnte es hören, nicht dorthin schicken. Kein Ersatz |

**Recherche: lädt `coreaudiod` ein nur ad-hoc signiertes Plugin?**

Kurzantwort: **unsicher, mit Tendenz zu nein.** Es gibt zwei brauchbare, sich
widersprechende Berichte:

- *Dagegen, empirisch (macOS 15):* Die CI-Untersuchung von `tympan-aspl`
  (Rust-Bindings für AudioServerPlugin, ADR 0001) hat es durchgespielt:
  `coreaudiod` findet das Bundle und *versucht* zu laden, dann weist der
  „Core Audio Driver Service"-Helfer es über **AMFI** mit
  `AppleMobileFileIntegrityError -423` ab – „the file is adhoc signed or signed
  by an unknown certificate chain". Ad-hoc, selbstsigniert, selbstsigniert als
  vertrauenswürdiger Root im Systemschlüsselbund und `DisableLibraryValidation`
  wurden einzeln und kombiniert probiert: alle enden bei `-423`. SIP-Aus hilft
  nicht, weil `amfid` unabhängig davon prüft. Durch kommt nur eine
  **Developer ID Application**-Signatur.
- *Dafür, von Hand (Apple Silicon):* Der offene Mikrofon-Treiber
  `Orra Microphone` berichtet am 2026-10-15, ein **ad hoc signierter Eigenbau**
  in `/Library/Audio/Plug-Ins/HAL` werde von `coreaudiod` geladen und erscheine
  als virtuelles Eingabegerät (geprüft mit QuickTime Player und Google Meet).
  Dieselbe Quelle nennt das „was andere offene HAL-Plugins zum Testen nutzen".
- *Älter, dagegen:* Eine Anfrage auf Apples Mailingliste `coreaudio-api`
  (2020, Catalina) sagt, Audio-Server-Plugins müssten notarisiert sein, sonst
  lade `coreaudiod` sie nicht.

Der Widerspruch ist mit Dokumentation nicht aufzulösen, nur mit einem Versuch
auf echter Hardware. Denkbare Erklärung: dem Von-Hand-Kopieren fehlen
Auslöser, die `-423` sonst provozieren (Quarantäne-Merkmal, Hardened Runtime,
`--deep`, fehlende `Info.plist`-Schlüssel) – oder es liegt an der
macOS-Version. **Gesichert ist nur:** ohne Apple-ausgestellte Signatur ist der
Weg mindestens brüchig, und wenn er bricht, sieht der Nutzer kein Gerät und
keine Fehlermeldung.

Folge für den Plan: **BlackHole wird die dauerhafte Lösung** für die virtuellen
Geräte, nicht der Zwischenschritt. Schritt (d) rutscht auf „optional, nur nach
erfolgreichem Machbarkeitstest". Der Test besteht genau dann, wenn der zweite
Bericht oben reproduzierbar ist – lokal, ohne Developer ID.

### 3.3 Echo-Referenz und Eingänge: Core Audio Taps

Das ist die Stelle, an der macOS **besser** ist als Linux. Heute sucht
`echo.rs` die Monitor-Ports der Ausgabegeräte und legt sie auf
`crosspoint:echo_ref_L/R` – genau das, was ADR 0005 beschreibt, und es hat
unter PipeWire die Einschränkung, dass ein Gerät einen Monitor haben muss.

Auf macOS gibt es seit **14.2** `AudioHardwareCreateProcessTap`
(`CATapDescription`) – und da die Untergrenze die jeweils aktuelle
macOS-Version ist (Frage 1), ist es immer vorhanden: ein Tap kopiert die
Ausgabe eines Prozesses, einer Gruppe
oder – als globaler Stereo-Mixdown – **des ganzen Systems**, ohne es zu
verändern. Der Tap wird in ein privates Aggregat-Gerät gehängt
(`AudioHardwareCreateAggregateDevice` mit `kAudioAggregateDeviceTapList`) und
über einen IOProc gelesen – also genau über denselben Weg wie ein Mikrofon.

Daraus folgt für den Plan:

- **Echo-Referenz** = globaler Tap auf die Systemausgabe. Ein Objekt statt
  einer Suche über Geräte-Monitore. `echo.rs` schrumpft auf „Referenz-Tap an,
  Referenz-Tap aus".
- **Eingänge 1–5** = je Streifen ein Tap auf einen Prozess oder eine Gruppe.
  Das ist die Funktion, für die unter Linux `pw-link` und Monitore gebraucht
  werden – auf macOS ist es ein API-Aufruf. Nebeneffekt: „Programm X auf
  Streifen 3" funktioniert **ohne** virtuelles Gerät, solange nur gehört und
  nicht zurückgespielt wird.
- **Berechtigung**: Taps laufen über TCC. Beim ersten Zugriff fragt macOS, der
  Eintrag steht in den Systemeinstellungen unter Datenschutz (Mikrofon bzw.
  Systemaudio – der genaue Name hängt von der macOS-Version ab und ist beim
  Bauen zu prüfen). Ohne Zustimmung kommt ein leerer Puffer, kein Fehler –
  das muss die Oberfläche als Zustand zeigen (`Virtuell::Keine`-Muster:
  „Berechtigung fehlt" ist ein Modellzustand, keine Meldung).
- Der Code dafür ist neuer als `cpal`/`coreaudio-rs` abdecken: `CATapDescription`
  ist eine Objective-C-Klasse ohne C-Entsprechung → `objc2` + `objc2-core-audio`,
  ~200 Zeilen eigene Bindung. Das ist der Preis für den bequemeren Weg.

### 3.4 Geräteauswahl und Verbindung ohne `pw-link`

Heute macht `devices.rs` das über Portnamen (`bus_port`), `inputs.rs` über
`pw_link`. Auf macOS gibt es kein `pw-link`, und man kann fremde Programme
**nicht** umleiten. Die drei Fälle:

1. **Bus A1–A5 auf eine Box.** Kein Routing nötig: der Nutzer wählt in den
   Systemeinstellungen die Ausgabe, oder wir hängen unser Ausgangsgerät an ein
   Aggregat. Für 5 verschiedene Hardware-Ausgänge *gleichzeitig* braucht es ein
   Aggregat-Gerät mit 10 Kanälen, das wir im Prozess anlegen (privat) – das
   ersetzt `pw-link` vollständig.
2. **Bus B1–B3 in ein Programm.** Das Programm wählt `CROSSPOINT-Output-B1` als
   Ausgabe. Kein Routing möglich, keine Zustimmung nötig – der Nutzer entscheidet
   im Programm. (Alternativ ein Tap, aber der kann nur mithören.)
3. **Streifen 1–5 aus einem Programm.** Tap (3.3) oder `CROSSPOINT-Input` als
   Ausgabe des Programms.

Damit ist `devices.rs` auf macOS **kleiner** als auf Linux: die Hälfte seiner
979 Zeilen sind Portnamen-Rechnerei für `pw-link`.

### 3.5 Tray, Autostart, Pfade

| Linux heute | macOS | Aufwand |
|---|---|---|
| `ksni` (D-Bus SNI), `tray.rs:227` | `tray-icon` 0.25 (tauri-apps, NSStatusItem) – **muss im Hauptthread** angelegt werden, Tray-Icon und Eventloop hängen zusammen | 8–14 h; `tray.rs` wird in `tray/mod.rs` (Befehlsumschlag, testbar) + `tray/linux.rs` + `tray/macos.rs` geteilt |
| `autostart.rs` schreibt `.desktop` nach `$XDG_CONFIG_HOME/autostart` | `~/Library/LaunchAgents/de.weppelmann.Crosspoint.plist` oder `SMAppService.mainApp.register()` (macOS 13+) | 8–12 h; `SMAppService` ist der richtige Weg, weil es dem Nutzer in den Systemeinstellungen sichtbar ist. Ob es eine nur ad-hoc signierte `.app` akzeptiert, ist beim Bauen zu prüfen – sonst LaunchAgent |
| `pfade.rs`: `$XDG_CONFIG_HOME/crosspoint` | `~/Library/Application Support/Crosspoint` | 2–3 h (eine Funktion, `basis()`) |
| Flatpak-Erkennung | entfällt | – |
| `scripts/crosspoint-wire.sh` (`pactl module-null-sink`) | entfällt: BlackHole (oder das HAL-Plugin) **ist** der virtuelle Ausgang | – |

Ein Punkt, der leicht übersehen wird: `--minimized` startet heute „ohne
Fenster, nur Tray". Auf macOS bedeutet das `NSApplicationActivationPolicy
Accessory`, nicht „Fenster verstecken" – sonst hüpft beim Anmelden ein
Fenster im Dock. Das ist ein `Umgebung`-Feld, kein `cfg`.

### 3.6 Pakete, Signatur, CI (ohne Notarisierung)

| Punkt | Befund | Aufwand |
|---|---|---|
| Form | `.app`-Bundle + `.dmg` (Drag-and-drop). Kein Flatpak, kein AppImage – beides gibt es auf macOS nicht | 6–10 h |
| Architektur | Nur `cargo build --target aarch64-apple-darwin`. **Kein** `x86_64`-Slice, kein `lipo`, kein Intel-Job – macOS 27 und folgende laufen nur noch auf Apple Silicon | 4–6 h |
| Signatur | **ad hoc**: `codesign --force --deep -s - Crosspoint.app`. Kein Zertifikat, kein Schlüsselbund, kein CI-Secret, keine Developer ID – kostet nichts. Hardened Runtime und Entitlements entfallen (wir sind nicht sandboxed) | 2–3 h |
| Notarisierung | **entfällt.** Ohne Developer ID gibt es nichts zu notarisieren. Der Preis dafür steht in §3.7: Gatekeeper warnt beim ersten Start | – |
| Kosten | **0 €.** Kein Apple Developer Program, keine Zertifikatsverwaltung, keine Verlängerung. Bleibt: Testhardware (Mac mini M4 + USB-Interface, ~700 €) oder geliehen | – |
| CI | nur `macos-14`-Runner (arm64), `macos-13` (Intel) entfällt. GitHub rechnet macOS-Minuten **10-fach**; ein Vollbau dürfte 8–15 min brauchen → spürbar, aber bezahlbar im Free-Kontingent | 6–9 h |
| Release | `release.yml` bekommt einen `macos`-Job; `release-please` schreibt zusätzlich in die `.app`-Version. Kein Flatpak-Quellen-Nachziehen | 4–6 h |

Die Zeilen überlappen sich (die Signatur steckt auch im Release-Job); verbindlich
ist die Summe in §5.

### 3.7 Erster Start ohne Entwicklerkonto (Gatekeeper)

Es gibt kein Entwicklerkonto (Frage 3), also keine Developer ID und keine
Notarisierung. Die `.app` wird **ad hoc** signiert – das hält den Binärinhalt
unverändert und die Signatur prüfbar, sagt macOS aber nichts über den
Hersteller. Eine heruntergeladene App gilt damit als unbekannt.

**Was der Nutzer sieht:** „Crosspoint kann nicht geöffnet werden, da Apple nicht
überprüfen kann, ob sie Schadsoftware enthält." Der Weg durch:

1. Doppelklick, im Dialog **Fertig** (früher „OK").
2. **Systemeinstellungen › Datenschutz & Sicherheit**, Abschnitt *Sicherheit*:
   „Crosspoint wurde blockiert, um deinen Mac zu schützen" → **Trotzdem
   öffnen** → mit Passwort oder Touch ID bestätigen → im zweiten Dialog noch
   einmal **Trotzdem öffnen**.
3. Einmalig. Danach startet die App normal.

**Was nicht mehr geht:** Rechtsklick → *Öffnen*. Diesen Umweg hat Apple in
**macOS 15 (Sequoia)** entfernt; er ist in macOS 26 (Tahoe) nicht
zurückgekommen. Jede Anleitung der Form „Rechtsklick, Öffnen, nochmal Öffnen"
ist auf einem aktuellen Mac falsch – sie darf weder ins README noch in eine
Fehlermeldung.

**Zweiter Weg, für Terminal-Nutzer und wenn die Einstellungen nicht auftauchen:**

```bash
xattr -dr com.apple.quarantine /Applications/Crosspoint.app
```

Der Quarantäne-Vermerk (`com.apple.quarantine`) hängt am Download, nicht an der
App. Ist er weg, prüft Gatekeeper beim Start nichts mehr; die Signatur bleibt
ad hoc. Bei jedem Update ist die neue `.app` wieder ein neuer Download.

**README-Entwurf** (Abschnitt „macOS", gesetzt erst mit (b)/(f)):

> **macOS – Apple Silicon, jeweils aktuelle macOS-Version**
>
> 1. `Crosspoint-<version>.dmg` von den GitHub-Releases laden und öffnen.
> 2. `Crosspoint` in den Ordner *Programme* ziehen.
> 3. Erster Start: Doppelklick, *Fertig*, dann *Systemeinstellungen ›
>    Datenschutz & Sicherheit › Trotzdem öffnen* und noch einmal *Trotzdem
>    öffnen*. Einmalig. Alternativ im Terminal:
>    `xattr -dr com.apple.quarantine /Applications/Crosspoint.app`
> 4. Virtuelle Geräte: `brew install blackhole-2ch` (oder `blackhole-16ch`),
>    danach den Mac neu starten.
>
> *Warum die Warnung?* Crosspoint ist kostenlos und ohne Entwicklerkonto
> gebaut – also ad hoc signiert und nicht von Apple notarisiert. Der Quelltext
> liegt in diesem Repository; wer das nicht möchte, baut selbst (unten).
>
> *Selbst bauen:* `cargo build --release --features ui` (braucht Rust und die
> Xcode Command Line Tools). Ein selbst gebautes Programm trägt keinen
> Quarantäne-Vermerk und startet ohne Warnung.

**Andere Verteilwege, geprüft:**

| Weg | Befund | Urteil |
|---|---|---|
| Bau aus Quellen (`cargo build --release --features ui`) | umgeht Gatekeeper ganz, weil ein lokaler Build keinen Quarantäne-Vermerk trägt; kostet Rust + Xcode Command Line Tools, ~10 min | guter Zweitweg – gehört ins README, nicht als einziger Weg |
| Homebrew-Cask (`brew install --cask crosspoint`) | **kein Weg mehr.** Homebrew 5.0.0 (Nov 2025) hat Casks ohne Codesignatur und Notarisierung abgekündigt; sie werden seit September 2026 aus dem offiziellen Tap entfernt und sind mit `brew install` nicht mehr installierbar. `--no-quarantine` ist ebenfalls abgekündigt („Homebrew does not wish to easily provide circumvention to macOS security features") | entfällt. Ein eigener Tap (`brew tap …`) bliebe möglich, verschiebt das Problem nur und ist zusätzliche Wartung |
| Selbstsigniertes Zertifikat | hilft weder Gatekeeper noch `amfid`: geprüft wird gegen Apple-ausgestellte Ketten, ein selbstsigniertes Zertifikat im Systemschlüsselbund genügt nicht | kein Ersatz |

**Folge für den Aufwand:** (f) verliert Notarisierung, Zertifikatsverwaltung im
CI und den zweiten Signaturschritt; übrig bleiben `.app`, `.dmg`, ad-hoc-Signatur
im CI, der `macos-14`-Job und der README-Abschnitt oben.

## 4. Reihenfolge: sechs Schritte, jeder lauffähig

Regel aus `docs/architektur.md`: „Wer die Portierung wieder aufbringt, beginnt
mit dem Audio-Backend allein und entscheidet erst danach über Oberfläche und
Geräte-Routing." Der Plan folgt dem, ergänzt aber Schritt (a) davor – ohne die
Grenze gibt es kein „allein".

### (a) Grenzen einziehen, Linux unverändert grün — **24–40 h**

1. `Zugriff` aus `pw.rs` nach `backend/geraete.rs`; `devices`/`inputs`/`echo`/
   `wache` sprechen `Geraet` statt `node:port`.
2. `AudioBackend` + `JackBackend`; `Rt::run` bekommt `&mut Block`.
3. `Umgebung`/`Plattform` ins Modell, `tray`/`autostart`/`pfade` hinter je
   einen kleinen Trait mit einer Linux-Umsetzung.

**Fertig, wenn:** `cargo test --workspace --features ui` grün ist, **ohne** dass
eine Testzeile geändert wurde, und die Oberflächenbilder in `docs/bilder/`
byte-gleich sind. Kein Verhalten ändert sich – nur die Naht.

### (b) macOS-Build mit Core-Audio, ohne virtuelle Geräte — **40–60 h**

1. `Cargo.toml`: `pipewire` und `jack` unter
   `[target.'cfg(target_os = "linux")'.dependencies]`, `objc2-core-audio` unter
   `cfg(target_os = "macos")`. `eframe`-Features `wayland`/`x11` nur für Linux.
2. `CoreAudioBackend`: n IOProcs, Ringtonnen, Mix-Thread, `Block` füllen.
3. `GeraeteBackend` für Core Audio: `AudioObjectGetPropertyData` über
   `kAudioHardwarePropertyDevices`, Aggregate im Prozess anlegen.
4. Erster Lauf auf echter Hardware: 5 Streifen aus dem Mikro, 5 Busse auf die
   Boxen, DSP dazwischen.

**Fertig, wenn:** ein Mac mit einem Mikrofon und einer Box den Mixer bedient –
Fader, Filter, Pegel, Tape, VBAN. Ohne virtuelle Geräte, ohne Taps, ohne Tray.
Das ist der Moment, an dem sich entscheidet, ob der Rest Geld wert ist.

### (c) BlackHole für die virtuellen Geräte — **12–20 h**

`Virtuell::BlackHole { gefunden: bool }`: erkennt installierte BlackHole-Geräte
(8 Kanal), bildet die Kanäle auf B1/B2/B3 und Input ab, zeigt in der Oberfläche
„BlackHole installieren" mit Link, wenn keins da ist. Kein eigener Code, der
Treiber berührt. Nach der Recherche in §3.2 ist das **der Endzustand** für die
virtuellen Geräte, nicht nur ein Zwischenschritt.

**Fertig, wenn:** ein Browser, der auf B1 spielt, auf Bus B1 im Pult ankommt –
mit einer Anleitung im README, die `brew install blackhole-2ch` nennt.

### (d) Eigenes HAL-Plugin — **60–100 h, optional**

**Nur nach erfolgreichem Machbarkeitstest auf echter Hardware.** Der Test:
ein ad hoc signierter Eigenbau in `/Library/Audio/Plug-Ins/HAL` erscheint nach
`killall coreaudiod` als Gerät – ohne Developer ID. Schlägt er fehl (die
CI-Untersuchung in §3.2 sagt genau das), entfällt (d) ersatzlos; (c) trägt die
virtuellen Geräte allein. Ohne Entwicklerkonto ist (d) ohnehin nicht
auslieferbar.

1. C++-Bundle mit `libASPL`, zwei Geräte: `CROSSPOINT-Input` (8 Kanal In) und
   `CROSSPOINT-Output-B1..B3` (je 2 Kanal). Namen wie unter Linux, damit
   `web/index.html`, Handbuch und Screenshots stimmen.
2. Shared Memory + zwei `rtrb`-Ringe (Rust-Seite) / zwei SPSC-Ringe (C++-Seite,
   dieselbe Speicherordnung), Übergabe über `mmap` auf eine Datei in
   `~/Library/Application Support/Crosspoint` oder POSIX-`shm_open`.
3. Installer-Skript, das nach `/Library/Audio/Plug-Ins/HAL` schreibt und
   `coreaudiod` neu startet – mit Rückfrage, nicht im Hintergrund.
4. Deinstallation: Datei weg, `coreaudiod` neu starten. Muss im Programm stehen,
   sonst bleibt Müll im System, den niemand findet.

**Fertig, wenn:** zwei Programme gleichzeitig auf B1 und B2 spielen und ein
drittes von `CROSSPOINT-Input` aufnimmt – ohne BlackHole, nach einem Reboot.

### (e) Taps für Echo und Eingänge — **20–30 h**

`CATapDescription` in `objc2`, globaler Tap als `echo_ref`, Prozess-Taps als
Streifen 1–5. Berechtigungszustand als Modellzustand. `echo.rs` bekommt eine
zweite, **kürzere** Umsetzung.

**Fertig, wenn:** ADR 0005 auf macOS nachweisbar ist – 30 dB Echo-Unterdrückung
mit Boxen und Mikrofon, gemessen wie in ADR 0005 beschrieben.

### (f) Pakete, Signatur, CI — **8–14 h**

`.app`, `.dmg` (arm64), **ad-hoc-Signatur** (`codesign --force --deep -s -`),
`macos-14`-Job allein in `release.yml`, `README`-Abschnitt „macOS" mit der
Gatekeeper-Anleitung aus §3.7. Kein `lipo`, kein Zertifikat, kein
`notarytool`.

**Fertig, wenn:** ein fremder Mac das `.dmg` lädt, die Anleitung aus §3.7 ihn
durch die Warnung bringt und die App danach startet – und ein zweiter Nutzer
das ohne Rückfrage schafft.

## 5. Aufwand

| Schritt | Inhalt | Stunden |
|---|---|---|
| (a) | Grenzen einziehen, Linux grün | 24–40 |
| (b) | Core-Audio-Backend, echte Geräte | 40–60 |
| (c) | BlackHole für die virtuellen Geräte | 12–20 |
| (d) | eigenes HAL-Plugin, **optional** (nur nach Machbarkeitstest) | 60–100 |
| (e) | Taps für Echo und Eingänge | 20–30 |
| (f) | `.app`/`.dmg` (arm64), ad-hoc-Signatur, CI, README | 8–14 |
| | **Summe, mit optionalem (d)** | **164–264 h** |

Die frühere Schätzung in `docs/architektur.md` nannte 170–240 h. Der
Unterschied kommt aus zwei Posten, die dort fehlten: die Trait-Grenze (a,
24–40 h) ist Voraussetzung für alles weitere, und die Taps (e) sind kein
Nebenschauplatz, sondern der Ersatz für `echo.rs` – ohne sie wiederholt sich der
Fehler aus ADR 0005 auf dem Mac.

**Ohne eigenes Plugin** (a+b+c+e+f): **104–164 h**. Das ist nicht mehr der
„Zwischenlösungs"-Preis, sondern der empfohlene Weg: (c) trägt die virtuellen
Geräte dauerhaft, (e) die Echo-Referenz. Mit (d) wären es 164–264 h – und (d)
bräuchte eine Developer ID, die es nicht gibt, also ist die Zahl heute nicht
gehbar.

Was **nicht** enthalten ist: Testhardware, und die Zeit, die ein Fehler im
`coreaudiod` kostet.

## 6. Risiken

| Risiko | Wirkung | Umgang |
|---|---|---|
| **HAL-Plugin im `coreaudiod`** | ein Absturz nimmt den Ton des ganzen Macs mit; ein Fehler im Speicherzugriff ist im Log des Nutzers nicht zu finden | kein Blockieren, kein Allokieren, kein Resampling im Plugin; Ringpuffer mit definiertem Verhalten bei Voll/Leer (Stille, nie Warten); Lasttest über 8 h; Rückfrage vor Installation |
| **Kein Mac zum Testen** | jede Zeile bleibt Vermutung; Latenz, Xruns und TCC-Verhalten lassen sich nicht simulieren | Testhardware **vor** (b): Mac mini M4 + USB-Interface, ~700 €, oder geliehen. Ohne Hardware ist (b) nicht fertig |
| **Kein Entwicklerkonto** | keine Developer ID, keine Notarisierung → Gatekeeper warnt beim ersten Start; ein HAL-Plugin ist so nicht zuverlässig auslieferbar | bewusst akzeptiert (Frage 3): Anleitung in §3.7, im README **direkt unter dem Download-Link**; virtuelle Geräte über BlackHole statt über eigenes Plugin |
| **Warnung wird als Defekt gelesen** | Nutzer lädt, sieht „Apple kann nicht überprüfen …" und bricht ab oder schreibt ein Issue | README-Absatz „Warum die Warnung?" mit einem Satz Begründung; `xattr`-Zeile als zweiter Weg; im Fenster nichts dazu (die App läuft ja schon, wenn man sie sieht) |
| **Jede Version ist für macOS eine andere App** | ad-hoc signiert → TCC-Berechtigungen (Mikrofon, Systemaudio) werden nach dem Update erneut erfragt | im Modell als Zustand führen und im Fenster erklären statt wegklicken zu lassen; kein „Berechtigung dauerhaft" versprechen |
| **HAL-Plugin lädt nicht** (falls (d) je gebaut wird) | ohne Apple-Signatur `-423`, Gerät erscheint nicht, keine Fehlermeldung | (d) nur nach Machbarkeitstest auf echter Hardware; bis dahin bleibt BlackHole der Weg |
| **n IOProcs statt einem Callback** | Takt-Drift zwischen Geräten, Aussetzer hörbar als Knacken | Aggregate-Geräte statt Handarbeit; Ringe mit Füllstandsanzeige im Modell („xruns: 3") |
| **Latenz** | 2 Puffer mehr als heute | im Modell als Zahl führen und im Fenster zeigen; kein „low latency"-Versprechen im README |
| **TCC** | Berechtigung fehlt → stiller leerer Puffer, wirkt wie „kaputt" | Zustand im Modell, Hinweis im Fenster mit Weg zu den Systemeinstellungen |
| **AGENTS.md §2.10** | „mal schnell ein `cfg!(target_os)`" im Fenster | Prüfung im Review: `grep -r "cfg(target_os" crates/fenster` muss leer bleiben; Unterschiede nur über `Umgebung` |
| **Wartung** | zwei Backends, zwei Tray-Wege, zwei Paketwege | die Linux-Seite bleibt unverändert; alles Neue liegt in `backend/*/macos.rs` und `packaging/macos/` – ein `git rm -r` entfernt es restlos |

## 7. Offene Fragen an den Nutzer

1. ~~Welche macOS-Version ist die Untergrenze?~~ **Beantwortet am 2026-11-21**,
   wörtlich: „ja es reicht die neuste version :)“. Also **nur die jeweils
   aktuelle macOS-Version**, keine Rückwärtskompatibilität. Folge: die Taps
   (seit 14.2) sind immer vorhanden, die Prüfung `taps: bool` entfällt, und
   ältere Versionen werden nicht getestet. Der frühere Vorschlag 14.2 und die
   Notiz „ab macOS 13“ in `docs/architektur.md` sind damit erledigt.
2. ~~Apple Silicon oder auch Intel?~~ **Beantwortet am 2026-11-21** (mit
   Frage 1): **nur Apple Silicon**. macOS 26 (Tahoe) war die letzte Version für
   Intel-Macs; ab macOS 27 läuft macOS ausschließlich auf Apple Silicon. Kein
   Universal Binary, kein `lipo`, kein `macos-13`-Job, HAL-Plugin nur `arm64`.
3. ~~Gibt es ein Entwicklerkonto (99 €/Jahr)?~~ **Beantwortet am 2026-11-21**,
   wörtlich: „ne habe ich nicht ist ja auch ein kostenloses tool was sich die
   leute über github ziehen können". Also **kein Apple-Entwicklerkonto, keine
   Developer ID, keine Notarisierung** – Verteilung kostenlos über
   GitHub-Releases, Signatur **ad hoc**. Folge: Gatekeeper-Warnung beim ersten
   Start (§3.7), und (d) ist damit nicht auslieferbar (§3.2).
4. **Steht ein Mac zum Testen bereit?** Welcher, mit welchem Audio-Interface?
   Ohne Hardware ist Schritt (b) nicht abnehmbar.
5. **Eigenes HAL-Plugin oder BlackHole?** **Empfehlung jetzt: BlackHole zuerst
   und dauerhaft.** Ohne Entwicklerkonto ist ein eigenes Plugin nicht
   zuverlässig ladbar; es wird nur noch gebaut, wenn der Machbarkeitstest aus
   (d) auf echter Hardware gelingt. BlackHole ist kostenlos, signiert, wird vom
   Nutzer per `brew install blackhole-2ch` installiert und kostet uns keinen
   Treiber-Support.
6. **Dürfen die Gerätenamen von Linux abweichen?** Vorschlag: nein –
   `CROSSPOINT-Input`, `CROSSPOINT-Output-B1..B3` wie unter PipeWire, damit
   Handbuch, Screenshots und Web-Fernbedienung für beide Plattformen gelten.
7. **Ist die Zielgruppe „nur Ton" oder auch „Musik machen"?** Bei Musik ist die
   Latenz aus 3.1 ein eigenes Thema (dann Core-Audio-Pfad direkt im IOProc
   rechnen, nicht über den Mix-Thread).

## 8. Quellen

Apple:

- Core Audio Taps: <https://developer.apple.com/documentation/coreaudio/capturing-system-audio-with-core-audio-taps>
- macOS 26 (Tahoe) ist die letzte Version für Intel-Macs (WWDC25, Platform State
  of the Union): <https://www.heise.de/en/news/macOS-26-Tahoe-Last-release-for-Intel-Launchpad-says-goodbye-10438678.html>
  und <https://arstechnica.com/gadgets/2025/06/one-last-look-at-software-support-as-macos-26-tahoe-winds-down-the-intel-mac-era/>
- `AudioHardwareCreateProcessTap` (neu in macOS 14.2): <https://sdkdiffs.furnacecreek.org/macos-14.2/CoreAudio.html>
- Audio Server Plug-in / Driver Extension: <https://developer.apple.com/documentation/coreaudio/building-an-audio-server-plug-in-and-driver-extension>
- `AudioServerPlugIn.h`: <https://developer.apple.com/documentation/coreaudio/audioserverplugin>

Gatekeeper und erster Start:

- „Safely open apps on your Mac": <https://support.apple.com/en-us/102445>
- „Open an app by overriding security settings" (Weg über *Datenschutz &
  Sicherheit › Trotzdem öffnen*):
  <https://support.apple.com/guide/mac-help/open-an-app-by-overriding-security-settings-mh40617/mac>
- Rechtsklick-Umweg in macOS 15 entfernt:
  <https://arstechnica.com/gadgets/2024/08/macos-15-sequoia-makes-you-jump-through-more-hoops-to-disable-gatekeeper-app-checks/>
- Homebrew 5.0.0 (Casks ohne Signatur abgekündigt, `--no-quarantine`
  abgekündigt): <https://brew.sh/2025/11/12/homebrew-5.0.0/> und
  <https://workbrew.com/blog/homebrew-5-0-0>

HAL-Plugin: lädt `coreaudiod` einen ad-hoc signierten Eigenbau?

- `tympan-aspl`, ADR 0001 (AMFI weist ad-hoc und selbstsigniert mit `-423` ab,
  empirisch auf `macos-15`):
  <https://docs.rs/crate/tympan-aspl/latest/source/docs/decisions/0001-ci-verification-strategy.md>
- `tympan-aspl`, Plugin-Author-Guide („ad-hoc signature is enough for
  `coreaudiod` to *discover and attempt* the bundle"):
  <https://docs.rs/crate/tympan-aspl/latest/source/docs/plugin-author-guide.md>
- Gegenbericht: `Orra Microphone`, von Hand installierter ad-hoc-Build läuft
  (2026-10-15, Apple Silicon):
  <https://github.com/db-ol/Orra/blob/main/Drivers/OrraMicrophone/README.md>
- Älterer Gegenbericht (2020, Catalina): „all Audio Server plug-ins need to be
  notarised": <https://www.mail-archive.com/coreaudio-api@lists.apple.com/msg01717.html>

Crates (Stand der Recherche, im Bau zu prüfen):

- `objc2-core-audio` 0.3: <https://crates.io/crates/objc2-core-audio>
- `coreaudio-rs` 0.14: <https://crates.io/crates/coreaudio-rs>
- `cpal` 0.18 (Vergleich, nicht gewählt): <https://crates.io/crates/cpal>
- `tray-icon` 0.25: <https://crates.io/crates/tray-icon>
- `audio_thread_priority`: <https://crates.io/crates/audio_thread_priority>
- `rtrb` (schon im Projekt, für die Ringe): <https://crates.io/crates/rtrb>

HAL-Plugin-Bausteine und Vorbilder:

- `libASPL` (C++17, MIT): <https://github.com/gavv/libASPL>
- `roc-vad` – dasselbe Muster in Produktion: <https://github.com/roc-streaming/roc-vad>
- BlackHole: <https://github.com/ExistentialAudio/BlackHole>

Hausinterne Belege: [ADR 0004](adr/0004-pakete-flatpak-appimage.md) (Pakete),
[ADR 0005](adr/0005-echounterdrueckung-mit-aec3.md) (Echo-Referenz),
[ADR 0010](adr/0010-macos-unterbau.md) (diese Entscheidung),
[architektur.md](architektur.md) (Schichten).
