//! Korrelation Referenz ↔ Mikro über alle Laufzeiten, je Frequenzband.
//! `cargo run --release -p crosspoint-core --example echo_band -- ref.wav mic.wav`
fn lies(p: &str) -> Vec<f32> {
    let mut r = hound::WavReader::open(p).unwrap();
    let ch = r.spec().channels as usize;
    let s: Vec<f32> = r
        .samples::<i16>()
        .map(|x| x.unwrap() as f32 / 32768.0)
        .collect();
    s.chunks(ch).map(|c| c[0]).collect()
}
/// Einfacher Bandpass: Differenz zweier Tiefpässe erster Ordnung.
fn band(x: &[f32], unten: f32, oben: f32) -> Vec<f32> {
    let k = |f: f32| 1.0 - (-2.0 * std::f32::consts::PI * f / 48_000.0).exp();
    let (ku, ko) = (k(unten), k(oben));
    let (mut a, mut b) = (0f32, 0f32);
    x.iter()
        .map(|&v| {
            a += ku * (v - a);
            b += ko * (v - b);
            b - a
        })
        .collect()
}
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let (r, m) = (lies(&a[1]), lies(&a[2]));
    let n = r.len().min(m.len()) - 30_000;
    for (name, u, o) in [
        ("breit", 20.0, 20_000.0),
        ("150-600 Hz", 150.0, 600.0),
        ("600-2500 Hz", 600.0, 2500.0),
        ("2,5-8 kHz", 2500.0, 8000.0),
    ] {
        let (rb, mb) = (band(&r, u, o), band(&m, u, o));
        let rr: f64 = rb[..n].iter().map(|v| (*v as f64).powi(2)).sum();
        let mm: f64 = mb[..n].iter().map(|v| (*v as f64).powi(2)).sum();
        let mut best = (0usize, 0f64);
        for lag in 0..28_800 {
            let c: f64 = (0..n)
                .step_by(3)
                .map(|i| rb[i] as f64 * mb[i + lag] as f64)
                .sum::<f64>()
                * 3.0;
            let k = c / (rr * mm).sqrt();
            if k.abs() > best.1.abs() {
                best = (lag, k);
            }
        }
        println!(
            "{name:12}: beste Korrelation {:+.3} bei {:.1} ms",
            best.1,
            best.0 as f32 / 48.0
        );
    }
    // Wandert die Laufzeit? Je Sekunde die beste Laufzeit (150-600 Hz).
    let (rb, mb) = (band(&r, 150.0, 600.0), band(&m, 150.0, 600.0));
    for s in 0..(n / 48_000) {
        let a0 = s * 48_000;
        let mut best = (0usize, 0f64);
        for lag in 1_800..2_800 {
            let c: f64 = (a0..a0 + 48_000)
                .map(|i| rb[i] as f64 * mb[i + lag] as f64)
                .sum();
            if c.abs() > best.1.abs() {
                best = (lag, c);
            }
        }
        let rr: f64 = rb[a0..a0 + 48_000]
            .iter()
            .map(|v| (*v as f64).powi(2))
            .sum();
        let mm: f64 = mb[a0 + best.0..a0 + best.0 + 48_000]
            .iter()
            .map(|v| (*v as f64).powi(2))
            .sum();
        println!(
            "  Sekunde {s}: Laufzeit {} Samples, Korrelation {:+.3}",
            best.0,
            best.1 / (rr * mm).sqrt()
        );
    }
}
