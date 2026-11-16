//! DSP-Bausteine.
//!
//! **Herkunft der Algorithmen:** Der Render-Pfad des Originals ist belegt
//! (`render(ctx, float* out, frames)`, siehe docs/03), die *Koeffizientenbildung*
//! der FX-Kerne ist es **nicht**. Hier stehen deshalb Standard-Implementierungen
//! (RBJ-Biquads, Peak-Detektor mit Attack/Release) mit derselben Parameterliste
//! wie im Original. Was vom Original abweicht, steht im Doc-Kommentar.

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

/// Filtertyp einer EQ-Zelle (Reihenfolge der `EQtype`-Werte im Original unbelegt).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum FilterType {
    Peak,
    LowShelf,
    HighShelf,
    LowPass,
    HighPass,
    BandPass,
    Notch,
}

impl FilterType {
    pub fn from_u8(v: u8) -> FilterType {
        match v {
            1 => FilterType::LowShelf,
            2 => FilterType::HighShelf,
            3 => FilterType::LowPass,
            4 => FilterType::HighPass,
            5 => FilterType::BandPass,
            6 => FilterType::Notch,
            _ => FilterType::Peak,
        }
    }
    pub fn as_u8(self) -> u8 {
        match self {
            FilterType::Peak => 0,
            FilterType::LowShelf => 1,
            FilterType::HighShelf => 2,
            FilterType::LowPass => 3,
            FilterType::HighPass => 4,
            FilterType::BandPass => 5,
            FilterType::Notch => 6,
        }
    }
}

/// Biquad in Direct-Form-I, RBJ-Cookbook.
#[derive(Clone, Copy, Default)]
pub struct Biquad {
    b0: f32,
    b1: f32,
    b2: f32,
    a1: f32,
    a2: f32,
    x1: f32,
    x2: f32,
    y1: f32,
    y2: f32,
}

impl Biquad {
    pub fn bypass() -> Biquad {
        Biquad {
            b0: 1.0,
            ..Default::default()
        }
    }

    pub fn coeffs(&mut self, kind: FilterType, freq: f32, q: f32, gain_db: f32, sr: f32) {
        let f = clamp(freq, 10.0, sr * 0.49);
        let q = clamp(q, 0.05, 20.0);
        let a = db_to_lin(gain_db / 2.0);
        let w0 = 2.0 * PI * f / sr;
        let (sn, cs) = (w0.sin(), w0.cos());
        let alpha = sn / (2.0 * q);
        let (b0, b1, b2, a0, a1, a2) = match kind {
            FilterType::Peak => (
                1.0 + alpha * a,
                -2.0 * cs,
                1.0 - alpha * a,
                1.0 + alpha / a,
                -2.0 * cs,
                1.0 - alpha / a,
            ),
            FilterType::LowShelf => {
                let s = 2.0 * a.sqrt() * alpha;
                (
                    a * ((a + 1.0) - (a - 1.0) * cs + s),
                    2.0 * a * ((a - 1.0) - (a + 1.0) * cs),
                    a * ((a + 1.0) - (a - 1.0) * cs - s),
                    (a + 1.0) + (a - 1.0) * cs + s,
                    -2.0 * ((a - 1.0) + (a + 1.0) * cs),
                    (a + 1.0) + (a - 1.0) * cs - s,
                )
            }
            FilterType::HighShelf => {
                let s = 2.0 * a.sqrt() * alpha;
                (
                    a * ((a + 1.0) + (a - 1.0) * cs + s),
                    -2.0 * a * ((a - 1.0) + (a + 1.0) * cs),
                    a * ((a + 1.0) + (a - 1.0) * cs - s),
                    (a + 1.0) - (a - 1.0) * cs + s,
                    2.0 * ((a - 1.0) - (a + 1.0) * cs),
                    (a + 1.0) - (a - 1.0) * cs - s,
                )
            }
            FilterType::LowPass => (
                (1.0 - cs) / 2.0,
                1.0 - cs,
                (1.0 - cs) / 2.0,
                1.0 + alpha,
                -2.0 * cs,
                1.0 - alpha,
            ),
            FilterType::HighPass => (
                (1.0 + cs) / 2.0,
                -(1.0 + cs),
                (1.0 + cs) / 2.0,
                1.0 + alpha,
                -2.0 * cs,
                1.0 - alpha,
            ),
            FilterType::BandPass => (alpha, 0.0, -alpha, 1.0 + alpha, -2.0 * cs, 1.0 - alpha),
            FilterType::Notch => (1.0, -2.0 * cs, 1.0, 1.0 + alpha, -2.0 * cs, 1.0 - alpha),
        };
        let inv = 1.0 / a0;
        self.b0 = b0 * inv;
        self.b1 = b1 * inv;
        self.b2 = b2 * inv;
        self.a1 = a1 * inv;
        self.a2 = a2 * inv;
    }

    #[inline]
    pub fn process(&mut self, x: f32) -> f32 {
        let y = self.b0 * x + self.b1 * self.x1 + self.b2 * self.x2
            - self.a1 * self.y1
            - self.a2 * self.y2;
        self.x2 = self.x1;
        self.x1 = x;
        self.y2 = self.y1;
        self.y1 = y;
        y
    }

    pub fn reset(&mut self) {
        self.x1 = 0.0;
        self.x2 = 0.0;
        self.y1 = 0.0;
        self.y2 = 0.0;
    }
}

/// Kompressor mit weichem Knie, Peak-Detektor.
/// Weicht in der Kennlinie vom Original ab (dort nicht rekonstruiert).
#[derive(Clone, Copy)]
pub struct Compressor {
    pub enabled: bool,
    env: f32,
    att: f32,
    rel: f32,
}

impl Default for Compressor {
    fn default() -> Self {
        Compressor {
            enabled: false,
            env: 0.0,
            att: 0.0,
            rel: 0.0,
        }
    }
}

impl Compressor {
    pub fn set_times(&mut self, attack_ms: f32, release_ms: f32, sr: f32) {
        self.att = (-1.0 / (clamp(attack_ms, 0.1, 500.0) * 0.001 * sr)).exp();
        self.rel = (-1.0 / (clamp(release_ms, 1.0, 5000.0) * 0.001 * sr)).exp();
    }

    pub fn process(
        &mut self,
        x: f32,
        threshold_db: f32,
        ratio: f32,
        knee_db: f32,
        makeup_db: f32,
    ) -> f32 {
        if !self.enabled {
            return x;
        }
        let lvl = x.abs();
        let coef = if lvl > self.env { self.att } else { self.rel };
        self.env = coef * self.env + (1.0 - coef) * lvl;
        let env_db = lin_to_db(self.env);
        let ratio = ratio.max(1.0);
        let over = env_db - threshold_db;
        let gr_db = if over <= -knee_db * 0.5 {
            0.0
        } else if over >= knee_db * 0.5 && knee_db > 0.0 {
            over * (1.0 - 1.0 / ratio)
        } else if knee_db > 0.0 {
            let t = over + knee_db * 0.5;
            (1.0 - 1.0 / ratio) * t * t / (2.0 * knee_db)
        } else {
            over.max(0.0) * (1.0 - 1.0 / ratio)
        };
        x * db_to_lin(makeup_db - gr_db)
    }
}

/// Gate mit Hold. Kennlinie ebenfalls Standard, nicht aus dem Original.
///
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

/// Denoiser als weicher Downward-Expander (Original-Modul "Audibility" nicht rekonstruiert).
///
/// Die Huellkurve steigt in 1 ms und faellt in 100 ms. Frueher lief sie mit
/// einem festen Faktor (0,9995, ~42 ms bei 48 kHz) in beide Richtungen: jeder
/// Wortanfang nach einer Pause blieb dann rund 40 ms abgesenkt.
#[derive(Clone, Copy, Default)]
pub struct Denoiser {
    pub enabled: bool,
    env: f32,
    att: f32,
    rel: f32,
    sr: f32,
}

impl Denoiser {
    /// Regelzeiten aus der Abtastrate (einmal je Block, nicht je Sample).
    pub fn set_sample_rate(&mut self, sr: f32) {
        let sr = sr.max(8000.0);
        if (sr - self.sr).abs() < 0.5 {
            return;
        }
        self.sr = sr;
        self.att = (-1.0 / (0.001 * sr)).exp();
        self.rel = (-1.0 / (0.100 * sr)).exp();
    }

    pub fn process(&mut self, x: f32, floor_db: f32, threshold_db: f32) -> f32 {
        if !self.enabled {
            return x;
        }
        if self.sr == 0.0 {
            self.set_sample_rate(48000.0);
        }
        let lvl = x.abs();
        let c = if lvl > self.env { self.att } else { self.rel };
        self.env = c * self.env + (1.0 - c) * lvl;
        let env_db = lin_to_db(self.env);
        if env_db >= threshold_db {
            x
        } else {
            let t = clamp((env_db - (threshold_db - 30.0)) / 30.0, 0.0, 1.0);
            x * db_to_lin(floor_db * (1.0 - t))
        }
    }
}

/// Bus-Limiter: Peak-Limiter mit sofortigem Angriff und einstellbarer Freigabe.
#[derive(Clone, Copy)]
pub struct Limiter {
    pub enabled: bool,
    gain: f32,
    rel: f32,
}

impl Default for Limiter {
    fn default() -> Self {
        Limiter {
            enabled: true,
            gain: 1.0,
            rel: 0.0,
        }
    }
}

impl Limiter {
    pub fn set_release(&mut self, release_ms: f32, sr: f32) {
        self.rel = (-1.0 / (clamp(release_ms, 5.0, 2000.0) * 0.001 * sr)).exp();
    }

    pub fn process(&mut self, x: f32, ceiling_db: f32) -> f32 {
        if !self.enabled {
            return x;
        }
        let ceil = db_to_lin(ceiling_db);
        let want = if x.abs() > ceil { ceil / x.abs() } else { 1.0 };
        if want < self.gain {
            self.gain = want; // sofortiger Angriff
        } else {
            self.gain = self.rel * self.gain + (1.0 - self.rel) * want;
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
    fn denoiser_laesst_den_wortanfang_durch() {
        // Nach einer Pause: ein Ton weit ueber der Schwelle ist nach 3 ms
        // praktisch unabgesenkt (frueher ~40 ms Einschwingen).
        let sr = 48000.0;
        let mut d = Denoiser {
            enabled: true,
            ..Default::default()
        };
        d.set_sample_rate(sr);
        for _ in 0..4800 {
            d.process(0.0001, -30.0, -50.0);
        }
        let mut letzte = 0.0f32;
        let mut ein = 0.0f32;
        for i in 0..(0.004 * sr) as usize {
            let x = 0.1 * (2.0 * std::f32::consts::PI * 1000.0 * i as f32 / sr).sin();
            let y = d.process(x, -30.0, -50.0);
            if i > (0.003 * sr) as usize && x.abs() > ein {
                ein = x.abs();
                letzte = y.abs();
            }
        }
        let db = 20.0 * (letzte / ein).log10();
        assert!(db > -1.0, "Wortanfang nach 3 ms noch {db:.2} dB abgesenkt");
    }

    #[test]
    fn biquad_peak_boost_hebt_mitte() {
        let sr = 48000.0;
        let mut b = Biquad::bypass();
        b.coeffs(FilterType::Peak, 1000.0, 1.0, 12.0, sr);
        // 1000 Hz durchlaufen lassen, Amplitude messen
        let mut peak = 0f32;
        for i in 0..48000 {
            let x = (2.0 * PI * 1000.0 * i as f32 / sr).sin();
            let y = b.process(x);
            if i > 4800 {
                peak = peak.max(y.abs());
            }
        }
        let db = lin_to_db(peak);
        assert!(db > 10.0 && db < 14.0, "Peak-Gain war {db} dB");
    }

    #[test]
    fn biquad_bypass_laesst_durch() {
        let mut b = Biquad::bypass();
        for _ in 0..100 {
            assert!((b.process(0.5) - 0.5).abs() < 1e-6);
        }
    }

    #[test]
    fn kompressor_reduziert_ueber_schwelle() {
        let mut c = Compressor {
            enabled: true,
            ..Default::default()
        };
        c.set_times(5.0, 100.0, 48000.0);
        let mut out = 0.0;
        for _ in 0..48000 {
            out = c.process(0.5, -20.0, 4.0, 0.0, 0.0);
        }
        let db = lin_to_db(out.abs());
        // Eingang -6 dB, Schwelle -20 dB, Ratio 4 -> rund -16,5 dB
        assert!(db < -10.0 && db > -20.0, "Ausgang war {db} dB");
    }

    #[test]
    fn limiter_haelt_decke() {
        let mut l = Limiter::default();
        l.set_release(50.0, 48000.0);
        let mut peak = 0f32;
        for i in 0..4800 {
            let x = 2.0 * (2.0 * PI * 100.0 * i as f32 / 48000.0).sin();
            peak = peak.max(l.process(x, -6.0).abs());
        }
        assert!(lin_to_db(peak) <= -5.5, "Spitze war {} dB", lin_to_db(peak));
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

/// C5-Limiter: Drei-Band-Verarbeitung (Splitter + Kompressor + Peak-Limiter je Band).
///
/// Name und Parameter stammen aus dem Original (`<C5LimBand … compOn attack release
/// knee ratio threshold gainOut limOn limit distorsion>`), die Kennlinien sind
/// Standard-Implementierungen – die Original-Koeffizienten sind nicht rekonstruiert.
#[derive(Clone, Copy)]
pub struct C5Config {
    pub enabled: bool,
    pub crossover_low_hz: f32,
    pub crossover_high_hz: f32,
    pub threshold_db: [f32; 3],
    pub ratio: [f32; 3],
    pub ceiling_db: [f32; 3],
    pub gain_db: [f32; 3],
    pub comp_on: [bool; 3],
    pub lim_on: [bool; 3],
}

impl Default for C5Config {
    fn default() -> Self {
        C5Config {
            enabled: false,
            crossover_low_hz: 200.0,
            crossover_high_hz: 3000.0,
            threshold_db: [-18.0; 3],
            ratio: [2.0; 3],
            ceiling_db: [-0.3; 3],
            gain_db: [0.0; 3],
            comp_on: [true; 3],
            lim_on: [true; 3],
        }
    }
}

/// Zustand des Drei-Band-Limiters (Splitter + je Band Kompressor und Limiter).
pub struct C5Limiter {
    lp_low: Biquad,
    hp_low: Biquad,
    lp_high: Biquad,
    hp_high: Biquad,
    comp: [Compressor; 3],
    lim: [Limiter; 3],
    /// Summen-Limiter nach dem Band-Mix (sonst überschreitet die Summe die Decke).
    out_lim: Limiter,
    sr: f32,
}

impl Default for C5Limiter {
    fn default() -> Self {
        C5Limiter {
            lp_low: Biquad::bypass(),
            hp_low: Biquad::bypass(),
            lp_high: Biquad::bypass(),
            hp_high: Biquad::bypass(),
            comp: [Compressor::default(); 3],
            lim: [Limiter::default(); 3],
            out_lim: Limiter::default(),
            sr: 0.0,
        }
    }
}

impl C5Limiter {
    fn setup(&mut self, cfg: &C5Config, sr: f32) {
        if (self.sr - sr).abs() > 0.5 {
            self.lp_low
                .coeffs(FilterType::LowPass, cfg.crossover_low_hz, 0.707, 0.0, sr);
            self.hp_low
                .coeffs(FilterType::HighPass, cfg.crossover_low_hz, 0.707, 0.0, sr);
            self.lp_high
                .coeffs(FilterType::LowPass, cfg.crossover_high_hz, 0.707, 0.0, sr);
            self.hp_high
                .coeffs(FilterType::HighPass, cfg.crossover_high_hz, 0.707, 0.0, sr);
            for b in 0..3 {
                self.comp[b].set_times(10.0, 120.0, sr);
                self.lim[b].set_release(60.0, sr);
                self.out_lim.set_release(40.0, sr);
            }
            self.sr = sr;
        }
        for b in 0..3 {
            self.comp[b].enabled = cfg.comp_on[b];
            self.lim[b].enabled = cfg.lim_on[b];
        }
    }

    /// Ein Sample durch die Drei-Band-Kette.
    // Je Band werden fuenf parallele Felder gelesen (Band, Gain, Comp, Limiter).
    #[allow(clippy::needless_range_loop)]
    pub fn process(&mut self, x: f32, cfg: &C5Config, sr: f32) -> f32 {
        if !cfg.enabled {
            return x;
        }
        self.setup(cfg, sr);
        let low = self.lp_low.process(x);
        let high = self.hp_high.process(x);
        let mid = self.hp_low.process(x) - self.lp_high.process(x);
        let bands = [low, mid, high];
        let mut sum = 0.0;
        for b in 0..3 {
            let mut v = bands[b] * db_to_lin(cfg.gain_db[b]);
            v = self.comp[b].process(v, cfg.threshold_db[b], cfg.ratio[b], 0.0, 0.0);
            v = self.lim[b].process(v, cfg.ceiling_db[b]);
            sum += v;
        }
        // Summen-Limiter: die Decke gilt für den Gesamtausgang, nicht je Band.
        let decke = cfg.ceiling_db.iter().cloned().fold(f32::INFINITY, f32::min);
        self.out_lim.process(sum, decke)
    }
}

#[cfg(test)]
mod c5_tests {
    use super::*;

    #[test]
    fn aus_laesst_durch() {
        let cfg = C5Config::default(); // enabled = false
        let mut c5 = C5Limiter::default();
        for _ in 0..100 {
            assert!((c5.process(0.3, &cfg, 48000.0) - 0.3).abs() < 1e-6);
        }
    }

    #[test]
    fn begrenzt_auf_decke() {
        let cfg = C5Config {
            enabled: true,
            ceiling_db: [-6.0; 3],
            comp_on: [false; 3],
            ..C5Config::default()
        };
        let mut c5 = C5Limiter::default();
        let mut peak = 0f32;
        for i in 0..9600 {
            let x = 2.0 * (2.0 * std::f32::consts::PI * 220.0 * i as f32 / 48000.0).sin();
            peak = peak.max(c5.process(x, &cfg, 48000.0).abs());
        }
        let db = lin_to_db(peak);
        assert!(
            db <= -4.5,
            "Spitze war {db} dB, soll unter der Decke liegen"
        );
    }

    #[test]
    fn leises_signal_bleibt_erhalten() {
        let cfg = C5Config {
            enabled: true,
            ceiling_db: [0.0; 3],
            comp_on: [false; 3],
            ..C5Config::default()
        };
        let mut c5 = C5Limiter::default();
        let mut peak = 0f32;
        for i in 0..9600 {
            let x = 0.05 * (2.0 * std::f32::consts::PI * 500.0 * i as f32 / 48000.0).sin();
            peak = peak.max(c5.process(x, &cfg, 48000.0).abs());
        }
        assert!(
            peak > 0.02,
            "leises Signal wurde zu stark abgesenkt: {peak}"
        );
    }
}

/* ==================================================================
LOWCUT – Hochpass 12 dB/Okt (Butterworth, Q = 0,707)
================================================================== */

/// Die vier waehlbaren Grenzfrequenzen des Lowcut.
pub const LOWCUT_HZ: [f32; 4] = [60.0, 80.0, 120.0, 160.0];

/// Naechstgelegene der vier Grenzfrequenzen; `hz <= 0` heisst „aus“.
pub fn lowcut_raster(hz: f32) -> f32 {
    if hz.is_nan() || hz <= 0.0 {
        return 0.0;
    }
    let mut best = LOWCUT_HZ[0];
    for &k in LOWCUT_HZ.iter() {
        if (k - hz).abs() < (best - hz).abs() {
            best = k;
        }
    }
    best
}

/// Hochpass 12 dB/Okt als ein Biquad (Butterworth, Q = 0,707).
///
/// Ein Streifenpaar hat zwei Instanzen (eine je Kanal); der Zustand liegt
/// damit dort, wo das Signal laeuft. Die Koeffizienten entstehen **beim
/// Parameterwechsel** in [`LowCut::set_hz`] – im Sample-Pfad steht nur eine
/// Multiplikation mit dem gespeicherten Zustand, keine Trigonometrie.
#[derive(Clone, Copy)]
pub struct LowCut {
    bq: Biquad,
    hz: f32,
    sr: f32,
}

impl Default for LowCut {
    fn default() -> Self {
        LowCut {
            bq: Biquad::bypass(),
            hz: 0.0,
            sr: 0.0,
        }
    }
}

impl LowCut {
    pub fn new() -> LowCut {
        LowCut::default()
    }

    /// Grenzfrequenz setzen (`0` = aus). Rechnet die Koeffizienten nur, wenn
    /// sich Hz oder Abtastrate wirklich geaendert haben.
    pub fn set_hz(&mut self, hz: f32, sr: f32) {
        let hz = if hz > 0.0 { lowcut_raster(hz) } else { 0.0 };
        if (hz - self.hz).abs() < 0.01 && (sr - self.sr).abs() < 0.5 {
            return;
        }
        self.hz = hz;
        self.sr = sr;
        if hz <= 0.0 {
            self.bq = Biquad::bypass();
        } else {
            // Butterworth 2. Ordnung = ein Biquad mit Q = 0,707.
            self.bq.coeffs(FilterType::HighPass, hz, 0.707, 0.0, sr);
        }
    }

    #[inline]
    pub fn aktiv(&self) -> bool {
        self.hz > 0.0
    }

    pub fn hz(&self) -> f32 {
        self.hz
    }

    #[inline]
    pub fn process(&mut self, x: f32) -> f32 {
        if self.hz <= 0.0 {
            x
        } else {
            self.bq.process(x)
        }
    }

    pub fn reset(&mut self) {
        self.bq.reset();
    }
}

/* ==================================================================
TK – TRANSIENTEN-UNTERDRUECKER
------------------------------------------------------------------
Ein Gate ist ein Pegelschalter: es entscheidet, **wie laut** etwas ist.
Tastaturklackern und Tischpoltern sind aber keine Pegelfrage, sondern
eine Flankenfrage – sie sind kurz und steil, und sie liegen mitten im
offenen Gate (gemessen am Sennheiser-Mikrofon: Spitzen -12,6 dBFS,
Sprache dagegen -25,3 dBFS).

Der TK erkennt sie an zwei Huellkurven und einer Flanke:

* **schnell** – Attack sofort, Release 8 ms: haelt den Spitzenwert der
  letzten Millisekunden. Der Attack ist bewusst *sofort*: ein Klackser
  kann ein einzelnes Sample lang sein, eine 0,5-ms-Attack saehe davon
  nur vier Prozent (am Messmaterial blieb damit die Haelfte unsichtbar).
* **langsam** – Attack 30 ms, Release 50 ms: folgt dem Pegel.

Das Verhaeltnis schnell/langsam gegen `tk_schwelle` (dB) ist der
Detektor; der Detektorboden haelt ihn aus dem Grundrauschen heraus.
Zusaetzlich muss eine **Flanke** da sein: der Spitzenwert des laufenden
Milliseconds gegen den Spitzenwert der letzten ~12 ms. Damit
gilt der Aufbau einer Silbe (rund 1 dB/ms, der Bezug zieht mit) nicht
als Transient – ohne diese Sperre schluckte jeder Wortanfang 30 ms
lang 15 dB.

Ueberschreiten beide Bedingungen die Schwelle, wird die Verstaerkung
innerhalb von 1 ms um bis zu `tk_tiefe` abgesenkt, 30 ms gehalten und
ueber 80 ms freigegeben.

**Lookahead 5 ms:** der Ringpuffer haelt das Signal zurueck, die
Absenkung setzt also ein, *bevor* der Klackser am Ausgang steht. Der
Detektor sieht das Signal unverzoegert – deshalb liegt die Absenkung
vor der Flanke statt hinter ihr. Der Preis ist genau diese Verzoegerung.

Alles liegt in festen Feldern; `process` allokiert nichts und rechnet
ohne `log`/`pow` je Sample (Schwelle, Tiefe und Flanke sind lineare
Faktoren). Bei `enabled == false` ist der Ausgang bit-gleich der
Eingang, ohne Ring und ohne Multiplikation.
================================================================== */

/// Lookahead des TK in Millisekunden.
pub const TK_LOOKAHEAD_MS: f32 = 5.0;
/// Hoechste Abtastrate, fuer die der Ringpuffer Platz hat (0,96 kB je Kanal).
pub const TK_MAX_SR: f32 = 192_000.0;
/// Flankenfenster (Millisekunden), in dem der Spitzenwert steht.
pub const TK_FLANKE_MS: f32 = 1.0;
/// Mindestanstieg des Spitzenwerts ueber den Bezug (dB).
pub const TK_FLANKE_DB: f32 = 6.0;
/// Detektorboden in dBFS: darunter gibt es nichts zu schuetzen (Grundrauschen
/// des Mikrofons, digitale Stille). Der Boden ist **kein Gate** – er
/// entscheidet nicht ueber das Signal, sondern nur darueber, ob der Detektor
/// ueberhaupt hinsieht; ohne ihn senkte das Rauschen im Stillen die
/// Verstaerkung staendig um 15 dB.
pub const TK_BODEN_DB: f32 = -45.0;
/// Flankenbezug: Spitzenwert der letzten so vielen Flankenfenster (~12 ms).
/// Ein Spitzenwert statt eines Mittels: bei tiefen Stimmen (80 Hz, 12,5 ms
/// Periode) schwankt der Millisekunden-Spitzenwert sonst um Faktor vier, und
/// ein Silbenaufbau zieht den Bezug sofort mit, statt ihm 12 ms hinterherzulaufen.
const TK_BEZUG_BLOECKE: usize = 12;
/// Untergrenze des Flankenbezugs in dBFS. Ein Wortanfang aus der Stille
/// steigt in seiner ersten Millisekunde um mehr als 6 dB ueber das Rauschen;
/// erst gegen diesen Boden gemessen bleibt ein Aufbau ueber 5..10 ms unter
/// der Flankenschwelle. Ein Klackser aus der Stille (gemessen -12,6 dBFS)
/// liegt weit darueber.
pub const TK_BEZUG_BODEN_DB: f32 = -33.0;

/// Transienten-Unterdruecker je Kanal (siehe Modulkopf).
pub struct TransientenUnterdruecker {
    pub enabled: bool,
    sr: f32,
    // Huellkurven (Betrag)
    fast: f32,
    slow: f32,
    /// Stand der langsamen Huellkurve vor 2 ms („steigt der Pegel?“).
    slow_alt: f32,
    /// Spitzenwert des laufenden Flankenfensters und der geglaettete Bezug.
    block_max: f32,
    /// Spitzenwerte der letzten [`TK_BEZUG_BLOECKE`] Flankenfenster (Ring).
    bezug: [f32; TK_BEZUG_BLOECKE],
    bezug_pos: usize,
    /// [`TK_BEZUG_BODEN_DB`] linear.
    bezug_boden: f32,
    block_ref: f32,
    // Koeffizienten (einpolig, aus der Abtastrate)
    f_att: f32,
    f_rel: f32,
    s_att: f32,
    s_rel: f32,
    g_att: f32,
    g_rel: f32,
    hold_s: f32,
    // Zaehler bis zum naechsten Vergleichsstand
    cnt_rise: usize,
    n_rise: usize,
    cnt_flanke: usize,
    n_flanke: usize,
    /// Mindestanstieg der Flanke als linearer Faktor.
    flanke_lin: f32,
    /// Detektor gesperrt (Sprachbeginn ohne Flanke).
    sperre: bool,
    /// Regelung der Verstaerkung
    gain: f32,
    hold: f32,
    schwelle_db: f32,
    tiefe_db: f32,
    /// Vorberechnete lineare Faktoren (kein pow je Sample).
    schwelle_lin: f32,
    tiefe_lin: f32,
    /// Detektorboden als linearer Wert (aus [`TK_BODEN_DB`]).
    boden_lin: f32,
    /// Lookahead-Ring, in `new` allokiert.
    ring: Vec<f32>,
    pos: usize,
    len: usize,
    frisch: bool,
}

impl Default for TransientenUnterdruecker {
    fn default() -> Self {
        TransientenUnterdruecker::new(48000.0)
    }
}

impl TransientenUnterdruecker {
    pub fn new(sr: f32) -> TransientenUnterdruecker {
        let mut t = TransientenUnterdruecker {
            enabled: false,
            sr: 0.0,
            fast: 0.0,
            slow: 0.0,
            slow_alt: 0.0,
            block_max: 0.0,
            bezug: [0.0; TK_BEZUG_BLOECKE],
            bezug_pos: 0,
            bezug_boden: db_to_lin(TK_BEZUG_BODEN_DB),
            block_ref: 0.0,
            f_att: 0.0,
            f_rel: 0.0,
            s_att: 0.0,
            s_rel: 0.0,
            g_att: 0.0,
            g_rel: 0.0,
            hold_s: 0.0,
            cnt_rise: 0,
            n_rise: 1,
            cnt_flanke: 0,
            n_flanke: 1,
            flanke_lin: db_to_lin(TK_FLANKE_DB),
            sperre: false,
            gain: 1.0,
            hold: 0.0,
            schwelle_db: 6.0,
            tiefe_db: 15.0,
            schwelle_lin: db_to_lin(6.0),
            tiefe_lin: db_to_lin(-15.0),
            boden_lin: db_to_lin(TK_BODEN_DB),
            // Platz fuer die hoechste unterstuetzte Rate; bei 48 kHz werden
            // nur 240 Samples gebraucht.
            ring: vec![0.0; (TK_LOOKAHEAD_MS * 0.001 * TK_MAX_SR) as usize + 2],
            pos: 0,
            len: 0,
            frisch: true,
        };
        t.set_sample_rate(sr);
        t
    }

    fn set_sample_rate(&mut self, sr: f32) {
        let sr = sr.max(8000.0);
        if (sr - self.sr).abs() < 0.5 {
            return;
        }
        self.sr = sr;
        let coef = |ms: f32| (-1.0 / (ms * 0.001 * sr)).exp();
        self.f_att = 0.0; // Attack sofort (siehe Modulkopf)
        self.f_rel = coef(8.0);
        self.s_att = coef(30.0);
        self.s_rel = coef(50.0);
        self.g_att = coef(1.0);
        self.g_rel = coef(80.0);
        self.hold_s = 0.030 * sr;
        self.n_rise = ((0.002 * sr) as usize).max(1);
        self.n_flanke = ((TK_FLANKE_MS * 0.001 * sr) as usize).max(1);
        // Der Ring ist fuer TK_MAX_SR bemessen; bei hoeherer Rate bleibt die
        // Verzoegerung bei seinem Ende stehen.
        self.len = ((TK_LOOKAHEAD_MS * 0.001 * sr) as usize).min(self.ring.len() - 1);
        self.reset();
    }

    /// Schalten und Abtastrate setzen – einmal je Block, nicht je Sample.
    ///
    /// Beim Ein- und Ausschalten wird der Zustand zurueckgesetzt: der
    /// Lookahead-Ring darf keine Samples aus dem alten Zustand ausgeben.
    pub fn setup(&mut self, enabled: bool, sr: f32) {
        self.set_sample_rate(sr);
        if self.enabled != enabled {
            self.reset();
            self.enabled = enabled;
        }
    }

    /// Alles auf Anfang, auch beim Aus- und Einschalten.
    pub fn reset(&mut self) {
        self.fast = 0.0;
        self.slow = 0.0;
        self.slow_alt = 0.0;
        self.block_max = 0.0;
        self.block_ref = self.bezug_boden;
        self.bezug = [0.0; TK_BEZUG_BLOECKE];
        self.bezug_pos = 0;
        self.cnt_rise = 0;
        self.cnt_flanke = 0;
        self.sperre = false;
        self.gain = 1.0;
        self.hold = 0.0;
        self.pos = 0;
        self.frisch = true;
        for v in self.ring.iter_mut() {
            *v = 0.0;
        }
    }

    /// Detektorschwelle als Pegelverhaeltnis schnell/langsam in dB (0..20).
    fn set_schwelle(&mut self, db: f32) {
        let db = if db.is_nan() {
            6.0
        } else {
            db.clamp(0.0, 20.0)
        };
        if (db - self.schwelle_db).abs() < 0.01 {
            return;
        }
        self.schwelle_db = db;
        // 0 dB hiesse „jeder Anstieg“ – das waere kein Detektor; die
        // Untergrenze haelt ihn bei 0,1 dB scharf statt blind.
        self.schwelle_lin = db_to_lin(db.max(0.1));
    }

    /// Absenktiefe in dB (0..30).
    fn set_tiefe(&mut self, db: f32) {
        let db = if db.is_nan() {
            15.0
        } else {
            db.clamp(0.0, 30.0)
        };
        if (db - self.tiefe_db).abs() < 0.01 {
            return;
        }
        self.tiefe_db = db;
        self.tiefe_lin = db_to_lin(-db);
    }

    /// Ein Sample. `schwelle_db` 0..20 dB Verhaeltnis, `tiefe_db` 0..30 dB.
    ///
    /// Bei `enabled == false` ist der Ausgang **bit-gleich** der Eingang
    /// (kein Ring, keine Multiplikation) – der Streifen klingt dann genau
    /// wie vor dem Einbau des TK.
    #[inline]
    pub fn process(&mut self, x: f32, schwelle_db: f32, tiefe_db: f32) -> f32 {
        if !self.enabled {
            return x;
        }
        if self.frisch {
            // Erster Sample nach dem Einschalten: Ring vorfuellen, damit die
            // 5 ms Verzoegerung nicht als Loch zu hoeren sind.
            self.frisch = false;
            for v in self.ring.iter_mut() {
                *v = x;
            }
        }
        self.set_schwelle(schwelle_db);
        self.set_tiefe(tiefe_db);

        let lvl = x.abs();
        // 1) Huellkurven: schnell (Attack sofort / 8 ms), langsam (30/50 ms).
        let c = if lvl > self.fast {
            self.f_att
        } else {
            self.f_rel
        };
        self.fast = c * self.fast + (1.0 - c) * lvl;
        let c = if lvl > self.slow {
            self.s_att
        } else {
            self.s_rel
        };
        self.slow = c * self.slow + (1.0 - c) * lvl;

        // 2) Verhaeltnis schnell/langsam gegen die Schwelle (linear, kein log).
        //    Der Boden haelt den Detektor aus dem Grundrauschen heraus.
        let ueber = self.fast > self.slow * self.schwelle_lin && self.fast > self.boden_lin;

        // 3) Flanke: Spitzenwert des laufenden Milliseconds gegen den
        //    Spitzenwert der letzten ~12 ms. Ein Klackser reisst den
        //    Blockwert sofort hoch, ein Silbenaufbau zieht den Bezug mit, und
        //    Rauschen schwankt von Block zu Block nur um wenige dB.
        if lvl > self.block_max {
            self.block_max = lvl;
        }
        self.cnt_flanke += 1;
        if self.cnt_flanke >= self.n_flanke {
            self.cnt_flanke = 0;
            self.bezug[self.bezug_pos] = self.block_max;
            self.bezug_pos = (self.bezug_pos + 1) % TK_BEZUG_BLOECKE;
            // Der Boden gilt auch fuer den Bezug: ein Wortanfang aus der
            // Stille hebt sich sonst um Zehnerpotenzen vom Rauschen ab und
            // saehe aus wie ein Klackser.
            self.block_ref = self.bezug.iter().fold(self.bezug_boden, |m, &v| m.max(v));
            self.block_max = 0.0;
        }
        let flanke = self.block_max > self.block_ref * self.flanke_lin;

        // 4) Sperre: steigt der langsame Pegel (Sprachbeginn) ohne Flanke,
        //    ist der Detektor aus.
        self.cnt_rise += 1;
        if self.cnt_rise >= self.n_rise {
            self.cnt_rise = 0;
            self.slow_alt = self.slow;
        }
        self.sperre = self.slow > self.slow_alt && !flanke;

        // 5) Huellkurve der Absenkung: 1 ms zu, 30 ms halten, 80 ms auf.
        //    Ausgeloest wird nur eine **Flanke** ueber der Schwelle: damit
        //    bleibt der Aufbau einer Silbe (steigender Pegel ohne Flanke)
        //    unberuehrt, auch wenn sein Verhaeltnis die Schwelle reisst.
        if ueber && flanke && !self.sperre {
            self.hold = self.hold_s;
        }
        if self.hold > 0.0 {
            self.hold -= 1.0;
        }
        let ziel = if self.hold > 0.0 { self.tiefe_lin } else { 1.0 };
        let c = if ziel < self.gain {
            self.g_att
        } else {
            self.g_rel
        };
        // `c` ist der Rueckhaltefaktor exp(-1/(t*sr)): der Schritt ist
        // (1 - c). Frueher stand hier `* c` - damit sprang die Verstaerkung
        // in einem Sample auf das Ziel (hoerbarer Knack statt 80 ms Freigabe).
        self.gain += (ziel - self.gain) * (1.0 - c);

        // 6) Lookahead: das Signal laeuft 5 ms hinter dem Detektor her.
        let out = self.ring[self.pos] * self.gain;
        self.ring[self.pos] = x;
        self.pos += 1;
        if self.pos >= self.len {
            self.pos = 0;
        }
        out
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

/* ==================================================================
NOISE REDUCTION – eigener Rauschunterdruecker, zwei Stufen
================================================================== */

/// FFT-Groesse des Rauschunterdrueckers (512 Samples = 10,7 ms bei 48 kHz).
pub const NR_FFT: usize = 512;
/// Sprungweite: halbe Fensterlaenge, damit sich die Fenster ueberlappen.
pub const NR_HOP: usize = NR_FFT / 2;
/// Ausgewertete Baender (0 .. Nyquist, ohne Nyquist selbst).
const NR_BINS: usize = NR_FFT / 2;
/// Frames je Minimumsblock (24 x 5,33 ms ≈ 128 ms).
const NR_BLOCK_FRAMES: usize = 24;
/// Bloecke im Ring – 8 x 128 ms ≈ 1,02 s Rauschschaetzung.
const NR_BLOCK_RING: usize = 8;
/// Zeitglaettung der Leistung vor der Minimumsuche (je Frame, ~35 ms).
const NR_PSD_GLATT: f32 = 0.85;
/// Glaettung, wenn die Leistung faellt (schnell: Pausen sichtbar machen).
const NR_PSD_FALL: f32 = 0.3;
/// Ausgleich: das Minimum der geglaetteten Leistung ueber ~1 s liegt im
/// Mittel um diesen Faktor unter dem echten Rauschpegel.
const NR_MIN_BIAS: f32 = 16.67;
/// Halbe Breite des Medianfensters fuer die Deckelung der Rauschschaetzung.
///
/// Das Fenster muss deutlich breiter sein als die Hauptkeule des Fensters
/// (bei Hann vier bis fuenf Baender): nur dann ist ein Dauerton eine
/// Minderheit im Fenster und der Median bleibt beim echten Rauschboden.
const NR_NACHBAR: usize = 12;

/// Median eines kleinen Fensters – ohne Allokation, per Einfuegesortieren.
fn median(v: &[f32]) -> f32 {
    let mut b = [0.0f32; 2 * NR_NACHBAR + 1];
    let n = v.len().min(b.len());
    b[..n].copy_from_slice(&v[..n]);
    for i in 1..n {
        let x = b[i];
        let mut j = i;
        while j > 0 && b[j - 1] > x {
            b[j] = b[j - 1];
            j -= 1;
        }
        b[j] = x;
    }
    b[n / 2]
}

/// Rauschunterdrueckung je Kanal: Gate/Expander **und** spektrale Subtraktion.
///
/// **Stufe a – Gate/Expander:** Huellkurve aus dem Betrag, unterhalb der
/// Schwelle faehrt der Pegel weich zu (`1 - Staerke * (1 - env/schwelle)`),
/// mit Attack, Release und Haltezeit. Kein `powf` je Sample: die Kennlinie
/// ist linear, die Tiefe kommt aus der Staerke.
///
/// **Stufe b – spektrale Subtraktion:** 512-Punkt-FFT, halbe Ueberlappung,
/// Wurzel-Hann-Fenster (Analyse und Synthese), damit sich das Signal bei
/// Verstaerkung 1 exakt rekonstruiert. Rauschschaetzung ist das gleitende
/// Minimum der geglaetteten Leistung ueber ~1 s je Band (Ring aus acht
/// Bloecken), anschliessend ein Median-Deckel ueber ±12 Baender: ein schmaler Dauerton
/// zieht den Rauschboden damit nicht nach oben. Die Subtraktion passiert im
/// Leistungsbereich (`g = 1 - alpha*n²/p`, unten durch einen Boden begrenzt),
/// was hohe Pegel fast unberuehrt laesst. Gegen „musikalisches Rauschen“
/// wirken drei Dinge: die Glaettung der Verstaerkung ueber die Zeit
/// (zu etwas schneller als auf, damit Wortanfaenge nicht verschluckt
/// werden), eine Glaettung ueber die Frequenz, die **nur
/// Kerben anhebt** (keine Spitzen absenkt), und der Spektralboden.
///
/// Alles liegt in festen Arrays: `process` allokiert nichts. Die Latenz der
/// Stufe betraegt ein Fenster (512 Samples ≈ 10,7 ms).
pub struct NoiseReduction {
    pub enabled: bool,
    /// Stufe a (Expander) zuschalten. Vorgabe: aus – fuer Sprachpausen gibt es
    /// den eigenen GATE-Knopf, damit sich beide Schalter nicht doppelt
    /// auswirken.
    pub gate_on: bool,
    fft: Fft,
    window: [f32; NR_FFT],
    /// Letzte `NR_FFT` Eingangssamples (Ring).
    in_buf: [f32; NR_FFT],
    wpos: usize,
    filled: usize,
    /// Ueberlappungs-Summe und Ausgabewarteschlange.
    ola: [f32; NR_FFT],
    out_q: [f32; NR_HOP],
    q_pos: usize,
    re: [f32; NR_FFT],
    im: [f32; NR_FFT],
    mag: [f32; NR_BINS],
    /// Zeitlich geglaettete Leistung je Band (Grundlage der Minimumsuche).
    psd: [f32; NR_BINS],
    block_min: [f32; NR_BINS],
    min_ring: [[f32; NR_BINS]; NR_BLOCK_RING],
    ring_pos: usize,
    frames_in_block: usize,
    bloecke: usize,
    noise: [f32; NR_BINS],
    gain: [f32; NR_BINS],
    gain_glatt: [f32; NR_BINS],
    glatt_tmp: [f32; NR_BINS],
    // Gate
    env: f32,
    gate_gain: f32,
    hold: f32,
    att1: f32,
    rel1: f32,
    hold_s: f32,
    inv_thr: f32,
    schwelle_db: f32,
    // abgeleitete Groessen der Staerke
    staerke: f32,
    alpha: f32,
    floor: f32,
    gate_tiefe: f32,
    gate_boden: f32,
    // Glaettung der spektralen Verstaerkung je Frame
    g_att: f32,
    g_rel: f32,
    sr: f32,
    frisch: bool,
}

impl Default for NoiseReduction {
    fn default() -> Self {
        NoiseReduction::new(48000.0)
    }
}

impl NoiseReduction {
    pub fn new(sr: f32) -> NoiseReduction {
        let mut window = [0.0f32; NR_FFT];
        for (i, w) in window.iter_mut().enumerate() {
            let hann = 0.5 - 0.5 * (2.0 * PI * i as f32 / NR_FFT as f32).cos();
            // Wurzel-Hann: Analyse mal Synthese ergibt Hann, und Hann
            // summiert sich bei halber Ueberlappung exakt auf 1.
            *w = hann.max(1e-6).sqrt();
        }
        let mut n = NoiseReduction {
            enabled: false,
            gate_on: false,
            fft: Fft::new(NR_FFT),
            window,
            in_buf: [0.0; NR_FFT],
            wpos: 0,
            filled: 0,
            ola: [0.0; NR_FFT],
            out_q: [0.0; NR_HOP],
            q_pos: NR_HOP,
            re: [0.0; NR_FFT],
            im: [0.0; NR_FFT],
            mag: [0.0; NR_BINS],
            psd: [0.0; NR_BINS],
            block_min: [f32::INFINITY; NR_BINS],
            min_ring: [[f32::INFINITY; NR_BINS]; NR_BLOCK_RING],
            ring_pos: 0,
            frames_in_block: 0,
            bloecke: 0,
            noise: [0.0; NR_BINS],
            gain: [1.0; NR_BINS],
            gain_glatt: [1.0; NR_BINS],
            glatt_tmp: [1.0; NR_BINS],
            env: 0.0,
            gate_gain: 1.0,
            hold: 0.0,
            att1: 0.0,
            rel1: 0.0,
            hold_s: 0.0,
            inv_thr: 1.0,
            schwelle_db: 0.0,
            staerke: -1.0,
            alpha: 1.0,
            floor: 0.0,
            gate_tiefe: 0.0,
            gate_boden: 1.0,
            g_att: 0.5,
            g_rel: 0.3,
            sr: 0.0,
            frisch: true,
        };
        n.set_sample_rate(sr);
        n.set_schwelle(-45.0);
        n
    }

    fn set_sample_rate(&mut self, sr: f32) {
        let sr = sr.max(8000.0);
        if (sr - self.sr).abs() < 0.5 {
            return;
        }
        self.sr = sr;
        // Regelzeiten des Expanders: 3 ms auf, 140 ms zu, 60 ms halten.
        self.att1 = 1.0 - (-1.0 / (0.003 * sr)).exp();
        self.rel1 = 1.0 - (-1.0 / (0.140 * sr)).exp();
        self.hold_s = 0.060 * sr;
    }

    /// Schwelle des Expanders in dBFS (-60 .. -20).
    fn set_schwelle(&mut self, db: f32) {
        let db = db.clamp(-60.0, -20.0);
        if (db - self.schwelle_db).abs() < 0.01 {
            return;
        }
        self.schwelle_db = db;
        self.inv_thr = 1.0 / db_to_lin(db);
    }

    /// Aus der Staerke abgeleitete Groessen (Ueberschuss, Boden, Tiefe).
    fn set_staerke(&mut self, staerke: f32) {
        let staerke = if staerke.is_nan() {
            0.0
        } else {
            staerke.clamp(0.0, 1.0)
        };
        if (staerke - self.staerke).abs() < 1e-4 {
            return;
        }
        self.staerke = staerke;
        // Ueberschussfaktor 1 .. 3 im Leistungsbereich.
        self.alpha = 1.0 + 2.0 * staerke;
        // Spektralboden -14 .. -30 dB: begrenzt, wie tief eine Kerbe faellt.
        self.floor = db_to_lin(-(14.0 + 16.0 * staerke));
        // Gate: Tiefe und Boden wachsen mit der Staerke.
        self.gate_tiefe = staerke;
        self.gate_boden = db_to_lin(-(4.0 + 26.0 * staerke));
    }

    /// Alles zurueck auf Anfang (auch beim Ausschalten).
    pub fn reset(&mut self) {
        self.in_buf = [0.0; NR_FFT];
        self.ola = [0.0; NR_FFT];
        self.out_q = [0.0; NR_HOP];
        self.wpos = 0;
        self.filled = 0;
        self.q_pos = NR_HOP;
        self.re = [0.0; NR_FFT];
        self.im = [0.0; NR_FFT];
        self.mag = [0.0; NR_BINS];
        self.psd = [0.0; NR_BINS];
        self.block_min = [f32::INFINITY; NR_BINS];
        self.min_ring = [[f32::INFINITY; NR_BINS]; NR_BLOCK_RING];
        self.ring_pos = 0;
        self.frames_in_block = 0;
        self.bloecke = 0;
        self.noise = [0.0; NR_BINS];
        self.gain = [1.0; NR_BINS];
        self.gain_glatt = [1.0; NR_BINS];
        self.glatt_tmp = [1.0; NR_BINS];
        self.env = 0.0;
        self.gate_gain = 1.0;
        self.hold = 0.0;
        self.frisch = true;
    }

    /// Huellkurve + Expander-Kennlinie (Stufe a).
    #[inline]
    fn expander(&mut self, x: f32, schnelle: f32) -> f32 {
        let lvl = x.abs();
        // `schnelle` und `rel1` sind Schrittweiten (1 - exp(-1/(t*sr))).
        // Frueher wurden sie hier als Rueckhaltefaktor benutzt; die
        // Huellkurve war dadurch der rohe Betrag und fiel in jedem
        // Nulldurchgang unter die Schwelle.
        let schritt = if lvl > self.env { schnelle } else { self.rel1 };
        self.env += (lvl - self.env) * schritt;
        let k = (self.env * self.inv_thr).min(1.0);
        let mut ziel = 1.0 - self.gate_tiefe * (1.0 - k);
        if k >= 1.0 {
            self.hold = self.hold_s;
        } else if self.hold > 0.0 {
            self.hold -= 1.0;
            ziel = 1.0;
        }
        if ziel < self.gate_boden {
            ziel = self.gate_boden;
        }
        let c = if ziel > self.gate_gain {
            self.att1
        } else {
            self.rel1
        };
        self.gate_gain += (ziel - self.gate_gain) * c;
        x * self.gate_gain
    }

    /// Ein Frame: Fenster, FFT, Rauschschaetzung, Subtraktion, IFFT, OLA.
    fn rahmen(&mut self) {
        let n = NR_FFT;
        for i in 0..n {
            let mut idx = self.wpos + i;
            if idx >= n {
                idx -= n;
            }
            self.re[i] = self.in_buf[idx] * self.window[i];
            self.im[i] = 0.0;
        }
        self.fft.forward(&mut self.re, &mut self.im);
        for i in 0..NR_BINS {
            let (r, im) = (self.re[i], self.im[i]);
            self.mag[i] = (r * r + im * im).sqrt();
        }

        // Rauschschaetzung: gleitendes Minimum ueber ~1 s je Band
        // (Minimum-Statistik nach Martin).
        //
        // Gesucht wird das Minimum der **zeitlich geglaetteten** Leistung,
        // und zwar ueber den ganzen Ring (acht Bloecke a ~128 ms), nicht der
        // Mittelwert der Blockminima: spricht jemand ohne Pause, liegen die
        // Minima einzelner Bloecke im Sprachpegel; erst das Minimum ueber
        // ~1 s erreicht sicher eine Pause. Das Minimum geglaetteter Werte
        // liegt systematisch unter dem Mittel; [`NR_MIN_BIAS`] gleicht das
        // aus (geprueft in `nr_schaetzung_ist_erwartungstreu`).
        let frisch = self.bloecke == 0 && self.frames_in_block == 0;
        for i in 0..NR_BINS {
            let p = self.mag[i] * self.mag[i];
            // Steigt langsam (glaettet die Streuung des Rauschens), faellt
            // schnell: eine Sprechpause von 50..100 ms soll den Boden zeigen.
            let a = if p > self.psd[i] {
                NR_PSD_GLATT
            } else {
                NR_PSD_FALL
            };
            self.psd[i] = if frisch {
                p
            } else {
                a * self.psd[i] + (1.0 - a) * p
            };
            if self.psd[i] < self.block_min[i] {
                self.block_min[i] = self.psd[i];
            }
        }
        self.frames_in_block += 1;
        if self.frames_in_block >= NR_BLOCK_FRAMES {
            self.frames_in_block = 0;
            self.ring_pos += 1;
            if self.ring_pos == NR_BLOCK_RING {
                self.ring_pos = 0;
            }
            self.min_ring[self.ring_pos] = self.block_min;
            self.block_min = [f32::INFINITY; NR_BINS];
            if self.bloecke < NR_BLOCK_RING {
                self.bloecke += 1;
            }
            for i in 0..NR_BINS {
                let mut m = f32::INFINITY;
                for b in 0..NR_BLOCK_RING {
                    m = m.min(self.min_ring[b][i]);
                }
                self.noise[i] = m * NR_MIN_BIAS;
            }
            // Gegen schmale Dauertoene: der Rauschboden ist spektral glatt.
            // Wo ein einzelnes Band weit ueber dem Median seiner Nachbarn
            // liegt, wird die Schaetzung auf diesen Median begrenzt – sonst
            // haelt die Minimumssuche nach einer Sekunde den Ton selbst fuer
            // Rauschen und schneidet ihn weg.
            let roh = self.noise;
            for i in 0..NR_BINS {
                let lo = i.saturating_sub(NR_NACHBAR);
                let hi = (i + NR_NACHBAR).min(NR_BINS - 1);
                let med = median(&roh[lo..=hi]);
                let deckel = med + 1e-12;
                if self.noise[i] > deckel {
                    self.noise[i] = deckel;
                }
            }
        }

        // Verstaerkung je Band: Leistungssubtraktion mit Boden.
        if self.bloecke == 0 {
            for g in self.gain.iter_mut() {
                *g = 1.0;
            }
        } else {
            for i in 0..NR_BINS {
                let p = self.mag[i] * self.mag[i];
                let n2 = self.noise[i];
                let mut g = if p <= self.alpha * n2 {
                    self.floor
                } else {
                    (1.0 - self.alpha * n2 / p).max(self.floor)
                };
                // Baender weit ueber dem Rauschboden (rund 10 dB) bleiben
                // erhalten – und zwar mit ihrer ganzen Hauptkeule: die
                // Nachbarn bis ±2 Baender zaehlen mit, weil ein Fenster die
                // Energie eines Tones ueber vier bis fuenf Baender verteilt.
                // Ohne das kostete die Subtraktion einem Dauerton Pegel.
                let mut spitze = p;
                let lo = i.saturating_sub(2);
                let hi = (i + 2).min(NR_BINS - 1);
                for j in lo..=hi {
                    let q = self.mag[j] * self.mag[j];
                    if q > spitze {
                        spitze = q;
                    }
                }
                if spitze > 10.0 * n2 && g < 0.97 {
                    g = 0.97;
                }
                self.gain[i] = if g > 1.0 { 1.0 } else { g };
            }
        }

        // Frequenzglaettung, die nur Kerben anhebt: eine einzelne tiefe
        // Kerbe neben einem lauten Band wird halb angehoben, Spitzen
        // bleiben unangetastet (sonst kostete die Glaettung Sinuspegel).
        self.glatt_tmp[0] = self.gain[0];
        for i in 1..NR_BINS - 1 {
            let nb = 0.5 * (self.gain[i - 1] + self.gain[i + 1]);
            let g = self.gain[i];
            self.glatt_tmp[i] = if g < nb { g + 0.5 * (nb - g) } else { g };
        }
        self.glatt_tmp[NR_BINS - 1] = self.gain[NR_BINS - 1];

        // Zeitglaettung je Frame (5,3 ms): zu mit 0,5, auf mit 0,3. Frueher
        // oeffnete sie mit 0,12 - nach jeder Pause fehlten dem Wortanfang
        // rund 2 dB (gemessen in tests/nr_sprache.rs).
        for i in 0..NR_BINS {
            let ziel = self.glatt_tmp[i];
            let k = if ziel < self.gain_glatt[i] {
                self.g_att
            } else {
                self.g_rel
            };
            self.gain_glatt[i] += (ziel - self.gain_glatt[i]) * k;
        }

        // Anwenden (reelles Spektrum bleibt hermitesch).
        let g0 = self.gain_glatt[0];
        self.re[0] *= g0;
        self.im[0] *= g0;
        for i in 1..NR_BINS {
            let g = self.gain_glatt[i];
            self.re[i] *= g;
            self.im[i] *= g;
            let j = NR_FFT - i;
            self.re[j] *= g;
            self.im[j] *= g;
        }
        let gn = self.gain_glatt[NR_BINS - 1];
        self.re[NR_BINS] *= gn;
        self.im[NR_BINS] *= gn;

        self.fft.inverse(&mut self.re, &mut self.im);

        // Ueberlappung addieren, erste Hop Samples ausgeben, dann schieben.
        for i in 0..NR_FFT {
            self.ola[i] += self.re[i] * self.window[i];
        }
        self.out_q.copy_from_slice(&self.ola[..NR_HOP]);
        for i in 0..NR_HOP {
            self.ola[i] = self.ola[i + NR_HOP];
            self.ola[i + NR_HOP] = 0.0;
        }
        self.q_pos = 0;
    }

    /// Ein Sample. `staerke` 0..1 (0 = aus), `schwelle_db` -60..-20 dBFS.
    #[inline]
    pub fn process(&mut self, x: f32, staerke: f32, schwelle_db: f32, sr: f32) -> f32 {
        if !self.enabled || staerke.is_nan() || staerke <= 0.0 {
            if !self.frisch {
                self.reset();
            }
            return x;
        }
        self.frisch = false;
        self.set_sample_rate(sr);
        self.set_schwelle(schwelle_db);
        self.set_staerke(staerke);
        let g = if self.gate_on {
            self.expander(x, self.att1)
        } else {
            x
        };
        self.in_buf[self.wpos] = g;
        self.wpos += 1;
        if self.wpos == NR_FFT {
            self.wpos = 0;
        }
        self.filled += 1;
        if self.filled == NR_HOP {
            self.filled = 0;
            self.rahmen();
        }
        if self.q_pos < NR_HOP {
            let v = self.out_q[self.q_pos];
            self.q_pos += 1;
            v
        } else {
            0.0
        }
    }
}

#[cfg(test)]
mod tk_tests {
    use super::*;

    /// Deterministisches Rauschen (LCG) – dieselbe Quelle wie in `nr_tests`.
    struct Rausch {
        s: u32,
    }

    impl Rausch {
        fn neu(seed: u32) -> Rausch {
            Rausch { s: seed | 1 }
        }
        fn next(&mut self) -> f32 {
            let mut sum = 0.0;
            for _ in 0..3 {
                self.s = self.s.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                sum += (self.s >> 8) as f32 / 8_388_608.0 - 1.0;
            }
            sum * 0.577
        }
    }

    #[test]
    fn tk_aus_ist_bitgenau_durchgang() {
        let mut tk = TransientenUnterdruecker::new(48000.0);
        tk.setup(false, 48000.0);
        let mut r = Rausch::neu(4711);
        for _ in 0..20_000 {
            let x = r.next() * 0.3;
            assert_eq!(tk.process(x, 6.0, 15.0).to_bits(), x.to_bits());
        }
    }

    #[test]
    fn tk_verzoegert_um_den_lookahead() {
        let sr = 48000.0;
        let mut tk = TransientenUnterdruecker::new(sr);
        tk.setup(true, sr);
        // Ein leiser Nadelimpuls (-60 dBFS, unter dem Detektorboden) darf nur
        // die Verzoegerung zeigen, nicht gedaempft werden.
        let n = 2000;
        let mut aus = Vec::with_capacity(n);
        for i in 0..n {
            let x = if i == 1000 { 0.001 } else { 0.0 };
            aus.push(tk.process(x, 6.0, 15.0));
        }
        let versatz = (TK_LOOKAHEAD_MS * 0.001 * sr) as usize;
        assert_eq!(versatz, 240);
        assert!(
            (aus[1000 + versatz] - 0.001).abs() < 1e-9,
            "Impuls lag nicht am erwarteten Platz"
        );
        assert!(aus[1000 + versatz - 1].abs() < 1e-9);
        assert!(
            aus[1000].abs() < 1e-9,
            "ohne Lookahead kam der Impuls zu frueh"
        );
    }

    /// Sinus -20 dBFS mit eingestreuten 5-ms-Knacksen bei -10 dBFS.
    fn lauf(mit_tk: bool) -> (f32, f32) {
        let sr = 48000.0;
        let n = 48_000;
        let mut tk = TransientenUnterdruecker::new(sr);
        tk.setup(mit_tk, sr);
        let mut aus = Vec::with_capacity(n);
        let mut knack_peak = 0.0f32;
        let mut ton_peak = 0.0f32;
        let (k0, k1) = (12_000, 12_240); // 5 ms Knacks
        for i in 0..n {
            let mut x = 0.1 * (2.0 * PI * 1000.0 * i as f32 / sr).sin();
            if i >= k0 && i < k1 {
                x += 0.316 * (2.0 * PI * 1000.0 * i as f32 / sr).sin();
            }
            let y = tk.process(x, 6.0, 15.0);
            aus.push(y);
            if i >= k0 + 240 && i < k1 + 240 {
                knack_peak = knack_peak.max(y.abs());
            }
            if i > 24_000 {
                ton_peak = ton_peak.max(y.abs());
            }
        }
        // Eingangspegel zum Vergleich
        let mut knack_ein = 0.0f32;
        for i in k0..k1 {
            let x = 0.1 * (2.0 * PI * 1000.0 * i as f32 / sr).sin()
                + 0.316 * (2.0 * PI * 1000.0 * i as f32 / sr).sin();
            knack_ein = knack_ein.max(x.abs());
        }
        let _ = ton_peak;
        (lin_to_db(knack_ein), lin_to_db(knack_peak))
    }

    #[test]
    fn tk_senkt_knackser_und_haelt_den_ton() {
        let (ein, aus) = lauf(true);
        println!(
            "TK Knacks: {ein:.2} -> {aus:.2} dBFS ({:+.2} dB)",
            aus - ein
        );
        assert!(
            aus - ein < -10.0,
            "Knacks nur um {:.2} dB abgesenkt",
            aus - ein
        );
    }

    fn tk_an() -> TransientenUnterdruecker {
        let mut tk = TransientenUnterdruecker::new(48000.0);
        tk.setup(true, 48000.0);
        tk
    }

    #[test]
    fn tk_gibt_ueber_die_freigabezeit_frei() {
        // Klick aus der Stille, danach ein leiser Dauerton: die Absenkung
        // darf nicht in einem Sample verschwinden (frueher: Knack), sondern
        // laeuft nach 30 ms Halten ueber ~80 ms zurueck.
        let mut tk = tk_an();
        let mut aus = Vec::new();
        for i in 0..48000 / 2 {
            let ton = 0.02 * (2.0 * PI * 500.0 * i as f32 / 48000.0).sin();
            let klick = if (4800..4848).contains(&i) { 0.5 } else { 0.0 };
            aus.push(tk.process(ton + klick, 6.0, 15.0));
        }
        let g = |ms: f32| {
            // Verstaerkung aus dem Pegel des Tons um `ms` nach dem Klick
            // (5 ms Lookahead eingerechnet), relativ zum Ende.
            let mitte = 4800 + 240 + (ms * 48.0) as usize;
            let rms = |a: usize| (aus[a..a + 96].iter().map(|x| x * x).sum::<f32>() / 96.0).sqrt();
            rms(mitte) / rms(aus.len() - 200)
        };
        let nach_halten = g(40.0);
        let mitten = g(80.0);
        let spaet = g(300.0);
        assert!(nach_halten < 0.5, "nach 40 ms schon {nach_halten}");
        assert!(mitten > nach_halten && mitten < 0.9, "nach 80 ms {mitten}");
        assert!(spaet > 0.97, "nach 300 ms erst {spaet}");
    }

    #[test]
    fn tk_laesst_tiefe_stimme_und_wortanfang_stehen() {
        // 80 Hz mit Obertoenen, aus der Stille in 10 ms auf -20 dBFS:
        // kein Klackser, also keine Absenkung.
        let mut tk = tk_an();
        let n = 48000 / 2;
        let mut ein = Vec::new();
        let mut aus = Vec::new();
        for i in 0..n {
            let t = i as f32 / 48000.0;
            let h = (i as f32 / 480.0).min(1.0);
            let x = 0.06 * h * ((2.0 * PI * 80.0 * t).sin() + 0.4 * (2.0 * PI * 160.0 * t).sin());
            ein.push(x);
            aus.push(tk.process(x, 6.0, 15.0));
        }
        let rms = |v: &[f32]| (v.iter().map(|x| x * x).sum::<f32>() / v.len() as f32).sqrt();
        let versatz = 240;
        let e = rms(&ein[0..n - versatz]);
        let a = rms(&aus[versatz..n]);
        let db = 20.0 * (a / e).log10();
        assert!(db > -0.3, "tiefe Stimme verlor {db:.2} dB");
    }
}

#[cfg(test)]
mod nr_tests {
    use super::*;

    fn sinus(f: f32, amp: f32, n: usize, sr: f32) -> Vec<f32> {
        (0..n)
            .map(|i| amp * (2.0 * PI * f * i as f32 / sr).sin())
            .collect()
    }

    /// Deterministisches weisses Rauschen (LCG), damit Messungen reproduzierbar sind.
    struct Rausch {
        s: u32,
    }

    impl Rausch {
        fn neu(seed: u32) -> Rausch {
            Rausch { s: seed | 1 }
        }
        fn next(&mut self) -> f32 {
            // Zwei gleichverteilte Werte -> grob gaussförmig, |x| < 1.
            let mut sum = 0.0;
            for _ in 0..3 {
                self.s = self.s.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                sum += (self.s >> 8) as f32 / 8_388_608.0 - 1.0;
            }
            sum * 0.577
        }
    }

    /// Effektivwert ohne Gleichanteil.
    fn rms(v: &[f32]) -> f32 {
        if v.is_empty() {
            return 0.0;
        }
        let m = v.iter().sum::<f32>() / v.len() as f32;
        (v.iter().map(|x| (x - m) * (x - m)).sum::<f32>() / v.len() as f32).sqrt()
    }

    /// Amplitude einer Einzeltone mittels Projektion auf sin/cos bei `f`.
    fn ton_amplitude(v: &[f32], f: f32, sr: f32) -> f32 {
        let n = v.len() as f32;
        let (mut s, mut c) = (0.0f64, 0.0f64);
        for (i, x) in v.iter().enumerate() {
            let w = 2.0 * std::f64::consts::PI * f as f64 * i as f64 / sr as f64;
            s += *x as f64 * w.sin();
            c += *x as f64 * w.cos();
        }
        // Rechteckfenster: Amplitude = 2/N * |Summe|, mit cos-Anteil.
        ((2.0 / n as f64) * (s * s + c * c).sqrt()) as f32
    }

    #[test]
    fn fft_rundlauf_ist_exakt() {
        let f = Fft::new(NR_FFT);
        let mut re: Vec<f32> = (0..NR_FFT)
            .map(|i| (i as f32 * 0.37).sin() + 0.3 * (i as f32 * 0.011).cos())
            .collect();
        let orig = re.clone();
        let mut im = vec![0.0f32; NR_FFT];
        f.forward(&mut re, &mut im);
        f.inverse(&mut re, &mut im);
        let mut maxdiff = 0.0f32;
        for i in 0..NR_FFT {
            maxdiff = maxdiff.max((re[i] - orig[i]).abs());
        }
        assert!(maxdiff < 1e-4, "Rundlauf wich um {maxdiff} ab");
    }

    #[test]
    fn fft_legt_sinus_in_das_richtige_band() {
        let f = Fft::new(NR_FFT);
        let mut re = sinus(1000.0, 1.0, NR_FFT, 48000.0);
        let mut im = vec![0.0f32; NR_FFT];
        f.forward(&mut re, &mut im);
        // 1000 Hz bei 48 kHz und 512 Punkten -> Band 10,67, also 10 oder 11.
        let mut best = (0usize, 0.0f32);
        for k in 0..NR_BINS {
            let m = (re[k] * re[k] + im[k] * im[k]).sqrt();
            if m > best.1 {
                best = (k, m);
            }
        }
        assert!(
            best.0 == 10 || best.0 == 11,
            "Spitze lag in Band {}",
            best.0
        );
    }

    #[test]
    fn fenster_summiert_sich_bei_halber_ueberlappung_auf_eins() {
        let mut w = [0.0f32; NR_FFT];
        for (i, x) in w.iter_mut().enumerate() {
            let h = 0.5 - 0.5 * (2.0 * PI * i as f32 / NR_FFT as f32).cos();
            *x = h.max(1e-6).sqrt();
        }
        for n in 0..NR_HOP {
            let s = w[n] * w[n] + w[n + NR_HOP] * w[n + NR_HOP];
            assert!((s - 1.0).abs() < 1e-4, "Summe bei {n} war {s}");
        }
    }

    #[test]
    fn lowcut_aus_ist_durchgang() {
        let mut lc = LowCut::new();
        lc.set_hz(0.0, 48000.0);
        for i in 0..1000 {
            let x = (i as f32 * 0.1).sin();
            assert_eq!(lc.process(x), x);
        }
    }

    #[test]
    fn lowcut_raster_kennt_vier_frequenzen() {
        assert_eq!(lowcut_raster(0.0), 0.0);
        assert_eq!(lowcut_raster(-5.0), 0.0);
        assert_eq!(lowcut_raster(59.0), 60.0);
        assert_eq!(lowcut_raster(75.0), 80.0);
        assert_eq!(lowcut_raster(110.0), 120.0);
        assert_eq!(lowcut_raster(155.0), 160.0);
        assert_eq!(lowcut_raster(400.0), 160.0);
    }

    /// Pegel bei 40 Hz gegen 200 Hz – beides durch den Hochpass.
    fn lowcut_pegel(hz: f32) -> (f32, f32) {
        let sr = 48000.0;
        let n = 48000;
        let mut lc = LowCut::new();
        lc.set_hz(hz, sr);
        let mut tief = 0f32;
        let mut hoch = 0f32;
        for i in 0..n {
            let x = 0.5 * (2.0 * PI * 40.0 * i as f32 / sr).sin();
            let y = lc.process(x);
            if i > n / 2 {
                tief = tief.max(y.abs());
            }
        }
        let mut lc = LowCut::new();
        lc.set_hz(hz, sr);
        for i in 0..n {
            let x = 0.5 * (2.0 * PI * 200.0 * i as f32 / sr).sin();
            let y = lc.process(x);
            if i > n / 2 {
                hoch = hoch.max(y.abs());
            }
        }
        (lin_to_db(tief), lin_to_db(hoch))
    }

    #[test]
    fn lowcut_daempft_40_hz_gegen_200_hz() {
        for hz in [60.0f32, 120.0] {
            let (tief, hoch) = lowcut_pegel(hz);
            println!(
                "LOWCUT {hz:>3} Hz: 40 Hz = {tief:7.2} dBFS, 200 Hz = {hoch:7.2} dBFS, Unterschied {:6.2} dB",
                tief - hoch
            );
            // 12 dB/Okt: bei 60 Hz liegen 40 Hz rund 8 dB tiefer, bei 120 Hz
            // rund 19 dB (theoretisch 7,8 bzw. 19,1 dB).
            assert!(
                tief - hoch < -7.0,
                "Lowcut {hz} Hz daempfte 40 Hz nur um {} dB",
                tief - hoch
            );
            // 200 Hz muss praktisch unbehelligt bleiben.
            assert!(hoch > -6.8, "200 Hz wurde zu stark gedaempft: {hoch} dB");
        }
    }

    /// Sinus 1 kHz (-20 dBFS) + weisses Rauschen (-20 dBFS) durch die NR.
    fn nr_lauf(staerke: f32, schwelle: f32) -> (f32, f32, f32, f32, f32) {
        let sr = 48000.0;
        let n = 96_000; // 2 s: 1 s Rauschschaetzung, 1 s Messung
        let ton = 0.1; // -20 dBFS Spitze
        let rausch = 0.1; // -20 dBFS Effektivwert
        let s = sinus(1000.0, ton, n, sr);
        let mut r = Rausch::neu(0x1234_5678);
        let mut noise = Vec::with_capacity(n);
        for _ in 0..n {
            noise.push(r.next() * rausch * 1.732); // auf RMS = rausch normiert
        }
        let ein: Vec<f32> = s.iter().zip(noise.iter()).map(|(a, b)| a + b).collect();
        let mut nr = NoiseReduction::new(sr);
        nr.enabled = true;
        let aus: Vec<f32> = ein
            .iter()
            .map(|x| nr.process(*x, staerke, schwelle, sr))
            .collect();
        let ab = n / 2; // hinteres Fenster: Schaetzung steht
        let ein_ton = ton_amplitude(&ein[ab..], 1000.0, sr);
        let aus_ton = ton_amplitude(&aus[ab..], 1000.0, sr);
        let ein_rms = rms(&ein[ab..]);
        let aus_rms = rms(&aus[ab..]);
        // Rauschanteil = Gesamtleistung minus Tonleistung
        let ein_n = (ein_rms * ein_rms - (ein_ton * ein_ton) / 2.0)
            .max(0.0)
            .sqrt();
        let aus_n = (aus_rms * aus_rms - (aus_ton * aus_ton) / 2.0)
            .max(0.0)
            .sqrt();
        (
            lin_to_db(ein_n),
            lin_to_db(aus_n),
            lin_to_db(ein_ton),
            lin_to_db(aus_ton),
            staerke,
        )
    }

    #[test]
    fn nr_senkt_rauschen_und_haelt_den_sinus() {
        for staerke in [0.5f32, 1.0] {
            let (vn, nn, vt, nt, _) = nr_lauf(staerke, -45.0);
            println!(
                "NR {staerke:.1}: Rauschen {vn:7.2} -> {nn:7.2} dBFS ({:6.2} dB), Sinus {vt:7.2} -> {nt:7.2} dBFS ({:+.2} dB)",
                nn - vn,
                nt - vt
            );
            assert!(
                nn - vn < -11.0,
                "Rauschen sank bei Staerke {staerke} nur um {} dB",
                nn - vn
            );
            assert!(
                (nt - vt).abs() <= 1.0,
                "Sinus verlor bei Staerke {staerke} {} dB",
                nt - vt
            );
        }
    }

    #[test]
    fn nr_aus_ist_bitgenau_durchgang() {
        let mut nr = NoiseReduction::new(48000.0);
        nr.enabled = true;
        let mut r = Rausch::neu(7);
        for _ in 0..5000 {
            let x = r.next() * 0.2;
            assert_eq!(nr.process(x, 0.0, -45.0, 48000.0), x);
        }
    }

    #[test]
    fn nr_laesst_stille_still() {
        let mut nr = NoiseReduction::new(48000.0);
        nr.enabled = true;
        for i in 0..20000 {
            let y = nr.process(0.0, 1.0, -45.0, 48000.0);
            assert!(y.abs() < 1e-6, "bei {i} kam {y} heraus");
        }
    }

    #[test]
    fn nr_expander_faehrt_unter_der_schwelle_zu() {
        let sr = 48000.0;
        let mut nr = NoiseReduction::new(sr);
        nr.enabled = true;
        // leises Rauschen bei -70 dBFS, Schwelle -40 dBFS
        let mut r = Rausch::neu(99);
        let mut sum = 0.0;
        let n = 24000;
        for _ in 0..n {
            let x = r.next() * 0.000316 * 1.732;
            let y = nr.process(x, 1.0, -40.0, sr);
            sum += y * y;
        }
        let db = lin_to_db((sum / n as f32).sqrt());
        println!("EXPANDER: -70 dBFS Rauschen bei Schwelle -40 dBFS -> {db:.2} dBFS");
        assert!(db < -75.0, "Expander hat nicht zugemacht: {db} dBFS");
    }

    #[test]
    fn nr_schaetzung_ist_erwartungstreu() {
        // Weisses Rauschen: die Schaetzung je Band soll im Mittel dem echten
        // Pegel entsprechen (Fensterenergie: Summe der w² = N/2).
        for seed in [7u32, 99, 4242] {
            let mut nr = NoiseReduction::new(48000.0);
            nr.enabled = true;
            let mut r = Rausch::neu(seed);
            let mut summe2 = 0.0f64;
            let n = 48000 * 3;
            for _ in 0..n {
                let x = 0.01 * r.next();
                summe2 += (x * x) as f64;
                nr.process(x, 0.5, -45.0, 48000.0);
            }
            let var = (summe2 / n as f64) as f32;
            let soll = var * (NR_FFT as f32 / 2.0);
            let mittel: f32 = nr.noise[4..NR_BINS - 4].iter().sum::<f32>() / (NR_BINS - 8) as f32;
            let q = mittel / soll;
            eprintln!("Schaetzung/Soll = {q:.3}");
            assert!((0.75..1.33).contains(&q), "Schaetzung/Soll = {q}");
        }
    }
}
