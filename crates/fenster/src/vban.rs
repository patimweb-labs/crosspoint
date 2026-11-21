//! Daten des VBAN-Dialogs - ohne GTK.
//!
//! Quelle ist dieselbe Funktion, die `GET /api/vban/config` beantwortet
//! (`crosspoint::vban_io::konfig_json`). So zeigen Web und Fenster dieselben
//! Zahlen, und die Oberflaeche rechnet nichts selbst.

use crosspoint::json::Value;
use crosspoint::vban_io::{VbanControl, VbanRecvControl};

/// Eine Zeile der Incoming- bzw. Outgoing-Tabelle.
#[derive(Clone, Debug, PartialEq)]
pub struct VbanZeile {
    pub stream: String,
    pub ip: String,
    pub sr: u32,
    pub kanaele: u32,
    pub format: String,
    pub quality: String,
    /// Incoming: Ziel (`vban_L/R`), Outgoing: Quelle (`Bus A1`).
    pub weg: String,
    pub fehler: u64,
}

/// Sende- oder Empfangsweg, wie ihn der Dialog zeigt.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Weg {
    pub an: bool,
    /// Nur beim Senden belegt.
    pub host: String,
    pub port: u16,
    pub stream: String,
    /// Senden: Frames, Empfangen: Pakete.
    pub zaehler: u64,
    /// Senden: Fehler, Empfangen: verworfene Pakete.
    pub fehler: u64,
    pub letzter_fehler: String,
}

/// Alles, was der Dialog anzeigt.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct VbanStand {
    pub senden: Weg,
    pub empfangen: Weg,
    pub samplerate: u32,
    pub incoming: Vec<VbanZeile>,
    pub outgoing: Vec<VbanZeile>,
}

fn text(v: &Value, k: &str) -> String {
    match v.get(k) {
        Some(Value::Str(s)) => s.clone(),
        _ => String::new(),
    }
}

fn zahl(v: &Value, k: &str) -> f64 {
    v.get(k).and_then(|x| x.as_f64()).unwrap_or(0.0)
}

fn weg(v: Option<&Value>, zaehler: &str, fehler: &str) -> Weg {
    let Some(v) = v else {
        return Weg::default();
    };
    Weg {
        an: v.get("an").and_then(|x| x.as_bool()).unwrap_or(false),
        host: text(v, "host"),
        port: zahl(v, "port").clamp(0.0, 65535.0) as u16,
        stream: text(v, "stream"),
        zaehler: zahl(v, zaehler).max(0.0) as u64,
        fehler: zahl(v, fehler).max(0.0) as u64,
        letzter_fehler: text(v, "letzter_fehler"),
    }
}

fn zeilen(v: Option<&Value>, weg_feld: &str) -> Vec<VbanZeile> {
    v.and_then(|x| x.as_arr())
        .map(|a| {
            a.iter()
                .map(|z| VbanZeile {
                    stream: text(z, "stream"),
                    ip: text(z, "ip"),
                    sr: zahl(z, "sr").max(0.0) as u32,
                    kanaele: zahl(z, "ch").max(0.0) as u32,
                    format: text(z, "format"),
                    quality: text(z, "quality"),
                    weg: text(z, weg_feld),
                    fehler: zahl(z, "errors").max(0.0) as u64,
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Stand aus dem JSON von `konfig_json` lesen.
pub fn stand_aus_json(json: &str) -> VbanStand {
    let Ok(v) = crosspoint::json::parse(json) else {
        return VbanStand::default();
    };
    VbanStand {
        senden: weg(v.get("senden"), "frames", "fehler"),
        empfangen: weg(v.get("empfangen"), "pakete", "verworfen"),
        samplerate: zahl(&v, "samplerate").max(0.0) as u32,
        incoming: zeilen(v.get("incoming"), "ziel"),
        outgoing: zeilen(v.get("outgoing"), "source"),
    }
}

/// Aktueller Stand direkt aus den Steuerungen des Kerns.
pub fn stand(send: &VbanControl, recv: &VbanRecvControl, sample_rate: u32) -> VbanStand {
    stand_aus_json(&crosspoint::vban_io::konfig_json(send, recv, sample_rate))
}

/// Eingaben des Formulars (Text wie getippt).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Formular {
    pub senden_an: bool,
    pub host: String,
    pub senden_port: f64,
    pub senden_stream: String,
    pub empfangen_an: bool,
    pub empfangen_port: f64,
    pub empfangen_stream: String,
}

impl Formular {
    /// Dasselbe JSON, das `POST /api/vban/config` annimmt.
    pub fn als_json(&self) -> Value {
        Value::obj(vec![
            (
                "senden",
                Value::obj(vec![
                    ("an", Value::Bool(self.senden_an)),
                    ("host", Value::Str(self.host.trim().to_string())),
                    ("port", Value::Num(self.senden_port)),
                    ("stream", Value::Str(self.senden_stream.trim().to_string())),
                ]),
            ),
            (
                "empfangen",
                Value::obj(vec![
                    ("an", Value::Bool(self.empfangen_an)),
                    ("port", Value::Num(self.empfangen_port)),
                    (
                        "stream",
                        Value::Str(self.empfangen_stream.trim().to_string()),
                    ),
                ]),
            ),
        ])
    }

    /// Formular anwenden. Ungueltige Werte lassen den Kern unveraendert
    /// (die Pruefung macht `wende_konfig_an`, wie bei der HTTP-Schnittstelle).
    pub fn anwenden(&self, send: &VbanControl, recv: &VbanRecvControl) -> Result<(), String> {
        crosspoint::vban_io::wende_konfig_an(send, recv, &self.als_json())
    }
}

/// Unterzeile des Dialogs, z. B. `SENDEN AN · 127.0.0.1:6980 · EMPFANG AUS`.
pub fn unterzeile(s: &VbanStand) -> String {
    let senden = if s.senden.an {
        format!("SENDEN AN · {}:{}", s.senden.host, s.senden.port)
    } else {
        "SENDEN AUS".to_string()
    };
    let empfangen = if s.empfangen.an {
        format!("EMPFANG AN · UDP {}", s.empfangen.port)
    } else {
        "EMPFANG AUS".to_string()
    };
    format!("{senden} · {empfangen} · MAIN STREAM {} Hz", s.samplerate)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stand_kommt_aus_dem_kern() {
        let send = VbanControl::new();
        let recv = VbanRecvControl::new();
        let s = stand(&send, &recv, 48_000);
        assert_eq!(s.samplerate, 48_000);
        assert!(!s.senden.an);
        assert_eq!(s.senden.host, "127.0.0.1");
        assert_eq!(s.senden.port, 6980);
        assert_eq!(s.senden.stream, "crosspointbusA1");
        // Die Ausgangszeile steht immer, auch ohne Senden.
        assert_eq!(s.outgoing.len(), 1);
        assert_eq!(s.outgoing[0].weg, "Bus A1");
        assert_eq!(s.outgoing[0].sr, 48_000);
        assert_eq!(s.outgoing[0].kanaele, 2);
        // Ohne Absender ist die Eingangstabelle leer - keine Beispielzeilen.
        assert!(s.incoming.is_empty());
    }

    #[test]
    fn formular_setzt_sende_und_empfangsweg() {
        let send = VbanControl::new();
        let recv = VbanRecvControl::new();
        let f = Formular {
            senden_an: true,
            host: " 192.168.1.20 ".into(),
            senden_port: 6990.0,
            senden_stream: "Studio".into(),
            empfangen_an: false,
            empfangen_port: 6981.0,
            empfangen_stream: "".into(),
        };
        f.anwenden(&send, &recv).expect("gueltig");
        let s = stand(&send, &recv, 48_000);
        assert!(s.senden.an);
        assert_eq!(s.senden.host, "192.168.1.20");
        assert_eq!(s.senden.port, 6990);
        assert_eq!(s.senden.stream, "Studio");
        assert_eq!(s.empfangen.port, 6981);
        assert!(!s.empfangen.an);
        assert!(unterzeile(&s).starts_with("SENDEN AN · 192.168.1.20:6990"));
    }

    #[test]
    fn ungueltiger_port_aendert_nichts() {
        let send = VbanControl::new();
        let recv = VbanRecvControl::new();
        let f = Formular {
            senden_an: true,
            host: "127.0.0.1".into(),
            senden_port: 0.0,
            senden_stream: "x".into(),
            empfangen_port: 6980.0,
            ..Default::default()
        };
        assert!(f.anwenden(&send, &recv).is_err());
        let s = stand(&send, &recv, 48_000);
        assert!(!s.senden.an, "abgewiesenes Formular darf nicht einschalten");
        assert_eq!(s.senden.stream, "crosspointbusA1");
    }

    #[test]
    fn kaputtes_json_ergibt_leeren_stand() {
        assert_eq!(stand_aus_json("kein json"), VbanStand::default());
    }
}
