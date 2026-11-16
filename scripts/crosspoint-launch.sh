#!/usr/bin/env bash
# crosspoint-launch.sh – Crosspoint auf dem Host starten.
#
# Reihenfolge:
#   1. Flatpak installiert   -> flatpak run de.weppelmann.Crosspoint
#   2. sonst natives Fenster -> `crosspoint` (GTK4, Engine im selben Prozess;
#      Fenster zu heisst Engine und virtuelle Geraete weg)
#   --minimized: nur Tray-Icon (Autostart-Eintrag) - `crosspoint --minimized`;
#      ist das native Fenster nicht gebaut, Engine plus `crosspoint-fenster`.
#
# Ein Browserfenster gibt es nicht mehr; die Seite auf http://127.0.0.1:8770/
# bleibt fuer Fernbedienung und Web-Oberflaeche erreichbar.
#
#   CROSSPOINT_UI           Pfad zum nativen Fenster (Vorgabe: neben dem Skript bzw. target/release)
#   CROSSPOINT_ENGINE       Pfad zur Engine fuer --minimized
#   CROSSPOINT_FENSTER      Pfad zum Tray-Fenster fuer --minimized
#   CROSSPOINT_HOST_WINDOW  =1: installiertes Flatpak ignorieren
set -uo pipefail

# --minimized: ohne Fenster starten, nur Tray-Icon (Autostart-Eintrag).
MINIMIERT=""
for argument in "$@"; do
  case "$argument" in
    --minimized|-m) MINIMIERT=1 ;;
    *) echo "crosspoint-launch: unbekanntes Argument: $argument" >&2; exit 2 ;;
  esac
done

HIER="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DIR="$(cd "$HIER/.." && pwd)"
URL="http://127.0.0.1:8770/"
LOG="${XDG_RUNTIME_DIR:-/tmp}/crosspoint-engine.log"

if [ -z "${CROSSPOINT_HOST_WINDOW:-}" ] && command -v flatpak >/dev/null 2>&1 \
   && flatpak info de.weppelmann.Crosspoint >/dev/null 2>&1; then
  exec flatpak run de.weppelmann.Crosspoint ${MINIMIERT:+--minimized}
fi

# Binaer suchen: ausdrueckliche Vorgabe, neben dem Skript, target/release.
binaer() {
  local name="$1" vorgabe="$2" kandidat
  if [ -n "$vorgabe" ] && [ -x "$vorgabe" ]; then echo "$vorgabe"; return; fi
  for kandidat in "$HIER/$name" "$DIR/target/release/$name"; do
    [ -x "$kandidat" ] && { echo "$kandidat"; return; }
  done
}

alive() { curl -s -o /dev/null --max-time 1 "$URL/api/state"; }

# Der Prozessname wird vom Kernel auf 15 Zeichen gekuerzt ("crosspoint-engi"),
# deshalb ueber die Kommandozeile suchen statt ueber comm.
ENGINE_MUSTER='(^|/)crosspoint-engine( |$)'
engine_pids() { pgrep -f "$ENGINE_MUSTER" 2>/dev/null; }

beende_engine() {
  pkill -f "$ENGINE_MUSTER" 2>/dev/null || true
  for _ in $(seq 1 30); do [ -z "$(engine_pids)" ] && break; sleep 0.2; done
  echo "Engine beendet, virtuelle Geraete entfernt" >&2
}

UI_BIN="$(binaer crosspoint "${CROSSPOINT_UI:-}")"

if [ -n "$MINIMIERT" ] && [ -n "$UI_BIN" ] && ! alive; then
  cd "$DIR" || exit 1
  exec "$UI_BIN" --minimized
fi

if [ -n "$MINIMIERT" ]; then
  FENSTER_BIN="$(binaer crosspoint-fenster "${CROSSPOINT_FENSTER:-}")"
  ENGINE_BIN="$(binaer crosspoint-engine "${CROSSPOINT_ENGINE:-}")"
  if [ -z "$ENGINE_BIN" ]; then
    echo "crosspoint-launch: crosspoint-engine nicht gebaut (cargo build --release)" >&2
    exit 1
  fi
  if ! alive; then
    # Ohne sichtbares Fenster haelt nichts die Engine am Leben (Leerlaufzeit).
    export CROSSPOINT_KEEP_RUNNING=1
    cd "$DIR" || exit 1
    setsid "$ENGINE_BIN" >"$LOG" 2>&1 &
    for _ in $(seq 1 60); do alive && break; sleep 0.25; done
  fi
  if ! alive; then
    echo "crosspoint-launch: Engine antwortet nicht – siehe $LOG" >&2
    exit 1
  fi
  if [ -z "$FENSTER_BIN" ]; then
    echo "crosspoint-launch: crosspoint-fenster ist nicht gebaut - Engine laeuft im Hintergrund" >&2
    exit 0
  fi
  echo "crosspoint-launch: minimiert - nur Tray-Icon ($FENSTER_BIN)" >&2
  "$FENSTER_BIN" --minimized
  STATUS=$?
  beende_engine
  exit "$STATUS"
fi

if [ -z "$UI_BIN" ]; then
  echo "crosspoint-launch: natives Fenster nicht gebaut - bitte: cargo build --release --features ui" >&2
  exit 1
fi
# Laeuft `crosspoint` schon (z. B. minimiert aus dem Autostart), reicht
# GApplication diesen Start an die laufende Instanz weiter: deren Fenster
# erscheint, eine zweite Engine entsteht nicht.
cd "$DIR" || exit 1
exec "$UI_BIN"
