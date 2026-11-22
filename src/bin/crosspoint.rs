//! `crosspoint` — Engine und Fenster in **einem** Prozess.
//!
//! Gebaut nur mit Feature `ui` (`cargo build --release --features ui`). Das
//! Fenster ist egui (`crates/fenster`), der Ablauf steht in
//! `crosspoint_fenster::betrieb`.
//!
//! Fenster zu → Engine und virtuelle Geraete weg; Engine zu → Fenster zu.

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
    crosspoint_fenster::betrieb::starten("de.weppelmann.Crosspoint", VERSION, args);
}
