# 0007 – Pult ohne Scrollleisten: Mindestfenster aus den Maßen

Stand: 2026-11-21 · entschieden · folgt aus [ADR 0003](0003-fenster-in-egui.md)

## Kontext

Wunsch des Nutzers, wörtlich: „Ich will, dass das Programm ohne Scrollleisten
auskommt, sorge dafür, dass es keine mehr gibt, das nervt.“ Dazu zwei
Vorgaben, die nicht verhandelbar sind:

* **Alle neun Skalenzahlen** der Fader (+12 … −60) stehen immer da, keine wird
  ausgedünnt, keine verschwindet.
* **Nichts wird abgeschnitten**, weder ein Zug noch ein Knopf noch eine Zahl.

Bisher löste das Pult Platzmangel mit `egui::ScrollArea::both()`: bei kleinem
Fenster scrollte es in beide Richtungen, und die Züge hatten eine Untergrenze
(„kompakt“ = 100 px), unter der gescrollt wurde. Die Dialoge taten dasselbe
(Presetliste, MIDI-Zeilen) oder wuchsen über das Fenster hinaus.

## Entscheidung

### 1. Das Pult füllt, es scrollt nicht

`logik::zugbreiten` verteilt die verfügbare Breite **exakt** auf die 17 Züge
(acht Kanäle, neun Busse/Master mit 0,82 × Kanalbreite); die Höhe der Züge ist
die verfügbare Höhe. Die Scrollleiste ist ersatzlos weg.

### 2. Die Sichtbarkeit folgt aus der Mindestgröße des Fensters

Weil nichts abgeschnitten werden darf, darf das Fenster nicht beliebig klein
werden. `logik::mindestfenster` rechnet die Grenze aus den Maßen – nicht aus
einer geratenen Zahl – und `betrieb.rs` setzt sie als
`with_min_inner_size`:

| Grenze | Wert | Woraus |
|---|---|---|
| Kanalbreite | 100 px (`SIZE_ZUG_KOMPAKT`) | Zuordnung A1..B3 als Spalte links neben dem Fader, 24 px breit („A1“ misst 15,6 px bei 13 px Schrift); dem Kanalfader bleiben 64 px, seine Skala steht in voller Schrift (Nachtrag unten). Der Busfader hat 72 px Blockbreite. |
| Faderlänge | 179,4 px | Alle neun Zahlen mit 1,15 × Schriftgröße Abstand; der engste Schritt der Skala sind 6 dB (+12 → +6). |
| Pult | 468,4 px | Zug: höchster Fall war der Kanal (Kopf, Fader, Pan, Zuordnung, zwei Schalterreihen); seit dem Nachtrag ist es der Filterbus, das Pult wird nicht höher. |
| **Fenster** | **1642 × 660 px** | Pult (468,4) + Kopfleiste (118) + Makroleiste eingeklappt (66) + Zuschlag fürs Runden der Zeilen. |

Breite und Höhe stehen als Funktion in `logik`, nicht als Zahl in `betrieb.rs`;
Darstellung, Tests und Fenster rechnen mit denselben Konstanten
(`ZugMass`, `fader_laenge`, `fader_block_mindesthoehe`). Was das Fenster
zusätzlich an Platz bekommt, nimmt der Fader – er wächst mit.

**Die Breite ist der Preis:** 17 Züge × 100 px plus Lücken sind 1642 px. Das
Ziel „höchstens 1280 × 720“ ist damit in der Breite nicht zu halten; schmaler
ginge nur, indem A1..B3 oder die Skala verschwinden – genau das ist
ausgeschlossen. In der Höhe liegt das Fenster mit 660 px unter 720 px.

### 3. Faderskala: die Zahlen sitzen links am ganzen Platz

Der Faderblock legte die Bahn bisher in die Mitte und gab der Skala die halbe
Breite. Jetzt sitzt er von rechts (Pegelsäule, Bahn, Strich, Zahlen): die
Zahlen bekommen den ganzen linken Rest. Damit steht die Skala bei 72 px
Blockbreite noch in voller Schrift; würde die Breite je kleiner, wird die
Schrift kleiner bis `SKALA_SCHRIFT_MIN` (¾ der kleinen Schrift) – **nie**
fallen Zahlen weg.

### 4. Makroleiste klappt bei knapper Höhe selbst ein

`logik::makros_eingeklappt` zeigt bei zu wenig Höhe nur die Kopfzeile, damit
die Mindesthöhe klein bleibt. Der gespeicherte Wunsch des Nutzers bleibt
unverändert und gilt wieder, sobald das Fenster hoch genug ist; ein Klick auf
„AUFKLAPPEN“ sagt per Tooltip, dass das Fenster dafür höher werden muss.

### 5. Dialoge: Spalten und Blätter statt Scrollbereiche

* **FILTER-Modal** – ein Raster in der Reihenfolge der Signalkette (LC, ECHO,
  NR, GATE, COMP, AGC, DUCK, PTT von links nach rechts, dann die nächste
  Zeile). Die Spaltenzahl folgt aus der Breite des Modals: vier, solange die
  Karten nicht zu schmal werden (im kleinsten Fenster und bei 1590 × 640 sind
  es vier). Alle Spalten sind gleich breit und füllen die Innenbreite; die
  Karten einer Zeile sind gleich hoch. Reicht die Höhe nicht (viele Geräte in
  der ECHO-Karte), zeigt die Karte weniger Geräte – der Rest bleibt im Hinweis
  und im Tooltip –, statt zu scrollen und statt über das Fenster zu laufen.
  Der Pegel vor/nach den Filtern steht als eigene Zeile unter dem Titel, in
  zwei gleich breiten Hälften mit denselben Spalten wie die Karten. Es passt
  im Alltagsfenster (1590 × 640) und im kleinsten Fenster (geprüft).
* **Presets** – zehn Zeilen je Seite mit „<< ZURÜCK“ / „WEITER >>“.
* **MIDI** – zusätzlich zum Seitenblatt ein Blatt von zehn Zeilen; die letzte
  Seite hat 56 Zeilen (Control Change 0..55) und kommt in sechs Blättern.
* **VBAN** – zeigt nur die laufenden Ströme, ohne Scrollbereich.

### 6. Dichte entfällt

`Dichte` (kompakt/normal/breit) war ein Faktor auf die Zugbreite und wirkte
zuletzt nur noch als Untergrenze – also genau als die Klemme, die das Scrollen
erzwang. Ohne Scrollen füllen die Züge das Fenster; wie breit sie sind,
entscheidet das Fenster. Die Einstellung ist weg (Migration: unbekannte Felder
in `fenster.json` werden ignoriert).

## Alternativen

| Alternative | Warum nicht |
|---|---|
| Scrollleisten verstecken (`ScrollStyle::without_bar`) | Der Inhalt bliebe abgeschnitten, nur ohne Balken – und ohne Balken merkt niemand, dass etwas fehlt. |
| Zahlen ausdünnen (jede zweite) | Vorgabe des Nutzers: alle neun bleiben. |
| Züge unter 100 px, A1..B3 in zwei Zeilen | Der Zug wird höher statt schmaler, und die Mindesthöhe steigt; zudem sind zwei Reihen à vier Knöpfe das Muster des Pults. |
| Mindestgröße als Zahl in `betrieb.rs` (720 × 480) | Genau die geratene Zahl, die das Abschneiden erzeugte. |

## Kosten

- **Mindestfenster 1642 × 660.** Auf einem 1366 × 768-Bildschirm passt das
  Fenster nicht mehr in die Breite; das ist der Preis für „alles sichtbar“.
- Wer die Skala kleiner lesen will, kann das Fenster nicht weiter verkleinern –
  die Grenze steht in `logik::mindestfenster` und wird von Tests gehalten.
- Vier Bildschirmfotos mehr je Theme (`pult-*-1590.png`, `pult-*-min.png`,
  `filter-modal-*-1590.png`, `filter-modal-*-min.png`); der Test
  `oberflaeche::bilder_in_allen_themes` erzeugt sie. Die Bilder werden in der
  Lage erzeugt, in der das Pult steht (1590 × 640 auf einem 2560 × 1440-
  Bildschirm) und in der Mindestgröße – Bilder in einer Wunschgröße, die es im
  Betrieb nicht gibt, zeigen Fehler nicht.

## Nachtrag 2026-11-23: Zuordnung neben dem Fader

Die Rechnung der Kanalbreite ging nicht auf: vier Knöpfe je Reihe bei 100 px
Kanal sind je 20 px breit, abzüglich der Polsterung bleiben 12 px – zu wenig
für „A1“ (15,6 px). In der Startgröße stand die Zuordnung deshalb als „…“ da
(Feedback zu 2.0.0). Zugleich blieb links im Faderblock Platz ungenutzt.

Die acht Knöpfe stehen jetzt als **eine Spalte links neben dem Fader**
(`logik::ZUG_ZUORDNUNG_BREITE`, `zuordnung_oben`), oben A1..A5, nach einer
Lücke B1..B3. Der Fader lässt dafür links einen Streifen frei
(`bausteine::fader`, Parameter `links_frei`); die Zahlenzeile darunter nutzt
weiter die ganze Breite. Die zwei Zuordnungsreihen unter dem Pan entfallen,
der Kanal wird dadurch niedriger. Geprüft von `zuordnung_passt_neben_den_fader`
(Breite, Höhe, Skala in voller Schrift) und `zuordnung_ist_ausgeschrieben`
(alle Kürzel gezeichnet, im Alltag und in der Mindestgröße).
