//! Das FILTER-Modal: die ganze Kette eines virtuellen Busses an einer Stelle.
//!
//! Ein Modal statt acht Knöpfe am Zug (der Platz reicht nicht, und ein
//! vergessener Schalter fällt in einer Reihe Lämpchen schneller auf als in
//! acht Popups). Aufbau: die Titelleiste des gemeinsamen Dialograhmens, darunter
//! der Pegel vor und nach den Filtern über die volle Breite und dann ein
//! **Raster** mit je einer Karte pro Filter – in der Reihenfolge der
//! Signalkette (ADR 0006). Die Spaltenzahl folgt aus der Breite des Fensters,
//! alle Spalten sind gleich breit und füllen die Innenbreite des Modals; die
//! Karten einer Zeile sind gleich hoch (ADR 0007: kein Scrollen).
//!
//! Wie überall im Pult: gelesen wird aus [`Oberflaeche`] und [`Kern`],
//! geschrieben nur als [`Aktion`] über `logik::ausfuehren`. Der offene Bus
//! steht im Modell (`Oberflaeche::offen`), nicht in der Ansicht.

use egui::{Label, Layout, Pos2, Rect, RichText, Sense, Ui, UiBuilder, Vec2, WidgetInfo};

use crosspoint::engine::NR_STUFEN;
use crosspoint::BUS_NAMES;

use crate::bausteine::{self, Art};
use crate::geraete::{self, Eintrag};
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

// ---------------------------------------------------------------- Maße ----

/// Unter dem Titel: was gefiltert wird und in welcher Reihenfolge.
const KETTE: &str = "Eingänge und A1–A5 tragen das Rohsignal · Kette: LC · ECHO · NR · GATE \
· COMP · AGC · LIMITER · DUCK · PTT";

/// Breite des Modals: höchstens vier Spalten. Schmaler wird es nicht, weil
/// eine Reglerzeile (Beschriftung, Schieber, Wert) sonst nicht mehr lesbar ist.
const MODAL_BREITE: f32 = thema::SPACE_6 * 56.0;

/// Kleinste Kartenbreite, ab der eine Reglerzeile mit Beschriftung und
/// Wertfeld noch Platz hat.
const KARTE_MINDESTBREITE: f32 = thema::SPACE_6 * 13.0;

/// Höhe einer Zeile im Raster (Regler, Häkchen, Wahlfeld). Eine Stufe enger
/// als ein Knopf im Pult – nur so passen die Karten auch im kleinsten Fenster
/// ohne Scrollen (ADR 0007).
const ZEILE: f32 = thema::SIZE_CONTROL_SM - thema::SPACE_2;

/// Höhe der Kopfzeile einer Karte: Kürzel, Titel und An/Aus-Schalter.
const KARTE_KOPF: f32 = thema::SIZE_CONTROL_SM;

/// Breite des An/Aus-Schalters oben rechts – in jeder Karte dieselbe.
const SCHALTER_BREITE: f32 = thema::SPACE_6 * 2.0;

/// Die längsten Werte, die ein Wertfeld aufnehmen muss (Grenze, Zeit in ms,
/// Verhältnis). Sie bestimmen die feste Breite der Wertspalte.
const WERT_LANG: [&str; 3] = ["-00.0 dBFS", "1000 ms", "20.0 : 1"];

/// Abstand zwischen zwei Karten.
const LUECKE: f32 = thema::SPACE_2;

/// Innenabstand einer Karte oben und unten (waagerecht ist es
/// [`thema::SPACE_2`]): enger, damit die Karten ins kleinste Fenster passen.
const KARTE_RAND: f32 = thema::SPACE_1;

/// Spalten aus der Breite: drei im kleinsten Fenster, vier bei breiterem.
const SPALTEN_MIN: usize = 3;
/// Mehr als vier Spalten verträgt keine Reglerzeile – dann kommt lieber eine
/// Zeile Karten dazu, als dass Beschriftung, Schieber und Wert zusammenrücken.
const SPALTEN_MAX: usize = 4;

/// Höchstens so viele Geräte in der ECHO-Karte. Mehr passt bei Mindestgröße
/// nicht ohne Scrollen (ADR 0007); der Rest steht als Hinweis darin.
const ECHO_ZEILEN: usize = 7;

/// Beschriftungen der Reglerzeilen, an einer Stelle: die breiteste bestimmt
/// die Beschriftungsspalte, und die Zeilen holen ihren Text von hier – so
/// bekommt keine Karte eine eigene Spalte, und keine Beschriftung läuft in
/// den Regler hinein.
mod wort {
    pub const GRENZE: &str = "Grenze";
    pub const SCHWELLE: &str = "Schwelle";
    pub const TIEFE: &str = "Tiefe";
    pub const ATTACK: &str = "Attack";
    pub const HOLD: &str = "Hold";
    pub const RELEASE: &str = "Release";
    pub const RATIO: &str = "Ratio";
    pub const MAKEUP: &str = "Make-up";
    pub const ZIELPEGEL: &str = "Zielpegel";
    pub const ZIELBUS: &str = "Zielbus";
    pub const ABSENKUNG: &str = "Absenkung";
    pub const STUFE: &str = "Stufe";
    pub const LIMITER: &str = "LIMITER";
    pub const OFFEN: &str = "OFFEN";

    /// Alle Beschriftungen, über die gemessen wird.
    pub const ALLE: [&str; 14] = [
        GRENZE, SCHWELLE, TIEFE, ATTACK, HOLD, RELEASE, RATIO, MAKEUP, ZIELPEGEL, ZIELBUS,
        ABSENKUNG, STUFE, LIMITER, OFFEN,
    ];
}

// ---------------------------------------------------------------- Spalten ----

/// Die drei Spalten einer Zeile, in jeder Karte an derselben Stelle:
/// Beschriftung (so breit wie die breiteste Beschriftung aller Karten),
/// Regler (der Rest der Kartenbreite) und Wert (rechts am Kartenrand).
#[derive(Clone, Copy)]
struct Spalten {
    label: f32,
    regler: f32,
    wert: f32,
}

impl Spalten {
    /// Die Spalten für eine Kartenbreite. `label` und `wert` kommen aus
    /// [`spalten_masse`] und gelten für alle Karten; den Rest bekommt der
    /// Regler, damit er bis an die Wertspalte reicht.
    fn fuer(karte: f32, label: f32, wert: f32) -> Self {
        let luecken = 2.0 * thema::SPACE_1;
        Self {
            label,
            regler: (karte - label - wert - luecken).max(thema::SPACE_6),
            wert,
        }
    }
}

/// Die Breiten der beiden festen Spalten, einmal für das ganze Modal gemessen:
/// die breiteste Beschriftung und der längste Wert. Gemessen wird in denselben
/// Schriften, in denen gezeichnet wird.
fn spalten_masse(ui: &Ui) -> (f32, f32) {
    let stil = ui.style();
    let beschriftung = egui::FontId::monospace(thema::FONT_SIZE_SM);
    let wert = stil.text_styles[&stil.drag_value_text_style].clone();
    let breite = |text: &str, font: &egui::FontId| {
        ui.painter()
            .layout_no_wrap(text.to_string(), font.clone(), egui::Color32::WHITE)
            .size()
            .x
    };
    let label = wort::ALLE
        .iter()
        .map(|text| breite(text, &beschriftung))
        .fold(0.0_f32, f32::max);
    let wertfeld = WERT_LANG
        .iter()
        .map(|text| breite(text, &wert))
        .fold(0.0_f32, f32::max);
    // Ein Pixel Luft zwischen Text und Spaltenkante, damit nichts klebt.
    (
        (label + thema::SPACE_1).ceil(),
        (wertfeld + 3.0 * thema::SPACE_1).ceil(),
    )
}

// --------------------------------------------------------------- Modal ----

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
    let karten = Karten {
        f,
        o,
        bus,
        ziel,
        fw,
        // Kartenbreite und Spalten setzt `raster`: sie hängen an der Breite des
        // Fensters und an der Schrift, also an dem, was erst im Dialog steht.
        breite: 0.0,
        spalten: Spalten {
            label: 0.0,
            regler: 0.0,
            wert: 0.0,
        },
    };
    // Titel ohne Dopplung: „FILTER · B1“, mit eigenem Namen „FILTER · B1 · Discord“.
    let titel = format!(
        "FILTER · {}{}",
        BUS_NAMES[bus],
        logik::eigener_name(ziel, o, k)
            .map(|n| format!(" · {n}"))
            .unwrap_or_default()
    );
    // Das Modal nutzt die Breite des Fensters, bleibt aber bei vier Spalten
    // stehen – sonst würden die Karten breiter als nötig.
    let breite = (ctx.content_rect().width() - 2.0 * thema::SPACE_4).min(MODAL_BREITE);
    let fensterhoehe = ctx.content_rect().height();
    let antwort = bausteine::dialog_rahmen(
        ctx,
        f,
        egui::Id::unique(("filter-modal", bus)),
        &titel,
        breite,
        |ui| {
            ui.spacing_mut().interact_size.y = ZEILE;
            ui.add(
                Label::new(bausteine::text(f, KETTE, false, true).size(thema::FONT_SIZE_SM))
                    .truncate(),
            );
            let innen = breite - 2.0 * bausteine::DIALOG_RAND;
            // Die festen Spalten einmal für alle Karten messen: die breiteste
            // Beschriftung und der längste Wert.
            let (label, wert) = spalten_masse(ui);
            let karten = Karten {
                spalten: Spalten {
                    label,
                    regler: 0.0,
                    wert,
                },
                ..karten
            };
            pegelzeile(ui, f, k, bus, innen, karten.spalten);
            raster(ui, f, &karten, innen, fensterhoehe, a);
        },
    );
    if antwort.zu {
        a.push(Aktion::Aufklappen(None));
    }
}

/// Pegel vor und nach den Filtern als eigene Zeile unter dem Titel, über die
/// volle Innenbreite: je eine Hälfte mit Beschriftung, Balken und Wert. Die
/// beiden Hälften tragen dieselben Spalten wie die Karten, damit Balken und
/// Werte untereinander stehen.
fn pegelzeile(ui: &mut Ui, f: &Farben, k: &Kern, bus: usize, breite: f32, spalten: Spalten) {
    let (roh, nachher) = logik::filter_pegel(&k.params, bus);
    // Eine Lücke zwischen den beiden Hälften, damit der Wert nicht an der
    // nächsten Beschriftung klebt.
    let halb = (breite - thema::SPACE_6) * 0.5;
    ui.allocate_ui_with_layout(
        Vec2::new(breite, ZEILE),
        Layout::left_to_right(egui::Align::Center),
        |ui| {
            for (titel, linear) in [("ROH", roh), ("NACHHER", nachher)] {
                ui.allocate_ui_with_layout(
                    Vec2::new(halb, ZEILE),
                    Layout::left_to_right(egui::Align::Center),
                    |ui| {
                        label_zelle(ui, f, spalten.label, titel);
                        let db = crate::fmt::lin_to_db(linear);
                        let rest =
                            (halb - spalten.label - spalten.wert - 2.0 * thema::SPACE_1).max(0.0);
                        let segmente = ((rest / 4.0) as usize).max(4);
                        bausteine::segmentleiste(ui, f, db, segmente);
                        ui.allocate_ui_with_layout(
                            Vec2::new(spalten.wert, ZEILE),
                            Layout::right_to_left(egui::Align::Center),
                            |ui| {
                                ui.set_min_width(spalten.wert);
                                ui.label(bausteine::text(
                                    f,
                                    &crate::fmt::db_text(db),
                                    false,
                                    false,
                                ));
                            },
                        );
                    },
                );
            }
        },
    );
}

// ---------------------------------------------------------------- Raster ----

/// Was alle Karten brauchen: Farben, Bestand und der Bus mit seinen Werten.
///
/// `breite` ist die Innenbreite der Karte. Zeilen rechnen damit statt mit
/// `ui.available_width()`: egui weitet `max_rect` auf, sobald ein Inhalt
/// überläuft, und dann würde jede Zeile ein Stück breiter als die Karte.
#[derive(Clone, Copy)]
struct Karten<'a> {
    f: &'a Farben,
    o: &'a Oberflaeche,
    bus: usize,
    ziel: Ziel,
    fw: logik::FilterWerte,
    breite: f32,
    spalten: Spalten,
}

/// Eine Karte der Kette: Titel, Beschreibung, Schalter und was sie zeichnet.
/// Alle acht stehen in einer Tabelle, damit Reihenfolge, Raster und Inhalt
/// dieselbe Quelle haben.
#[derive(Clone, Copy)]
struct Kette {
    kurz: &'static str,
    titel: &'static str,
    beschreibung: &'static str,
    param: &'static str,
    an: bool,
    /// Zeilen im Karteninhalt (eine Reglerzeile je Zeile).
    zeilen: usize,
    inhalt: fn(&mut Ui, &Karten, &mut Vec<Aktion>),
}

/// Eine Karte der Kette als Wert (die Tabelle ist zu lang für einen Aufruf).
#[allow(clippy::too_many_arguments)]
const fn karte(
    kurz: &'static str,
    titel: &'static str,
    beschreibung: &'static str,
    param: &'static str,
    an: bool,
    zeilen: usize,
    inhalt: fn(&mut Ui, &Karten, &mut Vec<Aktion>),
) -> Kette {
    Kette {
        kurz,
        titel,
        beschreibung,
        param,
        an,
        zeilen,
        inhalt,
    }
}

/// Die acht Karten in der Reihenfolge der Signalkette.
fn kette(k: &Karten) -> Vec<Kette> {
    let fw = k.fw;
    vec![
        karte(
            "LC",
            "LOW-CUT",
            "Alles unter der Grenze weg: Rumpeln, Trittschall.",
            "lc",
            fw.lc,
            1,
            lc,
        ),
        karte(
            "ECHO",
            "ECHOUNTERDRÜCKUNG",
            "Rechnet den Ton der Boxen aus dem Mikrofon heraus.",
            "aec",
            fw.aec,
            echo_zeilen(k.o),
            echo,
        ),
        karte(
            "NR",
            "RAUSCHUNTERDRÜCKUNG",
            "Lüfter, Tastatur, Maus – unabhängig von ECHO.",
            "nr",
            fw.nr,
            1,
            nr,
        ),
        karte(
            "GATE",
            "NOISE GATE",
            "Stille unter der Schwelle: Pausen zwischen den Sätzen.",
            "gate",
            fw.gate,
            5,
            gate,
        ),
        karte(
            "COMP",
            "KOMPRESSOR UND LIMITER",
            "Laute Stellen zusammen, leise bleiben.",
            "comp",
            fw.comp,
            6,
            comp,
        ),
        karte(
            "AGC",
            "AUTOMATISCHE PEGELANPASSUNG",
            "Zieht die Sprache auf den Zielpegel, egal wie laut man spricht.",
            "agc",
            fw.agc,
            1,
            agc,
        ),
        karte(
            "DUCK",
            "DUCKING",
            "Sprichst du hier, wird ein anderer Bus leiser.",
            "duck",
            fw.duck,
            5,
            duck,
        ),
        karte(
            "PTT",
            "PUSH-TO-TALK",
            "Bus stumm, solange die Taste nicht gedrückt ist.",
            "ptt",
            fw.ptt,
            1,
            ptt,
        ),
    ]
}

/// Das stehende Raster: Spaltenzahl, Kartenbreite und die Höhe je Zeile.
struct Raster {
    spalten: usize,
    breite: f32,
    zeilen: Vec<f32>,
    inhalt: Vec<Kette>,
}

impl Raster {
    fn hoehe(&self) -> f32 {
        self.zeilen.iter().sum::<f32>() + (self.zeilen.len().saturating_sub(1)) as f32 * LUECKE
    }
}

/// Die acht Karten als Raster zeichnen: gleiche Spaltenbreite, Zeilenhöhe nach
/// der höchsten Karte der Zeile, links nach rechts in der Reihenfolge der Kette.
fn raster(ui: &mut Ui, f: &Farben, k: &Karten, innen: f32, fensterhoehe: f32, a: &mut Vec<Aktion>) {
    let plan = raster_plan(ui, f, k, innen, platz_hoehe(fensterhoehe));
    let (bereich, antwort) = ui.allocate_exact_size(Vec2::new(innen, plan.hoehe()), Sense::hover());
    antwort.widget_info(|| WidgetInfo::labeled(egui::Role::Group, true, "Filterraster"));
    let mut y = bereich.top();
    for (zeile, hoehe) in plan.zeilen.iter().enumerate() {
        for spalte in 0..plan.spalten {
            let Some(info) = plan.inhalt.get(zeile * plan.spalten + spalte) else {
                continue;
            };
            let rect = Rect::from_min_size(
                Pos2::new(bereich.left() + spalte as f32 * (plan.breite + LUECKE), y),
                Vec2::new(plan.breite, *hoehe),
            );
            karte_zeichnen(ui, k, info, rect, a);
        }
        y += hoehe + LUECKE;
    }
}

/// Spaltenzahl und Höhen. Aus der Breite folgen drei oder vier Spalten; reicht
/// die Höhe des Fensters nicht (viele Geräte in der ECHO-Karte), kommt eine
/// Spalte dazu – lieber schmaler als abgeschnitten.
fn raster_plan(ui: &Ui, f: &Farben, k: &Karten, innen: f32, verfuegbar: f32) -> Raster {
    let inhalt = kette(k);
    let aus_breite = ((innen + LUECKE) / (KARTE_MINDESTBREITE + LUECKE)) as usize;
    let mut spalten = aus_breite.clamp(SPALTEN_MIN, SPALTEN_MAX);
    loop {
        let plan = plan_mit(ui, f, &inhalt, innen, spalten);
        if plan.hoehe() <= verfuegbar || spalten >= SPALTEN_MAX {
            return plan;
        }
        spalten += 1;
    }
}

fn plan_mit(ui: &Ui, f: &Farben, inhalt: &[Kette], innen: f32, spalten: usize) -> Raster {
    let breite = (innen - (spalten as f32 - 1.0) * LUECKE) / spalten as f32;
    let braucht: Vec<f32> = inhalt
        .iter()
        .map(|i| karte_hoehe(ui, f, i, breite))
        .collect();
    let zeilen = braucht
        .chunks(spalten)
        .map(|zeile| zeile.iter().copied().fold(0.0, f32::max))
        .collect();
    Raster {
        spalten,
        breite,
        zeilen,
        inhalt: inhalt.to_vec(),
    }
}

/// Platz für das Raster: die Fensterhöhe minus dem, was darüber steht (Luft
/// zum Fensterrand, Rahmen und Innenabstand des Modals, Titelleiste,
/// Kettentext, Pegelzeile).
fn platz_hoehe(fensterhoehe: f32) -> f32 {
    fensterhoehe
        - 2.0 * (thema::SPACE_2 + bausteine::DIALOG_RAND + thema::SIZE_BORDER)
        - bausteine::DIALOG_KOPF
        - 3.0 * thema::SPACE_1
        - thema::FONT_SIZE_SM * 1.5
        - ZEILE
        - 2.0 * thema::SPACE_1
}

/// Höhe, die eine Karte braucht: Kopf, Beschreibung (in der Kartenbreite
/// umbrochen) und ihre Zeilen. Dieselben Maße, mit denen sie gezeichnet wird.
fn karte_hoehe(ui: &Ui, f: &Farben, k: &Kette, breite: f32) -> f32 {
    let innen = breite - 2.0 * thema::SPACE_2;
    2.0 * KARTE_RAND
        + KARTE_KOPF
        + thema::SPACE_1
        + beschreibung_hoehe(ui, f, k.beschreibung, innen)
        + thema::SPACE_1
        + k.zeilen as f32 * ZEILE
        + k.zeilen.saturating_sub(1) as f32 * thema::SPACE_1
}

fn beschreibung_hoehe(ui: &Ui, f: &Farben, text: &str, breite: f32) -> f32 {
    ui.painter()
        .layout(
            text.to_string(),
            egui::FontId::monospace(thema::FONT_SIZE_SM),
            f.text_muted,
            breite.max(1.0),
        )
        .size()
        .y
}

/// Eine Karte in ihrem Rechteck: Fläche, Rahmen, Kopf mit Schalter,
/// Beschreibung und die Zeilen. Die Höhe kommt vom Raster, damit alle Karten
/// einer Zeile gleich hoch sind.
fn karte_zeichnen(ui: &mut Ui, k: &Karten, info: &Kette, rect: Rect, a: &mut Vec<Aktion>) {
    let f = k.f;
    let ecke = egui::CornerRadius::same(thema::RADIUS_MD as u8);
    let p = ui.painter_at(rect);
    p.rect_filled(rect, ecke, f.bg_raised);
    p.rect_stroke(
        rect,
        ecke,
        egui::Stroke::new(thema::SIZE_BORDER, f.border_subtle),
        egui::StrokeKind::Inside,
    );
    // Das Kartenrechteck als eigener Knoten: die Tests vergleichen daran die
    // Breiten, ein Screenreader liest den Kartennamen.
    let rand = ui.interact(
        rect,
        ui.scope_id().with(("filterkarte", info.kurz)),
        Sense::hover(),
    );
    rand.widget_info(|| {
        WidgetInfo::labeled(
            egui::Role::Group,
            true,
            format!("Filterkarte {}", info.kurz),
        )
    });
    let innen = rect.shrink2(Vec2::new(thema::SPACE_2, KARTE_RAND));
    let karte = Karten {
        breite: innen.width(),
        spalten: Spalten::fuer(innen.width(), k.spalten.label, k.spalten.wert),
        ..*k
    };
    ui.scope_builder(
        UiBuilder::new()
            .max_rect(innen)
            .id_salt(("karte", info.kurz)),
        |ui| {
            ui.spacing_mut().item_spacing = Vec2::splat(thema::SPACE_1);
            karten_kopf(ui, &karte, info, a);
            beschreibung(ui, &karte, info.beschreibung);
            (info.inhalt)(ui, &karte, a);
        },
    );
}

/// Kopf einer Karte: Kürzel und Titel links, An/Aus-Schalter rechts – in jeder
/// Karte gleich groß und auf derselben Höhe.
fn karten_kopf(ui: &mut Ui, k: &Karten, info: &Kette, a: &mut Vec<Aktion>) {
    let f = k.f;
    let breite = k.breite;
    let titel_breite = (breite - SCHALTER_BREITE - 2.0 * thema::SPACE_1).max(0.0);
    ui.allocate_ui_with_layout(
        Vec2::new(breite, KARTE_KOPF),
        Layout::left_to_right(egui::Align::Center),
        |ui| {
            ui.allocate_ui_with_layout(
                Vec2::new(titel_breite, KARTE_KOPF),
                Layout::left_to_right(egui::Align::Center),
                |ui| {
                    ui.add(
                        Label::new(
                            RichText::new(format!("{} · {}", info.kurz, info.titel))
                                .font(egui::FontId::new(thema::FONT_SIZE_SM, thema::fett()))
                                .color(if info.an {
                                    f.accent_default
                                } else {
                                    f.text_muted
                                }),
                        )
                        .truncate(),
                    );
                },
            );
            ui.with_layout(Layout::right_to_left(egui::Align::Center), |ui| {
                let groesse = Vec2::new(SCHALTER_BREITE, KARTE_KOPF - thema::SPACE_1);
                if bausteine::schalter(ui, f, info.kurz, info.an, Art::Akzent, groesse).clicked() {
                    a.push(setzen(k.ziel, info.param, schalter_wert(!info.an)));
                }
            });
        },
    );
}

/// Beschreibung der Karte: linksbündig umbrochen, keine gestreckten Zeilen.
fn beschreibung(ui: &mut Ui, k: &Karten, text: &str) {
    let (f, breite) = (k.f, k.breite);
    ui.allocate_ui_with_layout(
        Vec2::new(breite, 0.0),
        Layout::top_down(egui::Align::LEFT),
        |ui| {
            ui.set_width(breite);
            ui.add(Label::new(bausteine::text(f, text, false, true)).wrap());
        },
    );
}

// ---------------------------------------------------------------- Zeilen ----

/// Eine Zeile im Kartenraster: feste Höhe und Breite, links beginnend – so
/// stehen alle Regler untereinander auf einer Kante.
fn zeile(ui: &mut Ui, breite: f32, inhalt: impl FnOnce(&mut Ui)) {
    ui.allocate_ui_with_layout(
        Vec2::new(breite, ZEILE),
        Layout::left_to_right(egui::Align::Center),
        |ui| {
            // Ohne dieses Mindestmaß schrumpft die Zeile auf ihren Inhalt, und
            // dann stünde in jeder Karte eine andere Spalte.
            ui.set_min_size(Vec2::new(breite, ZEILE));
            ui.spacing_mut().item_spacing.x = thema::SPACE_1;
            inhalt(ui);
        },
    );
}

/// Die feste Beschriftungsspalte einer Zeile (gibt die Antwort für
/// `labelled_by` zurück).
fn label_zelle(ui: &mut Ui, f: &Farben, breite: f32, text: &str) -> egui::Response {
    ui.allocate_ui_with_layout(
        Vec2::new(breite, ZEILE),
        Layout::left_to_right(egui::Align::Center),
        |ui| {
            ui.set_min_width(breite);
            ui.add(Label::new(bausteine::text(f, text, false, true)).truncate())
        },
    )
    .inner
}

/// Der Schieber einer Zeile: er füllt die Reglerspalte genau, damit alle
/// Schieber einer Karte gleich lang sind und bis an die Wertspalte reichen.
fn schieber(ui: &mut Ui, breite: f32, wert: &mut f32, von: f32, bis: f32) -> egui::Response {
    ui.spacing_mut().slider_width = breite;
    ui.add(egui::Slider::new(wert, von..=bis).show_value(false))
}

/// Das feste Wertfeld rechts einer Zeile: feste Breite, rechte Kante am
/// Kartenrand, damit alle Zahlen in einer Linie enden.
fn wert_zelle(
    ui: &mut Ui,
    breite: f32,
    wert: &mut f32,
    von: f32,
    bis: f32,
    suffix: &str,
    dezimal: usize,
) -> egui::Response {
    ui.allocate_ui_with_layout(
        Vec2::new(breite, ZEILE),
        Layout::right_to_left(egui::Align::Center),
        |ui| {
            ui.set_min_width(breite);
            // Enge Polsterung: das Wertfeld soll die Zeilenhöhe nicht
            // aufweiten, sonst passt das Raster nicht mehr in sein Maß.
            ui.spacing_mut().button_padding = Vec2::new(thema::SPACE_1, 0.0);
            ui.add(
                egui::DragValue::new(wert)
                    .range(von..=bis)
                    .suffix(suffix)
                    .fixed_decimals(dezimal)
                    .min_size(Vec2::new(breite, ZEILE)),
            )
        },
    )
    .inner
}

/// Ein Regler: Beschriftung links in fester Spalte, Schieber über den ganzen
/// Rest, Wert rechts in fester Spalte.
struct Regler<'a> {
    titel: &'a str,
    param: &'a str,
    von: f32,
    bis: f32,
    wert: f32,
    suffix: &'a str,
    dezimal: usize,
}

fn regler(ui: &mut Ui, k: &Karten, r: Regler, a: &mut Vec<Aktion>) {
    let f = k.f;
    let spalten = k.spalten;
    zeile(ui, k.breite, |ui| {
        label_zelle(ui, f, spalten.label, r.titel);
        let mut v = r.wert;
        let schieber = schieber(ui, spalten.regler, &mut v, r.von, r.bis);
        let name = r.titel.to_string();
        schieber.widget_info(|| WidgetInfo::slider(true, v as f64, name.clone()));
        let feld = wert_zelle(ui, spalten.wert, &mut v, r.von, r.bis, r.suffix, r.dezimal);
        if schieber.changed() || feld.changed() {
            a.push(setzen(k.ziel, r.param, v));
        }
    });
}

// ---------------------------------------------------------- Die acht Karten ----

fn lc(ui: &mut Ui, k: &Karten, a: &mut Vec<Aktion>) {
    regler(
        ui,
        k,
        Regler {
            titel: wort::GRENZE,
            param: "lc_freq",
            von: 40.0,
            bis: 200.0,
            wert: k.fw.lc_freq,
            suffix: " Hz",
            dezimal: 0,
        },
        a,
    );
}

fn echo(ui: &mut Ui, k: &Karten, a: &mut Vec<Aktion>) {
    let f = k.f;
    let liste = ausgabegeraete(k.o);
    if liste.is_empty() {
        zeile(ui, k.breite, |ui| {
            ui.add(
                Label::new(bausteine::text(
                    f,
                    "kein lokales Ausgabegerät gefunden",
                    false,
                    true,
                ))
                .truncate(),
            );
        });
        return;
    }
    for e in liste.iter().take(ECHO_ZEILEN) {
        zeile(ui, k.breite, |ui| {
            let mut an = k.o.echo_geraete.contains(&e.name);
            // Einzeilig gekürzt, damit die Zeile in die Karte passt; der volle
            // Name steht im Tooltip.
            // Platz für den Text: die Zeile minus Häkchen und seine Lücke.
            let platz = k.breite - ui.spacing().icon_width - 2.0 * ui.spacing().item_spacing.x;
            let voll = geraete::eintrag_text(&e.anzeige, e.fehlt);
            // Kleine Schrift wie in der Karte übrigens; gemessen wird mit
            // derselben Schrift, in der das Häkchen den Text zeichnet.
            let font = egui::FontId::monospace(thema::FONT_SIZE_SM);
            let kurz = bausteine::kuerzen(ui, &voll, platz, font.clone());
            let antwort = ui.add(egui::Checkbox::new(
                &mut an,
                egui::RichText::new(kurz).font(font),
            ));
            antwort.widget_info(|| {
                WidgetInfo::selected(egui::Role::CheckBox, true, an, e.anzeige.clone())
            });
            let antwort = antwort.on_hover_text(&e.name);
            if antwort.changed() {
                a.push(Aktion::EchoGeraet {
                    name: e.name.clone(),
                    an,
                });
            }
        });
    }
    let rest = liste.len().saturating_sub(ECHO_ZEILEN);
    if rest > 0 {
        zeile(ui, k.breite, |ui| {
            ui.add(
                Label::new(bausteine::text(
                    f,
                    &format!("… {rest} weitere Geräte"),
                    false,
                    true,
                ))
                .truncate(),
            )
            .on_hover_text(
                liste
                    .iter()
                    .skip(ECHO_ZEILEN)
                    .map(|e| e.anzeige.clone())
                    .collect::<Vec<_>>()
                    .join("\n"),
            );
        });
    }
}

fn nr(ui: &mut Ui, k: &Karten, a: &mut Vec<Aktion>) {
    let f = k.f;
    let spalten = k.spalten;
    zeile(ui, k.breite, |ui| {
        label_zelle(ui, f, spalten.label, wort::STUFE);
        // Die vier Stufen füllen die Reglerspalte genau.
        let zelle = (spalten.regler - 3.0 * thema::SPACE_1) / 4.0;
        for (i, stufe) in NR_STUFEN.iter().enumerate() {
            let gewaehlt = k.fw.nr_stufe as usize == i;
            let groesse = Vec2::new(zelle, ZEILE - thema::SPACE_1);
            if bausteine::schalter(ui, f, stufe, gewaehlt, Art::Akzent, groesse).clicked()
                && !gewaehlt
            {
                a.push(setzen(k.ziel, "nr_stufe", i as f32));
            }
        }
    });
}

fn gate(ui: &mut Ui, k: &Karten, a: &mut Vec<Aktion>) {
    let fw = k.fw;
    for (titel, param, von, bis, wert, suffix, dezimal) in [
        (
            wort::SCHWELLE,
            "gate_schwelle",
            -80.0,
            -10.0,
            fw.gate_schwelle,
            " dBFS",
            0,
        ),
        (
            wort::TIEFE,
            "gate_tiefe",
            0.0,
            80.0,
            fw.gate_tiefe,
            " dB",
            0,
        ),
        (
            wort::ATTACK,
            "gate_attack",
            0.5,
            100.0,
            fw.gate_attack,
            " ms",
            1,
        ),
        (wort::HOLD, "gate_hold", 0.0, 500.0, fw.gate_hold, " ms", 0),
        (
            wort::RELEASE,
            "gate_release",
            5.0,
            1000.0,
            fw.gate_release,
            " ms",
            0,
        ),
    ] {
        regler(
            ui,
            k,
            Regler {
                titel,
                param,
                von,
                bis,
                wert,
                suffix,
                dezimal,
            },
            a,
        );
    }
}

fn comp(ui: &mut Ui, k: &Karten, a: &mut Vec<Aktion>) {
    let f = k.f;
    let fw = k.fw;
    for (titel, param, von, bis, wert, suffix, dezimal) in [
        (
            wort::SCHWELLE,
            "comp_schwelle",
            -60.0,
            0.0,
            fw.comp_schwelle,
            " dBFS",
            0,
        ),
        (
            wort::RATIO,
            "comp_ratio",
            1.0,
            20.0,
            fw.comp_ratio,
            " : 1",
            1,
        ),
        (
            wort::ATTACK,
            "comp_attack",
            0.1,
            100.0,
            fw.comp_attack,
            " ms",
            1,
        ),
        (
            wort::RELEASE,
            "comp_release",
            5.0,
            1000.0,
            fw.comp_release,
            " ms",
            0,
        ),
        (
            wort::MAKEUP,
            "comp_makeup",
            0.0,
            24.0,
            fw.comp_makeup,
            " dB",
            1,
        ),
    ] {
        regler(
            ui,
            k,
            Regler {
                titel,
                param,
                von,
                bis,
                wert,
                suffix,
                dezimal,
            },
            a,
        );
    }
    // Limiter in der Beschriftungsspalte, seine Decke auf demselben Schieber
    // wie die Regler darüber – so bleibt die Zeile im Raster.
    let spalten = k.spalten;
    zeile(ui, k.breite, |ui| {
        let groesse = Vec2::new(spalten.label, ZEILE - thema::SPACE_1);
        if bausteine::schalter(ui, f, wort::LIMITER, fw.limiter, Art::Akzent, groesse).clicked() {
            a.push(setzen(k.ziel, "limiter", schalter_wert(!fw.limiter)));
        }
        let mut v = fw.limiter_decke;
        let schieber =
            schieber(ui, spalten.regler, &mut v, -12.0, 0.0).on_hover_text("Decke des Limiters");
        schieber.widget_info(|| WidgetInfo::slider(true, v as f64, "Decke"));
        let feld = wert_zelle(ui, spalten.wert, &mut v, -12.0, 0.0, " dBFS", 1);
        if schieber.changed() || feld.changed() {
            a.push(setzen(k.ziel, "limiter_decke", v));
        }
    });
}

fn agc(ui: &mut Ui, k: &Karten, a: &mut Vec<Aktion>) {
    regler(
        ui,
        k,
        Regler {
            titel: wort::ZIELPEGEL,
            param: "agc_ziel",
            von: -40.0,
            bis: 0.0,
            wert: k.fw.agc_ziel,
            suffix: " dBFS",
            dezimal: 0,
        },
        a,
    );
}

fn duck(ui: &mut Ui, k: &Karten, a: &mut Vec<Aktion>) {
    let f = k.f;
    let fw = k.fw;
    let spalten = k.spalten;
    zeile(ui, k.breite, |ui| {
        let l = label_zelle(ui, f, spalten.label, wort::ZIELBUS);
        // Die Auswahl nimmt die ganze Reglerspalte ein – so breit wie die
        // Schieber darunter.
        egui::ComboBox::from_id_salt(("duck_ziel", k.bus))
            .width(spalten.regler)
            .selected_text(BUS_NAMES[fw.duck_ziel])
            .show_ui(ui, |ui| {
                for (b, busname) in BUS_NAMES.iter().enumerate() {
                    if ui.selectable_label(fw.duck_ziel == b, *busname).clicked()
                        && fw.duck_ziel != b
                    {
                        a.push(setzen(k.ziel, "duck_ziel", b as f32));
                    }
                }
            })
            .response
            .labelled_by(l.id);
    });
    for (titel, param, von, bis, wert, suffix, dezimal) in [
        (
            wort::ABSENKUNG,
            "duck_tiefe",
            0.0,
            48.0,
            fw.duck_tiefe,
            " dB",
            0,
        ),
        (
            wort::SCHWELLE,
            "duck_schwelle",
            -60.0,
            0.0,
            fw.duck_schwelle,
            " dBFS",
            0,
        ),
        (
            wort::ATTACK,
            "duck_attack",
            0.5,
            500.0,
            fw.duck_attack,
            " ms",
            0,
        ),
        (
            wort::RELEASE,
            "duck_release",
            5.0,
            3000.0,
            fw.duck_release,
            " ms",
            0,
        ),
    ] {
        regler(
            ui,
            k,
            Regler {
                titel,
                param,
                von,
                bis,
                wert,
                suffix,
                dezimal,
            },
            a,
        );
    }
}

fn ptt(ui: &mut Ui, k: &Karten, a: &mut Vec<Aktion>) {
    let f = k.f;
    let spalten = k.spalten;
    zeile(ui, k.breite, |ui| {
        let groesse = Vec2::new(spalten.label, ZEILE - thema::SPACE_1);
        let offen = bausteine::schalter(ui, f, wort::OFFEN, k.fw.ptt_offen, Art::Akzent, groesse)
            .on_hover_text("Taste gedrückt: der Bus ist hörbar");
        if offen.clicked() {
            a.push(setzen(k.ziel, "ptt_offen", schalter_wert(!k.fw.ptt_offen)));
        }
        // Der Hinweis bekommt Regler- und Wertspalte, damit er nicht über den
        // Kartenrand hinausläuft.
        let rest = spalten.regler + spalten.wert + thema::SPACE_1;
        ui.allocate_ui_with_layout(
            Vec2::new(rest, ZEILE),
            Layout::left_to_right(egui::Align::Center),
            |ui| {
                ui.set_min_width(rest);
                ui.add(
                    Label::new(
                        bausteine::text(f, "über MIDI oder /bus/<n>/ptt_offen", false, true)
                            .size(thema::FONT_SIZE_SM),
                    )
                    .truncate(),
                )
                .on_hover_text("Auf der Bühne über MIDI oder /bus/<n>/ptt_offen.");
            },
        );
    });
}

// ------------------------------------------------------------ Bestand ----

/// Ausgabegeräte für die ECHO-Auswahl: nur lokale Sinks, die im Raum zu hören
/// sind ([`geraete::ist_echo_kandidat`]), dazu gemerkte, die gerade fehlen.
///
/// Was schon gewählt ist, bleibt sichtbar – auch wenn es kein Kandidat mehr
/// ist (eine ältere `echo.json` kann so etwas enthalten). Sonst ließe sich
/// eine laufende Referenz nicht mehr abwählen.
fn ausgabegeraete(o: &Oberflaeche) -> Vec<Eintrag> {
    let bestand = match &o.geraete {
        Geraete::Fertig(b) => Some(b.as_ref()),
        Geraete::Laedt => None,
    };
    let gewaehlt = |name: &str| o.echo_geraete.iter().any(|n| n == name);
    let mut liste: Vec<Eintrag> = bestand
        .map(|b| {
            b.senken
                .iter()
                .filter(|e| geraete::ist_echo_kandidat(&e.name) || gewaehlt(&e.name))
                .cloned()
                .collect()
        })
        .unwrap_or_default();
    for name in &o.echo_geraete {
        if liste.iter().any(|e| e.name == *name) {
            continue;
        }
        liste.push(Eintrag {
            name: name.clone(),
            anzeige: geraete::kurzname(name),
            fehlt: true,
        });
    }
    liste
}

/// Zeilen der ECHO-Karte: eine je Gerät, dazu der Hinweis, wenn nicht alle
/// Platz haben.
fn echo_zeilen(o: &Oberflaeche) -> usize {
    let n = ausgabegeraete(o).len();
    n.clamp(1, ECHO_ZEILEN) + usize::from(n > ECHO_ZEILEN)
}
