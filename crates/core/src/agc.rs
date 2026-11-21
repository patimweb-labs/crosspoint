//! Automatische Pegelanpassung (AGC): AGC2 aus [`sonora`].
//!
//! `sonora` ist die Rust-Übersetzung von WebRTCs `AudioProcessing` (dieselbe
//! Bibliothek, aus der auch die Echounterdrückung kommt, siehe `aec.rs`). Der
//! Regler AGC2 besteht dort aus drei Teilen: einer Empfehlung für die
//! Gerätelautstärke (hier aus – die gehört dem Nutzer), einem adaptiven
//! Digitalregler und einem festen Verstärker mit Limiter.
//!
//! # Zielpegel
//!
//! AGC2 regelt die **Sprachlautstärke** gegen seine eigene Vorgabe (abhängig vom
//! Headroom des Sättigungsschutzes, gemessen um −12 dBFS RMS); einen
//! einstellbaren Zielpegel kennt es nicht. Der Zielpegel ist deshalb eine
//! eigene, langsame Regelung hinter AGC2: eine Spitzenhüllkurve misst den
//! Ausgang, und ein Trim zieht die Spitzen auf den eingestellten Wert
//! (Vorgabe −12 dBFS). 0 dB heißt „so laut, wie AGC2 es lässt“.
//!
//! # Takt und Latenz
//!
//! Wie die Echounterdrückung rechnet AGC2 in Rahmen von 10 ms. Der Sammler
//! gibt deshalb einen Rahmen später aus: **10 ms** Verzögerung, nur wenn AGC
//! eingeschaltet ist. Aus ist aus – dann geht das Sample unverändert durch,
//! ohne Verzögerung und ohne Kosten.
//!
//! # Kein Allokieren
//!
//! Der Rechner entsteht in [`Pegelregler::neu`] (außerhalb des Audio-Threads),
//! alle Puffer liegen im Struct. `process` baut nichts; belegt in
//! `crates/core/tests/allokation.rs` (AGC an allen virtuellen Bussen).

use sonora::config::{AdaptiveDigital, FixedDigital, GainController2};
use sonora::{AudioProcessing, Config, StreamConfig};

use crate::dsp::{clamp, db_to_lin, lin_to_db};

/// Länge eines Rahmens: 10 ms, wie bei AEC3.
pub const RAHMEN_MS: u32 = 10;
/// Größter Rahmen (10 ms bei 96 kHz) – Puffergröße des Sammlers.
pub const RAHMEN_MAX: usize = 960;

/// Was `sonora` rechnen soll: nur AGC2, ohne Echo und ohne Rauschunterdrückung.
fn konfiguration() -> Config {
    Config {
        gain_controller2: Some(GainController2 {
            // Die Gerätelautstärke gehört dem Nutzer, nicht dem Mixer.
            input_volume_controller: false,
            adaptive_digital: Some(AdaptiveDigital::default()),
            fixed_digital: FixedDigital::default(),
        }),
        ..Default::default()
    }
}

/// Ein Kanal AGC2 mit eigenem Rahmen-Sammler.
pub struct Pegelregler {
    /// Der Regler von `sonora`; entsteht in [`Self::neu`].
    apm: AudioProcessing,
    /// Samples je Rahmen (10 ms bei der Abtastrate der Engine).
    rahmen: usize,
    ein: [f32; RAHMEN_MAX],
    ein_pos: usize,
    aus: [f32; RAHMEN_MAX],
    aus_pos: usize,
    scratch: [f32; RAHMEN_MAX],
    /// Zielpegel (linear): dorthin werden die Spitzen gezogen.
    ziel: f32,
    /// Spitzenhüllkurve des Reglerausgangs (linear, vor dem Trim).
    huelle: f32,
    /// Hüllkurve: 500 ms Abfall (der Anstieg ist sofort).
    huelle_rel: f32,
    /// Wie schnell der Trim dem Ziel folgt (500 ms).
    trim_coef: f32,
    /// Aktueller Trim (linear).
    trim: f32,
}

impl Pegelregler {
    /// Legt den Regler an. **Außerhalb des Audio-Threads aufrufen** – hier
    /// wird allokiert.
    pub fn neu(abtastrate: u32) -> Pegelregler {
        let rahmen = ((abtastrate as usize / 1000) * RAHMEN_MS as usize).clamp(80, RAHMEN_MAX);
        let apm = AudioProcessing::builder()
            .config(konfiguration())
            .capture_config(StreamConfig::new(abtastrate, 1))
            .render_config(StreamConfig::new(abtastrate, 1))
            .build();
        let mut p = Pegelregler {
            apm,
            rahmen,
            ein: [0.0; RAHMEN_MAX],
            ein_pos: 0,
            aus: [0.0; RAHMEN_MAX],
            // Noch kein Rahmen fertig: die ersten Samples sind Stille, damit
            // die Ausgabe um genau einen Rahmen verzögert ist.
            aus_pos: RAHMEN_MAX,
            scratch: [0.0; RAHMEN_MAX],
            ziel: 1.0,
            huelle: 0.0,
            huelle_rel: (-1.0 / (0.500 * abtastrate as f32)).exp(),
            trim_coef: (-1.0 / (0.500 * abtastrate as f32)).exp(),
            trim: 1.0,
        };
        p.set_ziel(-12.0);
        p
    }

    /// Verzögerung der Ausgabe in Samples, solange der Regler läuft.
    pub fn verzoegerung(&self) -> usize {
        self.rahmen
    }

    /// Zielpegel in dBFS (0 dB = AGC2 ungebremst, Vorgabe −12 dB).
    pub fn set_ziel(&mut self, ziel_db: f32) {
        self.ziel = db_to_lin(ziel_db.clamp(-40.0, 0.0));
    }

    /// Zielpegel in dBFS (für Tests und die Anzeige).
    pub fn ziel_db(&self) -> f32 {
        lin_to_db(self.ziel)
    }

    /// Puffer leeren (beim Ein- oder Ausschalten, ohne Allokation).
    pub fn reset(&mut self) {
        self.ein.fill(0.0);
        self.ein_pos = 0;
        self.aus.fill(0.0);
        self.aus_pos = RAHMEN_MAX;
        self.huelle = 0.0;
        self.trim = 1.0;
    }

    /// Ein Sample durch den Regler. Die ersten `rahmen` Samples sind Stille
    /// (die Verzögerung), danach kommt der geregelte Pegel mit `ziel`.
    #[inline]
    pub fn process(&mut self, x: f32) -> f32 {
        self.ein[self.ein_pos] = x;
        self.ein_pos += 1;
        if self.ein_pos == self.rahmen {
            self.ein_pos = 0;
            let _ = self.apm.process_capture_f32(
                &[&self.ein[..self.rahmen]],
                &mut [&mut self.scratch[..self.rahmen]],
            );
            self.aus[..self.rahmen].copy_from_slice(&self.scratch[..self.rahmen]);
            self.aus_pos = 0;
        }
        if self.aus_pos < self.rahmen {
            let roh = self.aus[self.aus_pos];
            self.aus_pos += 1;
            // Spitzenhüllkurve: sofort auf die Spitze, langsam ab. Ein
            // geglätteter Detektor läge unter der Spitze, und der Trim würde
            // die Spitzen über den Zielpegel heben.
            let a = roh.abs();
            self.huelle = if a > self.huelle {
                a
            } else {
                self.huelle_rel * self.huelle
            };
            // Trim auf den Zielpegel; unter −60 dBFS passiert nichts (Stille
            // soll nicht auf Vollgas geregelt werden), und der Trim bleibt in
            // einem Bereich, der nicht rauscht.
            if self.huelle > db_to_lin(-60.0) {
                let noetig = clamp(self.ziel / self.huelle, db_to_lin(-30.0), db_to_lin(30.0));
                self.trim = noetig + (self.trim - noetig) * self.trim_coef;
            }
            roh * self.trim
        } else {
            0.0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn regler() -> Box<Pegelregler> {
        // Der Zustand von AGC2 ist groß: auf dem eigenen Stapel bauen.
        std::thread::Builder::new()
            .stack_size(16 * 1024 * 1024)
            .spawn(|| Box::new(Pegelregler::neu(48_000)))
            .unwrap()
            .join()
            .unwrap()
    }

    fn pegel(v: &[f32]) -> f32 {
        (v.iter().map(|x| x * x).sum::<f32>() / v.len().max(1) as f32).sqrt()
    }

    fn spitze(v: &[f32]) -> f32 {
        v.iter().fold(0.0f32, |a, x| a.max(x.abs()))
    }

    /// Sprache nachahmen: 300 ms Ton (mit etwas Rauschen), 200 ms Pause. Ein
    /// reiner Dauerton gilt AGC2 nicht als Sprache und wird nicht angehoben.
    fn sprache(amp: f32, sr: f32, sekunden: f32) -> Vec<f32> {
        let n = (sr * sekunden) as usize;
        let mut r = 987_654_321u32;
        let mut phase = 0.0f32;
        (0..n)
            .map(|i| {
                let t = i as f32 / sr;
                r ^= r << 13;
                r ^= r >> 17;
                r ^= r << 5;
                let rauschen = (r as f32 / u32::MAX as f32) * 2.0 - 1.0;
                let huell = if t % 0.5 < 0.3 { 1.0 } else { 0.02 };
                phase += 2.0 * std::f32::consts::PI * (180.0 + 20.0 * (t * 7.0).sin()) / sr;
                amp * huell * (phase.sin() + 0.05 * rauschen)
            })
            .collect()
    }

    #[test]
    fn leises_signal_wird_angehoben() {
        let sr = 48_000.0;
        let mut agc = regler();
        let ein = sprache(0.02, sr, 8.0);
        let aus: Vec<f32> = ein.iter().map(|&x| agc.process(x)).collect();
        let s = (6.0 * sr) as usize;
        let vorher = pegel(&ein[s..]);
        let nachher = pegel(&aus[s..]);
        assert!(
            nachher > vorher * 2.0,
            "AGC hat nicht angehoben: {vorher:.4} → {nachher:.4}"
        );
    }

    #[test]
    fn spitzen_landen_auf_dem_zielpegel() {
        // Vorgabe −12 dBFS: die Spitzen liegen danach dort, egal wie laut das
        // Mikrofon vorher war (AGC2 regelt die Sprache, der Trim die Spitzen).
        for amp in [0.02f32, 0.5] {
            let sr = 48_000.0;
            let mut agc = regler();
            let ein = sprache(amp, sr, 8.0);
            let aus: Vec<f32> = ein.iter().map(|&x| agc.process(x)).collect();
            let s = (6.0 * sr) as usize;
            let db = lin_to_db(spitze(&aus[s..]));
            assert!(
                (db + 12.0).abs() < 3.0,
                "Eingang {amp}: Spitze {db:.1} dBFS statt −12 dBFS"
            );
        }
    }

    #[test]
    fn zielpegel_ist_einstellbar() {
        let sr = 48_000.0;
        let mut leise = regler();
        leise.set_ziel(-24.0);
        assert!((leise.ziel_db() + 24.0).abs() < 0.01);
        let ein = sprache(0.02, sr, 8.0);
        let aus: Vec<f32> = ein.iter().map(|&x| leise.process(x)).collect();
        let db = lin_to_db(spitze(&aus[(6.0 * sr) as usize..]));
        assert!(
            (db + 24.0).abs() < 3.0,
            "Spitze {db:.1} dBFS statt −24 dBFS"
        );
        // Ausgabe kommt einen Rahmen später: die ersten 479 Samples sind still.
        let mut agc = regler();
        assert_eq!(agc.verzoegerung(), 480);
        for _ in 0..479 {
            assert_eq!(agc.process(0.5), 0.0);
        }
    }
}
