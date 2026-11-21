//! Reine Logik der Oberfläche: Aktionen ausführen, Werte lesen, Maße rechnen.
//!
//! Alles hier läuft ohne Fenster und ist getestet. Die Ansichten rufen nur
//! Lesefunktionen auf und geben [`Aktion`]en zurück.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering::Relaxed};
use std::sync::Arc;
use std::time::{Duration, Instant};

use crosspoint::engine::{AusgangsFilter, Params, ERSTER_VIRTUELLER};
use crosspoint::macros_io::MacroControl;
use crosspoint::tape_io::TapeControl;
use crosspoint::vban_io::{VbanControl, VbanRecvControl};
use crosspoint::{BUSES, BUS_NAMES, STRIP_NAMES};

use crate::geraete::{self, Zuordner};
use crate::modell::{Aktion, Dichte, Meldung, Oberflaeche, Tape, Umbenennen, Ziel};
use crate::thema;

/// Was eine Aktion im Kern erreichen kann. In Tests sind Tape und Makros
/// leer, die Geräte gehen an einen Zuordner ohne PipeWire.
pub struct Kern {
    pub params: Arc<Params>,
    pub tape: Option<Arc<TapeControl>>,
    pub makros: Option<Arc<MacroControl>>,
    pub tape_ordner: PathBuf,
    pub preset_ordner: PathBuf,
    pub zuordner: Box<dyn Zuordner>,
    pub vban: Option<(Arc<VbanControl>, Arc<VbanRecvControl>)>,
    pub sample_rate: u32,
    /// Ablage der MIDIMonster-Karte.
    pub midi_pfad: PathBuf,
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
        Aktion::Dichte(d) => {
            o.dichte = d;
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
            if let Some(t) = &k.tape {
                t.recording.store(true, Relaxed);
                t.set_status(format!("rec [{}]", k.tape_ordner.display()));
            }
            o.tape = Tape::Aufnahme {
                seit: Instant::now(),
            };
            false
        }
        Aktion::TapeStop => {
            if let Some(t) = &k.tape {
                t.recording.store(false, Relaxed);
                t.set_status("stop");
            }
            o.tape = Tape::Bereit;
            false
        }
        Aktion::Makro(i) => {
            if let Some(m) = &k.makros {
                match m.press(i).and_then(|_| m.release(i)) {
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
                Ziel::Kanal(i) => k.zuordner.streifen(i + 1, name.as_deref()),
                Ziel::Bus(i) => k.zuordner.bus(BUS_NAMES[i], name.as_deref()),
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
            o.meldung = match k.zuordner.echo(&liste) {
                Ok(()) => {
                    o.echo_geraete = liste;
                    Meldung::Hinweis(format!(
                        "Echo-Referenz: {}{}",
                        geraete::kurzname(&name),
                        if an { " an" } else { " aus" }
                    ))
                }
                Err(e) => Meldung::Fehler(format!("Echo-Referenz: {e}")),
            };
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
                match crosspoint::restore::preset_speichern(&k.params, &pfad) {
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
            o.meldung = if crosspoint::restore::preset_laden(&k.params, &pfad) {
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
            o.meldung = match crosspoint::autostart::set(an) {
                Ok(_) => Meldung::Hinweis(if an { "Autostart an" } else { "Autostart aus" }.into()),
                Err(e) => Meldung::Fehler(format!("Autostart: {e}")),
            };
            o.autostart = crosspoint::autostart::enabled();
            false
        }
        Aktion::VbanAnwenden => {
            if let (Some(form), Some((send, recv))) = (&o.vban, &k.vban) {
                o.meldung = match form.anwenden(send, recv) {
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
                    let _ = crosspoint::osc::cc_kanal().abholen();
                    m.lernt = Some(ziel);
                    m.status = format!("{titel}: warte auf MIDI-CC …");
                }
            }
            false
        }
        Aktion::MidiSeite(s) => {
            if let Some(m) = o.midi.as_mut() {
                m.seite = s % crate::midimonster::SEITEN;
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
            if let Some((send, recv)) = &k.vban {
                let s = crate::vban::stand(send, recv, k.sample_rate);
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
            o.autostart = crosspoint::autostart::enabled();
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
    if let Some(cc) = crosspoint::osc::cc_kanal().letzte() {
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

/// Filterstand eines virtuellen Busses.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FilterWerte {
    pub gate: bool,
    pub gate_schwelle: f32,
    pub gate_tiefe: f32,
    pub gate_attack: f32,
    pub gate_hold: f32,
    pub gate_release: f32,
    pub aec: bool,
}

pub fn filter_werte(f: &AusgangsFilter) -> FilterWerte {
    let g = &f.gate;
    FilterWerte {
        gate: an(&g.on),
        gate_schwelle: fget(&g.threshold),
        gate_tiefe: fget(&g.tiefe),
        gate_attack: fget(&g.attack),
        gate_hold: fget(&g.hold),
        gate_release: fget(&g.release),
        aec: an(&f.aec),
    }
}

/// Filter des Busses hinter einem Zug (nur virtuelle Busse haben einen).
pub fn filter_von(p: &Params, ziel: Ziel, master_bus: usize) -> Option<FilterWerte> {
    let b = bus_von(ziel, master_bus)?;
    p.buses[b].filter.as_ref().map(filter_werte)
}

pub fn trim_delay(p: &Params, bus: usize) -> (f32, f32) {
    let b = &p.buses[bus];
    (fget(&b.trim[0]), fget(&b.delay_ms[0]))
}

// ---------------------------------------------------------------- Maße ----

/// Breite eines Zugs je Dichte (aus den Tokens).
pub fn zug_breite(d: Dichte) -> f32 {
    match d {
        Dichte::Kompakt => thema::SIZE_ZUG_KOMPAKT,
        Dichte::Normal => thema::SIZE_ZUG_BREITE,
        Dichte::Breit => thema::SIZE_ZUG_BREIT,
    }
}

/// Busse und Master sind schmaler als Kanäle (kein Pan, keine Zuordnung).
pub const BUS_ANTEIL: f32 = 0.82;

/// Breiten (Kanal, Bus) für eine verfügbare Breite: so breit, dass alle 17
/// Züge passen, nie schmaler als die Dichte vorgibt (dann scrollt das Pult).
pub fn zugbreiten(d: Dichte, verfuegbar: f32, luecken: f32) -> (f32, f32) {
    let einheiten = crosspoint::STRIPS as f32 + (BUSES + 1) as f32 * BUS_ANTEIL;
    let mindest = zug_breite(d);
    let kanal = ((verfuegbar - luecken) / einheiten).clamp(mindest, mindest * 1.4);
    (kanal, kanal * BUS_ANTEIL)
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
/// hält und sinkt um 0,45 dB je Takt (wie in der GTK-Oberfläche).
/// `roh` sind die linearen Spitzenwerte aus der Pegelbank.
pub fn pegel_ticken(p: &mut crate::modell::Pegel, roh: &[f32; crate::modell::ANZEIGEN]) {
    for (i, &wert) in roh.iter().enumerate() {
        let ziel = crate::fmt::lin_to_db(wert);
        let alt = p.db[i];
        let neu = if ziel > alt {
            alt + (ziel - alt) * 0.72
        } else {
            alt + (ziel - alt) * 0.22
        };
        p.db[i] = neu;
        p.spitze[i] = if neu > p.spitze[i] {
            neu
        } else {
            (p.spitze[i] - 0.45).max(crate::fmt::DB_MIN)
        };
    }
}

/// Rohpegel aller Anzeigen aus der Pegelbank; der Master zeigt seinen Bus.
pub fn pegel_lesen(
    bank: &crosspoint::pegel::Pegelbank,
    master_bus: usize,
) -> [f32; crate::modell::ANZEIGEN] {
    let mut r = [0.0; crate::modell::ANZEIGEN];
    for (i, w) in r.iter_mut().enumerate().take(8) {
        *w = bank.strip(i);
    }
    for b in 0..BUSES {
        r[8 + b] = bank.bus(b);
    }
    r[crate::modell::MASTER_ANZEIGE] = bank.bus(master_bus);
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
/// höchstens [`MAKRO_SEITEN`]. Seite `s` zeigt Feld `s·anzahl ..`.
pub fn makro_seiten(anzahl: usize) -> usize {
    (MAKRO_FELDER / anzahl.max(1)).clamp(1, MAKRO_SEITEN)
}

/// Erlaubte Anzahlen der Makrofelder; alles andere wird 5.
pub fn makro_anzahl_pruefen(n: usize) -> usize {
    if [5, 8, 10, 16, 40, 80].contains(&n) {
        n
    } else {
        5
    }
}

/// Spalten des Makrorasters.
pub fn makro_spalten(anzahl: usize) -> usize {
    match anzahl {
        16 | 40 => 8,
        80 => 10,
        _ => 5,
    }
}

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

    struct Merker(std::sync::Mutex<Vec<String>>);
    impl Zuordner for Merker {
        fn streifen(&self, strip: usize, q: Option<&str>) -> Result<(), String> {
            self.0.lock().unwrap().push(format!("s{strip}={q:?}"));
            Ok(())
        }
        fn bus(&self, bus: &str, s: Option<&str>) -> Result<(), String> {
            self.0.lock().unwrap().push(format!("{bus}={s:?}"));
            Ok(())
        }
        fn lautstaerke(&self, _: &str, _: Option<f32>, _: Option<bool>) -> Result<(), String> {
            Ok(())
        }
        fn echo(&self, geraete: &[String]) -> Result<(), String> {
            self.0
                .lock()
                .unwrap()
                .push(format!("echo={}", geraete.join(",")));
            Ok(())
        }
    }

    fn kern() -> Kern {
        Kern {
            params: Arc::new(Params::new(48_000.0, 512)),
            tape: None,
            makros: None,
            tape_ordner: PathBuf::from("/tmp"),
            preset_ordner: std::env::temp_dir().join(format!("cp-presets-{}", std::process::id())),
            zuordner: Box::new(Merker(Default::default())),
            vban: None,
            sample_rate: 48_000,
            midi_pfad: std::env::temp_dir().join(format!("cp-midi-{}.cfg", std::process::id())),
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
        k.params.set("strip", 2, "gain", 0.0);
        ausfuehren(Aktion::PresetLaden(liste[0].1.clone()), &mut o, &k);
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
        assert_eq!(makro_spalten(80), 10);
        assert_eq!(makro_seiten(5), 6);
        assert_eq!(makro_seiten(40), 2);
        assert_eq!(makro_seiten(80), 1);
        assert_eq!(makro_anzahl_pruefen(12), 5);
    }

    #[test]
    fn pegel_steigt_schnell_und_faellt_langsam() {
        let mut p = crate::modell::Pegel::default();
        let mut roh = [0.0f32; crate::modell::ANZEIGEN];
        roh[0] = 1.0; // 0 dB
        pegel_ticken(&mut p, &roh);
        assert!(p.db[0] > -20.0, "nach einem Takt {}", p.db[0]);
        for _ in 0..20 {
            pegel_ticken(&mut p, &roh);
        }
        assert!(p.db[0] > -0.1);
        roh[0] = 0.0;
        pegel_ticken(&mut p, &roh);
        assert!(p.db[0] < -10.0 && p.db[0] > -20.0, "Abfall {}", p.db[0]);
        assert!(
            (p.spitze[0] - (-0.45)).abs() < 0.2,
            "Spitze {}",
            p.spitze[0]
        );
        assert_eq!(p.db[1], crate::fmt::DB_MIN);
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
        assert!(zug_breite(Dichte::Kompakt) < zug_breite(Dichte::Breit));
        let (k, b) = zugbreiten(Dichte::Normal, 2540.0, 100.0);
        assert!(8.0 * k + 9.0 * b <= 2440.5, "passt nicht: {k} {b}");
        // Full HD: alle 17 Züge ohne Scrollen.
        let (k, b) = zugbreiten(Dichte::Normal, 1900.0, 100.0);
        assert!(8.0 * k + 9.0 * b <= 1800.5, "1920 px scrollt: {k} {b}");
        let (k, _) = zugbreiten(Dichte::Normal, 800.0, 100.0);
        assert_eq!(
            k,
            zug_breite(Dichte::Normal),
            "zu schmal: Mindestbreite, Pult scrollt"
        );
    }
}
