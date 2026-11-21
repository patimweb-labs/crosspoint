//! Die Ansichten: `render(modell) → Aktionen`.
//!
//! Gelesen wird aus [`Oberflaeche`] und dem Kern ([`Kern`]), geschrieben
//! nichts – jede Bedienung wird zur [`Aktion`], die `logik::ausfuehren`
//! ausführt. Einzige Ausnahme sind Formularfelder der Dialoge (VBAN, MIDI),
//! die direkt im Modell bearbeitet und per Aktion übernommen werden.

pub mod dialoge;
pub mod filter;
pub mod kopf;
pub mod makros;
pub mod zug;

use std::time::Instant;

use egui::{Pos2, Rect, Vec2};

use crate::logik::{self, Kern};
use crate::modell::{Aktion, Oberflaeche, Ziel};
use crate::thema;

/// Abstand zwischen den Gruppen (Kanäle | Busse | Master).
const GRUPPE: f32 = thema::SPACE_3;

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
    egui::Panel::bottom("makros")
        .frame(egui::Frame::new().fill(f.bg_canvas).inner_margin(rand))
        .show(ui, |ui| makros::makros(ui, f, o, jetzt, &mut a));
    egui::CentralPanel::default()
        .frame(
            egui::Frame::new()
                .fill(f.bg_canvas)
                .inner_margin(egui::Margin::symmetric(thema::SPACE_2 as i8, 0)),
        )
        .show(ui, |ui| {
            let hoehe = ui
                .available_height()
                // Untergrenze: ein Fader, an dem alle Skalenzahlen lesbar
                // stehen, plus alle Knöpfe und die Lämpchenreihe der
                // Filterbusse. Darüber wächst der Fader mit dem Fenster,
                // darunter scrollt das Pult (in beide Richtungen).
                .max(thema::SIZE_FADER_HOEHE + thema::SPACE_6 * 8.0 + zug::LAEMPCHEN);
            let luecken = 2.0 * GRUPPE + 16.0 * thema::SPACE_1;
            let (kanal, bus) = logik::zugbreiten(o.dichte, ui.available_width(), luecken);
            let gesamt = 8.0 * kanal + 9.0 * bus + luecken;
            // Beide Richtungen: bei kleinem Fenster wird gescrollt, nie
            // abgeschnitten.
            egui::ScrollArea::both()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    let (rect, _) =
                        ui.allocate_exact_size(Vec2::new(gesamt, hoehe), egui::Sense::hover());
                    let mut x = rect.left();
                    let mut setze = |ui: &mut egui::Ui, ziel: Ziel, x: &mut f32| {
                        let breite = if matches!(ziel, Ziel::Kanal(_)) {
                            kanal
                        } else {
                            bus
                        };
                        let r = Rect::from_min_size(
                            Pos2::new(*x, rect.top()),
                            Vec2::new(breite, hoehe),
                        );
                        zug::zug(ui, f, r, ziel, o, k, &mut a);
                        *x += breite + thema::SPACE_1;
                    };
                    for i in 0..crosspoint::STRIPS {
                        setze(ui, Ziel::Kanal(i), &mut x);
                    }
                    x += GRUPPE - thema::SPACE_1;
                    for i in 0..crosspoint::BUSES {
                        setze(ui, Ziel::Bus(i), &mut x);
                    }
                    x += GRUPPE - thema::SPACE_1;
                    setze(ui, Ziel::Master, &mut x);
                });
        });
    dialoge::dialog(&ctx, f, o, k, &mut a);
    filter::modal(&ctx, f, o, k, &mut a);
    a
}
