//! Dialoge: VBAN-Konfiguration, MIDI-Mapping und "Darstellung".
//!
//! VBAN und MIDI lesen ihre Daten aus einem austauschbaren Trait; die Optik
//! kommt aus dem Token-Satz (CSS), damit ein Theme-Wechsel auch die offenen
//! Dialoge erfasst.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

use gtk::prelude::*;

use crate::layout::{Bereich, Dichte, MasterPlatz, Seite};
use crate::shell::Shell;
use crosspoint::engine::Params;

fn fenster_kopf(fenster: &gtk::Window, titel: &str, unter: &str) -> gtk::Widget {
    let kopf = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    kopf.add_css_class("dialogkopf");
    let block = gtk::Box::new(gtk::Orientation::Vertical, 1);
    let t = gtk::Label::new(Some(titel));
    t.add_css_class("dialogtitel");
    t.set_xalign(0.0);
    let u = gtk::Label::new(Some(unter));
    u.add_css_class("dialogunter");
    u.set_xalign(0.0);
    block.append(&t);
    block.append(&u);
    kopf.append(&block);
    let platz = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    platz.set_hexpand(true);
    kopf.append(&platz);
    let zu = gtk::Button::with_label("SCHLIESSEN");
    zu.add_css_class("tbtn");
    let f = fenster.clone();
    zu.connect_clicked(move |_| f.close());
    kopf.append(&zu);
    kopf.upcast()
}

// ---------------------------------------------------------------- VBAN ----

/// Eine Tabelle (Incoming oder Outgoing) als Raster; wird bei jedem Takt
/// neu gefuellt (hoechstens acht Zeilen).
/// Mindestbreiten der Tabellenspalten in Zeichen: beide Tabellen stehen so
/// buendig untereinander, auch wenn eine leer ist.
const VBAN_SPALTEN: [i32; 8] = [18, 15, 6, 3, 8, 8, 12, 6];

fn vban_tabelle(raster: &gtk::Grid, kopf: &[&str], zeilen: &[crate::vban::VbanZeile]) {
    while let Some(k) = raster.first_child() {
        raster.remove(&k);
    }
    for (x, t) in kopf.iter().enumerate() {
        let l = gtk::Label::new(Some(t));
        l.add_css_class("feld");
        l.set_xalign(0.0);
        l.set_width_chars(VBAN_SPALTEN.get(x).copied().unwrap_or(6));
        l.set_hexpand(x == 0);
        raster.attach(&l, x as i32, 0, 1, 1);
    }
    if zeilen.is_empty() {
        let l = gtk::Label::new(Some("—"));
        l.add_css_class("zeile");
        l.add_css_class("dim");
        l.set_xalign(0.0);
        raster.attach(&l, 0, 1, kopf.len() as i32, 1);
        return;
    }
    for (y, z) in zeilen.iter().enumerate() {
        let felder = [
            (z.stream.clone(), ""),
            (z.ip.clone(), "dim"),
            (z.sr.to_string(), "dim"),
            (z.kanaele.to_string(), "dim"),
            (z.format.clone(), "dim"),
            (z.quality.clone(), "dim"),
            (z.weg.clone(), ""),
            (
                z.fehler.to_string(),
                if z.fehler > 0 { "fehler" } else { "dim" },
            ),
        ];
        for (x, (text, klasse)) in felder.iter().enumerate() {
            let l = gtk::Label::new(Some(text));
            l.add_css_class("zeile");
            if !klasse.is_empty() {
                l.add_css_class(klasse);
            }
            l.set_xalign(0.0);
            l.set_ellipsize(gtk::pango::EllipsizeMode::End);
            l.set_hexpand(x == 0);
            raster.attach(&l, x as i32, y as i32 + 1, 1, 1);
        }
    }
}

/// Die Steuerungen des VBAN-Wegs, die der Dialog bedient.
#[derive(Clone)]
pub struct VbanAnbindung {
    pub send: std::sync::Arc<crosspoint::vban_io::VbanControl>,
    pub recv: std::sync::Arc<crosspoint::vban_io::VbanRecvControl>,
    pub sample_rate: u32,
}

/// VBAN-Fenster (`IP Audio Network Configuration`): Senden und Empfangen
/// einstellen, Incoming- und Outgoing-Tabelle, Reset/Load/Save Config.
///
/// Alles geht ueber dieselben Funktionen wie `/api/vban/config`.
pub fn vban_fenster(parent: &impl IsA<gtk::Window>, an: &VbanAnbindung) -> gtk::Window {
    let fenster = gtk::Window::builder()
        .title("Crosspoint — VBAN")
        .transient_for(parent)
        .modal(false)
        .default_width(1020)
        .default_height(520)
        .build();
    fenster.add_css_class("dialog");

    let stand = crate::vban::stand(&an.send, &an.recv, an.sample_rate);
    let aussen = gtk::Box::new(gtk::Orientation::Vertical, 0);
    let kopf = fenster_kopf(
        &fenster,
        "IP AUDIO NETWORK · VBAN",
        &crate::vban::unterzeile(&stand),
    );
    aussen.append(&kopf);

    let mitte = gtk::Box::new(gtk::Orientation::Vertical, 10);
    mitte.set_margin_top(10);
    mitte.set_margin_bottom(10);
    mitte.set_margin_start(10);
    mitte.set_margin_end(10);

    let feld = |text: &str| {
        let l = gtk::Label::new(Some(text));
        l.add_css_class("feld");
        l.set_xalign(0.0);
        l
    };
    let spalte = |titel: &str, w: &gtk::Widget| {
        let b = gtk::Box::new(gtk::Orientation::Vertical, 2);
        b.append(&feld(titel));
        b.append(w);
        b
    };
    let port_feld = |wert: u16| {
        let s = gtk::SpinButton::with_range(1.0, 65535.0, 1.0);
        s.set_value(f64::from(wert));
        s.set_numeric(true);
        s
    };

    // ----- Senden -----
    let senden_zeile = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    let senden_an = gtk::Switch::new();
    senden_an.set_halign(gtk::Align::Start);
    let host = gtk::Entry::new();
    host.set_width_chars(16);
    let senden_port = port_feld(stand.senden.port);
    let senden_stream = gtk::Entry::new();
    senden_stream.set_width_chars(16);
    senden_stream.set_max_length(16);
    let senden_status = feld("");
    senden_status.set_hexpand(true);
    senden_status.set_ellipsize(gtk::pango::EllipsizeMode::End);
    senden_zeile.append(&spalte("SENDEN (BUS A1)", senden_an.upcast_ref()));
    senden_zeile.append(&spalte("IP-ADRESSE", host.upcast_ref()));
    senden_zeile.append(&spalte("UDP-PORT", senden_port.upcast_ref()));
    senden_zeile.append(&spalte("STREAMNAME", senden_stream.upcast_ref()));
    senden_zeile.append(&spalte("STATUS", senden_status.upcast_ref()));
    mitte.append(&senden_zeile);

    // ----- Empfangen -----
    let empfangen_zeile = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    let empfangen_an = gtk::Switch::new();
    empfangen_an.set_halign(gtk::Align::Start);
    let empfangen_port = port_feld(stand.empfangen.port);
    let empfangen_stream = gtk::Entry::new();
    empfangen_stream.set_width_chars(16);
    empfangen_stream.set_max_length(16);
    empfangen_stream.set_placeholder_text(Some("alle Streams"));
    let empfangen_status = feld("");
    empfangen_status.set_hexpand(true);
    empfangen_status.set_ellipsize(gtk::pango::EllipsizeMode::End);
    empfangen_zeile.append(&spalte("EMPFANGEN (VBAN_L/R)", empfangen_an.upcast_ref()));
    empfangen_zeile.append(&spalte("UDP-PORT", empfangen_port.upcast_ref()));
    empfangen_zeile.append(&spalte("STREAMFILTER", empfangen_stream.upcast_ref()));
    empfangen_zeile.append(&spalte("STATUS", empfangen_status.upcast_ref()));
    mitte.append(&empfangen_zeile);

    // ----- Tabellen -----
    let tabellen_kopf_in = [
        "INCOMING STREAM",
        "IP",
        "SR",
        "CH",
        "FORMAT",
        "QUALITY",
        "DESTINATION",
        "ERRORS",
    ];
    let tabellen_kopf_out = [
        "OUTGOING STREAM",
        "IP",
        "SR",
        "CH",
        "FORMAT",
        "QUALITY",
        "SOURCE",
        "ERRORS",
    ];
    let raster = || {
        let g = gtk::Grid::new();
        g.set_column_spacing(12);
        g.set_row_spacing(2);
        g.add_css_class("tabelle");
        g
    };
    let incoming = raster();
    let outgoing = raster();
    let tabellen = gtk::Box::new(gtk::Orientation::Vertical, 12);
    tabellen.append(&incoming);
    tabellen.append(&outgoing);
    let scroll = gtk::ScrolledWindow::new();
    scroll.set_child(Some(&tabellen));
    scroll.set_vexpand(true);
    scroll.set_policy(gtk::PolicyType::Automatic, gtk::PolicyType::Automatic);
    mitte.append(&scroll);
    aussen.append(&mitte);

    // ----- Fuss -----
    let fuss = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    fuss.add_css_class("dialogfuss");
    let meldung = feld("");
    meldung.set_hexpand(true);
    meldung.set_ellipsize(gtk::pango::EllipsizeMode::End);
    fuss.append(&meldung);
    let knopf = |t: &str| {
        let b = gtk::Button::with_label(t);
        b.add_css_class("tbtn");
        b
    };
    let uebernehmen = knopf("UEBERNEHMEN");
    let reset = knopf("RESET");
    let laden = knopf("LOAD CONFIG");
    let speichern = knopf("SAVE CONFIG");
    fuss.append(&uebernehmen);
    fuss.append(&reset);
    fuss.append(&laden);
    fuss.append(&speichern);
    aussen.append(&fuss);

    // Formular aus dem Kern fuellen (beim Oeffnen und nach Reset/Load) -
    // nicht im Takt, sonst ueberschriebe der Takt das gerade Getippte.
    // `fuellt` haelt die Schalter still, solange das Programm sie setzt.
    let fuellt = Rc::new(Cell::new(false));
    let formular_fuellen: Rc<dyn Fn()> = {
        let an = an.clone();
        let fuellt = Rc::clone(&fuellt);
        let (senden_an, host, senden_port, senden_stream) = (
            senden_an.clone(),
            host.clone(),
            senden_port.clone(),
            senden_stream.clone(),
        );
        let (empfangen_an, empfangen_port, empfangen_stream) = (
            empfangen_an.clone(),
            empfangen_port.clone(),
            empfangen_stream.clone(),
        );
        Rc::new(move || {
            fuellt.set(true);
            let s = crate::vban::stand(&an.send, &an.recv, an.sample_rate);
            senden_an.set_active(s.senden.an);
            host.set_text(&s.senden.host);
            senden_port.set_value(f64::from(s.senden.port));
            senden_stream.set_text(&s.senden.stream);
            empfangen_an.set_active(s.empfangen.an);
            empfangen_port.set_value(f64::from(s.empfangen.port));
            empfangen_stream.set_text(&s.empfangen.stream);
            fuellt.set(false);
        })
    };
    formular_fuellen();

    let formular = {
        let (senden_an, host, senden_port, senden_stream) = (
            senden_an.clone(),
            host.clone(),
            senden_port.clone(),
            senden_stream.clone(),
        );
        let (empfangen_an, empfangen_port, empfangen_stream) = (
            empfangen_an.clone(),
            empfangen_port.clone(),
            empfangen_stream.clone(),
        );
        move || crate::vban::Formular {
            senden_an: senden_an.is_active(),
            host: host.text().to_string(),
            senden_port: senden_port.value(),
            senden_stream: senden_stream.text().to_string(),
            empfangen_an: empfangen_an.is_active(),
            empfangen_port: empfangen_port.value(),
            empfangen_stream: empfangen_stream.text().to_string(),
        }
    };
    let formular = Rc::new(formular);

    // Anzeige im Takt: Status, Tabellen, Unterzeile.
    let anzeigen: Rc<dyn Fn()> = {
        let an = an.clone();
        let kopf = kopf.clone();
        Rc::new(move || {
            let s = crate::vban::stand(&an.send, &an.recv, an.sample_rate);
            let mit_fehler = |t: String, f: &str| {
                if f.is_empty() {
                    t
                } else {
                    format!("{t} · {f}")
                }
            };
            senden_status.set_text(&mit_fehler(
                format!("{} Frames · {} Fehler", s.senden.zaehler, s.senden.fehler),
                &s.senden.letzter_fehler,
            ));
            empfangen_status.set_text(&mit_fehler(
                format!(
                    "{} Pakete · {} verworfen",
                    s.empfangen.zaehler, s.empfangen.fehler
                ),
                &s.empfangen.letzter_fehler,
            ));
            vban_tabelle(&incoming, &tabellen_kopf_in, &s.incoming);
            vban_tabelle(&outgoing, &tabellen_kopf_out, &s.outgoing);
            // Unterzeile im Kopf: zweites Label im ersten Block.
            if let Some(unter) = kopf
                .first_child()
                .and_then(|b| b.last_child())
                .and_then(|l| l.downcast::<gtk::Label>().ok())
            {
                unter.set_text(&crate::vban::unterzeile(&s));
            }
        })
    };
    anzeigen();

    // Schalter wirken sofort (wie im Original), Felder mit UEBERNEHMEN/Enter.
    let anwenden: Rc<dyn Fn()> = {
        let an = an.clone();
        let formular = Rc::clone(&formular);
        let meldung = meldung.clone();
        let anzeigen = Rc::clone(&anzeigen);
        Rc::new(move || {
            match formular().anwenden(&an.send, &an.recv) {
                Ok(()) => {
                    meldung.remove_css_class("fehler");
                    meldung.set_text("uebernommen");
                }
                Err(e) => {
                    meldung.add_css_class("fehler");
                    meldung.set_text(&format!("nicht uebernommen: {e}"));
                }
            }
            anzeigen();
        })
    };
    for sw in [&senden_an, &empfangen_an] {
        let anwenden = Rc::clone(&anwenden);
        let fuellt = Rc::clone(&fuellt);
        sw.connect_state_set(move |_, _| {
            if fuellt.get() {
                return gtk::glib::Propagation::Proceed;
            }
            // Erst nach dem Umschalten lesen: der Zustand kommt im naechsten
            // Durchlauf der Hauptschleife.
            let anwenden = Rc::clone(&anwenden);
            gtk::glib::idle_add_local_once(move || anwenden());
            gtk::glib::Propagation::Proceed
        });
    }
    for e in [&host, &senden_stream, &empfangen_stream] {
        let anwenden = Rc::clone(&anwenden);
        e.connect_activate(move |_| anwenden());
    }
    // Ein Port gilt, sobald er feststeht (Enter, Pfeile, Mausrad).
    for sp in [&senden_port, &empfangen_port] {
        let anwenden = Rc::clone(&anwenden);
        let fuellt = Rc::clone(&fuellt);
        sp.connect_value_changed(move |_| {
            if !fuellt.get() {
                anwenden();
            }
        });
    }
    {
        let anwenden = Rc::clone(&anwenden);
        uebernehmen.connect_clicked(move |_| anwenden());
    }
    {
        let an = an.clone();
        let fuellen = Rc::clone(&formular_fuellen);
        let anzeigen = Rc::clone(&anzeigen);
        let meldung = meldung.clone();
        reset.connect_clicked(move |_| {
            crosspoint::vban_io::setze_standard(&an.send, &an.recv);
            fuellen();
            anzeigen();
            meldung.remove_css_class("fehler");
            meldung.set_text("Standardwerte gesetzt (Datei bleibt, LOAD CONFIG holt sie zurueck)");
        });
    }
    {
        let an = an.clone();
        let fuellen = Rc::clone(&formular_fuellen);
        let anzeigen = Rc::clone(&anzeigen);
        let meldung = meldung.clone();
        laden.connect_clicked(move |_| {
            match crosspoint::vban_io::lade_konfig(&an.send, &an.recv) {
                Ok(()) => {
                    meldung.remove_css_class("fehler");
                    meldung.set_text(&format!(
                        "geladen: {}",
                        crosspoint::vban_io::konfig_pfad().display()
                    ));
                }
                Err(e) => {
                    meldung.add_css_class("fehler");
                    meldung.set_text(&e);
                }
            }
            fuellen();
            anzeigen();
        });
    }
    {
        let an = an.clone();
        let anwenden = Rc::clone(&anwenden);
        let meldung = meldung.clone();
        speichern.connect_clicked(move |_| {
            // Erst das Formular uebernehmen, dann sichern - gespeichert wird,
            // was zu sehen ist.
            anwenden();
            if meldung.has_css_class("fehler") {
                return;
            }
            match crosspoint::vban_io::speichere_konfig(&an.send, &an.recv) {
                Ok(()) => meldung.set_text(&format!(
                    "gespeichert: {}",
                    crosspoint::vban_io::konfig_pfad().display()
                )),
                Err(e) => {
                    meldung.add_css_class("fehler");
                    meldung.set_text(&e);
                }
            }
        });
    }

    // Takt nur, solange das Fenster offen ist.
    {
        let schwach = fenster.downgrade();
        let anzeigen = Rc::clone(&anzeigen);
        gtk::glib::timeout_add_local(Duration::from_millis(500), move || {
            match schwach.upgrade() {
                Some(f) if f.is_visible() => {
                    anzeigen();
                    gtk::glib::ControlFlow::Continue
                }
                _ => gtk::glib::ControlFlow::Break,
            }
        });
    }

    fenster.set_child(Some(&aussen));
    fenster.present();
    fenster
}

// ---------------------------------------------------------------- MIDI ----

/// MIDI-Fenster in der Optik des Originals: Geraetewahl, Map-Name, drei
/// Kartenbefehle, Rueckmeldung - und zwei Spalten mit Ziel/CC/Learn je Seite.
pub fn midi_fenster(parent: &impl IsA<gtk::Window>) -> gtk::Window {
    let fenster = gtk::Window::builder()
        .title("Crosspoint — MIDI-Mapping")
        .transient_for(parent)
        .modal(false)
        .default_width(1080)
        .default_height(640)
        .build();
    fenster.add_css_class("dialog");

    let karte = Rc::new(RefCell::new(crate::midimonster::Karte::laden(
        crate::midimonster::pfad(),
    )));
    karte.borrow_mut().alle();
    let seite = Rc::new(std::cell::Cell::new(0usize));

    let aussen = gtk::Box::new(gtk::Orientation::Vertical, 0);
    aussen.append(&fenster_kopf(
        &fenster,
        "MIDI-MAPPING",
        "MIDIMonster uebersetzt MIDI nach OSC — die Oberflaeche schreibt nur die Karte",
    ));

    // ----- Kopfzeile -----
    let kopf = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    kopf.set_margin_top(10);
    kopf.set_margin_bottom(6);
    kopf.set_margin_start(10);
    kopf.set_margin_end(10);

    let feld = |text: &str| {
        let l = gtk::Label::new(Some(text));
        l.add_css_class("feld");
        l.set_xalign(0.0);
        l
    };

    let eingang_box = gtk::Box::new(gtk::Orientation::Vertical, 2);
    eingang_box.append(&feld("MIDI INPUT DEVICE"));
    // MIDI-Anschluesse (ALSA-Sequencer), nicht die Audio-Eingaenge. Was in
    // der Karte steht, aber gerade nicht steckt, bleibt waehlbar.
    let mut geraete: Vec<String> = vec![crate::midimonster::KEIN_GERAET.to_string()];
    geraete.extend(crate::midimonster::midi_geraete());
    for n in [
        karte.borrow().eingang.clone(),
        karte.borrow().ausgang.clone(),
    ] {
        if !geraete.contains(&n) {
            geraete.push(n);
        }
    }
    let geraeteliste: Vec<&str> = geraete.iter().map(|s| s.as_str()).collect();
    let eingang = gtk::DropDown::from_strings(&geraeteliste);
    {
        let k = Rc::clone(&karte);
        let namen: Vec<String> = geraeteliste.iter().map(|s| s.to_string()).collect();
        eingang.connect_selected_notify(move |d| {
            let mut karte = k.borrow_mut();
            karte.eingang = namen
                .get(d.selected() as usize)
                .cloned()
                .unwrap_or_else(|| crate::midimonster::KEIN_GERAET.into());
        });
    }
    eingang_box.append(&eingang);
    kopf.append(&eingang_box);

    let name_box = gtk::Box::new(gtk::Orientation::Vertical, 2);
    name_box.append(&feld("MIDI MAP NAME"));
    let name = gtk::Entry::new();
    name.set_text(&karte.borrow().name);
    name.set_width_chars(18);
    {
        let k = Rc::clone(&karte);
        name.connect_changed(move |e| k.borrow_mut().name = e.text().to_string());
    }
    name_box.append(&name);
    kopf.append(&name_box);

    let kartenknoepfe = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    kartenknoepfe.set_valign(gtk::Align::End);
    let reset = gtk::Button::with_label("RESET MAP");
    reset.add_css_class("tbtn");
    let laden = gtk::Button::with_label("LOAD MIDI MAP");
    laden.add_css_class("tbtn");
    let speichern = gtk::Button::with_label("SAVE MIDI MAP");
    speichern.add_css_class("tbtn");
    kartenknoepfe.append(&reset);
    kartenknoepfe.append(&laden);
    kartenknoepfe.append(&speichern);
    kopf.append(&kartenknoepfe);

    let platz = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    platz.set_hexpand(true);
    kopf.append(&platz);

    let ausgang_box = gtk::Box::new(gtk::Orientation::Vertical, 2);
    ausgang_box.append(&feld("MIDI OUTPUT FEEDBACK"));
    let ausgang = gtk::DropDown::from_strings(&geraeteliste);
    {
        let k = Rc::clone(&karte);
        let namen: Vec<String> = geraeteliste.iter().map(|s| s.to_string()).collect();
        ausgang.connect_selected_notify(move |d| {
            k.borrow_mut().ausgang = namen
                .get(d.selected() as usize)
                .cloned()
                .unwrap_or_else(|| crate::midimonster::KEIN_GERAET.into());
        });
    }
    ausgang_box.append(&ausgang);
    kopf.append(&ausgang_box);
    aussen.append(&kopf);

    // ----- Zwei Spalten mit Zeilen -----
    let spalten = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    spalten.set_margin_start(10);
    spalten.set_margin_end(10);
    spalten.set_vexpand(true);
    let links = gtk::Box::new(gtk::Orientation::Vertical, 1);
    let rechts = gtk::Box::new(gtk::Orientation::Vertical, 1);
    links.set_hexpand(true);
    rechts.set_hexpand(true);
    let links_scroll = gtk::ScrolledWindow::new();
    links_scroll.set_child(Some(&links));
    links_scroll.set_vexpand(true);
    let rechts_scroll = gtk::ScrolledWindow::new();
    rechts_scroll.set_child(Some(&rechts));
    rechts_scroll.set_vexpand(true);
    spalten.append(&links_scroll);
    spalten.append(&rechts_scroll);
    aussen.append(&spalten);

    // ----- Fusszeile -----
    let fuss = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    fuss.add_css_class("dialogfuss");
    let seitenzeige = gtk::Label::new(Some("Page 1/6"));
    seitenzeige.add_css_class("feld");
    let zurueck = gtk::Button::with_label("<< PREV. PAGE");
    zurueck.add_css_class("tbtn");
    let vor = gtk::Button::with_label("NEXT PAGE >>");
    vor.add_css_class("tbtn");
    let status = gtk::Label::new(Some(""));
    status.add_css_class("feld");
    fuss.append(&status);
    let platz = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    platz.set_hexpand(true);
    fuss.append(&platz);
    fuss.append(&seitenzeige);
    fuss.append(&zurueck);
    fuss.append(&vor);
    aussen.append(&fuss);

    // Zeilen einer Seite aufbauen.
    let zeichnen: Rc<dyn Fn()> = {
        let links = links.clone();
        let rechts = rechts.clone();
        let karte = Rc::clone(&karte);
        let seite = Rc::clone(&seite);
        let seitenzeige = seitenzeige.clone();
        let status = status.clone();
        Rc::new(move || {
            while let Some(k) = links.first_child() {
                links.remove(&k);
            }
            while let Some(k) = rechts.first_child() {
                rechts.remove(&k);
            }
            let s = seite.get();
            seitenzeige.set_text(&format!("Page {}/{}", s + 1, crate::midimonster::SEITEN));
            let karte_ref = karte.borrow();
            let zeilen = karte_ref.seite(s);
            let pro_spalte = zeilen.len().div_ceil(2);
            for (i, z) in zeilen.iter().enumerate() {
                let zeile = gtk::Box::new(gtk::Orientation::Horizontal, 6);
                zeile.add_css_class("midi-zeile");
                let titel = gtk::Label::new(Some(&z.titel));
                titel.add_css_class("zeile");
                titel.set_xalign(0.0);
                titel.set_hexpand(true);
                titel.set_ellipsize(gtk::pango::EllipsizeMode::End);
                let cc = gtk::Label::new(Some(&match z.cc {
                    Some(c) => format!("CC {c}"),
                    None => "—".to_string(),
                }));
                cc.add_css_class("zeile");
                cc.add_css_class("dim");
                cc.set_size_request(46, -1);
                let lernen = gtk::Button::with_label("LEARN");
                lernen.add_css_class("tbtn");
                lernen.add_css_class("learn");
                {
                    let karte = Rc::clone(&karte);
                    let status = status.clone();
                    let ziel = z.ziel.clone();
                    let titel = z.titel.clone();
                    let cc_label = cc.clone();
                    let wartet = Rc::new(Cell::new(false));
                    lernen.connect_clicked(move |b| {
                        // Laeuft fuer diese Zeile schon ein Lernvorgang? Abbrechen.
                        if wartet.get() {
                            wartet.set(false);
                            b.remove_css_class("an");
                            b.set_label("LEARN");
                            status.set_text("Lernmodus abgebrochen");
                            return;
                        }
                        // Gebundene Zeile: Klick loest die Bindung (wie bisher).
                        let gebunden = {
                            let k = karte.borrow();
                            k.zeilen.iter().find(|z| z.ziel == ziel).and_then(|z| z.cc)
                        };
                        if gebunden.is_some() {
                            karte.borrow_mut().loesen(&ziel);
                            let _ = karte.borrow().speichern(crate::midimonster::pfad());
                            b.set_label("LEARN");
                            cc_label.set_text("—");
                            status.set_text(&format!("{titel}: Bindung geloescht"));
                            return;
                        }
                        // Sonst: auf die naechste CC-Meldung warten. Die kommt
                        // als OSC /midi/cc <kanal> <cc> <wert> aus dem Kern
                        // (MIDIMonster-Rueckweg) und liegt im CC-Kanal.
                        // Was vor dem Klick kam, gilt nicht: sonst bindet
                        // ein laengst bewegter Fader sofort.
                        let _ = crosspoint::osc::cc_kanal().abholen();
                        wartet.set(true);
                        b.add_css_class("an");
                        b.set_label("WARTET …");
                        status.set_text(&format!(
                            "{titel}: warte auf MIDI-CC (OSC /midi/cc <kanal> <cc> <wert>) …"
                        ));
                        let wartet2 = Rc::clone(&wartet);
                        let karte2 = Rc::clone(&karte);
                        let b2 = b.clone();
                        let cc_label2 = cc_label.clone();
                        let status2 = status.clone();
                        let ziel2 = ziel.clone();
                        let titel2 = titel.clone();
                        gtk::glib::timeout_add_local(Duration::from_millis(120), move || {
                            if !wartet2.get() {
                                return gtk::glib::ControlFlow::Break;
                            }
                            let Some(cc) = crosspoint::osc::cc_kanal().letzte() else {
                                return gtk::glib::ControlFlow::Continue;
                            };
                            karte2.borrow_mut().binden(&ziel2, cc.cc);
                            let _ = karte2.borrow().speichern(crate::midimonster::pfad());
                            cc_label2.set_text(&format!("CC {}", cc.cc));
                            b2.remove_css_class("an");
                            b2.set_label("LEARN");
                            wartet2.set(false);
                            status2.set_text(&format!(
                                "{titel2}: CC {} uebernommen (Kanal {}, Wert {})",
                                cc.cc, cc.kanal, cc.wert
                            ));
                            gtk::glib::ControlFlow::Break
                        });
                    });
                }
                zeile.append(&titel);
                zeile.append(&cc);
                zeile.append(&lernen);
                if i < pro_spalte {
                    links.append(&zeile);
                } else {
                    rechts.append(&zeile);
                }
            }
        })
    };

    // Kartenbefehle
    {
        let zeichnen = Rc::clone(&zeichnen);
        let karte = Rc::clone(&karte);
        let status = status.clone();
        let pfad = crate::midimonster::pfad();
        reset.connect_clicked(move |_| {
            karte.borrow_mut().zuruecksetzen();
            let _ = karte.borrow().speichern(&pfad);
            zeichnen();
            status.set_text("Karte zurueckgesetzt und gespeichert");
        });
    }
    {
        let zeichnen = Rc::clone(&zeichnen);
        let karte = Rc::clone(&karte);
        let status = status.clone();
        let pfad = crate::midimonster::pfad();
        laden.connect_clicked(move |_| {
            let neu = crate::midimonster::Karte::laden(&pfad);
            let anzahl = neu.gebunden();
            *karte.borrow_mut() = neu;
            karte.borrow_mut().alle();
            zeichnen();
            status.set_text(&format!(
                "geladen: {anzahl} Zuordnungen aus {}",
                pfad.display()
            ));
        });
    }
    {
        let karte = Rc::clone(&karte);
        let status = status.clone();
        let pfad = crate::midimonster::pfad();
        speichern.connect_clicked(move |_| match karte.borrow().speichern(&pfad) {
            Ok(()) => status.set_text(&format!("gespeichert: {}", pfad.display())),
            Err(e) => status.set_text(&format!("Fehler beim Speichern: {e}")),
        });
    }
    {
        let seite = Rc::clone(&seite);
        let zeichnen = Rc::clone(&zeichnen);
        zurueck.connect_clicked(move |_| {
            let s = seite.get();
            seite.set(if s == 0 {
                crate::midimonster::SEITEN - 1
            } else {
                s - 1
            });
            zeichnen();
        });
    }
    {
        let seite = Rc::clone(&seite);
        let zeichnen = Rc::clone(&zeichnen);
        vor.connect_clicked(move |_| {
            let s = seite.get();
            seite.set((s + 1) % crate::midimonster::SEITEN);
            zeichnen();
        });
    }
    // Geraetewahl vorbelegen (Ein- und Ausgang getrennt).
    if let Some(i) = geraete.iter().position(|n| *n == karte.borrow().eingang) {
        eingang.set_selected(i as u32);
    }
    if let Some(i) = geraete.iter().position(|n| *n == karte.borrow().ausgang) {
        ausgang.set_selected(i as u32);
    }

    zeichnen();
    fenster.set_child(Some(&aussen));
    fenster.present();
    fenster
}

// ------------------------------------------------------------ EQ-Editor ----

/// EQ-Overlay eines Streifens: drei Baender mit Frequenz, Guete, Verstaerkung.
pub fn eq_fenster(
    parent: &impl IsA<gtk::Window>,
    params: std::sync::Arc<Params>,
    strip: usize,
) -> gtk::Window {
    let fenster = gtk::Window::builder()
        .title(format!("Crosspoint — EQ Streifen {}", strip + 1))
        .transient_for(parent)
        .modal(false)
        .default_width(520)
        .default_height(420)
        .build();
    fenster.add_css_class("dialog");

    let aussen = gtk::Box::new(gtk::Orientation::Vertical, 0);
    aussen.append(&fenster_kopf(
        &fenster,
        &format!("EQ — STREIFEN {}", strip + 1),
        "drei Baender · Werte direkt in Params",
    ));

    let mitte = gtk::Box::new(gtk::Orientation::Vertical, 8);
    mitte.set_margin_top(10);
    mitte.set_margin_bottom(10);
    mitte.set_margin_start(10);
    mitte.set_margin_end(10);

    // EQ gesamt an/aus
    let kopfzeile = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let lbl = gtk::Label::new(Some("EQ"));
    lbl.add_css_class("schalter-zeile");
    lbl.set_xalign(0.0);
    lbl.set_size_request(60, -1);
    let eq_an = gtk::Switch::new();
    eq_an.set_active(
        params.strips[strip]
            .eq_on
            .load(std::sync::atomic::Ordering::Relaxed),
    );
    {
        let p = params.clone();
        eq_an.connect_state_set(move |_, an| {
            p.set("strip", strip, "eq", if an { 1.0 } else { 0.0 });
            gtk::glib::Propagation::Proceed
        });
    }
    kopfzeile.append(&lbl);
    kopfzeile.append(&eq_an);
    mitte.append(&kopfzeile);
    mitte.append(&trenner());

    for band in 0..3 {
        let zelle = &params.strips[strip].eq[0][band];
        let freq = f32::from_bits(zelle.freq.load(std::sync::atomic::Ordering::Relaxed));
        let q = f32::from_bits(zelle.q.load(std::sync::atomic::Ordering::Relaxed));
        let gain = f32::from_bits(zelle.gain.load(std::sync::atomic::Ordering::Relaxed));
        let an = zelle.on.load(std::sync::atomic::Ordering::Relaxed);

        let block = gtk::Box::new(gtk::Orientation::Vertical, 4);
        let kopf = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        let titel = gtk::Label::new(Some(&format!("BAND {}", band + 1)));
        titel.add_css_class("makro-titel");
        titel.set_xalign(0.0);
        titel.set_size_request(70, -1);
        let schalter = gtk::Switch::new();
        schalter.set_active(an);
        {
            let p = params.clone();
            schalter.connect_state_set(move |_, v| {
                p.set(
                    "strip",
                    strip,
                    &format!("eq{band}_on"),
                    if v { 1.0 } else { 0.0 },
                );
                gtk::glib::Propagation::Proceed
            });
        }
        kopf.append(&titel);
        kopf.append(&schalter);
        block.append(&kopf);

        for (feld, wert, von, bis, schritt) in [
            ("FREQ", freq, 20.0f64, 20000.0f64, 1.0),
            ("GUETE Q", q, 0.1f64, 10.0f64, 0.1),
            ("GAIN dB", gain, -24.0f64, 24.0f64, 0.5),
        ] {
            let zeile = gtk::Box::new(gtk::Orientation::Horizontal, 8);
            let l = gtk::Label::new(Some(feld));
            l.add_css_class("feld");
            l.set_xalign(0.0);
            l.set_size_request(70, -1);
            let skala = gtk::Scale::with_range(gtk::Orientation::Horizontal, von, bis, schritt);
            skala.set_hexpand(true);
            skala.set_draw_value(false);
            skala.set_value(wert as f64);
            let anzeige = gtk::Label::new(Some(&format!("{wert:.1}")));
            anzeige.add_css_class("zeile");
            anzeige.set_size_request(56, -1);
            anzeige.set_xalign(1.0);
            {
                let p = params.clone();
                let anzeige = anzeige.clone();
                skala.connect_value_changed(move |s| {
                    let v = s.value() as f32;
                    let name = match feld {
                        "FREQ" => format!("eq{band}_freq"),
                        "GUETE Q" => format!("eq{band}_q"),
                        _ => format!("eq{band}_gain"),
                    };
                    p.set("strip", strip, &name, v);
                    anzeige.set_text(&format!("{v:.1}"));
                });
            }
            zeile.append(&l);
            zeile.append(&skala);
            zeile.append(&anzeige);
            block.append(&zeile);
        }
        mitte.append(&block);
        mitte.append(&trenner());
    }

    aussen.append(&mitte);
    fenster.set_child(Some(&aussen));
    fenster.present();
    fenster
}

// -------------------------------------------------------------- Presets ----

/// Ordner der Presets.
pub fn preset_ordner() -> std::path::PathBuf {
    crate::theme::konfig_pfad()
        .parent()
        .map(|p| p.join("presets"))
        .unwrap_or_else(|| std::path::PathBuf::from("presets"))
}

/// Preset-Fenster: Liste, Laden, Speichern unter Name, zuletzt geladen.
pub fn preset_fenster(
    parent: &impl IsA<gtk::Window>,
    shell: &std::rc::Rc<crate::shell::Shell>,
) -> gtk::Window {
    let fenster = gtk::Window::builder()
        .title("Crosspoint — Presets")
        .transient_for(parent)
        .modal(false)
        .default_width(460)
        .default_height(460)
        .build();
    fenster.add_css_class("dialog");

    let aussen = gtk::Box::new(gtk::Orientation::Vertical, 0);
    aussen.append(&fenster_kopf(
        &fenster,
        "PRESETS",
        "Parameterbild laden und sichern",
    ));
    let mitte = gtk::Box::new(gtk::Orientation::Vertical, 8);
    mitte.set_margin_top(10);
    mitte.set_margin_bottom(10);
    mitte.set_margin_start(10);
    mitte.set_margin_end(10);

    let ordner = preset_ordner();
    let liste = gtk::ListBox::new();
    liste.add_css_class("presetliste");
    let scroll = gtk::ScrolledWindow::new();
    scroll.set_child(Some(&liste));
    scroll.set_vexpand(true);
    mitte.append(&scroll);

    let status = gtk::Label::new(Some(&format!("Ordner: {}", ordner.display())));
    status.add_css_class("feld");
    status.set_xalign(0.0);
    status.set_wrap(true);
    mitte.append(&status);

    let name_zeile = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    let name = gtk::Entry::new();
    name.set_placeholder_text(Some("Name fuer Speichern unter"));
    name.set_hexpand(true);
    let sichern = gtk::Button::with_label("SPEICHERN");
    sichern.add_css_class("tbtn");
    name_zeile.append(&name);
    name_zeile.append(&sichern);
    mitte.append(&name_zeile);

    let fuellen: Rc<dyn Fn()> = {
        let liste = liste.clone();
        let ordner = ordner.clone();
        Rc::new(move || {
            while let Some(k) = liste.first_child() {
                liste.remove(&k);
            }
            let mut namen: Vec<String> = Vec::new();
            if let Ok(eintraege) = std::fs::read_dir(&ordner) {
                for e in eintraege.flatten() {
                    let p = e.path();
                    if p.extension().map(|x| x == "json").unwrap_or(false) {
                        if let Some(st) = p.file_stem().and_then(|s| s.to_str()) {
                            namen.push(st.to_string());
                        }
                    }
                }
            }
            namen.sort();
            if namen.is_empty() {
                let l = gtk::Label::new(Some("noch kein Preset gespeichert"));
                l.add_css_class("feld");
                liste.append(&l);
            }
            for n in namen {
                let zeile = gtk::Box::new(gtk::Orientation::Horizontal, 6);
                let l = gtk::Label::new(Some(&n));
                l.add_css_class("zeile");
                l.set_xalign(0.0);
                l.set_hexpand(true);
                let laden = gtk::Button::with_label("LADEN");
                laden.add_css_class("tbtn");
                zeile.append(&l);
                zeile.append(&laden);
                liste.append(&zeile);
            }
        })
    };
    fuellen();

    {
        let shell = std::rc::Rc::clone(shell);
        let name = name.clone();
        let status = status.clone();
        let ordner = ordner.clone();
        let fuellen = Rc::clone(&fuellen);
        sichern.connect_clicked(move |_| {
            let n = name.text().trim().to_string();
            if n.is_empty() {
                status.set_text("Bitte einen Namen eingeben");
                return;
            }
            let pfad = ordner.join(format!("{n}.json"));
            match crosspoint::restore::preset_speichern(&shell.params, &pfad) {
                Ok(()) => {
                    status.set_text(&format!("gespeichert: {}", pfad.display()));
                    shell.hinweis(&format!("Preset '{n}' gespeichert"), false);
                    fuellen();
                }
                Err(e) => {
                    status.set_text(&format!("Fehler: {e}"));
                    shell.hinweis(&format!("Preset nicht gespeichert: {e}"), true);
                }
            }
        });
    }

    // Klick auf "LADEN" je Zeile
    {
        let shell = std::rc::Rc::clone(shell);
        let status = status.clone();
        let ordner = ordner.clone();
        liste.connect_row_activated(move |_, row| {
            let Some(zeile) = row.child() else { return };
            let Some(box_) = zeile.downcast_ref::<gtk::Box>() else {
                return;
            };
            let Some(l) = box_
                .first_child()
                .and_then(|c| c.downcast::<gtk::Label>().ok())
            else {
                return;
            };
            let pfad = ordner.join(format!("{}.json", l.text()));
            if crosspoint::restore::preset_laden(&shell.params, &pfad) {
                status.set_text(&format!("geladen: {}", pfad.display()));
                shell.hinweis(&format!("Preset '{}' geladen", l.text()), false);
                shell.presets_auffrischen();
            } else {
                status.set_text(&format!("nicht ladbar: {}", pfad.display()));
                shell.hinweis("Preset nicht ladbar", true);
            }
        });
    }

    aussen.append(&mitte);
    fenster.set_child(Some(&aussen));
    fenster.present();
    fenster
}

// --------------------------------------------------------- Darstellung ----

/// Fenster "Darstellung": Theme, Layout, Sichtbarkeit, Seite, Dichte,
/// Streifenreihenfolge. Jede Aenderung wirkt sofort.
pub fn darstellung_fenster(this: &Rc<Shell>) -> gtk::Window {
    let fenster = gtk::Window::builder()
        .title("Crosspoint — Darstellung")
        .transient_for(&this.fenster)
        .modal(false)
        .default_width(520)
        .default_height(620)
        .build();
    fenster.add_css_class("dialog");

    let aussen = gtk::Box::new(gtk::Orientation::Vertical, 0);
    aussen.append(&fenster_kopf(
        &fenster,
        "DARSTELLUNG",
        "Theme (Farben) und Layout (Anordnung) sind getrennte Modelle",
    ));
    let mitte = gtk::Box::new(gtk::Orientation::Vertical, 10);
    mitte.set_margin_top(12);
    mitte.set_margin_bottom(12);
    mitte.set_margin_start(12);
    mitte.set_margin_end(12);
    aussen.append(&mitte);

    let scroll = gtk::ScrolledWindow::new();
    scroll.set_child(Some(&mitte));
    scroll.set_vexpand(true);
    aussen.append(&scroll);
    fenster.set_child(Some(&aussen));

    fuellen(&mitte, this, &fenster);
    fenster.present();
    fenster
}

fn zeile(titel: &str) -> gtk::Box {
    let z = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let l = gtk::Label::new(Some(titel));
    l.add_css_class("schalter-zeile");
    l.set_xalign(0.0);
    l.set_size_request(150, -1);
    z.append(&l);
    z
}

fn trenner() -> gtk::Widget {
    let b = gtk::Box::new(gtk::Orientation::Vertical, 0);
    b.add_css_class("trenner");
    b.set_size_request(-1, 1);
    b.upcast()
}

fn fuellen(mitte: &gtk::Box, this: &Rc<Shell>, fenster: &gtk::Window) {
    while let Some(k) = mitte.first_child() {
        mitte.remove(&k);
    }

    // Theme
    let namen = this.konfig.borrow().theme.namen();
    let aktiv = this.konfig.borrow().theme.aktiv.clone();
    let refs: Vec<&str> = namen.iter().map(|s| s.as_str()).collect();
    let th = gtk::DropDown::from_strings(&refs);
    th.set_selected(namen.iter().position(|n| *n == aktiv).unwrap_or(0) as u32);
    let z = zeile("Theme");
    z.append(&th);
    mitte.append(&z);
    {
        let this = Rc::clone(this);
        let namen = namen.clone();
        th.connect_selected_notify(move |d| {
            if let Some(n) = namen.get(d.selected() as usize) {
                Shell::thema_setzen(&this, n, true);
            }
        });
    }

    // Layout
    let lnamen = this.konfig.borrow().layout.namen();
    let laktiv = this.konfig.borrow().layout.aktiv.clone();
    let lrefs: Vec<&str> = lnamen.iter().map(|s| s.as_str()).collect();
    let ld = gtk::DropDown::from_strings(&lrefs);
    ld.set_selected(lnamen.iter().position(|n| *n == laktiv).unwrap_or(0) as u32);
    let z = zeile("Layout");
    z.append(&ld);
    mitte.append(&z);
    {
        let this = Rc::clone(this);
        let fenster = fenster.clone();
        let mitte = mitte.clone();
        let lnamen = lnamen.clone();
        ld.connect_selected_notify(move |d| {
            if let Some(n) = lnamen.get(d.selected() as usize) {
                Shell::layout_setzen(&this, n, true);
                // Inhalt nachziehen (Sichtbarkeit/Schalter), ohne den Klick zu
                // zerstoeren: erst im naechsten Idle-Schritt.
                let this2 = Rc::clone(&this);
                let fenster2 = fenster.clone();
                let mitte2 = mitte.clone();
                gtk::glib::idle_add_local_once(move || {
                    fuellen(&mitte2, &this2, &fenster2);
                });
            }
        });
    }
    mitte.append(&trenner());

    // Sichtbarkeit
    let titel = gtk::Label::new(Some("BEREICHE"));
    titel.add_css_class("makro-titel");
    titel.set_xalign(0.0);
    mitte.append(&titel);
    let layout = this.konfig.borrow().layout.aktuell();
    for b in Bereich::ALLE {
        let z = zeile(b.name());
        let sw = gtk::Switch::new();
        sw.set_active(layout.sichtbar.get(b));
        sw.set_halign(gtk::Align::Start);
        {
            let this = Rc::clone(this);
            let fenster = fenster.clone();
            let mitte = mitte.clone();
            sw.connect_state_set(move |_, an| {
                Shell::bereich_setzen(&this, b, an);
                let this2 = Rc::clone(&this);
                let fenster2 = fenster.clone();
                let mitte2 = mitte.clone();
                gtk::glib::idle_add_local_once(move || {
                    fuellen(&mitte2, &this2, &fenster2);
                });
                gtk::glib::Propagation::Stop
            });
        }
        z.append(&sw);
        mitte.append(&z);
    }
    mitte.append(&trenner());

    // Busspalte / Master / Dichte
    let seiten = ["links", "rechts"];
    let sd = gtk::DropDown::from_strings(&seiten);
    sd.set_selected(if layout.busse_seite == Seite::Links {
        0
    } else {
        1
    });
    let z = zeile("Busspalte");
    z.append(&sd);
    mitte.append(&z);
    {
        let this = Rc::clone(this);
        sd.connect_selected_notify(move |d| {
            Shell::busseite_setzen(
                &this,
                if d.selected() == 0 {
                    Seite::Links
                } else {
                    Seite::Rechts
                },
            );
        });
    }

    let mnamen = ["in der Busspalte", "eigene Spalte", "rechts aussen"];
    let md = gtk::DropDown::from_strings(&mnamen);
    md.set_selected(match layout.master {
        MasterPlatz::InBusspalte => 0,
        MasterPlatz::EigeneSpalte => 1,
        MasterPlatz::RechtsAussen => 2,
    });
    let z = zeile("Master");
    z.append(&md);
    mitte.append(&z);
    {
        let this = Rc::clone(this);
        md.connect_selected_notify(move |d| {
            let p = match d.selected() {
                0 => MasterPlatz::InBusspalte,
                1 => MasterPlatz::EigeneSpalte,
                _ => MasterPlatz::RechtsAussen,
            };
            Shell::masterplatz_setzen(&this, p);
        });
    }

    let dnamen = ["kompakt", "normal", "breit"];
    let dd = gtk::DropDown::from_strings(&dnamen);
    dd.set_selected(match layout.dichte {
        Dichte::Kompakt => 0,
        Dichte::Normal => 1,
        Dichte::Breit => 2,
    });
    let z = zeile("Dichte");
    z.append(&dd);
    mitte.append(&z);
    {
        let this = Rc::clone(this);
        dd.connect_selected_notify(move |d| {
            let dichte = match d.selected() {
                0 => Dichte::Kompakt,
                2 => Dichte::Breit,
                _ => Dichte::Normal,
            };
            Shell::dichte_setzen(&this, dichte);
        });
    }
    mitte.append(&trenner());

    // Makro-Felder: wie viele der 80 Felder sichtbar sind. Umschalten wirkt
    // sofort (Raster wird neu gebaut), der Wert landet in ui.json.
    let mwerte = crate::layout::MAKROS_ANZAHL;
    let mtexte: Vec<String> = mwerte.iter().map(|n| format!("{n} Felder")).collect();
    let mrefs: Vec<&str> = mtexte.iter().map(|s| s.as_str()).collect();
    let md = gtk::DropDown::from_strings(&mrefs);
    md.set_selected(
        mwerte
            .iter()
            .position(|n| *n == layout.makros_anzahl)
            .unwrap_or(0) as u32,
    );
    let z = zeile("Makro-Felder");
    z.append(&md);
    mitte.append(&z);
    {
        let this = Rc::clone(this);
        md.connect_selected_notify(move |d| {
            if let Some(n) = mwerte.get(d.selected() as usize) {
                Shell::makro_anzahl_setzen(&this, *n);
            }
        });
    }
    mitte.append(&trenner());

    // Streifenreihenfolge
    let titel = gtk::Label::new(Some("STREIFEN-REIHENFOLGE"));
    titel.add_css_class("makro-titel");
    titel.set_xalign(0.0);
    mitte.append(&titel);
    let reihe = layout.streifen_reihenfolge.clone();
    for (pos, idx) in reihe.iter().copied().enumerate() {
        let z = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        let nr = gtk::Label::new(Some(&format!("{}", pos + 1)));
        nr.add_css_class("feld");
        nr.set_size_request(18, -1);
        let name = gtk::Label::new(Some(crosspoint::STRIP_NAMES[idx]));
        name.add_css_class("schalter-zeile");
        name.set_xalign(0.0);
        name.set_hexpand(true);
        z.append(&nr);
        z.append(&name);

        let hoch = gtk::Button::with_label("▲");
        hoch.add_css_class("tbtn");
        let runter = gtk::Button::with_label("▼");
        runter.add_css_class("tbtn");
        {
            let this = Rc::clone(this);
            let fenster = fenster.clone();
            let mitte = mitte.clone();
            hoch.connect_clicked(move |_| {
                Shell::streifen_verschieben(&this, pos, -1);
                let this2 = Rc::clone(&this);
                let fenster2 = fenster.clone();
                let mitte2 = mitte.clone();
                gtk::glib::idle_add_local_once(move || {
                    fuellen(&mitte2, &this2, &fenster2);
                });
            });
        }
        {
            let this = Rc::clone(this);
            let fenster = fenster.clone();
            let mitte = mitte.clone();
            runter.connect_clicked(move |_| {
                Shell::streifen_verschieben(&this, pos, 1);
                let this2 = Rc::clone(&this);
                let fenster2 = fenster.clone();
                let mitte2 = mitte.clone();
                gtk::glib::idle_add_local_once(move || {
                    fuellen(&mitte2, &this2, &fenster2);
                });
            });
        }
        z.append(&hoch);
        z.append(&runter);
        mitte.append(&z);
    }

    let pfad = this.konfig.borrow().pfad.display().to_string();
    let hinweis = gtk::Label::new(Some(&format!("gespeichert in {pfad}")));
    hinweis.add_css_class("feld");
    hinweis.set_xalign(0.0);
    hinweis.set_wrap(true);
    mitte.append(&hinweis);
}
