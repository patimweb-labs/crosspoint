//! Bedienung der Effektwege: FX-Menü je Streifen und Bus, der Insert-Editor
//! (LV2-Ketten über mod-host, Carla von Hand) und das Fenster EFFEKTE mit den
//! Einstellungen von Hall und Echo.
//!
//! Wie die übrigen Menüs schreiben alle Regler über `Params::set` – dieselbe
//! Stelle wie OSC, HTTP und Presets. Die Menüs entstehen bei jedem Öffnen neu
//! aus dem Modell; es gibt keinen Zustand, der nur hier stünde.

use std::rc::Rc;
use std::sync::atomic::Ordering::Relaxed;
use std::sync::Arc;

use crosspoint::effekte::{self, FX_NAMEN, FX_TITEL, FX_WEGE};
use crosspoint::engine::Params;
use crosspoint::fx::{self, Insert, Ziel};
use crosspoint::programme::{self, Modul};
use gtk::prelude::*;

use crate::strip::{f32_von, kopfzeile, menue_zu, schieber};

/// Text eines Send-/Return-Pegels: „aus“ an der Untergrenze.
pub fn pegel_text(db: f64) -> String {
    if db <= effekte::AUS_DB as f64 {
        "aus".to_string()
    } else {
        format!("{db:+.1} dB")
    }
}

/// Text der Klangfarbe: dunkel … neutral … hell.
pub fn farbe_text(f: f64) -> String {
    if f.abs() < 0.005 {
        "neutral".to_string()
    } else if f < 0.0 {
        format!("dunkler {:.0} %", -f * 100.0)
    } else {
        format!("heller {:.0} %", f * 100.0)
    }
}

/// Ein Schalter in einem Menü (Beschriftung links, Knopf rechts).
fn schalter(titel: &str, an: bool, tooltip: &str) -> (gtk::Box, gtk::ToggleButton) {
    let zeile = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    let l = gtk::Label::new(Some(titel));
    l.add_css_class("stufen-titel");
    l.set_xalign(0.0);
    l.set_hexpand(true);
    let b = gtk::ToggleButton::with_label(if an { "AN" } else { "AUS" });
    b.add_css_class("stufenwahl");
    b.set_active(an);
    b.set_tooltip_text(Some(tooltip));
    b.connect_toggled(|b| b.set_label(if b.is_active() { "AN" } else { "AUS" }));
    zeile.append(&l);
    zeile.append(&b);
    (zeile, b)
}

/// Programmauswahl oben in den Menüs von COMP, GATE und NR.
pub fn programmwahl(p: &Arc<Params>, index: usize, modul: Modul) -> gtk::Box {
    let zeile = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    let l = gtk::Label::new(Some("Programm"));
    l.add_css_class("stufen-titel");
    l.set_xalign(0.0);
    l.set_hexpand(true);
    let mut namen: Vec<&str> = vec![programme::name(modul, 0)];
    namen.extend(modul.programme().iter().map(|p| p.name));
    let dd = gtk::DropDown::from_strings(&namen);
    dd.set_selected(programme::aktuell(p, index, modul));
    dd.set_tooltip_text(Some(
        "Fertige Einstellung laden; eine Änderung von Hand macht daraus „Eigene“",
    ));
    let p = p.clone();
    dd.connect_selected_notify(move |d| {
        let n = d.selected();
        if n != programme::aktuell(&p, index, modul) {
            p.set("strip", index, modul.param(), n as f32);
            // Die Regler darunter zeigen noch die alten Werte: zu und beim
            // naechsten Oeffnen neu aus dem Modell.
            menue_zu(d);
        }
    });
    zeile.append(&l);
    zeile.append(&dd);
    zeile
}

/// Send- oder Return-Regler mit Text „aus“ an der Untergrenze.
fn pegelregler(
    p: &Arc<Params>,
    ziel: &'static str,
    index: usize,
    param: String,
    titel: &str,
    wert: f32,
) -> (gtk::Box, gtk::Scale) {
    let (k, s, w) = schieber(
        titel,
        effekte::AUS_DB as f64,
        effekte::MAX_DB as f64,
        0.5,
        wert as f64,
        "",
    );
    w.set_text(&pegel_text(wert as f64));
    let p = p.clone();
    s.connect_value_changed(move |s| {
        w.set_text(&pegel_text(s.value()));
        p.set(ziel, index, &param, s.value() as f32);
    });
    (k, s)
}

/// FX-Menü eines Streifens: Sends, Klangfarbe, Karaoke, Green Mode, Insert.
pub fn streifen_menue(p: &Arc<Params>, index: usize, oeffne_inserts: Rc<dyn Fn(Ziel)>) -> gtk::Box {
    let sp = &p.strips[index];
    let kasten = gtk::Box::new(gtk::Orientation::Vertical, 5);
    kasten.add_css_class("stufenmenue");
    kasten.append(&kopfzeile("FX · SENDS"));
    for w in 0..FX_WEGE {
        let zeile = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        let (k, _) = pegelregler(
            p,
            "strip",
            index,
            format!("send_{}", FX_NAMEN[w]),
            FX_TITEL[w],
            f32_von(&sp.fx_send[w]),
        );
        k.set_hexpand(true);
        let post = sp.fx_post[w].load(Relaxed);
        let b = gtk::ToggleButton::with_label(if post { "POST" } else { "PRE" });
        b.add_css_class("stufenwahl");
        b.set_active(!post);
        b.set_valign(gtk::Align::End);
        b.set_tooltip_text(Some(
            "POST: hinter Fader und Pan (Vorgabe) · PRE: vor dem Fader, z. B. fuer Monitor-Mischungen",
        ));
        {
            let p = p.clone();
            let name = format!("post_{}", FX_NAMEN[w]);
            b.connect_toggled(move |b| {
                let pre = b.is_active();
                b.set_label(if pre { "PRE" } else { "POST" });
                p.set("strip", index, &name, if pre { 0.0 } else { 1.0 });
            });
        }
        zeile.append(&k);
        zeile.append(&b);
        kasten.append(&zeile);
    }

    kasten.append(&kopfzeile("KLANG"));
    let farbe = f32_von(&sp.farbe);
    let (k, s, w) = schieber("Klangfarbe", -1.0, 1.0, 0.05, farbe as f64, "");
    w.set_text(&farbe_text(farbe as f64));
    s.set_tooltip_text(Some(
        "Kippt Tiefen und Hoehen gegeneinander (±6 dB): links waermer, rechts praesenter",
    ));
    s.add_mark(0.0, gtk::PositionType::Bottom, None);
    {
        let p = p.clone();
        s.connect_value_changed(move |s| {
            w.set_text(&farbe_text(s.value()));
            p.set("strip", index, "farbe", s.value() as f32);
        });
    }
    kasten.append(&k);

    let zeile = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    let l = gtk::Label::new(Some("Karaoke"));
    l.add_css_class("stufen-titel");
    l.set_xalign(0.0);
    l.set_hexpand(true);
    let dd = gtk::DropDown::from_strings(&["Aus", "Stimme weg", "Nur Mitte"]);
    dd.set_selected(sp.karaoke.load(Relaxed));
    dd.set_tooltip_text(Some(
        "Stimme weg: Mitte heraus, Baesse bleiben · Nur Mitte: Stereobild weg, Stimme bleibt",
    ));
    {
        let p = p.clone();
        dd.connect_selected_notify(move |d| {
            p.set("strip", index, "karaoke", d.selected() as f32);
        });
    }
    zeile.append(&l);
    zeile.append(&dd);
    kasten.append(&zeile);

    let (z, b) = schalter(
        "Mitte stumm (MC)",
        sp.muc.load(Relaxed),
        "Nur die Seitenanteile: alles, was in der Mitte steht, verschwindet",
    );
    {
        let p = p.clone();
        b.connect_toggled(move |b| {
            p.set("strip", index, "muc", b.is_active() as u8 as f32);
        });
    }
    kasten.append(&z);

    let (z, b) = schalter(
        "Green Mode",
        sp.green.load(Relaxed),
        "Bearbeitung umgehen (Lowcut, TK, Gate, NR, EQ, Klang, Comp): nur Fader, Pan und Routing",
    );
    {
        let p = p.clone();
        b.connect_toggled(move |b| {
            p.set("strip", index, "green", b.is_active() as u8 as f32);
        });
    }
    kasten.append(&z);

    insert_bereich(
        &kasten,
        p,
        "strip",
        Ziel::Streifen(index),
        sp.insert.load(Relaxed),
        sp.ins_bereit.load(Relaxed),
        oeffne_inserts,
    );
    kasten
}

/// FX-Menü eines Busses: Returns, Abhören, vaio, Green Mode, Insert.
pub fn bus_menue(p: &Arc<Params>, index: usize, oeffne_inserts: Rc<dyn Fn(Ziel)>) -> gtk::Box {
    let bp = &p.buses[index];
    let name = crate::strip::busname(index);
    let kasten = gtk::Box::new(gtk::Orientation::Vertical, 5);
    kasten.add_css_class("stufenmenue");
    kasten.append(&kopfzeile(&format!("FX · RETURNS AUF {name}")));
    for w in 0..FX_WEGE {
        let (k, _) = pegelregler(
            p,
            "bus",
            index,
            format!("ret_{}", FX_NAMEN[w]),
            FX_TITEL[w],
            f32_von(&bp.fx_ret[w]),
        );
        kasten.append(&k);
    }

    kasten.append(&kopfzeile("BUS"));
    if index != 0 {
        let (z, b) = schalter(
            "Auf A1 abhoeren",
            bp.monitor.load(Relaxed),
            "A1 gibt statt seiner eigenen Mischung diesen Bus aus – hoeren, was z. B. der Stream bekommt",
        );
        let p2 = p.clone();
        b.connect_toggled(move |b| {
            p2.set("bus", index, "monitor", b.is_active() as u8 as f32);
        });
        kasten.append(&z);
    }
    if index < crosspoint::vaio::VAIO_BUSSE {
        let (_, _, titel) = crosspoint::vaio::namen(index);
        let (z, b) = schalter(
            "Als virtuelles Geraet",
            bp.vaio.load(Relaxed),
            &format!("Zusaetzlich als Geraet „{titel}“ – z. B. zum Aufnehmen in OBS"),
        );
        let p2 = p.clone();
        b.connect_toggled(move |b| {
            p2.set("bus", index, "vaio", b.is_active() as u8 as f32);
        });
        kasten.append(&z);
    }
    let (z, b) = schalter(
        "Green Mode",
        bp.green.load(Relaxed),
        "EQ, Limiter und C5 umgehen",
    );
    {
        let p = p.clone();
        b.connect_toggled(move |b| {
            p.set("bus", index, "green", b.is_active() as u8 as f32);
        });
    }
    kasten.append(&z);

    insert_bereich(
        &kasten,
        p,
        "bus",
        Ziel::Bus(index),
        bp.insert.load(Relaxed),
        bp.ins_bereit.load(Relaxed),
        oeffne_inserts,
    );
    kasten
}

/// Insert-Schalter, Zustand des Wegs und Knopf zum Editor.
fn insert_bereich(
    kasten: &gtk::Box,
    p: &Arc<Params>,
    art: &'static str,
    ziel: Ziel,
    an: bool,
    bereit: bool,
    oeffne_inserts: Rc<dyn Fn(Ziel)>,
) {
    kasten.append(&kopfzeile("INSERT"));
    let (z, b) = schalter(
        "Insert-Weg",
        an,
        "Signal vor dem Fader ueber die Insert-Ports hinaus und zurueck (LV2-Kette oder Carla)",
    );
    {
        let p = p.clone();
        let i = ziel.index();
        b.connect_toggled(move |b| {
            p.set(art, i, "insert", b.is_active() as u8 as f32);
        });
    }
    kasten.append(&z);
    let zustand = gtk::Label::new(Some(&insert_zustand(an, bereit)));
    zustand.add_css_class("stufen-hinweis");
    zustand.set_xalign(0.0);
    zustand.set_wrap(true);
    zustand.set_max_width_chars(34);
    kasten.append(&zustand);
    let knopf = gtk::Button::with_label("PLUGINS …");
    knopf.add_css_class("stufenwahl");
    knopf.connect_clicked(move |k| {
        menue_zu(k);
        oeffne_inserts(ziel);
    });
    kasten.append(&knopf);
}

/// Zustand des Insert-Wegs in Worten.
pub fn insert_zustand(an: bool, bereit: bool) -> String {
    match (an, bereit) {
        (false, _) => "aus – das Signal geht direkt durch".to_string(),
        (true, true) => "aktiv – Signal laeuft durch die Kette".to_string(),
        (true, false) => {
            "wartet – Rueckweg nicht verbunden, das Signal geht solange direkt durch".to_string()
        }
    }
}

// ------------------------------------------------------------ Insert-Editor

/// Editor der Insert-Kette eines Kanals.
pub fn insert_fenster(
    parent: &impl IsA<gtk::Window>,
    params: Arc<Params>,
    ziel: Ziel,
) -> gtk::Window {
    let fenster = gtk::Window::builder()
        .title(format!("Crosspoint — Inserts {}", ziel.titel()))
        .transient_for(parent)
        .modal(false)
        .default_width(560)
        .default_height(560)
        .build();
    fenster.add_css_class("dialog");
    let aussen = gtk::Box::new(gtk::Orientation::Vertical, 0);
    aussen.append(&crate::dialogs::kopf(
        &fenster,
        &format!("INSERTS · {}", ziel.titel().to_uppercase()),
        "LV2-Kette ueber mod-host · VST ueber Carla",
    ));
    let mitte = gtk::Box::new(gtk::Orientation::Vertical, 8);
    mitte.set_margin_top(12);
    mitte.set_margin_bottom(12);
    mitte.set_margin_start(12);
    mitte.set_margin_end(12);
    let scroll = gtk::ScrolledWindow::new();
    scroll.set_child(Some(&mitte));
    scroll.set_vexpand(true);
    scroll.set_hscrollbar_policy(gtk::PolicyType::Never);
    aussen.append(&scroll);
    fenster.set_child(Some(&aussen));
    insert_fuellen(&mitte, &params, ziel);
    fenster.present();
    fenster
}

fn hinweis(text: &str) -> gtk::Label {
    let l = gtk::Label::new(Some(text));
    l.add_css_class("dialogunter");
    l.set_xalign(0.0);
    l.set_wrap(true);
    l.set_selectable(true);
    l
}

fn insert_fuellen(mitte: &gtk::Box, params: &Arc<Params>, ziel: Ziel) {
    while let Some(k) = mitte.first_child() {
        mitte.remove(&k);
    }
    let neu_fuellen = {
        let mitte = mitte.clone();
        let params = params.clone();
        Rc::new(move || insert_fuellen(&mitte, &params, ziel))
    };
    let host = fx::status();
    let kette = fx::listen().kette(ziel).to_vec();
    let plugins = fx::plugins();
    let (art, i) = match ziel {
        Ziel::Streifen(i) => ("strip", i),
        Ziel::Bus(i) => ("bus", i),
    };
    let (an, bereit) = match ziel {
        Ziel::Streifen(i) => (
            params.strips[i].insert.load(Relaxed),
            params.strips[i].ins_bereit.load(Relaxed),
        ),
        Ziel::Bus(i) => (
            params.buses[i].insert.load(Relaxed),
            params.buses[i].ins_bereit.load(Relaxed),
        ),
    };

    mitte.append(&kopfzeile("WEG"));
    let (z, b) = schalter("Insert-Weg", an, "Signal durch die Insert-Ports leiten");
    {
        let params = params.clone();
        let neu = neu_fuellen.clone();
        b.connect_toggled(move |b| {
            params.set(art, i, "insert", b.is_active() as u8 as f32);
            neu();
        });
    }
    mitte.append(&z);
    mitte.append(&hinweis(&insert_zustand(an, bereit)));
    let (hinaus, herein) = ziel.insert_ports(&fx::client());
    mitte.append(&hinweis(&format!(
        "Ports: {} / {} → Effekt → {} / {}",
        hinaus[0], hinaus[1], herein[0], herein[1]
    )));

    mitte.append(&kopfzeile("LV2-KETTE (MOD-HOST)"));
    mitte.append(&hinweis(if host.aktiv {
        "mod-host laeuft – Aenderungen wirken sofort, die Ports verbindet Crosspoint selbst."
    } else {
        "mod-host ist nicht erreichbar. Die Kette wird gespeichert und laeuft, sobald mod-host \
         installiert ist und laeuft (Port 5556)."
    }));
    if kette.is_empty() {
        mitte.append(&hinweis("Keine Plugins in der Kette."));
    }
    for (platz, insert) in kette.iter().enumerate() {
        mitte.append(&insert_zeile(
            params,
            ziel,
            platz,
            insert,
            &kette,
            &plugins,
            neu_fuellen.clone(),
        ));
    }

    // Hinzufuegen
    let effekte: Vec<&crosspoint::lv2::Plugin> =
        plugins.iter().filter(|p| p.ist_effekt()).collect();
    if effekte.is_empty() {
        mitte.append(&hinweis(
            "Keine LV2-Effekte gefunden. Auf Fedora z. B.: sudo dnf install lsp-plugins-lv2 x42-plugins \
             – danach im Fenster EFFEKTE „Plugins neu einlesen“.",
        ));
    } else if kette.len() < fx::MAX_INSERTS {
        let zeile = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        let namen: Vec<&str> = effekte.iter().map(|p| p.name.as_str()).collect();
        let dd = gtk::DropDown::from_strings(&namen);
        dd.set_hexpand(true);
        dd.set_enable_search(true);
        let dazu = gtk::Button::with_label("HINZUFUEGEN");
        dazu.add_css_class("tbtn");
        let uris: Vec<String> = effekte.iter().map(|p| p.uri.clone()).collect();
        {
            let dd = dd.clone();
            let kette = kette.clone();
            let neu = neu_fuellen.clone();
            let mitte = mitte.clone();
            dazu.connect_clicked(move |_| {
                let Some(uri) = uris.get(dd.selected() as usize) else {
                    return;
                };
                let mut k = kette.clone();
                k.push(Insert::neu(uri.clone()));
                if let Err(e) = fx::set_chain(ziel, k) {
                    melden(&mitte, &e);
                    return;
                }
                neu();
            });
        }
        zeile.append(&dd);
        zeile.append(&dazu);
        mitte.append(&zeile);
    }

    mitte.append(&kopfzeile("VST (CARLA)"));
    let carla = fx::carla();
    if carla.aktiv() {
        mitte.append(&hinweis(&format!(
            "Carla {}. In Carla ein Plugin laden und im Patchbay {} → Plugin → {} verbinden; \
             der Insert-Weg wird aktiv, sobald der Rueckweg verbunden ist.",
            carla.status(),
            hinaus[0],
            herein[0]
        )));
        let start = gtk::Button::with_label("CARLA STARTEN");
        start.add_css_class("tbtn");
        start.set_halign(gtk::Align::Start);
        let mitte2 = mitte.clone();
        start.connect_clicked(move |_| {
            if let Err(e) = carla.starten() {
                melden(&mitte2, &e);
            }
        });
        mitte.append(&start);
    } else {
        mitte.append(&hinweis(
            "Carla ist nicht installiert (sudo dnf install Carla oder Flatpak studio.kx.carla). \
             Jeder andere JACK-Host geht genauso: Insert-Ports wie oben verbinden.",
        ));
    }
}

/// Fehlermeldung oben in den Editor schreiben.
fn melden(mitte: &gtk::Box, text: &str) {
    let l = gtk::Label::new(Some(text));
    l.add_css_class("fehler");
    l.set_xalign(0.0);
    l.set_wrap(true);
    mitte.prepend(&l);
}

/// Eine Zeile der Kette: Name, Bypass, Reihenfolge, Entfernen, Regler.
fn insert_zeile(
    params: &Arc<Params>,
    ziel: Ziel,
    platz: usize,
    insert: &Insert,
    kette: &[Insert],
    plugins: &[crosspoint::lv2::Plugin],
    neu: Rc<dyn Fn()>,
) -> gtk::Widget {
    let _ = params;
    let plugin = plugins.iter().find(|p| p.uri == insert.uri);
    let kasten = gtk::Box::new(gtk::Orientation::Vertical, 3);
    kasten.add_css_class("insertzeile");
    let kopf = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    let name = gtk::Label::new(Some(&format!(
        "{}. {}",
        platz + 1,
        plugin
            .map(|p| p.name.as_str())
            .unwrap_or(insert.uri.as_str())
    )));
    name.add_css_class("stufen-titel");
    name.set_xalign(0.0);
    name.set_hexpand(true);
    name.set_ellipsize(gtk::pango::EllipsizeMode::End);
    name.set_tooltip_text(Some(&insert.uri));
    kopf.append(&name);

    let bypass = gtk::ToggleButton::with_label("BYPASS");
    bypass.add_css_class("stufenwahl");
    bypass.set_active(insert.bypass);
    bypass.connect_toggled(move |b| {
        let _ = fx::set_bypass(ziel, platz, b.is_active());
    });
    kopf.append(&bypass);

    let kette_v = kette.to_vec();
    let hoch = gtk::Button::with_label("↑");
    hoch.add_css_class("stufenwahl");
    hoch.set_sensitive(platz > 0);
    hoch.set_tooltip_text(Some("Weiter nach vorn in der Kette"));
    {
        let neu = neu.clone();
        let k = kette_v.clone();
        hoch.connect_clicked(move |_| {
            let mut k = k.clone();
            k.swap(platz, platz - 1);
            let _ = fx::set_chain(ziel, k);
            neu();
        });
    }
    kopf.append(&hoch);
    let weg = gtk::Button::with_label("ENTFERNEN");
    weg.add_css_class("stufenwahl");
    {
        let neu = neu.clone();
        let k = kette_v.clone();
        weg.connect_clicked(move |_| {
            let mut k = k.clone();
            k.remove(platz);
            let _ = fx::set_chain(ziel, k);
            neu();
        });
    }
    kopf.append(&weg);
    kasten.append(&kopf);

    match plugin {
        None => {
            let l = gtk::Label::new(Some(
                "Plugin nicht installiert – die Kette laeuft ohne Verbindung.",
            ));
            l.add_css_class("fehler");
            l.set_xalign(0.0);
            kasten.append(&l);
        }
        Some(p) => {
            for r in &p.regler {
                let wert = insert.param(&r.symbol).unwrap_or(r.vorgabe);
                let schritt = ((r.max - r.min) / 200.0).max(0.001) as f64;
                let (k, s, w) = schieber(
                    &r.name,
                    r.min as f64,
                    r.max as f64,
                    schritt,
                    wert as f64,
                    "",
                );
                w.set_text(&format!("{wert:.2}"));
                let symbol = r.symbol.clone();
                s.connect_value_changed(move |s| {
                    w.set_text(&format!("{:.2}", s.value()));
                    let _ = fx::set_param(ziel, platz, &symbol, s.value() as f32);
                });
                kasten.append(&k);
            }
        }
    }
    kasten.upcast()
}

// ---------------------------------------------------------- Fenster EFFEKTE

/// Ein Regler im Fenster EFFEKTE.
struct FxRegler {
    titel: &'static str,
    param: &'static str,
    min: f64,
    max: f64,
    schritt: f64,
    einheit: &'static str,
    stellen: usize,
}

/// Hall (die ersten zwei) und Echo (der Rest), Grenzen wie im Kern.
const FX_REGLER: [FxRegler; 5] = [
    FxRegler {
        titel: "Raum",
        param: "hall_raum",
        min: 0.0,
        max: 1.0,
        schritt: 0.01,
        einheit: "",
        stellen: 2,
    },
    FxRegler {
        titel: "Daempfung",
        param: "hall_daempfung",
        min: 0.0,
        max: 1.0,
        schritt: 0.01,
        einheit: "",
        stellen: 2,
    },
    FxRegler {
        titel: "Zeit",
        param: "echo_zeit",
        min: 10.0,
        max: 2000.0,
        schritt: 5.0,
        einheit: " ms",
        stellen: 0,
    },
    FxRegler {
        titel: "Rueckkopplung",
        param: "echo_rueck",
        min: 0.0,
        max: 0.9,
        schritt: 0.01,
        einheit: "",
        stellen: 2,
    },
    FxRegler {
        titel: "Daempfung",
        param: "echo_daempfung",
        min: 0.0,
        max: 1.0,
        schritt: 0.01,
        einheit: "",
        stellen: 2,
    },
];

/// Einstellungen der eingebauten Send-Effekte und Stand der Effekt-Hosts.
pub fn effekte_fenster(parent: &impl IsA<gtk::Window>, params: Arc<Params>) -> gtk::Window {
    let fenster = gtk::Window::builder()
        .title("Crosspoint — Effekte")
        .transient_for(parent)
        .modal(false)
        .default_width(520)
        .default_height(620)
        .build();
    fenster.add_css_class("dialog");
    let aussen = gtk::Box::new(gtk::Orientation::Vertical, 0);
    aussen.append(&crate::dialogs::kopf(
        &fenster,
        "EFFEKTE",
        "Hall und Echo fuer die Sends · FX1/FX2 und Inserts ueber JACK-Ports",
    ));
    let mitte = gtk::Box::new(gtk::Orientation::Vertical, 6);
    mitte.set_margin_top(12);
    mitte.set_margin_bottom(12);
    mitte.set_margin_start(12);
    mitte.set_margin_end(12);
    let scroll = gtk::ScrolledWindow::new();
    scroll.set_child(Some(&mitte));
    scroll.set_vexpand(true);
    scroll.set_hscrollbar_policy(gtk::PolicyType::Never);
    aussen.append(&scroll);
    fenster.set_child(Some(&aussen));

    let werte = params.fx.werte();
    mitte.append(&kopfzeile("HALL (SEND R)"));
    for (n, r) in FX_REGLER.iter().enumerate() {
        if n == 2 {
            mitte.append(&kopfzeile("ECHO (SEND D)"));
        }
        let wert = werte
            .iter()
            .find(|(k, _)| *k == r.param)
            .map(|(_, v)| *v)
            .unwrap_or(r.min as f32);
        let (k, s, w) = schieber(r.titel, r.min, r.max, r.schritt, wert as f64, r.einheit);
        let (einheit, stellen, param) = (r.einheit, r.stellen, r.param);
        w.set_text(&format!("{:.*}{einheit}", stellen, wert));
        let p = params.clone();
        s.connect_value_changed(move |s| {
            w.set_text(&format!("{:.*}{einheit}", stellen, s.value()));
            p.set("fx", 0, param, s.value() as f32);
        });
        mitte.append(&k);
    }
    mitte.append(&hinweis(
        "Wie viel ein Kanal schickt, steht in seinem FX-Menue (Knopf FX am Kanal). \
         Wo der Effekt ankommt, regeln die Returns im FX-Menue jedes Busses – ab Werk auf A1.",
    ));

    mitte.append(&kopfzeile("FX1 / FX2 (EXTERN)"));
    mitte.append(&hinweis(
        "Sends: crosspoint:fx1_L/R und fx2_L/R · Rueckwege: crosspoint:fx1_ret_L/R und fx2_ret_L/R. \
         Ein beliebiger JACK-Effekt (Carla, Guitarix, …) dazwischen; die Returns stehen im FX-Menue der Busse.",
    ));

    mitte.append(&kopfzeile("EFFEKT-HOSTS"));
    let host = fx::status();
    mitte.append(&hinweis(&format!(
        "mod-host (LV2): {}",
        if host.aktiv {
            format!("laeuft auf Port {}", host.port)
        } else {
            "nicht erreichbar – LV2-Ketten werden gespeichert und spaeter angewandt".to_string()
        }
    )));
    mitte.append(&hinweis(&format!("Carla (VST): {}", fx::carla().status())));
    let anzahl = gtk::Label::new(Some(&format!(
        "{} LV2-Effekte gefunden",
        fx::plugins().len()
    )));
    anzahl.add_css_class("stufen-titel");
    anzahl.set_xalign(0.0);
    let neu = gtk::Button::with_label("PLUGINS NEU EINLESEN");
    neu.add_css_class("tbtn");
    neu.set_halign(gtk::Align::Start);
    {
        let anzahl = anzahl.clone();
        neu.connect_clicked(move |_| {
            let n = fx::plugins_neu_lesen();
            anzahl.set_text(&format!("{n} LV2-Effekte gefunden"));
        });
    }
    mitte.append(&anzahl);
    mitte.append(&neu);

    fenster.present();
    fenster
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn texte_der_regler() {
        assert_eq!(pegel_text(-60.0), "aus");
        assert_eq!(pegel_text(-6.0), "-6.0 dB");
        assert_eq!(pegel_text(0.0), "+0.0 dB");
        assert_eq!(farbe_text(0.0), "neutral");
        assert_eq!(farbe_text(-0.5), "dunkler 50 %");
        assert_eq!(farbe_text(0.25), "heller 25 %");
    }

    #[test]
    fn regler_im_fenster_kennt_der_kern() {
        let p = Params::new(48_000.0, 64);
        for r in &FX_REGLER {
            assert!(p.set("fx", 0, r.param, r.min as f32), "{}", r.param);
            assert!(p.fx.werte().iter().any(|(k, _)| *k == r.param));
        }
    }

    #[test]
    fn insert_zustand_sagt_was_passiert() {
        assert!(insert_zustand(false, true).starts_with("aus"));
        assert!(insert_zustand(true, true).starts_with("aktiv"));
        assert!(insert_zustand(true, false).contains("direkt durch"));
    }
}
