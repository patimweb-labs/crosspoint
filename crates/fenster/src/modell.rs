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

/// Dialogfenster über dem Pult.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dialog {
    Presets,
    Vban,
    Midi,
    Darstellung,
}

impl Dialog {
    /// Alle Dialoge (für Tests und die Übersicht).
    pub const ALLE: [Dialog; 4] = [
        Dialog::Presets,
        Dialog::Vban,
        Dialog::Midi,
        Dialog::Darstellung,
    ];

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

/// Was gerade aufgeklappt ist: ein Menü am Zug oder das FILTER-Modal.
///
/// Der Zustand steht hier (nicht in der Ansicht), damit er sich als
/// [`Aktion::Aufklappen`] über `logik::ausfuehren` setzen lässt – auch von
/// außen (Start mit `--dialog`, Tests).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Aufklapp {
    /// Trim und Delay des Master-Busses (Popup am Knopf).
    TrimDelay,
    /// Modal „FILTER“ eines virtuellen Busses (B1–B3).
    Filter(usize),
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
    /// Blatt innerhalb der Seite (ohne Scrollen, [`crate::midimonster::BLATT_ZEILEN`]).
    pub blatt: usize,
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
    /// Version des Wurzelpakets (`CARGO_PKG_VERSION`), wie `--version` sie
    /// ausgibt. Sie kommt von außen (`betrieb::starten`), nicht aus diesem
    /// Crate: hier steht nur die Zahl, die in der DARSTELLUNG unten steht.
    /// `None` = unbekannt, dann bleibt die Zeile weg.
    pub version: Option<String>,
    pub thema: Thema,
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
    /// Seite der Presetliste (ohne Scrollen).
    pub preset_seite: usize,
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
            version: None,
            thema: Thema::default(),
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
            preset_seite: 0,
            vban: None,
            midi: None,
            autostart: false,
            // Leer; das Programm lädt die Auswahl beim Start (betrieb.rs),
            // Tests setzen sie selbst – kein Dateizugriff im Standardwert.
            echo_geraete: Vec::new(),
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
    /// Seite der Presetliste.
    PresetSeite(usize),
    PresetSpeichern,
    PresetLaden(std::path::PathBuf),
    Meldung(Meldung),
    VbanAnwenden,
    Autostart(bool),
    /// Lernen für ein Ziel starten oder abbrechen; gebundene Ziele werden gelöst.
    MidiLernen(String),
    MidiSeite(usize),
    /// Blatt innerhalb der MIDI-Seite.
    MidiBlatt(usize),
    MidiZuruecksetzen,
    MidiSpeichern,
    MidiLaden,
}
