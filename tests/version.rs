//! `--version` beider Binärziele: eine Zeile, Exit 0 – und **nichts** startet.
//!
//! Geprüft werden die gebauten Programme (`CARGO_BIN_EXE_…`), nicht die App
//! selbst: ein zweiter Lauf neben einer laufenden Instanz würde deren Geräte
//! abräumen. Deshalb steht hier auch kein Test, der `crosspoint` ohne
//! `--version` aufruft – die Ausgabe kommt aus dem Wurzelpaket
//! (`CARGO_PKG_VERSION`), also derselben Zahl wie `crosspoint --version`.

use std::process::Command;

/// Was `--version` ausgeben muss: die Version des Wurzelpakets.
const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Startet ein Binary mit einem Argument und sammelt Exit-Code und Ausgaben.
///
/// Hinge der Aufruf in JACK oder der Instanzprüfung, käme hier nichts zurück –
/// der Test liefe in die Zeitgrenze. Genau das ist die Zusage: `--version`
/// endet vorher.
fn aufruf(bin: &str, arg: &str) -> (i32, String, String) {
    let aus = Command::new(bin)
        .arg(arg)
        .output()
        .unwrap_or_else(|e| panic!("{bin} {arg}: {e}"));
    (
        aus.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&aus.stdout).to_string(),
        String::from_utf8_lossy(&aus.stderr).to_string(),
    )
}

/// Prüft beide Schreibweisen (`--version`, `-V`) für ein Binary.
fn version_und_kurzform(bin: &str) {
    let (code, lang, fehler) = aufruf(bin, "--version");
    assert_eq!(code, 0, "{bin}: Exit {code}, stderr: {fehler}");
    assert_eq!(lang, format!("crosspoint {VERSION}\n"));
    // Nichts geredet, nichts gestartet: keine Meldung über Geräte oder JACK.
    assert!(fehler.is_empty(), "{bin}: stderr: {fehler}");

    let (code, kurz, _) = aufruf(bin, "-V");
    assert_eq!(code, 0, "{bin}: Exit {code} bei -V");
    assert_eq!(kurz, lang, "{bin}: -V und --version sagen nicht dasselbe");
}

#[test]
fn engine_nennt_die_version_und_startet_nicht() {
    version_und_kurzform(env!("CARGO_BIN_EXE_crosspoint-engine"));
    // `--version` steht in der Hilfe.
    let (code, hilfe, _) = aufruf(env!("CARGO_BIN_EXE_crosspoint-engine"), "--help");
    assert_eq!(code, 0);
    assert!(hilfe.contains("--version"), "{hilfe}");
}

#[cfg(feature = "ui")]
#[test]
fn oberflaeche_nennt_die_version_und_startet_nicht() {
    version_und_kurzform(env!("CARGO_BIN_EXE_crosspoint"));
    let (code, hilfe, _) = aufruf(env!("CARGO_BIN_EXE_crosspoint"), "--help");
    assert_eq!(code, 0);
    assert!(hilfe.contains("--version"), "{hilfe}");
}
