//! Betrieb: läuft der Kern, wie geht er aus, was will er vom Fenster.

/// Eine MIDI-CC-Meldung (Kanal, Controller, Wert) – so, wie sie aus OSC hereinkommt.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cc {
    pub kanal: u8,
    pub cc: u8,
    pub wert: u8,
}

/// Der MIDI-CC-Kanal (MIDIMonster schickt CCs über OSC an den Kern).
pub trait MidiCc: Send + Sync {
    /// Alle wartenden Meldungen abholen.
    fn abholen(&self) -> Vec<Cc>;
}

/// Was das Fenster über den Lauf des Kerns wissen muss.
pub trait Betrieb: Send + Sync {
    /// Rechnet der Kern noch? (Fenster zu, wenn nicht.)
    fn laeuft(&self) -> bool;
    /// Kern geordnet herunterfahren (Fenster zu).
    fn beenden(&self);
    /// Jemand will das Fenster sehen (`POST /api/fenster`, ein zweiter Start).
    fn fenster_gewuenscht(&self) -> bool;
    /// Ausgabegeräte, deren Ton aus dem Mikrofon gerechnet wird (Echo-Referenz).
    fn echo_auswahl(&self) -> Vec<String>;
}
