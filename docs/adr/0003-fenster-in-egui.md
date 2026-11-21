# 0003 – Fenster in egui statt GTK4

Stand: 2026-11-21 · entschieden

## Kontext

Die Oberfläche war GTK4 (`crates/ui-gtk`, ≈ 11 700 Zeilen) mit eigenem
CSS-Generator, Laufzeit-Themes und Theme-Editor, dazu ein WebKitGTK-Rückfall
(`fenster/fenster.c`). Für das Bauen brauchte es GTK-Header, für das Flatpak
die GNOME-Runtime. Der Nutzer wollte das Frontend „ganz in Rust wie in
sprechwerk“ – und „am Ende genauso gut aussehend“.

## Entscheidung

1. **egui 0.37 mit eframe (glow, AccessKit)**, aufgebaut wie
   sprechwerk/crates/fenster: `modell.rs` (Summentypen, `Aktion`), `logik.rs`
   (führt Aktionen aus, rechnet, getestet), `ansichten/` (`render(modell) →
   Aktionen`), `bausteine.rs` (gezeichnete Fader, Pegel, Pan, Schalter).
2. **Drei Themes aus Tokens** (`tokens/*.tokens.json` → `build.rs`), IBM Plex
   Mono eingebettet. Kein Theme-Editor, keine Nutzer-Themes (AGENTS.md §2.7).
3. **Optik wie das GTK-Pult**: gleiche Zugfolge, Skala +12…−60, segmentierte
   Pegel mit Spitze, Pan-Kranz, Zuordnungsraster A1–B3, Schalterfarben.
   Formen, die Plex Mono nicht hat (●, ■, ▸, ▾), werden gezeichnet.
4. **Tray mit `ksni`** statt eines handgeschriebenen glib-D-Bus-Dienstes.
   Mit Tray schließt der Fensterknopf das Fenster wirklich; „Fenster zeigen“
   öffnet ein neues (`run_and_return`). Grund: winit kann unter Wayland kein
   Fenster verstecken.
5. **Ein Prozess, eine Instanz**: ein zweiter Start ruft `POST /api/fenster`
   der laufenden Instanz und endet.
6. **Tests mit `egui_kittest`**: bedienen über AccessKit (fehlende Namen und
   fehlende Glyphen lassen sie scheitern) und erzeugen `docs/bilder/`.

## Alternativen

| Alternative | Warum nicht |
|---|---|
| GTK4 behalten und nur anpassen | Header beim Bauen, GNOME-Runtime im Flatpak, zweiter Stil neben sprechwerk |
| iced | Barrierefreiheit (AccessKit) in egui reifer; sprechwerk zeigt egui im Einsatz |
| Slint | eigene Auszeichnungssprache |
| Tauri/Web | Node und WebKit, genau was sprechwerk abgelöst hat |

## Kosten

- Frei anordenbare Layouts und eigene Themes der GTK-Fassung entfallen; es
  bleiben drei Themes. Die Dichte der Züge entfällt später mit dem Scrollen
  ([ADR 0007](0007-pult-ohne-scrollleisten.md)).
- Die Web-Oberfläche (`web/index.html`) bleibt als Fernbedienung, nicht mehr
  als Paritätsreferenz.
