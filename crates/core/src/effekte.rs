//! Eingebaute Effekte des Pults: die beiden Send-Effekte Hall (`R`) und
//! Echo (`D`) sowie die Klangbausteine eines Streifens – Klangfarbe
//! (Voice Color) und Karaoke/Mitte stumm.
//!
//! Alles hier läuft im Audio-Thread: Puffer entstehen in `neu`, `process`
//! allokiert nicht und nimmt keine Sperre.
//!
//! # Sends und Returns
//!
//! Jeder Streifen hat vier Sends ([`FX_NAMEN`]): `r` (Hall), `d` (Echo),
//! `fx1` und `fx2` (externe Effekte über die Ports `fx1_L/R` →
//! `fx1_ret_L/R`). Je Bus regelt ein Return, wie viel jedes Effektsignals
//! dort ankommt. Die Effekte arbeiten **100 % nass**: der trockene Anteil
//! kommt wie immer über den Streifen.

use crate::dsp::{db_to_lin, Biquad, FilterType};

/// Anzahl der Effektwege je Streifen: Hall, Echo, FX1, FX2.
pub const FX_WEGE: usize = 4;
/// Parameternamen der Wege (`send_<name>`, `post_<name>`, `ret_<name>`).
pub const FX_NAMEN: [&str; FX_WEGE] = ["r", "d", "fx1", "fx2"];
/// Anzeigenamen der Wege.
pub const FX_TITEL: [&str; FX_WEGE] = ["Hall", "Echo", "FX1", "FX2"];
/// Send- und Return-Pegel gelten an dieser Grenze als aus.
pub const AUS_DB: f32 = -60.0;
/// Obergrenze für Send und Return.
pub const MAX_DB: f32 = 6.0;

/// dB → linear, wobei [`AUS_DB`] und darunter wirklich null ist.
#[inline]
pub fn pegel_lin(db: f32) -> f32 {
    if db <= AUS_DB {
        0.0
    } else {
        db_to_lin(db.min(MAX_DB))
    }
}

/// Index eines Wegs aus seinem Namen (`"r"` → 0).
pub fn weg_index(name: &str) -> Option<usize> {
    FX_NAMEN.iter().position(|n| *n == name)
}

/// Höchste Abtastrate, für die die Puffer bemessen sind.
pub const MAX_RATE: f32 = 192_000.0;

// ---------------------------------------------------------------------------
// Hall (Schroeder/Moorer-Struktur wie Freeverb)
// ---------------------------------------------------------------------------

/// Kammfilterlängen bei 44,1 kHz (Freeverb).
const KAMM: [usize; 8] = [1116, 1188, 1277, 1356, 1422, 1491, 1557, 1617];
/// Allpasslängen bei 44,1 kHz.
const ALLPASS: [usize; 4] = [556, 441, 341, 225];
/// Versatz des rechten Kanals – macht den Raum breit.
const SPREIZUNG: usize = 23;
/// Eingangsdämpfung: die Kammfilter summieren sich sonst weit über 0 dBFS.
const HALL_EINGANG: f32 = 0.015;
/// Ausgangsverstärkung, damit ein voller Send etwa so laut ist wie trocken.
const HALL_AUSGANG: f32 = 1.5;

struct Kamm {
    buf: Vec<f32>,
    len: usize,
    pos: usize,
    speicher: f32,
}

impl Kamm {
    fn neu(max: usize) -> Kamm {
        Kamm {
            buf: vec![0.0; max.max(1)],
            len: 1,
            pos: 0,
            speicher: 0.0,
        }
    }

    fn laenge(&mut self, n: usize) {
        self.len = n.clamp(1, self.buf.len());
        self.pos = 0;
        self.speicher = 0.0;
        self.buf.iter_mut().for_each(|x| *x = 0.0);
    }

    #[inline]
    fn process(&mut self, x: f32, rueck: f32, daempf: f32) -> f32 {
        let y = self.buf[self.pos];
        self.speicher = y * (1.0 - daempf) + self.speicher * daempf;
        self.buf[self.pos] = x + self.speicher * rueck;
        self.pos += 1;
        if self.pos >= self.len {
            self.pos = 0;
        }
        y
    }
}

struct Allpass {
    buf: Vec<f32>,
    len: usize,
    pos: usize,
}

impl Allpass {
    fn neu(max: usize) -> Allpass {
        Allpass {
            buf: vec![0.0; max.max(1)],
            len: 1,
            pos: 0,
        }
    }

    fn laenge(&mut self, n: usize) {
        self.len = n.clamp(1, self.buf.len());
        self.pos = 0;
        self.buf.iter_mut().for_each(|x| *x = 0.0);
    }

    #[inline]
    fn process(&mut self, x: f32) -> f32 {
        let b = self.buf[self.pos];
        let y = b - x;
        self.buf[self.pos] = x + b * 0.5;
        self.pos += 1;
        if self.pos >= self.len {
            self.pos = 0;
        }
        y
    }
}

/// Stereo-Hall. `raum` 0..1 (Nachhall), `daempfung` 0..1 (Höhen im Nachhall).
pub struct Hall {
    kamm: [[Kamm; 8]; 2],
    allpass: [[Allpass; 4]; 2],
    rate: f32,
}

impl Hall {
    pub fn neu() -> Hall {
        let faktor = MAX_RATE / 44_100.0;
        let max = |n: usize| ((n + SPREIZUNG) as f32 * faktor) as usize + 2;
        Hall {
            kamm: std::array::from_fn(|_| std::array::from_fn(|i| Kamm::neu(max(KAMM[i])))),
            allpass: std::array::from_fn(|_| {
                std::array::from_fn(|i| Allpass::neu(max(ALLPASS[i])))
            }),
            rate: 0.0,
        }
    }

    /// Längen an die Abtastrate anpassen (nur wenn sie sich ändert).
    pub fn set_sample_rate(&mut self, sr: f32) {
        if (sr - self.rate).abs() < 0.5 {
            return;
        }
        self.rate = sr;
        let faktor = sr.clamp(8000.0, MAX_RATE) / 44_100.0;
        for (ch, (kamm, allpass)) in self
            .kamm
            .iter_mut()
            .zip(self.allpass.iter_mut())
            .enumerate()
        {
            let versatz = if ch == 1 { SPREIZUNG } else { 0 };
            for (k, n) in kamm.iter_mut().zip(KAMM) {
                k.laenge(((n + versatz) as f32 * faktor) as usize);
            }
            for (a, n) in allpass.iter_mut().zip(ALLPASS) {
                a.laenge(((n + versatz) as f32 * faktor) as usize);
            }
        }
    }

    /// Ein Stereo-Sample, Ausgang nur nass.
    #[inline]
    #[allow(clippy::needless_range_loop)]
    pub fn process(&mut self, x: [f32; 2], raum: f32, daempfung: f32) -> [f32; 2] {
        let rueck = 0.7 + 0.25 * raum.clamp(0.0, 1.0);
        let daempf = 0.4 * daempfung.clamp(0.0, 1.0);
        // Mono hinein, Stereo heraus: die Breite kommt aus der Spreizung.
        let ein = (x[0] + x[1]) * HALL_EINGANG;
        let mut aus = [0.0f32; 2];
        for ch in 0..2 {
            let mut s = 0.0;
            for k in self.kamm[ch].iter_mut() {
                s += k.process(ein, rueck, daempf);
            }
            for a in self.allpass[ch].iter_mut() {
                s = a.process(s);
            }
            aus[ch] = s * HALL_AUSGANG;
        }
        aus
    }
}

impl Default for Hall {
    fn default() -> Self {
        Hall::neu()
    }
}

// ---------------------------------------------------------------------------
// Echo
// ---------------------------------------------------------------------------

/// Längstes Echo in Millisekunden.
pub const ECHO_MAX_MS: f32 = 2000.0;

/// Stereo-Echo mit Rückkopplung und gedämpften Wiederholungen.
pub struct Echo {
    buf: [Vec<f32>; 2],
    pos: usize,
    tief: [f32; 2],
}

impl Echo {
    pub fn neu() -> Echo {
        let n = (ECHO_MAX_MS * 0.001 * MAX_RATE) as usize + 2;
        Echo {
            buf: [vec![0.0; n], vec![0.0; n]],
            pos: 0,
            tief: [0.0; 2],
        }
    }

    /// Ein Stereo-Sample, nur nass. `zeit_ms` 1..2000, `rueck` 0..0,9,
    /// `daempfung` 0..1 (wie stark jede Wiederholung dumpfer wird).
    #[inline]
    pub fn process(
        &mut self,
        x: [f32; 2],
        zeit_ms: f32,
        rueck: f32,
        daempfung: f32,
        sr: f32,
    ) -> [f32; 2] {
        let n = self.buf[0].len();
        let abstand = ((zeit_ms.clamp(1.0, ECHO_MAX_MS) * 0.001 * sr) as usize).clamp(1, n - 1);
        let lesen = (self.pos + n - abstand) % n;
        let rueck = rueck.clamp(0.0, 0.9);
        let d = daempfung.clamp(0.0, 1.0) * 0.85;
        let mut aus = [0.0f32; 2];
        for ch in 0..2 {
            let y = self.buf[ch][lesen];
            self.tief[ch] = y * (1.0 - d) + self.tief[ch] * d;
            self.buf[ch][self.pos] = x[ch] + self.tief[ch] * rueck;
            aus[ch] = y;
        }
        self.pos = (self.pos + 1) % n;
        aus
    }
}

impl Default for Echo {
    fn default() -> Self {
        Echo::neu()
    }
}

// ---------------------------------------------------------------------------
// Klangfarbe (Voice Color)
// ---------------------------------------------------------------------------

/// Kippfilter: `farbe` −1 (dunkel) … 0 (neutral) … +1 (hell). Tiefen und
/// Höhen gehen gegenläufig um bis zu [`FARBE_DB`] – die Stimme wird wärmer
/// oder präsenter, ohne lauter zu werden.
pub const FARBE_DB: f32 = 6.0;
const FARBE_TIEF_HZ: f32 = 250.0;
const FARBE_HOCH_HZ: f32 = 2500.0;

#[derive(Clone, Copy)]
pub struct Klangfarbe {
    tief: Biquad,
    hoch: Biquad,
    farbe: f32,
    rate: f32,
}

impl Default for Klangfarbe {
    fn default() -> Self {
        Klangfarbe {
            tief: Biquad::bypass(),
            hoch: Biquad::bypass(),
            farbe: 0.0,
            rate: 0.0,
        }
    }
}

impl Klangfarbe {
    /// Koeffizienten nur neu rechnen, wenn sich Farbe oder Rate ändern.
    pub fn setup(&mut self, farbe: f32, sr: f32) {
        let farbe = farbe.clamp(-1.0, 1.0);
        if (farbe - self.farbe).abs() < 1e-4 && (sr - self.rate).abs() < 0.5 {
            return;
        }
        self.farbe = farbe;
        self.rate = sr;
        let db = FARBE_DB * farbe;
        self.tief
            .coeffs(FilterType::LowShelf, FARBE_TIEF_HZ, 0.7, -db, sr);
        self.hoch
            .coeffs(FilterType::HighShelf, FARBE_HOCH_HZ, 0.7, db, sr);
    }

    /// Neutral (0) ist ein bit-gleicher Durchgang.
    #[inline]
    pub fn process(&mut self, x: f32) -> f32 {
        if self.farbe == 0.0 {
            return x;
        }
        self.hoch.process(self.tief.process(x))
    }
}

// ---------------------------------------------------------------------------
// Karaoke und „Mitte stumm“
// ---------------------------------------------------------------------------

/// Karaoke-Modi (`karaoke`).
pub const KARAOKE_AUS: u32 = 0;
/// Mitte (meist die Stimme) heraus, Bässe bleiben.
pub const KARAOKE_STIMME_WEG: u32 = 1;
/// Nur die Mitte – die Stimme bleibt, das Stereobild verschwindet.
pub const KARAOKE_NUR_MITTE: u32 = 2;
pub const KARAOKE_MAX: u32 = KARAOKE_NUR_MITTE;
const KARAOKE_BASS_HZ: f32 = 180.0;

/// Arbeitet auf einem Stereopaar. In der Mitte gemischte Signale (L = R)
/// stehen im Mittenanteil `(L+R)/2`, Seitenanteile in `(L−R)/2`.
#[derive(Clone, Copy)]
pub struct Karaoke {
    /// Zwei Tiefpässe hintereinander (24 dB/Oktave): die Stimme darf nicht
    /// über den Bassweg zurückkommen.
    bass: [Biquad; 2],
    rate: f32,
}

impl Default for Karaoke {
    fn default() -> Self {
        Karaoke {
            bass: [Biquad::bypass(); 2],
            rate: 0.0,
        }
    }
}

impl Karaoke {
    pub fn set_sample_rate(&mut self, sr: f32) {
        if (sr - self.rate).abs() < 0.5 {
            return;
        }
        self.rate = sr;
        for b in self.bass.iter_mut() {
            b.coeffs(FilterType::LowPass, KARAOKE_BASS_HZ, 0.707, 0.0, sr);
        }
    }

    /// `muc` = Mitte stumm (nur Seitenanteil). `modus` siehe `KARAOKE_*`;
    /// ein gesetzter Modus hat Vorrang vor `muc`.
    #[inline]
    pub fn process(&mut self, x: [f32; 2], modus: u32, muc: bool) -> [f32; 2] {
        let mitte = 0.5 * (x[0] + x[1]);
        let seite = 0.5 * (x[0] - x[1]);
        match modus {
            KARAOKE_STIMME_WEG => {
                let b = self.bass[0].process(mitte);
                let b = self.bass[1].process(b);
                [seite + b, -seite + b]
            }
            KARAOKE_NUR_MITTE => [mitte, mitte],
            _ if muc => [seite, -seite],
            _ => x,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sinus(f: f32, sr: f32, n: usize) -> Vec<f32> {
        (0..n)
            .map(|i| (2.0 * std::f32::consts::PI * f * i as f32 / sr).sin())
            .collect()
    }

    fn rms(x: &[f32]) -> f32 {
        (x.iter().map(|v| v * v).sum::<f32>() / x.len().max(1) as f32).sqrt()
    }

    #[test]
    fn pegel_aus_ist_null() {
        assert_eq!(pegel_lin(AUS_DB), 0.0);
        assert_eq!(pegel_lin(-80.0), 0.0);
        assert!((pegel_lin(0.0) - 1.0).abs() < 1e-6);
        assert!((pegel_lin(20.0) - db_to_lin(MAX_DB)).abs() < 1e-6);
        assert_eq!(weg_index("fx2"), Some(3));
        assert_eq!(weg_index("x"), None);
    }

    #[test]
    fn hall_klingt_nach_und_verstummt() {
        let sr = 48_000.0;
        let mut h = Hall::neu();
        h.set_sample_rate(sr);
        // Ein Impuls, dann Stille.
        let mut aus = Vec::new();
        for i in 0..(sr as usize * 4) {
            let x = if i == 0 { 1.0 } else { 0.0 };
            aus.push(h.process([x, x], 0.5, 0.5));
        }
        let fenster = |ab: f32, bis: f32| {
            let a = (ab * sr) as usize;
            let b = (bis * sr) as usize;
            let l: Vec<f32> = aus[a..b].iter().map(|v| v[0]).collect();
            rms(&l)
        };
        let frueh = fenster(0.05, 0.3);
        let spaet = fenster(3.5, 4.0);
        assert!(frueh > 1e-3, "Hall hoerbar: {frueh}");
        assert!(spaet < frueh * 0.01, "klingt aus: {spaet} vs {frueh}");
        // Stereo: die Kanaele sind verschieden (Spreizung).
        let diff: f32 = aus[2000..6000].iter().map(|v| (v[0] - v[1]).abs()).sum();
        assert!(diff > 1e-3, "breit");
    }

    #[test]
    fn hall_bleibt_bei_dauersignal_begrenzt() {
        let sr = 48_000.0;
        let mut h = Hall::neu();
        h.set_sample_rate(sr);
        let s = sinus(440.0, sr, sr as usize * 3);
        let mut spitze = 0.0f32;
        for x in s {
            let y = h.process([x, x], 1.0, 0.0);
            spitze = spitze.max(y[0].abs()).max(y[1].abs());
        }
        assert!(spitze < 4.0, "keine Rueckkopplung: {spitze}");
        assert!(spitze > 0.05, "hoerbar: {spitze}");
    }

    #[test]
    fn echo_wiederholt_nach_der_zeit() {
        let sr = 48_000.0;
        let mut e = Echo::neu();
        let mut aus = Vec::new();
        for i in 0..(sr as usize) {
            let x = if i == 0 { 1.0 } else { 0.0 };
            aus.push(e.process([x, x], 250.0, 0.5, 0.0, sr)[0]);
        }
        let erstes = aus.iter().position(|v| v.abs() > 0.5).expect("Echo");
        assert_eq!(erstes, 12_000, "250 ms bei 48 kHz");
        let zweites = aus[erstes + 1..]
            .iter()
            .position(|v| v.abs() > 0.1)
            .map(|p| p + erstes + 1)
            .expect("Wiederholung");
        assert_eq!(zweites, 24_000);
        assert!((aus[zweites] - 0.5).abs() < 1e-3, "Rueckkopplung 0,5");
    }

    #[test]
    fn echo_ohne_rueckkopplung_nur_einmal() {
        let sr = 48_000.0;
        let mut e = Echo::neu();
        let mut treffer = 0;
        for i in 0..(sr as usize) {
            let x = if i == 0 { 1.0 } else { 0.0 };
            if e.process([x, x], 100.0, 0.0, 0.0, sr)[1].abs() > 0.01 {
                treffer += 1;
            }
        }
        assert_eq!(treffer, 1);
    }

    #[test]
    fn klangfarbe_kippt_tiefen_und_hoehen() {
        let sr = 48_000.0;
        let mess = |farbe: f32, f: f32| {
            let mut k = Klangfarbe::default();
            k.setup(farbe, sr);
            let y: Vec<f32> = sinus(f, sr, 48_000)
                .into_iter()
                .map(|x| k.process(x))
                .collect();
            20.0 * (rms(&y[24_000..]) / rms(&sinus(f, sr, 24_000))).log10()
        };
        assert!(mess(0.0, 100.0).abs() < 0.01, "neutral");
        assert!(mess(1.0, 8000.0) > 4.5, "hell hebt Hoehen");
        assert!(mess(1.0, 80.0) < -4.5, "hell senkt Tiefen");
        assert!(mess(-1.0, 8000.0) < -4.5, "dunkel senkt Hoehen");
        assert!(mess(-1.0, 1000.0).abs() < 2.0, "Mitte bleibt etwa");
    }

    #[test]
    fn karaoke_nimmt_die_mitte_heraus() {
        let sr = 48_000.0;
        let mut k = Karaoke::default();
        k.set_sample_rate(sr);
        // Stimme in der Mitte verschwindet.
        let stimme = sinus(1000.0, sr, 48_000);
        let y: Vec<f32> = stimme
            .iter()
            .map(|s| k.process([*s, *s], KARAOKE_STIMME_WEG, false)[0])
            .collect();
        let rest = 20.0 * (rms(&y[24_000..]) / rms(&stimme)).log10();
        assert!(rest < -30.0, "Stimme weg: {rest} dB");
        // Ein Instrument nur links bleibt hoerbar (halber Pegel als Seite).
        let mut k = Karaoke::default();
        k.set_sample_rate(sr);
        let links = sinus(3000.0, sr, 48_000);
        let y: Vec<f32> = links
            .iter()
            .map(|l| k.process([*l, 0.0], KARAOKE_STIMME_WEG, false)[0])
            .collect();
        let pegel = 20.0 * (rms(&y[24_000..]) / rms(&links)).log10();
        assert!((pegel + 6.0).abs() < 0.5, "Instrument bleibt: {pegel} dB");
    }

    #[test]
    fn karaoke_laesst_den_bass_stehen() {
        let sr = 48_000.0;
        let mut k = Karaoke::default();
        k.set_sample_rate(sr);
        let bass = sinus(60.0, sr, 96_000);
        let y: Vec<f32> = bass
            .iter()
            .map(|b| k.process([*b, *b], KARAOKE_STIMME_WEG, false)[0])
            .collect();
        let verlust = 20.0 * (rms(&y[48_000..]) / rms(&bass)).log10();
        assert!(verlust > -3.5, "Bass bleibt: {verlust} dB");
        // Mitte stumm nimmt auch den Bass.
        let y: Vec<f32> = bass
            .iter()
            .map(|b| k.process([*b, *b], 0, true)[0])
            .collect();
        assert!(rms(&y) < 1e-6);
    }

    #[test]
    fn nur_mitte_und_aus() {
        let mut k = Karaoke::default();
        k.set_sample_rate(48_000.0);
        assert_eq!(k.process([1.0, 0.0], KARAOKE_NUR_MITTE, false), [0.5, 0.5]);
        assert_eq!(k.process([0.3, -0.2], KARAOKE_AUS, false), [0.3, -0.2]);
    }
}
