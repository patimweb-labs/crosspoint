//! Dialoge über dem Pult: Presets, VBAN, MIDI, Darstellung.

use egui::{RichText, Ui};

use crate::bausteine;
use crate::logik::{self, Kern};
use crate::modell::{Aktion, Dialog, Oberflaeche};
use crate::thema::{self, Farben, Thema};

pub fn dialog(ctx: &egui::Context, f: &Farben, o: &mut Oberflaeche, k: &Kern, a: &mut Vec<Aktion>) {
    let Some(d) = o.dialog else { return };
    let mut offen = true;
    let (breite, unter) = match d {
        Dialog::Presets => (thema::SPACE_6 * 14.0, "Parameterbild laden und sichern"),
        Dialog::Vban => (
            thema::SPACE_6 * 22.0,
            "Audio übers Netz senden und empfangen",
        ),
        Dialog::Midi => (thema::SPACE_6 * 26.0, "MIDIMonster übersetzt MIDI nach OSC"),
        Dialog::Darstellung => (thema::SPACE_6 * 12.0, "Theme, Makros, Autostart"),
    };
    egui::Window::new(
        RichText::new(d.titel()).font(egui::FontId::new(thema::FONT_SIZE_MD, thema::fett())),
    )
    .id(egui::Id::unique(("dialog", d.titel())))
    .open(&mut offen)
    .collapsible(false)
    .resizable(true)
    .default_width(breite)
    .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
    .show(ctx, |ui| {
        ui.label(bausteine::text(f, unter, false, true));
        ui.separator();
        match d {
            Dialog::Presets => presets(ui, f, o, k, a),
            Dialog::Vban => vban(ui, f, o, k, a),
            Dialog::Midi => midi(ui, f, o, a),
            Dialog::Darstellung => darstellung(ui, f, o, a),
        }
        ui.separator();
        if bausteine::kopfknopf_icon(ui, f, bausteine::Icon::Schliessen, "SCHLIESSEN", None)
            .clicked()
        {
            a.push(Aktion::Dialog(None));
        }
    });
    if !offen {
        a.push(Aktion::Dialog(None));
    }
}

fn presets(ui: &mut Ui, f: &Farben, o: &mut Oberflaeche, k: &Kern, a: &mut Vec<Aktion>) {
    let liste = logik::presets(&k.preset_ordner);
    let seiten = logik::preset_seiten(liste.len());
    let seite = o.preset_seite.min(seiten - 1);
    // Feste Zahl von Zeilen je Seite statt Scrollen: das Fenster bleibt so
    // hoch wie der Dialog (ADR 0007).
    for (name, pfad) in liste
        .iter()
        .skip(seite * logik::PRESET_ZEILEN)
        .take(logik::PRESET_ZEILEN)
    {
        ui.horizontal(|ui| {
            ui.label(bausteine::text(f, name, false, false));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if bausteine::kopfknopf(ui, f, "LADEN", None).clicked() {
                    a.push(Aktion::PresetLaden(pfad.clone()));
                }
            });
        });
    }
    if liste.is_empty() {
        ui.label(bausteine::text(
            f,
            "noch kein Preset gespeichert",
            false,
            true,
        ));
    }
    if seiten > 1 {
        ui.horizontal(|ui| {
            if bausteine::kopfknopf(ui, f, "<< ZURÜCK", None).clicked() {
                a.push(Aktion::PresetSeite(logik::blaettern(seite, seiten, false)));
            }
            ui.label(bausteine::text(
                f,
                &format!("SEITE {}/{seiten}", seite + 1),
                false,
                true,
            ));
            if bausteine::kopfknopf(ui, f, "WEITER >>", None).clicked() {
                a.push(Aktion::PresetSeite(logik::blaettern(seite, seiten, true)));
            }
        });
    }
    ui.label(bausteine::text(
        f,
        &format!("Ordner: {}", k.preset_ordner.display()),
        false,
        true,
    ));
    ui.horizontal(|ui| {
        let mut name = o.preset_name.clone();
        let feld =
            ui.add(egui::TextEdit::singleline(&mut name).hint_text("Name für Speichern unter"));
        if feld.changed() {
            a.push(Aktion::PresetName(name));
        }
        let enter = feld.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
        if bausteine::kopfknopf(ui, f, "SPEICHERN", None).clicked() || enter {
            a.push(Aktion::PresetSpeichern);
        }
    });
}

fn vban(ui: &mut Ui, f: &Farben, o: &mut Oberflaeche, k: &Kern, a: &mut Vec<Aktion>) {
    let Some((send, recv)) = &k.vban else {
        ui.label(bausteine::text(
            f,
            "VBAN ist in diesem Lauf nicht verfügbar",
            false,
            true,
        ));
        return;
    };
    let stand = crate::vban::stand(send, recv, k.sample_rate);
    ui.label(bausteine::text(
        f,
        &crate::vban::unterzeile(&stand),
        false,
        true,
    ));
    let Some(form) = o.vban.as_mut() else { return };
    egui::Grid::new("vban_formular")
        .num_columns(2)
        .spacing(egui::Vec2::splat(thema::SPACE_2))
        .show(ui, |ui| {
            ui.label(bausteine::text(f, "SENDEN", true, false));
            ui.checkbox(&mut form.senden_an, "an");
            ui.end_row();
            let l = ui.label(bausteine::text(f, "Ziel-IP", false, true));
            ui.text_edit_singleline(&mut form.host).labelled_by(l.id);
            ui.end_row();
            let l = ui.label(bausteine::text(f, "Sendeport", false, true));
            ui.add(egui::DragValue::new(&mut form.senden_port).range(1.0..=65535.0))
                .labelled_by(l.id);
            ui.end_row();
            let l = ui.label(bausteine::text(f, "Sendestream", false, true));
            ui.text_edit_singleline(&mut form.senden_stream)
                .labelled_by(l.id);
            ui.end_row();
            ui.label(bausteine::text(f, "EMPFANGEN", true, false));
            ui.checkbox(&mut form.empfangen_an, "an");
            ui.end_row();
            let l = ui.label(bausteine::text(f, "Empfangsport", false, true));
            ui.add(egui::DragValue::new(&mut form.empfangen_port).range(1.0..=65535.0))
                .labelled_by(l.id);
            ui.end_row();
            let l = ui.label(bausteine::text(f, "Empfangsstream", false, true));
            ui.text_edit_singleline(&mut form.empfangen_stream)
                .labelled_by(l.id);
            ui.end_row();
        });
    if bausteine::kopfknopf(ui, f, "ÜBERNEHMEN", None).clicked() {
        a.push(Aktion::VbanAnwenden);
    }
    ui.separator();
    for (titel, zeilen) in [("INCOMING", &stand.incoming), ("OUTGOING", &stand.outgoing)] {
        ui.label(bausteine::text(f, titel, true, false));
        if zeilen.is_empty() {
            ui.label(bausteine::text(f, "—", false, true));
            continue;
        }
        egui::Grid::new(("vban", titel))
            .striped(true)
            .show(ui, |ui| {
                for kopf in ["STREAM", "IP", "SR", "CH", "FORMAT", "WEG", "FEHLER"] {
                    ui.label(bausteine::text(f, kopf, false, true));
                }
                ui.end_row();
                for z in zeilen {
                    for t in [
                        z.stream.clone(),
                        z.ip.clone(),
                        z.sr.to_string(),
                        z.kanaele.to_string(),
                        z.format.clone(),
                        z.weg.clone(),
                        z.fehler.to_string(),
                    ] {
                        ui.label(bausteine::text(f, &t, false, false));
                    }
                    ui.end_row();
                }
            });
    }
}

fn midi(ui: &mut Ui, f: &Farben, o: &mut Oberflaeche, a: &mut Vec<Aktion>) {
    let Some(m) = o.midi.as_mut() else { return };
    ui.horizontal(|ui| {
        ui.vertical(|ui| {
            let l = ui.label(bausteine::text(f, "MIDI INPUT DEVICE", false, true));
            egui::ComboBox::from_id_salt("midi_ein")
                .selected_text(m.karte.eingang.clone())
                .show_ui(ui, |ui| {
                    for g in &m.geraete {
                        ui.selectable_value(&mut m.karte.eingang, g.clone(), g);
                    }
                })
                .response
                .labelled_by(l.id);
        });
        ui.vertical(|ui| {
            let l = ui.label(bausteine::text(f, "MIDI MAP NAME", false, true));
            ui.text_edit_singleline(&mut m.karte.name).labelled_by(l.id);
        });
        ui.vertical(|ui| {
            let l = ui.label(bausteine::text(f, "MIDI OUTPUT DEVICE", false, true));
            egui::ComboBox::from_id_salt("midi_aus")
                .selected_text(m.karte.ausgang.clone())
                .show_ui(ui, |ui| {
                    for g in &m.geraete {
                        ui.selectable_value(&mut m.karte.ausgang, g.clone(), g);
                    }
                })
                .response
                .labelled_by(l.id);
        });
    });
    ui.horizontal(|ui| {
        if bausteine::kopfknopf(ui, f, "RESET MAP", None).clicked() {
            a.push(Aktion::MidiZuruecksetzen);
        }
        if bausteine::kopfknopf(ui, f, "LOAD MIDI MAP", None).clicked() {
            a.push(Aktion::MidiLaden);
        }
        if bausteine::kopfknopf(ui, f, "SAVE MIDI MAP", None).clicked() {
            a.push(Aktion::MidiSpeichern);
        }
        ui.label(bausteine::text(
            f,
            &format!("{} gebunden", m.karte.gebunden()),
            false,
            true,
        ));
    });
    ui.separator();
    // Ein Blatt je Seite: so viele Zeilen, wie im kleinsten Fenster Platz
    // haben – kein Scrollen (ADR 0007).
    egui::Grid::new("midi_zeilen").striped(true).show(ui, |ui| {
        for z in m.karte.blatt(m.seite, m.blatt) {
            ui.label(bausteine::text(f, &z.titel, false, false));
            ui.label(bausteine::text(f, &z.ziel, false, true));
            ui.label(bausteine::text(
                f,
                &z.cc
                    .map(|c| format!("CC {c}"))
                    .unwrap_or_else(|| "—".into()),
                false,
                true,
            ));
            let lernt = m.lernt.as_deref() == Some(z.ziel.as_str());
            let (text, an) = if lernt {
                ("WARTET …", Some(f.status_warn))
            } else if z.cc.is_some() {
                ("LÖSEN", None)
            } else {
                ("LEARN", None)
            };
            if bausteine::kopfknopf(ui, f, text, an).clicked() {
                a.push(Aktion::MidiLernen(z.ziel.clone()));
            }
            ui.end_row();
        }
    });
    ui.horizontal(|ui| {
        let n = crate::midimonster::seiten_blaetter(m.seite);
        if bausteine::kopfknopf(ui, f, "<< ZURÜCK", None).clicked() {
            let (seite, blatt) = logik::midi_blaettern(m.seite, m.blatt, false);
            a.push(Aktion::MidiSeite(seite));
            a.push(Aktion::MidiBlatt(blatt));
        }
        ui.label(bausteine::text(
            f,
            &format!(
                "SEITE {}/{} · BLATT {}/{n}",
                m.seite + 1,
                crate::midimonster::SEITEN,
                m.blatt + 1
            ),
            false,
            true,
        ));
        if bausteine::kopfknopf(ui, f, "WEITER >>", None).clicked() {
            let (seite, blatt) = logik::midi_blaettern(m.seite, m.blatt, true);
            a.push(Aktion::MidiSeite(seite));
            a.push(Aktion::MidiBlatt(blatt));
        }
    });
    if !m.status.is_empty() {
        ui.label(bausteine::text(f, &m.status, false, true));
    }
}

fn darstellung(ui: &mut Ui, f: &Farben, o: &Oberflaeche, a: &mut Vec<Aktion>) {
    ui.label(bausteine::text(f, "THEME", true, false));
    ui.horizontal(|ui| {
        for t in Thema::ALLE {
            let gewaehlt = o.thema == t;
            let knopf = egui::Button::image_and_text(
                bausteine::thema_icon(t).bild(thema::FONT_SIZE_MD, f.text_muted),
                t.anzeige(),
            )
            .selected(gewaehlt)
            .frame_when_inactive(gewaehlt);
            if ui.add(knopf).clicked() {
                a.push(Aktion::Thema(t));
            }
        }
    });
    ui.label(bausteine::text(f, "MAKROFELDER", true, false));
    ui.horizontal(|ui| {
        for n in [5, 8, 10, 16, 40, 80] {
            if ui
                .selectable_label(o.makro_anzahl == n, n.to_string())
                .clicked()
            {
                a.push(Aktion::MakroAnzahl(n));
            }
        }
    });
    ui.separator();
    let mut autostart = o.autostart;
    if ui
        .checkbox(&mut autostart, "Beim Anmelden starten (nur Tray-Icon)")
        .changed()
    {
        a.push(Aktion::Autostart(autostart));
    }
}
