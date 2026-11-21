# 0004 – Pakete: Flatpak, AppImage, tar.gz – nur Linux

Stand: 2026-11-21 · entschieden

## Kontext

Bisher entstanden ein Flatpak (GNOME-Runtime wegen WebKit/GTK) und ein
tar.gz; die README nannte zusätzlich rpm und deb, die nie gebaut wurden.
Wunsch: „nur für Linux, aber für verschiedene Derivate – Flatpak und AppImage“.

## Entscheidung

1. **Flatpak** auf `org.freedesktop.Platform//25.08` (enthält `pactl`,
   `pw-*`, `libjack.so.0` von pipewire-jack, EGL/GL). Die Programme kommen
   fertig aus dem CI (`prebuilt/`); die Cargo-Quellen sind in
   `flatpak/new-sources.json` eingefroren, erzeugt von
   `packaging/flatpak/quellen.py` aus `Cargo.lock`.
2. **AppImage und tar.gz** aus `packaging/linux/paket.sh` (wie in sprechwerk).
   Nichts wird gebündelt, was das System liefern muss: libjack (pipewire-jack),
   Wayland/X11, OpenGL. Der Autostart schreibt im AppImage `$APPIMAGE`.
3. Der Release-Workflow hängt alle drei an; rpm/deb entfallen.

## Alternativen

| Alternative | Warum nicht |
|---|---|
| rpm und deb zusätzlich | je Distribution pflegen; Flatpak und AppImage decken alle ab |
| libjack ins AppImage packen | muss zur PipeWire-Version des Systems passen; pipewire-jack ist überall vorhanden |
| Snap | nicht gewünscht, auf Fedora/Bazzite kein Standard |
| GNOME-Runtime behalten | wird ohne GTK/WebKit nicht mehr gebraucht, größerer Download |

## Kosten

- Ohne `pipewire-jack` auf dem System startet das AppImage nicht (Meldung im
  Fenster); das Flatpak bringt es mit.
