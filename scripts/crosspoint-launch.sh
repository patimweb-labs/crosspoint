#!/usr/bin/env bash
# crosspoint-launch.sh – Crosspoint auf dem Host starten.
#
# Reihenfolge:
#   1. Flatpak installiert -> flatpak run de.weppelmann.Crosspoint
#   2. Läuft schon eine Instanz (Steuer-API antwortet) -> sie zeigt ihr
#      Fenster (POST /api/fenster); eine zweite Engine entsteht nicht.
#   3. sonst `crosspoint` (egui-Fenster, Engine im selben Prozess; Fenster zu
#      heißt Engine und virtuelle Geräte weg).
#   --minimized: nur Tray-Icon (Autostart-Eintrag).
#
#   CROSSPOINT_UI           Pfad zum Programm (Vorgabe: neben dem Skript bzw. target/release)
#   CROSSPOINT_HOST_WINDOW  =1: installiertes Flatpak ignorieren
set -uo pipefail

MINIMIERT=""
for argument in "$@"; do
  case "$argument" in
    --minimized|-m) MINIMIERT=1 ;;
    *) echo "crosspoint-launch: unbekanntes Argument: $argument" >&2; exit 2 ;;
  esac
done

HIER="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DIR="$(cd "$HIER/.." && pwd)"
URL="http://127.0.0.1:8770"

if [ -z "${CROSSPOINT_HOST_WINDOW:-}" ] && command -v flatpak >/dev/null 2>&1 \
   && flatpak info de.weppelmann.Crosspoint >/dev/null 2>&1; then
  exec flatpak run de.weppelmann.Crosspoint ${MINIMIERT:+--minimized}
fi

# Zweiter Start: die laufende Instanz zeigt ihr Fenster (minimiert: nichts tun).
if curl -s -o /dev/null --max-time 1 "$URL/api/state"; then
  [ -n "$MINIMIERT" ] && exit 0
  curl -s -o /dev/null --max-time 2 -X POST "$URL/api/fenster" && exit 0
fi

# Programm suchen: ausdrückliche Vorgabe, neben dem Skript, target/release.
UI_BIN=""
for kandidat in "${CROSSPOINT_UI:-}" "$HIER/crosspoint" "$DIR/target/release/crosspoint"; do
  if [ -n "$kandidat" ] && [ -x "$kandidat" ]; then UI_BIN="$kandidat"; break; fi
done
if [ -z "$UI_BIN" ]; then
  echo "crosspoint-launch: crosspoint nicht gebaut - bitte: cargo build --release --features ui" >&2
  exit 1
fi
cd "$DIR" || exit 1
exec "$UI_BIN" ${MINIMIERT:+--minimized}
