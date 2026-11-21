#!/usr/bin/env bash
# Erzeugt aus den SVG-Quellen in assets/ die Icons, die installiert werden
# (AGENTS.md §8, ADR 0008). Nicht von Hand ablegen: dieses Skript ist die
# einzige Brücke von der Zeichnung zu den PNGs.
#
# Aufruf aus dem Wurzelverzeichnis: packaging/icons/erzeugen.sh
# Braucht: ImageMagick mit librsvg (SVG-Delegate) *oder* rsvg-convert oder resvg.
#
# 16, 24 und 32 px kommen aus der vereinfachten Matrix (5x5), alles darüber aus
# dem vollen Zeichen; das SVG bleibt als scalable-Variante daneben liegen.
set -euo pipefail
cd "$(dirname "$0")/../.."

ID=de.weppelmann.Crosspoint
QUELLE=assets

# --- Zeichner wählen ------------------------------------------------------
# zeichne <svg> <kante> <ziel>          quadratisch, durchsichtiger Grund
# zeichne_breit <svg> <breite> <ziel> <grund>   volle Breite, farbiger Grund
if command -v resvg >/dev/null; then
  zeichne() { resvg --background '#00000000' --width "$2" --height "$2" "$1" "$3"; }
  zeichne_breit() { resvg --background "$4" --width "$2" "$1" "$3"; }
elif command -v rsvg-convert >/dev/null; then
  zeichne() { rsvg-convert --background-color='#00000000' --width "$2" --height "$2" -o "$3" "$1"; }
  zeichne_breit() { rsvg-convert --background-color="$4" --width "$2" -o "$3" "$1"; }
elif command -v magick >/dev/null; then
  zeichne() { magick -background none "$1" -resize "${2}x${2}" "PNG32:$3"; }
  zeichne_breit() { magick -background "$4" "$1" -resize "${2}x" -flatten "PNG32:$3"; }
else
  echo "Kein SVG-Zeichner gefunden: resvg, rsvg-convert oder ImageMagick installieren." >&2
  exit 1
fi

# --- PNGs -----------------------------------------------------------------
# Klein (vereinfachte Matrix), damit das Zeichen bei 16 px lesbar bleibt.
for n in 16 24 32; do
  mkdir -p "data/icons/hicolor/${n}x${n}/apps"
  zeichne "$QUELLE/crosspoint-klein.svg" "$n" \
    "data/icons/hicolor/${n}x${n}/apps/$ID.png"
done
for n in 48 64 128 256 512 1024; do
  mkdir -p "data/icons/hicolor/${n}x${n}/apps"
  zeichne "$QUELLE/crosspoint-logo.svg" "$n" \
    "data/icons/hicolor/${n}x${n}/apps/$ID.png"
done

# --- SVG: skalierbar, Zeichen für den Schreibtisch, Fenstersymbol ----------
install -Dm644 "$QUELLE/crosspoint-logo.svg" \
  "data/icons/hicolor/scalable/apps/$ID.svg"
install -Dm644 "$QUELLE/crosspoint-symbolic.svg" \
  "data/icons/symbolic/apps/$ID-symbolic.svg"
install -Dm644 "$QUELLE/crosspoint-logo.svg" assets/icon.svg

# --- Bilder für die Dokumentation ----------------------------------------
# Dieselben Zeichnungen, nur groß: das Icon und die Marke auf Nachthintergrund.
zeichne "$QUELLE/crosspoint-logo.svg" 1024 docs/brand/crosspoint-icon-1024.png
zeichne_breit "$QUELLE/crosspoint-wortmarke.svg" 1800 docs/brand/crosspoint-marke.png '#0A0C10'

echo "Icons erzeugt:"
find data/icons -name "$ID*" -printf '  %p %s Byte\n' | sort
