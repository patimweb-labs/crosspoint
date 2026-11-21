//! Das FILTER-Modal: die ganze Kette eines virtuellen Busses an einer Stelle.
//!
//! Ein Modal statt acht Knöpfe am Zug (der Platz reicht nicht, und ein
//! vergessener Schalter fällt in einer Reihe Lämpchen schneller auf als in
//! acht Popups). Aufbau: oben der Pegel vor und nach den Filtern, darunter je
//! Filter eine Karte mit Schalter, Kurzbeschreibung und Reglern – in der
//! Reihenfolge der Signalkette (ADR 0006).
//!
//! Wie überall im Pult: gelesen wird aus [`Oberflaeche`] und [`Kern`],
//! geschrieben nur als [`Aktion`] über `logik::ausfuehren`. Der offene Bus
//! steht im Modell (`Oberflaeche::offen`), nicht in der Ansicht.

use egui::{RichText, Ui, Vec2};

use crosspoint::engine::NR_STUFEN;
use crosspoint::BUS_NAMES;

use crate::bausteine::{self, Art};
use crate::geraete;
use crate::logik::{self, Kern};
use crate::modell::{Aktion, Aufklapp, Geraete, Oberflaeche, Ziel};
use crate::thema::{self, Farben};

fn setzen(ziel: Ziel, param: &str, wert: f32) -> Aktion {
    Aktion::Setzen {
        ziel,
        param: param.into(),
        wert,
    }
}

fn schalter_wert(an: bool) -> f32 {
    if an {
        1.0
    } else {
        0.0
    }
}

/// Das Modal zeichnen, wenn eines offen ist.
pub fn modal(ctx: &egui::Context, f: &Farben, o: &Oberflaeche, k: &Kern, a: &mut Vec<Aktion>) {
    let Some(Aufklapp::Filter(bus)) = o.offen else {
        return;
    };
    let ziel = Ziel::Bus(bus);
    let Some(fw) = logik::filter_von(&k.params, ziel, o.master_bus) else {
        // Ein Bus ohne Filter (A1–A5): es gibt nichts zu zeigen. Der Zustand
        // wird zurückgesetzt, damit das Modal nicht „offen, aber leer“ bleibt.
        a.push(Aktion::Aufklappen(None));
        return;
    };
    let name = logik::name(ziel, o, k);
    let mut schliessen = false;
    let antwort = egui::Modal::new(egui::Id::unique(("filter-modal", bus)))
        .frame(
            bausteine::rahmen(f, f.bg_panel).inner_margin(egui::Margin::same(thema::SPACE_3 as i8)),
        )
        .show(ctx, |ui| {
            ui.set_width(MODAL_BREITE);
            // Drei Spalten: der Platz reicht auch im kleinsten Fenster
            // (logik::mindestfenster), also braucht nichts zu scrollen. Dafür
            // sind die Zeilen im Modal eine Stufe enger als im Pult.
            ui.spacing_mut().slider_width = thema::SPACE_6 * 5.0;
            ui.spacing_mut().interact_size.y = thema::SPACE_5;
            // Kopfzeile: Titel, Pegel vor und nach den Filtern, Schließen –
            // alles in einer Zeile, damit die Karten im kleinsten Fenster
            // Platz haben.
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(format!("FILTER · {} {}", BUS_NAMES[bus], name))
                        .font(egui::FontId::new(thema::FONT_SIZE_MD, thema::fett()))
                        .color(f.text_default),
                );
                ui.separator();
                pegel(ui, f, k, bus);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if bausteine::kopfknopf_icon(
                        ui,
                        f,
                        bausteine::Icon::Schliessen,
                        "SCHLIESSEN",
                        None,
                    )
                    .clicked()
                    {
                        schliessen = true;
                    }
                });
            });
            // Was das Modal zeigt und in welcher Reihenfolge – eine Zeile,
            // damit die Karten auch im kleinsten Fenster Platz haben.
            ui.label(bausteine::text(f, KETTE, false, true).size(thema::FONT_SIZE_SM));
            ui.separator();
            // Spalte für Spalte in der Reihenfolge der Signalkette; die
            // Karten sind so verteilt, dass keine Spalte überläuft.
            let karten = Karten {
                f,
                o,
                bus,
                ziel,
                fw,
            };
            ui.columns(3, |spalten| {
                for (i, (von, bis)) in SPALTEN.iter().enumerate() {
                    a.extend(kette(&mut spalten[i], &karten, *von, *bis));
                }
            });
        });
    if antwort.should_close() || schliessen {
        a.push(Aktion::Aufklappen(None));
    }
}

/// Unter dem Titel: was gefiltert wird und in welcher Reihenfolge.
const KETTE: &str = "Eingänge und A1–A5 tragen das Rohsignal · Kette: LC · ECHO · NR · GATE \
· COMP · AGC · LIMITER · DUCK · PTT";

/// Breite des Modals: drei Spalten nebeneinander, jede breit genug für eine
/// Reglerzeile (Beschriftung, Schieber, Wert).
const MODAL_BREITE: f32 = thema::SPACE_6 * 36.0;

/// Welche Karten in welcher Spalte stehen (Index in der Signalkette): links
/// die Eingriffe vorn, in der Mitte Gate und Kompressor, rechts die Regler
/// hinten. So bleibt jede Spalte ungefähr gleich hoch.
const SPALTEN: [(usize, usize); 3] = [(0, 3), (3, 5), (5, 8)];

/// Pegel vor und nach den Filtern („roh“ = Summe am Bus-Eingang), beide in
/// einer Zeile.
fn pegel(ui: &mut Ui, f: &Farben, k: &Kern, bus: usize) {
    let (roh, nachher) = logik::filter_pegel(&k.params, bus);
    ui.horizontal(|ui| {
        for (titel, linear) in [("ROH", roh), ("NACHHER", nachher)] {
            let db = crate::fmt::lin_to_db(linear);
            ui.label(bausteine::text(f, titel, true, true));
            bausteine::segmentleiste(ui, f, db, 24);
            ui.label(bausteine::text(f, &crate::fmt::db_text(db), false, false));
        }
    });
}

/// Was alle Karten brauchen: Farben, Bestand und der Bus mit seinen Werten.
struct Karten<'a> {
    f: &'a Farben,
    o: &'a Oberflaeche,
    bus: usize,
    ziel: Ziel,
    fw: logik::FilterWerte,
}

/// Die acht Karten in der Reihenfolge der Signalkette. Als Tabelle, damit
/// die Spalten des Modals nur Bereiche daraus zeigen ("[`SPALTEN`]").
const KARTEN: [fn(&mut Ui, &Karten) -> Vec<Aktion>; 8] = [lc, echo, nr, gate, comp, agc, duck, ptt];

/// Eine Spalte des Modals: die Karten `von..bis` untereinander.
fn kette(ui: &mut Ui, karten: &Karten, von: usize, bis: usize) -> Vec<Aktion> {
    let mut a = Vec::new();
    for karte in &KARTEN[von..bis.min(KARTEN.len())] {
        a.extend(karte(ui, karten));
    }
    a
}

fn lc(ui: &mut Ui, k: &Karten) -> Vec<Aktion> {
    let (f, ziel, fw) = (k.f, k.ziel, k.fw);
    let mut a = Vec::new();
    a.extend(karte(
        ui,
        f,
        "LC",
        "LOW-CUT",
        "Alles unter der Grenze weg: Rumpeln, Trittschall, Lüfterbrummen.",
        fw.lc,
        ziel,
        "lc",
        |ui, a| {
            regler(
                ui, f, ziel, a, "Grenze", "lc_freq", 40.0, 200.0, fw.lc_freq, " Hz", 0,
            );
        },
    ));
    a
}

fn echo(ui: &mut Ui, k: &Karten) -> Vec<Aktion> {
    let (f, ziel, fw, o) = (k.f, k.ziel, k.fw, k.o);
    let mut a = Vec::new();
    a.extend(karte(
        ui,
        f,
        "ECHO",
        "ECHOUNTERDRÜCKUNG",
        "Rechnet den Ton der gewählten Ausgabegeräte aus dem Mikrofon heraus.",
        fw.aec,
        ziel,
        "aec",
        |ui, a| {
            ui.label(bausteine::text(f, "Was spielt auf:", false, true).size(thema::FONT_SIZE_SM));
            for e in ausgabegeraete(o) {
                let mut wert = o.echo_geraete.contains(&e.name);
                if ui
                    .add(egui::Checkbox::new(&mut wert, eintrag_text(&e)))
                    .on_hover_text(&e.name)
                    .changed()
                {
                    a.push(Aktion::EchoGeraet {
                        name: e.name.clone(),
                        an: wert,
                    });
                }
            }
            ui.label(
                bausteine::text(f, "Die Referenz kommt vom Ausgabegerät.", false, true)
                    .size(thema::FONT_SIZE_SM),
            );
        },
    ));
    a
}

fn nr(ui: &mut Ui, k: &Karten) -> Vec<Aktion> {
    let (f, ziel, fw) = (k.f, k.ziel, k.fw);
    let mut a = Vec::new();
    a.extend(karte(
        ui,
        f,
        "NR",
        "RAUSCHUNTERDRÜCKUNG",
        "Lüfter, Tastatur, Maus. Unabhängig von ECHO; macht den Klang rauer.",
        fw.nr,
        ziel,
        "nr",
        |ui, a| {
            ui.horizontal(|ui| {
                ui.label(bausteine::text(f, "Stufe", false, true));
                for (i, stufe) in NR_STUFEN.iter().enumerate() {
                    let gewaehlt = fw.nr_stufe as usize == i;
                    if ui.selectable_label(gewaehlt, *stufe).clicked() && !gewaehlt {
                        a.push(setzen(ziel, "nr_stufe", i as f32));
                    }
                }
            });
        },
    ));
    a
}

fn gate(ui: &mut Ui, k: &Karten) -> Vec<Aktion> {
    let (f, ziel, fw) = (k.f, k.ziel, k.fw);
    let mut a = Vec::new();
    a.extend(karte(
        ui,
        f,
        "GATE",
        "NOISE GATE",
        "Stille unter der Schwelle: Pausen zwischen den Sätzen.",
        fw.gate,
        ziel,
        "gate",
        |ui, a| {
            regler(
                ui,
                f,
                ziel,
                a,
                "Schwelle",
                "gate_schwelle",
                -80.0,
                -10.0,
                fw.gate_schwelle,
                " dBFS",
                0,
            );
            regler(
                ui,
                f,
                ziel,
                a,
                "Tiefe",
                "gate_tiefe",
                0.0,
                80.0,
                fw.gate_tiefe,
                " dB",
                0,
            );
            regler(
                ui,
                f,
                ziel,
                a,
                "Attack",
                "gate_attack",
                0.5,
                100.0,
                fw.gate_attack,
                " ms",
                1,
            );
            regler(
                ui,
                f,
                ziel,
                a,
                "Hold",
                "gate_hold",
                0.0,
                500.0,
                fw.gate_hold,
                " ms",
                0,
            );
            regler(
                ui,
                f,
                ziel,
                a,
                "Release",
                "gate_release",
                5.0,
                1000.0,
                fw.gate_release,
                " ms",
                0,
            );
        },
    ));
    a
}

fn comp(ui: &mut Ui, k: &Karten) -> Vec<Aktion> {
    let (f, ziel, fw) = (k.f, k.ziel, k.fw);
    let mut a = Vec::new();
    a.extend(karte(
        ui,
        f,
        "COMP",
        "KOMPRESSOR UND LIMITER",
        "Laute Stellen zusammen, leise bleiben. Der Limiter hält die Decke.",
        fw.comp,
        ziel,
        "comp",
        |ui, a| {
            regler(
                ui,
                f,
                ziel,
                a,
                "Schwelle",
                "comp_schwelle",
                -60.0,
                0.0,
                fw.comp_schwelle,
                " dBFS",
                0,
            );
            regler(
                ui,
                f,
                ziel,
                a,
                "Ratio",
                "comp_ratio",
                1.0,
                20.0,
                fw.comp_ratio,
                " : 1",
                1,
            );
            regler(
                ui,
                f,
                ziel,
                a,
                "Attack",
                "comp_attack",
                0.1,
                100.0,
                fw.comp_attack,
                " ms",
                1,
            );
            regler(
                ui,
                f,
                ziel,
                a,
                "Release",
                "comp_release",
                5.0,
                1000.0,
                fw.comp_release,
                " ms",
                0,
            );
            regler(
                ui,
                f,
                ziel,
                a,
                "Make-up",
                "comp_makeup",
                0.0,
                24.0,
                fw.comp_makeup,
                " dB",
                1,
            );
            ui.horizontal(|ui| {
                let breite = Vec2::new(thema::SPACE_6 * 2.0, thema::SIZE_CONTROL_SM);
                if bausteine::schalter(ui, f, "LIMITER", fw.limiter, Art::Akzent, breite).clicked()
                {
                    a.push(setzen(ziel, "limiter", schalter_wert(!fw.limiter)));
                }
                regler(
                    ui,
                    f,
                    ziel,
                    a,
                    "Decke",
                    "limiter_decke",
                    -12.0,
                    0.0,
                    fw.limiter_decke,
                    " dBFS",
                    1,
                );
            });
        },
    ));
    a
}

fn agc(ui: &mut Ui, k: &Karten) -> Vec<Aktion> {
    let (f, ziel, fw) = (k.f, k.ziel, k.fw);
    let mut a = Vec::new();
    a.extend(karte(
        ui,
        f,
        "AGC",
        "AUTOMATISCHE PEGELANPASSUNG",
        "Zieht die Sprache auf den Zielpegel – egal wie laut man spricht.",
        fw.agc,
        ziel,
        "agc",
        |ui, a| {
            regler(
                ui,
                f,
                ziel,
                a,
                "Zielpegel",
                "agc_ziel",
                -40.0,
                0.0,
                fw.agc_ziel,
                " dBFS",
                0,
            );
        },
    ));
    a
}

fn duck(ui: &mut Ui, k: &Karten) -> Vec<Aktion> {
    let (f, ziel, fw, bus) = (k.f, k.ziel, k.fw, k.bus);
    let mut a = Vec::new();
    a.extend(karte(
        ui,
        f,
        "DUCK",
        "DUCKING",
        "Sprichst du hier, wird ein anderer Bus leiser (z. B. der Spielton).",
        fw.duck,
        ziel,
        "duck",
        |ui, a| {
            ui.horizontal(|ui| {
                let l = ui.label(bausteine::text(f, "Zielbus", false, true));
                egui::ComboBox::from_id_salt(("duck_ziel", bus))
                    .selected_text(BUS_NAMES[fw.duck_ziel])
                    .show_ui(ui, |ui| {
                        for (b, busname) in BUS_NAMES.iter().enumerate() {
                            if ui.selectable_label(fw.duck_ziel == b, *busname).clicked()
                                && fw.duck_ziel != b
                            {
                                a.push(setzen(ziel, "duck_ziel", b as f32));
                            }
                        }
                    })
                    .response
                    .labelled_by(l.id);
            });
            regler(
                ui,
                f,
                ziel,
                a,
                "Absenkung",
                "duck_tiefe",
                0.0,
                48.0,
                fw.duck_tiefe,
                " dB",
                0,
            );
            regler(
                ui,
                f,
                ziel,
                a,
                "Schwelle",
                "duck_schwelle",
                -60.0,
                0.0,
                fw.duck_schwelle,
                " dBFS",
                0,
            );
            regler(
                ui,
                f,
                ziel,
                a,
                "Attack",
                "duck_attack",
                0.5,
                500.0,
                fw.duck_attack,
                " ms",
                0,
            );
            regler(
                ui,
                f,
                ziel,
                a,
                "Release",
                "duck_release",
                5.0,
                3000.0,
                fw.duck_release,
                " ms",
                0,
            );
        },
    ));
    a
}

fn ptt(ui: &mut Ui, k: &Karten) -> Vec<Aktion> {
    let (f, ziel, fw) = (k.f, k.ziel, k.fw);
    let mut a = Vec::new();
    a.extend(karte(
        ui,
        f,
        "PTT",
        "PUSH-TO-TALK",
        "Bus stumm, solange die Taste nicht gedrückt ist (MIDI/OSC: ptt_offen).",
        fw.ptt,
        ziel,
        "ptt",
        |ui, a| {
            ui.horizontal(|ui| {
                let breite = Vec2::new(thema::SPACE_6 * 2.0, thema::SIZE_CONTROL_SM);
                let offen = bausteine::schalter(ui, f, "OFFEN", fw.ptt_offen, Art::Akzent, breite)
                    .on_hover_text("Taste gedrückt: der Bus ist hörbar");
                if offen.clicked() {
                    a.push(setzen(ziel, "ptt_offen", schalter_wert(!fw.ptt_offen)));
                }
                ui.label(
                    bausteine::text(
                        f,
                        "Auf der Bühne über MIDI oder /bus/<n>/ptt_offen.",
                        false,
                        true,
                    )
                    .size(thema::FONT_SIZE_SM),
                );
            });
        },
    ));
    a
}

/// Eine Filterkarte: Titel mit Schalter, Kurzbeschreibung, Regler.
#[allow(clippy::too_many_arguments)]
fn karte(
    ui: &mut Ui,
    f: &Farben,
    kurz: &str,
    titel: &str,
    beschreibung: &str,
    an: bool,
    ziel: Ziel,
    param: &str,
    inhalt: impl FnOnce(&mut Ui, &mut Vec<Aktion>),
) -> Vec<Aktion> {
    let mut schalter = None;
    let mut vec = Vec::new();
    bausteine::rahmen(f, f.bg_raised)
        .inner_margin(egui::Margin::same(thema::SPACE_2 as i8))
        .show(ui, |ui| {
            // Alle Karten einer Spalte sind gleich breit, damit die Kanten
            // untereinander stehen.
            ui.set_min_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(format!("{kurz} · {titel}"))
                        .font(egui::FontId::new(thema::FONT_SIZE_SM, thema::fett()))
                        .color(if an { f.accent_default } else { f.text_muted }),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let breite = Vec2::new(thema::SPACE_6 * 2.0, thema::SIZE_CONTROL_SM);
                    if bausteine::schalter(ui, f, kurz, an, Art::Akzent, breite).clicked() {
                        schalter = Some(!an);
                    }
                });
            });
            ui.label(bausteine::text(f, beschreibung, false, true).size(thema::FONT_SIZE_SM));
            inhalt(ui, &mut vec);
        });
    ui.add_space(thema::SPACE_1);
    if let Some(an) = schalter {
        vec.push(setzen(ziel, param, schalter_wert(an)));
    }
    vec
}

/// Eine Reglerzeile: Beschriftung links, Schieber mit Wert rechts.
#[allow(clippy::too_many_arguments)]
fn regler(
    ui: &mut Ui,
    f: &Farben,
    ziel: Ziel,
    a: &mut Vec<Aktion>,
    titel: &str,
    param: &str,
    von: f32,
    bis: f32,
    wert: f32,
    suffix: &str,
    dezimal: usize,
) {
    ui.horizontal(|ui| {
        ui.add_sized(
            [thema::SPACE_6 * 2.5, thema::SIZE_CONTROL_SM],
            egui::Label::new(bausteine::text(f, titel, false, true)),
        );
        let mut v = wert;
        let antwort = ui.add(
            egui::Slider::new(&mut v, von..=bis)
                .suffix(suffix)
                .fixed_decimals(dezimal),
        );
        let name = titel.to_string();
        antwort.widget_info(|| egui::WidgetInfo::slider(true, v as f64, name.clone()));
        if antwort.changed() {
            a.push(setzen(ziel, param, v));
        }
    });
}

/// Ausgabegeräte für die ECHO-Auswahl: alle Sinks aus dem Bestand, dazu die
/// gemerkten, die gerade fehlen.
fn ausgabegeraete(o: &Oberflaeche) -> Vec<geraete::Eintrag> {
    let bestand = match &o.geraete {
        Geraete::Fertig(b) => Some(b.as_ref()),
        Geraete::Laedt => None,
    };
    let mut liste = bestand.map(|b| b.senken.clone()).unwrap_or_default();
    for name in &o.echo_geraete {
        if liste.iter().any(|e| e.name == *name) {
            continue;
        }
        liste.push(geraete::Eintrag {
            name: name.clone(),
            anzeige: geraete::kurzname(name),
            fehlt: true,
        });
    }
    liste
}

fn eintrag_text(e: &geraete::Eintrag) -> String {
    geraete::eintrag_text(&e.anzeige, e.fehlt)
}
