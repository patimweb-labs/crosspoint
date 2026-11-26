//! Ablage: Presets, Autostart, Dateien. Was der Kern schreibt und liest.

use std::path::Path;
use std::sync::Arc;

use crosspoint_modell::params::Params;

/// Alles, was das Fenster vom Kern an Dateien erwartet.
///
/// Was ein Kern nicht kann (Autostart unter Windows sieht anders aus), darf er
/// hier ablehnen – das Fenster zeigt dann die Meldung, die der Kern zurückgibt.
pub trait Ablage: Send + Sync {
    /// Preset laden. `false` heißt „nicht lesbar“, nicht „nichts zu tun“.
    fn preset_laden(&self, params: &Arc<Params>, pfad: &Path) -> bool;
    /// Das vollständige Parameterbild als Preset schreiben.
    fn preset_speichern(&self, params: &Arc<Params>, pfad: &Path) -> Result<(), String>;

    /// Ist der Autostart eingetragen?
    fn autostart(&self) -> bool;
    /// Autostart ein- oder austragen.
    fn autostart_setzen(&self, an: bool) -> Result<(), String>;
}
