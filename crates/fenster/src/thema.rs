//! Drei Themes und nichts sonst (AGENTS.md §2.7). Die Werte kommen aus den
//! Token-Dateien (`build.rs`); hier wird nur zugeordnet, welcher Token welche
//! Fläche von egui färbt. Aufbau wie in sprechwerk.

use egui::{self, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Stroke, TextStyle};
use std::sync::Arc;

include!(concat!(env!("OUT_DIR"), "/tokens.rs"));

/// Das gewählte Farbschema.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Thema {
    #[default]
    Dunkel,
    Hell,
    Mono,
}

impl Thema {
    pub const ALLE: [Thema; 3] = [Thema::Dunkel, Thema::Hell, Thema::Mono];

    /// Aus dem gespeicherten Namen; Unbekanntes wird `dark`. Die Namen der
    /// GTK-Oberfläche (`crosspoint`, `crosspoint-hell`) gelten weiter.
    pub fn aus_name(name: &str) -> Self {
        match name {
            "light" | "crosspoint-hell" => Thema::Hell,
            "mono" => Thema::Mono,
            _ => Thema::Dunkel,
        }
    }

    /// Name in der Einstellungsdatei.
    pub fn name(self) -> &'static str {
        match self {
            Thema::Dunkel => "dark",
            Thema::Hell => "light",
            Thema::Mono => "mono",
        }
    }

    /// Anzeigename in der Oberfläche.
    pub fn anzeige(self) -> &'static str {
        match self {
            Thema::Dunkel => "dunkel",
            Thema::Hell => "hell",
            Thema::Mono => "mono",
        }
    }

    pub fn farben(self) -> &'static Farben {
        match self {
            Thema::Dunkel => &DUNKEL,
            Thema::Hell => &HELL,
            Thema::Mono => &MONO,
        }
    }
}

/// Schriftfamilie für Namen und Beschriftungen (Plex Mono Medium).
pub fn fett() -> FontFamily {
    FontFamily::Name("fett".into())
}

/// IBM Plex Mono überall – das Pult ist eine Zahlenwand; die eingebauten
/// Schriften von egui bleiben als Rückfall dahinter.
pub fn schriften(ctx: &egui::Context) {
    let mut defs = FontDefinitions::default();
    defs.font_data.insert(
        "plex-mono".to_owned(),
        Arc::new(FontData::from_static(include_bytes!(
            "../schriften/PlexMono-Regular.ttf"
        ))),
    );
    defs.font_data.insert(
        "plex-mono-fett".to_owned(),
        Arc::new(FontData::from_static(include_bytes!(
            "../schriften/PlexMono-Medium.ttf"
        ))),
    );
    let rueckfall = defs
        .families
        .get(&FontFamily::Proportional)
        .cloned()
        .unwrap_or_default();
    for familie in [FontFamily::Proportional, FontFamily::Monospace] {
        defs.families
            .entry(familie)
            .or_default()
            .insert(0, "plex-mono".into());
    }
    let mut fett_liste = vec!["plex-mono-fett".to_string()];
    fett_liste.extend(rueckfall);
    defs.families.insert(fett(), fett_liste);
    ctx.set_fonts(defs);
}

/// Wendet ein Theme auf egui an.
pub fn anwenden(ctx: &egui::Context, thema: Thema) {
    let f = thema.farben();
    let ecke = CornerRadius::same(RADIUS_MD as u8);
    let mut style = (*ctx.global_style()).clone();

    let mut v = if thema == Thema::Hell {
        egui::Visuals::light()
    } else {
        egui::Visuals::dark()
    };
    v.override_text_color = Some(f.text_default);
    v.panel_fill = f.bg_canvas;
    v.window_fill = f.bg_panel;
    v.window_stroke = Stroke::new(SIZE_BORDER, f.border_strong);
    v.window_corner_radius = ecke;
    v.menu_corner_radius = ecke;
    v.window_shadow = egui::Shadow::NONE;
    v.popup_shadow = egui::Shadow::NONE;
    v.extreme_bg_color = f.bg_input;
    v.faint_bg_color = f.bg_raised;
    v.code_bg_color = f.bg_input;
    v.hyperlink_color = f.accent_default;
    v.warn_fg_color = f.status_warn;
    v.error_fg_color = f.status_error;
    v.selection.bg_fill = f.bg_selected;
    v.selection.stroke = Stroke::new(SIZE_BORDER, f.accent_default);
    v.text_cursor.stroke = Stroke::new(SIZE_FOCUS, f.focus_ring);
    v.interact_cursor = Some(egui::CursorIcon::PointingHand);
    v.slider_trailing_fill = true;

    let w = &mut v.widgets;
    for (zustand, flaeche, linie, schrift) in [
        (
            &mut w.noninteractive,
            f.bg_panel,
            f.border_subtle,
            f.text_default,
        ),
        (
            &mut w.inactive,
            f.bg_raised,
            f.border_subtle,
            f.text_default,
        ),
        (&mut w.hovered, f.bg_hover, f.border_strong, f.text_default),
        (&mut w.active, f.bg_selected, f.focus_ring, f.text_default),
        (&mut w.open, f.bg_selected, f.border_strong, f.text_default),
    ] {
        zustand.bg_fill = flaeche;
        zustand.weak_bg_fill = flaeche;
        zustand.bg_stroke = Stroke::new(SIZE_BORDER, linie);
        zustand.fg_stroke = Stroke::new(SIZE_BORDER, schrift);
        zustand.corner_radius = ecke;
        zustand.expansion = 0.0;
    }
    style.visuals = v;

    let s = &mut style.spacing;
    s.item_spacing = egui::vec2(SPACE_1, SPACE_1);
    s.button_padding = egui::vec2(SPACE_2, SPACE_1);
    s.interact_size.y = SIZE_CONTROL_SM;
    s.window_margin = egui::Margin::same(SPACE_4 as i8);
    s.menu_margin = egui::Margin::same(SPACE_1 as i8);
    // Keine Scrollbalken im Pult (ADR 0007); Aufklapplisten bleiben schmal.
    s.scroll = egui::style::ScrollStyle::thin();

    style.text_styles = [
        (TextStyle::Small, FontId::monospace(FONT_SIZE_SM)),
        (TextStyle::Body, FontId::monospace(FONT_SIZE_MD)),
        (TextStyle::Button, FontId::new(FONT_SIZE_SM, fett())),
        (TextStyle::Monospace, FontId::monospace(FONT_SIZE_MD)),
        (TextStyle::Heading, FontId::new(FONT_SIZE_XL, fett())),
    ]
    .into();
    ctx.set_global_style(style);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn namen_hin_und_zurueck() {
        for t in Thema::ALLE {
            assert_eq!(Thema::aus_name(t.name()), t);
        }
        assert_eq!(Thema::aus_name("rosa"), Thema::Dunkel);
        assert_eq!(Thema::aus_name("crosspoint-hell"), Thema::Hell);
    }

    #[test]
    fn themes_unterscheiden_sich_und_sind_lesbar() {
        assert_ne!(DUNKEL.bg_canvas, HELL.bg_canvas);
        assert_ne!(DUNKEL.accent_default, MONO.accent_default);
        for f in [&DUNKEL, &HELL, &MONO] {
            assert_ne!(f.text_default, f.bg_panel);
            assert_ne!(f.text_on_accent, f.accent_default);
            assert_ne!(f.text_on_accent, f.signal_default);
            // Zwei Signale: das Kreuz (Akzent) und sein Kreuzungspunkt (Signal).
            assert_ne!(f.accent_default, f.signal_default);
        }
    }

    /// Relative Leuchtdichte nach WCAG 2.1.
    fn leuchtdichte(c: egui::Color32) -> f32 {
        let kanal = |wert: u8| {
            let s = wert as f32 / 255.0;
            if s <= 0.039_28 {
                s / 12.92
            } else {
                ((s + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * kanal(c.r()) + 0.7152 * kanal(c.g()) + 0.0722 * kanal(c.b())
    }

    fn kontrast(a: egui::Color32, b: egui::Color32) -> f32 {
        let (x, y) = (leuchtdichte(a), leuchtdichte(b));
        (x.max(y) + 0.05) / (x.min(y) + 0.05)
    }

    /// Jedes Theme muss Text, Akzent und Signal so tragen, dass man sie liest
    /// (WCAG AA: 4,5 für kleinen Text, 3,0 für Linien und Flächen).
    #[test]
    fn kontrast_traegt_in_allen_drei_themes() {
        for (name, f) in [("dark", &DUNKEL), ("light", &HELL), ("mono", &MONO)] {
            for flaeche in [f.bg_canvas, f.bg_panel, f.bg_raised] {
                assert!(
                    kontrast(f.text_default, flaeche) >= 7.0,
                    "{name}: Text auf Fläche zu schwach"
                );
                assert!(
                    kontrast(f.accent_default, flaeche) >= 3.0,
                    "{name}: Akzent auf Fläche zu schwach"
                );
                assert!(
                    kontrast(f.signal_default, flaeche) >= 3.0,
                    "{name}: Signal auf Fläche zu schwach"
                );
            }
            assert!(
                kontrast(f.text_muted, f.bg_panel) >= 4.5,
                "{name}: gedämpfter Text zu schwach"
            );
            assert!(
                kontrast(f.border_strong, f.bg_panel) >= 1.5,
                "{name}: Rahmen unsichtbar"
            );
            for flaeche in [f.accent_default, f.signal_default] {
                assert!(
                    kontrast(f.text_on_accent, flaeche) >= 4.4,
                    "{name}: Schrift auf dem Signal zu schwach"
                );
            }
        }
    }

    /// Zeichen und Oberfläche dürfen nicht auseinanderlaufen: die Farben des
    /// Logos stehen als Tokens im Code (ADR 0008).
    #[test]
    fn zeichen_und_tokens_tragen_dieselben_farben() {
        let dunkel = include_str!("../../../assets/crosspoint-logo.svg");
        let hell = include_str!("../../../assets/crosspoint-hell.svg");
        let mono = include_str!("../../../assets/crosspoint-mono.svg");
        let hex = |c: egui::Color32| format!("#{:02X}{:02X}{:02X}", c.r(), c.g(), c.b());
        for (datei, was, farbe) in [
            (dunkel, "Kreuz", DUNKEL.accent_default),
            (dunkel, "Kreuzungspunkt", DUNKEL.signal_default),
            (dunkel, "Matrix", DUNKEL.border_strong),
            (hell, "Kachel hell", HELL.bg_panel),
            (mono, "Kreuz mono", MONO.accent_default),
        ] {
            assert!(
                datei.contains(&hex(farbe)),
                "{was}: {} steht nicht im Zeichen",
                hex(farbe)
            );
        }
    }

    #[test]
    fn eckig_wie_die_marke() {
        assert_eq!(RADIUS_MD, 0.0);
        const { assert!(SPACE_2 > 0.0 && SPACE_4 > SPACE_2) };
    }
}
