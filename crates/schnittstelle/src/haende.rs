//! Bedienhände: Band, Makros, Netz. Nur die Griffe, die das Fenster benutzt.

use crosspoint_modell::json::Value;

/// Aufnahme und Wiedergabe.
pub trait Band: Send + Sync {
    /// Aufnahme ein- oder ausschalten; `status` ist der Text für die Kopfzeile.
    fn aufnahme(&self, an: bool, status: &str);
    /// Was steht in der Kopfzeile?
    fn status(&self) -> String;
}

/// Die Makro-Engine.
pub trait Makros: Send + Sync {
    /// Ein Makrofeld drücken und wieder loslassen.
    fn druck(&self, index: usize) -> Result<(), String>;
}

/// Netz-Audio (VBAN): der Stand als JSON, wie ihn auch die Weboberfläche sieht.
pub trait Netz: Send + Sync {
    /// Aktuelle Einstellungen (senden/empfangen) als JSON.
    fn stand(&self, sample_rate: u32) -> Value;
    /// Einstellungen übernehmen.
    fn anwenden(&self, json: &Value) -> Result<(), String>;
}
