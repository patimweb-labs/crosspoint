//! Die Anwendung: Takt, Pegel und Gerätebestand einsammeln, zeichnen,
//! Aktionen ausführen. Jedes Bild ist `render(modell)` (AGENTS.md §2.1).

use std::sync::mpsc::{Receiver, Sender};
use std::sync::Arc;
use std::time::{Duration, Instant};

use eframe::egui;

use crate::geraete::Bestand;
use crate::konfig;
use crate::logik::{self, Kern};
use crate::modell::{Geraete, Oberflaeche};
use crate::thema::{self, Thema};

/// Bildtakt, solange das Fenster sichtbar ist (Pegel leben).
pub const TAKT: Duration = Duration::from_millis(33);
/// Der Gerätebestand wird so oft neu gelesen (PipeWire-Graph).
pub const GERAETE_TAKT: Duration = Duration::from_secs(5);

/// Was das Fenster außer dem Kern braucht (ADR 0012).
///
/// Alles Übrige – Pegel, Geräte, Lauf – kommt über
/// [`crosspoint_schnittstelle::Schnittstelle`] aus dem Kern.
pub struct Umgebung {
    /// Gerätebestand lesen? (Tests: nein – kein Thread, kein Graph)
    pub geraete_lesen: bool,
    /// Fenstereinstellungen schreiben (`None` in Tests).
    pub konfig_pfad: Option<std::path::PathBuf>,
}

impl Default for Umgebung {
    /// Vorgabe für Prüfstände: kein Thread, kein Graph. Der Betrieb setzt
    /// `geraete_lesen` ausdrücklich auf `true` (siehe `betrieb::Pult::neu`).
    fn default() -> Umgebung {
        Umgebung {
            geraete_lesen: false,
            konfig_pfad: None,
        }
    }
}

pub struct Fenster {
    pub kern: Kern,
    pub o: Oberflaeche,
    umgebung: Umgebung,
    angewandt: Option<Thema>,
    geraete_rx: Option<Receiver<Bestand>>,
    geraete_tx: Sender<Bestand>,
    geraete_zuletzt: Option<Instant>,
}

impl Fenster {
    pub fn neu(kern: Kern, o: Oberflaeche, umgebung: Umgebung) -> Fenster {
        let (tx, rx) = std::sync::mpsc::channel();
        Fenster {
            kern,
            o,
            umgebung,
            angewandt: None,
            geraete_rx: Some(rx),
            geraete_tx: tx,
            geraete_zuletzt: None,
        }
    }

    /// Gerätebestand im Hintergrund nachlesen (alle [`GERAETE_TAKT`] oder
    /// nach einer Zuordnung).
    fn geraete(&mut self, ctx: &egui::Context, jetzt: Instant) {
        if let Some(rx) = &self.geraete_rx {
            while let Ok(b) = rx.try_recv() {
                // Die Echo-Auswahl gehoert zum Kern (`echo.json`); der Bestand
                // bringt sie mit, damit eine Änderung von außen (API) im Menü
                // ankommt.
                self.o.echo_geraete = b.echo.clone();
                self.o.geraete = Geraete::Fertig(Box::new(b));
            }
        }
        if !self.umgebung.geraete_lesen {
            return;
        }
        let faellig = matches!(self.o.geraete, Geraete::Laedt)
            || self
                .geraete_zuletzt
                .is_none_or(|t| jetzt - t >= GERAETE_TAKT);
        let laeuft_schon = self
            .geraete_zuletzt
            .is_some_and(|t| jetzt - t < Duration::from_millis(500));
        if faellig && !laeuft_schon {
            self.geraete_zuletzt = Some(jetzt);
            let tx = self.geraete_tx.clone();
            let ctx = ctx.clone();
            let kern = Arc::clone(&self.kern.schnittstelle);
            std::thread::spawn(move || {
                let _ = tx.send(kern.geraete().bestand());
                ctx.request_repaint();
            });
        }
    }

    /// Eine Aktion von außen ausführen (Start mit `--dialog`, Tests).
    pub fn ausfuehren(&mut self, a: crate::modell::Aktion) {
        logik::ausfuehren(a, &mut self.o, &self.kern);
    }

    /// Ein Bild: einsammeln, zeichnen, ausführen.
    pub fn bild(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        if self.angewandt != Some(self.o.thema) {
            thema::anwenden(&ctx, self.o.thema);
            self.angewandt = Some(self.o.thema);
        }
        let jetzt = Instant::now();
        {
            let bank = self.kern.schnittstelle.pegel();
            let roh = logik::pegel_lesen(bank, self.o.master_bus);
            logik::pegel_ticken(&mut self.o.pegel, &roh);
        }
        logik::takt(&mut self.o, &self.kern, jetzt);
        self.geraete(&ctx, jetzt);

        let laeuft = self.kern.betrieb().laeuft();
        let aktionen = crate::ansichten::pult(ui, &mut self.o, &self.kern, laeuft, jetzt);
        handzeiger(&ctx);
        let mut speichern = false;
        for a in aktionen {
            speichern |= logik::ausfuehren(a, &mut self.o, &self.kern);
        }
        if speichern {
            if let Some(p) = &self.umgebung.konfig_pfad {
                if let Err(e) = konfig::Datei::aus(&self.o).speichern(p) {
                    eprintln!("crosspoint: {} nicht geschrieben: {e}", p.display());
                }
            }
        }
        ctx.request_repaint_after(TAKT);
    }
}

/// Handzeiger über allem, was man anklicken kann.
///
/// egui setzt `Visuals::interact_cursor` nur bei `Button`; ComboBox,
/// Checkbox, Auswahlfelder, das Schließen-X der Dialoge und die gezeichneten
/// Schalter blieben beim Pfeil. Hier einmal je Bild für alle: liegt der Zeiger
/// über einem klickbaren Element und hat noch niemand einen eigenen Zeiger
/// gewählt (Textfeld, Fader), wird es die Hand.
pub fn handzeiger(ctx: &egui::Context) {
    let unter_zeiger: Vec<egui::Id> =
        ctx.viewport(|vp| vp.interact_widgets.hovered.iter().copied().collect());
    let klickbar = unter_zeiger.into_iter().any(|id| {
        ctx.read_response(id)
            .is_some_and(|r| r.enabled() && r.sense.senses_click())
    });
    if klickbar {
        ctx.output_mut(|o| {
            if o.cursor_icon == egui::CursorIcon::Default {
                o.cursor_icon = egui::CursorIcon::PointingHand;
            }
        });
    }
}

impl eframe::App for Fenster {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.bild(ui);
    }
}
