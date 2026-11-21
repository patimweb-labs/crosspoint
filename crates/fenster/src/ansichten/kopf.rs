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
                    marke(ui, f);
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
                            if bausteine::kopfknopf_icon(ui, f, dialog_icon(d), d.titel(), an)
                                .clicked()
                            {
                                a.push(Aktion::Dialog(if offen { None } else { Some(d) }));
                            }
                        }
                        // Im RTL-Lauf landet das zuerst gesetzte Element rechts.
                        egui::ComboBox::from_id_salt("thema")
                            .selected_text(o.thema.anzeige())
                            .width(thema::SPACE_6 * 2.5)
                            .show_ui(ui, |ui| {
                                for t in Thema::ALLE {
                                    let gewaehlt = o.thema == t;
                                    let knopf = egui::Button::image_and_text(
                                        bausteine::thema_icon(t)
                                            .bild(thema::FONT_SIZE_MD, f.text_muted),
                                        t.anzeige(),
                                    )
                                    .selected(gewaehlt)
                                    .frame_when_inactive(gewaehlt);
                                    if ui.add(knopf).clicked() {
                                        a.push(Aktion::Thema(t));
                                    }
                                }
                            })
                            .response
                            .on_hover_text("Theme");
                        // … deshalb erst jetzt, damit Sonne/Mond/Kontrast links
                        // vom Feld stehen und das gewählte Theme zeigen.
                        bausteine::icon(ui, bausteine::thema_icon(o.thema), f.text_muted);
                        let (icon, text, farbe) = if laeuft {
                            (bausteine::Icon::Aktivitaet, "LAEUFT", f.accent_default)
                        } else {
                            (bausteine::Icon::Kreis, "BEREIT", f.status_warn)
                        };
                        bausteine::icon_text(
                            ui,
                            icon,
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

/// Die Marke im Kopf: das Zeichen auf der Kachel und die Wortmarke. Beides
/// sind SVGs aus `assets/` (ADR 0008), keine Schrift und kein Text: die
/// Wortmarke wird mit der Textfarbe eingefärbt, das Zeichen bringt seine
/// Farben selbst mit.
pub fn marke(ui: &mut Ui, f: &Farben) {
    // In der Vorlage ist das Zeichen zweimal so hoch wie die Großbuchstaben.
    let hoehe = thema::FONT_SIZE_LG * 0.75;
    ui.add(
        egui::Image::new(egui::include_image!(
            "../../../../assets/crosspoint-logo.svg"
        ))
        .fit_to_exact_size(egui::Vec2::splat(hoehe * 2.0))
        .alt_text("Crosspoint-Zeichen"),
    );
    ui.add(
        egui::Image::new(egui::include_image!(
            "../../../../assets/crosspoint-schriftzug.svg"
        ))
        .tint(f.text_default)
        .fit_to_exact_size(egui::Vec2::new(hoehe * 8.6625, hoehe))
        .alt_text("Crosspoint"),
    );
}

/// Das Icon eines Dialogs für den Kopfknopf: Ordner (Presets), Netzwerk
/// (VBAN), Regler (MIDI), Palette (Darstellung).
fn dialog_icon(d: Dialog) -> bausteine::Icon {
    match d {
        Dialog::Presets => bausteine::Icon::Ordner,
        Dialog::Vban => bausteine::Icon::Netzwerk,
        Dialog::Midi => bausteine::Icon::Regler,
        Dialog::Darstellung => bausteine::Icon::Palette,
    }
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
                    let rec = bausteine::kopfknopf_icon(
                        ui,
                        f,
                        bausteine::Icon::Kreis,
                        "REC",
                        nimmt_auf.then_some(f.status_error),
                    );
                    if rec.clicked() {
                        a.push(if nimmt_auf {
                            Aktion::TapeStop
                        } else {
                            Aktion::TapeStart
                        });
                    }
                    if bausteine::kopfknopf_icon(ui, f, bausteine::Icon::Quadrat, "STOP", None)
                        .clicked()
                    {
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
