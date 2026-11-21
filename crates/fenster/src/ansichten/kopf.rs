//! Kopfleiste (Marke, Kennzahlen, Meldung, Status, Theme, Dialoge) und
//! Tape-Leiste (Aufnahme, Laufzeit, Pegel, Ziel, Status).

use std::sync::atomic::Ordering::Relaxed;
use std::time::Instant;

use egui::{RichText, Ui};

use crate::bausteine::{self, alpha};
use crate::logik::{self, Kern};
use crate::modell::{Aktion, Dialog, Meldung, Oberflaeche, Tape};
use crate::thema::{self, Farben, Thema};

pub fn kopf(ui: &mut Ui, f: &Farben, o: &Oberflaeche, k: &Kern, laeuft: bool, a: &mut Vec<Aktion>) {
    bausteine::rahmen(f, f.bg_panel)
        .inner_margin(egui::Margin::symmetric(
            thema::SPACE_2 as i8,
            thema::SPACE_1 as i8,
        ))
        .show(ui, |ui| {
            let zeile = egui::Vec2::new(
                ui.available_width(),
                thema::SIZE_CONTROL_MD + thema::SPACE_3,
            );
            let teile = crate::logik::kopf_teile(zeile.x);
            ui.allocate_ui_with_layout(
                zeile,
                egui::Layout::left_to_right(egui::Align::Center),
                |ui| {
                    ui.spacing_mut().item_spacing.x = thema::SPACE_3;
                    ui.label(
                        RichText::new("crosspoint")
                            .font(egui::FontId::new(thema::FONT_SIZE_LG, thema::fett()))
                            .color(f.text_default),
                    );
                    bausteine::zeichen_text(
                        ui,
                        "▪",
                        egui::FontId::monospace(thema::FONT_SIZE_SM),
                        f.accent_default,
                        f.accent_default,
                    );
                    if teile.untertitel {
                        ui.separator();
                        ui.vertical(|ui| {
                            ui.spacing_mut().item_spacing.y = 0.0;
                            ui.label(bausteine::text(f, "8×8 DIGITAL MIXER", true, false));
                            ui.label(bausteine::text(f, "8 x 8 · PIPEWIRE / JACK", false, true));
                        });
                    }
                    if teile.kennzahlen {
                        let sr = f32::from_bits(k.params.sample_rate.load(Relaxed));
                        let block = k.params.block.load(Relaxed);
                        bausteine::chip(ui, f, "SR", &format!("{:.1} kHz", sr / 1000.0));
                        bausteine::chip(ui, f, "BLK", &format!("{block}"));
                        bausteine::chip(
                            ui,
                            f,
                            "RATE",
                            &format!("{:.1} ms", block as f32 / sr.max(1.0) * 1000.0),
                        );
                    }
                    // Die Meldung nimmt nur den Platz, der neben den Knöpfen
                    // rechts frei bleibt, und wird sonst gekürzt.
                    let text = match &o.meldung {
                        Meldung::Hinweis(t) if teile.meldung => {
                            Some(bausteine::text(f, t, false, true))
                        }
                        Meldung::Fehler(t) if teile.meldung => {
                            Some(bausteine::text(f, t, false, false).color(f.status_warn))
                        }
                        _ => None,
                    };
                    if let Some(text) = text {
                        let frei = (ui.available_width() - thema::SPACE_6 * 19.0).max(0.0);
                        ui.allocate_ui(egui::Vec2::new(frei, zeile.y), |ui| {
                            ui.add(egui::Label::new(text).truncate());
                        });
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        for d in [
                            Dialog::Darstellung,
                            Dialog::Midi,
                            Dialog::Vban,
                            Dialog::Presets,
                        ] {
                            let offen = o.dialog == Some(d);
                            let an = offen.then_some(f.accent_default);
                            if bausteine::kopfknopf(ui, f, d.titel(), an).clicked() {
                                a.push(Aktion::Dialog(if offen { None } else { Some(d) }));
                            }
                        }
                        egui::ComboBox::from_id_salt("thema")
                            .selected_text(o.thema.anzeige())
                            .width(thema::SPACE_6 * 2.5)
                            .show_ui(ui, |ui| {
                                for t in Thema::ALLE {
                                    if ui.selectable_label(o.thema == t, t.anzeige()).clicked() {
                                        a.push(Aktion::Thema(t));
                                    }
                                }
                            })
                            .response
                            .on_hover_text("Theme");
                        let (text, farbe) = if laeuft {
                            ("● LAEUFT", f.accent_default)
                        } else {
                            ("● BEREIT", f.status_warn)
                        };
                        bausteine::zeichen_text(
                            ui,
                            text,
                            egui::FontId::monospace(thema::FONT_SIZE_SM),
                            farbe,
                            farbe,
                        );
                    });
                },
            );
        });
}

pub fn tape(
    ui: &mut Ui,
    f: &Farben,
    o: &Oberflaeche,
    k: &Kern,
    jetzt: Instant,
    a: &mut Vec<Aktion>,
) {
    bausteine::rahmen(f, f.bg_canvas)
        .inner_margin(egui::Margin::symmetric(thema::SPACE_2 as i8, 3))
        .show(ui, |ui| {
            let zeile = egui::Vec2::new(ui.available_width(), thema::SIZE_CONTROL_MD);
            ui.allocate_ui_with_layout(
                zeile,
                egui::Layout::left_to_right(egui::Align::Center),
                |ui| {
                    ui.spacing_mut().item_spacing.x = thema::SPACE_2;
                    let nimmt_auf = matches!(o.tape, Tape::Aufnahme { .. });
                    let rec =
                        bausteine::kopfknopf(ui, f, "● REC", nimmt_auf.then_some(f.status_error));
                    if rec.clicked() {
                        a.push(if nimmt_auf {
                            Aktion::TapeStop
                        } else {
                            Aktion::TapeStart
                        });
                    }
                    if bausteine::kopfknopf(ui, f, "■ STOP", None).clicked() {
                        a.push(Aktion::TapeStop);
                    }
                    let zeit = logik::tape_zeit(logik::tape_dauer(o.tape, jetzt));
                    let farbe = if nimmt_auf {
                        f.status_error
                    } else {
                        f.text_muted
                    };
                    ui.label(
                        RichText::new(zeit)
                            .font(egui::FontId::new(thema::FONT_SIZE_LG, thema::fett()))
                            .color(farbe),
                    );
                    bausteine::segmentleiste(ui, f, o.pegel.db[8], 22);
                    let ziel = format!("Ziel: {}", k.tape_ordner.display());
                    let status = k.tape.as_ref().map(|t| t.status()).unwrap_or_default();
                    let status = if status.is_empty() {
                        "idle".to_string()
                    } else {
                        status
                    };
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(
                            bausteine::text(f, &status, false, true)
                                .color(alpha(f.text_muted, 0.9)),
                        );
                        ui.add(egui::Label::new(bausteine::text(f, &ziel, false, true)).truncate())
                            .on_hover_text(&ziel);
                    });
                },
            );
        });
}
