//! Crosspoint-Modell: das Parametermodell ohne Plattform (ADR 0012).
//!
//! Was hier liegt, teilen sich alle Kerne: das Fenster (`crates/fenster`), der
//! Linux-Kern (`crates/core`) und später ein Kern für Windows oder macOS. Es
//! hängt an nichts außer `std` und `serde_json` – kein PipeWire, kein JACK,
//! kein HTTP, keine Pfade, keine Dateien.
//!
//! * [`params`] – `Params` mit Streifen und Bussen, die Filterkette, `set`,
//!   `to_json`
//! * [`pegel`] – die Pegelbank (der Audio-Thread schreibt, die Anzeige liest)
//! * [`json`] – die geordnete Wertform der Steuer-API
//! * [`einheiten`] – dB ↔ linear
//!
//! Was hier bewusst **nicht** liegt: DSP, Ports, Geräte, Dateiablage, Prozess.
//! Das ist Sache des Kerns.

pub mod einheiten;
pub mod json;
pub mod params;
pub mod pegel;

/// Streifen (Kanalzüge) im Pult.
pub const STRIPS: usize = 8;
/// Busse im Pult.
pub const BUSES: usize = 8;

/// Streifennamen wie im Original: 5 Hardware-Eingänge, 3 virtuelle.
pub const STRIP_NAMES: [&str; STRIPS] = [
    "HW In 1",
    "HW In 2",
    "HW In 3",
    "HW In 4",
    "HW In 5",
    "Virt In 1",
    "Virt In 2",
    "Virt In 3",
];

/// Busse: A1..A5 sind Hardware-Ausgänge, B1..B3 virtuelle (VAIO).
pub const BUS_NAMES: [&str; BUSES] = ["A1", "A2", "A3", "A4", "A5", "B1", "B2", "B3"];

/// Art eines Busses: echter Ausgang oder virtueller (VAIO).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum BusKind {
    /// Ausgang auf ein reales Gerät (im Original A1..A5).
    Hardware,
    /// Virtueller Ausgang (VAIO, im Original B1..B3).
    Virtual,
}

/// Art des Busses an dieser Stelle (A1..A5 Hardware, B1..B3 virtuell).
pub fn bus_kind(index: usize) -> BusKind {
    if index < 5 {
        BusKind::Hardware
    } else {
        BusKind::Virtual
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Acht Streifen, acht Busse, die Namen wie im Original: 5 Hardware,
    /// 3 virtuelle Eingänge; A1–A5 Hardware-Ausgänge, B1–B3 virtuell.
    /// (Stand bis ADR 0012 in `crates/core/src/lib.rs`.)
    #[test]
    fn acht_und_acht() {
        assert_eq!(STRIPS, 8);
        assert_eq!(BUSES, 8);
        assert_eq!(STRIP_NAMES[0], "HW In 1");
        assert_eq!(STRIP_NAMES[7], "Virt In 3");
        assert_eq!(BUS_NAMES[5], "B1");
        assert_eq!(bus_kind(0), BusKind::Hardware);
        assert_eq!(bus_kind(4), BusKind::Hardware);
        assert_eq!(bus_kind(5), BusKind::Virtual);
        assert_eq!(bus_kind(7), BusKind::Virtual);
    }
}
