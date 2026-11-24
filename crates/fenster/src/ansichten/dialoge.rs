//! Dialoge über dem Pult: Presets, VBAN, MIDI, Darstellung.
//!
//! Alle vier stehen im selben Rahmen wie das FILTER-Modal
//! ([`bausteine::dialog_rahmen`]): Titelleiste mit dem Titel links und dem
//! Schließen-X rechts, gleicher Rand, gleicher Innenabstand – und geschlossen
//! wird überall gleich (X, Escape, Klick daneben). Innen stehen die Felder in
//! Rastern fester Breite: Beschriftung und Bedienung stehen untereinander auf
//! einer Kante, und nichts scrollt (ADR 0007).

use egui::{Align, Label, Layout, Ui, Vec2};

use crate::bausteine;
use crate::logik::{self, Kern};
use crate::modell::{Aktion, Dialog, Oberflaeche};
use crate::thema::{self, Farben, Thema};

/// Breite einer Beschriftungsspalte in den Dialogen.
const LABEL_BREITE: f32 = thema::SPACE_6 * 4.0;

/// Was im Autostart-Kästchen steht – der längste Text des Dialogs und damit
/// das Maß, aus dem er seine Breite nimmt.
const AUTOSTART: &str = "Beim Anmelden starten (nur Tray-Icon)";

/// Die Makrofeld-Zahlen, die zur Wahl stehen.
const MAKROFELDER: [usize; 6] = [5, 8, 10, 16, 40, 80];

/// Breite eines Textes in der Schrift der Beschriftungen – gemessen, nicht
/// geschätzt: die Dialoge nehmen ihre Breite aus ihrem Inhalt, damit kein Text
/// am Rand klebt.
fn text_breite(ctx: &egui::Context, text: &str, font: egui::FontId) -> f32 {
    ctx.fonts_mut(|f| {
        f.layout(text.to_owned(), font, egui::Color32::WHITE, f32::INFINITY)
            .size()
            .x
    })
}

/// Inhaltbreite des DARSTELLUNG-Dialogs: so breit, wie die breiteste Zeile
/// (das Autostart-Kästchen samt Häkchen) ist, plus die Beschriftungsspalte und
/// Luft rechts – kein Text stößt an den Rand.
pub fn darstellung_breite(ctx: &egui::Context) -> f32 {
    // Gemessen wird in den Schriften, in denen die Bausteine wirklich
    // schreiben: das Häkchen seinen Text in der Textschrift, die Knöpfe in der
    // Knopfschrift.
    let stil = ctx.global_style();
    let knopfschrift = stil.text_styles[&egui::TextStyle::Button].clone();
    let textschrift = stil.text_styles[&egui::TextStyle::Body].clone();
    let kaestchen = stil.spacing.icon_width
        + stil.spacing.item_spacing.x
        + text_breite(ctx, AUTOSTART, textschrift)
        // Das Kästchen bringt seine eigene Polsterung mit (Widget-Stil von
        // egui): hier steht die Luft, die den Text vom Rand fernhält.
        + thema::SPACE_4;
    // Was die beiden Knopfreihen mindestens brauchen, wenn jeder Knopf nur so
    // breit wäre wie sein Wort und seine Polsterung.
    let themen = Thema::ALLE
        .iter()
        .map(|t| {
            text_breite(ctx, t.anzeige(), knopfschrift.clone())
                + thema::FONT_SIZE_MD
                + thema::SPACE_1
                + 2.0 * thema::SPACE_2
        })
        .sum::<f32>()
        + 2.0 * thema::SPACE_1;
    let makros = MAKROFELDER
        .iter()
        .map(|n| text_breite(ctx, &n.to_string(), knopfschrift.clone()) + 2.0 * thema::SPACE_2)
        .sum::<f32>()
        + 5.0 * thema::SPACE_1;
    LABEL_BREITE + thema::SPACE_2 + kaestchen.max(themen).max(makros)
}

pub fn dialog(ctx: &egui::Context, f: &Farben, o: &mut Oberflaeche, k: &Kern, a: &mut Vec<Aktion>) {
    let Some(d) = o.dialog else { return };
    let (breite, unter) = match d {
        Dialog::Presets => (thema::SPACE_6 * 14.0, "Parameterbild laden und sichern"),
        Dialog::Vban => (
            thema::SPACE_6 * 22.0,
            "Audio übers Netz senden und empfangen",
        ),
        Dialog::Midi => (thema::SPACE_6 * 26.0, "MIDIMonster übersetzt MIDI nach OSC"),
        Dialog::Darstellung => (darstellung_breite(ctx), "Theme, Makros, Autostart"),
    };
    let breite = breite.min(ctx.content_rect().width() - 2.0 * thema::SPACE_4);
    let antwort = bausteine::dialog_rahmen(
        ctx,
        f,
        egui::Id::unique(("dialog", d.titel())),
        d.titel(),
        breite,
        |ui| {
            subtitel(ui, f, unter);
            match d {
                Dialog::Presets => presets(ui, f, o, k, a, breite),
                Dialog::Vban => vban(ui, f, o, k, a),
                Dialog::Midi => midi(ui, f, o, a, breite),
                Dialog::Darstellung => darstellung(ui, f, o, a, breite),
            }
        },
    );
    if antwort.zu {
        a.push(Aktion::Dialog(None));
    }
}

/// Unter dem Titel: was der Dialog tut – linksbündig in einer Zeile.
fn subtitel(ui: &mut Ui, f: &Farben, text: &str) {
    ui.add(Label::new(bausteine::text(f, text, false, true)).truncate());
    ui.separator();
}

/// Ein Feld in fester Breite, linksbündig.
fn feld<R>(
    ui: &mut Ui,
    breite: f32,
    hoehe: f32,
    inhalt: impl FnOnce(&mut Ui) -> R,
) -> egui::InnerResponse<R> {
    ui.allocate_ui_with_layout(
        Vec2::new(breite, hoehe),
        Layout::left_to_right(Align::Center),
        inhalt,
    )
}

fn presets(
    ui: &mut Ui,
    f: &Farben,
    o: &mut Oberflaeche,
    k: &Kern,
    a: &mut Vec<Aktion>,
    breite: f32,
) {
    let liste = logik::presets(&k.preset_ordner);
    let seiten = logik::preset_seiten(liste.len());
    let seite = o.preset_seite.min(seiten - 1);
    if liste.is_empty() {
        ui.add(Label::new(bausteine::text(
            f,
            "noch kein Preset gespeichert",
            false,
            true,
        )));
    } else {
        // Feste Zahl von Zeilen je Seite statt Scrollen: das Fenster bleibt so
        // hoch wie der Dialog (ADR 0007). Der Name steht in einer Spalte, der
        // Knopf in der nächsten – alle Knöpfe untereinander auf einer Kante.
        let name_breite = (breite - thema::SPACE_6 * 3.0).max(thema::SPACE_6 * 3.0);
        ui.spacing_mut().item_spacing.y = 0.0;
        egui::Grid::new("presets_liste")
            .num_columns(2)
            .spacing(Vec2::new(thema::SPACE_2, 0.0))
            .show(ui, |ui| {
                for (name, pfad) in liste
                    .iter()
                    .skip(seite * logik::PRESET_ZEILEN)
                    .take(logik::PRESET_ZEILEN)
                {
                    feld(ui, name_breite, thema::SIZE_CONTROL_MD, |ui| {
                        ui.add(Label::new(bausteine::text(f, name, false, false)).truncate())
                            .on_hover_text(pfad.display().to_string());
                    });
                    if bausteine::kopfknopf(ui, f, "LADEN", None).clicked() {
                        a.push(Aktion::PresetLaden(pfad.clone()));
                    }
                    ui.end_row();
                }
            });
        ui.spacing_mut().item_spacing.y = thema::SPACE_1;
    }
    if seiten > 1 {
        egui::Grid::new("presets_seite")
            .num_columns(3)
            .spacing(Vec2::new(thema::SPACE_2, thema::SPACE_1))
            .show(ui, |ui| {
                if bausteine::kopfknopf(ui, f, "<< ZURÜCK", None).clicked() {
                    a.push(Aktion::PresetSeite(logik::blaettern(seite, seiten, false)));
                }
                feld(ui, thema::SPACE_6 * 3.0, thema::SIZE_CONTROL_MD, |ui| {
                    ui.add(Label::new(bausteine::text(
                        f,
                        &format!("SEITE {}/{seiten}", seite + 1),
                        false,
                        true,
                    )));
                });
                if bausteine::kopfknopf(ui, f, "WEITER >>", None).clicked() {
                    a.push(Aktion::PresetSeite(logik::blaettern(seite, seiten, true)));
                }
                ui.end_row();
            });
    }
    let ordner = format!("Ordner: {}", k.preset_ordner.display());
    ui.add(Label::new(bausteine::text(f, &ordner, false, true)).truncate())
        .on_hover_text(&ordner);
    ui.horizontal(|ui| {
        let mut name = o.preset_name.clone();
        let platz = (breite - thema::SPACE_6 * 4.0).max(thema::SPACE_6 * 5.0);
        let feld = ui.add_sized(
            [platz, thema::SIZE_CONTROL_MD],
            egui::TextEdit::singleline(&mut name).hint_text("Name für Speichern unter"),
        );
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
        ui.add(Label::new(bausteine::text(
            f,
            "VBAN ist in diesem Lauf nicht verfügbar",
            false,
            true,
        )));
        return;
    };
    let stand = crate::vban::stand(send, recv, k.sample_rate);
    ui.add(Label::new(bausteine::text(
        f,
        &crate::vban::unterzeile(&stand),
        false,
        true,
    )));
    let Some(form) = o.vban.as_mut() else { return };
    egui::Grid::new("vban_formular")
        .num_columns(2)
        .spacing(egui::Vec2::splat(thema::SPACE_1))
        .min_col_width(LABEL_BREITE)
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
        ui.add(Label::new(bausteine::text(f, titel, true, false)));
        if zeilen.is_empty() {
            ui.add(Label::new(bausteine::text(f, "—", false, true)));
            continue;
        }
        egui::Grid::new(("vban", titel))
            .striped(true)
            .spacing(egui::Vec2::splat(thema::SPACE_1))
            .show(ui, |ui| {
                for kopf in ["STREAM", "IP", "SR", "CH", "FORMAT", "WEG", "FEHLER"] {
                    ui.add(Label::new(bausteine::text(f, kopf, false, true)));
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
                        ui.add(Label::new(bausteine::text(f, &t, false, false)));
                    }
                    ui.end_row();
                }
            });
    }
}

fn midi(ui: &mut Ui, f: &Farben, o: &mut Oberflaeche, a: &mut Vec<Aktion>, breite: f32) {
    let Some(m) = o.midi.as_mut() else { return };
    // Drei Felder in gleich breiten Spalten: Beschriftung oben, Feld darunter.
    // Die Beschriftung ist der zugängliche Name des Feldes.
    let spalte = (breite - 2.0 * thema::SPACE_2) / 3.0;
    let mut namen = [egui::Id::NULL; 3];
    egui::Grid::new("midi_kopf")
        .num_columns(3)
        .spacing(Vec2::new(thema::SPACE_2, thema::SPACE_1))
        .show(ui, |ui| {
            for (i, titel) in ["MIDI INPUT DEVICE", "MIDI MAP NAME", "MIDI OUTPUT DEVICE"]
                .into_iter()
                .enumerate()
            {
                namen[i] = feld(ui, spalte, thema::FONT_SIZE_SM * 1.5, |ui| {
                    ui.add(Label::new(bausteine::text(f, titel, false, true)).truncate())
                })
                .inner
                .id;
            }
            ui.end_row();
            feld(ui, spalte, thema::SIZE_CONTROL_MD, |ui| {
                egui::ComboBox::from_id_salt("midi_ein")
                    .width(spalte - thema::SPACE_6)
                    .selected_text(m.karte.eingang.clone())
                    .show_ui(ui, |ui| {
                        for g in &m.geraete {
                            ui.selectable_value(&mut m.karte.eingang, g.clone(), g);
                        }
                    })
                    .response
                    .labelled_by(namen[0]);
            });
            feld(ui, spalte, thema::SIZE_CONTROL_MD, |ui| {
                ui.add_sized(
                    [spalte, thema::SIZE_CONTROL_MD],
                    egui::TextEdit::singleline(&mut m.karte.name),
                )
                .labelled_by(namen[1]);
            });
            feld(ui, spalte, thema::SIZE_CONTROL_MD, |ui| {
                egui::ComboBox::from_id_salt("midi_aus")
                    .width(spalte - thema::SPACE_6)
                    .selected_text(m.karte.ausgang.clone())
                    .show_ui(ui, |ui| {
                        for g in &m.geraete {
                            ui.selectable_value(&mut m.karte.ausgang, g.clone(), g);
                        }
                    })
                    .response
                    .labelled_by(namen[2]);
            });
            ui.end_row();
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
        ui.add(Label::new(bausteine::text(
            f,
            &format!("{} gebunden", m.karte.gebunden()),
            false,
            true,
        )));
    });
    // Abschnitte als Reiter: Name und wie viele Ziele darin gebunden sind.
    // Kein Aufklappen – ein offener Abschnitt mit 56 Zeilen bräuchte eine
    // Scrollleiste (ADR 0007).
    ui.horizontal_wrapped(|ui| {
        for s in 0..crate::midimonster::SEITEN {
            let n = m.karte.gebunden_auf(s);
            let titel = crate::midimonster::seiten_titel(s);
            let text = if n > 0 {
                format!("{titel} · {n}")
            } else {
                titel.to_string()
            };
            let gewaehlt = m.seite == s;
            if ui.add(egui::Button::selectable(gewaehlt, text)).clicked() && !gewaehlt {
                a.push(Aktion::MidiSeite(s));
            }
        }
    });
    // Ein Blatt je Abschnitt: so viele Zeilen, wie im kleinsten Fenster Platz
    // haben – kein Scrollen (ADR 0007). Die drei Spalten stehen bündig.
    let erste = (breite * 0.45).max(thema::SPACE_6 * 5.0);
    let zweite = (breite * 0.3).max(thema::SPACE_6 * 4.0);
    egui::Grid::new("midi_zeilen")
        .striped(true)
        .spacing(Vec2::new(thema::SPACE_2, 0.0))
        .show(ui, |ui| {
            for z in m.karte.blatt(m.seite, m.blatt) {
                for (text, breite, gedaempft) in [(&z.titel, erste, false), (&z.ziel, zweite, true)]
                {
                    feld(ui, breite, thema::SIZE_CONTROL_MD, |ui| {
                        ui.add(Label::new(bausteine::text(f, text, false, gedaempft)).truncate());
                    });
                }
                let cc =
                    z.cc.map(|c| format!("CC {c}"))
                        .unwrap_or_else(|| "—".into());
                feld(ui, thema::SPACE_6 * 2.0, thema::SIZE_CONTROL_MD, |ui| {
                    ui.add(Label::new(bausteine::text(f, &cc, false, true)).truncate());
                });
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
    // Blättern nur, wo ein Abschnitt mehr Zeilen hat als ein Blatt.
    let n = crate::midimonster::seiten_blaetter(m.seite);
    if n > 1 {
        ui.horizontal(|ui| {
            if bausteine::kopfknopf(ui, f, "<< ZURÜCK", None).clicked() {
                a.push(Aktion::MidiBlatt(logik::midi_blaettern(
                    m.seite, m.blatt, false,
                )));
            }
            ui.add(Label::new(bausteine::text(
                f,
                &format!("BLATT {}/{n}", m.blatt + 1),
                false,
                true,
            )));
            if bausteine::kopfknopf(ui, f, "WEITER >>", None).clicked() {
                a.push(Aktion::MidiBlatt(logik::midi_blaettern(
                    m.seite, m.blatt, true,
                )));
            }
        });
    }
    if !m.status.is_empty() {
        ui.add(Label::new(bausteine::text(f, &m.status, false, true)).truncate());
    }
}

fn darstellung(ui: &mut Ui, f: &Farben, o: &mut Oberflaeche, a: &mut Vec<Aktion>, breite: f32) {
    // Zwei Spalten: links die Beschriftung, rechts die Bedienung – alles
    // untereinander auf einer Kante. Die Knopfreihen enden auf derselben Kante
    // wie der Autostart-Text: eine Luft vor dem Rand, damit nichts am Rahmen
    // klebt.
    let regler = (breite - LABEL_BREITE - thema::SPACE_2 - thema::SPACE_4).max(thema::SPACE_6);
    egui::Grid::new("darstellung")
        .num_columns(2)
        .spacing(Vec2::new(thema::SPACE_2, thema::SPACE_2))
        .min_col_width(LABEL_BREITE)
        .show(ui, |ui| {
            ui.add(Label::new(bausteine::text(f, "THEME", true, false)));
            ui.horizontal(|ui| {
                let je = (regler - 2.0 * thema::SPACE_1) / 3.0;
                for t in Thema::ALLE {
                    let gewaehlt = o.thema == t;
                    let knopf = egui::Button::image_and_text(
                        bausteine::thema_icon(t).bild(thema::FONT_SIZE_MD, f.text_muted),
                        t.anzeige(),
                    )
                    .selected(gewaehlt)
                    .frame_when_inactive(gewaehlt);
                    if ui.add_sized([je, thema::SIZE_CONTROL_SM], knopf).clicked() {
                        a.push(Aktion::Thema(t));
                    }
                }
            });
            ui.end_row();
            ui.add(Label::new(bausteine::text(f, "MAKROFELDER", true, false)));
            ui.horizontal(|ui| {
                let je = (regler - 5.0 * thema::SPACE_1) / MAKROFELDER.len() as f32;
                for n in MAKROFELDER {
                    let gewaehlt = o.makro_anzahl == n;
                    let knopf = egui::Button::selectable(gewaehlt, n.to_string());
                    if ui.add_sized([je, thema::SIZE_CONTROL_SM], knopf).clicked() && !gewaehlt {
                        a.push(Aktion::MakroAnzahl(n));
                    }
                }
            });
            ui.end_row();
            ui.add(Label::new(bausteine::text(f, "AUTOSTART", true, false)));
            let mut autostart = o.autostart;
            if ui.checkbox(&mut autostart, AUTOSTART).changed() {
                a.push(Aktion::Autostart(autostart));
            }
            ui.end_row();
        });
    // Unten die Version – gedämpft und linksbündig im Rahmen, wie die
    // Beschriftungen oben. Sie ist keine Einstellung, sondern eine Auskunft:
    // dieselbe Zahl, die `crosspoint --version` ausgibt.
    if let Some(version) = &o.version {
        ui.add_space(thema::SPACE_2);
        ui.add(
            Label::new(bausteine::text(
                f,
                &format!("Crosspoint {version}"),
                false,
                true,
            ))
            .truncate(),
        );
    }
}
