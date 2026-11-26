//! Was das Fenster vom Kern verlangt – und nichts mehr (ADR 0012, Stufe 2/3).
//!
//! Dieses Crate hängt nur an `crosspoint-modell`. Es beschreibt die Naht
//! zwischen Oberfläche und Kern: Geräte, Ablage (Presets, Autostart, MIDI-Datei),
//! Betrieb (läuft die Engine, Fensterwunsch, Herunterfahren) und die
//! Bedienhände für Band, Makros, Netz und MIDI-CC.
//!
//! Ein Kern für eine andere Plattform (Windows/WASAPI, macOS/CoreAudio)
//! implementiert dieselben Traits und bringt das Fenster ohne Änderung zum
//! Laufen. Der Linux-Kern (`crosspoint-core`) tut das in `kern.rs`.

pub mod ablage;
pub mod betrieb;
pub mod geraete;
pub mod haende;

use std::path::PathBuf;
use std::sync::Arc;

pub use ablage::Ablage;
pub use betrieb::{Betrieb, Cc};
pub use geraete::{
    auswahl_anwenden, zuordnung_gleich, Bestand, Eintrag, Geraete, Lautstaerke, Ziel, Zuordner,
    FEHLT_TIPP, KEIN_AUSGANG, KEIN_EINGANG, NICHT_VERBUNDEN,
};
pub use haende::{Band, Makros, Netz};

use crosspoint_modell::params::Params;
use crosspoint_modell::pegel::Pegelbank;

/// Der Kern, wie das Fenster ihn sieht.
///
/// Alles, was die Oberfläche braucht, ist hier versammelt: das Modell (Atome),
/// die Pegelbank, die Plattformteile als Traits und die Pfade, die das Fenster
/// für Anzeige und Ablage kennt.
pub trait Schnittstelle: Send + Sync {
    /// Das Parametermodell – derselbe Speicher, den der Audio-Thread liest.
    fn params(&self) -> &Arc<Params>;
    /// Die Pegelbank (der Audio-Thread schreibt, die Anzeige liest).
    fn pegel(&self) -> &Arc<Pegelbank>;
    /// Abtastrate, mit der der Kern rechnet.
    fn sample_rate(&self) -> u32;
    /// Ordner für Aufnahmen (Anzeige im Band-Kopf).
    fn tape_ordner(&self) -> PathBuf;
    /// Ordner für Presets.
    fn preset_ordner(&self) -> PathBuf;
    /// Datei, in der der Kern den Zustand selbsttätig sichert („letztes Preset“).
    fn preset_pfad(&self) -> PathBuf;
    /// Ablage der MIDIMonster-Karte.
    fn midi_pfad(&self) -> PathBuf;

    fn geraete(&self) -> &dyn Geraete;
    fn ablage(&self) -> &dyn Ablage;
    fn betrieb(&self) -> &dyn Betrieb;

    /// Band (Aufnahme/Wiedergabe), wenn der Kern eines hat.
    fn band(&self) -> Option<&dyn Band> {
        None
    }
    /// Makro-Engine, wenn vorhanden.
    fn makros(&self) -> Option<&dyn Makros> {
        None
    }
    /// Netz-Audio (VBAN), wenn vorhanden.
    fn netz(&self) -> Option<&dyn Netz> {
        None
    }
    /// MIDI-CC-Kanal (MIDIMonster über OSC), wenn vorhanden.
    fn cc(&self) -> Option<&dyn betrieb::MidiCc> {
        None
    }
}
