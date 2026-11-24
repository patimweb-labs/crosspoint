//! Makroleiste: Seitenwahl, Felder, Einklappen.

use std::time::Instant;

use egui::{RichText, Ui, Vec2};

use crate::bausteine;
use crate::logik;
use crate::modell::{Aktion, Oberflaeche};
use crate::thema::{self, Farben};

pub fn makros(ui: &mut Ui, f: &Farben, o: &Oberflaeche, jetzt: Instant, a: &mut Vec<Aktion>) {
    bausteine::rahmen(f, f.bg_panel)
        .inner_margin(egui::Margin::same(thema::SPACE_2 as i8))
        .show(ui, |ui| {
            // Bei knapper Höhe zeigt die Leiste nur ihre Kopfzeile, damit dem
            // Pult seine Mindesthöhe bleibt (der gespeicherte Wunsch bleibt
            // unberührt). Die Spalten folgen der Breite der Leiste.
            let raster = logik::makro_raster_im_fenster(
                o.makros_eingeklappt,
                o.makro_anzahl,
                ui.available_width(),
                ui.ctx().content_rect().height(),
            );
            let eingeklappt = raster.is_none();
            let erzwungen = eingeklappt && !o.makros_eingeklappt;
            let seiten = logik::makro_seiten(o.makro_anzahl);
            ui.horizontal(|ui| {
                let titel = ui.label(
                    RichText::new("MAKRO")
                        .font(egui::FontId::new(thema::FONT_SIZE_MD, thema::fett()))
                        .color(f.text_default),
                );
                ui.label(bausteine::text(
                    f,
                    &format!("{} FELDER ·", o.makro_anzahl),
                    false,
                    true,
                ));
                egui::ComboBox::from_id_salt("makroseite")
                    .selected_text(format!("{}", o.makro_seite + 1))
                    .width(thema::SPACE_6)
                    .show_ui(ui, |ui| {
                        for s in 0..seiten {
                            if ui
                                .selectable_label(o.makro_seite == s, format!("{}", s + 1))
                                .clicked()
                            {
                                a.push(Aktion::MakroSeite(s));
                            }
                        }
                    })
                    .response
                    .labelled_by(titel.id);
                ui.label(bausteine::text(
                    f,
                    &format!("SEITE {}/{seiten}", o.makro_seite + 1),
                    false,
                    true,
                ));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let (icon, text) = if eingeklappt {
                        (bausteine::Icon::PfeilRechts, "AUFKLAPPEN")
                    } else {
                        (bausteine::Icon::PfeilRunter, "EINKLAPPEN")
                    };
                    let antwort = bausteine::kopfknopf_icon(ui, f, icon, text, None);
                    if erzwungen {
                        antwort
                            .on_hover_text("Fenster höher ziehen: für die Felder ist kein Platz");
                    } else if antwort.clicked() {
                        a.push(Aktion::MakrosEinklappen(!o.makros_eingeklappt));
                    }
                });
            });
            let Some(raster) = raster else {
                return;
            };
            ui.add_space(thema::SPACE_1);
            egui::Grid::new("makrofelder")
                .spacing(Vec2::splat(thema::SPACE_2))
                .show(ui, |ui| {
                    for i in 0..o.makro_anzahl {
                        let erste = o.makro_seite * o.makro_anzahl;
                        let nr = erste + i;
                        let leuchtet = logik::makro_leuchtet(o, nr, jetzt);
                        if feld(ui, f, nr, raster.kante, leuchtet).clicked() {
                            a.push(Aktion::Makro(nr));
                        }
                        if (i + 1) % raster.spalten == 0 {
                            ui.end_row();
                        }
                    }
                });
        });
}

fn feld(ui: &mut Ui, f: &Farben, nr: usize, kante: f32, an: bool) -> egui::Response {
    let (rect, r) = ui.allocate_exact_size(Vec2::splat(kante), egui::Sense::click());
    let name = format!("MAKRO {}", nr + 1);
    r.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, &name));
    let (flaeche, linie, schrift, unter) = if an {
        (
            f.accent_default,
            f.accent_default,
            f.text_on_accent,
            f.text_on_accent,
        )
    } else if r.hovered() {
        (f.bg_canvas, f.accent_default, f.text_default, f.text_muted)
    } else {
        (f.bg_canvas, f.border_subtle, f.text_default, f.text_muted)
    };
    let p = ui.painter();
    let ecke = egui::CornerRadius::same(thema::RADIUS_MD as u8);
    p.rect_filled(rect, ecke, flaeche);
    p.rect_stroke(
        rect,
        ecke,
        egui::Stroke::new(thema::SIZE_BORDER, linie),
        egui::StrokeKind::Inside,
    );
    p.text(
        rect.center() - Vec2::new(0.0, thema::SPACE_2),
        egui::Align2::CENTER_CENTER,
        name,
        egui::FontId::new(thema::FONT_SIZE_SM, thema::fett()),
        schrift,
    );
    p.text(
        rect.center() + Vec2::new(0.0, thema::SPACE_2),
        egui::Align2::CENTER_CENTER,
        format!("FELD {:02}", nr + 1),
        egui::FontId::monospace(thema::FONT_SIZE_SM),
        unter,
    );
    if r.has_focus() {
        p.rect_stroke(
            rect.expand(1.0),
            ecke,
            egui::Stroke::new(thema::SIZE_FOCUS, f.focus_ring),
            egui::StrokeKind::Outside,
        );
    }
    r
}
