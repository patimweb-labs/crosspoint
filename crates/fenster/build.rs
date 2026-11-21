//! Erzeugt `tokens.rs` aus den Token-Dateien (AGENTS.md §2.6).
//!
//! Die Werte stehen in `tokens/*.tokens.json`: Grundwerte in
//! `primitives.tokens.json`, je Theme (dark, light, mono) die Zuordnung auf
//! semantische Namen. Im Code stehen danach nur Namen wie `f.text_default`
//! oder `SIZE_FADER_BAHN` — nie ein Farbwert. Übernommen aus sprechwerk.

use serde_json::Value;
use std::collections::BTreeMap;
use std::fmt::Write;

fn flach(wert: &Value, pfad: &str, aus: &mut BTreeMap<String, String>) {
    if let Some(objekt) = wert.as_object() {
        if let Some(v) = objekt.get("$value") {
            aus.insert(
                pfad.to_string(),
                v.as_str().map(str::to_string).unwrap_or(v.to_string()),
            );
            return;
        }
        for (k, v) in objekt {
            if k.starts_with('$') {
                continue;
            }
            let neu = if pfad.is_empty() {
                k.clone()
            } else {
                format!("{pfad}.{k}")
            };
            flach(v, &neu, aus);
        }
    }
}

fn lesen(datei: &str) -> BTreeMap<String, String> {
    println!("cargo:rerun-if-changed=tokens/{datei}");
    let text = std::fs::read_to_string(format!("tokens/{datei}")).expect("Token-Datei");
    let json: Value = serde_json::from_str(&text).expect("Token-JSON");
    let mut aus = BTreeMap::new();
    flach(&json, "", &mut aus);
    aus
}

/// `{color.neutral.900}` → Wert aus den Grundwerten.
fn aufloesen(wert: &str, grund: &BTreeMap<String, String>) -> String {
    match wert.strip_prefix('{').and_then(|w| w.strip_suffix('}')) {
        Some(verweis) => aufloesen(grund.get(verweis).expect("Verweis auflösbar"), grund),
        None => wert.to_string(),
    }
}

fn farbe(hex: &str) -> String {
    let h = hex.trim_start_matches('#');
    assert_eq!(h.len(), 6, "Farbe als #RRGGBB: {hex}");
    let r = u8::from_str_radix(&h[0..2], 16).unwrap();
    let g = u8::from_str_radix(&h[2..4], 16).unwrap();
    let b = u8::from_str_radix(&h[4..6], 16).unwrap();
    format!("Color32::from_rgb({r}, {g}, {b})")
}

fn zahl(wert: &str) -> String {
    let w = wert.trim_end_matches("px").trim_end_matches("ms");
    let f: f32 = w.parse().unwrap_or_else(|_| panic!("Zahl: {wert}"));
    format!("{f:?}")
}

fn bezeichner(pfad: &str) -> String {
    pfad.replace(['.', '-'], "_")
}

fn main() {
    let grund = lesen("primitives.tokens.json");
    let mut rs = String::from(
        "// Erzeugt von build.rs aus tokens/*.tokens.json — nicht von Hand ändern.\n\
         use egui::Color32;\n\n\
         /// Die semantischen Farben eines Themes.\n\
         #[derive(Debug, Clone, Copy, PartialEq)]\npub struct Farben {\n",
    );
    let dunkel = lesen("theme-dark.tokens.json");
    let namen: Vec<String> = dunkel
        .keys()
        .filter(|k| k.starts_with("color."))
        .cloned()
        .collect();
    for n in &namen {
        writeln!(
            rs,
            "    pub {}: Color32,",
            bezeichner(n.trim_start_matches("color."))
        )
        .unwrap();
    }
    rs.push_str("}\n\n");
    for (datei, konst) in [
        ("theme-dark.tokens.json", "DUNKEL"),
        ("theme-light.tokens.json", "HELL"),
        ("theme-mono.tokens.json", "MONO"),
    ] {
        let thema = lesen(datei);
        writeln!(rs, "pub const {konst}: Farben = Farben {{").unwrap();
        for n in &namen {
            let roh = thema.get(n).unwrap_or_else(|| panic!("{datei}: {n} fehlt"));
            writeln!(
                rs,
                "    {}: {},",
                bezeichner(n.trim_start_matches("color.")),
                farbe(&aufloesen(roh, &grund))
            )
            .unwrap();
        }
        rs.push_str("};\n\n");
    }
    // Maße: Abstände, Größen, Schriftgrößen, Dauer — als f32-Konstanten.
    for (pfad, wert) in &grund {
        let gruppe = pfad.split('.').next().unwrap_or("");
        let groesse = pfad.starts_with("font.size.") || pfad.starts_with("font.line.");
        if ["space", "size", "duration", "radius"].contains(&gruppe) || groesse {
            writeln!(
                rs,
                "pub const {}: f32 = {};",
                bezeichner(pfad).to_uppercase(),
                zahl(wert)
            )
            .unwrap();
        }
    }
    let ziel = std::path::Path::new(&std::env::var("OUT_DIR").unwrap()).join("tokens.rs");
    std::fs::write(ziel, rs).unwrap();
}
