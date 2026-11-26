//! Das FILTER-Modal: die ganze Kette eines virtuellen Busses an einer Stelle.
//!
//! Ein Modal statt acht Knöpfe am Zug (der Platz reicht nicht, und ein
//! vergessener Schalter fällt in einer Reihe Lämpchen schneller auf als in
//! acht Popups). Aufbau: die Titelleiste des gemeinsamen Dialograhmens, darunter
//! der Pegel vor und nach den Filtern über die volle Breite und dann ein
//! **Raster** mit je einer Karte pro Filter – in der Reihenfolge der
//! Signalkette (ADR 0006). Die Spaltenzahl folgt aus der Breite des Fensters,
//! alle Spalten sind gleich breit und füllen die Innenbreite des Modals; die
//! Karten einer Zeile sind gleich hoch (ADR 0007: kein Scrollen). Reicht die
//! Höhe nicht, zeigt die ECHO-Karte weniger Geräte – lieber eine Zeile weniger
//! als eine Karte, die aus dem Fenster läuft.
//!
//! Wie überall im Pult: gelesen wird aus [`Oberflaeche`] und [`Kern`],
//! geschrieben nur als [`Aktion`] über `logik::ausfuehren`. Der offene Bus
//! steht im Modell (`Oberflaeche::offen`), nicht in der Ansicht.

use egui::{Label, Layout, Pos2, Rect, RichText, Sense, Ui, UiBuilder, Vec2, WidgetInfo};

use crosspoint_modell::params::NR_STUFEN;
use crosspoint_modell::BUS_NAMES;

use crate::bausteine::{self, Art};
use crate::geraete::{self, Eintrag};
use crate::logik::{self, Kern, PEGEL_SEGMENTE};
use crate::modell::{Aktion, Aufklapp, Geraete, Oberflaeche, Pegel, Ziel};
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
const KETTE_BUS: &str = "Kette am Bus hinter der Summe: LC · ECHO · NR · GATE · COMP · AGC \
· LIMITER · DUCK · PTT";

/// Am Kanalzug steht dieselbe Kette, aber vor dem Fader und damit vor dem
/// Routing (ADR 0011) – das ist der Unterschied, den man hören kann.
const KETTE_KANAL: &str = "Kette am Kanalzug vor dem Fader: LC · ECHO · NR · GATE · COMP · AGC · \
LIMITER · DUCK · PTT";

/// Breite des Modals: höchstens vier Spalten. Schmaler wird es nicht, weil
/// eine Reglerzeile (Beschriftung, Schieber, Wert) sonst nicht mehr lesbar ist.
const MODAL_BREITE: f32 = thema::SPACE_6 * 56.0;

/// Kleinste Kartenbreite, ab der eine Reglerzeile mit Beschriftung und
/// Wertfeld noch Platz hat. Sie entscheidet, wie viele Spalten nebeneinander
/// stehen: bei der Mindestgröße des Fensters sind es vier, und vier ist auch
/// die Regel – acht Karten füllen dann zwei volle Zeilen, keine Zelle bleibt
/// leer.
const KARTE_MINDESTBREITE: f32 = thema::SPACE_6 * 11.0;

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

/// Spaltenzahl aus der Breite: vier ist die Regel (acht Karten sind dann zwei
/// volle Zeilen), weniger nur, wenn die Karten sonst zu schmal würden. Mehr
/// als vier verträgt keine Reglerzeile.
const SPALTEN_MIN: usize = 3;
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
    let Some(Aufklapp::Filter(ziel)) = o.offen else {
        return;
    };
    let Some(fw) = logik::filter_von(&k.params, ziel, o.master_bus) else {
        // Ein Bus ohne Filter (A1–A5): es gibt nichts zu zeigen. Der Zustand
        // wird zurückgesetzt, damit das Modal nicht „offen, aber leer“ bleibt.
        a.push(Aktion::Aufklappen(None));
        return;
    };
    let karten = Karten {
        f,
        o,
        ziel,
        fw,
        // Kartenbreite, Spalten und die Zahl der gezeigten Geräte setzt
        // `raster`: sie hängen an der Fläche des Fensters und an der Schrift,
        // also an dem, was erst im Dialog feststeht.
        breite: 0.0,
        spalten: Spalten {
            label: 0.0,
            regler: 0.0,
            wert: 0.0,
        },
        echo_max: ECHO_ZEILEN,
    };
    // Titel ohne Dopplung: „FILTER · B1“, mit eigenem Namen „FILTER · B1 · Discord“.
    let titel = format!(
        "FILTER · {}{}",
        logik::vorgabe_name(ziel, o.master_bus),
        logik::eigener_name(ziel, o, k)
            .map(|n| format!(" · {n}"))
            .unwrap_or_default()
    );
    let kette = if matches!(ziel, Ziel::Kanal(_)) {
        KETTE_KANAL
    } else {
        KETTE_BUS
    };
    // Das Modal nutzt die Breite des Fensters, bleibt aber bei vier Spalten
    // stehen – sonst würden die Karten breiter als nötig.
    let breite = (ctx.content_rect().width() - 2.0 * thema::SPACE_4).min(MODAL_BREITE);
    let fensterhoehe = ctx.content_rect().height();
    let antwort = bausteine::dialog_rahmen(
        ctx,
        f,
        egui::Id::unique(("filter-modal", Pegel::index(ziel))),
        &titel,
        breite,
        |ui| {
            ui.spacing_mut().interact_size.y = ZEILE;
            ui.add(
                Label::new(bausteine::text(f, kette, false, true).size(thema::FONT_SIZE_SM))
                    .truncate(),
            );
            // `breite` ist schon die Innenbreite des Rahmens (der Rand des
            // Dialogs steckt im Frame) – sie wird nicht noch einmal um den Rand
            // gekürzt, sonst bliebe rechts eine Lücke.
            let innen = breite;
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
            pegelzeile(ui, f, k, ziel, o.master_bus, innen, karten.spalten);
            raster(ui, f, &karten, innen, fensterhoehe, a);
        },
    );
    if antwort.zu {
        a.push(Aktion::Aufklappen(None));
    }
}

/// Pegel vor und nach den Filtern als eigene Zeile unter dem Titel: zwei
/// gleich breite Hälften mit einer Lücke dazwischen, je Beschriftung, Balken
/// und Wert. Die Hälften tragen dieselben Spalten wie die Karten und stehen
/// auf denselben Kanten – der Balken reicht von der Beschriftung bis an die
/// Wertspalte, der Wert endet unter den Werten der Karten.
fn pegelzeile(
    ui: &mut Ui,
    f: &Farben,
    k: &Kern,
    ziel: Ziel,
    master_bus: usize,
    breite: f32,
    spalten: Spalten,
) {
    let (roh, nachher) = logik::filter_pegel(&k.params, ziel, master_bus);
    // Die Zeile steht auf den Innenkanten der Karten: links und rechts ein
    // Kartenrand, dazwischen zwei Hälften und eine Lücke.
    let innen = (breite - 2.0 * thema::SPACE_2).max(spalten.label + spalten.wert);
    let luecke = thema::SPACE_6;
    let halb = (innen - luecke) * 0.5;
    ui.allocate_ui_with_layout(
        Vec2::new(breite, ZEILE),
        Layout::left_to_right(egui::Align::Center),
        |ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            ui.add_space(thema::SPACE_2);
            ui.spacing_mut().item_spacing.x = luecke;
            for (titel, linear) in [("ROH", roh), ("NACHHER", nachher)] {
                ui.allocate_ui_with_layout(
                    Vec2::new(halb, ZEILE),
                    Layout::left_to_right(egui::Align::Center),
                    |ui| {
                        ui.spacing_mut().item_spacing.x = thema::SPACE_1;
                        label_zelle(ui, f, spalten.label, titel);
                        let db = crate::fmt::lin_to_db(linear);
                        let rest =
                            (halb - spalten.label - spalten.wert - 2.0 * thema::SPACE_1).max(0.0);
                        bausteine::segmentleiste_breit(ui, f, db, rest, PEGEL_SEGMENTE);
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
    ziel: Ziel,
    fw: logik::FilterWerte,
    breite: f32,
    spalten: Spalten,
    /// Wie viele Geräte die ECHO-Karte zeigt (höchstens [`ECHO_ZEILEN`]).
    echo_max: usize,
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
            echo_zeilen(k),
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

/// Das stehende Raster: Spaltenzahl, Kartenbreite, die Höhe je Zeile und wie
/// viele Geräte die ECHO-Karte zeigt.
struct Raster {
    spalten: usize,
    breite: f32,
    zeilen: Vec<f32>,
    inhalt: Vec<Kette>,
    echo_max: usize,
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
    // Gezeichnet wird mit dem Stand des Plans: dieselbe Zahl Geräte in der
    // ECHO-Karte, mit der gerechnet wurde.
    let k = Karten {
        echo_max: plan.echo_max,
        ..*k
    };
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
            karte_zeichnen(ui, &k, info, rect, a);
        }
        y += hoehe + LUECKE;
    }
}

/// Spaltenzahl und Höhen. Die Spalten folgen aus der Breite (so viele, wie ohne
/// zu enge Karten passen, höchstens vier); reicht die Höhe des Fensters nicht,
/// zeigt die ECHO-Karte weniger Geräte – lieber eine Gerätezeile weniger als
/// eine Karte, die unten aus dem Fenster läuft.
fn raster_plan(ui: &Ui, f: &Farben, k: &Karten, innen: f32, verfuegbar: f32) -> Raster {
    let spalten = spalten_fuer(innen);
    let moeglich = echo_zeilen(&Karten {
        echo_max: ECHO_ZEILEN,
        ..*k
    });
    let mut plan = plan_mit(ui, f, k, innen, spalten, moeglich);
    let mut zeilen = moeglich;
    while plan.hoehe() > verfuegbar && zeilen > 1 {
        zeilen -= 1;
        plan = plan_mit(ui, f, k, innen, spalten, zeilen);
    }
    plan
}

/// Die Spaltenzahl zur Innenbreite: vier, solange die Karten nicht zu schmal
/// werden, sonst eine weniger.
fn spalten_fuer(innen: f32) -> usize {
    let mut spalten = SPALTEN_MAX;
    while spalten > SPALTEN_MIN {
        let breite = (innen - (spalten as f32 - 1.0) * LUECKE) / spalten as f32;
        if breite >= KARTE_MINDESTBREITE {
            break;
        }
        spalten -= 1;
    }
    spalten
}

fn plan_mit(
    ui: &Ui,
    f: &Farben,
    k: &Karten,
    innen: f32,
    spalten: usize,
    echo_max: usize,
) -> Raster {
    let breite = (innen - (spalten as f32 - 1.0) * LUECKE) / spalten as f32;
    let karten = Karten {
        breite,
        echo_max,
        ..*k
    };
    let inhalt = kette(&karten);
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
        inhalt,
        echo_max,
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
/// umbrochen) und ihre Zeilen. Dieselben Maße, mit denen sie gezeichnet wird:
/// die Beschreibung wird als fertiger Textblock in genau dieser Höhe gezeichnet
/// und die Zeilen sind fest hoch – so kann keine Karte ihre Zeile sprengen.
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

/// Der umbrochene Text der Beschreibung – einmal für das Maß, einmal fürs
/// Zeichnen. Beide nehmen dieselbe Funktion: so können Plan und Bild nicht
/// auseinanderlaufen.
fn beschreibung_galley(
    ui: &Ui,
    f: &Farben,
    text: &str,
    breite: f32,
) -> std::sync::Arc<egui::Galley> {
    ui.painter().layout(
        text.to_string(),
        egui::FontId::monospace(thema::FONT_SIZE_SM),
        f.text_muted,
        breite.max(1.0),
    )
}

fn beschreibung_hoehe(ui: &Ui, f: &Farben, text: &str, breite: f32) -> f32 {
    beschreibung_galley(ui, f, text, breite).size().y
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

/// Beschreibung der Karte: linksbündig umbrochen, in **genau** der Höhe, die
/// das Raster eingeplant hat. Gezeichnet wird der fertige Textblock statt eines
/// Labels: ein Label wächst mit seinem Text, und dann schöbe es die Zeilen
/// darunter aus der Karte (der Regler der letzten Zeile lag auf dem Rahmen).
/// Für Screenreader bleibt der Text als Beschriftung da.
fn beschreibung(ui: &mut Ui, k: &Karten, text: &str) {
    let (f, breite) = (k.f, k.breite);
    let galley = beschreibung_galley(ui, f, text, breite);
    let hoehe = galley.size().y;
    let (rect, antwort) = ui.allocate_exact_size(Vec2::new(breite, hoehe), Sense::hover());
    antwort.widget_info(|| WidgetInfo::labeled(egui::Role::Label, true, text));
    ui.painter().galley(rect.min, galley, f.text_muted);
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
/// Schieber einer Karte gleich lang sind und bis an die Wertspalte reichen –
/// mit sichtbarer Schiene ([`bausteine::schieber`]).
fn schieber(
    ui: &mut Ui,
    k: &Karten,
    titel: &str,
    wert: f32,
    von: f32,
    bis: f32,
    dezimal: usize,
) -> Option<f32> {
    let f = k.f;
    bausteine::schieber(
        ui,
        f,
        titel,
        k.spalten.regler,
        ZEILE,
        wert,
        von,
        bis,
        dezimal,
    )
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
        let gezogen = schieber(ui, k, r.titel, r.wert, r.von, r.bis, r.dezimal);
        let mut v = gezogen.unwrap_or(r.wert);
        let feld = wert_zelle(ui, spalten.wert, &mut v, r.von, r.bis, r.suffix, r.dezimal);
        if gezogen.is_some() || feld.changed() {
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
    let zeigen = liste.len().min(k.echo_max.max(1));
    for e in liste.iter().take(zeigen) {
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
    let rest = liste.len().saturating_sub(zeigen);
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
                    .skip(zeigen)
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
        // Die vier Stufen teilen sich, was rechts von der Beschriftung übrig
        // ist – Regler- und Wertspalte zusammen, damit die Reihe am selben Rand
        // endet wie die Zeilen mit Regler. Gleiche Stücke; passt das längste
        // Wort nicht in sein Stück, wird die Schrift kleiner.
        let breite = spalten.regler + thema::SPACE_1 + spalten.wert;
        let zelle =
            (breite - (NR_STUFEN.len() as f32 - 1.0) * thema::SPACE_1) / NR_STUFEN.len() as f32;
        let schrift = stufen_schrift(ui, zelle);
        for (i, stufe) in NR_STUFEN.iter().enumerate() {
            let gewaehlt = k.fw.nr_stufe as usize == i;
            let groesse = Vec2::new(zelle, ZEILE - thema::SPACE_1);
            if bausteine::schalter_schrift(ui, f, stufe, gewaehlt, Art::Akzent, groesse, schrift)
                .clicked()
                && !gewaehlt
            {
                a.push(setzen(k.ziel, "nr_stufe", i as f32));
            }
        }
    });
}

/// Die Schriftgröße der Stufenknöpfe: so groß wie üblich, wenn das längste Wort
/// in seine Zelle passt, sonst kleiner – aber nicht unter
/// [`logik::SKALA_SCHRIFT_MIN`], denn lesbar muss es bleiben.
fn stufen_schrift(ui: &Ui, zelle: f32) -> f32 {
    let font = egui::FontId::new(thema::FONT_SIZE_SM, thema::fett());
    let breiteste = NR_STUFEN
        .iter()
        .map(|wort| {
            ui.painter()
                .layout_no_wrap(wort.to_string(), font.clone(), egui::Color32::WHITE)
                .size()
                .x
        })
        .fold(0.0_f32, f32::max);
    if breiteste <= 0.0 {
        return thema::FONT_SIZE_SM;
    }
    let platz = (zelle - 2.0 * thema::SPACE_1).max(thema::FONT_SIZE_SM);
    (thema::FONT_SIZE_SM * platz / breiteste).clamp(logik::SKALA_SCHRIFT_MIN, thema::FONT_SIZE_SM)
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
        let gezogen = schieber(ui, k, "Decke", fw.limiter_decke, -12.0, 0.0, 1);
        let mut v = gezogen.unwrap_or(fw.limiter_decke);
        let feld = wert_zelle(ui, spalten.wert, &mut v, -12.0, 0.0, " dBFS", 1);
        if gezogen.is_some() || feld.changed() {
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
        egui::ComboBox::from_id_salt(("duck_ziel", Pegel::index(k.ziel)))
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
/// sind (`Eintrag::echo_kandidat`, vom Kern gesetzt), dazu gemerkte, die fehlen.
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
                .filter(|e| e.echo_kandidat || gewaehlt(&e.name))
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
            echo_kandidat: true,
        });
    }
    liste
}

/// Zeilen der ECHO-Karte: eine je Gerät, dazu der Hinweis, wenn nicht alle
/// Platz haben. `k.echo_max` ist die Zahl der Geräte, die in dieses Fenster
/// passen – zeigt die Karte weniger als der Bestand hergibt, steht der Rest im
/// Hinweis.
fn echo_zeilen(k: &Karten) -> usize {
    let n = ausgabegeraete(k.o).len();
    let zeigen = n.clamp(1, ECHO_ZEILEN).min(k.echo_max.max(1));
    zeigen + usize::from(n > zeigen)
}
