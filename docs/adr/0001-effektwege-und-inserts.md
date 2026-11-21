# 0001 – Effektwege und Inserts

Stand: 2026-11-17

## Entscheidung

1. **Hall und Echo rechnet die Engine selbst** (`crates/core/src/effekte.rs`,
   Freeverb-Struktur bzw. Stereo-Echo). Sie sind die Send-Effekte `R` und `D`
   des Originals und laufen ohne weitere Software.
2. **FX1/FX2 und Inserts sind JACK-Ports**, keine eingebetteten Plugins:
   `fx1_L/R` → Effekt → `fx1_ret_L/R`, je Streifen und Bus
   `<kanal>_ins_L/R` → Effekt → `<kanal>_ret_L/R`. LV2-Ketten verkabelt
   Crosspoint über mod-host selbst (`fx.rs`, Port-Symbole aus `lv2.rs`), VST
   läuft über Carla oder jeden anderen JACK-Host.
3. **Ein Insert wirkt nur mit verbundenem Rückweg.** Ein Wächter
   (`lauf.rs`, 300 ms) setzt `ins_bereit`; ohne ihn geht das Signal direkt
   durch. Ein vergessenes Kabel macht nie einen Kanal stumm.
4. **Alles steht im Modell** (`Params`): Sends, Returns, `sel`, `monitor`,
   `vaio`, `green`, Programme. Oberfläche, OSC, HTTP und Presets benutzen
   denselben Setter; `vaio` legt ein Wächter aus dem Modell an.

## Alternativen

- **Plugins im Prozess hosten (lilv/suil):** ein abstürzendes Plugin risse den
  Mixer mit, Worker-Threads und State-Restore müssten nachgebaut werden.
- **Insert-Ports erst bei Bedarf anlegen:** weniger Ports im Graphen, aber
  Port-Anlage aus dem Audio-Thread heraus ist nicht erlaubt und der Rest müsste
  umgebaut werden. 64 zusätzliche Ports sind billig.

## Kosten

- Inserts und FX1/FX2 bilden eine Schleife im Graphen: eine Periode Latenz.
- mod-host ist nicht in Fedora; ohne ihn bleiben LV2-Ketten gespeichert, aber
  inaktiv.
- `stripmode` des Originals ist nicht belegt und deshalb nicht umgesetzt.
- Intellipan ist auf Klangfarbe + Pan reduziert (Busse sind stereo).
