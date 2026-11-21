//! `crosspoint` — Engine und Fenster in **einem** Prozess.
//!
//! Gebaut nur mit Feature `ui` (`cargo build --release --features ui`). Das
//! Fenster ist egui (`crates/fenster`), der Ablauf steht in
//! `crosspoint_fenster::betrieb`.
//!
//! Fenster zu → Engine und virtuelle Geraete weg; Engine zu → Fenster zu.

fn main() {
    let args = crosspoint_fenster::betrieb::args_lesen();
    crosspoint_fenster::betrieb::starten("de.weppelmann.Crosspoint", args);
}
