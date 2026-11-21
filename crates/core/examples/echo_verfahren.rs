//! Was die Echounterdrückung auf welchem Echopfad schafft – und dass sie im
//! Betrieb nichts allokiert.
//!
//! `cargo run --release -p crosspoint-core --example echo_verfahren [quelle.wav]`
//!
//! Ohne Argument wird ein Rauschsignal benutzt, mit Argument eine Aufnahme
//! (mono, 48 kHz). Vier Pfade, stufenweise schwieriger:
//!
//! 1. **linear** – das Echo ist die um 47 ms verzögerte Referenz.
//! 2. **+ Nachhall** – dazu ein Raum mit RT60 0,3 s.
//! 3. **+ Taktversatz** – das Mikrofon läuft 50 ppm schneller (getrennte Uhren
//!    von Ausgabe- und Aufnahmegerät, wie im echten Betrieb).
//! 4. **+ Begrenzung** – der kleine Lautsprecher verzerrt (weiche Begrenzung).
//!
//! Gemessen wird die Echodämpfung im eingeschwungenen Zustand (4–8 s), also
//! nach dem Einlernen.

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering};

use crosspoint::aec::Echounterdruecker;

/// Zählt Allokationen im ganzen Programm; gemessen wird nur um `process`.
struct Zaehler;
static ALLOCS: AtomicUsize = AtomicUsize::new(0);

unsafe impl GlobalAlloc for Zaehler {
    unsafe fn alloc(&self, l: Layout) -> *mut u8 {
        ALLOCS.fetch_add(1, Ordering::Relaxed);
        System.alloc(l)
    }
    unsafe fn alloc_zeroed(&self, l: Layout) -> *mut u8 {
        ALLOCS.fetch_add(1, Ordering::Relaxed);
        System.alloc_zeroed(l)
    }
    unsafe fn realloc(&self, p: *mut u8, l: Layout, neu: usize) -> *mut u8 {
        ALLOCS.fetch_add(1, Ordering::Relaxed);
        System.realloc(p, l, neu)
    }
    unsafe fn dealloc(&self, p: *mut u8, l: Layout) {
        System.dealloc(p, l)
    }
}

#[global_allocator]
static SPION: Zaehler = Zaehler;

fn db(x: &[f32]) -> f32 {
    10.0 * (x.iter().map(|v| v * v).sum::<f32>() / x.len().max(1) as f32 + 1e-20).log10()
}

/// Ein Echopfad: Referenz hinein, Mikrofon heraus.
fn pfad(r: &[f32], nachhall: bool, versatz: bool, begrenzung: bool) -> Vec<f32> {
    let n = r.len();
    let vorlauf = 2256; // 47 ms
    let mut ir = vec![0f32; vorlauf + if nachhall { 3000 } else { 1 }];
    ir[vorlauf] = 1.0;
    if nachhall {
        let mut z = 99_991u32;
        for (k, v) in ir.iter_mut().enumerate().skip(vorlauf + 1) {
            z ^= z << 13;
            z ^= z >> 17;
            z ^= z << 5;
            let t = (k - vorlauf) as f32 / 48_000.0;
            *v = 0.35 * (-6.9 * t / 0.3).exp() * ((z as f32 / u32::MAX as f32) * 2.0 - 1.0);
        }
        let e: f32 = ir.iter().map(|v| v * v).sum::<f32>().sqrt();
        for v in ir.iter_mut() {
            *v /= e;
        }
    }
    let mut echo = vec![0f32; n];
    for i in vorlauf..n {
        let mut s = 0.0;
        for (k, g) in ir.iter().enumerate() {
            if i >= k {
                s += g * r[i - k];
            }
        }
        echo[i] = s;
    }
    // Das Echo kommt 20 dB leiser an, als die Referenz laut ist.
    let g = 10f32.powf((db(r) - 20.0 - db(&echo)) / 20.0);
    let mut z = 12_345u32;
    let mut mik: Vec<f32> = (0..n)
        .map(|i| {
            z ^= z << 13;
            z ^= z >> 17;
            z ^= z << 5;
            let rausch = ((z as f32 / u32::MAX as f32) * 2.0 - 1.0) * 3.0e-4;
            let e = g * echo[i];
            let e = if begrenzung {
                (e * 25.0).tanh() / 25.0
            } else {
                e
            };
            e + rausch
        })
        .collect();
    if versatz {
        // Getrennte Uhren: das Mikrofon läuft 50 ppm schneller.
        let mut v = Vec::with_capacity(n);
        let mut pos = 0f64;
        while v.len() < n {
            let i = pos as usize;
            let f = (pos - i as f64) as f32;
            let a = *mik.get(i).unwrap_or(&0.0);
            let b = *mik.get(i + 1).unwrap_or(&0.0);
            v.push(a + f * (b - a));
            pos += 1.000_05;
        }
        mik = v;
    }
    mik
}

fn main() {
    let quelle: Vec<f32> = match std::env::args().nth(1) {
        Some(pfad) => {
            let mut r = hound::WavReader::open(&pfad).expect("WAV lesbar");
            let ch = r.spec().channels as usize;
            r.samples::<i16>()
                .map(|x| x.unwrap() as f32 / 32768.0)
                .collect::<Vec<f32>>()
                .chunks(ch)
                .map(|c| c[0])
                .collect()
        }
        None => {
            // Rauschen als Referenz, wenn keine Aufnahme vorliegt.
            let mut z = 1u32;
            (0..48_000 * 10)
                .map(|_| {
                    z ^= z << 13;
                    z ^= z >> 17;
                    z ^= z << 5;
                    (z as f32 / u32::MAX as f32) * 2.0 - 1.0
                })
                .collect()
        }
    };
    let n = 48_000 * 10;
    let r: Vec<f32> = (0..n).map(|i| quelle[i % quelle.len()] * 0.25).collect();

    println!("{:26} {:>12} {:>12}", "Echopfad", "AEC3", "AEC3 + NS");
    for (name, nachhall, versatz, begrenzung) in [
        ("1 linear", false, false, false),
        ("2 + Nachhall", true, false, false),
        ("3 + Taktversatz 50 ppm", true, true, false),
        ("4 + Begrenzung", true, true, true),
    ] {
        let mik = pfad(&r, nachhall, versatz, begrenzung);
        let mut zeile = String::new();
        for ns in [false, true] {
            let mut aec = Box::new(Echounterdruecker::neu(48_000));
            // `set_filter` allokiert einmal – gehört deshalb vor die Messung.
            aec.set_filter(true, ns, 1);
            let d = aec.verzoegerung();
            let aus: Vec<f32> = (0..n).map(|i| aec.process(mik[i], r[i])).collect();
            let von = 4 * 48_000;
            let bis = 8 * 48_000;
            zeile.push_str(&format!(
                " {:>+9.1} dB",
                db(&mik[von..bis]) - db(&aus[von + d..bis + d])
            ));
        }
        println!("{name:26}{zeile}");
    }

    // Allokationen im laufenden Betrieb: Puffer sind angelegt, `process` darf
    // nichts mehr bauen.
    let mut aec = Box::new(Echounterdruecker::neu(48_000));
    aec.set_filter(true, true, 1);
    let mik = pfad(&r[..48_000], true, true, true);
    for i in 0..4800 {
        aec.process(mik[i], r[i]);
    }
    let vorher = ALLOCS.load(Ordering::Relaxed);
    let mut summe = 0.0f32;
    for i in 4800..48_000 {
        summe += aec.process(mik[i], r[i]);
    }
    let nachher = ALLOCS.load(Ordering::Relaxed);
    println!(
        "\nAllokationen in {} Frames Audio: {} ({summe:.2})",
        48_000 - 4800,
        nachher - vorher
    );
}
