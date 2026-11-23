# 0010 – macOS-Unterbau

Stand: 2026-11-23 · **verworfen** · Crosspoint bleibt bei Linux

## Kontext

Am 2026-11-21 war eine Mac-Version geplant: Core Audio statt PipeWire hinter
zwei Backend-Grenzen im Kern, ad hoc signiert, `.dmg` über GitHub-Releases.
Der vollständige Plan steht in der Git-Geschichte (`docs/plan-macos.md`, PR #10).

## Entscheidung

Keine Mac-Version, auch keine Windows-Version. Crosspoint bleibt bei Linux
(ADR 0004). Entscheidung des Nutzers am 2026-11-23.

## Gründe

- **Virtuelle Geräte** (B1–B3 als Mikrofon für andere Programme) sind der
  Kern eines VoiceMeeter-Ersatzes. Auf macOS braucht ein solches Gerät ein
  Plugin im `coreaudiod`, das ohne Apple-Signatur nicht zuverlässig lädt –
  und es gibt kein Entwicklerkonto. Der Ausweg BlackHole ist für Laien eine
  zusätzliche Installation mit fremden Gerätenamen.
- **Windows** ist schwerer: virtuelle Geräte brauchen einen signierten
  Kernel-Treiber (EV-Zertifikat), und das Vorbild Voicemeeter gibt es dort
  bereits (siehe `docs/architektur.md`).
- Eine zweite Plattform kostet dauerhaft doppelte Pflege bei jeder Änderung.

## Kosten

Keine. Der Linux-Code bleibt unverändert; es entsteht nichts, das gepflegt
werden müsste.
