//! Ein Zug des Pults: Kanal, Bus oder Master.
//!
//! Aufbau von oben: Gerätewahl, Kopf (Name, Nummer, Unterzeile), Faderblock,
//! beim Kanal Pan und Zuordnung A1..B3, unten die Schalter. Die Schalter
//! stehen bei allen Zügen auf derselben Höhe; der Fader nimmt den Rest.
//!
//! An den virtuellen Bussen (und am Master, wenn er einen bedient) stehen
//! GATE und ECHO: Linksklick schaltet, Rechtsklick öffnet die Einstellungen.

use egui::{Pos2, Rect, RichText, Ui, UiBuilder, Vec2};

use crosspoint::engine::ERSTER_VIRTUELLER;
use crosspoint::BUS_NAMES;

use crate::bausteine::{self, Art};
use crate::geraete::{self, KEIN_AUSGANG, KEIN_EINGANG};
use crate::logik::{self, FilterWerte, Kern};
use crate::modell::{Aktion, Aufklapp, Geraete, Oberflaeche, Pegel, Ziel};
use crate::thema::{self, Farben};

/// Höhe der Lämpchenreihe am FILTER-Knopf (eine Zeile Anfangsbuchstaben);
/// das Maß steht in [`logik`], weil die Mindestgröße damit rechnet.
pub const LAEMPCHEN: f32 = logik::ZUG_LAEMPCHEN;

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

/// Zeichnet einen Zug in `rect`.
pub fn zug(
    ui: &mut Ui,
    f: &Farben,
    rect: Rect,
    ziel: Ziel,
    o: &Oberflaeche,
    k: &Kern,
    a: &mut Vec<Aktion>,
) {
    let rahmen = bausteine::rahmen(f, f.bg_panel);
    ui.scope_builder(UiBuilder::new().max_rect(rect).id_salt(ziel), |ui| {
        rahmen.show(ui, |ui| {
            ui.set_min_size(rect.size() - Vec2::splat(2.0 * thema::SIZE_BORDER));
            ui.spacing_mut().item_spacing = Vec2::splat(thema::SPACE_1);
            let innen = ui.max_rect();
            inhalt(ui, f, innen, ziel, o, k, a);
        });
    });
}

fn inhalt(
    ui: &mut Ui,
    f: &Farben,
    r: Rect,
    ziel: Ziel,
    o: &Oberflaeche,
    k: &Kern,
    a: &mut Vec<Aktion>,
) {
    let pad = logik::ZUG_RAND;
    let breite = r.width() - 2.0 * pad;
    let links = r.left() + pad;
    let ist_kanal = matches!(ziel, Ziel::Kanal(_));
    let werte = logik::zug_werte(&k.params, ziel, o.master_bus);
    let filter = logik::filter_von(&k.params, ziel, o.master_bus);
    // Was über und unter dem Fader steht, steht in `logik::ZugMass` – dieselbe
    // Rechnung ergibt die Mindestgröße des Fensters.
    let mass = logik::ZugMass::fuer(filter.is_some(), ist_kanal);
    let mut y = r.top() + pad;

    // Gerätewahl (Hardware-Kanäle und -Busse); sonst bleibt die Zeile frei,
    // damit alle Köpfe auf einer Höhe stehen.
    let geraet = Rect::from_min_size(Pos2::new(links, y), Vec2::new(breite, logik::ZUG_GERAET));
    geraetewahl(ui, f, geraet, ziel, o, a);
    y += logik::ZUG_GERAET + pad;

    // Kopf
    let kopf = Rect::from_min_size(
        Pos2::new(r.left(), y),
        Vec2::new(r.width(), logik::ZUG_KOPF),
    );
    zugkopf(ui, f, kopf, ziel, o, k, a);
    y += logik::ZUG_KOPF + pad;

    // Fader: der Rest der Höhe, oben und unten von den festen Zeilen begrenzt.
    let fader = Rect::from_min_size(
        Pos2::new(links, y),
        Vec2::new(breite, mass.fader(r.height())),
    );
    y = fader.bottom();
    let idx = Pegel::index(ziel);
    let name = logik::name(ziel, o, k);
    ui.scope_builder(UiBuilder::new().max_rect(fader), |ui| {
        let antw = bausteine::fader(
            ui,
            f,
            &name,
            fader.size(),
            werte.gain,
            o.pegel.db[idx],
            o.pegel.spitze[idx],
        );
        if let Some(neu) = antw.neu {
            a.push(setzen(ziel, "gain", neu));
        }
    });

    // Pan und Zuordnung gibt es nur am Kanal; danach folgen bei allen Zügen
    // die Schalterreihen und beim Filterbus die Lämpchenzeile.
    let pan_y = if ist_kanal {
        let oben = y;
        y += thema::SIZE_PAN + thema::SPACE_5 + pad;
        Some(oben)
    } else {
        None
    };
    let zuordnung_y = if ist_kanal {
        let oben = y;
        y += 2.0 * (logik::ZUG_ZUORDNUNG + pad);
        Some(oben)
    } else {
        None
    };
    let schalter_y = y;
    let lampen_y = schalter_y + 2.0 * (logik::ZUG_REIHE + pad);

    if let (Some(py), Some(pan)) = (pan_y, werte.pan) {
        let rp = Rect::from_min_size(
            Pos2::new(links, py),
            Vec2::new(breite, thema::SIZE_PAN + thema::SPACE_5),
        );
        ui.scope_builder(UiBuilder::new().max_rect(rp), |ui| {
            let (_, neu) = bausteine::pan(ui, f, &name, breite, pan);
            if let Some(neu) = neu {
                a.push(setzen(ziel, "pan", neu));
            }
        });
    }

    if let (Some(zy), Ziel::Kanal(kanal)) = (zuordnung_y, ziel) {
        let w = (breite - 3.0 * pad) / 4.0;
        for (bus, bus_name) in BUS_NAMES.iter().enumerate() {
            let (reihe, spalte) = (bus / 4, bus % 4);
            let rb = Rect::from_min_size(
                Pos2::new(
                    links + spalte as f32 * (w + pad),
                    zy + reihe as f32 * (logik::ZUG_ZUORDNUNG + pad),
                ),
                Vec2::new(w, logik::ZUG_ZUORDNUNG),
            );
            let an = crate::state::send_an(&k.params, kanal, bus);
            ui.scope_builder(UiBuilder::new().max_rect(rb), |ui| {
                // An ist der Kreuzungspunkt Kanal → Bus: bernsteinfarben, das
                // zweite Signal der Marke (ADR 0008).
                let resp = bausteine::schalter(ui, f, bus_name, an, Art::Signal, rb.size())
                    .on_hover_text(format!("{name} → {}", *bus_name));
                if resp.clicked() {
                    a.push(Aktion::Routing {
                        kanal,
                        bus,
                        an: !an,
                    });
                }
            });
        }
    }

    // Schalter: zwei Reihen à zwei
    let halb = (breite - pad) / 2.0;
    let platz = |reihe: usize, spalte: usize, spalten: usize| {
        let w = if spalten == 1 { breite } else { halb };
        Rect::from_min_size(
            Pos2::new(
                links + spalte as f32 * (halb + pad),
                schalter_y + reihe as f32 * (logik::ZUG_REIHE + pad),
            ),
            Vec2::new(w, logik::ZUG_REIHE),
        )
    };
    let knopf = |ui: &mut Ui, rb: Rect, text: &str, an: bool, art: Art| {
        ui.scope_builder(UiBuilder::new().max_rect(rb), |ui| {
            bausteine::schalter(ui, f, text, an, art, rb.size())
        })
        .inner
    };

    if knopf(ui, platz(0, 0, 2), "M", werte.mute, Art::Akzent).clicked() {
        a.push(setzen(ziel, "mute", schalter_wert(!werte.mute)));
    }
    match ziel {
        Ziel::Kanal(_) => {
            let solo = werte.solo.unwrap_or(false);
            if knopf(ui, platz(0, 1, 2), "S", solo, Art::Solo).clicked() {
                a.push(setzen(ziel, "solo", schalter_wert(!solo)));
            }
            if knopf(ui, platz(1, 0, 1), "MONO", werte.mono, Art::Tief).clicked() {
                a.push(setzen(ziel, "mono", schalter_wert(!werte.mono)));
            }
        }
        Ziel::Bus(_) => {
            if knopf(ui, platz(0, 1, 2), "MONO", werte.mono, Art::Tief).clicked() {
                a.push(setzen(ziel, "mono", schalter_wert(!werte.mono)));
            }
            if let Some(fw) = filter {
                filter_knopf(
                    ui,
                    f,
                    platz(1, 0, 1),
                    Rect::from_min_size(Pos2::new(links, lampen_y), Vec2::new(breite, LAEMPCHEN)),
                    ziel,
                    o,
                    fw,
                    a,
                );
            }
        }
        Ziel::Master => {
            let sel = ui
                .scope_builder(UiBuilder::new().max_rect(platz(0, 1, 2)), |ui| {
                    bausteine::schalter(ui, f, "SEL", false, Art::Akzent, platz(0, 1, 2).size())
                })
                .inner
                .on_hover_text("Bus für den Master wählen");
            egui::Popup::menu(&sel).show(|ui| {
                for (b, bus_name) in BUS_NAMES.iter().enumerate() {
                    if ui.selectable_label(o.master_bus == b, *bus_name).clicked() {
                        a.push(Aktion::MasterBus(b));
                    }
                }
            });
            match filter {
                Some(fw) => {
                    filter_knopf(
                        ui,
                        f,
                        platz(1, 0, 1),
                        Rect::from_min_size(
                            Pos2::new(links, lampen_y),
                            Vec2::new(breite, LAEMPCHEN),
                        ),
                        ziel,
                        o,
                        fw,
                        a,
                    );
                }
                None => {
                    if knopf(ui, platz(1, 0, 2), "MONO", werte.mono, Art::Tief).clicked() {
                        a.push(setzen(ziel, "mono", schalter_wert(!werte.mono)));
                    }
                    let td = knopf(ui, platz(1, 1, 2), "T/D", false, Art::Akzent)
                        .on_hover_text("Trim und Delay");
                    aufklapp(f, &td, Aufklapp::TrimDelay, o, k, a);
                }
            }
        }
    }
}

/// FILTER-Knopf und Lämpchenreihe eines virtuellen Busses.
///
/// Links- wie Rechtsklick öffnen das FILTER-Modal; dort stehen Schalter und
/// Regler. Der Knopf selbst schaltet nichts – er zeigt nur, ob etwas an ist.
#[allow(clippy::too_many_arguments)]
fn filter_knopf(
    ui: &mut Ui,
    f: &Farben,
    knopf: Rect,
    lampen: Rect,
    ziel: Ziel,
    o: &Oberflaeche,
    fw: FilterWerte,
    a: &mut Vec<Aktion>,
) {
    let bus = logik::bus_von(ziel, o.master_bus).unwrap_or(ERSTER_VIRTUELLER);
    let stand = fw.stand();
    let antwort = ui
        .scope_builder(UiBuilder::new().max_rect(knopf), |ui| {
            bausteine::schalter_icon(
                ui,
                f,
                Some(bausteine::Icon::Regler),
                "FILTER",
                fw.irgendwas_an(),
                Art::Akzent,
                knopf.size(),
            )
        })
        .inner
        .on_hover_text(format!("Filter von {} · {stand}", BUS_NAMES[bus]));
    if antwort.clicked() || antwort.secondary_clicked() {
        a.push(Aktion::Aufklappen(Some(Aufklapp::Filter(bus))));
    }
    ui.scope_builder(UiBuilder::new().max_rect(lampen), |ui| {
        bausteine::laempchen(
            ui,
            f,
            lampen.size(),
            BUS_NAMES[bus],
            &fw.laempchen(),
            &stand,
        );
    });
}

/// Trim/Delay des Master-Busses.
fn aufklapp(
    f: &Farben,
    knopf: &egui::Response,
    was: Aufklapp,
    o: &Oberflaeche,
    k: &Kern,
    a: &mut Vec<Aktion>,
) {
    if knopf.clicked() {
        a.push(Aktion::Aufklappen(if o.offen == Some(was) {
            None
        } else {
            Some(was)
        }));
    }
    let mut offen = o.offen == Some(was);
    if !offen {
        return;
    }
    let (trim, delay) = logik::trim_delay(&k.params, o.master_bus);
    egui::Popup::from_response(knopf)
        .open_bool(&mut offen)
        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
        .show(|ui| {
            ui.set_min_width(thema::SPACE_6 * 7.0);
            ui.label(bausteine::text(
                f,
                &format!("TRIM · DELAY {}", BUS_NAMES[o.master_bus]),
                false,
                true,
            ));
            let mut t = trim;
            if regler_im_menue(ui, f, "TRIM", &mut t, -24.0..=12.0, " dB", 1) {
                a.push(setzen(Ziel::Master, "trim", t));
            }
            let mut d = delay;
            if regler_im_menue(ui, f, "DELAY", &mut d, 0.0..=500.0, " ms", 0) {
                a.push(setzen(Ziel::Master, "delay", d));
            }
        });
    if !offen {
        a.push(Aktion::Aufklappen(None));
    }
}

/// Ein Regler im Aufklappmenü (TRIM · DELAY): sichtbare Schiene in den Farben
/// des Modals, Wert im Feld daneben. Der Name steht im Screenreader-Baum, denn
/// „TRIM · DELAY B1“ ist die Überschrift über zwei baugleichen Reglern.
///
/// Gibt zurück, ob der Wert geändert wurde.
fn regler_im_menue(
    ui: &mut egui::Ui,
    f: &Farben,
    name: &str,
    wert: &mut f32,
    bereich: std::ops::RangeInclusive<f32>,
    suffix: &str,
    dezimal: usize,
) -> bool {
    let antwort = ui.scope(|ui| {
        bausteine::sichtbare_schiene(ui, f);
        let antwort = ui
            .add(
                egui::Slider::new(wert, bereich)
                    .suffix(suffix)
                    .fixed_decimals(dezimal),
            )
            .on_hover_text(name);
        // Ohne eigenen Namen meldet egui den Regler ohne Beschriftung: das
        // Wort steht im Menü nur in der Überschrift.
        antwort.widget_info(|| egui::WidgetInfo::slider(ui.is_enabled(), *wert as f64, name));
        antwort
    });
    antwort.inner.changed()
}

fn zugkopf(
    ui: &mut Ui,
    f: &Farben,
    r: Rect,
    ziel: Ziel,
    o: &Oberflaeche,
    k: &Kern,
    a: &mut Vec<Aktion>,
) {
    let p = ui.painter_at(r);
    p.rect_filled(r, egui::CornerRadius::ZERO, f.bg_raised);
    let linie = if ziel == Ziel::Master {
        egui::Stroke::new(thema::SIZE_FOCUS, f.accent_default)
    } else {
        egui::Stroke::new(thema::SIZE_BORDER, f.border_subtle)
    };
    p.line_segment([r.left_bottom(), r.right_bottom()], linie);
    let innen = r.shrink2(Vec2::new(thema::SPACE_1 + 1.0, 3.0));
    let name = logik::name(ziel, o, k);

    if let Some(u) = o.umbenennen.as_ref().filter(|u| u.ziel == ziel) {
        let mut text = u.text.clone();
        let feld = Rect::from_min_size(
            innen.min,
            Vec2::new(innen.width(), thema::SIZE_CONTROL_SM - 4.0),
        );
        let resp = ui.put(
            feld,
            egui::TextEdit::singleline(&mut text)
                .char_limit(crosspoint::engine::MAX_LABEL)
                .font(egui::FontId::new(thema::FONT_SIZE_MD, thema::fett())),
        );
        resp.request_focus();
        if resp.changed() {
            a.push(Aktion::UmbenennenText(text));
        }
        if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            a.push(Aktion::UmbenennenEnde(false));
        } else if resp.lost_focus() {
            a.push(Aktion::UmbenennenEnde(true));
        }
        return;
    }

    let titel = Rect::from_min_size(
        innen.min,
        Vec2::new(innen.width(), thema::FONT_SIZE_MD + 4.0),
    );
    let resp = ui.interact(
        titel,
        ui.make_persistent_id(("name", ziel)),
        egui::Sense::click(),
    );
    resp.widget_info(|| {
        egui::WidgetInfo::labeled(
            egui::Role::Label,
            true,
            format!("{name} (Doppelklick: umbenennen)"),
        )
    });
    if resp.double_clicked() && ziel != Ziel::Master {
        a.push(Aktion::UmbenennenBeginnen(ziel));
    }
    let nummer = logik::nummer(ziel);
    let nummer_w = thema::FONT_SIZE_SM * 0.62 * nummer.len() as f32 + thema::SPACE_1;
    let max_zeichen = ((innen.width() - nummer_w) / (thema::FONT_SIZE_MD * 0.62)).max(3.0) as usize;
    p.text(
        innen.left_top(),
        egui::Align2::LEFT_TOP,
        crate::fmt::kurz(&name, max_zeichen),
        egui::FontId::new(thema::FONT_SIZE_MD, thema::fett()),
        f.text_default,
    );
    p.text(
        innen.right_top(),
        egui::Align2::RIGHT_TOP,
        nummer,
        egui::FontId::new(thema::FONT_SIZE_SM, thema::fett()),
        f.text_muted,
    );
    p.text(
        innen.left_bottom(),
        egui::Align2::LEFT_BOTTOM,
        logik::unterzeile(ziel),
        egui::FontId::monospace(thema::FONT_SIZE_SM),
        f.text_muted,
    );
    if name.chars().count() > max_zeichen {
        resp.on_hover_text(name);
    }
}

fn geraetewahl(ui: &mut Ui, f: &Farben, r: Rect, ziel: Ziel, o: &Oberflaeche, a: &mut Vec<Aktion>) {
    let (index, quelle) = match ziel {
        Ziel::Kanal(i) if i < 5 => (i, true),
        Ziel::Bus(i) if i < ERSTER_VIRTUELLER => (i, false),
        _ => return,
    };
    let Geraete::Fertig(bestand) = &o.geraete else {
        ui.put(r, egui::Label::new(bausteine::text(f, "…", false, true)));
        return;
    };
    let (liste, aktuell, fehlt, leer) = if quelle {
        (
            &bestand.quellen,
            bestand.streifen.get(index).cloned().flatten(),
            bestand.streifen_fehlt.get(index).copied().unwrap_or(false),
            KEIN_EINGANG,
        )
    } else {
        (
            &bestand.senken,
            bestand.busse.get(index).and_then(|(_, s)| s.clone()),
            bestand.busse_fehlt.get(index).copied().unwrap_or(false),
            KEIN_AUSGANG,
        )
    };
    let anzeige = aktuell
        .as_ref()
        .map(|n| {
            liste
                .iter()
                .find(|e| &e.name == n)
                .map(|e| e.anzeige.clone())
                .unwrap_or_else(|| geraete::kurzname(n))
        })
        .unwrap_or_else(|| "—".to_string());
    let farbe = if fehlt { f.status_warn } else { f.text_muted };
    let tipp = if fehlt {
        format!("{anzeige}: {}", geraete::FEHLT_TIPP)
    } else if !quelle {
        geraete::bus_lautstaerke(
            BUS_NAMES[index],
            bestand.busse_lautstaerke.get(index).copied().flatten(),
            fehlt,
        )
    } else {
        anzeige.clone()
    };
    ui.scope_builder(UiBuilder::new().max_rect(r), |ui| {
        ui.horizontal(|ui| {
            // Mikro für Eingänge, Lautsprecher für die A-Busse: worauf der
            // Zug zeigt, steht damit vor dem Namen.
            let icon = if quelle {
                bausteine::Icon::Mikrofon
            } else {
                bausteine::Icon::Lautsprecher
            };
            bausteine::icon(ui, icon, farbe);
            let vorne = thema::FONT_SIZE_MD + thema::SPACE_1;
            let frei = (r.width() - vorne).max(1.0);
            egui::ComboBox::from_id_salt(("geraet", ziel))
                // Volle Zugbreite abzüglich des Icons; der Name wird nur so
                // weit gekürzt, dass er neben dem Pfeil Platz hat.
                .width(frei)
                .wrap_mode(egui::TextWrapMode::Truncate)
                .selected_text(
                    RichText::new(crate::fmt::kurz(
                        &anzeige,
                        ((frei - thema::SPACE_5) / (thema::FONT_SIZE_SM * 0.6)).max(3.0) as usize,
                    ))
                    .size(thema::FONT_SIZE_SM)
                    .color(farbe),
                )
                .show_ui(ui, |ui| {
                    if ui.selectable_label(aktuell.is_none(), leer).clicked() {
                        a.push(Aktion::Geraet { ziel, name: None });
                    }
                    for e in liste {
                        let gewaehlt = aktuell.as_deref() == Some(e.name.as_str());
                        let text = geraete::eintrag_text(&e.anzeige, e.fehlt);
                        if ui
                            .selectable_label(gewaehlt, text)
                            .on_hover_text(&e.name)
                            .clicked()
                            && !gewaehlt
                        {
                            a.push(Aktion::Geraet {
                                ziel,
                                name: Some(e.name.clone()),
                            });
                        }
                    }
                })
                .response
                .on_hover_text(tipp);
        });
    });
}
