# Datenfluss

Was ein Bedienschritt auslöst, bis das Bild steht. Ein Prozess: GTK-Oberfläche
und Engine laufen im selben Binary (`crosspoint` mit Feature `ui`).

## „Fader ziehen"

1. **GTK** (`crates/ui-gtk/src/strip.rs`, `StripView`): Der Zeiger setzt die
   Faderposition; `fmt::fader_pos`/`pos_db` rechnen zwischen Pixel und dB um,
   `fmt::einrasten` hält die 0-dB-Raste.
2. **Schreibpfad** (`set_gain`): schreibt in das Atom `StripParam::gain`
   (`crates/core/src/engine.rs`) — keinen eigenen Puffer, kein JSON.
3. **Audio-Thread** (`crates/core/src/lauf.rs`): Der JACK-Callback liest das Atom
   im nächsten Block, die DSP-Kette (`crates/core/src/dsp.rs`) wendet Gain, EQ,
   Gate, Kompressor an, die Routing-Matrix verteilt auf die Busse über
   `StripParam::sends`.
4. **Pegel** (`crates/core/src/pegel.rs`): Der Callback schreibt Spitzenwerte je
   Streifen und Bus in die Pegelbank.
5. **Anzeige** (`shell.rs`, 16-ms-Takt): Der Takt liest die Pegelbank, glättet
   und zeichnet — kein Wert wird erfunden, ohne Audio bleibt die Anzeige still.

## „Bus wählen, dann Master bedienen"

1. Klick auf den Buskopf → `Shell::bus_waehlen` setzt `Master::bus` und zieht alle
   Züge nach (Beschriftung „MAS A1", Mute/Mono/Limiter des Busses).
2. Master-Fader, Mute, Mono, Limiter schreiben **auf den gewählten Bus**
   (`p.set("bus", …)`) — der Master ist eine Fernbedienung, keine zweite Summe.
3. Der Weg in den Audio-Thread ist derselbe wie oben: Atom → Callback → Bus.

## „Aufnahme (Tape)"

1. REC-Knopf → `TapeControl` (`crates/core/src/tape_io.rs`).
2. Der Audio-Callback schreibt den Ring; der Tape-Schreiber legt die WAV-Datei
   im Zielordner ab.
3. Die Oberfläche liest Zustand, Zeit und Spitzenpegel im Takt und zeigt sie in
   der Tape-Leiste.

## Web-Oberfläche (Referenz)

`crates/core/src/http.rs` liefert unter `127.0.0.1:8770` dieselben Parameter als
JSON (`/api/state`, `/api/set`, `/api/levels`) und `web/index.html`. Bis die
GTK-Oberfläche vollständig ist, dient sie als abgleichbare Referenz — sie ist
**keine** zweite Wahrheit: beide Wege schreiben in dieselben Atome.
