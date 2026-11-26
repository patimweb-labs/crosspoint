//! DSP-Bausteine: dB-Umrechnung, Noise Gate, Low-Cut, Kompressor, Limiter,
//! Ducking, Verzögerung, FFT.
//!
//! Die Echounterdrückung steht in `aec.rs`, die Pegelregelung (AGC2) in
//! `agc.rs` – beide bauen auf `sonora` auf. Alles hier ist Handarbeit: kleine,
//! allokationsfreie Bausteine, die im Audio-Thread je Sample laufen.

use std::f32::consts::PI;

// Die Umrechnung steht im Modell (ADR 0012), weil auch der Zustand sie braucht
// (Pegel als dB im JSON); hier wird sie nur wieder angeboten.
pub use crosspoint_modell::einheiten::{db_to_lin, lin_to_db};

#[inline]
pub fn clamp(v: f32, lo: f32, hi: f32) -> f32 {
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

/* ==================================================================
Low-Cut, Kompressor, Limiter, Ducking
================================================================== */

/// Biquad in transponierter Direktform II (zwei Zustandswerte, ein Koeffizient
/// mehr für die Rückkopplung).
#[derive(Clone, Copy, Default)]
pub struct Biquad {
    b0: f32,
    b1: f32,
    b2: f32,
    a1: f32,
    a2: f32,
    z1: f32,
    z2: f32,
}

impl Biquad {
    /// Hochpass 2. Ordnung (Butterworth, Q = 0,707): bei `freq_hz` liegt die
    /// Grenze bei −3 dB, darunter fällt er mit 12 dB je Oktave.
    pub fn hochpass(freq_hz: f32, sr: f32) -> Biquad {
        let f = clamp(freq_hz, 10.0, sr * 0.45);
        let w = 2.0 * PI * f / sr;
        let (sin, cos) = (w.sin(), w.cos());
        let alpha = sin / (2.0 * std::f32::consts::FRAC_1_SQRT_2);
        let a0 = 1.0 + alpha;
        Biquad {
            b0: (1.0 + cos) * 0.5 / a0,
            b1: -(1.0 + cos) / a0,
            b2: (1.0 + cos) * 0.5 / a0,
            a1: -2.0 * cos / a0,
            a2: (1.0 - alpha) / a0,
            z1: 0.0,
            z2: 0.0,
        }
    }

    /// Neue Grenzfrequenz bei stehendem Zustand: beim Drehen am Regler knackt
    /// nichts, weil nur die Koeffizienten getauscht werden.
    pub fn setze_hochpass(&mut self, freq_hz: f32, sr: f32) {
        let (z1, z2) = (self.z1, self.z2);
        *self = Biquad::hochpass(freq_hz, sr);
        self.z1 = z1;
        self.z2 = z2;
    }

    #[inline]
    pub fn process(&mut self, x: f32) -> f32 {
        let y = self.b0 * x + self.z1;
        self.z1 = self.b1 * x - self.a1 * y + self.z2;
        self.z2 = self.b2 * x - self.a2 * y;
        y
    }

    pub fn reset(&mut self) {
        self.z1 = 0.0;
        self.z2 = 0.0;
    }
}

/// Kompressor mit gekoppeltem Stereo-Detektor (wie das Gate: beide Kanäle
/// bekommen denselben Schlüssel `max(|L|, |R|)`, damit das Stereobild steht).
///
/// Harte Kennlinie (kein Knee): über der Schwelle wird um `1 − 1/ratio`
/// reduziert, darunter bleibt das Signal unberührt. Make-up hebt danach
/// wieder an.
#[derive(Clone, Copy)]
pub struct Kompressor {
    pub enabled: bool,
    env: f32,
    att: f32,
    rel: f32,
    /// Aktuelle Verstärkung (linear, mit Make-up).
    gain: f32,
    makeup: f32,
    schwelle: f32,
    ratio: f32,
}

impl Default for Kompressor {
    fn default() -> Self {
        Kompressor {
            enabled: false,
            env: 0.0,
            att: 0.0,
            rel: 0.0,
            gain: 1.0,
            makeup: 1.0,
            schwelle: -18.0,
            ratio: 3.0,
        }
    }
}

impl Kompressor {
    /// Schwelle, Ratio, Zeiten und Make-up in dB bzw. ms (einmal je Block).
    pub fn set_werte(
        &mut self,
        schwelle_db: f32,
        ratio: f32,
        attack_ms: f32,
        release_ms: f32,
        makeup_db: f32,
        sr: f32,
    ) {
        self.schwelle = clamp(schwelle_db, -80.0, 0.0);
        self.ratio = clamp(ratio, 1.0, 40.0);
        self.att = (-1.0 / (clamp(attack_ms, 0.1, 200.0) * 0.001 * sr)).exp();
        self.rel = (-1.0 / (clamp(release_ms, 5.0, 4000.0) * 0.001 * sr)).exp();
        self.makeup = db_to_lin(clamp(makeup_db, 0.0, 30.0));
    }

    /// Ein Sample; `key` ist der Detektorwert (bei Stereo `max(|L|, |R|)`).
    #[inline]
    pub fn process_key(&mut self, x: f32, key: f32) -> f32 {
        if !self.enabled {
            return x;
        }
        let lvl = key.abs();
        let coef = if lvl > self.env { self.att } else { self.rel };
        self.env = coef * self.env + (1.0 - coef) * lvl;
        let ueber = lin_to_db(self.env) - self.schwelle;
        let ziel_db = if ueber > 0.0 {
            -ueber * (1.0 - 1.0 / self.ratio)
        } else {
            0.0
        };
        let ziel = db_to_lin(ziel_db) * self.makeup;
        let c = if ziel < self.gain { self.att } else { self.rel };
        self.gain = ziel + (self.gain - ziel) * c;
        x * self.gain
    }

    /// Verstärkung, die der Kompressor gerade anlegt (für Messungen).
    pub fn gain(&self) -> f32 {
        self.gain
    }
}

/// Limiter mit Decke: was über die Decke will, wird sofort heruntergezogen und
/// danach mit `release_ms` wieder freigegeben.
#[derive(Clone, Copy)]
pub struct Limiter {
    pub enabled: bool,
    decke: f32,
    gain: f32,
    rel: f32,
}

impl Default for Limiter {
    fn default() -> Self {
        Limiter {
            enabled: false,
            decke: db_to_lin(-1.0),
            gain: 1.0,
            rel: 0.0,
        }
    }
}

impl Limiter {
    pub fn set_werte(&mut self, decke_db: f32, release_ms: f32, sr: f32) {
        self.decke = db_to_lin(clamp(decke_db, -30.0, 0.0));
        self.rel = (-1.0 / (clamp(release_ms, 5.0, 5000.0) * 0.001 * sr)).exp();
    }

    /// Decke in dBFS (für Zustand und Tests).
    pub fn decke_db(&self) -> f32 {
        lin_to_db(self.decke)
    }

    /// Ein Sample; `key` wie beim Kompressor der gekoppelte Detektorwert.
    #[inline]
    pub fn process_key(&mut self, x: f32, key: f32) -> f32 {
        if !self.enabled {
            return x;
        }
        let noetig = if key > self.decke && key > 1e-9 {
            self.decke / key
        } else {
            1.0
        };
        // Sofort zu, langsam auf: kein Überschwingen über die Decke.
        if noetig < self.gain {
            self.gain = noetig;
        } else {
            self.gain = noetig + (self.gain - noetig) * self.rel;
        }
        let y = x * self.gain;
        // Letzte Hand: die Decke hält auch, wenn im Block etwas springt.
        clamp(y, -self.decke, self.decke)
    }
}

/// Ducking: senkt einen anderen Bus ab, solange der eigene Bus über der
/// Schwelle liegt.
///
/// Das Ziel steht im Parameter des Quellbusses; die Absenkung wird über den
/// **Pegel aus dem vorigen Block** entschieden (siehe `EngineDsp::process`).
#[derive(Clone, Copy)]
pub struct Ducker {
    gain: f32,
    att: f32,
    rel: f32,
}

impl Default for Ducker {
    fn default() -> Self {
        Ducker {
            gain: 1.0,
            att: 0.0,
            rel: 0.0,
        }
    }
}

impl Ducker {
    pub fn set_times(&mut self, attack_ms: f32, release_ms: f32, sr: f32) {
        self.att = (-1.0 / (clamp(attack_ms, 0.5, 2000.0) * 0.001 * sr)).exp();
        self.rel = (-1.0 / (clamp(release_ms, 5.0, 8000.0) * 0.001 * sr)).exp();
    }

    /// Ein Sample weiterrechnen. `ueber` = Quellbus liegt über der Schwelle,
    /// `tiefe` = Zielverstärkung der Absenkung (linear, z. B. −12 dB).
    #[inline]
    pub fn step(&mut self, ueber: bool, tiefe: f32) -> f32 {
        let ziel = if ueber { tiefe } else { 1.0 };
        let c = if ziel < self.gain { self.att } else { self.rel };
        self.gain = ziel + (self.gain - ziel) * c;
        self.gain
    }

    /// Aktuelle Absenkung (linear).
    pub fn gain(&self) -> f32 {
        self.gain
    }

    pub fn reset(&mut self) {
        self.gain = 1.0;
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
    fn lowcut_daempft_tiefen_und_laesst_hoehen_stehen() {
        let sr = 48_000.0;
        let mut bq = Biquad::hochpass(80.0, sr);
        // Pegel (Effektivwert) einer Sinuswelle nach dem Filter, eingeschwungen.
        let messen = |freq: f32, bq: &mut Biquad| {
            let mut summe = 0.0f64;
            let n = (sr * 0.5) as usize;
            for i in 0..n {
                let x = (2.0 * PI * freq * i as f32 / sr).sin();
                let y = bq.process(x);
                if i >= n / 2 {
                    summe += (y * y) as f64;
                }
            }
            (summe / (n / 2) as f64).sqrt() as f32
        };
        let tief = messen(30.0, &mut bq);
        let hoch = messen(1000.0, &mut bq);
        // Sinus hat Effektivwert 1/√2; bei 30 Hz muss deutlich gedämpft sein.
        let tief_db = lin_to_db(tief * std::f32::consts::SQRT_2);
        let hoch_db = lin_to_db(hoch * std::f32::consts::SQRT_2);
        assert!(tief_db < -12.0, "30 Hz nur {tief_db:.1} dB");
        assert!(hoch_db.abs() < 0.2, "1 kHz verändert: {hoch_db:.2} dB");
    }

    #[test]
    fn kompressor_reduziert_um_die_ratio() {
        let sr = 48_000.0;
        let mut k = Kompressor {
            enabled: true,
            ..Default::default()
        };
        // Schwelle −20 dBFS, Ratio 4:1, schnelle Attacke.
        k.set_werte(-20.0, 4.0, 1.0, 100.0, 0.0, sr);
        // Sinus mit −6 dBFS Spitze (0,5) → 14 dB über der Schwelle.
        let mut spitze = 0.0f32;
        for i in 0..(sr * 0.5) as usize {
            let x = 0.5 * (2.0 * PI * 400.0 * i as f32 / sr).sin();
            let key = x.abs();
            let y = k.process_key(x, key);
            if i > (sr * 0.25) as usize {
                spitze = spitze.max(y.abs());
            }
        }
        let erwartet = -6.0 - 14.0 * (1.0 - 0.25);
        let gemessen = lin_to_db(spitze);
        assert!(
            (gemessen - erwartet).abs() < 1.5,
            "Ausgang {gemessen:.1} dBFS, erwartet {erwartet:.1} dBFS"
        );
        // Aus: unverändert.
        k.enabled = false;
        assert!((k.process_key(0.5, 0.5) - 0.5).abs() < 1e-6);
    }

    #[test]
    fn limiter_haelt_die_decke() {
        let sr = 48_000.0;
        let mut l = Limiter {
            enabled: true,
            ..Default::default()
        };
        l.set_werte(-1.0, 50.0, sr);
        let decke = db_to_lin(-1.0);
        assert!((l.decke_db() - -1.0).abs() < 0.01);
        let mut spitze = 0.0f32;
        for i in 0..(sr * 0.2) as usize {
            // 6 dB über der Decke, dazu ein Sprung mitten im Block.
            let a = if i % 1000 == 0 { 2.0 } else { 1.2 };
            let x = a * (2.0 * PI * 200.0 * i as f32 / sr).sin();
            let y = l.process_key(x, x.abs());
            assert!(y.abs() <= decke + 1e-6, "Sample {y} über der Decke");
            spitze = spitze.max(y.abs());
        }
        assert!(
            spitze > decke * 0.7,
            "Limiter hat zu stark zugepackt: {spitze}"
        );
    }

    #[test]
    fn ducker_senkt_ab_und_gibt_wieder_frei() {
        let sr = 48_000.0;
        let mut d = Ducker::default();
        d.set_times(20.0, 200.0, sr);
        let tiefe = db_to_lin(-12.0);
        for _ in 0..(sr * 0.2) as usize {
            d.step(true, tiefe);
        }
        let abgesenkt = d.gain();
        assert!(
            (lin_to_db(abgesenkt) + 12.0).abs() < 0.5,
            "abgesenkt auf {:.1} dB",
            lin_to_db(abgesenkt)
        );
        for _ in 0..(sr * 1.0) as usize {
            d.step(false, tiefe);
        }
        assert!(d.gain() > 0.99, "nicht freigegeben: {:.3}", d.gain());
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
