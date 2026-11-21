#!/usr/bin/env bash
# Linux-Pakete: tar.gz und AppImage (das Flatpak baut flatpak-builder aus
# denselben Programmen, siehe flatpak/de.weppelmann.Crosspoint.yml).
#
# Aufruf aus dem Wurzelverzeichnis: packaging/linux/paket.sh
# Braucht: cargo, appimagetool (Pfad über APPIMAGETOOL änderbar).
# Ergebnis: dist/crosspoint-<version>-linux-<arch>.tar.gz
#           dist/Crosspoint-<version>-<arch>.AppImage
#
# OHNE_BAU=1 überspringt cargo (die Programme liegen schon in target/release,
# z. B. aus dem CI-Job). Das Fenster (egui) lädt Wayland/X11 und OpenGL zur
# Laufzeit; JACK kommt als libjack.so.0 vom System (pipewire-jack). Beides
# gehört deshalb nicht ins AppImage.
set -euo pipefail
cd "$(dirname "$0")/../.."
VERSION=${VERSION:-$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -n1)}
ARCH=${ARCH:-$(uname -m)}
APPIMAGETOOL=${APPIMAGETOOL:-appimagetool}
ID=de.weppelmann.Crosspoint

if [ -z "${OHNE_BAU:-}" ]; then
  cargo build --release --locked --features ui
fi

stufe=dist/stage/crosspoint
rm -rf dist/stage dist/Crosspoint.AppDir
mkdir -p dist

# --- Stufe: ein Baum unter usr/ --------------------------------------------
install -Dm755 target/release/crosspoint "$stufe/usr/bin/crosspoint"
install -Dm755 target/release/crosspoint-engine "$stufe/usr/bin/crosspoint-engine"
install -Dm755 scripts/crosspoint-launch.sh "$stufe/usr/bin/crosspoint-launch"
install -Dm755 scripts/crosspoint-wire.sh "$stufe/usr/bin/crosspoint-wire.sh"
install -Dm644 packaging/linux/$ID.desktop "$stufe/usr/share/applications/$ID.desktop"
install -Dm644 packaging/linux/$ID.metainfo.xml "$stufe/usr/share/metainfo/$ID.metainfo.xml"
for n in 16 32 48 64 128 256 512; do
  install -Dm644 "data/icons/hicolor/${n}x${n}/apps/$ID.png" \
    "$stufe/usr/share/icons/hicolor/${n}x${n}/apps/$ID.png"
done
install -Dm644 LICENSE "$stufe/usr/share/doc/crosspoint/LICENSE"
# IBM Plex ist im Fenster eingebettet (die OFL verlangt die Beilage).
install -Dm644 crates/fenster/schriften/OFL.txt "$stufe/usr/share/doc/crosspoint/IBM-Plex-OFL.txt"

# --- tar.gz: zum Auspacken ohne Paketverwaltung ----------------------------
tar -C "$stufe" -czf "dist/crosspoint-${VERSION}-linux-${ARCH}.tar.gz" usr

# --- AppImage: eine Datei, läuft auf jeder Distribution mit PipeWire -------
dir=dist/Crosspoint.AppDir
mkdir -p "$dir"
cp -a "$stufe/usr" "$dir/usr"
cp "packaging/linux/$ID.desktop" "$dir/$ID.desktop"
# Im AppImage startet der Eintrag das Fenster direkt (kein Launcher-Skript,
# das nach installierten Programmen sucht).
sed -i 's|^Exec=.*|Exec=crosspoint|' "$dir/$ID.desktop"
cp "data/icons/hicolor/256x256/apps/$ID.png" "$dir/$ID.png"
# Die Engine findet crosspoint-wire.sh neben dem Programm (usr/bin), der
# Autostart schreibt den Pfad aus $APPIMAGE.
cat > "$dir/AppRun" <<'APPRUN'
#!/bin/sh
HIER=$(dirname "$(readlink -f "$0")")
exec "$HIER/usr/bin/crosspoint" "$@"
APPRUN
chmod +x "$dir/AppRun"
ARCH=$ARCH "$APPIMAGETOOL" --no-appstream "$dir" "dist/Crosspoint-${VERSION}-${ARCH}.AppImage"

rm -rf dist/stage "$dir"
ls -la dist/
