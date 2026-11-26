//! Geräte: was das Fenster über Eingänge, Ausgänge und Zuordnungen wissen darf.
//!
//! Die Typen hier sind plattformneutral. Wie die Liste entsteht (PipeWire-Graph,
//! WASAPI-Geräte, CoreAudio) entscheidet der Kern; das Fenster sieht nur
//! [`Eintrag`] und [`Bestand`].

/// Kein Eingang gewählt (Text im Aufklappmenü).
pub const KEIN_EINGANG: &str = "kein Eingang";
/// Kein Ausgang gewählt (Text im Aufklappmenü).
pub const KEIN_AUSGANG: &str = "kein Ausgang";
/// Ein Ziel ohne Gerät.
pub const NICHT_VERBUNDEN: &str = "(nicht verbunden)";
/// Zusatz an einem gemerkten, aber abgesteckten Gerät.
pub const FEHLT_TIPP: &str = "Geraet ist nicht angeschlossen (abgesteckt?)";

/// Ein Eintrag im Aufklappmenü.
#[derive(Clone, Debug, PartialEq)]
pub struct Eintrag {
    /// Technischer Name, wie ihn der Kern erwartet.
    pub name: String,
    /// Anzeigename (bei Monitoren mit Zusatz).
    pub anzeige: String,
    /// Gerät ist gerade nicht im Graphen (gemerkt, aber abgesteckt).
    pub fehlt: bool,
    /// Taugt als Echo-Referenz: lokale Ausgabe im Raum, kein Netzwerkziel und
    /// kein Gerät von Crosspoint selbst. Der Kern entscheidet das – er kennt
    /// den Graphen.
    pub echo_kandidat: bool,
}

/// Gerätelautstärke eines Ziels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Lautstaerke {
    /// Linearer Pegel, 1.0 = ohne Absenkung.
    pub volumen: f32,
    /// Stumm geschaltet (unabhängig vom Pegel).
    pub stumm: bool,
}

impl Lautstaerke {
    /// Prozentwert wie `wpctl` (kubische Wurzel, gerundet, 0..=150).
    pub fn prozent(&self) -> u32 {
        let kubisch = self.volumen.clamp(0.0, 3.375).cbrt();
        (kubisch * 100.0).round().clamp(0.0, 150.0) as u32
    }

    /// Anzeige für die Oberfläche: `45 %`, `STUMM`, `100 %`.
    pub fn text(&self) -> String {
        if self.stumm {
            "STUMM".to_string()
        } else {
            format!("{} %", self.prozent())
        }
    }
}

/// Was die Oberfläche vom Kern braucht – ein Schnappschuss, kein Zustand.
#[derive(Clone, Debug, Default)]
pub struct Bestand {
    pub quellen: Vec<Eintrag>,
    pub senken: Vec<Eintrag>,
    /// Belegung der Hardware-Streifen 1..5.
    pub streifen: Vec<Option<String>>,
    /// Je Streifen 1..5: gemerkte Quelle ist gerade nicht angeschlossen.
    pub streifen_fehlt: Vec<bool>,
    /// Belegung der Hardware-Busse A1..A5.
    pub busse: Vec<(String, Option<String>)>,
    /// Je Bus A1..A5: gemerktes Gerät ist gerade nicht angeschlossen.
    pub busse_fehlt: Vec<bool>,
    /// Je Bus A1..A5: Gerätelautstärke des Ziels (None = unbekannt).
    pub busse_lautstaerke: Vec<Option<Lautstaerke>>,
    /// Klartext der fehlenden Geräte (für die Hinweiszeile).
    pub fehlend: Vec<String>,
    /// Ausgabegeräte, deren Ton aus dem Mikrofon gerechnet wird.
    pub echo: Vec<String>,
    pub fehler: Option<String>,
}

/// Ziel einer Zuordnung.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ziel<'a> {
    Streifen(usize),
    Bus(&'a str),
}

/// Schreibpfad für Zuordnungen.
///
/// Schon vor ADR 0012 ein Trait: die Auswahllogik ist damit ohne Plattform
/// prüfbar, und ein zweiter Kern füllt nur diese vier Methoden.
pub trait Zuordner: Send + Sync {
    fn streifen(&self, strip: usize, quelle: Option<&str>) -> Result<(), String>;
    fn bus(&self, bus: &str, senke: Option<&str>) -> Result<(), String>;
    /// Gerätelautstärke setzen (`None` lässt den Wert stehen).
    fn lautstaerke(
        &self,
        senke: &str,
        volumen: Option<f32>,
        stumm: Option<bool>,
    ) -> Result<(), String>;
    /// Echo-Referenz: die vollständige Liste der Ausgabegeräte, deren Ton aus
    /// dem Mikrofon gerechnet wird.
    fn echo(&self, geraete: &[String]) -> Result<(), String>;
}

/// Was das Fenster über die Geräte des Kerns fragen darf.
pub trait Geraete: Send + Sync {
    /// Gerätebestand lesen. Darf blockieren (Graph lesen) – im Thread aufrufen.
    fn bestand(&self) -> Bestand;
    /// Der Schreibpfad für Zuordnungen.
    fn zuordner(&self) -> &dyn Zuordner;
    /// Wie ist dieses Ziel gerade zugeordnet?
    fn aktuell(&self, ziel: Ziel<'_>) -> Option<String>;
}

/// Auswahl anwenden. Ein leerer Eintrag („kein Eingang“/„kein Ausgang“) löscht
/// die Zuordnung, statt einen leeren Namen zu schreiben.
pub fn auswahl_anwenden<Z: Zuordner + ?Sized>(
    z: &Z,
    ziel: Ziel<'_>,
    anzeige: &str,
) -> Result<(), String> {
    let text = anzeige.trim();
    let wert = if text.is_empty() || text == KEIN_EINGANG || text == KEIN_AUSGANG {
        None
    } else {
        Some(text)
    };
    match ziel {
        Ziel::Streifen(i) => z.streifen(i, wert),
        Ziel::Bus(b) => z.bus(b, wert),
    }
}

/// Steht die Zuordnung schon so, wie sie geschrieben werden soll?
///
/// Das Nachführen der Aufklappmenüs (alle paar Sekunden, aus dem Graphen) darf
/// eine gespeicherte Zuordnung niemals ändern. Ohne diese Prüfung schrieb die
/// Oberfläche beim bloßen Anzeigen „kein Eingang“ in die Zuordnungsdatei –
/// genau der Weg, auf dem eine Zuordnung früher stillschweigend verschwand.
pub fn zuordnung_gleich<G: Geraete + ?Sized>(g: &G, ziel: Ziel<'_>, quelle: Option<&str>) -> bool {
    g.aktuell(ziel) == quelle.map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    /// Ein Zuordner, der nur mitschreibt – mehr braucht die Regel nicht.
    #[derive(Default)]
    struct Mitschrift {
        streifen: Mutex<Vec<(usize, Option<String>)>>,
        busse: Mutex<Vec<(String, Option<String>)>>,
    }

    impl Zuordner for Mitschrift {
        fn streifen(&self, strip: usize, quelle: Option<&str>) -> Result<(), String> {
            self.streifen
                .lock()
                .unwrap()
                .push((strip, quelle.map(str::to_string)));
            Ok(())
        }
        fn bus(&self, bus: &str, senke: Option<&str>) -> Result<(), String> {
            self.busse
                .lock()
                .unwrap()
                .push((bus.to_string(), senke.map(str::to_string)));
            Ok(())
        }
        fn lautstaerke(&self, _: &str, _: Option<f32>, _: Option<bool>) -> Result<(), String> {
            Ok(())
        }
        fn echo(&self, _: &[String]) -> Result<(), String> {
            Ok(())
        }
    }

    #[test]
    fn leerer_eintrag_loescht_statt_zu_schreiben() {
        let z = Mitschrift::default();
        auswahl_anwenden(&z, Ziel::Streifen(0), KEIN_EINGANG).unwrap();
        assert_eq!(z.streifen.lock().unwrap().as_slice(), [(0, None)]);
        auswahl_anwenden(&z, Ziel::Bus("A1"), "  ").unwrap();
        assert_eq!(
            z.busse.lock().unwrap().as_slice(),
            [("A1".to_string(), None)]
        );
        auswahl_anwenden(&z, Ziel::Bus("A1"), "Sennheiser USB").unwrap();
        assert_eq!(
            z.busse.lock().unwrap().as_slice()[1],
            ("A1".to_string(), Some("Sennheiser USB".to_string()))
        );
    }
}
