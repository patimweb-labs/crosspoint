//! Geräte in der Oberfläche: Beschriftungen, Kurznamen, Auswahltexte.
//!
//! Was ein Gerät ist und wie es zugeordnet wird, weiß der Kern
//! ([`crosspoint_schnittstelle::geraete`], ADR 0012). Hier steht nur, wie es im
//! Pult aussieht: der Kurzname für die schmale Spalte, der Zusatz „nicht
//! verbunden“, die Lautstärke in der Busspalte.
//!
//! Ein gemerktes Gerät, das gerade nicht angeschlossen ist, bleibt in der Liste
//! stehen und wird als „nicht verbunden“ geführt. Verworfen wird nichts: die
//! Zuordnung ist eine Absicht, kein Ist-Zustand.

pub use crosspoint_schnittstelle::geraete::{
    auswahl_anwenden, zuordnung_gleich, Bestand, Eintrag, Lautstaerke, Ziel, Zuordner, FEHLT_TIPP,
    KEIN_AUSGANG, KEIN_EINGANG, NICHT_VERBUNDEN,
};
/// Kurzform eines technischen Namens fuer die schmale Spalte.
///
/// `alsa_input.usb-Sennheiser_Sennheiser_Profile_0226…-00.mono-fallback` wird zu
/// `Sennheiser_Sennheiser_Profile_0226…` – in 65 px Spaltenbreite ist das der
/// Teil, der das Geraet erkennbar macht. Der vollstaendige Name steht im Tooltip.
pub fn kurzname(name: &str) -> String {
    let ohne_klasse = [
        "alsa_input.usb-",
        "alsa_output.usb-",
        "alsa_input.pci-",
        "alsa_output.pci-",
        "alsa_input.",
        "alsa_output.",
    ]
    .iter()
    .find_map(|p| name.strip_prefix(p))
    .unwrap_or(name);
    ohne_klasse
        .split('.')
        .next()
        .unwrap_or(ohne_klasse)
        .to_string()
}

/// Anzeigename eines Eintrags im Aufklappmenü.
///
/// Ein fehlendes Gerät wird **nicht** stillschweigend weggelassen: es steht
/// mit Zusatz in der Liste, damit sichtbar bleibt, wohin der Streifen zeigt.
pub fn eintrag_text(anzeige: &str, fehlt: bool) -> String {
    if fehlt {
        format!("{anzeige} {NICHT_VERBUNDEN}")
    } else {
        anzeige.to_string()
    }
}

/// Beschriftung der Busspalte: `A1 · 45 %`, `A1 · STUMM`, `A1 · FEHLT`.
///
/// Die Gerätelautstärke steht bewusst neben dem Busnamen: bei einem USB-Headset
/// ohne ALSA-Mixer ist sie die einzige Hör-Schraube, und PipeWire stellt beim
/// Neustart seinen eigenen Wert wieder her.
pub fn bus_lautstaerke(bus: &str, lautstaerke: Option<Lautstaerke>, fehlt: bool) -> String {
    let wert = if fehlt {
        "FEHLT".to_string()
    } else {
        match lautstaerke {
            Some(l) => l.text(),
            None => "—".to_string(),
        }
    };
    format!("{bus} · {wert}")
}

/// Ist die Beschriftung ein Hinweis auf ein fehlendes Geraet?
pub fn bus_fehlt_text(bus: &str) -> String {
    format!("{bus} · FEHLT")
}
