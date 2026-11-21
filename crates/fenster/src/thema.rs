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
        }
    }

    #[test]
    fn eckig_wie_die_marke() {
        assert_eq!(RADIUS_MD, 0.0);
        const { assert!(SPACE_2 > 0.0 && SPACE_4 > SPACE_2) };
    }
}
