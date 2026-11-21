//! ECHO mit echten Aufnahmen nachmessen.
//!
//! `cargo run --release -p crosspoint-core --example echo_probe -- referenz.wav mikro.wav`
//!
//! Gemessen wird die Echounterdrückung, wie sie in der Engine läuft (AEC3 aus
//! `sonora`, siehe `src/aec.rs`).
//!
//! `referenz.wav`: was zu den Boxen geht (z. B. Monitor von crosspoint_in_1),
//! `mikro.wav`: gleichzeitig aufgenommenes Mikrofon. Beide mono, 48 kHz.
//! Ausgabe: Laufzeit Referenz → Mikro (Kreuzkorrelation) und die
//! Echodämpfung des Echounterdrückers je zusätzlicher Verzögerung.

use crosspoint::aec::Echounterdruecker;

fn lies(p: &str) -> Vec<f32> {
    let mut r = hound::WavReader::open(p).unwrap_or_else(|e| panic!("{p}: {e}"));
    let ch = r.spec().channels as usize;
    let s: Vec<f32> = r
        .samples::<i16>()
        .map(|x| x.unwrap() as f32 / 32768.0)
        .collect();
    s.chunks(ch).map(|c| c[0]).collect()
}

fn leistung_db(x: &[f32]) -> f32 {
    10.0 * (x.iter().map(|v| v * v).sum::<f32>() / x.len().max(1) as f32 + 1e-12).log10()
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let (r, m) = (lies(&a[1]), lies(&a[2]));
    let n = r.len().min(m.len());
    // Laufzeit: Kreuzkorrelation 0..600 ms (grob über 4er-Schritte, dann fein).
    let korr = |lag: usize| -> f32 { (0..n - lag).step_by(2).map(|i| r[i] * m[i + lag]).sum() };
    let grob = (0..28_800)
        .step_by(4)
        .max_by(|&x, &y| korr(x).abs().total_cmp(&korr(y).abs()))
        .unwrap();
    let lag = (grob.saturating_sub(8)..grob + 8)
        .max_by(|&x, &y| korr(x).abs().total_cmp(&korr(y).abs()))
        .unwrap();
    println!(
        "Laufzeit Referenz -> Mikro: {lag} Samples = {:.1} ms",
        lag as f32 / 48.0
    );
    println!(
        "Mikro {:.1} dB, Referenz {:.1} dB",
        leistung_db(&m[..n]),
        leistung_db(&r[..n])
    );
    // Wie eng hängen Mikro und Referenz zusammen? Korrelation je Abschnitt
    // (1 s) bei der gefundenen Laufzeit; 1,0 = rein lineares, festes Echo.
    for (k, a0) in (0..n - lag - 48_000).step_by(48_000).enumerate() {
        let (mut rm, mut rr, mut mm) = (0f64, 0f64, 0f64);
        for i in a0..a0 + 48_000 {
            let (x, y) = (r[i] as f64, m[i + lag] as f64);
            rm += x * y;
            rr += x * x;
            mm += y * y;
        }
        println!(
            "  Sekunde {k}: Korrelation bei {lag} = {:.3}",
            rm / (rr * mm).sqrt().max(1e-12)
        );
    }
    // Gegenprobe: künstliches Mikro mit denselben Pegeln (Echo = Referenz
    // um die Laufzeit verzögert und auf den Mikropegel gedämpft, dazu Rauschen
    // auf dem Pegel der Ruhe).
    let g = (10f32.powf((leistung_db(&m[..n]) - leistung_db(&r[..n])) / 20.0)) * 0.9;
    let mut z = 1u32;
    let kuenstlich: Vec<f32> = (0..n)
        .map(|i| {
            z ^= z << 13;
            z ^= z >> 17;
            z ^= z << 5;
            let rausch = ((z as f32 / u32::MAX as f32) * 2.0 - 1.0) * 0.0012;
            if i >= lag {
                g * r[i - lag] + rausch
            } else {
                rausch
            }
        })
        .collect();
    let start = n / 2; // zweite Hälfte = nach dem Einlernen
    {
        let mut aec = Box::new(Echounterdruecker::neu(48_000));
        let b = aec.verzoegerung();
        let aus: Vec<f32> = (0..n).map(|i| aec.process(kuenstlich[i], r[i])).collect();
        println!(
            "Gegenprobe künstliches Mikro (gleiche Pegel): Echo gesenkt um {:.1} dB",
            leistung_db(&kuenstlich[start..n - b]) - leistung_db(&aus[start + b..n])
        );
    }
    for extra in [0usize, 4800, 9600, 14_400] {
        let mut aec = Box::new(Echounterdruecker::neu(48_000));
        let b = aec.verzoegerung();
        let aus: Vec<f32> = (0..n)
            .map(|i| {
                let rv = if i >= extra { r[i - extra] } else { 0.0 };
                aec.process(m[i], rv)
            })
            .collect();
        let erle = leistung_db(&m[start..n - b]) - leistung_db(&aus[start + b..n]);
        println!("Referenz zusätzlich {extra:>5} Samples verzögert: Echo gesenkt um {erle:5.1} dB");
    }
}
