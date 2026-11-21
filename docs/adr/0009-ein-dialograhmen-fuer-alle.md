# 0009 – Ein Dialograhmen für alle Dialoge

## Kontext

Die Dialoge über dem Pult waren gewachsen, wie sie gebraucht wurden: PRESETS,
VBAN, MIDI und DARSTELLUNG als `egui::Window` mit eigenem Titel, eigener Breite
und einem „✕ SCHLIESSEN“-Knopf am unteren Rand; das FILTER-Modal als
`egui::Modal` mit einem Schließen-X oben rechts. Gleiche Aufgaben, drei
verschiedene Lösungen – und im FILTER-Modal stand der Schließen-Knopf dort, wo
ihn niemand sucht. Dazu ein Modal ohne Raster: acht Karten in drei festen
Spalten, unterschiedlich breit, mit Lücken.

Nutzerfeedback (wörtlich): „Das Fenster sieht nur scheiße aus ^^ schau mal wo
Schließen ist usw. – konsistent zu allen anderen bitte, und responsive
bedeutet: die Kästen nutzen ein Grid und die volle Breite vom Modal.“

## Entscheidung

### 1. Ein Baustein: `bausteine::dialog_rahmen`

Alle Dialoge (FILTER, PRESETS, VBAN, MIDI, DARSTELLUNG) laufen durch denselben
Rahmen:

* `egui::Modal` – zentriert, abgedunkelter Hintergrund, blockiert das Pult.
* Titelleiste: Titel links in derselben Schrift wie die übrigen
  Überschriften, Schließen-X als Icon rechts (`Icon::Schliessen`,
  zugänglicher Namen `SCHLIESSEN` in jedem Dialog).
* Gleicher Rand (`DIALOG_RAND`), gleicher Hintergrund (`bg_panel`), gleiche
  Innenabstände.
* Geschlossen wird überall gleich: X, Escape oder ein Klick daneben
  (`Modal::should_close`). Der separate „✕ SCHLIESSEN“-Knopf entfällt.

### 2. Raster statt fester Spalten (FILTER-Modal)

* Die Spaltenzahl folgt aus der Innenbreite: drei im kleinsten Fenster, vier
  bei breiterem. Alle Spalten sind gleich breit und füllen die Innenbreite
  genau; die Kartengröße wird gerechnet, nicht geschätzt.
* Die Höhe einer Zeile ist die höchste Karte der Zeile. Dafür misst
  `karte_hoehe` die Beschreibung im Umbruch der Kartenbreite und zählt die
  Reglerzeilen – dieselben Maße, mit denen die Karte gezeichnet wird. Kein
  zweiter Zeichenlauf, keine doppelten Widget-IDs.
* Reicht die Fensterhöhe nicht (viele Geräte in der ECHO-Karte), kommt eine
  Spalte dazu – lieber schmaler als abgeschnitten (ADR 0007: kein Scrollen).
* In der Karte: Beschriftung und Wert in festen Spalten, der Schieber nimmt den
  Rest. Zeilen rechnen mit der Kartenbreite, nicht mit
  `ui.available_width()`: egui weitet `max_rect` auf, sobald ein Inhalt
  überläuft, und dann würde jede Zeile ein Stück breiter als die Karte.
* Der An/Aus-Schalter sitzt oben rechts und ist in jeder Karte gleich groß.

### 3. Die ECHO-Auswahl bietet nur lokale Ausgabegeräte

`geraete::ist_echo_kandidat` lässt weg:

* Crosspoints eigene Sinks (`crosspoint_out_B1`, `CROSSPOINT-Output-B1`):
  ihr Ton kommt aus diesem Mixer; als Echo-Referenz gelegt schlösse sich die
  Schleife Bus → Referenz → Bus, und die Echounterdrückung rechnete gegen sich
  selbst. (Dieselbe Prüfung wie bei den Eingängen: `inputs::ist_eigen`.)
* Netzwerk- und AirPlay-Senken (`raop_*`, `airplay`, `tunnel`, `rtp`,
  `netjack`): was dort hinausgeht, spielt in einem anderen Raum; der Monitor
  eines solchen Sinks sagt nichts über den Schall am Mikrofon, und die
  Netzlatenz macht die Referenz unbrauchbar.

AirPlay also **nicht** anbieten, obwohl der Monitor technisch existiert: die
Referenz muss den Schall am Mikrofon abbilden, nicht den Ton in der Küche.
Was schon gewählt ist, bleibt sichtbar – eine ältere `echo.json` kann so etwas
enthalten, und eine laufende Referenz muss abwählbar bleiben.

Lange Gerätenamen stehen einzeilig gekürzt da (`bausteine::kuerzen`, gemessen
in der Schrift, in der wirklich gezeichnet wird), der volle Name im Tooltip.

### 4. Meldung mit Gerätenamen

Die Kopfzeile meldete „Echo-Referenz: 0000_03_00 an“ – der technische Kurzname
eines HDMI-Sinks. `logik::geraete_anzeige` nimmt die Beschreibung aus dem
Bestand: „Echo-Referenz: Navi 48 HDMI/DP Audio Controller … an“.

## Alternativen

| Alternative | Warum nicht |
|---|---|
| `egui::Window` behalten, nur den Schließen-Knopf nach oben | kein Klick-daneben, kein abgedunkelter Hintergrund, und jedes Fenster braucht seine eigene Titelleiste |
| Rahmen je Dialog, aber gleiche Maße | genau die Dopplung, die auseinanderläuft; ein Baustein kann nicht vergessen werden |
| Kartenhöhe messen, indem jede Karte zweimal gezeichnet wird | doppelte Widget-IDs, doppelte Arbeit je Bild, flackernde Zustände |
| Karten in einem `egui::Grid` | Grid richtet Zellen aus, macht aber keine gleich hohen Karten je Zeile |
| ECHO-Liste in einem Scrollbereich | verboten (ADR 0007); siehe `docs/TODO.md` für den Fall „mehr als sieben Geräte“ |
| AirPlay-Senken anbieten, weil der Monitor existiert | die Referenz wäre der Ton eines anderen Raums, die Echounterdrückung rechnete mit dem falschen Signal |

## Kosten

* Ein Baustein mehr in `bausteine.rs`, dafür vier Fenster weniger, die ihre
  Titelleiste selbst zeichnen.
* Das Raster rechnet die Zeilenhöhen (Beschreibung im Umbruch gemessen). Wer
  Texte verlängert, ändert damit die Kartenhöhe – die Tests halten fest, dass
  alles im kleinsten Fenster bleibt.
* Im kleinsten Fenster nimmt das FILTER-Modal fast die ganze Höhe ein.
* Die ECHO-Auswahl ist auf lokale Geräte eingeschränkt; wer eine
  Netzwerk-Senke als Referenz will, muss `echo.json` von Hand schreiben (der
  Kern verbindet sie weiterhin).
