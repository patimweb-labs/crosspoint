//! Rauschunterdrueckung (NR, Modus "eigen") gegen Sprache.
//!
//! Modell: Rauschteppich bei rund -52 dBFS, eine Sekunde allein zum
//! Einlernen, dann 4 s "Sprache" (breitbandiges Rauschen im 4-Hz-Silbentakt
//! moduliert), dann 3 s Pause. Gemessen wird
//!
//! * der Pegelverlust der Sprache frueh (0,25..0,75 s) und spaet (2..4 s) –
//!   steigt er spaet an, haelt die Rauschschaetzung Sprache fuer Rauschen;
//! * die Absenkung des Rauschens in der Pause.
//!
//! Vorher (Mittel der Blockminima, langsames Oeffnen) verlor laute Sprache
//! mit Luecken rund 2 dB, ohne Luecken stieg der Verlust bei 14 dB Abstand
//! spaet um fast 3 dB an.
use crosspoint::dsp::NoiseReduction;

const SR: f32 = 48_000.0;
const LATENZ: usize = 512;

struct Rausch(u32);
impl Rausch {
    fn next(&mut self) -> f32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 17;
        self.0 ^= self.0 << 5;
        (self.0 as f32 / u32::MAX as f32) * 2.0 - 1.0
    }
}

fn rms(v: &[f32]) -> f32 {
    (v.iter().map(|x| x * x).sum::<f32>() / v.len() as f32).sqrt()
}

fn db(v: f32) -> f32 {
    20.0 * v.max(1e-12).log10()
}

struct Ergebnis {
    frueh: f32,
    spaet: f32,
    rauschen: f32,
}

/// `amp`: Spitzenamplitude der Sprache; `luecken`: Silben mit echten Pausen.
fn lauf(staerke: f32, amp: f32, luecken: bool) -> Ergebnis {
    let mut nr = NoiseReduction::new(SR);
    nr.enabled = true;
    let mut r = Rausch(12345);
    let mut s = Rausch(999);
    let start = SR as usize;
    let n_sprache = (4.0 * SR) as usize;
    let gesamt = start + n_sprache + (3.0 * SR) as usize;
    let mut sprache = Vec::with_capacity(gesamt);
    let mut rauschen = Vec::with_capacity(gesamt);
    let mut aus = Vec::with_capacity(gesamt);
    for i in 0..gesamt {
        let t = i as f32 / SR;
        let n = 0.004 * r.next();
        let sp = if (start..start + n_sprache).contains(&i) {
            let w = (2.0 * std::f32::consts::PI * 4.0 * t).sin();
            let huelle = if luecken { w.max(0.0) } else { 0.5 + 0.5 * w };
            amp * huelle * s.next()
        } else {
            0.0
        };
        aus.push(nr.process(sp + n, staerke, -45.0, SR));
        sprache.push(sp);
        rauschen.push(n);
    }
    let verlust = |von: usize, bis: usize| {
        db(rms(&sprache[von..bis])) - db(rms(&aus[von + LATENZ..bis + LATENZ]))
    };
    let pv = gesamt - (1.5 * SR) as usize;
    Ergebnis {
        frueh: verlust(start + (0.25 * SR) as usize, start + (0.75 * SR) as usize),
        spaet: verlust(start + 2 * SR as usize, start + n_sprache),
        rauschen: db(rms(&rauschen[pv - LATENZ..gesamt - LATENZ])) - db(rms(&aus[pv..])),
    }
}

#[test]
fn laute_sprache_bleibt_erhalten() {
    for luecken in [false, true] {
        let e = lauf(0.6, 0.3, luecken);
        println!(
            "NR laut (luecken {luecken}): frueh {:.2} dB, spaet {:.2} dB, Rauschen -{:.2} dB",
            e.frueh, e.spaet, e.rauschen
        );
        assert!(e.frueh < 0.5, "Wortanfaenge verloren {:.2} dB", e.frueh);
        assert!(e.spaet < 0.5, "Dauersprache verlor {:.2} dB", e.spaet);
    }
}

#[test]
fn schaetzung_driftet_bei_dauersprache_nicht_hoch() {
    for luecken in [false, true] {
        let e = lauf(0.6, 0.03, luecken);
        println!(
            "NR mittel (luecken {luecken}): frueh {:.2} dB, spaet {:.2} dB",
            e.frueh, e.spaet
        );
        assert!(
            e.spaet - e.frueh < 1.0,
            "Verlust stieg von {:.2} auf {:.2} dB",
            e.frueh,
            e.spaet
        );
    }
}

#[test]
fn rauschen_in_der_pause_sinkt_mit_der_staerke() {
    let mut vorher = 0.0;
    for st in [0.3f32, 0.6, 1.0] {
        let a = lauf(st, 0.3, true).rauschen;
        println!("NR Staerke {st}: Rauschen -{a:.2} dB");
        assert!(a > 10.0, "Staerke {st}: nur {a:.2} dB");
        assert!(
            a > vorher,
            "Staerke {st} senkt nicht staerker als die vorige"
        );
        vorher = a;
    }
}
