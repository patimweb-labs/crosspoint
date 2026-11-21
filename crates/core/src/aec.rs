//! Echounterdrückung (AEC): rechnet den Systemton aus dem Mikrofon heraus,
//! der über die Boxen zurück ins Mikrofon kommt.
//!
//! # Verfahren
//!
//! Partitionierter adaptiver Filter im Frequenzbereich (PBFDAF, „MDF“):
//! Das Echo ist der Boxen-Ton (Referenz), gefaltet mit dem Raum. Der Filter
//! lernt diese Faltung und zieht seine Schätzung vom Mikrofon ab. Die Länge
//! ist [`PARTITIONEN`] × [`BLOCK`] Samples (12 288 = 256 ms bei 48 kHz) – das
//! deckt die Latenz von PipeWire (Ausgabe- plus Aufnahmepuffer) und den
//! Nachhall eines Zimmers.
//!
//! * Block [`BLOCK`] = 128 Samples, FFT [`FFT_N`] = 256 (Overlap-Save).
//!   Das Ergebnis kommt einen Block später: 2,7 ms bei 48 kHz.
//! * Schrittweite je Frequenz normiert auf die geglättete Leistung der
//!   Referenz (NLMS im Frequenzbereich).
//! * **Doppelsprechen:** zwei Filter. Der Hintergrund lernt immer mit fester
//!   Schrittweite; ausgegeben wird mit dem Vordergrund. Er übernimmt die
//!   Gewichte des Hintergrunds nur, wenn dessen Fehler kleiner ist. Spricht
//!   jemand, während Systemton läuft, läuft der Hintergrund weg – dann wird
//!   er auf den Vordergrund zurückgesetzt, und die Stimme bleibt heil.
//! * Die Zeitbegrenzung der Gradienten (Constraint) kostet zwei FFTs je
//!   Partition; sie läuft reihum, je Block für eine Partition (wie in Speex).
//!
//! Alles wird in [`Echounterdruecker::neu`] angelegt; [`Echounterdruecker::process`]
//! allokiert nicht und nimmt keine Sperre.

// Je Bin laufen mehrere parallele Felder (Spektren, Gewichte, Leistung,
// Schrittweite) im Gleichschritt; der Index liest sich dort besser als ein
// Zip über vier Iteratoren.
#![allow(clippy::needless_range_loop)]

use crate::dsp::Fft;

/// Samples je Block (Hop).
pub const BLOCK: usize = 128;
/// FFT-Größe (Overlap-Save: zwei Blöcke).
pub const FFT_N: usize = 2 * BLOCK;
/// Anzahl der Partitionen; Filterlänge = `PARTITIONEN * BLOCK`.
pub const PARTITIONEN: usize = 96;
/// Frequenzbins eines reellen Signals (0..=N/2).
const BINS: usize = FFT_N / 2 + 1;

/// Schrittweite des Hintergrundfilters (NLMS, normiert).
const MU: f32 = 0.5;
/// Glättung der Fehlerleistungen für den Vergleich der beiden Filter.
const FEHLER_GLATT: f32 = 0.8;
/// Der Hintergrund wird übernommen, wenn sein Fehler kleiner ist als dieser
/// Anteil des Vordergrundfehlers.
const UEBERNEHMEN: f32 = 0.9;
/// Ist der Hintergrund um diesen Faktor schlechter, ist er beim
/// Doppelsprechen weggelaufen und wird auf den Vordergrund zurückgesetzt.
const ZURUECKSETZEN: f32 = 4.0;
/// Glättung der Referenzleistung je Bin.
const LEISTUNG_GLATT: f32 = 0.9;
/// Unter dieser mittleren Leistung je Sample gilt die Referenz als still –
/// dann wird nicht gelernt (nichts zu lernen, nur Rauschen).
const REF_STILL: f32 = 1.0e-8;

/// Komplexer Wert, schlicht.
#[derive(Clone, Copy, Default)]
struct K {
    re: f32,
    im: f32,
}

/// Echounterdrücker für einen Kanal.
pub struct Echounterdruecker {
    fft: Fft,
    /// Hintergrundfilter (lernt immer) je Partition und Bin.
    w: Vec<[K; BINS]>,
    /// Vordergrundfilter (wird ausgegeben).
    w_vorn: Vec<[K; BINS]>,
    /// Geglättete Fehlerleistungen von Hintergrund und Vordergrund.
    fehler_hinten: f32,
    fehler_vorn: f32,
    /// Spektren der letzten Referenzblöcke (Ring, `x_pos` = neuester).
    x: Vec<[K; BINS]>,
    x_pos: usize,
    /// Geglättete Leistung der Referenz je Bin.
    leistung: [f32; BINS],
    /// Letzter Referenzblock (Overlap-Save braucht zwei).
    ref_alt: [f32; BLOCK],
    /// Eingangssammler: Mikrofon und Referenz des laufenden Blocks.
    mic_in: [f32; BLOCK],
    ref_in: [f32; BLOCK],
    /// Ausgabe des vorigen Blocks (wird während des Sammelns ausgegeben).
    aus: [f32; BLOCK],
    pos: usize,
    /// Partition, deren Gradient im nächsten Block begrenzt wird.
    constraint: usize,
    // Arbeitspuffer
    re: [f32; FFT_N],
    im: [f32; FFT_N],
    e_spek: [K; BINS],
    y_spek: [K; BINS],
}

impl Default for Echounterdruecker {
    fn default() -> Self {
        Self::neu()
    }
}

impl Echounterdruecker {
    /// Legt alle Puffer an (außerhalb des Audio-Threads aufrufen).
    pub fn neu() -> Echounterdruecker {
        Echounterdruecker {
            fft: Fft::new(FFT_N),
            w: vec![[K::default(); BINS]; PARTITIONEN],
            w_vorn: vec![[K::default(); BINS]; PARTITIONEN],
            fehler_hinten: 0.0,
            fehler_vorn: 0.0,
            x: vec![[K::default(); BINS]; PARTITIONEN],
            x_pos: 0,
            leistung: [0.0; BINS],
            ref_alt: [0.0; BLOCK],
            mic_in: [0.0; BLOCK],
            ref_in: [0.0; BLOCK],
            aus: [0.0; BLOCK],
            pos: 0,
            constraint: 0,
            re: [0.0; FFT_N],
            im: [0.0; FFT_N],
            e_spek: [K::default(); BINS],
            y_spek: [K::default(); BINS],
        }
    }

    /// Alles vergessen (Filter, Verlauf, Puffer). Allokiert nicht.
    pub fn reset(&mut self) {
        for p in self
            .w
            .iter_mut()
            .chain(self.w_vorn.iter_mut())
            .chain(self.x.iter_mut())
        {
            *p = [K::default(); BINS];
        }
        self.x_pos = 0;
        self.leistung = [0.0; BINS];
        self.ref_alt = [0.0; BLOCK];
        self.mic_in = [0.0; BLOCK];
        self.ref_in = [0.0; BLOCK];
        self.aus = [0.0; BLOCK];
        self.pos = 0;
        self.constraint = 0;
        self.fehler_hinten = 0.0;
        self.fehler_vorn = 0.0;
    }

    /// Ein Sample: `mic` mit Echo hinein, Referenz `referenz` (was die Boxen
    /// spielen) dazu; heraus kommt das bereinigte Sample, [`BLOCK`] Samples
    /// verzögert.
    #[inline]
    pub fn process(&mut self, mic: f32, referenz: f32) -> f32 {
        let y = self.aus[self.pos];
        self.mic_in[self.pos] = mic;
        self.ref_in[self.pos] = referenz;
        self.pos += 1;
        if self.pos == BLOCK {
            self.pos = 0;
            self.block();
        }
        y
    }

    /// Spektrum eines reellen Puffers in `self.re/im` → Bins.
    fn vorwaerts(&mut self) {
        self.im = [0.0; FFT_N];
        self.fft.forward(&mut self.re, &mut self.im);
    }

    /// Bins → reelles Zeitsignal in `self.re` (Spiegelung ergänzt).
    fn rueckwaerts(&mut self, spek: &[K; BINS]) {
        for (k, s) in spek.iter().enumerate() {
            self.re[k] = s.re;
            self.im[k] = s.im;
        }
        for k in 1..FFT_N / 2 {
            self.re[FFT_N - k] = spek[k].re;
            self.im[FFT_N - k] = -spek[k].im;
        }
        self.fft.inverse(&mut self.re, &mut self.im);
    }

    fn block(&mut self) {
        // (1) Referenzspektrum: [voriger Block, aktueller Block].
        self.re[..BLOCK].copy_from_slice(&self.ref_alt);
        self.re[BLOCK..].copy_from_slice(&self.ref_in);
        self.ref_alt = self.ref_in;
        self.vorwaerts();
        self.x_pos = (self.x_pos + 1) % PARTITIONEN;
        let neu = &mut self.x[self.x_pos];
        for k in 0..BINS {
            neu[k] = K {
                re: self.re[k],
                im: self.im[k],
            };
        }
        let ref_energie: f32 = self.ref_in.iter().map(|v| v * v).sum::<f32>() / BLOCK as f32;
        for k in 0..BINS {
            let p = neu[k].re * neu[k].re + neu[k].im * neu[k].im;
            self.leistung[k] = LEISTUNG_GLATT * self.leistung[k] + (1.0 - LEISTUNG_GLATT) * p;
        }

        // (2) Echoschätzung beider Filter, Fehler im Zeitbereich.
        let mut e_hinten = [0.0f32; BLOCK];
        let mut leistung_hinten = 0.0f32;
        let mut leistung_vorn = 0.0f32;
        for vorn in [false, true] {
            self.schaetzen(vorn);
            let y_spek = self.y_spek;
            self.rueckwaerts(&y_spek);
            for n in 0..BLOCK {
                let e = self.mic_in[n] - self.re[BLOCK + n];
                if vorn {
                    self.aus[n] = e;
                    leistung_vorn += e * e;
                } else {
                    e_hinten[n] = e;
                    leistung_hinten += e * e;
                }
            }
        }
        if ref_energie < REF_STILL {
            return;
        }

        // (3) Vergleich: besserer Hintergrund wird übernommen, weggelaufener
        //     zurückgesetzt.
        let g = FEHLER_GLATT;
        self.fehler_hinten = g * self.fehler_hinten + (1.0 - g) * leistung_hinten;
        self.fehler_vorn = g * self.fehler_vorn + (1.0 - g) * leistung_vorn;
        if self.fehler_hinten < UEBERNEHMEN * self.fehler_vorn {
            self.w_vorn.copy_from_slice(&self.w);
            self.fehler_vorn = self.fehler_hinten;
        } else if self.fehler_hinten > ZURUECKSETZEN * self.fehler_vorn {
            self.w.copy_from_slice(&self.w_vorn);
            self.fehler_hinten = self.fehler_vorn;
            return;
        }

        // (4) Fehlerspektrum des Hintergrunds aus [0, e].
        self.re[..BLOCK].fill(0.0);
        self.re[BLOCK..].copy_from_slice(&e_hinten);
        self.vorwaerts();
        for k in 0..BINS {
            self.e_spek[k] = K {
                re: self.re[k],
                im: self.im[k],
            };
        }

        // (5) Schrittweite je Bin, normiert auf die Referenzleistung.
        let mut norm = [0.0f32; BINS];
        for k in 0..BINS {
            norm[k] = MU / (PARTITIONEN as f32 * self.leistung[k] + 1.0e-6 * FFT_N as f32);
        }

        // (6) W_p += μ_k · conj(X_{k−p}) · E.
        for p in 0..PARTITIONEN {
            let xi = (self.x_pos + PARTITIONEN - p) % PARTITIONEN;
            let x = &self.x[xi];
            let w = &mut self.w[p];
            for k in 0..BINS {
                let g_re = x[k].re * self.e_spek[k].re + x[k].im * self.e_spek[k].im;
                let g_im = x[k].re * self.e_spek[k].im - x[k].im * self.e_spek[k].re;
                w[k].re += norm[k] * g_re;
                w[k].im += norm[k] * g_im;
            }
        }

        // (7) Zeitbegrenzung einer Partition reihum: zweite Hälfte der
        //     Impulsantwort null, sonst entsteht zirkuläre Faltung.
        let p = self.constraint;
        self.constraint = (self.constraint + 1) % PARTITIONEN;
        let w = self.w[p];
        self.rueckwaerts(&w);
        self.re[BLOCK..].fill(0.0);
        self.vorwaerts();
        for k in 0..BINS {
            self.w[p][k] = K {
                re: self.re[k],
                im: self.im[k],
            };
        }
    }

    /// `y_spek` = Σ W_p · X_{k−p} für Vorder- oder Hintergrund.
    fn schaetzen(&mut self, vorn: bool) {
        self.y_spek = [K::default(); BINS];
        let w_alle = if vorn { &self.w_vorn } else { &self.w };
        for (p, w) in w_alle.iter().enumerate() {
            let x = &self.x[(self.x_pos + PARTITIONEN - p) % PARTITIONEN];
            for k in 0..BINS {
                self.y_spek[k].re += w[k].re * x[k].re - w[k].im * x[k].im;
                self.y_spek[k].im += w[k].re * x[k].im + w[k].im * x[k].re;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Reproduzierbares Rauschen (xorshift), −1..1.
    struct Rauschen(u32);
    impl Rauschen {
        fn next(&mut self) -> f32 {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 17;
            self.0 ^= self.0 << 5;
            (self.0 as f32 / u32::MAX as f32) * 2.0 - 1.0
        }
    }

    /// Raum: Verzögerung (Vorgabe 30 ms), gedämpft, mit zwei Reflexionen.
    struct Raum {
        verlauf: Vec<f32>,
        pos: usize,
        wege: [(usize, f32); 3],
    }
    impl Raum {
        fn neu() -> Raum {
            Raum {
                verlauf: vec![0.0; 12_000],
                pos: 0,
                wege: [(1440, 0.5), (1700, 0.2), (3100, -0.1)],
            }
        }
        fn process(&mut self, x: f32) -> f32 {
            let n = self.verlauf.len();
            self.verlauf[self.pos] = x;
            let y = self
                .wege
                .iter()
                .map(|&(d, g)| g * self.verlauf[(self.pos + n - d) % n])
                .sum();
            self.pos = (self.pos + 1) % n;
            y
        }
    }

    fn leistung(v: &[f32]) -> f32 {
        v.iter().map(|x| x * x).sum::<f32>() / v.len() as f32
    }

    fn neu() -> Box<Echounterdruecker> {
        // Gut 200 KiB Zustand: auf dem Heap bauen.
        std::thread::Builder::new()
            .stack_size(16 * 1024 * 1024)
            .spawn(|| Box::new(Echounterdruecker::neu()))
            .unwrap()
            .join()
            .unwrap()
    }

    #[test]
    fn reines_echo_wird_um_mehr_als_30_db_gesenkt() {
        let mut aec = neu();
        let mut raum = Raum::neu();
        let mut r = Rauschen(1);
        let sr = 48_000;
        let mut mic_letzte = Vec::new();
        let mut aus_letzte = Vec::new();
        for i in 0..sr * 6 {
            let boxen = 0.3 * r.next();
            let mic = raum.process(boxen);
            let aus = aec.process(mic, boxen);
            if i >= sr * 5 {
                mic_letzte.push(mic);
                aus_letzte.push(aus);
            }
        }
        let erle = 10.0 * (leistung(&mic_letzte) / leistung(&aus_letzte)).log10();
        eprintln!("ERLE {erle:.1} dB");
        assert!(erle > 30.0, "ERLE nur {erle:.1} dB");
    }

    #[test]
    fn stille_referenz_laesst_das_mikrofon_durch() {
        let mut aec = neu();
        let mut r = Rauschen(7);
        let eingang: Vec<f32> = (0..4800).map(|_| 0.2 * r.next()).collect();
        let aus: Vec<f32> = eingang.iter().map(|&x| aec.process(x, 0.0)).collect();
        // Um einen Block verzögert, sonst unverändert.
        for n in BLOCK..eingang.len() {
            assert!((aus[n] - eingang[n - BLOCK]).abs() < 1e-5, "Sample {n}");
        }
    }

    #[test]
    fn stimme_bleibt_beim_doppelsprechen_erhalten() {
        let mut aec = neu();
        let mut raum = Raum::neu();
        let mut r = Rauschen(3);
        let mut s = Rauschen(99);
        let sr = 48_000;
        // Erst lernen (nur Systemton) …
        for _ in 0..sr * 4 {
            let boxen = 0.3 * r.next();
            aec.process(raum.process(boxen), boxen);
        }
        // … dann spricht jemand dazu: die Stimme muss heraus, das Echo nicht.
        let mut stimme = Vec::new();
        let mut aus = Vec::new();
        for i in 0..sr {
            let boxen = 0.3 * r.next();
            let v = 0.2 * s.next() * (i as f32 * 0.001).sin().abs();
            stimme.push(v);
            aus.push(aec.process(raum.process(boxen) + v, boxen));
        }
        // Ausgabe ist um BLOCK verzögert; Rest = Ausgabe − Stimme.
        let rest: Vec<f32> = (BLOCK..sr).map(|n| aus[n] - stimme[n - BLOCK]).collect();
        let snr = 10.0 * (leistung(&stimme) / leistung(&rest)).log10();
        eprintln!("Stimme/Rest {snr:.1} dB");
        assert!(snr > 20.0, "Stimme zu stark verändert: {snr:.1} dB");
    }

    #[test]
    fn nach_einem_raumwechsel_lernt_er_neu() {
        let mut aec = neu();
        let mut raum = Raum::neu();
        let mut r = Rauschen(11);
        let sr = 48_000;
        for _ in 0..sr * 3 {
            let b = 0.3 * r.next();
            aec.process(raum.process(b), b);
        }
        // Jemand stellt die Boxen um: anderer Weg, andere Laufzeit.
        raum.wege = [(2400, 0.4), (2900, -0.25), (6000, 0.1)];
        let (mut mic, mut aus) = (Vec::new(), Vec::new());
        for i in 0..sr * 4 {
            let b = 0.3 * r.next();
            let m = raum.process(b);
            let a = aec.process(m, b);
            if i >= sr * 3 {
                mic.push(m);
                aus.push(a);
            }
        }
        let erle = 10.0 * (leistung(&mic) / leistung(&aus)).log10();
        eprintln!("ERLE nach Wechsel {erle:.1} dB");
        assert!(erle > 20.0, "ERLE nach Wechsel nur {erle:.1} dB");
    }

    #[test]
    fn reset_vergisst_den_filter() {
        let mut aec = neu();
        let mut r = Rauschen(5);
        for _ in 0..48_000 {
            let b = 0.3 * r.next();
            aec.process(0.5 * b, b);
        }
        aec.reset();
        let null = |w: &Vec<[K; BINS]>| {
            w.iter()
                .all(|p| p.iter().all(|k| k.re == 0.0 && k.im == 0.0))
        };
        assert!(null(&aec.w) && null(&aec.w_vorn));
    }
}
