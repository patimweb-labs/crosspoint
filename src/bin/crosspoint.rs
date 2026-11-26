//! `crosspoint` — Engine und Fenster in **einem** Prozess.
//!
//! Gebaut nur mit Feature `ui` (`cargo build --release --features ui`). Hier ist
//! die Kompositionswurzel (ADR 0012): dieses Binary wählt den **Kern**
//! (`crosspoint::kern::LinuxKern`, PipeWire/JACK) und gibt ihn an das Fenster,
//! das nur die Naht aus `crosspoint-schnittstelle` kennt. Ein Windows- oder
//! macOS-Binary täte hier dasselbe mit seinem Kern – die Oberfläche bliebe
//! unverändert.
//!
//! Fenster zu → Engine und virtuelle Geraete weg; Engine zu → Fenster zu.

use std::sync::Arc;

use crosspoint::kern::LinuxKern;

/// Version des Wurzelpakets (AGENTS.md §6: die Zahl steht nur in der
/// `Cargo.toml` oben). Dieselbe geht an `betrieb::starten` und steht in der
/// DARSTELLUNG.
const VERSION: &str = env!("CARGO_PKG_VERSION");

fn main() {
    // `--version` endet hier: keine Instanzprüfung, kein JACK, kein Fenster.
    // Ein zweiter Lauf neben einer laufenden Instanz räumte sonst deren
    // Geräte ab.
    if crosspoint_fenster::betrieb::version_ausgeben(VERSION, std::env::args().skip(1)) {
        return;
    }
    let args = crosspoint_fenster::betrieb::args_lesen();

    // Läuft schon eine Instanz, zeigt sie ihr Fenster – eine zweite Engine
    // gäbe es sonst doppelt (gleicher Port, gleiche virtuelle Geräte).
    if crosspoint_fenster::betrieb::laufende_instanz_wecken(&args.listen, args.minimiert) {
        eprintln!("crosspoint: läuft schon – Fenster der laufenden Instanz");
        return;
    }

    // Den Kern bauen: Engine mit eigenen Threads für Audio, HTTP, OSC, Tape,
    // VBAN, Makros, Autosave.
    let mut lk = crosspoint::lauf::Konfig::vorgabe().dauerbetrieb();
    lk.client_name = args.client.clone();
    lk.listen = args.listen.clone();
    let lauf = match crosspoint::lauf::starte(lk) {
        Ok(l) => Arc::new(l),
        Err(e) => {
            // Kein Audio-Client: das Fenster zeigt die Fehlermeldung statt
            // eines Pults, das nicht rechnet.
            eprintln!("crosspoint: {e}");
            crosspoint_fenster::betrieb::ohne_engine(&args, &e);
            return;
        }
    };

    let kern = Arc::new(LinuxKern::neu(lauf));
    crosspoint_fenster::betrieb::starten("de.weppelmann.Crosspoint", VERSION, args, kern);
}
