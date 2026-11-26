//! Reine Logik der Oberfläche: Aktionen ausführen, Werte lesen, Maße rechnen.
//!
//! Alles hier läuft ohne Fenster und ist getestet. Die Ansichten rufen nur
//! Lesefunktionen auf und geben [`Aktion`]en zurück.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering::Relaxed};
use std::sync::Arc;
use std::time::{Duration, Instant};

use crosspoint_modell::params::{AusgangsFilter, Params, ERSTER_VIRTUELLER};
use crosspoint_modell::{BUSES, BUS_NAMES, STRIP_NAMES};
use crosspoint_schnittstelle::ablage::Ablage;
use crosspoint_schnittstelle::betrieb::MidiCc;
use crosspoint_schnittstelle::geraete::Zuordner;

use crate::geraete;
use crate::modell::{Aktion, Geraete, Meldung, Oberflaeche, Tape, Umbenennen, Ziel};
use crate::thema;
use crosspoint_schnittstelle::haende::{Band, Makros, Netz};
use crosspoint_schnittstelle::{Betrieb, Schnittstelle};

/// Was das Fenster vom Kern hält (ADR 0012).
///
/// Nur noch das Modell und **eine** Naht: alles Plattformabhängige kommt über
/// [`Schnittstelle`] – Geräte, Ablage, Betrieb, Band, Makros, Netz, MIDI-CC.
/// Welcher Kern dahintersteht (heute der Linux-Kern, später Windows/macOS),
/// weiß die Oberfläche nicht.
pub struct Kern {
    pub params: Arc<Params>,
    pub tape_ordner: PathBuf,
    pub preset_ordner: PathBuf,
    pub sample_rate: u32,
    /// Ablage der MIDIMonster-Karte.
    pub midi_pfad: PathBuf,
    pub schnittstelle: Arc<dyn Schnittstelle>,
}

impl Kern {
    /// Band (Aufnahme/Wiedergabe), wenn der Kern eines hat.
    pub fn band(&self) -> Option<&dyn Band> {
        self.schnittstelle.band()
    }
    /// Makro-Engine, wenn vorhanden.
    pub fn makros(&self) -> Option<&dyn Makros> {
        self.schnittstelle.makros()
    }
    /// Netz-Audio (VBAN), wenn vorhanden.
    pub fn netz(&self) -> Option<&dyn Netz> {
        self.schnittstelle.netz()
    }
    /// Der Schreibpfad für Zuordnungen.
    pub fn zuordner(&self) -> &dyn Zuordner {
        self.schnittstelle.geraete().zuordner()
    }
    /// Dateien: Presets und Autostart.
    pub fn ablage(&self) -> &dyn Ablage {
        self.schnittstelle.ablage()
    }
    /// Läuft der Kern, will jemand das Fenster?
    pub fn betrieb(&self) -> &dyn Betrieb {
        self.schnittstelle.betrieb()
    }
    /// MIDI-CC-Kanal (MIDIMonster), wenn vorhanden.
    pub fn cc(&self) -> Option<&dyn MidiCc> {
        self.schnittstelle.cc()
    }
}

/// Wie lange ein gedrücktes Makrofeld aufleuchtet.
pub const MAKRO_LEUCHTEN: Duration = Duration::from_millis(140);

/// Ziel im Kern: (`"strip"`/`"bus"`, Index).
pub fn kern_ziel(ziel: Ziel, master_bus: usize) -> (&'static str, usize) {
    match ziel {
        Ziel::Kanal(i) => ("strip", i),
        Ziel::Bus(i) => ("bus", i),
        Ziel::Master => ("bus", master_bus),
    }
}

/// Führt eine Aktion aus. Liefert `true`, wenn sich Fenstereinstellungen
/// geändert haben (dann wird `fenster.json` geschrieben).
pub fn ausfuehren(a: Aktion, o: &mut Oberflaeche, k: &Kern) -> bool {
    match a {
        Aktion::Setzen { ziel, param, wert } => {
            let (t, i) = kern_ziel(ziel, o.master_bus);
            if !k.params.set(t, i, &param, wert) {
                o.meldung = Meldung::Fehler(format!("{param} an {} abgelehnt", name(ziel, o, k)));
            }
            false
        }
        Aktion::Routing { kanal, bus, an } => {
            crate::state::send_setzen(&k.params, kanal, bus, an);
            false
        }
        Aktion::UmbenennenBeginnen(ziel) => {
            o.umbenennen = Some(Umbenennen {
                ziel,
                text: name(ziel, o, k),
            });
            false
        }
        Aktion::UmbenennenText(text) => {
            if let Some(u) = o.umbenennen.as_mut() {
                u.text = text;
            }
            false
        }
        Aktion::UmbenennenEnde(uebernehmen) => {
            if let (true, Some(u)) = (uebernehmen, o.umbenennen.take()) {
                let (t, i) = kern_ziel(u.ziel, o.master_bus);
                if !k.params.set_text(t, i, "label", &u.text) {
                    o.meldung = Meldung::Fehler("Name zu lang oder ungültig".into());
                }
            }
            o.umbenennen = None;
            false
        }
        Aktion::MasterBus(i) => {
            o.master_bus = i.min(BUSES - 1);
            true
        }
        Aktion::Thema(t) => {
            o.thema = t;
            true
        }
        Aktion::Dialog(d) => {
            dialog_oeffnen(d, o, k);
            false
        }
        Aktion::Aufklappen(a) => {
            o.offen = a;
            false
        }
        Aktion::TapeStart => {
            if let Some(t) = k.band() {
                t.aufnahme(true, &format!("rec [{}]", k.tape_ordner.display()));
            }
            o.tape = Tape::Aufnahme {
                seit: Instant::now(),
            };
            false
        }
        Aktion::TapeStop => {
            if let Some(t) = k.band() {
                t.aufnahme(false, "stop");
            }
            o.tape = Tape::Bereit;
            false
        }
        Aktion::Makro(i) => {
            if let Some(m) = k.makros() {
                match m.druck(i) {
                    Ok(()) => o.meldung = Meldung::Hinweis(format!("Makro {} gesendet", i + 1)),
                    Err(e) => o.meldung = Meldung::Fehler(format!("Makro {}: {e}", i + 1)),
                }
            }
            o.makro_leuchtet.retain(|(n, _)| *n != i);
            o.makro_leuchtet.push((i, Instant::now() + MAKRO_LEUCHTEN));
            false
        }
        Aktion::MakroSeite(s) => {
            o.makro_seite = s.min(makro_seiten(o.makro_anzahl) - 1);
            false
        }
        Aktion::MakrosEinklappen(e) => {
            o.makros_eingeklappt = e;
            true
        }
        Aktion::MakroAnzahl(n) => {
            o.makro_anzahl = makro_anzahl_pruefen(n);
            o.makro_seite = o.makro_seite.min(makro_seiten(o.makro_anzahl) - 1);
            true
        }
        Aktion::Geraet { ziel, name } => {
            let ergebnis = match ziel {
                Ziel::Kanal(i) => k.zuordner().streifen(i + 1, name.as_deref()),
                Ziel::Bus(i) => k.zuordner().bus(BUS_NAMES[i], name.as_deref()),
                Ziel::Master => Ok(()),
            };
            o.meldung = match ergebnis {
                Ok(()) => Meldung::Hinweis(match &name {
                    Some(n) => format!("{} → {}", geraete::kurzname(n), zug_kurz(ziel)),
                    None => format!("{}: kein Gerät", zug_kurz(ziel)),
                }),
                Err(e) => Meldung::Fehler(e),
            };
            // Der Bestand wird neu gelesen (Thread im Fenster).
            o.geraete = crate::modell::Geraete::Laedt;
            false
        }
        Aktion::EchoGeraet { name, an } => {
            // Die Auswahl ist eine Liste von Ausgabegeraeten; an- und abwaehlen
            // aendert genau einen Eintrag, die anderen bleiben.
            let mut liste = o.echo_geraete.clone();
            liste.retain(|n| *n != name);
            if an {
                liste.push(name.clone());
            }
            o.meldung = match k.zuordner().echo(&liste) {
                Ok(()) => {
                    o.echo_geraete = liste;
                    Meldung::Hinweis(format!(
                        "Echo-Referenz: {}{}",
                        geraete_anzeige(&name, o),
                        if an { " an" } else { " aus" }
                    ))
                }
                Err(e) => Meldung::Fehler(format!("Echo-Referenz: {e}")),
            };
            false
        }
        Aktion::PresetSeite(s) => {
            o.preset_seite = s;
            false
        }
        Aktion::PresetName(n) => {
            o.preset_name = n;
            false
        }
        Aktion::PresetSpeichern => {
            let n = o.preset_name.trim().to_string();
            o.meldung = if !preset_name_gueltig(&n) {
                Meldung::Fehler("Bitte einen Namen ohne / eingeben".into())
            } else {
                let pfad = k.preset_ordner.join(format!("{n}.json"));
                match k.ablage().preset_speichern(&k.params, &pfad) {
                    Ok(()) => Meldung::Hinweis(format!("Preset '{n}' gespeichert")),
                    Err(e) => Meldung::Fehler(format!("Preset nicht gespeichert: {e}")),
                }
            };
            false
        }
        Aktion::PresetLaden(pfad) => {
            let n = pfad
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_default();
            o.meldung = if k.ablage().preset_laden(&k.params, &pfad) {
                Meldung::Hinweis(format!("Preset '{n}' geladen"))
            } else {
                Meldung::Fehler(format!("Preset '{n}' nicht ladbar"))
            };
            false
        }
        Aktion::Meldung(m) => {
            o.meldung = m;
            false
        }
        Aktion::Autostart(an) => {
            o.meldung = match k.ablage().autostart_setzen(an) {
                Ok(_) => Meldung::Hinweis(if an { "Autostart an" } else { "Autostart aus" }.into()),
                Err(e) => Meldung::Fehler(format!("Autostart: {e}")),
            };
            o.autostart = k.ablage().autostart();
            false
        }
        Aktion::VbanAnwenden => {
            if let (Some(form), Some(netz)) = (&o.vban, k.netz()) {
                o.meldung = match form.anwenden(netz) {
                    Ok(()) => Meldung::Hinweis("VBAN übernommen".into()),
                    Err(e) => Meldung::Fehler(format!("VBAN: {e}")),
                };
            }
            false
        }
        Aktion::MidiLernen(ziel) => {
            if let Some(m) = o.midi.as_mut() {
                let titel = m
                    .karte
                    .zeilen
                    .iter()
                    .find(|z| z.ziel == ziel)
                    .map(|z| z.titel.clone())
                    .unwrap_or_else(|| ziel.clone());
                let gebunden = m
                    .karte
                    .zeilen
                    .iter()
                    .any(|z| z.ziel == ziel && z.cc.is_some());
                if m.lernt.as_deref() == Some(ziel.as_str()) {
                    m.lernt = None;
                    m.status = "Lernmodus abgebrochen".into();
                } else if gebunden {
                    m.karte.loesen(&ziel);
                    m.status = format!("{titel}: Bindung gelöscht");
                    let _ = m.karte.speichern(&k.midi_pfad);
                } else {
                    // Was vor dem Klick kam, gilt nicht.
                    if let Some(cc) = k.cc() {
                        let _ = cc.abholen();
                    }
                    m.lernt = Some(ziel);
                    m.status = format!("{titel}: warte auf MIDI-CC …");
                }
            }
            false
        }
        Aktion::MidiSeite(s) => {
            if let Some(m) = o.midi.as_mut() {
                m.seite = s % crate::midimonster::SEITEN;
                m.blatt = 0;
            }
            false
        }
        Aktion::MidiBlatt(b) => {
            if let Some(m) = o.midi.as_mut() {
                m.blatt = b.min(crate::midimonster::seiten_blaetter(m.seite) - 1);
            }
            false
        }
        Aktion::MidiZuruecksetzen => {
            if let Some(m) = o.midi.as_mut() {
                m.karte.zuruecksetzen();
                m.lernt = None;
                m.status = "Karte zurückgesetzt (noch nicht gespeichert)".into();
            }
            false
        }
        Aktion::MidiSpeichern => {
            if let Some(m) = o.midi.as_mut() {
                m.status = match m.karte.speichern(&k.midi_pfad) {
                    Ok(()) => format!("gespeichert: {}", k.midi_pfad.display()),
                    Err(e) => format!("nicht gespeichert: {e}"),
                };
            }
            false
        }
        Aktion::MidiLaden => {
            if let Some(m) = o.midi.as_mut() {
                m.karte = crate::midimonster::Karte::laden(&k.midi_pfad);
                m.karte.alle();
                m.lernt = None;
                m.status = format!("geladen: {}", k.midi_pfad.display());
            }
            false
        }
    }
}

/// Dialog öffnen oder schließen; beim Öffnen wird sein Inhalt aus dem Kern
/// gelesen (VBAN-Formular, MIDI-Karte).
fn dialog_oeffnen(d: Option<crate::modell::Dialog>, o: &mut Oberflaeche, k: &Kern) {
    use crate::modell::Dialog;
    o.dialog = d;
    o.offen = None;
    o.vban = None;
    o.midi = None;
    match d {
        Some(Dialog::Vban) => {
            if let Some(netz) = k.netz() {
                let s = crate::vban::stand(netz, k.sample_rate);
                o.vban = Some(crate::vban::Formular {
                    senden_an: s.senden.an,
                    host: s.senden.host,
                    senden_port: s.senden.port as f64,
                    senden_stream: s.senden.stream,
                    empfangen_an: s.empfangen.an,
                    empfangen_port: s.empfangen.port as f64,
                    empfangen_stream: s.empfangen.stream,
                });
            }
        }
        Some(Dialog::Darstellung) => {
            o.autostart = k.ablage().autostart();
        }
        Some(Dialog::Midi) => {
            let mut karte = crate::midimonster::Karte::laden(&k.midi_pfad);
            karte.alle();
            let mut geraete = vec![crate::midimonster::KEIN_GERAET.to_string()];
            geraete.extend(crate::midimonster::midi_geraete());
            for n in [karte.eingang.clone(), karte.ausgang.clone()] {
                if !geraete.contains(&n) {
                    geraete.push(n);
                }
            }
            o.midi = Some(crate::modell::MidiDialog {
                karte,
                seite: 0,
                blatt: 0,
                lernt: None,
                geraete,
                status: String::new(),
            });
        }
        _ => {}
    }
}

/// Ein Takt des Fensters ohne Bedienung: Lernmodus prüfen (wartet auf eine
/// CC-Meldung aus dem OSC-Kanal) und abgelaufene Makroleuchten entfernen.
pub fn takt(o: &mut Oberflaeche, k: &Kern, jetzt: Instant) {
    o.makro_leuchtet.retain(|(_, bis)| *bis > jetzt);
    let Some(m) = o.midi.as_mut() else { return };
    let Some(ziel) = m.lernt.clone() else { return };
    if let Some(cc) = k.cc().and_then(|c| c.abholen().pop()) {
        m.karte.binden(&ziel, cc.cc);
        let _ = m.karte.speichern(&k.midi_pfad);
        m.lernt = None;
        m.status = format!(
            "{ziel}: CC {} übernommen (Kanal {}, Wert {})",
            cc.cc, cc.kanal, cc.wert
        );
    }
}

// ---------------------------------------------------------------- Lesen ----

fn fget(a: &AtomicU32) -> f32 {
    f32::from_bits(a.load(Relaxed))
}

fn an(a: &AtomicBool) -> bool {
    a.load(Relaxed)
}

/// Bus-Index hinter einem Zug (Master → gewählter Bus).
pub fn bus_von(ziel: Ziel, master_bus: usize) -> Option<usize> {
    match ziel {
        Ziel::Bus(i) => Some(i),
        Ziel::Master => Some(master_bus),
        Ziel::Kanal(_) => None,
    }
}

/// Vorgabename eines Zugs („HW In 1“, „A1“, „MAS A1“).
pub fn vorgabe_name(ziel: Ziel, master_bus: usize) -> String {
    match ziel {
        Ziel::Kanal(i) => STRIP_NAMES[i].to_string(),
        Ziel::Bus(i) => BUS_NAMES[i].to_string(),
        Ziel::Master => format!("MAS {}", BUS_NAMES[master_bus]),
    }
}

/// Anzeigename: eigener Name, sonst Vorgabe. Der Master trägt den Namen
/// seines Busses nicht – er heißt immer „MAS <Bus>“.
pub fn name(ziel: Ziel, o: &Oberflaeche, k: &Kern) -> String {
    if ziel == Ziel::Master {
        return vorgabe_name(ziel, o.master_bus);
    }
    let (t, i) = kern_ziel(ziel, o.master_bus);
    match k.params.label(t, i) {
        Some(l) if !l.is_empty() => l,
        _ => vorgabe_name(ziel, o.master_bus),
    }
}

/// Der gesetzte eigene Name eines Zugs (`None`, solange keiner gesetzt ist).
/// [`name`] liefert dagegen immer einen – notfalls die Vorgabe.
pub fn eigener_name(ziel: Ziel, o: &Oberflaeche, k: &Kern) -> Option<String> {
    if ziel == Ziel::Master {
        return None;
    }
    let (t, i) = kern_ziel(ziel, o.master_bus);
    k.params.label(t, i).filter(|l| !l.is_empty())
}

/// Anzeigename eines Geräts für Meldungen: die Beschreibung aus dem Bestand
/// („HDMI (AG326UD)“), sonst der technische Kurzname. Die Kennung allein
/// (`0000_03_00`) sagt niemandem etwas.
pub fn geraete_anzeige(name: &str, o: &Oberflaeche) -> String {
    if let Geraete::Fertig(b) = &o.geraete {
        if let Some(e) = b.senken.iter().find(|e| e.name == name) {
            return e.anzeige.clone();
        }
    }
    geraete::kurzname(name)
}

/// Kurzname für Meldungen.
pub fn zug_kurz(ziel: Ziel) -> String {
    match ziel {
        Ziel::Kanal(i) => format!("Kanal {}", i + 1),
        Ziel::Bus(i) => BUS_NAMES[i].to_string(),
        Ziel::Master => "Master".into(),
    }
}

/// Unterzeile im Zugkopf (`IN 1`, `VIRT 2`, `HW`, `VIRT`, `SUM`).
pub fn unterzeile(ziel: Ziel) -> String {
    match ziel {
        Ziel::Kanal(i) if i < 5 => format!("IN {}", i + 1),
        Ziel::Kanal(i) => format!("VIRT {}", i - 4),
        Ziel::Bus(i) if i < ERSTER_VIRTUELLER => "HW".into(),
        Ziel::Bus(_) => "VIRT".into(),
        Ziel::Master => "SUM".into(),
    }
}

/// Nummer rechts im Zugkopf.
pub fn nummer(ziel: Ziel) -> String {
    match ziel {
        Ziel::Kanal(i) | Ziel::Bus(i) => format!("{}", i + 1),
        // Beim Master steht „SUM“ schon in der Unterzeile.
        Ziel::Master => String::new(),
    }
}

/// Werte eines Zugs, wie die Ansicht sie zeigt.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ZugWerte {
    pub gain: f32,
    pub pan: Option<f32>,
    pub mute: bool,
    pub solo: Option<bool>,
    pub mono: bool,
}

pub fn zug_werte(p: &Params, ziel: Ziel, master_bus: usize) -> ZugWerte {
    match ziel {
        Ziel::Kanal(i) => {
            let s = &p.strips[i];
            ZugWerte {
                gain: fget(&s.gain),
                pan: Some(fget(&s.pan)),
                mute: an(&s.mute),
                solo: Some(an(&s.solo)),
                mono: an(&s.mono),
            }
        }
        Ziel::Bus(_) | Ziel::Master => {
            let b = &p.buses[bus_von(ziel, master_bus).unwrap_or(0)];
            ZugWerte {
                gain: fget(&b.gain),
                pan: None,
                mute: an(&b.mute),
                solo: None,
                mono: an(&b.mono),
            }
        }
    }
}

/// Filterstand eines Zugs (Kanalzug oder virtueller Bus) – alles, was das
/// FILTER-Modal zeigt und die Lämpchenreihe am Knopf zusammenfasst.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FilterWerte {
    pub gate: bool,
    pub gate_schwelle: f32,
    pub gate_tiefe: f32,
    pub gate_attack: f32,
    pub gate_hold: f32,
    pub gate_release: f32,
    /// Low-Cut (Hochpass) an und seine Grenze in Hz.
    pub lc: bool,
    pub lc_freq: f32,
    pub aec: bool,
    /// Rauschunterdrückung und ihre Stufe (0..3).
    pub nr: bool,
    pub nr_stufe: u32,
    pub comp: bool,
    pub comp_schwelle: f32,
    pub comp_ratio: f32,
    pub comp_attack: f32,
    pub comp_release: f32,
    pub comp_makeup: f32,
    pub limiter: bool,
    pub limiter_decke: f32,
    pub agc: bool,
    pub agc_ziel: f32,
    pub duck: bool,
    pub duck_ziel: usize,
    pub duck_tiefe: f32,
    pub duck_schwelle: f32,
    pub duck_attack: f32,
    pub duck_release: f32,
    pub ptt: bool,
    pub ptt_offen: bool,
}

/// Buchstabe eines Filters in der Lämpchenzeile: der erste; die Anfänge
/// sind alle verschieden (L E N G C A D P), der Test hält das fest.
pub fn filter_buchstabe(kuerzel: &str) -> char {
    kuerzel.chars().next().unwrap_or('?')
}

/// Die acht Lämpchen am FILTER-Knopf: Kürzel und ob der Filter an ist.
pub const LAEMPCHEN: [&str; 8] = ["LC", "ECHO", "NR", "GATE", "COMP", "AGC", "DUCK", "PTT"];

impl FilterWerte {
    /// Stand der acht Filter, in der Reihenfolge der Signalkette.
    pub fn laempchen(&self) -> [(&'static str, bool); 8] {
        [
            (LAEMPCHEN[0], self.lc),
            (LAEMPCHEN[1], self.aec),
            (LAEMPCHEN[2], self.nr),
            (LAEMPCHEN[3], self.gate),
            (LAEMPCHEN[4], self.comp),
            (LAEMPCHEN[5], self.agc),
            (LAEMPCHEN[6], self.duck),
            (LAEMPCHEN[7], self.ptt),
        ]
    }

    /// Ist irgendein Filter an? (Füllt den FILTER-Knopf.)
    pub fn irgendwas_an(&self) -> bool {
        self.laempchen().iter().any(|(_, an)| *an)
    }

    /// „LC an · ECHO aus · …“ für Tooltip und Screenreader.
    pub fn stand(&self) -> String {
        let teile: Vec<String> = self
            .laempchen()
            .iter()
            .map(|(k, an)| format!("{k} {}", if *an { "an" } else { "aus" }))
            .collect();
        teile.join(" · ")
    }
}

pub fn filter_werte(f: &AusgangsFilter) -> FilterWerte {
    let g = &f.gate;
    let c = &f.comp;
    let d = &f.duck;
    FilterWerte {
        gate: an(&g.on),
        gate_schwelle: fget(&g.threshold),
        gate_tiefe: fget(&g.tiefe),
        gate_attack: fget(&g.attack),
        gate_hold: fget(&g.hold),
        gate_release: fget(&g.release),
        lc: an(&f.lc.on),
        lc_freq: fget(&f.lc.freq),
        aec: an(&f.aec),
        nr: an(&f.nr.on),
        nr_stufe: f.nr.stufe.load(Relaxed),
        comp: an(&c.on),
        comp_schwelle: fget(&c.schwelle),
        comp_ratio: fget(&c.ratio),
        comp_attack: fget(&c.attack),
        comp_release: fget(&c.release),
        comp_makeup: fget(&c.makeup),
        limiter: an(&c.limiter),
        limiter_decke: fget(&c.decke),
        agc: an(&f.agc.on),
        agc_ziel: fget(&f.agc.ziel),
        duck: an(&d.on),
        duck_ziel: d.ziel.load(Relaxed) as usize,
        duck_tiefe: fget(&d.tiefe),
        duck_schwelle: fget(&d.schwelle),
        duck_attack: fget(&d.attack),
        duck_release: fget(&d.release),
        ptt: an(&f.ptt.on),
        ptt_offen: an(&f.ptt.offen),
    }
}

/// Pegel des Zugs **vor** den Filtern (linear) und danach (aus `Params`, wie
/// die Pegelanzeige im Pult).
///
/// Am Kanalzug ist „vorher“ der Pegel nach Mono und vor der Kette, am Bus die
/// Summe am Bus-Eingang.
pub fn filter_pegel(p: &Params, ziel: Ziel, master_bus: usize) -> (f32, f32) {
    match ziel {
        Ziel::Kanal(i) => match p.strips.get(i) {
            Some(s) => (fget(&s.roh), fget(&s.level)),
            None => (0.0, 0.0),
        },
        Ziel::Bus(_) | Ziel::Master => match bus_von(ziel, master_bus).and_then(|b| p.buses.get(b))
        {
            Some(b) => (fget(&b.roh), fget(&b.level)),
            None => (0.0, 0.0),
        },
    }
}

/// Filter des Zugs hinter einem Zug: jeder Kanalzug hat eine Kette, von den
/// Bussen nur die virtuellen B1–B3 (ADR 0002, ADR 0011).
pub fn filter_von(p: &Params, ziel: Ziel, master_bus: usize) -> Option<FilterWerte> {
    match ziel {
        Ziel::Kanal(i) => p.strips.get(i).map(|s| filter_werte(&s.filter)),
        Ziel::Bus(_) | Ziel::Master => {
            let b = bus_von(ziel, master_bus)?;
            p.buses[b].filter.as_ref().map(filter_werte)
        }
    }
}

pub fn trim_delay(p: &Params, bus: usize) -> (f32, f32) {
    let b = &p.buses[bus];
    (fget(&b.trim[0]), fget(&b.delay_ms[0]))
}

// ---------------------------------------------------------------- Maße ----
//
// Alle Maße, aus denen sich die Mindestgröße des Fensters ergibt, stehen
// hier: Darstellung (`ansichten`) und `mindestfenster` rechnen mit denselben
// Zahlen. Kein Scrollen, kein Abschneiden – was nicht passt, passt nicht ins
// Fenster, und das Fenster lässt sich nicht kleiner ziehen (ADR 0007).

/// Abstand zwischen den Gruppen (Kanäle | Busse | Master).
pub const GRUPPE: f32 = thema::SPACE_3;

/// Lücken zwischen den 17 Zügen: [`thema::SPACE_1`] zwischen zwei Zügen,
/// [`GRUPPE`] zwischen den Gruppen.
pub fn zugluecken() -> f32 {
    2.0 * GRUPPE + 16.0 * thema::SPACE_1
}

/// Busse und Master sind schmaler als Kanäle (kein Pan, keine Zuordnung).
pub const BUS_ANTEIL: f32 = 0.82;

/// Breiten (Kanal, Bus) für eine verfügbare Breite: die 17 Züge füllen sie
/// genau, ohne Untergrenze. Dass die Züge dabei nicht zu schmal werden, hält
/// die Mindestbreite des Fensters ([`mindestfenster`]) fest.
pub fn zugbreiten(verfuegbar: f32, luecken: f32) -> (f32, f32) {
    let einheiten = crosspoint_modell::STRIPS as f32 + (BUSES + 1) as f32 * BUS_ANTEIL;
    let kanal = ((verfuegbar - luecken) / einheiten).max(1.0);
    (kanal, kanal * BUS_ANTEIL)
}

/// Innenabstand in einem Zug.
pub const ZUG_RAND: f32 = thema::SPACE_1;
/// Höhe der Gerätewahl-Zeile eines Zugs.
pub const ZUG_GERAET: f32 = thema::SIZE_CONTROL_SM;
/// Höhe des Zugkopfs (Name, Nummer, Unterzeile).
pub const ZUG_KOPF: f32 = thema::SPACE_6 + thema::SPACE_2;
/// Höhe einer Schalterreihe (M, S, MONO, FILTER …).
pub const ZUG_REIHE: f32 = thema::SIZE_CONTROL_SM - thema::SPACE_1;
/// Höhe eines Zuordnungsknopfs (A1..B3).
pub const ZUG_ZUORDNUNG: f32 = thema::SIZE_CONTROL_SM - thema::SPACE_2;
/// Breite der Zuordnungsspalte links neben dem Fader eines Kanals: zwei
/// Zeichen in kleiner Schrift (15,6 px) und die Polsterung des Schalters –
/// nicht breiter, damit die Skala auch im schmalsten Kanal in der kleinen
/// Schrift steht.
pub const ZUG_ZUORDNUNG_BREITE: f32 = thema::SIZE_CONTROL_SM - thema::SPACE_1;

/// Oberkante des Zuordnungsknopfs `bus` in der Spalte, gemessen vom oberen
/// Rand des Faderblocks. Zwischen A5 und B1 steht eine Lücke mehr: oben die
/// Hardware-Busse, unten die virtuellen.
pub fn zuordnung_oben(bus: usize) -> f32 {
    let gruppe = if bus >= 5 { ZUG_RAND } else { 0.0 };
    bus as f32 * (ZUG_ZUORDNUNG + ZUG_RAND) + gruppe
}

/// Höhe der ganzen Zuordnungsspalte (A1..B3 mit Lücken).
pub fn zuordnung_hoehe() -> f32 {
    zuordnung_oben(BUSES - 1) + ZUG_ZUORDNUNG
}
/// Höhe der Lämpchenzeile am FILTER-Knopf.
pub const ZUG_LAEMPCHEN: f32 = thema::FONT_SIZE_SM + thema::SPACE_2;
/// Rahmenlinie eines Zugs (oben und unten je einmal).
pub const ZUG_RAHMEN: f32 = thema::SIZE_BORDER;

/// Wie viele Reihen Schalter ein Zug hat. Ein Kanalzug mit Filterkette hat
/// eine mehr als die übrigen: M/S, MONO, FILTER (ADR 0011). „FILTER“ allein
/// ist 54 px breit und passt in keinen halben Zug, also steht der Knopf auf
/// einer eigenen Reihe.
pub fn schalterreihen(filterbus: bool, kanal: bool) -> usize {
    if filterbus && kanal {
        3
    } else {
        2
    }
}

/// Was ein Zug über und unter dem Fader braucht; den Rest der Höhe bekommt
/// der Fader. `ansichten::zug` setzt die Zeilen damit untereinander, und
/// [`pult_mindesthoehe`] rechnet daraus die Mindesthöhe des Fensters.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ZugMass {
    /// Pan (nur Kanäle, sonst 0); die Zuordnung steht neben dem Fader.
    pub kanal_teil: f32,
    /// Reihen Schalter mit ihren Lücken.
    pub schalter: f32,
    /// Lämpchenzeile der Filterbusse und des Masters daran (sonst 0).
    pub lampen: f32,
}

impl ZugMass {
    /// `filterbus`: der Zug zeigt am FILTER-Knopf eine Lämpchenzeile.
    /// `kanal`: der Zug hat Pan (und neben dem Fader die Zuordnung A1..B3).
    pub fn fuer(filterbus: bool, kanal: bool) -> Self {
        ZugMass {
            kanal_teil: if kanal {
                thema::SIZE_PAN + thema::SPACE_5 + ZUG_RAND
            } else {
                0.0
            },
            schalter: schalterreihen(filterbus, kanal) as f32 * (ZUG_REIHE + ZUG_RAND),
            lampen: if filterbus { ZUG_LAEMPCHEN } else { 0.0 },
        }
    }

    /// Höhe vom oberen Rand bis zum Fader: Gerätewahl und Zugkopf.
    fn oben() -> f32 {
        ZUG_RAND + ZUG_GERAET + ZUG_RAND + ZUG_KOPF + ZUG_RAND
    }

    /// Alles, was im Zug nicht Fader ist (die Zeilen unter ihm inklusive
    /// ihrer Abstände).
    pub fn fest(&self) -> f32 {
        Self::oben() + ZUG_RAND + self.kanal_teil + self.schalter + self.lampen + ZUG_RAND
    }

    /// Höhe des Faderblocks in einem Zug dieser Höhe (Innenmaß ohne Rahmen).
    pub fn fader(&self, zug_hoehe: f32) -> f32 {
        zug_hoehe - self.fest()
    }
}

/// Länge der Faderbahn in einem Block dieser Höhe: der Rest sind der Rand
/// und die Zahlenzeile darunter.
pub fn fader_laenge(block_hoehe: f32) -> f32 {
    let werte = thema::FONT_SIZE_SM + thema::SPACE_1;
    (block_hoehe - 2.0 * thema::SPACE_1 - thema::SPACE_2 - werte - thema::SPACE_2).max(20.0)
}

/// Mindestabstand zweier Skalenzahlen in Schriftgrößen. Der engste Schritt
/// der Skala sind 6 dB (zwischen +12 und +6).
pub const SKALA_ABSTAND: f32 = 1.15;

/// Länge der Faderbahn, bei der alle neun Zahlen der Skala mit
/// [`SKALA_ABSTAND`] Abstand untereinander stehen.
pub fn fader_mindestlaenge() -> f32 {
    let spanne = crate::fmt::DB_MAX - crate::fmt::DB_MIN;
    SKALA_ABSTAND * thema::FONT_SIZE_SM * spanne / 6.0
}

/// Höhe des Faderblocks, bei der die Skalenzahlen nicht übereinander liegen.
pub fn fader_block_mindesthoehe() -> f32 {
    let werte = thema::FONT_SIZE_SM + thema::SPACE_1;
    fader_mindestlaenge() + 2.0 * thema::SPACE_1 + thema::SPACE_2 + werte + thema::SPACE_2
}

/// Kleinste Schrift der Faderskala: die Zahlen fallen nie weg, sie werden
/// höchstens kleiner (wie in der GTK-Oberfläche).
pub const SKALA_SCHRIFT_MIN: f32 = thema::FONT_SIZE_SM * 0.75;
/// Platz für eine Skalenzahl je Schriftgröße („-12“: drei Zeichen).
const SKALA_JE_SCHRIFT: f32 = 1.9;
/// Strich der Skala an der Bahn (der längste, für 0 dB).
pub const SKALA_STRICH: f32 = 5.0;

/// Was im Faderblock rechts von den Skalenzahlen steht: Bahn, Pegelsäule,
/// Striche und die Luft dazwischen.
fn fader_rechts() -> f32 {
    thema::SPACE_1
        + thema::SIZE_PEGEL_BREITE
        + thema::SPACE_1
        + thema::SIZE_FADER_BAHN
        + 2.0 * thema::SPACE_1
        + SKALA_STRICH
}

/// Platz für die Skalenzahlen in einem Faderblock dieser Breite.
pub fn fader_zahlenbreite(block_breite: f32) -> f32 {
    (block_breite - fader_rechts()).max(0.0)
}

/// Schrift der Skalenzahlen in einem Faderblock dieser Breite: die kleine
/// Schrift, wenn sie passt, sonst kleiner. Dass sie nicht unter
/// [`SKALA_SCHRIFT_MIN`] fällt, hält die Mindestbreite des Fensters fest.
pub fn fader_skala_schrift(block_breite: f32) -> f32 {
    (fader_zahlenbreite(block_breite) / SKALA_JE_SCHRIFT).min(thema::FONT_SIZE_SM)
}

/// Mindestbreite eines Faderblocks, damit die Zahlen in
/// [`SKALA_SCHRIFT_MIN`] stehen.
pub fn fader_block_mindestbreite() -> f32 {
    fader_rechts() + SKALA_JE_SCHRIFT * SKALA_SCHRIFT_MIN
}

/// Mindestbreite eines Kanals: links die Zuordnungsspalte
/// ([`ZUG_ZUORDNUNG_BREITE`]), rechts der Fader mit seiner Skala.
/// [`thema::SIZE_ZUG_KOMPAKT`] ist der kleinste Zug der Tokens und reicht für
/// beides (geprüft in `zuordnung_passt_neben_den_fader`).
pub fn kanal_mindestbreite() -> f32 {
    thema::SIZE_ZUG_KOMPAKT
}

/// Mindesthöhe des Pults: der höchste Zug, bei dem die Skala noch lesbar ist.
pub fn pult_mindesthoehe() -> f32 {
    let varianten = [
        ZugMass::fuer(true, true),   // Kanal mit Filterkette (ADR 0011)
        ZugMass::fuer(false, true),  // Kanal ohne Filter (alter Stand)
        ZugMass::fuer(true, false),  // virtueller Bus (Lämpchen am FILTER)
        ZugMass::fuer(false, false), // Bus ohne Filter (Master an A1..A5)
    ];
    varianten
        .iter()
        .map(|m| m.fest() + fader_block_mindesthoehe() + 2.0 * ZUG_RAHMEN)
        .fold(0.0, f32::max)
}

/// Zeilenhöhe einer Schriftzeile in egui: die Schrift plus Luft darüber und
/// darunter. Die Kopf- und Tapezeile bekommen ihre Höhe vom Inhalt.
const fn zeile(schrift: f32) -> f32 {
    schrift * 1.5
}

/// Höhe der Kopfleiste (Marke, Status und Tape) samt ihren Rändern.
pub const KOPF_HOEHE: f32 = 2.0 * thema::SPACE_2
    + (2.0 * thema::SIZE_BORDER + 2.0 * thema::SPACE_1 + thema::SIZE_CONTROL_MD + thema::SPACE_3)
    + thema::SPACE_2
    + (2.0 * thema::SIZE_BORDER + 2.0 * 3.0 + TAPE_ZEILE);

/// Höhe der Tapezeile: der höhere von Knopf und Laufzeit in großer Schrift.
const TAPE_ZEILE: f32 = if thema::SIZE_CONTROL_MD > zeile(thema::FONT_SIZE_LG) {
    thema::SIZE_CONTROL_MD
} else {
    zeile(thema::FONT_SIZE_LG)
};

/// Kante eines Makrofelds.
pub const MAKRO_FELD: f32 = thema::SPACE_6 * 3.0;

/// Kleinste Kante eines Makrofelds: reicht die Höhe auch dafür nicht, zeigt
/// die Leiste nur ihre Kopfzeile.
pub const MAKRO_FELD_MIN: f32 = thema::SPACE_6 * 2.0;

/// Wie die Makrofelder stehen: so viele nebeneinander, wie in die Breite
/// passen, alle Reihen (bis auf die letzte) gleich lang.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MakroRaster {
    pub spalten: usize,
    pub reihen: usize,
    pub kante: f32,
}

impl MakroRaster {
    pub fn hoehe(&self) -> f32 {
        self.reihen as f32 * self.kante + (self.reihen as f32 - 1.0) * thema::SPACE_2
    }
}

/// Raster für `anzahl` Felder auf `breite` × `hoehe` (Innenmaß der Leiste).
///
/// Erst wird die volle Kante [`MAKRO_FELD`] versucht; reicht die Höhe nicht,
/// werden die Felder in Schritten von [`thema::SPACE_1`] kleiner, bis
/// [`MAKRO_FELD_MIN`]. `None`: es passt nicht, die Leiste klappt zu. Die
/// Reihen werden gleichmäßig gefüllt – 16 Felder bei Platz für 15 stehen als
/// 8 + 8, nicht als 15 + 1.
pub fn makro_raster(anzahl: usize, breite: f32, hoehe: f32) -> Option<MakroRaster> {
    if anzahl == 0 {
        return None;
    }
    let mut kante = MAKRO_FELD;
    while kante >= MAKRO_FELD_MIN {
        let passt = (((breite + thema::SPACE_2) / (kante + thema::SPACE_2)) as usize).max(1);
        let reihen = anzahl.div_ceil(passt);
        let raster = MakroRaster {
            spalten: anzahl.div_ceil(reihen),
            reihen,
            kante,
        };
        if raster.hoehe() <= hoehe {
            return Some(raster);
        }
        kante -= thema::SPACE_1;
    }
    None
}

/// Höhe der Makroleiste ohne Raster: Kopfzeile, Rand und Rahmen.
pub fn makros_kopfhoehe() -> f32 {
    2.0 * thema::SPACE_2 + 2.0 * thema::SIZE_BORDER + 2.0 * thema::SPACE_2 + thema::SIZE_CONTROL_MD
}

/// Höhe, die das Raster bei dieser Fensterhöhe haben darf: was nach Kopf,
/// Mindesthöhe des Pults und Kopfzeile der Leiste übrig bleibt.
pub fn makro_rasterhoehe(fensterhoehe: f32) -> f32 {
    fensterhoehe - KOPF_HOEHE - pult_mindesthoehe() - makros_kopfhoehe() - thema::SPACE_1
}

/// Das Raster, das die Leiste zeigt – `None` heißt: nur die Kopfzeile, weil
/// der Nutzer eingeklappt hat oder der Platz nicht reicht. Der gespeicherte
/// Wunsch (`gewuenscht`) bleibt davon unberührt.
pub fn makro_raster_im_fenster(
    gewuenscht_eingeklappt: bool,
    anzahl: usize,
    breite: f32,
    fensterhoehe: f32,
) -> Option<MakroRaster> {
    if gewuenscht_eingeklappt {
        return None;
    }
    makro_raster(anzahl, breite, makro_rasterhoehe(fensterhoehe))
}

/// Mindestgröße des Fensters (Innenmaß): die 17 Züge nebeneinander und
/// untereinander so, dass alles zu sehen ist, dazu Kopf- und Makroleiste.
pub fn mindestfenster() -> egui::Vec2 {
    let kanal = kanal_mindestbreite();
    egui::Vec2::new(
        crosspoint_modell::STRIPS as f32 * kanal
            + (BUSES + 1) as f32 * kanal * BUS_ANTEIL
            + zugluecken()
            + 2.0 * thema::SPACE_2,
        // Zuschlag: die Zeilen runden sich auf ganze Punkte auf.
        KOPF_HOEHE + pult_mindesthoehe() + makros_kopfhoehe() + thema::SPACE_2,
    )
}

/// Was die Kopfleiste bei einer Breite zeigt; Marke, Status und Knöpfe
/// stehen immer da, Zusatzangaben fallen bei wenig Platz zuerst weg.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KopfTeile {
    pub meldung: bool,
    pub untertitel: bool,
    pub kennzahlen: bool,
}

pub fn kopf_teile(breite: f32) -> KopfTeile {
    KopfTeile {
        meldung: breite >= 1000.0,
        untertitel: breite >= 1250.0,
        kennzahlen: breite >= 1500.0,
    }
}

/// Skala der Fader: (dB, Beschriftung).
pub const SKALA: [(f32, &str); 9] = [
    (12.0, "+12"),
    (6.0, "+6"),
    (0.0, "0"),
    (-6.0, "-6"),
    (-12.0, "-12"),
    (-24.0, "-24"),
    (-36.0, "-36"),
    (-48.0, "-48"),
    (-60.0, "-60"),
];

/// Segmente der Pegelanzeige je Zug.
pub const PEGEL_SEGMENTE: usize = 26;

/// Farbstufe eines Pegelsegments.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stufe {
    Normal,
    Laut,
    Spitze,
}

pub fn segment_stufe(db: f32) -> Stufe {
    if db > -3.0 {
        Stufe::Spitze
    } else if db > -12.0 {
        Stufe::Laut
    } else {
        Stufe::Normal
    }
}

/// Faderwert aus einer Zugbewegung: Startwert + Weg (Bildschirm, nach unten
/// positiv), eingerastet auf 0 dB.
pub fn fader_ziehen(start_db: f32, weg_px: f32, laenge_px: f32, vorher: f32) -> f32 {
    if laenge_px <= 0.0 {
        return start_db;
    }
    let pos = crate::fmt::fader_pos(start_db) - weg_px / laenge_px;
    crate::fmt::einrasten(crate::fmt::pos_db(pos), vorher)
}

/// Pan-Text: `C`, `L40`, `R100`.
pub fn pan_text(pan: f32) -> String {
    if pan.abs() < 0.02 {
        "C".into()
    } else if pan < 0.0 {
        format!("L{:.0}", -pan * 100.0)
    } else {
        format!("R{:.0}", pan * 100.0)
    }
}

/// Ein Takt der Pegelanzeigen: Anstieg schnell, Abfall langsam, Spitze
/// hält und sinkt um 0,45 dB je Takt (wie in der GTK-Oberfläche) – links und
/// rechts getrennt. `roh` sind die linearen Spitzenwerte aus der Pegelbank.
pub fn pegel_ticken(p: &mut crate::modell::Pegel, roh: &[[f32; 2]; crate::modell::ANZEIGEN]) {
    for (i, paar) in roh.iter().enumerate() {
        for (k, &wert) in paar.iter().enumerate() {
            let ziel = crate::fmt::lin_to_db(wert);
            let alt = p.db[i][k];
            let neu = if ziel > alt {
                alt + (ziel - alt) * 0.72
            } else {
                alt + (ziel - alt) * 0.22
            };
            p.db[i][k] = neu;
            p.spitze[i][k] = if neu > p.spitze[i][k] {
                neu
            } else {
                (p.spitze[i][k] - 0.45).max(crate::fmt::DB_MIN)
            };
        }
    }
}

/// Rohpegel aller Anzeigen aus der Pegelbank (links, rechts); der Master
/// zeigt seinen Bus.
pub fn pegel_lesen(
    bank: &crosspoint_modell::pegel::Pegelbank,
    master_bus: usize,
) -> [[f32; 2]; crate::modell::ANZEIGEN] {
    let mut r = [[0.0; 2]; crate::modell::ANZEIGEN];
    for (i, w) in r.iter_mut().enumerate().take(8) {
        *w = bank.strip_lr(i);
    }
    for b in 0..BUSES {
        r[8 + b] = bank.bus_lr(b);
    }
    r[crate::modell::MASTER_ANZEIGE] = bank.bus_lr(master_bus);
    r
}

/// Laufzeit der Aufnahme `m:ss.mmm`.
pub fn tape_zeit(d: Duration) -> String {
    let ms = d.as_millis();
    format!("{}:{:02}.{:03}", ms / 60_000, (ms / 1000) % 60, ms % 1000)
}

pub fn tape_dauer(t: Tape, jetzt: Instant) -> Duration {
    match t {
        Tape::Bereit => Duration::ZERO,
        Tape::Aufnahme { seit } => jetzt.saturating_duration_since(seit),
    }
}

/// Makroseiten wie im Original.
pub const MAKRO_SEITEN: usize = 6;

/// Felder im Kern (`VBVMR_MACROBUTTON_NUMBER`).
pub const MAKRO_FELDER: usize = 80;

/// Seiten bei einer Felderzahl: so viele, wie in die 80 Felder passen,
/// höchstens [`MAKRO_SEITEN`]. Seite `s` zeigt Feld `s·anzahl ..`. Ohne
/// Felder (`0`) gibt es eine leere Seite.
pub fn makro_seiten(anzahl: usize) -> usize {
    if anzahl == 0 {
        return 1;
    }
    (MAKRO_FELDER / anzahl.max(1)).clamp(1, MAKRO_SEITEN)
}

/// Erlaubte Anzahlen der Makrofelder; `0` heißt: keine Makroleiste. Alles
/// andere wird 5.
pub fn makro_anzahl_pruefen(n: usize) -> usize {
    if MAKRO_ANZAHLEN.contains(&n) {
        n
    } else {
        5
    }
}

/// Wählbare Anzahlen der Makrofelder (DARSTELLUNG); `0` = keine Leiste.
pub const MAKRO_ANZAHLEN: [usize; 7] = [0, 5, 8, 10, 16, 40, 80];

/// Leuchtet ein Makrofeld gerade?
pub fn makro_leuchtet(o: &Oberflaeche, i: usize, jetzt: Instant) -> bool {
    o.makro_leuchtet
        .iter()
        .any(|(n, bis)| *n == i && *bis > jetzt)
}

/// Ein Presetname taugt als Dateiname.
pub fn preset_name_gueltig(n: &str) -> bool {
    !n.is_empty() && !n.contains(['/', '\\', '\0']) && n != "." && n != ".."
}

/// Zeilen der Presetliste je Seite: mehr passen im kleinsten Fenster nicht
/// hin, ohne zu scrollen (ADR 0007).
pub const PRESET_ZEILEN: usize = 10;

/// Seiten der Presetliste (mindestens eine, auch ohne Presets).
pub fn preset_seiten(anzahl: usize) -> usize {
    anzahl.div_ceil(PRESET_ZEILEN).max(1)
}

/// Nächstes oder voriges Blatt einer Liste mit `anzahl` Einträgen; am Rand
/// geht es vorn wieder los.
pub fn blaettern(blatt: usize, blaetter: usize, vor: bool) -> usize {
    let blaetter = blaetter.max(1);
    if vor {
        (blatt + 1) % blaetter
    } else {
        (blatt + blaetter - 1) % blaetter
    }
}

/// Nächstes Blatt im MIDI-Dialog – nur innerhalb des Abschnitts, am Rand
/// bleibt es stehen. Den Abschnitt wählt sein Reiter, nicht das Blättern.
pub fn midi_blaettern(seite: usize, blatt: usize, vor: bool) -> usize {
    let letztes = crate::midimonster::seiten_blaetter(seite) - 1;
    if vor {
        (blatt + 1).min(letztes)
    } else {
        blatt.saturating_sub(1)
    }
}

/// Presets im Ordner, nach Namen sortiert: (Name, Pfad).
pub fn presets(ordner: &Path) -> Vec<(String, PathBuf)> {
    let mut v: Vec<(String, PathBuf)> = std::fs::read_dir(ordner)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "json"))
        .filter_map(|p| Some((p.file_stem()?.to_str()?.to_string(), p)))
        .collect();
    v.sort();
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kern() -> Kern {
        let params = Arc::new(Params::new(48_000.0, 512));
        Kern {
            params: Arc::clone(&params),
            tape_ordner: PathBuf::from("/tmp"),
            preset_ordner: std::env::temp_dir().join(format!("cp-presets-{}", std::process::id())),
            sample_rate: 48_000,
            midi_pfad: std::env::temp_dir().join(format!("cp-midi-{}.cfg", std::process::id())),
            schnittstelle: Arc::new(crate::ohne_kern::OhneKern::neu(params)),
        }
    }

    #[test]
    fn master_schreibt_auf_den_gewaehlten_bus() {
        let k = kern();
        let mut o = Oberflaeche::default();
        ausfuehren(Aktion::MasterBus(6), &mut o, &k);
        let setzen = |param: &str, wert| Aktion::Setzen {
            ziel: Ziel::Master,
            param: param.into(),
            wert,
        };
        ausfuehren(setzen("gain", -9.0), &mut o, &k);
        assert_eq!(zug_werte(&k.params, Ziel::Bus(6), 0).gain, -9.0);
        assert_eq!(zug_werte(&k.params, Ziel::Bus(0), 0).gain, 0.0);
        // Der Master an B2 hat die Filter von B2.
        assert!(filter_von(&k.params, Ziel::Master, o.master_bus).is_some());
        ausfuehren(Aktion::MasterBus(0), &mut o, &k);
        assert!(filter_von(&k.params, Ziel::Master, o.master_bus).is_none());
    }

    #[test]
    fn abgelehnter_wert_wird_gemeldet() {
        let k = kern();
        let mut o = Oberflaeche::default();
        ausfuehren(
            Aktion::Setzen {
                ziel: Ziel::Bus(0),
                param: "gate".into(),
                wert: 1.0,
            },
            &mut o,
            &k,
        );
        assert!(matches!(o.meldung, Meldung::Fehler(_)));
    }

    #[test]
    fn umbenennen_uebernehmen_und_verwerfen() {
        let k = kern();
        let mut o = Oberflaeche::default();
        ausfuehren(Aktion::UmbenennenBeginnen(Ziel::Kanal(0)), &mut o, &k);
        assert_eq!(o.umbenennen.as_ref().unwrap().text, "HW In 1");
        ausfuehren(Aktion::UmbenennenText("Mikro".into()), &mut o, &k);
        ausfuehren(Aktion::UmbenennenEnde(true), &mut o, &k);
        assert_eq!(name(Ziel::Kanal(0), &o, &k), "Mikro");
        ausfuehren(Aktion::UmbenennenBeginnen(Ziel::Kanal(0)), &mut o, &k);
        ausfuehren(Aktion::UmbenennenText("Weg".into()), &mut o, &k);
        ausfuehren(Aktion::UmbenennenEnde(false), &mut o, &k);
        assert_eq!(name(Ziel::Kanal(0), &o, &k), "Mikro");
        assert!(o.umbenennen.is_none());
    }

    #[test]
    fn routing_und_geraete() {
        let k = kern();
        let mut o = Oberflaeche::default();
        ausfuehren(
            Aktion::Routing {
                kanal: 0,
                bus: 5,
                an: true,
            },
            &mut o,
            &k,
        );
        assert!(crate::state::send_an(&k.params, 0, 5));
        ausfuehren(
            Aktion::Geraet {
                ziel: Ziel::Kanal(1),
                name: Some("alsa_input.usb-Mikro.mono".into()),
            },
            &mut o,
            &k,
        );
        assert!(matches!(o.meldung, Meldung::Hinweis(ref t) if t.contains("Kanal 2")));
    }

    #[test]
    fn preset_speichern_und_laden() {
        let k = kern();
        let mut o = Oberflaeche::default();
        k.params.set("strip", 2, "gain", -4.5);
        ausfuehren(Aktion::PresetName("a/b".into()), &mut o, &k);
        ausfuehren(Aktion::PresetSpeichern, &mut o, &k);
        assert!(matches!(o.meldung, Meldung::Fehler(_)));
        ausfuehren(Aktion::PresetName("Abend".into()), &mut o, &k);
        ausfuehren(Aktion::PresetSpeichern, &mut o, &k);
        let liste = presets(&k.preset_ordner);
        assert_eq!(liste.len(), 1);
        // Geladen wird über den Kern: hier der echte (Testabhängigkeit auf
        // `crosspoint`), damit die geschriebene Datei auch geprüft wird.
        assert!(crosspoint::restore::preset_laden(&k.params, &liste[0].1));
        k.params.set("strip", 2, "gain", 0.0);
        assert!(crosspoint::restore::preset_laden(&k.params, &liste[0].1));
        assert_eq!(zug_werte(&k.params, Ziel::Kanal(2), 0).gain, -4.5);
        let _ = std::fs::remove_dir_all(&k.preset_ordner);
    }

    #[test]
    fn tape_uhr() {
        let k = kern();
        let mut o = Oberflaeche::default();
        ausfuehren(Aktion::TapeStart, &mut o, &k);
        assert!(matches!(o.tape, Tape::Aufnahme { .. }));
        ausfuehren(Aktion::TapeStop, &mut o, &k);
        assert_eq!(o.tape, Tape::Bereit);
        assert_eq!(tape_zeit(Duration::from_millis(83_045)), "1:23.045");
    }

    #[test]
    fn makros_leuchten_kurz() {
        let k = kern();
        let mut o = Oberflaeche::default();
        ausfuehren(Aktion::Makro(3), &mut o, &k);
        let jetzt = Instant::now();
        assert!(makro_leuchtet(&o, 3, jetzt));
        assert!(!makro_leuchtet(&o, 3, jetzt + MAKRO_LEUCHTEN * 2));
        assert_eq!(makro_seiten(5), 6);
        assert_eq!(makro_seiten(40), 2);
        assert_eq!(makro_seiten(80), 1);
        assert_eq!(makro_anzahl_pruefen(12), 5);
        assert_eq!(makro_anzahl_pruefen(0), 0);
    }

    #[test]
    fn pegel_steigt_schnell_und_faellt_langsam() {
        let mut p = crate::modell::Pegel::default();
        let mut roh = [[0.0f32; 2]; crate::modell::ANZEIGEN];
        roh[0] = [1.0, 0.0]; // links 0 dB, rechts still
        pegel_ticken(&mut p, &roh);
        assert!(p.db[0][0] > -20.0, "nach einem Takt {}", p.db[0][0]);
        for _ in 0..20 {
            pegel_ticken(&mut p, &roh);
        }
        assert!(p.db[0][0] > -0.1);
        assert_eq!(p.db[0][1], crate::fmt::DB_MIN, "rechts bleibt still");
        assert!(p.summe(0) > -0.1);
        roh[0] = [0.0, 0.0];
        pegel_ticken(&mut p, &roh);
        assert!(
            p.db[0][0] < -10.0 && p.db[0][0] > -20.0,
            "Abfall {}",
            p.db[0][0]
        );
        assert!(
            (p.spitze[0][0] - (-0.45)).abs() < 0.2,
            "Spitze {}",
            p.spitze[0][0]
        );
        assert_eq!(p.db[1], [crate::fmt::DB_MIN; 2]);
    }

    #[test]
    fn texte_und_rechnungen() {
        assert_eq!(pan_text(0.0), "C");
        assert_eq!(pan_text(-0.4), "L40");
        assert_eq!(pan_text(1.0), "R100");
        assert_eq!(unterzeile(Ziel::Kanal(6)), "VIRT 2");
        assert_eq!(unterzeile(Ziel::Bus(5)), "VIRT");
        assert_eq!(segment_stufe(-1.0), Stufe::Spitze);
        assert_eq!(segment_stufe(-20.0), Stufe::Normal);
        // Ganzer Weg nach oben (negativ, Bildschirm) = +12, nach unten -60,
        // ein kleiner Weg um 0 rastet ein.
        assert_eq!(fader_ziehen(0.0, -500.0, 500.0, 0.0), 12.0);
        assert_eq!(fader_ziehen(0.0, 500.0, 500.0, 0.0), -60.0);
        assert_eq!(fader_ziehen(0.0, 1.0, 500.0, 0.0), 0.0);
        // Lämpchenzeile: jeder Filter hat einen eigenen Buchstaben.
        let buchstaben: Vec<char> = LAEMPCHEN.iter().map(|k| filter_buchstabe(k)).collect();
        assert_eq!(buchstaben.iter().collect::<String>(), "LENGCADP");
        assert_eq!(
            kopf_teile(900.0),
            KopfTeile {
                meldung: false,
                untertitel: false,
                kennzahlen: false
            }
        );
        assert!(kopf_teile(1920.0).kennzahlen);
        // Die 17 Züge füllen die verfügbare Breite genau, ohne Klemme.
        for breite in [1400.0, 1920.0, 2560.0] {
            let luecken = zugluecken();
            let (k, b) = zugbreiten(breite, luecken);
            assert!(
                (8.0 * k + 9.0 * b + luecken - breite).abs() < 0.01,
                "{breite}: {k} {b} füllt nicht"
            );
            assert!((b - k * BUS_ANTEIL).abs() < 0.01);
        }
    }

    /// Die Mindestgröße des Fensters ist die Zusage „kein Scrollen, kein
    /// Abschneiden": Kanalbreite, Faderskala und Faderlänge müssen darin
    /// über ihren Grenzen liegen.
    #[test]
    fn mindestgroesse_haelt_alle_masse() {
        let min = mindestfenster();
        let luecken = zugluecken();
        let (kanal, bus) = zugbreiten(min.x - 2.0 * thema::SPACE_2, luecken);
        assert!(
            (kanal - kanal_mindestbreite()).abs() < 0.01,
            "Kanal {kanal} bei Mindestbreite"
        );
        assert!(8.0 * kanal + 9.0 * bus + luecken <= min.x - 2.0 * thema::SPACE_2 + 0.01);

        // Zuordnungsknöpfe A1..B3: vier nebeneinander unter dem Fader, „A1“
        // misst 15,6 px bei 13 px Schrift – zwei Pixel Luft müssen bleiben.
        let innen = kanal - 2.0 * ZUG_RAHMEN - 2.0 * ZUG_RAND;
        let knopf = (innen - 3.0 * ZUG_RAND) / 4.0;
        assert!(knopf >= 15.6 + 2.0, "Zuordnungsknopf {knopf} px");

        // Faderskala: in der Mindestbreite stehen die Zahlen in der kleinen
        // Schrift, niemals kleiner als SKALA_SCHRIFT_MIN (kein Ausdünnen).
        for breite in [bus, kanal] {
            let fader = breite - 2.0 * ZUG_RAHMEN - 2.0 * ZUG_RAND;
            assert!(fader >= fader_block_mindestbreite(), "Fader {fader} px");
            assert!(
                fader_skala_schrift(fader) >= thema::FONT_SIZE_SM,
                "Skala nur {:.1} px",
                fader_skala_schrift(fader)
            );
        }

        // Faderlänge: neun Zahlen mit 1,15 Schrift Abstand, engster Schritt
        // 6 dB (zwischen +12 und +6).
        for mass in [
            ZugMass::fuer(false, true),
            ZugMass::fuer(true, false),
            ZugMass::fuer(false, false),
        ] {
            let zug = pult_mindesthoehe() - 2.0 * ZUG_RAHMEN;
            let laenge = fader_laenge(mass.fader(zug));
            assert!(
                laenge >= fader_mindestlaenge(),
                "Faderlänge {laenge} px (mindestens {})",
                fader_mindestlaenge()
            );
            let sechs_db = laenge * 6.0 / (crate::fmt::DB_MAX - crate::fmt::DB_MIN);
            assert!(
                sechs_db >= SKALA_ABSTAND * thema::FONT_SIZE_SM,
                "+12 bis +6 nur {sechs_db:.1} px"
            );
        }

        // Ziel war 1280 × 720. In der Höhe ist es gehalten; die Breite geben
        // die 17 Züge vor (je mindestens kanal_mindestbreite) – schmaler
        // ginge nur, wenn A1..B3 oder die Skala verschwinden (ADR 0007).
        assert!(min.y <= 720.0, "Mindesthöhe {} px", min.y);
        assert!(
            min.x >= 8.0 * kanal + 9.0 * bus + luecken + 2.0 * thema::SPACE_2 - 0.01,
            "Mindestbreite {} px trägt die Züge nicht",
            min.x
        );
    }

    /// Listen in Dialogen kommen in Blättern, nicht in Scrollbereichen
    /// (ADR 0007): kein Blatt hat mehr Zeilen, als ins kleinste Fenster passen.
    #[test]
    fn dialoglisten_blaettern_statt_zu_scrollen() {
        use crate::midimonster::{seiten_blaetter, BLATT_ZEILEN, SEITEN};
        const { assert!(PRESET_ZEILEN >= 5 && PRESET_ZEILEN <= 12) };
        assert_eq!(preset_seiten(0), 1);
        assert_eq!(preset_seiten(PRESET_ZEILEN), 1);
        assert_eq!(preset_seiten(PRESET_ZEILEN + 1), 2);
        // Blättern läuft im Kreis, vorwärts wie rückwärts.
        assert_eq!(blaettern(0, 3, true), 1);
        assert_eq!(blaettern(2, 3, true), 0);
        assert_eq!(blaettern(0, 3, false), 2);
        assert_eq!(blaettern(2, 3, false), 1);
        // MIDI: die letzte Seite (Control Change 0..55) braucht mehrere
        // Blätter, jede Seite mindestens eins.
        for seite in 0..SEITEN {
            assert!(seiten_blaetter(seite) >= 1);
            let zeilen = crate::midimonster::seiten_ziele(seite).len();
            assert!(zeilen <= seiten_blaetter(seite) * BLATT_ZEILEN);
        }
        assert_eq!(seiten_blaetter(5), 6);
        // Blättern bleibt im Abschnitt und hält an seinen Rändern.
        assert_eq!(midi_blaettern(5, 5, true), 5);
        assert_eq!(midi_blaettern(5, 0, false), 0);
        assert_eq!(
            midi_blaettern(0, 0, true),
            0,
            "ein Blatt: nichts zu blättern"
        );
        assert_eq!(midi_blaettern(5, 1, true), 2);
        assert_eq!(midi_blaettern(5, 2, false), 1);
    }

    /// Feedback zu 2.0.0: in der Startgröße standen A1..B3 nur als „…“ da –
    /// vier Knöpfe in einer Reihe waren zu schmal für zwei Zeichen. Als
    /// Spalte neben dem Fader passen sie, ohne dem Fader die Skala zu nehmen.
    #[test]
    fn zuordnung_passt_neben_den_fader() {
        // Zwei Zeichen in kleiner Schrift (fett, Monospace ≈ 0,6 em je Zeichen).
        let zwei_zeichen = 2.0 * 0.6 * thema::FONT_SIZE_SM;
        assert!(ZUG_ZUORDNUNG_BREITE - 2.0 * thema::SPACE_1 >= zwei_zeichen);
        // Breite: Spalte, Lücke und Fader im schmalsten Kanal – die Skala
        // bleibt in der kleinen Schrift.
        let innen = kanal_mindestbreite() - 2.0 * ZUG_RAND;
        let fader = innen - ZUG_ZUORDNUNG_BREITE - ZUG_RAND;
        assert!(fader >= fader_block_mindestbreite());
        assert_eq!(fader_skala_schrift(fader), thema::FONT_SIZE_SM);
        // Höhe: die Spalte ist nie höher als der kleinste Faderblock.
        assert!(zuordnung_hoehe() <= fader_block_mindesthoehe());
        // Lücke zwischen A5 und B1.
        assert!(zuordnung_oben(5) - zuordnung_oben(4) > zuordnung_oben(4) - zuordnung_oben(3));
    }

    #[test]
    fn makroleiste_klappt_bei_knapper_hoehe_ein() {
        let min = mindestfenster();
        // Bei Mindesthöhe nur die Kopfzeile, eingeklappt bleibt eingeklappt.
        assert_eq!(makro_raster_im_fenster(false, 5, min.x, min.y), None);
        assert_eq!(makro_raster_im_fenster(true, 5, 1900.0, 1080.0), None);
        // Genug Platz: der Wunsch des Nutzers gilt.
        assert!(makro_raster_im_fenster(false, 5, 1900.0, 1080.0).is_some());
        // Keine Felder: keine Leiste.
        assert_eq!(makro_raster(0, 1900.0, 1000.0), None);
    }

    /// Feedback zu 2.0.0: 8 standen als 5 + 3, 10 als 5 + 5, 40 und 80 gar
    /// nicht. Jetzt so viele nebeneinander, wie hineinpassen.
    #[test]
    fn makrofelder_nutzen_die_breite() {
        let breite = 1580.0;
        let hoch = makro_rasterhoehe(1080.0);
        let r = |n| makro_raster(n, breite, hoch).expect("passt");
        let pro_reihe = ((breite + thema::SPACE_2) / (MAKRO_FELD + thema::SPACE_2)) as usize;
        assert!(pro_reihe >= 10, "Platz für {pro_reihe}");
        assert_eq!((r(5).spalten, r(5).reihen), (5, 1));
        assert_eq!((r(8).spalten, r(8).reihen), (8, 1));
        assert_eq!((r(10).spalten, r(10).reihen), (10, 1));
        // 16 passen nicht in eine Reihe: zwei gleich lange statt 15 + 1.
        assert_eq!((r(16).spalten, r(16).reihen), (8, 2));
        for n in [40, 80] {
            let raster = r(n);
            assert!(raster.spalten * raster.reihen >= n);
            assert!(raster.hoehe() <= hoch, "{n} Felder passen in Full HD");
            assert!(raster.kante >= MAKRO_FELD_MIN);
        }
        // Volle Kante, solange die Höhe reicht.
        assert_eq!(r(5).kante, MAKRO_FELD);
    }
}
