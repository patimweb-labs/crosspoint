#!/usr/bin/env python3
"""Version und Datum im AppStream-Eintrag der metainfo aktuell halten.

Die **Version** schreibt release-please: `release-please-config.json` fuehrt die
Datei als `extra-files` mit einem xpath auf das erste `<release>`-Element. Ein
**Datum** kennt release-please nicht, der XML-Updater setzt nur die Version.
Damit die Paket-Metadaten trotzdem stimmen, kommt das Datum hier aus
`CHANGELOG.md` - dort traegt release-please es mit dem Release-PR ein, es ist
also derselbe Tag, den der Tag traegt. Bewusst kein "heute": ein Paket aus
demselben Tag enthaelt damit immer dieselbe Datei.

Aufruf aus dem Wurzelverzeichnis:
    packaging/linux/metainfo.py                 # Version und Datum setzen
    packaging/linux/metainfo.py --pruefen       # nur pruefen, Exit 1
    packaging/linux/metainfo.py --datum 2026-11-21

Geprueft wird in der CI (Job "Paket-Metadaten"): die Version muss zu
`Cargo.toml` passen. Weicht nur das Datum ab, gibt es eine Warnung statt eines
Fehlers - direkt nach dem Merge des Release-PR traegt die Datei die neue
Version noch mit dem alten Datum, bis der Release-Workflow (oder ein Lauf
dieses Skripts) es nachzieht. Die aelteren `<release>`-Eintraege bleiben
unangetastet.
"""

import argparse
import datetime
import re
import sys
from pathlib import Path

WURZEL = Path(__file__).resolve().parents[2]
METAINFO = WURZEL / "packaging/linux/de.weppelmann.Crosspoint.metainfo.xml"
CARGO = WURZEL / "Cargo.toml"
CHANGELOG = WURZEL / "CHANGELOG.md"

RELEASE = re.compile(r"<release\b[^>]*>")


def cargo_version() -> str:
    """Die Version, die auch die Pakete tragen: `Cargo.toml`."""
    treffer = re.search(r'^version = "([^"]+)"', CARGO.read_text("utf-8"), re.MULTILINE)
    if not treffer:
        sys.exit("keine Version in Cargo.toml")
    return treffer.group(1)


def changelog_datum(version: str) -> str | None:
    """Das Datum, das release-please in den Changelog-Eintrag schreibt."""
    text = CHANGELOG.read_text("utf-8")
    treffer = re.search(
        rf"^## \[{re.escape(version)}\]\([^)]*\) \((\d{{4}}-\d{{2}}-\d{{2}})\)",
        text,
        re.MULTILINE,
    )
    return treffer.group(1) if treffer else None


def setze_attribut(tag: str, name: str, wert: str) -> str:
    if re.search(rf'\b{name}="[^"]*"', tag):
        return re.sub(rf'\b{name}="[^"]*"', f'{name}="{wert}"', tag, count=1)
    # Attribut fehlt: vor dem Schliessen einfuegen, der Rest bleibt.
    return re.sub(r"\s*(/?>)$", f' {name}="{wert}"\\1', tag)


def setze_release(text: str, version: str, datum: str | None) -> str:
    treffer = RELEASE.search(text)
    if not treffer:
        sys.exit(f"kein <release>-Eintrag in {METAINFO.name}")
    tag = setze_attribut(treffer.group(0), "version", version)
    if datum:
        tag = setze_attribut(tag, "date", datum)
    return text[: treffer.start()] + tag + text[treffer.end():]


def main() -> int:
    p = argparse.ArgumentParser(description="Version und Datum in der metainfo halten")
    p.add_argument("--version", help="Vorgabe: Version aus Cargo.toml")
    p.add_argument("--datum", help="JJJJ-MM-TT, Vorgabe: Datum aus CHANGELOG.md")
    p.add_argument("--pruefen", action="store_true", help="nur pruefen, nichts schreiben")
    args = p.parse_args()

    version = args.version or cargo_version()
    datum = args.datum or changelog_datum(version)
    text = METAINFO.read_text("utf-8")

    treffer = RELEASE.search(text)
    if not treffer:
        print(f"metainfo: kein <release>-Eintrag in {METAINFO}", file=sys.stderr)
        return 1
    ist = re.search(r'\bversion="([^"]*)"', treffer.group(0))
    ist_datum = re.search(r'\bdate="([^"]*)"', treffer.group(0))

    if args.pruefen:
        fehler = False
        if not ist or ist.group(1) != version:
            print(
                f"metainfo: neuester Eintrag ist {ist.group(1) if ist else '?'}, "
                f"erwartet {version} (Cargo.toml)",
                file=sys.stderr,
            )
            fehler = True
        if datum and (not ist_datum or ist_datum.group(1) != datum):
            print(
                f"Warnung: metainfo-Datum ist "
                f"{ist_datum.group(1) if ist_datum else '?'}, im Changelog steht {datum}"
                " - der Release-Workflow zieht es beim Paketbau nach",
                file=sys.stderr,
            )
        if not datum:
            print(f"Hinweis: kein Changelog-Eintrag fuer {version}, Datum ungeprueft")
        return 1 if fehler else 0

    if not datum:
        print(f"Hinweis: kein Changelog-Eintrag fuer {version}, Datum bleibt stehen")
    neu = setze_release(text, version, datum)
    if neu != text:
        METAINFO.write_text(neu, encoding="utf-8")
        print(f"metainfo: {version} {datum or ''} geschrieben".rstrip())
    else:
        print(f"metainfo: schon aktuell ({version} {datum or ''})".rstrip())
    return 0


if __name__ == "__main__":
    sys.exit(main())
