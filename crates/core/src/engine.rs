//! Echtzeit-Kern: Parameter als Atome, DSP-Zustand ausschließlich im Audio-Thread.
//!
//! # Rohsignal an den Eingängen, Filter nur an den virtuellen Ausgängen
//!
//! Streifen 1..8:  Eingang → Mono → Fader → Pan → Routing (8 Busse).
//!   Keine Bearbeitung: auf jedem Bus, auf den ein Streifen geht, liegt sein
//!   Rohsignal. Wer es unverändert braucht (Aufnahme, Kopfhörer), nimmt einen
//!   Hardware-Bus.
//!
//! Busse A1..A5 (Hardware):  Summe → Mono/Modus → Delay → Trim → Gain/Mute.
//!
//! Busse B1..B3 (virtuell):  Summe → Mono/Modus → **Low-Cut** →
//!   **Echounterdrückung** → **Rauschunterdrückung** → **Noise Gate** →
//!   **Kompressor** → **AGC** → **Limiter** → **Ducking** → **PTT** →
//!   Delay → Trim → Gain/Mute.
//!   Die Echounterdrückung zieht ab, was die Boxen spielen. Die Referenz kommt
//!   von außen: an den Eingängen `echo_ref_L/R` hängen die Monitore der
//!   gewählten Ausgabegeräte (siehe [`crate::echo`]) – also genau der Ton, den
//!   die Boxen spielen, auch wenn ein Programm an Crosspoint vorbei auf das
//!   Gerät spielt (Chromium auf HDMI). Die übrigen Filter arbeiten am
//!   echofreien Signal: erst aufräumen (LC, NR), dann Pegel (Gate, COMP, AGC,
//!   Limiter), dann die Pegelsenkung fürs Ducking und die PTT-Stummschaltung.
//!   Reihenfolge und Begründung: `docs/adr/0006-filterkette-und-modal.md`.
//!
//! Die Filter stehen im Modell als [`AusgangsFilter`] – nur die virtuellen
//! Busse haben einen; ein Hardware-Bus nimmt `gate`/`aec`/`nr`/… nicht an.
//!
//! Es gibt **keinen** Mutex im Audio-Pfad: die Steuerung schreibt Atome, der
//! Callback liest sie mit `Relaxed`.

use std::sync::atomic::{Ordering, Ordering::Relaxed};

// Das Parametermodell steht im eigenen Crate (ADR 0012); hier wird es unter
// den alten Namen weiter angeboten, damit Fenster, OSC, HTTP und Tests
// unverändert `crosspoint::engine::Params` benutzen können.
pub use crosspoint_modell::params::{
    fget, filter_get, fset, AusgangsFilter, BusParam, Params, StripParam, BUS_MODE_COMPOSITE,
    BUS_MODE_DOWNMIX_A, BUS_MODE_DOWNMIX_B, BUS_MODE_MAX, BUS_MODE_NORMAL, BUS_MODE_STEREO_REPEAT,
    BUS_MODE_TV_MIX, BUS_MODE_UPMIX_21, BUS_MODE_UPMIX_41, BUS_MODE_UPMIX_61, FILTER_PARAMS,
    MAX_LABEL, NR_STUFEN,
};

use crate::aec::Echounterdruecker;
use crate::agc::Pegelregler;
use crate::dsp::*;
use crate::{BUSES, STRIPS};

/// Erster virtueller Bus (B1).
pub const ERSTER_VIRTUELLER: usize = 5;
/// Anzahl der virtuellen Busse (B1..B3).
pub const VIRTUELLE: usize = BUSES - ERSTER_VIRTUELLER;

/// −3 dB als Linearwert; Dämpfung für Composite und TV-Mix.
const MINUS_3DB: f32 = 0.707_945_8;

/// Kanalmischung eines Busses am Bus-Eingang, vor Filtern/Delay/Trim.
///
/// | Modus | links | rechts |
/// |---|---|---|
/// | 0 Normal | L | R |
/// | 1 Downmix A | 0,5·(L+R) | 0 |
/// | 2 Downmix B | 0 | 0,5·(L+R) |
/// | 3 Stereo Repeat | 0,5·(L+R) | 0,5·(L+R) |
/// | 4 Composite | 0,5·(L+R) −3 dB | 0,5·(L+R) −3 dB |
/// | 5 TV-Mix | 0,5·(L+R) −3 dB | 0 |
/// | 6/7/8 Upmix 2.1/4.1/6.1 | 0,5·(L+R) | 0,5·(L+R) |
///
/// `cross` tauscht anschließend L und R.
///
/// **Upmix 2.1/4.1/6.1:** Im Original legen diese Modi das Monosignal zusätzlich
/// auf Center, LFE und Rear. Ein Bus ist hier immer stereo – es bleibt die
/// Monosumme auf beiden Kanälen.
#[inline]
fn bus_mix(v: [f32; 2], mode: u32, cross: bool) -> [f32; 2] {
    let sum = 0.5 * (v[0] + v[1]);
    let mut o = match mode {
        BUS_MODE_DOWNMIX_A => [sum, 0.0],
        BUS_MODE_DOWNMIX_B => [0.0, sum],
        BUS_MODE_STEREO_REPEAT => [sum, sum],
        BUS_MODE_COMPOSITE => [sum * MINUS_3DB; 2],
        BUS_MODE_TV_MIX => [sum * MINUS_3DB, 0.0],
        BUS_MODE_UPMIX_21 | BUS_MODE_UPMIX_41 | BUS_MODE_UPMIX_61 => [sum, sum],
        _ => v,
    };
    if cross {
        o.swap(0, 1);
    }
    o
}

/// Zeiger auf die Kanalpuffer eines Blocks (aus den JACK-Ports).
pub struct Io {
    pub ins: [[*const f32; 2]; STRIPS],
    pub outs: [[*mut f32; 2]; BUSES],
    /// Referenz der Echounterdrückung (`echo_ref_L/R`): was die Boxen spielen.
    /// `null` heißt „kein Ausgabegerät gewählt“ – dann bleibt die
    /// Echounterdrückung still und lässt das Mikrofon unverändert.
    pub echo_ref: [*const f32; 2],
}

/// Filterzustand eines Zugs – Kanalzug oder virtueller Bus (beide Kanäle).
struct FilterDsp {
    gate: [Gate; 2],
    aec: [Echounterdruecker; 2],
    /// Low-Cut je Kanal (Hochpass 2. Ordnung).
    lc: [Biquad; 2],
    /// Kompressor je Kanal, gekoppelt über `max(|L|, |R|)`.
    comp: [Kompressor; 2],
    /// AGC2 je Kanal (eigener Rahmen-Sammler, 10 ms Verzögerung).
    agc: [Pegelregler; 2],
    /// Limiter je Kanal, gleicher Detektor wie der Kompressor.
    limiter: [Limiter; 2],
    /// Ducking-Absenkung dieses Busses (Quelle: dieser Bus).
    duck: Ducker,
    /// War die Echounterdrückung im vorigen Block an? Beim Einschalten wird
    /// sie zurückgesetzt, damit kein alter Raum nachwirkt.
    aec_war_an: bool,
    /// Lief die Echounterdrückung im vorigen Block (`echo_an`)?
    echo_an: bool,
    /// Lief die Rauschunterdrückung im vorigen Block?
    nr_an: bool,
    /// Stand von ECHO und NR im vorigen Block (`sonora`-Konfiguration):
    /// (Echounterdrückung an, Rauschunterdrückung an, Stufe).
    filter_war: (bool, bool, u32),
    /// War AGC im vorigen Block an? Beim Einschalten wird der Sammler geleert.
    agc_war_an: bool,
    /// Grenzfrequenz des Low-Cut im vorigen Block (0 = noch nie gesetzt).
    lc_freq: f32,
    /// Schalter und Werte des Duckings aus `reconfigure` (nicht je Sample aus
    /// den Atomen lesen).
    duck_on: bool,
    duck_ziel: usize,
    duck_schwelle: f32,
    duck_tiefe: f32,
    /// Ist der Bus durch PTT stumm (`ptt` an, `ptt_offen` aus)?
    ptt_stumm: bool,
    /// Pegel dieses Zuges **nach den Filtern**: laufender und voriger Block.
    pegel_jetzt: f32,
    pegel_alt: f32,
}

impl FilterDsp {
    /// Die Puffer entstehen hier, außerhalb des Callbacks (AEC3 und AGC2
    /// allokieren beim Bauen).
    fn neu(abtastrate: u32) -> FilterDsp {
        FilterDsp {
            gate: [Gate::default(); 2],
            aec: [
                Echounterdruecker::neu(abtastrate),
                Echounterdruecker::neu(abtastrate),
            ],
            lc: [Biquad::default(); 2],
            comp: [Kompressor::default(); 2],
            agc: [Pegelregler::neu(abtastrate), Pegelregler::neu(abtastrate)],
            limiter: [Limiter::default(); 2],
            duck: Ducker::default(),
            aec_war_an: false,
            echo_an: false,
            nr_an: false,
            filter_war: (false, false, 0),
            agc_war_an: false,
            lc_freq: 0.0,
            duck_on: false,
            duck_ziel: 0,
            duck_schwelle: -30.0,
            duck_tiefe: db_to_lin(-12.0),
            ptt_stumm: false,
            pegel_jetzt: 0.0,
            pegel_alt: 0.0,
        }
    }

    /// Schalter und Werte eines Filterblocks übernehmen. Wird je Block einmal
    /// gerufen: exp/sin in den Reglern kosten so nur einmal je Block.
    fn reconfigure(&mut self, fp: &AusgangsFilter, sr: f32) {
        let g = &fp.gate;
        for gate in self.gate.iter_mut() {
            gate.enabled = g.on.load(Relaxed);
            gate.set_tiefe(fget(&g.tiefe));
            gate.set_times(fget(&g.attack), fget(&g.hold), fget(&g.release), sr);
        }
        // Low-Cut: die Koeffizienten nur neu rechnen, wenn sich die Frequenz
        // (oder der Filter) geändert hat.
        let lc_an = fp.lc.on.load(Relaxed);
        let lc_freq = fget(&fp.lc.freq);
        if lc_an && (lc_freq != self.lc_freq || self.lc_freq == 0.0) {
            for ch in self.lc.iter_mut() {
                ch.setze_hochpass(lc_freq, sr);
            }
            self.lc_freq = lc_freq;
        }
        // Kompressor und Limiter: Werte je Block setzen (exp/sin kosten hier
        // nur einmal je Block, nicht je Sample).
        let c = &fp.comp;
        for ch in self.comp.iter_mut() {
            ch.enabled = c.on.load(Relaxed);
            ch.set_werte(
                fget(&c.schwelle),
                fget(&c.ratio),
                fget(&c.attack),
                fget(&c.release),
                fget(&c.makeup),
                sr,
            );
        }
        let limiter_an = c.limiter.load(Relaxed);
        for ch in self.limiter.iter_mut() {
            ch.enabled = limiter_an;
            ch.set_werte(fget(&c.decke), 50.0, sr);
        }
        // AGC: Zielpegel je Block, Sammler beim Einschalten leeren.
        let agc_an = fp.agc.on.load(Relaxed);
        let ziel = fget(&fp.agc.ziel);
        for ch in self.agc.iter_mut() {
            ch.set_ziel(ziel);
            if agc_an && !self.agc_war_an {
                ch.reset();
            }
        }
        self.agc_war_an = agc_an;
        // Ducking: Schalter und Werte je Block in den Zustand.
        let d = &fp.duck;
        self.duck_on = d.on.load(Relaxed);
        self.duck_ziel = (d.ziel.load(Relaxed) as usize).min(BUSES - 1);
        self.duck_schwelle = fget(&d.schwelle);
        self.duck_tiefe = db_to_lin(-fget(&d.tiefe));
        self.duck.set_times(fget(&d.attack), fget(&d.release), sr);
        if !self.duck_on {
            self.duck.reset();
        }
        // PTT: stumm, solange nicht gedrückt.
        self.ptt_stumm = fp.ptt.on.load(Relaxed) && !fp.ptt.offen.load(Relaxed);
        // ECHO und NR teilen sich eine `sonora`-Instanz; nur bei Änderung
        // umschalten (das allokiert, siehe `Echounterdruecker::set_filter`).
        let echo = fp.aec.load(Relaxed);
        let nr = fp.nr.on.load(Relaxed);
        let stufe = fp.nr.stufe.load(Relaxed);
        let stand = (echo, nr, stufe);
        if stand != self.filter_war {
            self.aec
                .iter_mut()
                .for_each(|a| a.set_filter(echo, nr, stufe as u8));
            self.filter_war = stand;
        }
        if echo && !self.aec_war_an {
            self.aec.iter_mut().for_each(Echounterdruecker::reset);
        }
        self.aec_war_an = echo;
        self.echo_an = echo;
        self.nr_an = nr;
    }

    /// Die Kette (1)–(7) auf ein Stereopaar.
    ///
    /// Reihenfolge und Begründung: ADR 0006. Sie ist für Kanalzüge und
    /// virtuelle Busse dieselbe (ADR 0011); nur der Ort in der Signalkette
    /// unterscheidet sich – am Kanalzug steht sie vor dem Fader, am Bus hinter
    /// der Summe.
    // Der Audio-Pfad liest je Index zwei parallele Felder (Signal und
    // Filterzustand); Iteratoren ueber eines davon machen ihn nicht lesbarer.
    #[allow(clippy::needless_range_loop)]
    fn kette(&mut self, fp: &AusgangsFilter, mut v: [f32; 2], referenz: f32) -> [f32; 2] {
        // (1) Low-Cut vor der Echounterdrückung.
        if fp.lc.on.load(Relaxed) {
            for ch in 0..2 {
                v[ch] = self.lc[ch].process(v[ch]);
            }
        }
        // (2) ECHO und NR (eine `sonora`-Instanz, beide Schalter).
        if self.echo_an || self.nr_an {
            for ch in 0..2 {
                v[ch] = self.aec[ch].process(v[ch], referenz);
            }
        }
        // (3) Gate: gekoppelter Detektor, beide Kanäle öffnen gemeinsam.
        let key = v[0].abs().max(v[1].abs());
        let schwelle = fget(&fp.gate.threshold);
        for ch in 0..2 {
            v[ch] = self.gate[ch].process_key(v[ch], key, schwelle);
        }
        // (4) Kompressor, gleicher gekoppelter Detektor.
        let key = v[0].abs().max(v[1].abs());
        for ch in 0..2 {
            v[ch] = self.comp[ch].process_key(v[ch], key);
        }
        // (5) AGC2 (nur wenn an: der Sammler kostet 10 ms Latenz).
        if self.agc_war_an {
            for ch in 0..2 {
                v[ch] = self.agc[ch].process(v[ch]);
            }
        }
        // (6) Limiter mit Decke.
        let key = v[0].abs().max(v[1].abs());
        for ch in 0..2 {
            v[ch] = self.limiter[ch].process_key(v[ch], key);
        }
        // Pegel nach den Filtern: Grundlage fürs Ducking im nächsten Block.
        self.pegel_jetzt = self.pegel_jetzt.max(v[0].abs().max(v[1].abs()));
        // (7) PTT: stumm, solange die Taste nicht gedrückt ist.
        if self.ptt_stumm {
            v = [0.0, 0.0];
        }
        v
    }

    /// Beitrag dieses Zuges zum Ducking: der Pegel aus dem **vorigen** Block
    /// (eine Blocklänge Verzögerung, siehe ADR 0006) entscheidet, wie weit der
    /// Zielbus abgesenkt wird.
    fn duckung(&mut self, att: &mut [f32; BUSES]) {
        if !self.duck_on {
            return;
        }
        let ueber = lin_to_db(self.pegel_alt) > self.duck_schwelle;
        let g = self.duck.step(ueber, self.duck_tiefe);
        let ziel = &mut att[self.duck_ziel];
        if g < *ziel {
            *ziel = g;
        }
    }
}

/// DSP-Zustand, ausschließlich vom Audio-Thread benutzt.
pub struct EngineDsp {
    bus_delay: [[DelayLine; 2]; BUSES],
    /// Filter der virtuellen Busse (Index `bi - ERSTER_VIRTUELLER`).
    filter: Vec<FilterDsp>,
    /// Filter der Kanalzüge (Index = Streifen).
    kanal_filter: Vec<FilterDsp>,
    pub blocks: u64,
}

impl EngineDsp {
    /// `abtastrate` legt die Rahmengröße der Echounterdrückung fest (10 ms) und
    /// gehört deshalb hierher, nicht in den Callback.
    pub fn new(max_block: usize, abtastrate: u32) -> EngineDsp {
        EngineDsp {
            bus_delay: std::array::from_fn(|_| {
                [DelayLine::new(max_block * 4), DelayLine::new(max_block * 4)]
            }),
            // Die Filter bringen ihre Puffer mit; sie entstehen hier,
            // außerhalb des Callbacks (AEC3 und AGC2 allokieren beim Bauen).
            filter: (0..VIRTUELLE).map(|_| FilterDsp::neu(abtastrate)).collect(),
            kanal_filter: (0..STRIPS).map(|_| FilterDsp::neu(abtastrate)).collect(),
            blocks: 0,
        }
    }

    fn reconfigure(&mut self, p: &Params, sr: f32) {
        for (i, b) in p.buses.iter().enumerate() {
            for ch in 0..2 {
                self.bus_delay[i][ch].set_ms(fget(&b.delay_ms[ch]), sr);
            }
        }
        for (v, f) in self.filter.iter_mut().enumerate() {
            let Some(fp) = p.buses[ERSTER_VIRTUELLER + v].filter.as_ref() else {
                continue;
            };
            f.reconfigure(fp, sr);
        }
        // Dieselbe Kette für die Kanalzüge (ADR 0011).
        for (i, f) in self.kanal_filter.iter_mut().enumerate() {
            f.reconfigure(&p.strips[i].filter, sr);
        }
    }

    /// Ein Block. Die Kanäle werden als rohe Zeiger übergeben, damit der
    /// Callback ohne Kopieren direkt in die JACK-Puffer schreiben kann.
    // Der Audio-Pfad liest je Index mehrere parallele Felder (Parameter,
    // Filterzustand, Pegel, Ein-/Ausgangszeiger); Iteratoren ueber eines
    // davon machen ihn nicht lesbarer.
    #[allow(clippy::needless_range_loop)]
    pub fn process(&mut self, p: &Params, io: &Io, frames: usize) {
        if frames == 0 {
            return;
        }
        let sr = fget(&p.sample_rate).max(8000.0);
        self.reconfigure(p, sr);

        // Ducking: der Pegel, den die Quellbusse im **vorigen** Block nach den
        // Filtern hatten, entscheidet über die Absenkung in diesem Block. Das
        // ist eine Blocklänge Verzögerung (bei 512 Frames 10,7 ms) – nötig,
        // weil der abzusenkende Bus in derselben Schleife schon dran gewesen
        // sein kann. Begründung und Messung: ADR 0006.
        for f in self.filter.iter_mut() {
            f.pegel_alt = f.pegel_jetzt;
            f.pegel_jetzt = 0.0;
        }
        for f in self.kanal_filter.iter_mut() {
            f.pegel_alt = f.pegel_jetzt;
            f.pegel_jetzt = 0.0;
        }

        let any_solo = p.strips.iter().any(|s| s.solo.load(Relaxed));
        let mut peak_s = [0f32; STRIPS];
        let mut peak_roh_s = [0f32; STRIPS];
        let mut peak_b = [0f32; BUSES];
        let mut peak_roh = [0f32; BUSES];

        for n in 0..frames {
            // Absenkung je Bus für dieses Sample: der stärkste Ducking-Wunsch
            // gilt. Die Hüllkurve läuft je Sample, damit die Absenkung weich
            // kommt und geht (Attack/Release aus den Parametern). Quellen sind
            // die virtuellen Busse und – seit ADR 0011 – die Kanalzüge.
            let mut duck_att = [1.0f32; BUSES];
            for f in self.filter.iter_mut() {
                f.duckung(&mut duck_att);
            }
            for f in self.kanal_filter.iter_mut() {
                f.duckung(&mut duck_att);
            }

            // Die Referenz der Echounterdrückung kommt von den Ports
            // `echo_ref_L/R` – dort hängen die Monitore der gewählten
            // Ausgabegeräte, unabhängig davon, welcher Bus sie spielt. Sie
            // steht vor beiden Schleifen, weil Kanalzüge sie seit ADR 0011
            // ebenso brauchen wie die Busse.
            let referenz = if io.echo_ref[0].is_null() {
                0.0
            } else {
                // SAFETY: wie oben, `frames` Samples je Port. Ein Kanal reicht
                // als Referenz; bei Stereo wird gemittelt.
                unsafe {
                    if io.echo_ref[1].is_null() {
                        *io.echo_ref[0].add(n)
                    } else {
                        0.5 * (*io.echo_ref[0].add(n) + *io.echo_ref[1].add(n))
                    }
                }
            };

            let mut acc = [[0f32; 2]; BUSES];
            for si in 0..STRIPS {
                let sp = &p.strips[si];
                let audible = !sp.mute.load(Relaxed) && (!any_solo || sp.solo.load(Relaxed));
                let gain = if audible {
                    db_to_lin(fget(&sp.gain))
                } else {
                    0.0
                };
                let pan = fget(&sp.pan).clamp(-1.0, 1.0);
                // SAFETY: JACK liefert je Port einen Puffer von `frames` Samples.
                let mut s = unsafe { [*io.ins[si][0].add(n), *io.ins[si][1].add(n)] };
                if sp.mono.load(Relaxed) {
                    let m = 0.5 * (s[0] + s[1]);
                    s = [m, m];
                }
                // „Roh“ fürs FILTER-Modal: nach Mono, vor der Kette.
                peak_roh_s[si] = peak_roh_s[si].max(s[0].abs().max(s[1].abs()));
                // Die Kette des Kanalzugs (ADR 0011): vor dem Fader, damit der
                // Fader die Limiter-Decke nicht aushebelt – dieselbe
                // Begründung wie beim Bus.
                s = self.kanal_filter[si].kette(&sp.filter, s, referenz);
                let g = [1.0 - pan, 1.0 + pan];
                let l = s[0] * gain * g[0].min(1.0);
                let r = s[1] * gain * g[1].min(1.0);
                peak_s[si] = peak_s[si].max(l.abs().max(r.abs()));
                for bi in 0..BUSES {
                    let send = fget(&sp.sends[bi]);
                    if send != 0.0 {
                        acc[bi][0] += l * send;
                        acc[bi][1] += r * send;
                    }
                }
            }

            // Alle Busse in einer Reihe.
            for bi in 0..BUSES {
                let bp = &p.buses[bi];
                let mut v = acc[bi];
                if bp.mono.load(Relaxed) {
                    let m = 0.5 * (v[0] + v[1]);
                    v = [m, m];
                }
                v = bus_mix(v, bp.mode.load(Relaxed), bp.cross.load(Relaxed));
                // "Roh" für die Pegelanzeige im FILTER-Modal: die Summe am
                // Bus-Eingang, vor allen Filtern.
                peak_roh[bi] = peak_roh[bi].max(v[0].abs().max(v[1].abs()));
                if let Some(fp) = bp.filter.as_ref() {
                    let f = &mut self.filter[bi - ERSTER_VIRTUELLER];
                    v = f.kette(fp, v, referenz);
                }
                // (8) Ducking: dieser Bus wird von einem anderen abgesenkt.
                let att = duck_att[bi];
                if att < 1.0 {
                    for ch in 0..2 {
                        v[ch] *= att;
                    }
                }
                let gain = if bp.mute.load(Relaxed) {
                    0.0
                } else {
                    db_to_lin(fget(&bp.gain))
                };
                for ch in 0..2 {
                    let mut x = self.bus_delay[bi][ch].process(v[ch]);
                    x *= db_to_lin(fget(&bp.trim[ch]));
                    v[ch] = x * gain;
                }
                peak_b[bi] = peak_b[bi].max(v[0].abs().max(v[1].abs()));
                // SAFETY: wie oben, `frames` Samples je Ausgangsport.
                unsafe {
                    *io.outs[bi][0].add(n) = v[0];
                    *io.outs[bi][1].add(n) = v[1];
                }
            }
        }

        for si in 0..STRIPS {
            let old = fget(&p.strips[si].level);
            fset(&p.strips[si].level, peak_s[si].max(old * 0.72));
            let old = fget(&p.strips[si].roh);
            fset(&p.strips[si].roh, peak_roh_s[si].max(old * 0.72));
        }
        for bi in 0..BUSES {
            let old = fget(&p.buses[bi].level);
            fset(&p.buses[bi].level, peak_b[bi].max(old * 0.72));
            let old = fget(&p.buses[bi].roh);
            fset(&p.buses[bi].roh, peak_roh[bi].max(old * 0.72));
        }
        // Letzte Handlung des Blocks: der Zaehler zeigt an, dass die Pegel
        // oben vollstaendig geschrieben sind. Nur ein Store, keine Allokation.
        p.frames.fetch_add(frames as u64, Ordering::Release);
        self.blocks += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Testpuffer: 2 Kanäle je Streifen/Bus, alle mit demselben Pegel gefüllt.
    struct Buf {
        ins: Vec<Vec<f32>>,
        outs: Vec<Vec<f32>>,
        /// Referenz der Echounterdrückung (`echo_ref`), ein Paar.
        echo_ref: [Vec<f32>; 2],
        /// Liegt sie an den Ports? Ohne Ausgabegerät bleiben die Ports still.
        echo_ref_an: bool,
    }

    impl Buf {
        /// Nur Streifen 0 wird angeregt, alle anderen bleiben still.
        fn new(level: f32, frames: usize) -> Buf {
            let mut b = Buf {
                ins: vec![vec![0.0f32; frames]; STRIPS * 2],
                outs: vec![vec![0.0f32; frames]; BUSES * 2],
                echo_ref: [vec![0.0f32; frames], vec![0.0f32; frames]],
                echo_ref_an: false,
            };
            b.set_strip(0, level);
            b
        }

        fn set_strip(&mut self, strip: usize, level: f32) {
            for ch in 0..2 {
                self.ins[strip * 2 + ch].iter_mut().for_each(|x| *x = level);
            }
        }

        /// Einzelner Eingangskanal eines Streifens (für L/R-unterschiedliche Tests).
        fn set_strip_ch(&mut self, strip: usize, ch: usize, level: f32) {
            self.ins[strip * 2 + ch].iter_mut().for_each(|x| *x = level);
        }

        /// Referenz der Echounterdrückung: was die Boxen spielen.
        fn set_echo_ref(&mut self, frame: impl Fn(usize) -> f32) {
            self.echo_ref_an = true;
            for n in 0..self.echo_ref[0].len() {
                let v = frame(n);
                self.echo_ref[0][n] = v;
                self.echo_ref[1][n] = v;
            }
        }

        fn io(&mut self) -> Io {
            let mut io = Io {
                ins: [[std::ptr::null(); 2]; STRIPS],
                outs: [[std::ptr::null_mut(); 2]; BUSES],
                echo_ref: [std::ptr::null(); 2],
            };
            for s in 0..STRIPS {
                for c in 0..2 {
                    io.ins[s][c] = self.ins[s * 2 + c].as_ptr();
                }
            }
            for b in 0..BUSES {
                for c in 0..2 {
                    io.outs[b][c] = self.outs[b * 2 + c].as_mut_ptr();
                }
            }
            if self.echo_ref_an {
                for c in 0..2 {
                    io.echo_ref[c] = self.echo_ref[c].as_ptr();
                }
            }
            io
        }

        fn bus(&self, bus: usize) -> f32 {
            self.outs[bus * 2][0]
        }

        /// Ausgang eines Bus-Kanals (0 = L, 1 = R).
        fn out(&self, bus: usize, ch: usize) -> f32 {
            self.outs[bus * 2 + ch][0]
        }
    }

    fn params() -> Params {
        Params::new(48000.0, 64)
    }

    /// `EngineDsp` ist gut 500 KiB gross; im Debug-Build entstehen beim
    /// Bauen mehrere Kopien auf dem Stack, und der 2-MiB-Stack eines
    /// Testthreads laeuft ueber. Wie in den Integrationstests: auf einem
    /// Thread mit grossem Stack bauen und als Box weiterreichen.
    fn dsp_neu() -> Box<EngineDsp> {
        std::thread::Builder::new()
            .stack_size(16 * 1024 * 1024)
            .spawn(|| Box::new(EngineDsp::new(64, 48_000)))
            .expect("Thread")
            .join()
            .expect("DSP")
    }

    #[test]
    fn durchgang_und_mute() {
        let p = params();
        let mut dsp = dsp_neu();
        let mut buf = Buf::new(0.5, 64);
        let io = buf.io();
        dsp.process(&p, &io, 64);
        assert!((buf.bus(0) - 0.5).abs() < 1e-6, "A1 war {}", buf.bus(0));
        p.set("strip", 0, "mute", 1.0);
        let io = buf.io();
        dsp.process(&p, &io, 64);
        assert!(buf.bus(0).abs() < 1e-6);
    }

    #[test]
    fn routing_auf_zweiten_bus() {
        let p = params();
        let mut dsp = dsp_neu();
        p.set("strip", 0, "send0", 0.0);
        p.set("strip", 0, "send1", 1.0);
        let mut buf = Buf::new(0.25, 64);
        let io = buf.io();
        dsp.process(&p, &io, 64);
        assert!(buf.bus(0).abs() < 1e-6);
        assert!((buf.bus(1) - 0.25).abs() < 1e-6, "A2 war {}", buf.bus(1));
    }

    #[test]
    fn solo_schaltet_andere_streifen_ab() {
        let p = params();
        let mut dsp = dsp_neu();
        p.set("strip", 1, "solo", 1.0);
        let mut buf = Buf::new(0.1, 64);
        buf.set_strip(1, 0.4);
        let io = buf.io();
        dsp.process(&p, &io, 64);
        assert!(
            (buf.bus(0) - 0.4).abs() < 1e-6,
            "nur Streifen 2 darf durch: {}",
            buf.bus(0)
        );
    }

    #[test]
    fn frame_zaehler_steigt_je_block() {
        let p = params();
        let mut dsp = dsp_neu();
        let mut buf = Buf::new(0.5, 64);
        assert_eq!(p.frame_counter(), 0);
        for erwartet in 1..=3u64 {
            let io = buf.io();
            dsp.process(&p, &io, 64);
            assert_eq!(p.frame_counter(), erwartet * 64);
        }
        // Ein Block anderer Laenge zaehlt seine echten Frames.
        let io = buf.io();
        dsp.process(&p, &io, 32);
        assert_eq!(p.frame_counter(), 3 * 64 + 32);
    }

    #[test]
    fn pegel_json_ist_ein_text_mit_frame_stand() {
        let p = params();
        let mut dsp = dsp_neu();
        let mut buf = Buf::new(0.5, 64);
        let io = buf.io();
        dsp.process(&p, &io, 64);

        let mut s = String::with_capacity(256);
        p.levels_into(&mut s);
        let j = crate::json::parse(&s).expect("JSON parsebar");
        assert_eq!(
            j.get("strips").and_then(|v| v.as_arr()).map(|a| a.len()),
            Some(STRIPS)
        );
        assert_eq!(
            j.get("buses").and_then(|v| v.as_arr()).map(|a| a.len()),
            Some(BUSES)
        );
        assert_eq!(j.get("f").and_then(|v| v.as_f64()), Some(64.0));
        // Streifen 1 liegt bei -6 dB (0,5 linear), die stillen Streifen bei -140.
        let strips = j.get("strips").and_then(|v| v.as_arr()).unwrap();
        assert_eq!(strips[0].as_f64().map(|x| x.round()), Some(-6.0));
        assert_eq!(strips[7].as_f64(), Some(-140.0));

        // Zweiter Aufruf schreibt in denselben Puffer, ohne Reste.
        // (Seit dem Frame-Zaehler endet das JSON auf "f":<n>}.)
        p.levels_into(&mut s);
        assert!(s.starts_with("{\"strips\":["), "{s}");
        assert!(s.ends_with('}'), "{s}");
        assert_eq!(s.matches("\"strips\":").count(), 1, "keine Reste: {s}");
        assert_eq!(s.matches("\"buses\":").count(), 1, "keine Reste: {s}");
    }

    #[test]
    fn gain_und_meter() {
        let p = params();
        let mut dsp = dsp_neu();
        p.set("strip", 0, "gain", -6.0);
        let mut buf = Buf::new(0.5, 64);
        let io = buf.io();
        dsp.process(&p, &io, 64);
        assert!((buf.bus(0) - 0.25).abs() < 1e-3, "A1 war {}", buf.bus(0));
        assert!((fget(&p.strips[0].level) - 0.25).abs() < 1e-3);
    }

    // ---- Bus-Mischmodi -------------------------------------------------

    /// Setzt `mode`/`cross` auf Bus A1, regt Streifen 0 mit L/R an und liefert
    /// den Ausgang (L, R) von A1 nach einem Block.
    fn mix_block(mode: f32, cross: f32, l: f32, r: f32) -> (f32, f32) {
        let p = params();
        let mut dsp = dsp_neu();
        assert!(p.set("bus", 0, "mode", mode), "mode {mode} nicht gesetzt");
        assert!(p.set("bus", 0, "cross", cross), "cross nicht gesetzt");
        let mut buf = Buf::new(0.0, 64);
        buf.set_strip_ch(0, 0, l);
        buf.set_strip_ch(0, 1, r);
        let io = buf.io();
        dsp.process(&p, &io, 64);
        (buf.out(0, 0), buf.out(0, 1))
    }

    /// Prüft beide Kanäle gegen Sollwerte.
    fn check(got: (f32, f32), want: (f32, f32), name: &str) {
        assert!(
            (got.0 - want.0).abs() < 1e-6,
            "{name}: L war {}, soll {} sein",
            got.0,
            want.0
        );
        assert!(
            (got.1 - want.1).abs() < 1e-6,
            "{name}: R war {}, soll {} sein",
            got.1,
            want.1
        );
    }

    #[test]
    fn bus_mode_normal_laesst_stereo_stehen() {
        // Vorgabe: mode 0, cross 0 -> L/R unverändert durch den Bus.
        check(mix_block(0.0, 0.0, 0.5, 0.25), (0.5, 0.25), "Normal");
        let p = params();
        assert_eq!(p.buses[0].mode.load(Relaxed), BUS_MODE_NORMAL);
        assert!(!p.buses[0].cross.load(Relaxed));
    }

    #[test]
    fn bus_mode_downmix_a_summe_nur_links() {
        // 0.5*(0.5+0.5) = 0.5 auf L, R bleibt still.
        check(mix_block(1.0, 0.0, 0.5, 0.5), (0.5, 0.0), "DownmixA");
        // Asymmetrisch: 0.5*(0.5+0.25) = 0.375, R = 0.
        check(
            mix_block(1.0, 0.0, 0.5, 0.25),
            (0.375, 0.0),
            "DownmixA asym",
        );
    }

    #[test]
    fn bus_mode_downmix_b_summe_nur_rechts() {
        check(mix_block(2.0, 0.0, 0.5, 0.5), (0.0, 0.5), "DownmixB");
        check(
            mix_block(2.0, 0.0, 0.5, 0.25),
            (0.0, 0.375),
            "DownmixB asym",
        );
    }

    #[test]
    fn bus_mode_stereo_repeat_summe_auf_beide() {
        check(mix_block(3.0, 0.0, 0.5, 0.5), (0.5, 0.5), "StereoRepeat");
        check(
            mix_block(3.0, 0.0, 0.5, 0.25),
            (0.375, 0.375),
            "StereoRepeat asym",
        );
    }

    #[test]
    fn bus_mode_composite_summe_beide_minus_3db() {
        let s = 0.5 * (0.5 + 0.5) * MINUS_3DB;
        check(mix_block(4.0, 0.0, 0.5, 0.5), (s, s), "Composite");
        // −3 dB sind rund 0.708, also deutlich leiser als Stereo Repeat mit 0.5.
        let (l, r) = mix_block(4.0, 0.0, 0.5, 0.5);
        assert!(
            l < 0.5 && (l - 0.353_972_9).abs() < 1e-6,
            "Composite L war {l}"
        );
        assert!((l - r).abs() < 1e-9, "Composite muss symmetrisch sein");
    }

    #[test]
    fn bus_mode_tv_mix_summe_links_minus_3db() {
        let s = 0.5 * (0.5 + 0.5) * MINUS_3DB;
        check(mix_block(5.0, 0.0, 0.5, 0.5), (s, 0.0), "TV-Mix");
        check(
            mix_block(5.0, 0.0, 0.5, 0.25),
            (0.375 * MINUS_3DB, 0.0),
            "TV-Mix asym",
        );
    }

    #[test]
    fn bus_mode_upmix21_mono_summe_auf_beide() {
        check(mix_block(6.0, 0.0, 0.5, 0.5), (0.5, 0.5), "Upmix21");
        check(
            mix_block(6.0, 0.0, 0.5, 0.25),
            (0.375, 0.375),
            "Upmix21 asym",
        );
    }

    #[test]
    fn bus_mode_upmix41_mono_summe_auf_beide() {
        check(mix_block(7.0, 0.0, 0.5, 0.5), (0.5, 0.5), "Upmix41");
        check(mix_block(7.0, 0.0, 0.1, 0.3), (0.2, 0.2), "Upmix41 asym");
    }

    #[test]
    fn bus_mode_upmix61_mono_summe_auf_beide() {
        // Zusatzkanäle (Center/LFE/Rear) gibt es im Stereo-Bus nicht.
        check(mix_block(8.0, 0.0, 0.5, 0.5), (0.5, 0.5), "Upmix61");
        check(mix_block(8.0, 0.0, 1.0, 0.0), (0.5, 0.5), "Upmix61 nur L");
    }

    #[test]
    fn bus_cross_tauscht_kanaele() {
        // Ohne Modus, nur Kreuzung: L und R tauschen.
        check(mix_block(0.0, 1.0, 0.5, 0.25), (0.25, 0.5), "cross");
        // Kreuzung nach Downmix A landet auf R.
        check(
            mix_block(1.0, 1.0, 0.5, 0.25),
            (0.0, 0.375),
            "cross+DownmixA",
        );
    }

    #[test]
    fn bus_mode_parameter_wird_geklemmt_und_gemeldet() {
        let p = params();
        assert!(p.set("bus", 0, "mode", 3.0));
        assert_eq!(p.buses[0].mode.load(Relaxed), BUS_MODE_STEREO_REPEAT);
        assert!(p.set("bus", 0, "mode", 99.0));
        assert_eq!(p.buses[0].mode.load(Relaxed), BUS_MODE_MAX);
        assert!(p.set("bus", 0, "mode", -5.0));
        assert_eq!(p.buses[0].mode.load(Relaxed), BUS_MODE_NORMAL);
        assert!(p.set("bus", 0, "cross", 1.0));
        assert!(p.buses[0].cross.load(Relaxed));
        // Unbekanntes Feld bleibt ein Fehler.
        assert!(!p.set("bus", 0, "modex", 1.0));
        let js = p.to_json().to_json();
        assert!(js.contains("\"mode\""), "to_json ohne mode: {js}");
        assert!(js.contains("\"cross\""), "to_json ohne cross: {js}");
    }

    #[test]
    fn anzeigename_wird_gesetzt_getrimmt_und_geleert() {
        let p = params();
        assert!(p.set_text("strip", 0, "label", "  Gesang  "));
        assert_eq!(p.label("strip", 0).as_deref(), Some("Gesang"), "getrimmt");
        assert!(p.set_text("bus", 7, "label", "Monitor"));
        assert_eq!(p.label("bus", 7).as_deref(), Some("Monitor"));

        // Unbekanntes Ziel, unbekannter Parameter, unbekannter Index.
        assert!(!p.set_text("strip", 99, "label", "x"));
        assert!(!p.set_text("strip", 0, "gain", "x"));
        assert!(!p.set_text("master", 0, "label", "x"));

        // Steuerzeichen und Ueberlaenge werden abgelehnt.
        assert!(!p.set_text("strip", 0, "label", "a\tb"));
        assert!(!p.set_text("strip", 0, "label", &"x".repeat(MAX_LABEL + 1)));

        // Der Name steht im Zustand (und damit im Preset).
        let js = p.to_json().to_json();
        assert!(js.contains("\"label\":\"Gesang\""), "{js}");
        assert!(js.contains("\"label\":\"Monitor\""), "{js}");

        // Leerer Text setzt wieder den Vorgabenamen.
        assert!(p.set_text("strip", 0, "label", ""));
        assert_eq!(p.label("strip", 0).as_deref(), Some(""));
    }

    // ---- Filter an den virtuellen Ausgängen -------------------------------

    #[test]
    fn hardware_busse_haben_keine_filter() {
        let p = params();
        for bi in 0..ERSTER_VIRTUELLER {
            assert!(p.buses[bi].filter.is_none());
            assert!(!p.set("bus", bi, "gate", 1.0), "A{} nahm gate an", bi + 1);
            assert!(!p.set("bus", bi, "aec", 1.0), "A{} nahm aec an", bi + 1);
        }
        for bi in ERSTER_VIRTUELLER..BUSES {
            for name in FILTER_PARAMS {
                assert!(p.set("bus", bi, name, 1.0), "{name} an B{}", bi - 4);
            }
        }
    }

    #[test]
    fn streifen_nehmen_die_kette_sind_aber_keine_alte_bearbeitung() {
        let p = params();
        // Die Kette des alten Kanalzugs (EQ, Lowcut, TK, Denoiser) gibt es nicht
        // mehr (ADR 0002, Punkt 5).
        for name in ["eq", "lowcut", "tk", "denoiser"] {
            assert!(!p.set("strip", 0, name, 1.0), "{name} noch am Streifen");
        }
        // `aec_ref` bleibt ein alter Name, der stillschweigend durchgeht
        // (alte Presets), aber nichts mehr bewirkt.
        assert!(p.set("strip", 0, "aec_ref", 4.0));
        assert_eq!(filter_get(&p.strips[0].filter, "aec_ref"), None);
        // Die Filterkette aus ADR 0006 hängt seit ADR 0011 auch am Kanalzug.
        for name in FILTER_PARAMS {
            assert!(p.set("strip", 0, name, 1.0), "{name} am Streifen");
        }
    }

    #[test]
    fn kette_am_kanal_wirkt_vor_dem_routing() {
        let p = params();
        let mut dsp = dsp_neu();
        // Gate an HW In 1, Schwelle über dem Signal: der Zug ist zu, und weil
        // die Kette vor dem Routing sitzt, kommt auch an A1 nichts an.
        assert!(p.set("strip", 0, "gate", 1.0));
        assert!(p.set("strip", 0, "gate_schwelle", -10.0));
        assert!(p.set("strip", 0, "gate_tiefe", 60.0));
        let mut buf = Buf::new(0.05, 64);
        for _ in 0..200 {
            let io = buf.io();
            dsp.process(&p, &io, 64);
        }
        assert!(buf.bus(0).abs() < 0.05 * 0.01, "A1 war {}", buf.bus(0));
        // Der Pegel vor der Kette zeigt das Rohsignal weiter an.
        let roh = fget(&p.strips[0].roh);
        assert!(roh > 0.03, "roh war {roh}");
        assert!(fget(&p.strips[0].level) < 0.01);
    }

    #[test]
    fn kette_am_kanal_laesst_ohne_schalter_alles_durch() {
        // Rohsignal ist die Vorgabe: ohne eingeschalteten Filter klingt der
        // Kanalzug wie vor ADR 0011.
        let p = params();
        let mut dsp = dsp_neu();
        let mut buf = Buf::new(0.2, 64);
        for _ in 0..20 {
            let io = buf.io();
            dsp.process(&p, &io, 64);
        }
        assert!((buf.bus(0) - 0.2).abs() < 1e-6, "A1 war {}", buf.bus(0));
    }

    #[test]
    fn filter_des_kanals_steht_im_preset_und_kommt_zurueck() {
        let p = params();
        assert!(p.set("strip", 2, "lc", 1.0));
        assert!(p.set("strip", 2, "lc_freq", 120.0));
        assert!(p.set("strip", 2, "comp", 1.0));
        assert!(p.set("strip", 2, "comp_ratio", 4.0));
        let bild = p.to_json();
        let q = std::sync::Arc::new(params());
        crate::http::apply_preset(&q, &bild);
        let f = &q.strips[2].filter;
        assert_eq!(filter_get(f, "lc"), Some(1.0));
        assert_eq!(filter_get(f, "lc_freq"), Some(120.0));
        assert_eq!(filter_get(f, "comp"), Some(1.0));
        assert_eq!(filter_get(f, "comp_ratio"), Some(4.0));
        // Ein Preset ohne Filterkette am Streifen schaltet sie nicht ein.
        let r = std::sync::Arc::new(params());
        crate::http::apply_preset(&r, &bild);
        let alt =
            crate::json::parse(r#"{"strips":[{"gain":-3,"eq":true,"comp":true}],"buses":[]}"#)
                .expect("JSON");
        crate::http::apply_preset(&r, &alt);
        assert_eq!(filter_get(&r.strips[0].filter, "comp"), Some(0.0));
        assert_eq!(fget(&r.strips[0].gain), -3.0);
    }

    #[test]
    fn alte_presets_mit_aec_ref_laden_weiter() {
        // `aec_ref` (welcher Hardware-Bus auf den Boxen liegt) gibt es nicht
        // mehr – die Referenz kommt vom Ausgabegerät. Ein altes Preset darf
        // trotzdem laden, der Wert wird stillschweigend übergangen.
        let p = params();
        assert!(p.set("bus", 5, "aec_ref", 4.0));
        let f = p.buses[5].filter.as_ref().unwrap();
        assert_eq!(filter_get(f, "aec_ref"), None);
        assert!(!p.set("bus", 5, "aec_refquatsch", 1.0));
    }

    #[test]
    fn rohsignal_auf_a1_gefiltert_auf_b1() {
        // Mikro auf A1 und B1; Gate an B1 mit hoher Schwelle macht B1 zu,
        // A1 bleibt das Rohsignal.
        let p = params();
        let mut dsp = dsp_neu();
        p.set("strip", 0, "send5", 1.0);
        p.set("bus", 5, "gate", 1.0);
        p.set("bus", 5, "gate_schwelle", -10.0);
        p.set("bus", 5, "gate_tiefe", 60.0);
        let mut buf = Buf::new(0.05, 64);
        for _ in 0..200 {
            let io = buf.io();
            dsp.process(&p, &io, 64);
        }
        assert!((buf.bus(0) - 0.05).abs() < 1e-6, "A1 war {}", buf.bus(0));
        assert!(buf.bus(5).abs() < 0.05 * 0.01, "B1 war {}", buf.bus(5));
    }

    #[test]
    fn gate_regelt_links_und_rechts_gemeinsam() {
        let p = params();
        let mut dsp = dsp_neu();
        p.set("strip", 0, "send5", 1.0);
        assert!(p.set("bus", 5, "gate", 1.0));
        assert!(p.set("bus", 5, "gate_schwelle", -45.0));
        // Links laut, rechts leise: beide Seiten bleiben offen.
        let mut buf = Buf::new(0.0, 64);
        buf.set_strip_ch(0, 0, 0.5);
        buf.set_strip_ch(0, 1, 0.001);
        for _ in 0..100 {
            let io = buf.io();
            dsp.process(&p, &io, 64);
        }
        assert!(
            (buf.out(5, 1) - 0.001).abs() < 1e-4,
            "R war {}",
            buf.out(5, 1)
        );
    }

    #[test]
    fn ausschlag_am_bus_zeigt_das_gefilterte_signal() {
        // Der Pegel eines Busses wird **nach** den Filtern geschrieben (siehe
        // `process`: `peak_b` nach AEC, Gate, Delay, Trim, Gain). Deshalb
        // fällt der Ausschlag in der Oberfläche, sobald ein Filter greift –
        // während der Streifen (Rohsignal) unverändert bleibt.
        let p = params();
        let mut dsp = dsp_neu();
        p.set("strip", 0, "send0", 0.0);
        p.set("strip", 0, "send5", 1.0);
        let mut buf = Buf::new(0.05, 64);
        for _ in 0..40 {
            let io = buf.io();
            dsp.process(&p, &io, 64);
        }
        let offen = fget(&p.buses[5].level);
        let streifen = fget(&p.strips[0].level);
        assert!(offen > 0.04, "B1 ohne Filter war {offen}");

        // Gate zu: der Ausschlag am Bus fällt, der am Streifen nicht.
        p.set("bus", 5, "gate", 1.0);
        p.set("bus", 5, "gate_schwelle", -10.0);
        p.set("bus", 5, "gate_tiefe", 60.0);
        for _ in 0..200 {
            let io = buf.io();
            dsp.process(&p, &io, 64);
        }
        let zu = fget(&p.buses[5].level);
        let streifen_danach = fget(&p.strips[0].level);
        assert!(zu < offen * 0.01, "B1 mit Gate war {zu}, offen {offen}");
        assert!(
            (streifen_danach - streifen).abs() < 1e-3,
            "Streifen 1 war {streifen}, danach {streifen_danach}"
        );
    }

    #[test]
    fn alter_restecho_schalter_schaltet_die_rauschunterdrueckung() {
        // `aec_ns` war bis ADR 0005 der Restecho-Schalter an ECHO. Er wird
        // weiter angenommen und meint jetzt NR – damit laden alte Presets, und
        // ein neuer Leser findet in `nr` denselben Schalter.
        let p = params();
        for bi in 0..ERSTER_VIRTUELLER {
            assert!(
                !p.set("bus", bi, "aec_ns", 1.0),
                "A{} nahm aec_ns an",
                bi + 1
            );
            assert!(!p.set("bus", bi, "nr", 1.0), "A{} nahm nr an", bi + 1);
        }
        assert!(p.set("bus", 5, "aec_ns", 1.0));
        let f = p.buses[5].filter.as_ref().unwrap();
        assert_eq!(filter_get(f, "aec_ns"), Some(1.0));
        assert_eq!(filter_get(f, "nr"), Some(1.0));
        assert!(f.nr.on.load(Relaxed));
        // Und umgekehrt: `nr` schreibt denselben Schalter.
        assert!(p.set("bus", 5, "nr", 0.0));
        assert_eq!(filter_get(f, "aec_ns"), Some(0.0));
        let text = p.to_json().to_json();
        assert!(text.contains("aec_ns"), "aec_ns fehlt im Zustand");
        assert!(text.contains("nr_stufe"), "nr_stufe fehlt im Zustand");
    }

    #[test]
    fn echo_der_boxen_wird_aus_b1_gerechnet() {
        // Streifen 1 = Mikro, das den Systemton gedämpft und verzögert
        // aufnimmt; das Mikro geht auf B1. Die Referenz kommt von den Ports
        // `echo_ref_L/R` – so, wie sie im Betrieb an den Monitoren der
        // Ausgabegeräte hängen.
        let p = params();
        let mut dsp = dsp_neu();
        p.set("strip", 0, "send0", 0.0);
        p.set("strip", 0, "send5", 1.0);
        p.set("bus", 5, "aec", 1.0);
        let block = 256;
        let mut buf = Buf::new(0.0, block);
        let mut zufall = 1u32;
        let mut verlauf = vec![0.0f32; 4096];
        let mut pos = 0usize;
        let (mut mic_p, mut aus_p) = (0.0f64, 0.0f64);
        let bloecke = 48_000 * 6 / block;
        let mut systemton = vec![0.0f32; block];
        for b in 0..bloecke {
            for (n, ton) in systemton.iter_mut().enumerate().take(block) {
                zufall ^= zufall << 13;
                zufall ^= zufall >> 17;
                zufall ^= zufall << 5;
                let sys = 0.3 * ((zufall as f32 / u32::MAX as f32) * 2.0 - 1.0);
                *ton = sys;
                // Der Raum verzögert den Systemton um 20 ms.
                verlauf[pos] = sys;
                let echo = 0.4 * verlauf[(pos + 4096 - 960) % 4096];
                pos = (pos + 1) % 4096;
                buf.ins[0][n] = echo;
                buf.ins[1][n] = echo;
            }
            buf.set_echo_ref(|n| systemton[n]);
            let io = buf.io();
            dsp.process(&p, &io, block);
            if b >= bloecke - 48_000 / block {
                for n in 0..block {
                    mic_p += (buf.ins[0][n] as f64).powi(2);
                    aus_p += (buf.outs[5 * 2][n] as f64).powi(2);
                }
            }
        }
        let erle = 10.0 * (mic_p / aus_p).log10();
        assert!(erle > 15.0, "Echo auf B1 nur um {erle:.1} dB gesenkt");
    }

    #[test]
    fn ohne_ausgabegeraet_bleibt_das_mikrofon_unveraendert() {
        // Keine Referenz gewählt: die Ports sind still, die Echounterdrückung
        // darf das Signal nicht verbiegen.
        let p = params();
        let mut dsp = dsp_neu();
        p.set("strip", 0, "send0", 0.0);
        p.set("strip", 0, "send5", 1.0);
        p.set("bus", 5, "aec", 1.0);
        // Ein Signal statt Gleichspannung: AEC3 hat einen Hochpass, der
        // Gleichanteile wegnimmt – das wäre hier nicht die Frage.
        let mut buf = Buf::new(0.0, 64);
        let mut zufall = 1u32;
        let mut verlauf: Vec<f32> = Vec::new();
        for n in 0..64 {
            zufall ^= zufall << 13;
            zufall ^= zufall >> 17;
            zufall ^= zufall << 5;
            let v = 0.25 * ((zufall as f32 / u32::MAX as f32) * 2.0 - 1.0);
            buf.ins[0][n] = v;
            buf.ins[1][n] = v;
            verlauf.push(v);
        }
        for _ in 0..60 {
            let io = buf.io();
            dsp.process(&p, &io, 64);
        }
        // Nach der Verzögerung steht der Pegel unverändert am Ausgang.
        let pegel =
            |v: &[f32]| -> f32 { (v.iter().map(|x| x * x).sum::<f32>() / v.len() as f32).sqrt() };
        // Das Testsignal wiederholt sich alle 64 Samples; damit ist der
        // Pegel eines Fensters unabhängig davon, um wie viele Samples die
        // Echounterdrückung verzögert (der verzögerte Block ist eine
        // zyklische Verschiebung derselben Periode).
        let ein = pegel(&verlauf);
        let aus = pegel(&buf.outs[5 * 2]);
        eprintln!("ein {ein:.4}, aus {aus:.4}");
        assert!((aus - ein).abs() < 0.1 * ein, "B1 war {aus}, Eingang {ein}");
    }

    // ---- Die Filterkette an B1..B3 (ADR 0006) ---------------------------

    /// Ein Sinus auf Streifen `strip` (beide Kanäle), Block für Block
    /// fortlaufend – ohne Phasensprung an der Blockgrenze.
    fn sinus(buf: &mut Buf, strip: usize, freq: f32, amp: f32, phase: &mut f32) {
        let frames = buf.ins[0].len();
        for n in 0..frames {
            let x =
                amp * (2.0 * std::f32::consts::PI * freq * (*phase + n as f32) / 48_000.0).sin();
            buf.ins[strip * 2][n] = x;
            buf.ins[strip * 2 + 1][n] = x;
        }
        *phase += frames as f32;
    }

    /// Spitze eines Buskanals im aktuellen Block.
    fn spitze(buf: &Buf, bus: usize, ch: usize) -> f32 {
        buf.outs[bus * 2 + ch]
            .iter()
            .fold(0.0f32, |a, x| a.max(x.abs()))
    }

    /// Streifen 1 zusätzlich auf B1 legen: A1 trägt weiter das Rohsignal,
    /// B1 bekommt die Filterkette.
    fn mikro_auf_b1(p: &Params) {
        assert!(p.set("strip", 0, "send5", 1.0));
    }

    #[test]
    fn lowcut_daempft_tiefen_an_b1_und_laesst_a1_unberuehrt() {
        let p = params();
        let mut dsp = dsp_neu();
        // Mikrofon auf A1 (Rohsignal) und B1 (gefiltert).
        assert!(p.set("strip", 0, "send5", 1.0));
        assert!(p.set("bus", 5, "lc", 1.0));
        assert!(p.set("bus", 5, "lc_freq", 80.0));
        for (freq, grenze) in [(30.0f32, 0.25f32), (1000.0, 1.0)] {
            let mut buf = Buf::new(0.0, 512);
            let mut phase = 0.0;
            for _ in 0..40 {
                sinus(&mut buf, 0, freq, 0.5, &mut phase);
                let io = buf.io();
                dsp.process(&p, &io, 512);
            }
            let b1 = spitze(&buf, 5, 0);
            let a1 = spitze(&buf, 0, 0);
            assert!(
                (a1 - 0.5).abs() < 1e-3,
                "{freq} Hz: A1 (Rohsignal) war {a1}"
            );
            if grenze < 1.0 {
                assert!(
                    b1 < 0.5 * grenze,
                    "{freq} Hz: B1 nur auf {b1:.3} gedämpft (soll < {:.3})",
                    0.5 * grenze
                );
            } else {
                assert!(
                    (b1 - 0.5).abs() < 0.02,
                    "{freq} Hz: B1 war {b1}, soll 0,5 sein"
                );
            }
        }
    }

    #[test]
    fn kompressor_reduziert_pegel_ueber_der_schwelle() {
        // 1 kHz mit −6 dBFS Spitze, Schwelle −20 dBFS, Ratio 4:1 → 14 dB über
        // der Schwelle werden um 10,5 dB abgesenkt: rund −16,5 dBFS.
        let p = params();
        let mut dsp = dsp_neu();
        mikro_auf_b1(&p);
        assert!(p.set("bus", 5, "comp", 1.0));
        assert!(p.set("bus", 5, "comp_schwelle", -20.0));
        assert!(p.set("bus", 5, "comp_ratio", 4.0));
        assert!(p.set("bus", 5, "comp_attack", 1.0));
        assert!(p.set("bus", 5, "comp_release", 100.0));
        let mut buf = Buf::new(0.0, 512);
        let mut phase = 0.0;
        for _ in 0..60 {
            sinus(&mut buf, 0, 1000.0, 0.5, &mut phase);
            let io = buf.io();
            dsp.process(&p, &io, 512);
        }
        let db = lin_to_db(spitze(&buf, 5, 0));
        assert!(
            (db + 16.5).abs() < 1.5,
            "B1 liegt bei {db:.1} dBFS, erwartet −16,5 dBFS"
        );
        // A1 bleibt das Rohsignal.
        assert!((spitze(&buf, 0, 0) - 0.5).abs() < 1e-3);
    }

    #[test]
    fn limiter_haelt_die_decke() {
        let p = params();
        let mut dsp = dsp_neu();
        mikro_auf_b1(&p);
        assert!(p.set("bus", 5, "limiter", 1.0));
        assert!(p.set("bus", 5, "limiter_decke", -1.0));
        let mut buf = Buf::new(0.0, 512);
        let mut phase = 0.0;
        for _ in 0..20 {
            // 1,5-fache Vollaussteuerung: ohne Limiter clippt das.
            sinus(&mut buf, 0, 200.0, 1.5, &mut phase);
            let io = buf.io();
            dsp.process(&p, &io, 512);
        }
        let decke = db_to_lin(-1.0);
        let b1 = spitze(&buf, 5, 0);
        assert!(b1 <= decke + 1e-4, "B1 über der Decke: {b1}");
        assert!(b1 > decke * 0.8, "Limiter hat zu stark zugepackt: {b1}");
        // A1 ist ungehindert laut.
        assert!((spitze(&buf, 0, 0) - 1.5).abs() < 1e-3);
    }

    #[test]
    fn agc_verzoegert_um_einen_rahmen_und_regelt() {
        // AGC2 rechnet in 10-ms-Rahmen: die ersten 480 Samples sind still.
        let p = params();
        let mut dsp = dsp_neu();
        mikro_auf_b1(&p);
        assert!(p.set("bus", 5, "agc", 1.0));
        let mut buf = Buf::new(0.0, 512);
        let mut phase = 0.0;
        sinus(&mut buf, 0, 220.0, 0.2, &mut phase);
        let io = buf.io();
        dsp.process(&p, &io, 512);
        // Die ersten 480 Samples (10 ms) sind still, danach kommt der Ton.
        assert!(
            buf.outs[5 * 2][..480].iter().all(|x| *x == 0.0),
            "AGC hat nicht einen Rahmen gewartet"
        );
        assert!(buf.outs[5 * 2][480..].iter().any(|x| *x != 0.0));
        // Zweiter Block: der Ton kommt (verzögert).
        let mut buf = Buf::new(0.0, 512);
        let mut phase = 0.0;
        sinus(&mut buf, 0, 220.0, 0.2, &mut phase);
        let io = buf.io();
        dsp.process(&p, &io, 512);
        assert!(spitze(&buf, 5, 0) > 0.01, "AGC gibt nichts aus");
    }

    #[test]
    fn ducking_senkt_den_zielbus_im_naechsten_block() {
        // B1 (Spielton) wird von B2 (Mikrofon) abgesenkt, sobald B2 über der
        // Schwelle liegt. Die Entscheidung fällt mit dem Pegel des vorigen
        // Blocks – im ersten Block ist B2 deshalb noch offen.
        let p = params();
        let mut dsp = dsp_neu();
        assert!(p.set("strip", 0, "send0", 0.0));
        assert!(p.set("strip", 0, "send6", 1.0)); // Streifen 1 → B2 (spricht)
        assert!(p.set("strip", 1, "send5", 1.0)); // Streifen 2 → A1 und B1 (Spielton)
                                                  // B2 spricht, B1 (Spielton) wird um 12 dB abgesenkt.
        assert!(p.set("bus", 6, "duck", 1.0));
        assert!(p.set("bus", 6, "duck_ziel", 5.0));
        assert!(p.set("bus", 6, "duck_tiefe", 12.0));
        assert!(p.set("bus", 6, "duck_schwelle", -30.0));
        assert!(p.set("bus", 6, "duck_attack", 5.0));
        assert!(p.set("bus", 6, "duck_release", 100.0));

        let mut buf = Buf::new(0.0, 512);
        let mut stimme = 0.0;
        let mut ton = 0.0;
        // Erster Block: B2 noch nicht abgesenkt (Pegel des vorigen Blocks = 0).
        sinus(&mut buf, 0, 300.0, 0.3, &mut stimme);
        sinus(&mut buf, 1, 1000.0, 0.5, &mut ton);
        let io = buf.io();
        dsp.process(&p, &io, 512);
        assert!(
            (spitze(&buf, 5, 0) - 0.5).abs() < 1e-3,
            "B1 schon im ersten Block abgesenkt: {}",
            spitze(&buf, 5, 0)
        );
        // Weiter, bis die Hüllkurve eingeschwungen ist.
        for _ in 0..40 {
            sinus(&mut buf, 0, 300.0, 0.3, &mut stimme);
            sinus(&mut buf, 1, 1000.0, 0.5, &mut ton);
            let io = buf.io();
            dsp.process(&p, &io, 512);
        }
        let b1 = lin_to_db(spitze(&buf, 5, 0));
        assert!(
            (b1 + 6.0 + 12.0).abs() < 1.5,
            "B1 liegt bei {b1:.1} dBFS, erwartet −18 dBFS (−6 dB minus 12 dB)"
        );
        // Der Quellbus selbst und der Hardware-Bus (Rohsignal) bleiben
        // unberührt – abgesenkt wird nur der Zielbus.
        assert!((spitze(&buf, 6, 0) - 0.3).abs() < 1e-3);
        assert!((spitze(&buf, 0, 0) - 0.5).abs() < 1e-3);
        // Sprechpause (der Sprecher schweigt): nach dem Release ist B1 wieder
        // offen.
        for _ in 0..80 {
            for ch in 0..2 {
                buf.ins[ch].fill(0.0);
            }
            sinus(&mut buf, 1, 1000.0, 0.5, &mut ton);
            let io = buf.io();
            dsp.process(&p, &io, 512);
        }
        assert!(
            spitze(&buf, 5, 0) > 0.45,
            "B1 nach der Pause nur {}",
            spitze(&buf, 5, 0)
        );
    }

    #[test]
    fn ptt_schaltet_den_bus_stumm() {
        let p = params();
        let mut dsp = dsp_neu();
        mikro_auf_b1(&p);
        assert!(p.set("bus", 5, "ptt", 1.0));
        let mut buf = Buf::new(0.05, 64);
        let io = buf.io();
        dsp.process(&p, &io, 64);
        assert_eq!(buf.out(5, 0), 0.0, "PTT ließ den Bus durch");
        assert!((buf.out(0, 0) - 0.05).abs() < 1e-6, "A1 muss bleiben");
        // Taste gedrückt: der Bus ist offen.
        assert!(p.set("bus", 5, "ptt_offen", 1.0));
        let io = buf.io();
        dsp.process(&p, &io, 64);
        assert!((buf.out(5, 0) - 0.05).abs() < 1e-6, "PTT blieb zu");
        // PTT aus: die Taste allein stummschaltet nicht.
        assert!(p.set("bus", 5, "ptt", 0.0));
        assert!(p.set("bus", 5, "ptt_offen", 0.0));
        let io = buf.io();
        dsp.process(&p, &io, 64);
        assert!((buf.out(5, 0) - 0.05).abs() < 1e-6, "ohne ptt stumm");
    }

    #[test]
    fn alle_filterparameter_lassen_sich_lesen_und_schreiben() {
        let p = params();
        let f = p.buses[5].filter.as_ref().unwrap();
        for name in FILTER_PARAMS {
            assert!(p.set("bus", 5, name, 1.0), "{name} nicht gesetzt");
            assert!(filter_get(f, name).is_some(), "{name} nicht lesbar");
        }
        // Die Vorgaben stehen so, wie die Oberfläche sie zeigt.
        let g = params();
        let f = g.buses[5].filter.as_ref().unwrap();
        assert_eq!(filter_get(f, "lc_freq"), Some(80.0));
        assert_eq!(filter_get(f, "limiter_decke"), Some(-1.0));
        assert_eq!(filter_get(f, "nr_stufe"), Some(1.0));
        assert_eq!(filter_get(f, "agc_ziel"), Some(-12.0));
        assert_eq!(filter_get(f, "duck_tiefe"), Some(12.0));
        assert_eq!(filter_get(f, "duck_ziel"), Some(0.0));
        // An A1..A5 gibt es keine Filter – jeder Name wird abgelehnt.
        for bi in 0..ERSTER_VIRTUELLER {
            for name in FILTER_PARAMS {
                assert!(!p.set("bus", bi, name, 1.0), "A{} nahm {name} an", bi + 1);
            }
        }
    }
}
