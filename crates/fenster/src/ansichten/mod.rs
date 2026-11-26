//! Die Ansichten: `render(modell) → Aktionen`.
//!
//! Gelesen wird aus [`Oberflaeche`] und dem Kern ([`Kern`]), geschrieben
//! nichts – jede Bedienung wird zur [`Aktion`], die `logik::ausfuehren`
//! ausführt. Einzige Ausnahme sind Formularfelder der Dialoge (VBAN, MIDI),
//! die direkt im Modell bearbeitet und per Aktion übernommen werden.
//!
//! Kein Bereich scrollt (ADR 0007): Die 17 Züge füllen die Fläche zwischen
//! Kopf- und Makroleiste genau, und die Mindestgröße des Fensters
//! (`logik::mindestfenster`) hält sie so groß, dass Skala, Knöpfe und
//! Lämpchen Platz haben.

pub mod dialoge;
pub mod filter;
pub mod kopf;
pub mod makros;
pub mod zug;

use std::time::Instant;

use egui::{Pos2, Rect, Sense, Vec2};

use crate::logik::{self, Kern};
use crate::modell::{Aktion, Oberflaeche, Ziel};
use crate::thema;

/// Das ganze Pult in einem Bild.
pub fn pult(
    ui: &mut egui::Ui,
    o: &mut Oberflaeche,
    k: &Kern,
    laeuft: bool,
    jetzt: Instant,
) -> Vec<Aktion> {
    let ctx = ui.ctx().clone();
    let f = o.thema.farben();
    let mut a = Vec::new();
    let rand = egui::Margin::same(thema::SPACE_2 as i8);

    egui::Panel::top("kopf")
        .frame(egui::Frame::new().fill(f.bg_canvas).inner_margin(rand))
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = thema::SPACE_2;
            kopf::kopf(ui, f, o, k, laeuft, &mut a);
            kopf::tape(ui, f, o, k, jetzt, &mut a);
        });
    // Ohne Makrofelder (DARSTELLUNG › AUS) gibt es die Leiste gar nicht.
    if o.makro_anzahl > 0 {
        egui::Panel::bottom("makros")
            .frame(egui::Frame::new().fill(f.bg_canvas).inner_margin(rand))
            .show(ui, |ui| makros::makros(ui, f, o, jetzt, &mut a));
    }
    egui::CentralPanel::default()
        .frame(
            egui::Frame::new()
                .fill(f.bg_canvas)
                .inner_margin(egui::Margin::symmetric(thema::SPACE_2 as i8, 0)),
        )
        .show(ui, |ui| {
            let flaeche = ui.available_rect_before_wrap();
            let luecken = logik::zugluecken();
            let (kanal, bus) = logik::zugbreiten(flaeche.width(), luecken);
            let gesamt = crosspoint_modell::STRIPS as f32 * kanal
                + (crosspoint_modell::BUSES + 1) as f32 * bus
                + luecken;
            let (rect, _) = ui.allocate_exact_size(
                Vec2::new(gesamt.max(flaeche.width()), flaeche.height()),
                Sense::hover(),
            );
            let mut x = rect.left();
            let mut setze = |ui: &mut egui::Ui, ziel: Ziel, x: &mut f32| {
                let breite = if matches!(ziel, Ziel::Kanal(_)) {
                    kanal
                } else {
                    bus
                };
                let r = Rect::from_min_size(
                    Pos2::new(*x, rect.top()),
                    Vec2::new(breite, rect.height()),
                );
                zug::zug(ui, f, r, ziel, o, k, &mut a);
                *x += breite + thema::SPACE_1;
            };
            for i in 0..crosspoint_modell::STRIPS {
                setze(ui, Ziel::Kanal(i), &mut x);
            }
            x += logik::GRUPPE - thema::SPACE_1;
            for i in 0..crosspoint_modell::BUSES {
                setze(ui, Ziel::Bus(i), &mut x);
            }
            x += logik::GRUPPE - thema::SPACE_1;
            setze(ui, Ziel::Master, &mut x);
        });
    dialoge::dialog(&ctx, f, o, k, &mut a);
    filter::modal(&ctx, f, o, k, &mut a);
    a
}
