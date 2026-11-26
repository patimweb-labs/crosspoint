//! crosspoint — Linux-Kern des 8×8-Mixers (ADR 0012).
//!
//! Hier liegt, was die Plattform angeht: DSP, PipeWire/JACK-Anbindung, HTTP,
//! OSC, VBAN, Tape, Ablage und der Prozessstart. Das **Modell** (Parameters,
//! Filterkette, Pegelbank, JSON-Form) steht in `crosspoint-modell` und wird
//! hier nur wieder angeboten, damit `crosspoint::Params` & Co. bleiben, wo sie
//! waren.
//!
//! Aufbau der Signalkette: Rohsignal an den Eingängen (ADR 0002), Filterkette
//! an den virtuellen Bussen (ADR 0006) und am Kanalzug (ADR 0011). Die alten
//! Kanalzug-Typen des Referenzprogramms (EQ, Denoiser, Pitch) sind entfernt –
//! sie waren seit ADR 0002 nicht mehr in Gebrauch.

#![forbid(unsafe_op_in_unsafe_fn)]

/// Echounterdrückung für die virtuellen Ausgänge (B1–B3).
pub mod aec;
pub mod agc;
/// Geraete abraeumen: ein Sweep ohne Merkliste (Absturz, kill -9, doppelte Pfade).
pub mod aufraeumen;
pub mod autostart;
pub mod devices;
pub mod dsp;
/// Echo-Referenz: welche Ausgabegeräte spielen auf den Boxen?
pub mod echo;
pub mod engine;
pub mod http;
pub mod inputs;
pub mod jack;
/// Der Linux-Kern als Schnittstelle fürs Fenster (ADR 0012).
pub mod kern;
/// Start des ganzen Prozesses (Engine, Adapter, Aufpasser) an einer Stelle.
pub mod lauf;
pub mod macros_io;
/// OSC-Eingang (`/strip/<n>/<param>`) für MIDIMonster und Fader.
pub mod osc;
pub mod pfade;
/// PipeWire-Graph (`pw-dump`) als Grundlage der Geräteerkennung.
pub mod pw;
pub mod remote_api;
/// Zustand beim Start wiederherstellen, Änderungen selbsttätig sichern.
pub mod restore;
pub mod tape_io;
pub mod vban_io;
/// Wächter über abgesteckte Geräte: Zustand melden, Zuordnung behalten, heilen.
pub mod wache;

// Das Modell gehört dem eigenen Crate (ADR 0012) und steht hier weiter unter
// denselben Namen: `crosspoint::Params`, `crosspoint::STRIPS`, …
pub use crosspoint_modell::json;
pub use crosspoint_modell::pegel;
pub use crosspoint_modell::{bus_kind, BusKind, BUSES, BUS_NAMES, STRIPS, STRIP_NAMES};
