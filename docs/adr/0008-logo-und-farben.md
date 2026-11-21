# 0008 – Logo 1:1 aus der Vorlage, Farben aus dem Zeichen

Stand: 2026-11-21 · entschieden · Ausnahme zu [AGENTS.md §8](../../AGENTS.md),
folgt aus [ADR 0003](0003-fenster-in-egui.md)

## Kontext

Wunsch des Nutzers, wörtlich: „Das möchte ich als Logo und App-Icon haben … das
sollst du 1 zu 1 nehmen bitte und die Farben im Programm anpassen.“

Vorlage ist `CROSSPOINT Tech-Logo-Showcase.png` (1536 × 1024): oben das
App-Icon – eine dunkle, abgerundete Kachel mit einer 7 × 7-Matrix grauer
Quadrate, deren mittlere Spalte und Zeile ein cyanfarbenes Kreuz mit
abgerundeten Endstücken bildet, in der Mitte ein bernsteinfarbenes Quadrat mit
Glühen –, daneben das Zeichen und die Wortmarke **CROSSPOINT**. Unten die
Varianten: dunkle Kachel klein, helle Kachel (fast weiß, schwarze Matrix,
schwarzes Kreuz, Bernstein-Mitte), Mono (dunkle Kachel, weißes Kreuz, weiße
Matrix) sowie Zeichen ohne Kachel, groß und klein.

AGENTS.md §8 legt für Marken etwas anderes fest: die Quelle ist ein
ASCII-Raster (`assets/<name>.pm`), die Kachelform ein Achteck, die Zeichnung
zweifarbig, und erzeugt wird mit `apps/markenwerk`. Dieses Logo ist keine
Zeichnung aus dem Raster: es hat sieben Abstufungen, Verläufe, ein Glühen,
zwei Signalfarben und eine Wortmarke aus Buchstabenpfaden.

Am Bild ausgemessen (Pixel, umgerechnet auf das 512er-Raster):

| Teil | Vorlage | 512er-Raster |
|---|---|---|
| Kachel | 404 × 388 px, Ecken r ≈ 64 | 512 × 512, r = 81 |
| Matrix | 7 × 7, Punkt 28,5 px, Raster 45 × 43,5 px | Punkt 36, Raster 56, r = 12 |
| Kreuz | Balken 13 px, Endstück 33 px | Balken 16, Endstück 44, r = 13 |
| Mitte | 40 px, r ≈ 10 | 40 × 40, r = 12, Glühen r = 54 |
| Schrift | Großbuchstaben 80 px hoch, Strich 16 px, `CROSSPOINT` 693 px breit | 100 hoch, Strich 20, 866,25 breit |

Farben: Cyan `#00BDE6`, Bernstein `#FEBD31`, Matrix `#404856`, Kachel
`#14191F`/`#191E25`→`#0C0F14`, Kachelrand `#485163`, Hintergrund `#0A0C10`,
Wortmarke `#E9E9EA`. Helle Variante: Kachel `#E6E5E5`, Matrix und Kreuz
`#14181D`. Mono: Kachel `#15181F`, Kreuz `#FFFFFF`, Matrix `#C6C4C0`.

## Entscheidung

### 1. Das Logo wird übernommen, nicht an §8 angepasst

§8 gilt weiter für alles, was aus einem Raster entsteht; **Crosspoint ist die
Ausnahme**: Zeichen, Wortmarke und App-Icon sind so, wie der Nutzer sie
gewählt hat. Die Quelle ist nicht mehr das Raster, sondern die SVG-Datei.

### 2. Quelle sind die SVG-Dateien, erzeugt wird mit einem Skript

`assets/`:

| Datei | Was |
|---|---|
| `crosspoint-logo.svg` | App-Icon: Kachel, Matrix, Kreuz, Mitte mit Glühen |
| `crosspoint-zeichen.svg` | dasselbe ohne Kachel |
| `crosspoint-hell.svg` | helle Variante (Kachel `#E6E5E5`) |
| `crosspoint-mono.svg` | Mono-Variante (eine Tinte) |
| `crosspoint-klein.svg` | vereinfachte Matrix 5 × 5 für 16, 24 und 32 px |
| `crosspoint-wortmarke.svg` | Zeichen + `CROSSPOINT`, Buchstaben als Pfade |
| `crosspoint-schriftzug.svg` | nur die Buchstaben, weiß — im Fenster eingefärbt |
| `crosspoint-symbolic.svg` | einfarbig, nur Kreuz und Mitte (Schreibtisch-Icon) |

Die Buchstaben der Wortmarke sind **Pfade, keine Schrift**: keine
Font-Abhängigkeit, kein Ausweichen auf eine Ersatzschrift, wenn Plex Mono ein
Zeichen nicht hat.

`packaging/icons/erzeugen.sh` zeichnet daraus die PNGs
(`data/icons/hicolor/<n>x<n>/apps/de.weppelmann.Crosspoint.png` für 16, 24, 32,
48, 64, 128, 256, 512 und 1024), kopiert `scalable` und `symbolic` und setzt
`assets/icon.svg`. 16, 24 und 32 px kommen aus `crosspoint-klein.svg`: bei
16 px ist eine 7 × 7-Matrix mit 1,1 px großen Punkten nur noch Matsch, fünf
Punkte je Kante mit dickeren Balken bleiben erkennbar. Gerendert wird mit
`resvg`, `rsvg-convert` oder ImageMagick — was zuerst da ist.

`assets/crosspoint.pm` ist entfallen: das Raster beschreibt dieses Logo nicht,
und ein Lauf von `apps/markenwerk` würde das alte Zeichen zurückbringen.

### 3. Die Farben stehen als Tokens, nicht im Code

`crates/fenster/tokens/` bekommt die Farben des Zeichens; im Code stehen nur
Namen (AGENTS.md §2.6). Neu sind zwei Grundskalen: `nacht` (die Grauwerte der
Kachel) und `tinte` (die Neutralen der hellen Variante). `papier` schrumpft auf
Weiß.

| Theme | Fläche | Linie | Akzent (Kreuz) | Signal (Kreuzungspunkt) |
|---|---|---|---|---|
| dark | `#0A0C10` / `#14191F` | `#404856` | `#00BDE6` Cyan | `#FEBD31` Bernstein |
| light | `#FAFAF9` / `#E6E5E5` | `#485163` | `#14181D` (das schwarze Kreuz) | `#A96410` |
| mono | `#0A0A0A` / `#101010` | `#555555` | `#FFFFFF` | `#D6D3CE` |

Im hellen Theme ist der Akzent dunkel, nicht cyan: die helle Variante des
Zeichens hat ein schwarzes Kreuz. Bernstein bleibt in allen drei Themes die
zweite Farbe; im hellen Theme ist es für Text abgedunkelt (`#A96410`), weil
`#FEBD31` auf Weiß nur 1,6 : 1 erreicht.

### 4. Zwei Signalfarben, und wo Bernstein hingehört

Das Zeichen erzählt: das Kreuz führt (Cyan), der Kreuzungspunkt sitzt in der
Mitte (Bernstein). Im Pult heißt das:

* **Cyan** = das Signal selbst: Faderfüllung, Pegelsegmente, Filterlämpchen,
  „LÄUFT“, Fokusring, Auswahl.
* **Bernstein** = der Kreuzungspunkt: die Zuordnungstasten `A1 … B3` am Kanal
  (genau die Matrix aus Kanal × Bus, die das Zeichen zeigt), die Spitze der
  Pegelsäule und die obersten Segmente der Pegel (ab „Laut“).

Damit ist Bernstein kein Schmuck, sondern eine Aussage: *hier kreuzt etwas*.
`Art::Signal` in `bausteine.rs` ist die eine Stelle, die es zeichnet.

Kontrast (WCAG 2.1, geprüft in `thema::tests`): Text auf Fläche ≥ 7 : 1,
gedämpfter Text ≥ 4,5 : 1, Akzent und Signal gegen die Fläche ≥ 3 : 1, Schrift
auf Akzent und Signal ≥ 4,4 : 1 — in allen drei Themes.

### 5. Die Marke im Kopf ist das Zeichen mit der Wortmarke

Statt `crosspoint ▪` steht in der Kopfleiste das Kachel-Zeichen (SVG, eigene
Farben) und daneben die Wortmarke als SVG, mit der Textfarbe eingefärbt. Beide
sind über `egui_extras` gezeichnet wie die Icons; das Zeichen bringt seine
Farben mit und ist damit auf allen drei Themes dasselbe Bild.

### 6. Ein Test hält Zeichen und Tokens zusammen

`thema::tests::zeichen_und_tokens_tragen_dieselben_farben` liest die SVGs und
verlangt, dass Kreuz-, Signal-, Matrix- und Kachelfarbe der Tokens darin
vorkommen. Wer eine Farbe nur an einer der beiden Stellen ändert, bekommt einen
roten Test.

## Verworfene Alternative: an §8 anpassen

Das Logo als ASCII-Raster nachzeichnen, Achteck-Kachel, zwei Farben
(`#C9862C` + `#E9E5DC`), erzeugt von `apps/markenwerk`. Das wäre regelkonform
und würde die Werkzeugkette einhalten — es ist aber genau das, was der Nutzer
nicht wollte: „1 zu 1“. Ein nachgezeichnetes Achteck-Logo ist ein anderes Logo.
Der Preis der Ausnahme steht unten.

## Kosten

* **Zwei Quellen für dieselbe Marke**, je nach Repo: `apps/markenwerk` erzeugt
  die anderen Apps, Crosspoint hat seine eigenen SVGs. Wer die Markenfamilie
  ändert, muss hier von Hand nachziehen.
* **Die Buchstaben sind Pfade.** Ein Tippfehler in der Wortmarke ist eine
  Zeichnung, kein Text; die Wortmarke lässt sich nicht umbenennen, ohne die
  Pfade neu zu bauen.
* **Nachzeichnen statt kopieren.** Die Vorlage ist ein Bild, kein Vektor: die
  Maße sind ausgemessen und auf runde Werte gebracht (Tabelle oben). Pixelgleich
  ist das Ergebnis nicht — es ist auf ein paar Zehntel genau und bei 16 px
  bewusst vereinfacht.
* **Die SVGs sind nicht mehr aus dem Raster ableitbar.** `assets/crosspoint.pm`
  ist weg; ein `markenwerk render crosspoint` gibt es nicht mehr.
