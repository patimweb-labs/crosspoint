//! DSP-Bausteine: dB-Umrechnung, Noise Gate, Verzögerung, FFT.
//!
//! Die Echounterdrückung steht in `aec.rs`, sie baut auf [`Fft`] auf.

use std::f32::consts::PI;

#[inline]
pub fn db_to_lin(db: f32) -> f32 {
    10f32.powf(db / 20.0)
}

#[inline]
pub fn lin_to_db(v: f32) -> f32 {
    if v <= 1e-7 {
        -140.0
    } else {
        20.0 * v.log10()
    }
}

#[inline]
fn clamp(v: f32, lo: f32, hi: f32) -> f32 {
    if v < lo {
        lo
    } else if v > hi {
        hi
    } else {
        v
    }
}

/// Schwelle, Hold, Attack und Release kommen aus den Parametern; `tiefe_db`
/// bestimmt, wie weit das Gate zumacht (Boden), damit ein offenes Mikrofon
/// nicht auf digitale Stille fällt, sondern auf einen einstellbaren Rest.
#[derive(Clone, Copy)]
pub struct Gate {
    pub enabled: bool,
    env: f32,
    hold: f32,
    gain: f32,
    att: f32,
    rel: f32,
    hold_s: f32,
    /// Freigabe des **Detektors** (fest 20 ms): die Huellkurve muss dem
    /// Signal schnell folgen, sonst bleibt das Gate nach einer Pause noch
    /// Sekunden offen. Die hoerbare Regelzeit ist `rel` (Parameter).
    env_rel: f32,
    /// Anstieg des **Detektors** (fest 0,5 ms). Frueher lief er mit der
    /// Attack-Zeit: bei 100 ms Attack merkte das Gate erst nach rund 100 ms,
    /// dass gesprochen wird, und oeffnete dann noch einmal 100 ms lang.
    env_att: f32,
    /// Linearer Boden bei geschlossenem Gate (aus `tiefe_db`).
    boden: f32,
}

impl Default for Gate {
    fn default() -> Self {
        Gate {
            enabled: false,
            env: 0.0,
            hold: 0.0,
            gain: 0.0,
            att: 0.0,
            rel: 0.0,
            hold_s: 0.0,
            env_rel: 0.0,
            env_att: 0.0,
            boden: db_to_lin(-60.0),
        }
    }
}

impl Gate {
    pub fn set_times(&mut self, attack_ms: f32, hold_ms: f32, release_ms: f32, sr: f32) {
        self.att = (-1.0 / (clamp(attack_ms, 0.1, 500.0) * 0.001 * sr)).exp();
        self.rel = (-1.0 / (clamp(release_ms, 1.0, 5000.0) * 0.001 * sr)).exp();
        self.hold_s = clamp(hold_ms, 0.0, 2000.0) * 0.001 * sr;
        self.env_rel = (-1.0 / (0.020 * sr)).exp();
        self.env_att = (-1.0 / (0.0005 * sr)).exp();
    }

    /// Tiefe in dB (0..80): Daempfung, wenn das Gate zu ist.
    pub fn set_tiefe(&mut self, db: f32) {
        self.boden = db_to_lin(-db.clamp(0.0, 80.0));
    }

    pub fn process(&mut self, x: f32, threshold_db: f32) -> f32 {
        self.process_key(x, x.abs(), threshold_db)
    }

    /// Wie [`Gate::process`], aber der Detektor sieht `key` statt `|x|`.
    /// Fuer Stereo bekommen beide Kanaele denselben Schluessel
    /// (`max(|L|, |R|)`): beide Gates laufen dann im Gleichschritt, und das
    /// Stereobild wandert nicht, wenn nur eine Seite leise wird.
    pub fn process_key(&mut self, x: f32, key: f32, threshold_db: f32) -> f32 {
        if !self.enabled {
            return x;
        }
        let lvl = key;
        // Einpolige Huellkurve: oeffnet mit Attack, schliesst mit Release.
        // (Frueher stand hier `0.999.max(att)` bei `1.0 - att`, wodurch die
        // Huellkurve den Pegel um Faktor 1/(1-0,999) zu hoch zeigte und die
        // Schwelle nie erreichte.)
        let coef = if lvl > self.env {
            self.env_att
        } else {
            self.env_rel
        };
        self.env = coef * self.env + (1.0 - coef) * lvl;
        if lin_to_db(self.env) > threshold_db {
            self.hold = self.hold_s;
            self.gain += (1.0 - self.gain) * (1.0 - self.att);
        } else if self.hold > 0.0 {
            self.hold -= 1.0;
        } else {
            self.gain += (self.boden - self.gain) * (1.0 - self.rel);
        }
        x * self.gain
    }
}

/// Verzögerungsleitung für Bus-Delay und Kanaltrennung.
pub struct DelayLine {
    buf: Vec<f32>,
    pos: usize,
    len: usize,
}

impl DelayLine {
    pub fn new(max_frames: usize) -> DelayLine {
        DelayLine {
            buf: vec![0.0; max_frames.max(1)],
            pos: 0,
            len: 0,
        }
    }

    pub fn set_ms(&mut self, ms: f32, sr: f32) {
        let n = (clamp(ms, 0.0, 2000.0) * 0.001 * sr) as usize;
        self.len = n.min(self.buf.len() - 1);
    }

    #[inline]
    pub fn process(&mut self, x: f32) -> f32 {
        if self.len == 0 {
            return x;
        }
        let n = self.buf.len();
        self.buf[self.pos] = x;
        let read = (self.pos + n - self.len) % n;
        self.pos = (self.pos + 1) % n;
        self.buf[read]
    }
}

/* ==================================================================
FFT – eigene Radix-2-Implementierung, Tabellen vorab
================================================================== */

/// In-Place-FFT der Groesse `n` (Zweierpotenz) mit vorab gebauten Tabellen.
///
/// Vorwaerts und rueckwaerts ohne jede Allokation: Bit-Umkehr-Tabelle und
/// Drehfaktoren entstehen einmal in [`Fft::new`], danach lebt die Rechnung
/// nur auf den uebergebenen Puffern.
pub struct Fft {
    n: usize,
    rev: Vec<u32>,
    cos: Vec<f32>,
    sin: Vec<f32>,
}

impl Fft {
    pub fn new(n: usize) -> Fft {
        assert!(n.is_power_of_two(), "FFT-Groesse muss Zweierpotenz sein");
        let bits = n.trailing_zeros();
        let rev: Vec<u32> = (0..n as u32)
            .map(|i| i.reverse_bits() >> (32 - bits))
            .collect();
        let mut cos = vec![0.0f32; n / 2];
        let mut sin = vec![0.0f32; n / 2];
        for k in 0..n / 2 {
            let w = 2.0 * PI * k as f32 / n as f32;
            cos[k] = w.cos();
            sin[k] = w.sin();
        }
        Fft { n, rev, cos, sin }
    }

    pub fn n(&self) -> usize {
        self.n
    }

    /// Vorwaertstransformation, in place. `re`/`im` muessen `n` lang sein.
    pub fn forward(&self, re: &mut [f32], im: &mut [f32]) {
        let n = self.n;
        for i in 0..n {
            let j = self.rev[i] as usize;
            if j > i {
                re.swap(i, j);
                im.swap(i, j);
            }
        }
        let mut len = 2;
        while len <= n {
            let half = len / 2;
            let step = n / len;
            let mut start = 0;
            while start < n {
                for j in 0..half {
                    let k = j * step;
                    let (wr, wi) = (self.cos[k], -self.sin[k]);
                    let a = start + j;
                    let b = a + half;
                    let tr = re[b] * wr - im[b] * wi;
                    let ti = re[b] * wi + im[b] * wr;
                    re[b] = re[a] - tr;
                    im[b] = im[a] - ti;
                    re[a] += tr;
                    im[a] += ti;
                }
                start += len;
            }
            len *= 2;
        }
    }

    /// Rueckwaertstransformation mit Faktor `1/n`, in place.
    pub fn inverse(&self, re: &mut [f32], im: &mut [f32]) {
        for v in im.iter_mut() {
            *v = -*v;
        }
        self.forward(re, im);
        let s = 1.0 / self.n as f32;
        for i in 0..self.n {
            re[i] *= s;
            im[i] = -im[i] * s;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gate_oeffnet_trotz_langer_attack_sofort_zu_erkennen() {
        // Attack 200 ms: das Gate soll *weich* oeffnen, aber sofort damit
        // anfangen. Nach 20 ms ist die Verstaerkung deutlich ueber dem Boden.
        let sr = 48000.0;
        let mut g = Gate {
            enabled: true,
            ..Default::default()
        };
        g.set_times(200.0, 80.0, 200.0, sr);
        g.set_tiefe(60.0);
        for i in 0..(0.020 * sr) as usize {
            let x = 0.1 * (2.0 * std::f32::consts::PI * 300.0 * i as f32 / sr).sin();
            g.process(x, -45.0);
        }
        assert!(g.gain > 0.08, "nach 20 ms erst {}", g.gain);
    }

    #[test]
    fn delay_verzoegert_um_frames() {
        let mut d = DelayLine::new(1024);
        d.set_ms(1.0, 48000.0); // 48 Frames
        for k in 0..48 {
            assert_eq!(d.process(if k == 0 { 1.0 } else { 0.0 }), 0.0);
        }
        assert!((d.process(0.0) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn gate_laesst_lautes_durch() {
        let mut g = Gate {
            enabled: true,
            ..Default::default()
        };
        g.set_times(1.0, 50.0, 50.0, 48000.0);
        let mut out = 0.0;
        for _ in 0..9600 {
            out = g.process(0.5, -40.0);
        }
        assert!(out > 0.4, "Gate hat zu stark gedämpft: {out}");
    }
}
