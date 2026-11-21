#!/usr/bin/env python3
"""Cargo-Quellen für das Flatpak aus Cargo.lock erzeugen.

Dasselbe Format wie flatpak-cargo-generator (flatpak-builder-tools), aber
ohne dessen Abhängigkeiten (aiohttp, toml): je Paket aus crates.io ein Archiv
mit Prüfsumme und die `.cargo-checksum.json`, die `cargo vendor` erwartet.

Aufruf aus dem Wurzelverzeichnis:
    python3 packaging/flatpak/quellen.py > flatpak/new-sources.json
"""

import json
import sys
import tomllib


def quellen(lock: dict) -> list:
    aus = []
    for p in sorted(lock.get("package", []), key=lambda p: (p["name"], p["version"])):
        if not p.get("source", "").startswith("registry+") or "checksum" not in p:
            continue  # eigene Crates (Pfad) brauchen keine Quelle
        name, version, summe = p["name"], p["version"], p["checksum"]
        ziel = f"cargo/vendor/{name}-{version}"
        aus.append({
            "type": "archive",
            "archive-type": "tar-gzip",
            "url": f"https://static.crates.io/crates/{name}/{name}-{version}.crate",
            "sha256": summe,
            "dest": ziel,
        })
        aus.append({
            "type": "inline",
            "contents": json.dumps({"package": summe, "files": {}}),
            "dest": ziel,
            "dest-filename": ".cargo-checksum.json",
        })
    return aus


if __name__ == "__main__":
    with open(sys.argv[1] if len(sys.argv) > 1 else "Cargo.lock", "rb") as f:
        json.dump(quellen(tomllib.load(f)), sys.stdout, indent=4)
        sys.stdout.write("\n")
