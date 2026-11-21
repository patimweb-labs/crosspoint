//! Das Modell der Oberfläche – alles, was das Fenster zeigt und nicht schon
//! im Kern (`Params`) steht.
//!
//! Der Mischzustand selbst (Fader, Mute, Routing, Filter) lebt **nur** in
//! `Params`: die Ansichten lesen ihn jedes Bild neu, und eine Bedienung wird
//! zur [`Aktion`], die `logik::ausfuehren` in den Kern schreibt. Hier steht,
//! was nur das Fenster betrifft: Theme, Dichte, offene Dialoge, Umbenennen,
//! Meldungen, Tape-Uhr, Makroseite, Gerätebestand.

use std::time::Instant;

use crate::geraete::Bestand;
use crate::thema::Thema;

/// Ein Zug im Pult.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Ziel {
    /// Eingangsstreifen 0..8.
    Kanal(usize),
    /// Bus 0..8 (A1..A5, B1..B3).
    Bus(usize),
    /// Master: bedient den gewählten Bus ([`Oberflaeche::master_bus`]).
    Master,
}

/// Dichte: Faktor auf die Zugbreite.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Dichte {
    Kompakt,
    #[default]
    Normal,
    Breit,
}

impl Dichte {
    pub const ALLE: [Dichte; 3] = [Dichte::Kompakt, Dichte::Normal, Dichte::Breit];

    pub fn name(self) -> &'static str {
        match self {
            Dichte::Kompakt => "kompakt",
            Dichte::Normal => "normal",
            Dichte::Breit => "breit",
        }
    }

    pub fn aus_name(name: &str) -> Dichte {
        match name {
            "kompakt" => Dichte::Kompakt,
            "breit" => Dichte::Breit,
            _ => Dichte::Normal,
        }
    }
}

/// Dialogfenster über dem Pult.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dialog {
    Presets,
    Vban,
    Midi,
    Darstellung,
}

impl Dialog {
    pub fn titel(self) -> &'static str {
        match self {
            Dialog::Presets => "PRESETS",
            Dialog::Vban => "VBAN",
            Dialog::Midi => "MIDI",
            Dialog::Darstellung => "DARSTELLUNG",
        }
    }

    pub fn aus_name(name: &str) -> Option<Dialog> {
        match name {
            "presets" => Some(Dialog::Presets),
            "vban" => Some(Dialog::Vban),
            "midi" => Some(Dialog::Midi),
            "darstellung" => Some(Dialog::Darstellung),
            _ => None,
        }
    }
}

/// Aufgeklapptes Menü an einem Zug.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Aufklapp {
    /// Noise Gate eines virtuellen Busses.
    Gate(usize),
    /// Echounterdrückung eines virtuellen Busses.
    Echo(usize),
    /// Trim und Delay des Master-Busses.
    TrimDelay,
}

/// Laufende Umbenennung eines Zugs.
#[derive(Debug, Clone, PartialEq)]
pub struct Umbenennen {
    pub ziel: Ziel,
    pub text: String,
}

/// Meldung in der Kopfzeile.
#[derive(Debug, Clone, Default, PartialEq)]
pub enum Meldung {
    #[default]
    Keine,
    Hinweis(String),
    Fehler(String),
}

/// Tape-Uhr: läuft nur während der Aufnahme.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub enum Tape {
    #[default]
    Bereit,
    Aufnahme {
        seit: Instant,
    },
}

/// Gerätebestand aus dem PipeWire-Graph (kommt aus einem Thread).
#[derive(Debug, Clone, Default)]
pub enum Geraete {
    #[default]
    Laedt,
    Fertig(Box<Bestand>),
}

/// Zustand des MIDI-Dialogs: die Karte wird hier bearbeitet und beim
/// Binden/Speichern geschrieben.
#[derive(Debug, Clone)]
pub struct MidiDialog {
    pub karte: crate::midimonster::Karte,
    pub seite: usize,
    /// Ziel, das gerade auf eine CC-Meldung wartet.
    pub lernt: Option<String>,
    /// MIDI-Anschlüsse des ALSA-Sequencers (beim Öffnen gelesen).
    pub geraete: Vec<String>,
    pub status: String,
}

/// Anzahl der Pegelanzeigen: 8 Kanäle, 8 Busse, Master.
pub const ANZEIGEN: usize = 17;
/// Index der Master-Anzeige.
pub const MASTER_ANZEIGE: usize = 16;

/// Pegelanzeige je Zug: geglätteter Wert und gehaltene Spitze (dB).
/// Gerechnet in `logik::pegel_ticken`, gezeichnet in den Zügen.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Pegel {
    pub db: [f32; ANZEIGEN],
    pub spitze: [f32; ANZEIGEN],
}

impl Default for Pegel {
    fn default() -> Self {
        Pegel {
            db: [crate::fmt::DB_MIN; ANZEIGEN],
            spitze: [crate::fmt::DB_MIN; ANZEIGEN],
        }
    }
}

impl Pegel {
    /// Index der Anzeige eines Zugs.
    pub fn index(ziel: Ziel) -> usize {
        match ziel {
            Ziel::Kanal(i) => i,
            Ziel::Bus(i) => 8 + i,
            Ziel::Master => MASTER_ANZEIGE,
        }
    }
}

/// Alles, was das Fenster neben `Params` weiß.
#[derive(Debug, Clone)]
pub struct Oberflaeche {
    pub thema: Thema,
    pub dichte: Dichte,
    /// Bus, den der Master-Zug bedient.
    pub master_bus: usize,
    pub dialog: Option<Dialog>,
    pub offen: Option<Aufklapp>,
    pub umbenennen: Option<Umbenennen>,
    pub meldung: Meldung,
    pub tape: Tape,
    pub makro_seite: usize,
    pub makros_eingeklappt: bool,
    /// Gezeigte Makrofelder (1..=80, Vorgabe 5).
    pub makro_anzahl: usize,
    /// Makrofelder, die gerade gedrückt aufleuchten (bis zum Zeitpunkt).
    pub makro_leuchtet: Vec<(usize, Instant)>,
    pub geraete: Geraete,
    pub pegel: Pegel,
    /// Eingabefeld „Preset speichern als“.
    pub preset_name: String,
    /// VBAN-Formular (beim Öffnen des Dialogs aus dem Kern gefüllt).
    pub vban: Option<crate::vban::Formular>,
    pub midi: Option<MidiDialog>,
    /// Autostart-Eintrag vorhanden (beim Öffnen der Darstellung gelesen).
    pub autostart: bool,
    /// Ausgabegeräte, deren Ton aus dem Mikrofon gerechnet wird
    /// (`echo.json` im Kern). Wird beim Laden des Bestands und nach jeder
    /// Änderung nachgeführt.
    pub echo_geraete: Vec<String>,
}

impl Default for Oberflaeche {
    fn default() -> Self {
        Oberflaeche {
            thema: Thema::default(),
            dichte: Dichte::default(),
            master_bus: 0,
            dialog: None,
            offen: None,
            umbenennen: None,
            meldung: Meldung::Keine,
            tape: Tape::Bereit,
            makro_seite: 0,
            makros_eingeklappt: false,
            makro_anzahl: 5,
            makro_leuchtet: Vec::new(),
            geraete: Geraete::Laedt,
            pegel: Pegel::default(),
            preset_name: String::new(),
            vban: None,
            midi: None,
            autostart: false,
            echo_geraete: crosspoint::echo::auswahl(),
        }
    }
}

/// Was eine Bedienung auslöst. Die Ansichten erzeugen nur Aktionen;
/// ausgeführt wird zentral (`logik::ausfuehren`).
#[derive(Debug, Clone, PartialEq)]
pub enum Aktion {
    /// Zahlen- oder Schalterwert eines Zugs (`Params::set`).
    Setzen {
        ziel: Ziel,
        param: String,
        wert: f32,
    },
    /// Kanal auf einen Bus legen oder davon nehmen.
    Routing {
        kanal: usize,
        bus: usize,
        an: bool,
    },
    UmbenennenBeginnen(Ziel),
    UmbenennenText(String),
    /// `true` übernimmt, `false` verwirft.
    UmbenennenEnde(bool),
    MasterBus(usize),
    Thema(Thema),
    Dichte(Dichte),
    Dialog(Option<Dialog>),
    Aufklappen(Option<Aufklapp>),
    TapeStart,
    TapeStop,
    Makro(usize),
    MakroSeite(usize),
    MakrosEinklappen(bool),
    MakroAnzahl(usize),
    /// Gerät zuordnen (`None` = keins).
    Geraet {
        ziel: Ziel,
        name: Option<String>,
    },
    /// Ausgabegerät als Echo-Referenz an- oder abwählen.
    EchoGeraet {
        name: String,
        an: bool,
    },
    PresetName(String),
    PresetSpeichern,
    PresetLaden(std::path::PathBuf),
    Meldung(Meldung),
    VbanAnwenden,
    Autostart(bool),
    /// Lernen für ein Ziel starten oder abbrechen; gebundene Ziele werden gelöst.
    MidiLernen(String),
    MidiSeite(usize),
    MidiZuruecksetzen,
    MidiSpeichern,
    MidiLaden,
}
