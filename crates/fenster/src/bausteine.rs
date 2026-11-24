//! Gezeichnete Bausteine des Pults: Faderblock (Skala, Bahn, Griff, Pegel,
//! Werte), Pan-Drehregler, Schalter, Kopfknöpfe, Chips, Segmentleiste.
//!
//! Jeder Baustein liest seine Farben aus dem Theme ([`Farben`]) und seine
//! Maße aus den Tokens – kein Farbwert, keine Pixelzahl ohne Namen. Werte
//! kommen herein, eine Änderung geht als Rückgabe hinaus; gespeichert wird
//! hier nichts.

use egui::{
    Align2, Color32, CornerRadius, FontId, Pos2, Rect, Response, Sense, Stroke, StrokeKind, Ui,
    Vec2, WidgetInfo,
};

use crate::fmt::{self, fader_pos, pos_db, DB_MAX, DB_MIN};
use crate::logik::{self, segment_stufe, Stufe, PEGEL_SEGMENTE, SKALA};
use crate::thema::{self, Farben, Thema};

fn mono(groesse: f32) -> FontId {
    FontId::monospace(groesse)
}

fn fett(groesse: f32) -> FontId {
    FontId::new(groesse, thema::fett())
}

/// Farbe mit Deckkraft.
pub fn alpha(c: Color32, a: f32) -> Color32 {
    c.gamma_multiply(a)
}

/// Rahmen eines Bereichs (Kopf, Tape, Zug, Makros).
pub fn rahmen(f: &Farben, flaeche: Color32) -> egui::Frame {
    egui::Frame::new()
        .fill(flaeche)
        .stroke(Stroke::new(thema::SIZE_BORDER, f.border_subtle))
        .corner_radius(CornerRadius::same(thema::RADIUS_MD as u8))
}

// -------------------------------------------------------------- Dialoge ----

/// Zugänglicher Name des Schließen-X. In jedem Dialog derselbe: Screenreader,
/// Tooltip und Tests sollen nicht drei Namen kennen müssen.
pub const SCHLIESSEN: &str = "SCHLIESSEN";

/// Innenabstand eines Dialogs: vom Rahmen zur Titelleiste und zum Inhalt.
pub const DIALOG_RAND: f32 = thema::SPACE_3;

/// Höhe der Titelleiste eines Dialogs.
pub const DIALOG_KOPF: f32 = thema::SIZE_CONTROL_MD;

/// Ein Dialog: was sein Inhalt zurückgibt, und ob er geschlossen werden soll.
pub struct DialogAntwort<R> {
    pub inhalt: R,
    /// Schließen-X, Escape oder ein Klick neben den Dialog.
    pub zu: bool,
}

/// Der gemeinsame Rahmen aller Dialoge über dem Pult (FILTER, PRESETS, VBAN,
/// MIDI, DARSTELLUNG): Titelleiste mit dem Titel links und dem Schließen-X
/// rechts, darunter der Inhalt. Rand, Hintergrund, Innenabstand und die Wege
/// zum Schließen (X, Escape, Klick daneben) sind überall dieselben, weil es
/// überall derselbe Baustein ist.
///
/// Der Titel ist der zugängliche Name des Dialogs. Den Inhalt zeichnet der
/// Aufrufer; er bekommt eine Spalte von `breite` und gibt zurück, was er an
/// Aktionen gesammelt hat.
pub fn dialog_rahmen<R>(
    ctx: &egui::Context,
    f: &Farben,
    id: egui::Id,
    titel: &str,
    breite: f32,
    inhalt: impl FnOnce(&mut Ui) -> R,
) -> DialogAntwort<R> {
    let mut schliessen = false;
    let antwort = egui::Modal::new(id)
        .frame(rahmen(f, f.bg_panel).inner_margin(egui::Margin::same(DIALOG_RAND as i8)))
        .show(ctx, |ui| {
            ui.set_width(breite);
            ui.spacing_mut().item_spacing = Vec2::splat(thema::SPACE_1);
            titelleiste(ui, f, titel, &mut schliessen);
            // Die Fläche des Dialogs als eigener Knoten: die Tests prüfen
            // daran, dass der Inhalt im Fenster und mit Abstand zum Rand steht,
            // und ein Screenreader liest den Dialog als Ganzes.
            let inhalt = ui.scope_builder(
                egui::UiBuilder::new()
                    .sense(egui::Sense::hover())
                    .id_salt(("dialoginhalt", titel)),
                inhalt,
            );
            inhalt.response.widget_info(|| {
                WidgetInfo::labeled(egui::Role::Group, true, format!("Dialog {titel}"))
            });
            inhalt.inner
        });
    let zu = antwort.should_close() || schliessen;
    DialogAntwort {
        inhalt: antwort.inner,
        zu,
    }
}

/// Die Titelleiste: Titel links in derselben Schrift wie die übrigen
/// Überschriften, das Schließen-X als Icon rechts.
fn titelleiste(ui: &mut Ui, f: &Farben, titel: &str, schliessen: &mut bool) {
    let breite = ui.available_width();
    ui.allocate_ui_with_layout(
        Vec2::new(breite, DIALOG_KOPF),
        egui::Layout::left_to_right(egui::Align::Center),
        |ui| {
            ui.add(
                egui::Label::new(
                    egui::RichText::new(titel)
                        .font(fett(thema::FONT_SIZE_MD))
                        .color(f.text_default),
                )
                .truncate(),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if icon_knopf(ui, f, Icon::Schliessen, SCHLIESSEN, false).clicked() {
                    *schliessen = true;
                }
            });
        },
    );
    ui.separator();
}

/// Kürzt einen Text so, dass er einzeilig in `breite` passt (mit „…“).
/// Gekürzt wird nur die Anzeige; der volle Text gehört in den Tooltip.
/// `font` ist die Schrift, in der der Text wirklich gezeichnet wird – sonst
/// schätzt die Messung daneben und der Text läuft über den Rand.
pub fn kuerzen(ui: &Ui, text: &str, breite: f32, font: FontId) -> String {
    let passt = |t: &str| {
        ui.painter()
            .layout_no_wrap(t.to_string(), font.clone(), Color32::WHITE)
            .size()
            .x
            <= breite
    };
    if breite <= 0.0 {
        return "…".into();
    }
    if passt(text) {
        return text.to_string();
    }
    let zeichen: Vec<char> = text.chars().collect();
    let mut n = zeichen.len();
    while n > 0 {
        let kandidat = format!("{}…", zeichen[..n].iter().collect::<String>());
        if passt(&kandidat) {
            return kandidat;
        }
        n -= 1;
    }
    "…".into()
}

// ------------------------------------------------------------ Faderblock ----

/// Ergebnis einer Faderbedienung.
pub struct FaderAntwort {
    pub response: Response,
    /// Neuer Wert in dB, falls bewegt.
    pub neu: Option<f32>,
}

/// Faderblock eines Zugs: Skala links, Bahn mit Füllung und Griff, Pegel
/// rechts, darunter Verstärkung und Pegel als Zahl.
///
/// `links_frei`: so breit bleibt links neben Skala und Bahn Platz für andere
/// Bedienelemente (die Zuordnung A1..B3 am Kanal). Die Zahlenzeile darunter
/// nutzt trotzdem die ganze Breite.
///
/// Bedienung: ziehen, Mausrad (±0,5 dB), Doppelklick → 0 dB, Pfeiltasten bei
/// Fokus (±0,5 dB, mit Umschalt ±3 dB). Für Screenreader ist er ein Regler
/// mit Namen und Wert.
#[allow(clippy::too_many_arguments)]
pub fn fader(
    ui: &mut Ui,
    f: &Farben,
    name: &str,
    groesse: Vec2,
    links_frei: f32,
    gain_db: f32,
    pegel_db: f32,
    spitze_db: f32,
) -> FaderAntwort {
    let (rect, platz) = ui.allocate_exact_size(groesse, Sense::hover());
    // Bedient wird nur der Teil rechts vom freien Streifen: dort liegen
    // Skala, Bahn und Pegel. Was links steht, bekommt seine Klicks selbst.
    let bedien = Rect::from_min_max(Pos2::new(rect.left() + links_frei, rect.top()), rect.max);
    let response = ui.interact(bedien, platz.id.with("fader"), Sense::click_and_drag());
    let pad = thema::SPACE_1;
    let wert_h = thema::FONT_SIZE_SM + thema::SPACE_1;
    let top = rect.top() + pad + thema::SPACE_2;
    let len = logik::fader_laenge(rect.height());
    let bahn = thema::SIZE_FADER_BAHN;
    // Von rechts: Pegelsäule, Bahn, Skalenstrich, Zahlen – die Bahn sitzt so
    // weit rechts, dass die Skala links den ganzen Platz bekommt. Schmale
    // Züge (Busse) verlieren so keine Zahlen.
    let meter_w = thema::SIZE_PEGEL_BREITE;
    let meter = Rect::from_min_size(
        Pos2::new(rect.right() - pad - meter_w, top),
        Vec2::new(meter_w, len),
    );
    let x_bahn = meter.left() - thema::SPACE_1 - bahn;

    // Bedienung
    let mut neu = None;
    let id = response.id;
    if response.drag_started() {
        ui.data_mut(|d| d.insert_temp(id, (gain_db, 0.0f32)));
    }
    if response.dragged() {
        let (start, weg) = ui
            .data(|d| d.get_temp::<(f32, f32)>(id))
            .unwrap_or((gain_db, 0.0));
        let weg = weg + response.drag_delta().y;
        ui.data_mut(|d| d.insert_temp(id, (start, weg)));
        let w = logik::fader_ziehen(start, weg, len, gain_db);
        if w != gain_db {
            neu = Some(w);
        }
    }
    if response.double_clicked() {
        neu = Some(0.0);
    }
    if response.hovered() {
        let rad = ui.input(|i| i.smooth_scroll_delta.y);
        if rad.abs() > 0.5 {
            neu = Some((gain_db + rad.signum() * 0.5).clamp(DB_MIN, DB_MAX));
        }
    }
    if response.has_focus() {
        let (hoch, runter, gross) = ui.input(|i| {
            (
                i.key_pressed(egui::Key::ArrowUp),
                i.key_pressed(egui::Key::ArrowDown),
                i.modifiers.shift,
            )
        });
        let schritt = if gross { 3.0 } else { 0.5 };
        if hoch {
            neu = Some((gain_db + schritt).min(DB_MAX));
        } else if runter {
            neu = Some((gain_db - schritt).max(DB_MIN));
        }
    }
    let gain = neu.unwrap_or(gain_db);
    // Fader: senkrecht ziehen (wie in der GTK-Oberfläche).
    let response = response.on_hover_cursor(egui::CursorIcon::ResizeVertical);
    response.widget_info(|| WidgetInfo::slider(true, gain as f64, format!("{name} Fader dB")));

    let p = ui.painter_at(rect);
    let ecke = CornerRadius::same(thema::RADIUS_MD as u8);
    p.rect_filled(rect, ecke, alpha(f.bg_canvas, 0.75));

    // Bahn und Füllung unterhalb des Griffs
    let bahn_rect = Rect::from_min_size(Pos2::new(x_bahn, top), Vec2::new(bahn, len));
    p.rect_filled(bahn_rect, ecke, alpha(f.border_subtle, 0.55));
    p.rect_filled(bahn_rect.shrink(1.0), ecke, alpha(f.bg_canvas, 0.9));
    let gy = top + (1.0 - fader_pos(gain)) * len;
    if gy < top + len - 2.0 {
        let fuellung = Rect::from_min_max(
            Pos2::new(x_bahn + 1.0, gy + 2.0),
            Pos2::new(x_bahn + bahn - 1.0, top + len - 1.0),
        );
        p.rect_filled(fuellung, ecke, f.accent_deep);
    }

    // Skala: alle neun Zahlen, nie ausgedünnt. Reicht die Breite nicht für die
    // kleine Schrift, wird sie kleiner (`logik::fader_skala_schrift`).
    let skala_rechts = x_bahn - thema::SPACE_1;
    let fs = logik::fader_skala_schrift(bedien.width());
    for (db, text) in SKALA {
        let y = top + (1.0 - fader_pos(db)) * len;
        let haupt = db == 0.0;
        let farbe = if haupt { f.text_default } else { f.text_muted };
        let strich = if haupt { logik::SKALA_STRICH } else { 3.0 };
        p.line_segment(
            [
                Pos2::new(skala_rechts, y),
                Pos2::new(skala_rechts + strich, y),
            ],
            Stroke::new(
                if haupt { 1.5 } else { 1.0 },
                alpha(farbe, if haupt { 0.9 } else { 0.55 }),
            ),
        );
        p.text(
            Pos2::new(skala_rechts - thema::SPACE_1, y),
            Align2::RIGHT_CENTER,
            text,
            mono(fs),
            alpha(farbe, if haupt { 0.95 } else { 0.6 }),
        );
    }

    // Pegel rechts
    pegelsaeule(&p, f, meter, pegel_db, spitze_db);

    // Griff
    let griff_w = bahn + thema::SPACE_3;
    let griff_h = thema::SPACE_3 + 1.0;
    let griff = Rect::from_center_size(
        Pos2::new(x_bahn + bahn * 0.5, gy),
        Vec2::new(griff_w, griff_h),
    );
    p.rect_filled(griff, ecke, f.bg_raised);
    let rand = if response.hovered() || response.dragged() || response.has_focus() {
        f.focus_ring
    } else {
        f.border_strong
    };
    p.rect_stroke(
        griff,
        ecke,
        Stroke::new(thema::SIZE_BORDER, rand),
        StrokeKind::Inside,
    );
    p.line_segment(
        [
            Pos2::new(griff.left() + 1.0, gy),
            Pos2::new(griff.right() - 1.0, gy),
        ],
        Stroke::new(1.5, f.accent_default),
    );

    // Werte: Verstärkung | Pegel
    let unten = rect.bottom() - pad - wert_h;
    let feld_w = (rect.width() - pad * 2.0 - 1.0) * 0.5;
    for (i, (text, farbe)) in [
        (fmt::gain_text(gain), f.text_default),
        (fmt::db_text(pegel_db), f.accent_default),
    ]
    .into_iter()
    .enumerate()
    {
        let feld = Rect::from_min_size(
            Pos2::new(rect.left() + pad + i as f32 * (feld_w + 1.0), unten),
            Vec2::new(feld_w, wert_h),
        );
        p.rect_filled(feld, ecke, alpha(f.bg_canvas, 0.85));
        p.text(
            feld.center(),
            Align2::CENTER_CENTER,
            text,
            mono(thema::FONT_SIZE_SM),
            farbe,
        );
    }
    if response.has_focus() {
        p.rect_stroke(
            rect,
            ecke,
            Stroke::new(thema::SIZE_FOCUS, f.focus_ring),
            StrokeKind::Inside,
        );
    }

    FaderAntwort { response, neu }
}

/// Ein waagerechter Regler für die Zeilen im FILTER-Modal.
///
/// Die **ganze** Schiene ist zu sehen – von der Beschriftungsspalte bis an die
/// Wertspalte –, der gefüllte Teil bis zum Griff steht im Akzent, der Griff
/// sitzt als Knopf darauf. Gezeichnet wie der Fader: der Standardregler von
/// egui malt seine Schiene in der Fläche, auf der er steht, und die
/// verschwindet dann in der Karte.
///
/// Bedienung: Klick oder Ziehen setzt den Wert an den Zeiger, Pfeiltasten bei
/// Fokus (mit Umschalt zehn Schritte), Mausrad fein. Für Screenreader ist er
/// ein Regler mit Namen und Wert. Gibt den neuen Wert zurück, wenn sich einer
/// ergibt – sonst `None`.
#[allow(clippy::too_many_arguments)]
pub fn schieber(
    ui: &mut Ui,
    f: &Farben,
    name: &str,
    breite: f32,
    hoehe: f32,
    wert: f32,
    von: f32,
    bis: f32,
    dezimal: usize,
) -> Option<f32> {
    let (rect, response) =
        ui.allocate_exact_size(Vec2::new(breite, hoehe), Sense::click_and_drag());
    let stellen = 10f32.powi(dezimal as i32);
    let runden = |v: f32| (v * stellen).round() / stellen;
    // Ein Schritt ist der hundertste Teil der Spanne, aber nie feiner als die
    // letzte Stelle, die die Zeile zeigt.
    let fein = (((bis - von) * 0.01) * stellen).round().max(1.0) / stellen;
    let spanne = bis - von;
    // Der Griff läuft von links nach rechts über die ganze Breite: an beiden
    // Enden steht seine Mitte, damit er nicht über die Schiene hinausragt.
    let griff = Vec2::new(
        thema::SPACE_3,
        (hoehe - 2.0 * thema::SIZE_BORDER).max(thema::SPACE_2),
    );
    let weg = (rect.width() - griff.x).max(1.0);

    let mut neu = None;
    if response.clicked() || response.dragged() {
        if let Some(zeiger) = response.interact_pointer_pos() {
            let t = ((zeiger.x - rect.left() - griff.x * 0.5) / weg).clamp(0.0, 1.0);
            neu = Some(runden(von + t * spanne));
        }
    }
    if response.has_focus() {
        // Pfeiltasten gehören dem Regler, solange er den Fokus hat – sonst
        // springt der Fokus im Modal weiter, statt den Wert zu ändern.
        ui.memory_mut(|m| {
            m.set_focus_lock_filter(
                response.id,
                egui::EventFilter {
                    horizontal_arrows: true,
                    ..Default::default()
                },
            );
        });
        let (links, rechts, weit) = ui.input(|i| {
            (
                i.key_pressed(egui::Key::ArrowLeft),
                i.key_pressed(egui::Key::ArrowRight),
                i.modifiers.shift,
            )
        });
        let schritt = if weit { fein * 10.0 } else { fein };
        if links {
            neu = Some(runden(wert - schritt));
        } else if rechts {
            neu = Some(runden(wert + schritt));
        }
    }
    if response.hovered() {
        let rad = ui.input(|i| i.smooth_scroll_delta.x + i.smooth_scroll_delta.y);
        if rad.abs() > 0.5 {
            neu = Some(runden(wert + rad.signum() * fein));
        }
    }
    if response.clicked() {
        response.request_focus();
    }
    let neu = neu.map(|v| v.clamp(von.min(bis), von.max(bis)));
    let anzeige = neu.unwrap_or(wert);
    response.widget_info(|| WidgetInfo::slider(true, anzeige as f64, name));

    let p = ui.painter_at(rect);
    let ecke = CornerRadius::same(thema::RADIUS_MD as u8);
    // Die Schiene: die ganze Spalte, nicht nur der gefüllte Teil.
    let bahn = schiene(rect);
    p.rect_filled(bahn, ecke, f.border_strong);
    let t = ((anzeige - von) / spanne).clamp(0.0, 1.0);
    let x = rect.left() + griff.x * 0.5 + t * weg;
    if x > bahn.left() + 1.0 {
        p.rect_filled(
            Rect::from_min_max(bahn.min, Pos2::new(x, bahn.max.y)),
            ecke,
            f.accent_default,
        );
    }
    // Der Griff: Fläche der Karte, Rahmen wie am Fader.
    let kopf = Rect::from_center_size(Pos2::new(x, rect.center().y), griff);
    p.rect_filled(kopf, ecke, f.bg_raised);
    let rand = if response.hovered() || response.dragged() || response.has_focus() {
        f.accent_default
    } else {
        f.text_default
    };
    p.rect_stroke(
        kopf,
        ecke,
        Stroke::new(thema::SIZE_BORDER, rand),
        StrokeKind::Inside,
    );
    if response.has_focus() {
        p.rect_stroke(
            rect,
            ecke,
            Stroke::new(thema::SIZE_FOCUS, f.focus_ring),
            StrokeKind::Inside,
        );
    }
    neu.filter(|v| *v != wert)
}

/// Schiene eines Reglers: die ganze Breite seines Rechtecks, in der Höhe von
/// [`thema::SIZE_FADER_BAHN`] mittig darin.
pub fn schiene(rect: Rect) -> Rect {
    let dick = thema::SIZE_FADER_BAHN;
    Rect::from_min_max(
        Pos2::new(rect.left(), rect.center().y - dick * 0.5),
        Pos2::new(rect.right(), rect.center().y + dick * 0.5),
    )
}

/// Gibt dem Standardregler von egui eine sichtbare Schiene: die ganze Schiene
/// in der Rahmenfarbe, der gefüllte Teil im Akzent – dieselben Farben wie
/// [`schieber`]. Für die wenigen Stellen, an denen der Regler seine Zahl
/// selbst mitbringt (TRIM · DELAY im Kopf); überall sonst gilt [`schieber`].
pub fn sichtbare_schiene(ui: &mut Ui, f: &Farben) {
    let v = ui.visuals_mut();
    v.widgets.inactive.bg_fill = f.border_strong;
    v.selection.bg_fill = f.accent_default;
    // Dieselbe Schienenhöhe wie [`schieber`] – egui hätte sonst eine eigene.
    ui.spacing_mut().slider_rail_height = thema::SIZE_FADER_BAHN;
}

/// Senkrechte Pegelsäule mit Segmenten und Spitzenmarke.
pub fn pegelsaeule(p: &egui::Painter, f: &Farben, r: Rect, db: f32, spitze: f32) {
    let ecke = CornerRadius::same(thema::RADIUS_MD as u8);
    p.rect_filled(r, ecke, alpha(f.bg_canvas, 0.95));
    let seg_h = r.height() / PEGEL_SEGMENTE as f32;
    for i in 0..PEGEL_SEGMENTE {
        let seg_db = pos_db((i as f32 + 1.0) / PEGEL_SEGMENTE as f32);
        let y = r.bottom() - (i as f32 + 1.0) * seg_h;
        let farbe = if seg_db <= db {
            // Oben am Anschlag wird das Signal bernsteinfarben – die Farbe des
            // Kreuzungspunkts im Zeichen (ADR 0008).
            match segment_stufe(seg_db) {
                Stufe::Spitze => f.signal_default,
                Stufe::Laut => alpha(f.signal_default, 0.9),
                Stufe::Normal => f.accent_default,
            }
        } else {
            alpha(f.border_subtle, 0.26)
        };
        let seg = Rect::from_min_size(
            Pos2::new(r.left() + 1.0, y + 0.6),
            Vec2::new(r.width() - 2.0, (seg_h - 1.4).max(0.8)),
        );
        p.rect_filled(seg, CornerRadius::ZERO, farbe);
    }
    if spitze > DB_MIN + 5.0 {
        let y = r.top() + (1.0 - fader_pos(spitze)) * r.height();
        p.line_segment(
            [Pos2::new(r.left() + 1.0, y), Pos2::new(r.right() - 1.0, y)],
            Stroke::new(1.5, alpha(f.signal_default, 0.9)),
        );
    }
}

// ----------------------------------------------------------- Pan-Regler ----

/// Pan-Drehregler mit Kranz (−135°..135°) und Text (`C`, `L40`, `R100`).
/// Ziehen waagerecht oder senkrecht, Doppelklick → Mitte, Mausrad, Pfeiltasten.
pub fn pan(ui: &mut Ui, f: &Farben, name: &str, breite: f32, pan: f32) -> (Response, Option<f32>) {
    let hoehe = thema::SIZE_PAN + thema::SPACE_5;
    let (rect, response) =
        ui.allocate_exact_size(Vec2::new(breite, hoehe), Sense::click_and_drag());
    let mut neu = None;
    if response.dragged() {
        let d = response.drag_delta();
        let w = (pan + (d.x - d.y) / 120.0).clamp(-1.0, 1.0);
        if w != pan {
            neu = Some(w);
        }
    }
    if response.double_clicked() {
        neu = Some(0.0);
    }
    if response.hovered() {
        let rad = ui.input(|i| i.smooth_scroll_delta.y);
        if rad.abs() > 0.5 {
            neu = Some((pan + rad.signum() * 0.05).clamp(-1.0, 1.0));
        }
    }
    if response.has_focus() {
        let (l, r) = ui.input(|i| {
            (
                i.key_pressed(egui::Key::ArrowLeft),
                i.key_pressed(egui::Key::ArrowRight),
            )
        });
        if l {
            neu = Some((pan - 0.1).max(-1.0));
        } else if r {
            neu = Some((pan + 0.1).min(1.0));
        }
    }
    let wert = neu.unwrap_or(pan);
    let response = response.on_hover_cursor(egui::CursorIcon::ResizeHorizontal);
    response.widget_info(|| WidgetInfo::slider(true, wert as f64, format!("{name} Pan")));

    let p = ui.painter_at(rect);
    p.text(
        rect.left_top() + Vec2::new(1.0, 0.0),
        Align2::LEFT_TOP,
        "PAN",
        mono(thema::FONT_SIZE_SM),
        alpha(f.text_muted, 0.85),
    );
    let radius = thema::SIZE_PAN * 0.5;
    let c = Pos2::new(
        rect.center().x - thema::SPACE_2,
        rect.bottom() - radius - thema::SPACE_1,
    );
    p.circle_filled(c, radius, f.bg_raised);
    let rand = if response.hovered() || response.has_focus() {
        f.focus_ring
    } else {
        f.border_subtle
    };
    p.circle_stroke(c, radius, Stroke::new(thema::SIZE_BORDER, rand));
    p.circle_filled(c, radius - 3.0, alpha(f.bg_canvas, 0.35));
    let zeiger = wert.clamp(-1.0, 1.0) * 135.0;
    for i in 0..=20 {
        let winkel = -135.0 + i as f32 * 13.5;
        let aktiv = if zeiger >= 0.0 {
            (0.0..=zeiger).contains(&winkel)
        } else {
            (zeiger..=0.0).contains(&winkel)
        };
        let (s, co) = winkel.to_radians().sin_cos();
        let r1 = radius + 2.0;
        let r2 = radius + if i % 5 == 0 { 4.5 } else { 3.2 };
        p.line_segment(
            [
                Pos2::new(c.x + s * r1, c.y - co * r1),
                Pos2::new(c.x + s * r2, c.y - co * r2),
            ],
            Stroke::new(
                1.0,
                if aktiv {
                    alpha(f.accent_default, 0.95)
                } else {
                    alpha(f.border_strong, 0.55)
                },
            ),
        );
    }
    let (s, co) = (wert.clamp(-1.0, 1.0) * 135.0).to_radians().sin_cos();
    p.line_segment(
        [
            Pos2::new(c.x + s * 2.0, c.y - co * 2.0),
            Pos2::new(c.x + s * (radius - 2.0), c.y - co * (radius - 2.0)),
        ],
        Stroke::new(2.0, f.accent_default),
    );
    p.text(
        Pos2::new(c.x + radius + thema::SPACE_2, c.y),
        Align2::LEFT_CENTER,
        logik::pan_text(wert),
        mono(thema::FONT_SIZE_SM),
        f.text_default,
    );
    (response, neu)
}

// ------------------------------------------------------------- Schalter ----

/// Farbe eines eingeschalteten Schalters.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Art {
    /// Akzent (M, GATE, ECHO, Filterlämpchen).
    Akzent,
    /// Zweites Signal, Bernstein: der Kreuzungspunkt der Zuordnung (ADR 0008).
    Signal,
    /// Schrift auf Grund (S).
    Solo,
    /// Dunkler Akzent (MONO).
    Tief,
}

/// Umschalter im Zug: aus = Grund mit Linie, an = gefüllt.
pub fn schalter(
    ui: &mut Ui,
    f: &Farben,
    text: &str,
    an: bool,
    art: Art,
    groesse: Vec2,
) -> Response {
    schalter_gross(ui, f, None, text, an, art, groesse, thema::FONT_SIZE_SM)
}

/// Wie [`schalter`], aber mit eigener Schriftgröße – für Reihen, in denen
/// mehrere Wörter in einer schmalen Spalte Platz haben müssen (die vier Stufen
/// der Rauschunterdrückung). Reicht es auch in kleiner Schrift nicht, wird das
/// Wort gekürzt statt über den Nachbarn zu laufen; der volle Text bleibt der
/// Name für den Screenreader.
pub fn schalter_schrift(
    ui: &mut Ui,
    f: &Farben,
    text: &str,
    an: bool,
    art: Art,
    groesse: Vec2,
    schriftgroesse: f32,
) -> Response {
    schalter_gross(ui, f, None, text, an, art, groesse, schriftgroesse)
}

/// Wie [`schalter`], aber mit einem Icon vor dem Wort (`FILTER`). Das Wort
/// bleibt der zugängliche Name, das Icon ist nur das Bild dazu.
pub fn schalter_icon(
    ui: &mut Ui,
    f: &Farben,
    icon: Option<Icon>,
    text: &str,
    an: bool,
    art: Art,
    groesse: Vec2,
) -> Response {
    schalter_gross(ui, f, icon, text, an, art, groesse, thema::FONT_SIZE_SM)
}

/// Der gemeinsame Bau aller Schalter: Fläche, Rahmen, Schrift (und Icon) –
/// nur die Schriftgröße kommt von außen, damit enge Reihen sie verkleinern
/// können.
#[allow(clippy::too_many_arguments)]
fn schalter_gross(
    ui: &mut Ui,
    f: &Farben,
    icon: Option<Icon>,
    text: &str,
    an: bool,
    art: Art,
    groesse: Vec2,
    schriftgroesse: f32,
) -> Response {
    let (rect, response) = ui.allocate_exact_size(groesse, Sense::click());
    response.widget_info(|| WidgetInfo::selected(egui::Role::CheckBox, true, an, text));
    let (flaeche, linie, schrift) = if an {
        match art {
            Art::Akzent => (f.accent_default, f.accent_default, f.text_on_accent),
            Art::Signal => (f.signal_default, f.signal_default, f.text_on_accent),
            Art::Solo => (f.text_default, f.text_default, f.bg_canvas),
            Art::Tief => (f.accent_deep, f.accent_deep, f.text_default),
        }
    } else if response.hovered() {
        (f.bg_canvas, f.accent_default, f.text_default)
    } else {
        (f.bg_canvas, f.border_subtle, f.text_muted)
    };
    let p = ui.painter().clone();
    let ecke = CornerRadius::same(thema::RADIUS_MD as u8);
    p.rect_filled(rect, ecke, flaeche);
    p.rect_stroke(
        rect,
        ecke,
        Stroke::new(thema::SIZE_BORDER, linie),
        StrokeKind::Inside,
    );
    // Platz für das Wort: die Fläche minus Rand – was nicht passt, wird
    // gekürzt.
    let font = fett(schriftgroesse);
    match icon {
        None => {
            let passend = kuerzen(ui, text, rect.width() - 2.0 * thema::SPACE_1, font.clone());
            let galley = p.layout_no_wrap(passend, font, schrift);
            p.galley(
                Pos2::new(
                    rect.center().x - galley.size().x * 0.5,
                    rect.center().y - galley.size().y * 0.5,
                ),
                galley,
                schrift,
            );
        }
        Some(icon) => {
            let galley = p.layout_no_wrap(text.to_string(), font, schrift);
            let kante = schriftgroesse * 0.8;
            let luecke = thema::SPACE_1;
            let gesamt = kante + luecke + galley.size().x;
            let links = rect.center().x - gesamt * 0.5;
            icon.bild(kante, schrift).paint_at(
                ui,
                Rect::from_center_size(
                    Pos2::new(links + kante * 0.5, rect.center().y),
                    Vec2::splat(kante),
                ),
            );
            p.galley(
                Pos2::new(
                    links + kante + luecke,
                    rect.center().y - galley.size().y * 0.5,
                ),
                galley,
                schrift,
            );
        }
    }
    if response.has_focus() {
        p.rect_stroke(
            rect.expand(1.0),
            ecke,
            Stroke::new(thema::SIZE_FOCUS, f.focus_ring),
            StrokeKind::Outside,
        );
    }
    response
}

/// Lämpchenreihe: die acht Filter eines virtuellen Busses als Kürzel, an in
/// der Akzentfarbe, aus in der gedämpften. Nur Anzeige – geschaltet wird im
/// FILTER-Modal; der zugängliche Name nennt den Bus, der Tooltip den Stand.
pub fn laempchen(
    ui: &mut Ui,
    f: &Farben,
    groesse: Vec2,
    bus: &str,
    stand: &[(&'static str, bool); 8],
    zusammenfassung: &str,
) {
    let (rect, response) = ui.allocate_exact_size(groesse, Sense::hover());
    response.widget_info(|| {
        WidgetInfo::labeled(egui::Role::Label, true, format!("Filteranzeige {bus}"))
    });
    let p = ui.painter().clone();
    // Eine Zeile, je Filter sein Anfangsbuchstabe (L E N G C A D P); aktive
    // leuchten im Akzent. Die vollen Namen stehen im Tooltip.
    let zelle = rect.width() / stand.len() as f32;
    for (i, (kurz, an)) in stand.iter().enumerate() {
        let buchstabe = crate::logik::filter_buchstabe(kurz);
        let mitte = Pos2::new(rect.left() + (i as f32 + 0.5) * zelle, rect.center().y);
        p.text(
            mitte,
            Align2::CENTER_CENTER,
            buchstabe,
            fett(thema::FONT_SIZE_SM),
            if *an { f.accent_default } else { f.text_faint },
        );
    }
    if !zusammenfassung.is_empty() {
        response.on_hover_text(zusammenfassung);
    }
}

// ----------------------------------------------------------------- Icons ----

/// Die Icons des Pultes: eine kuratierte Auswahl aus Lucide (ISC, Beilage
/// `icons/LUCIDE-LICENSE.txt`), dieselbe Sorte wie in sprechwerk. Eine Ansicht
/// kann nur verlangen, was hier steht — kein Name, der ins Leere zeigt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Icon {
    /// Theme hell
    Sonne,
    /// Theme dunkel
    Mond,
    /// Theme mono (Kontrast)
    Kontrast,
    /// Presets: Ordner
    Ordner,
    /// VBAN: Netzwerk
    Netzwerk,
    /// MIDI: Regler (Klaviatur ist bei 15 px nicht mehr zu erkennen)
    Regler,
    /// Darstellung: Palette
    Palette,
    /// Aufnahme
    Kreis,
    /// Stopp
    Quadrat,
    /// eingeklappt (weiter unten mehr)
    PfeilRunter,
    /// aufgeklappt / weiter
    PfeilRechts,
    /// Dialog schließen
    Schliessen,
    /// Eingang (Mikrofon)
    Mikrofon,
    /// Ausgang (Lautsprecher)
    Lautsprecher,
    /// läuft
    Aktivitaet,
}

impl Icon {
    /// Alle Icons — damit ein Test prüfen kann, dass jede Datei da ist.
    pub const ALLE: [Icon; 15] = [
        Icon::Sonne,
        Icon::Mond,
        Icon::Kontrast,
        Icon::Ordner,
        Icon::Netzwerk,
        Icon::Regler,
        Icon::Palette,
        Icon::Kreis,
        Icon::Quadrat,
        Icon::PfeilRunter,
        Icon::PfeilRechts,
        Icon::Schliessen,
        Icon::Mikrofon,
        Icon::Lautsprecher,
        Icon::Aktivitaet,
    ];

    /// Der Name der Datei in `icons/`.
    pub fn datei(self) -> &'static str {
        match self {
            Icon::Sonne => "sun.svg",
            Icon::Mond => "moon.svg",
            Icon::Kontrast => "contrast.svg",
            Icon::Ordner => "folder.svg",
            Icon::Netzwerk => "network.svg",
            Icon::Regler => "sliders-horizontal.svg",
            Icon::Palette => "palette.svg",
            Icon::Kreis => "circle.svg",
            Icon::Quadrat => "square.svg",
            Icon::PfeilRunter => "chevron-down.svg",
            Icon::PfeilRechts => "chevron-right.svg",
            Icon::Schliessen => "x.svg",
            Icon::Mikrofon => "mic.svg",
            Icon::Lautsprecher => "volume-2.svg",
            Icon::Aktivitaet => "activity.svg",
        }
    }

    /// Das Icon als Bild, eingefärbt mit `farbe` (Strich, keine Fläche).
    pub fn quelle(self) -> egui::ImageSource<'static> {
        match self {
            Icon::Sonne => egui::include_image!("../icons/sun.svg"),
            Icon::Mond => egui::include_image!("../icons/moon.svg"),
            Icon::Kontrast => egui::include_image!("../icons/contrast.svg"),
            Icon::Ordner => egui::include_image!("../icons/folder.svg"),
            Icon::Netzwerk => egui::include_image!("../icons/network.svg"),
            Icon::Regler => egui::include_image!("../icons/sliders-horizontal.svg"),
            Icon::Palette => egui::include_image!("../icons/palette.svg"),
            Icon::Kreis => egui::include_image!("../icons/circle.svg"),
            Icon::Quadrat => egui::include_image!("../icons/square.svg"),
            Icon::PfeilRunter => egui::include_image!("../icons/chevron-down.svg"),
            Icon::PfeilRechts => egui::include_image!("../icons/chevron-right.svg"),
            Icon::Schliessen => egui::include_image!("../icons/x.svg"),
            Icon::Mikrofon => egui::include_image!("../icons/mic.svg"),
            Icon::Lautsprecher => egui::include_image!("../icons/volume-2.svg"),
            Icon::Aktivitaet => egui::include_image!("../icons/activity.svg"),
        }
    }

    /// Das Icon als Bild in Größe und Farbe.
    pub fn bild(self, groesse: f32, farbe: Color32) -> egui::Image<'static> {
        egui::Image::new(self.quelle())
            .tint(farbe)
            .fit_to_exact_size(Vec2::splat(groesse))
    }

    /// Malt das Icon mittig in ein Quadrat der Kante `groesse` — für alles,
    /// was selbst zeichnet (Kopfknopf, Statuszeile, Gerätewahl).
    pub fn malen(self, ui: &Ui, mitte: Pos2, groesse: f32, farbe: Color32) {
        self.bild(groesse, farbe)
            .paint_at(ui, Rect::from_center_size(mitte, Vec2::splat(groesse)));
    }
}

/// Ein Icon ohne Knopf (Gerätewahl, Zeile im ECHO-Menü).
pub fn icon(ui: &mut Ui, icon: Icon, farbe: Color32) -> Response {
    ui.add(icon.bild(thema::FONT_SIZE_MD, farbe))
}

/// Das Icon eines Themes: Sonne (hell), Mond (dunkel), Kontrast (mono).
pub fn thema_icon(t: Thema) -> Icon {
    match t {
        Thema::Hell => Icon::Sonne,
        Thema::Dunkel => Icon::Mond,
        Thema::Mono => Icon::Kontrast,
    }
}

/// Knopf nur mit Icon. `name` ist Tooltip **und** zugänglicher Name — ein
/// Screenreader liest ihn vor, die Maus zeigt ihn beim Darüberfahren.
///
/// Im Pult steht das Wort noch neben jedem Icon (die Beschriftung ist der
/// Name); der Baustein liegt bereit, sobald eine Ansicht ohne Text auskommen
/// muss, und setzt dann den Namen selbst.
pub fn icon_knopf(ui: &mut Ui, f: &Farben, icon: Icon, name: &str, an: bool) -> Response {
    let farbe = if an { f.accent_default } else { f.text_muted };
    let antwort = ui
        .add(
            egui::Button::image(icon.bild(thema::FONT_SIZE_MD, farbe))
                .selected(an)
                .frame_when_inactive(an)
                .min_size(Vec2::splat(thema::SIZE_CONTROL_SM)),
        )
        .on_hover_text(name);
    let name = name.to_string();
    antwort.widget_info(|| WidgetInfo::labeled(egui::Role::Button, true, &name));
    antwort
}

// ------------------------------------------------------ Gezeichnete Zeichen ----

/// Gezeichnetes Zeichen (IBM Plex Mono hat keine geometrischen Formen).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Zeichen {
    /// ● Aufnahme, Status
    Punkt,
    /// ■ Stopp, Marke
    Quadrat,
    /// ▾ eingeklappt umschalten
    Runter,
    /// ▸ aufklappen
    Rechts,
}

impl Zeichen {
    /// Führendes Zeichen eines Textes abtrennen: `"● REC"` → (Punkt, `"REC"`).
    pub fn aus_text(text: &str) -> (Option<Zeichen>, &str) {
        let mut z = text.chars();
        let zeichen = match z.next() {
            Some('●') => Zeichen::Punkt,
            Some('■') | Some('▪') => Zeichen::Quadrat,
            Some('▾') => Zeichen::Runter,
            Some('▸') => Zeichen::Rechts,
            _ => return (None, text),
        };
        (Some(zeichen), z.as_str().trim_start())
    }

    /// Zeichnet das Zeichen mittig in ein Quadrat der Kante `kante`.
    pub fn malen(self, p: &egui::Painter, mitte: Pos2, kante: f32, farbe: Color32) {
        let h = kante * 0.5;
        match self {
            Zeichen::Punkt => {
                p.circle_filled(mitte, h * 0.8, farbe);
            }
            Zeichen::Quadrat => {
                p.rect_filled(
                    Rect::from_center_size(mitte, Vec2::splat(kante * 0.7)),
                    CornerRadius::ZERO,
                    farbe,
                );
            }
            Zeichen::Runter => {
                p.add(egui::Shape::convex_polygon(
                    vec![
                        mitte + Vec2::new(-h, -h * 0.5),
                        mitte + Vec2::new(h, -h * 0.5),
                        mitte + Vec2::new(0.0, h * 0.6),
                    ],
                    farbe,
                    Stroke::NONE,
                ));
            }
            Zeichen::Rechts => {
                p.add(egui::Shape::convex_polygon(
                    vec![
                        mitte + Vec2::new(-h * 0.5, -h),
                        mitte + Vec2::new(h * 0.6, 0.0),
                        mitte + Vec2::new(-h * 0.5, h),
                    ],
                    farbe,
                    Stroke::NONE,
                ));
            }
        }
    }
}

/// Text mit Icon davor (`LAEUFT`, `BEREIT`).
pub fn icon_text(
    ui: &mut Ui,
    icon: Icon,
    text: &str,
    font: FontId,
    farbe: Color32,
    icon_farbe: Color32,
) -> Response {
    let kante = font.size;
    let galley = ui.painter().layout_no_wrap(text.to_string(), font, farbe);
    let luecke = if text.is_empty() { 0.0 } else { thema::SPACE_1 };
    let groesse = Vec2::new(kante + luecke + galley.size().x, galley.size().y.max(kante));
    let (rect, r) = ui.allocate_exact_size(groesse, Sense::hover());
    r.widget_info(|| WidgetInfo::labeled(egui::Role::Label, true, text));
    icon.malen(
        ui,
        Pos2::new(rect.left() + kante * 0.5, rect.center().y),
        kante,
        icon_farbe,
    );
    ui.painter().galley(
        Pos2::new(
            rect.left() + kante + luecke,
            rect.center().y - galley.size().y * 0.5,
        ),
        galley,
        farbe,
    );
    r
}

/// Knopf der Kopf- und Tape-Leiste (`PRESETS`, `LADEN`). `an` füllt ihn.
/// Ein führendes ●/■/▾/▸ im Text wird gezeichnet, nicht gesetzt — Rückfall für
/// Zeichen, die die Schrift nicht hat; im Pult steht dort ein [`Icon`].
pub fn kopfknopf(ui: &mut Ui, f: &Farben, text: &str, an: Option<Color32>) -> Response {
    kopfknopf_inner(ui, f, None, text, an)
}

/// Derselbe Knopf mit Icon vor dem Text (`PRESETS`, `REC`). Der Text bleibt
/// der zugängliche Name, das Icon ist Beiwerk.
pub fn kopfknopf_icon(
    ui: &mut Ui,
    f: &Farben,
    icon: Icon,
    text: &str,
    an: Option<Color32>,
) -> Response {
    kopfknopf_inner(ui, f, Some(icon), text, an)
}

fn kopfknopf_inner(
    ui: &mut Ui,
    f: &Farben,
    vor: Option<Icon>,
    text: &str,
    an: Option<Color32>,
) -> Response {
    let font = fett(thema::FONT_SIZE_SM);
    let (zeichen, rest) = Zeichen::aus_text(text);
    let kante = thema::FONT_SIZE_SM * 0.6;
    // Vor dem Text: Icon, sonst gezeichnetes Zeichen, sonst nichts. Beide
    // bekommen denselben Platz, damit die Knöpfe einer Leiste ausgerichtet
    // bleiben.
    let (icon_breite, zeichen_breite) = match (vor, zeichen) {
        (Some(_), _) => (thema::FONT_SIZE_MD + thema::SPACE_1, 0.0),
        (None, Some(_)) => (0.0, kante + thema::SPACE_1),
        (None, None) => (0.0, 0.0),
    };
    let z_breite = icon_breite + zeichen_breite;
    let text_breite = ui
        .painter()
        .layout_no_wrap(rest.to_string(), font.clone(), f.text_default)
        .size()
        .x;
    let breite = z_breite + text_breite + thema::SPACE_4 + thema::SPACE_1;
    let (rect, response) =
        ui.allocate_exact_size(Vec2::new(breite, thema::SIZE_CONTROL_MD), Sense::click());
    response.widget_info(|| WidgetInfo::labeled(egui::Role::Button, true, rest));
    let (flaeche, linie, schrift) = match an {
        Some(c) => (c, c, f.text_on_accent),
        None if response.hovered() => (f.bg_canvas, f.accent_default, f.text_default),
        None => (f.bg_canvas, f.border_subtle, f.text_default),
    };
    let ecke = CornerRadius::same(thema::RADIUS_MD as u8);
    let links = rect.center().x - (z_breite + text_breite) * 0.5;
    let p = ui.painter();
    p.rect_filled(rect, ecke, flaeche);
    p.rect_stroke(
        rect,
        ecke,
        Stroke::new(thema::SIZE_BORDER, linie),
        StrokeKind::Inside,
    );
    if let Some(i) = vor {
        i.malen(
            ui,
            Pos2::new(links + thema::FONT_SIZE_MD * 0.5, rect.center().y),
            thema::FONT_SIZE_MD,
            schrift,
        );
    } else if let Some(z) = zeichen {
        z.malen(
            p,
            Pos2::new(links + kante * 0.5, rect.center().y),
            kante,
            schrift,
        );
    }
    p.text(
        Pos2::new(links + z_breite, rect.center().y),
        Align2::LEFT_CENTER,
        rest,
        font,
        schrift,
    );
    if response.has_focus() {
        p.rect_stroke(
            rect.expand(1.0),
            ecke,
            Stroke::new(thema::SIZE_FOCUS, f.focus_ring),
            StrokeKind::Outside,
        );
    }
    response
}

/// Chip in der Kopfleiste: `SR 48.0 kHz`.
pub fn chip(ui: &mut Ui, f: &Farben, schluessel: &str, wert: &str) {
    let font = mono(thema::FONT_SIZE_SM);
    let k = ui
        .painter()
        .layout_no_wrap(schluessel.to_string(), font.clone(), f.text_muted);
    let w = ui
        .painter()
        .layout_no_wrap(wert.to_string(), font, f.text_default);
    let innen = Vec2::new(k.size().x + thema::SPACE_1 + w.size().x, k.size().y);
    let (rect, r) =
        ui.allocate_exact_size(innen + Vec2::new(2.0 * thema::SPACE_2, 4.0), Sense::hover());
    r.widget_info(|| WidgetInfo::labeled(egui::Role::Label, true, format!("{schluessel} {wert}")));
    let p = ui.painter();
    let ecke = CornerRadius::same(thema::RADIUS_MD as u8);
    p.rect_filled(rect, ecke, f.bg_canvas);
    p.rect_stroke(
        rect,
        ecke,
        Stroke::new(thema::SIZE_BORDER, f.border_subtle),
        StrokeKind::Inside,
    );
    let links = Pos2::new(rect.left() + thema::SPACE_2, rect.top() + 2.0);
    let kb = k.size().x;
    p.galley(links, k, f.text_muted);
    p.galley(
        links + Vec2::new(kb + thema::SPACE_1, 0.0),
        w,
        f.text_default,
    );
}

/// Waagerechte Segmentleiste (Tape-Pegel).
pub fn segmentleiste(ui: &mut Ui, f: &Farben, db: f32, segmente: usize) {
    let seg_w = 3.0;
    let breite = segmente as f32 * (seg_w + 1.0) - 1.0;
    segmentleiste_breit(ui, f, db, breite, segmente);
}

/// Waagerechte Segmentleiste, die **genau** `breite` füllt: die Segmente
/// teilen sich die Breite, dazwischen steht eine Lücke von
/// [`thema::SIZE_BORDER`]. So reicht der Balken von der Beschriftung bis an die
/// Wertspalte, statt mitten in der Zeile aufzuhören.
pub fn segmentleiste_breit(ui: &mut Ui, f: &Farben, db: f32, breite: f32, segmente: usize) {
    let segmente = segmente.max(1);
    let hoehe = thema::SPACE_3 - 3.0;
    let (rect, _) = ui.allocate_exact_size(Vec2::new(breite, hoehe), Sense::hover());
    let luecke = thema::SIZE_BORDER;
    let seg_w = ((breite - (segmente as f32 - 1.0) * luecke) / segmente as f32).max(1.0);
    let p = ui.painter();
    for i in 0..segmente {
        let seg_db = pos_db((i as f32 + 1.0) / segmente as f32);
        let spitze = i + 3 >= segmente;
        let farbe = match (seg_db <= db, spitze) {
            (true, true) => f.signal_default,
            (true, false) => f.accent_default,
            _ => f.border_subtle,
        };
        let x = rect.left() + i as f32 * (seg_w + luecke);
        p.rect_filled(
            Rect::from_min_size(Pos2::new(x, rect.top()), Vec2::new(seg_w, rect.height())),
            CornerRadius::ZERO,
            farbe,
        );
    }
}

/// Kleiner Kopftext (`8×8 DIGITAL MIXER`).
pub fn text(f: &Farben, inhalt: &str, fett_: bool, gedaempft: bool) -> egui::RichText {
    let font = if fett_ {
        fett(thema::FONT_SIZE_SM)
    } else {
        mono(thema::FONT_SIZE_SM)
    };
    egui::RichText::new(inhalt).font(font).color(if gedaempft {
        f.text_muted
    } else {
        f.text_default
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Jedes Icon ist eine Datei im Crate, ein SVG mit derselben Strichstärke
    /// (2 px auf 24er Raster) und weißem Strich — nur so färbt `tint` es.
    #[test]
    fn jedes_icon_ist_eine_datei_und_wird_eingefaerbt() {
        let ordner = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("icons");
        for i in Icon::ALLE {
            let pfad = ordner.join(i.datei());
            let svg = std::fs::read_to_string(&pfad)
                .unwrap_or_else(|e| panic!("{}: {e}", pfad.display()));
            assert!(svg.starts_with("<svg"), "{}: kein SVG", i.datei());
            assert!(
                svg.ends_with("</svg>\n"),
                "{}: nicht abgeschlossen",
                i.datei()
            );
            assert!(
                svg.contains("stroke=\"#ffffff\""),
                "{}: Strich nicht weiß, tint wirkt nicht",
                i.datei()
            );
            assert!(
                svg.contains("stroke-width=\"2\""),
                "{}: andere Strichstärke als die übrigen",
                i.datei()
            );
            assert!(
                svg.contains("viewBox=\"0 0 24 24\""),
                "{}: anderes Raster",
                i.datei()
            );
        }
    }

    /// Die Lizenz der Icons liegt bei — ohne sie darf das Paket nicht raus.
    #[test]
    fn lizenz_der_icons_liegt_bei() {
        let text = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("icons/LUCIDE-LICENSE.txt"),
        )
        .expect("LUCIDE-LICENSE.txt");
        assert!(text.contains("Lucide"));
    }

    #[test]
    fn theme_icons_sind_sonne_mond_und_kontrast() {
        assert_eq!(thema_icon(Thema::Hell), Icon::Sonne);
        assert_eq!(thema_icon(Thema::Dunkel), Icon::Mond);
        assert_eq!(thema_icon(Thema::Mono), Icon::Kontrast);
    }
}
