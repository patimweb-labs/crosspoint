//! Crosspoint: das Fenster in egui, direkt auf dem laufenden Kern.
//!
//! Aufbau wie in sprechwerk: [`modell`] (was das Fenster weiß), [`logik`]
//! (Aktionen ausführen, reine Rechnungen), [`ansichten`] (`render(modell) →
//! Aktionen`), [`bausteine`] (gezeichnete Fader, Pegel, Schalter), [`thema`]
//! (drei Themes aus Tokens). [`betrieb`] startet Engine und Fenster in einem
//! Prozess, [`tray`] das Symbol im Benachrichtigungsbereich.

pub mod ansichten;
pub mod app;
pub mod bausteine;
pub mod betrieb;
pub mod fmt;
pub mod geraete;
pub mod konfig;
pub mod logik;
pub mod midimonster;
pub mod modell;
pub mod state;
pub mod thema;
pub mod tray;
pub mod vban;
