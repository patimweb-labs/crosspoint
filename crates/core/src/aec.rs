//! Echounterdrückung (AEC): rechnet den Systemton aus dem Mikrofon heraus,
//! der über die Boxen zurück ins Mikrofon kommt.
//!
//! # Verfahren: AEC3
//!
//! Gerechnet wird mit [`sonora`], der Übersetzung von WebRTCs
//! `AudioProcessing` nach Rust (BSD-3, reines Rust, keine C-Bibliothek) – also
//! dasselbe Verfahren, das auch PipeWires `module-echo-cancel` benutzt. Der
//! frühere eigene Filter (partitionierter adaptiver Filter im Frequenzbereich)
//! steht in der Historie; er hat im Raum 1,5 dB und auf einem Pfad mit
//! getrennten Uhren 2,4 dB geschafft, AEC3 auf demselben Pfad 21 dB. Die
//! Messung steht in `crates/core/examples/echo_verfahren.rs`, die Begründung in
//! `docs/adr/0005-echounterdrueckung-mit-aec3.md`.
//!
//! # Takt
//!
//! AEC3 rechnet in Rahmen von 10 ms (480 Samples bei 48 kHz). Der Audio-Callback
//! liefert beliebig große Blöcke, deshalb sammelt [`Echounterdruecker`] die
//! Samples ein und gibt sie einen Rahmen später aus: Mehrtakt von
//! [`VERZOEGERUNG`] Samples (10 ms), dazu die Verzögerung, die AEC3 selbst
//! einrechnet. Die Laufzeit Referenz → Mikro muss **nicht** eingestellt werden;
//! AEC3 schätzt sie selbst (gemessen: 0, 47 und 400 ms eingestellt ändern das
//! Ergebnis nicht).
//!
//! # Rauschunterdrückung (NR)
//!
//! Dieselbe `sonora`-Instanz kann auch die Rauschunterdrückung rechnen (Lüfter,
//! Tastatur, Maus). Sie ist ein **eigener Filter** (`nr`), unabhängig von der
//! Echounterdrückung: mit `nr` allein läuft nur die Rauschunterdrückung, ohne
//! Referenz und ohne AEC3. Die Stufe (`nr_stufe`, 0..3) entspricht WebRTCs
//! `NoiseSuppressionLevel` (6/12/18/21 dB). Der alte Parameter `aec_ns` wird
//! weiter angenommen und schaltet dieselbe Rauschunterdrückung ein.
//!
//! # Kein Allokieren, keine Sperre
//!
//! Alle Puffer liegen im [`Echounterdruecker`] und entstehen in
//! [`Echounterdruecker::neu`]; `process` baut nichts und sperrt nichts. Belegt
//! in `crates/core/tests/allokation.rs` (mit Echounterdrückung an allen
//! virtuellen Bussen) und – für `sonora` allein – in
//! `crates/core/examples/echo_verfahren.rs`.

use sonora::config::{EchoCanceller, NoiseSuppression, NoiseSuppressionLevel};
use sonora::{AudioProcessing, Config, StreamConfig};

/// Was `sonora` rechnen soll: Echounterdrückung (`echo`) und/oder
/// Rauschunterdrückung (`nr`, Stufe 0..3).
///
/// Eine Instanz trägt beides, weil `sonora` beides in einem Durchlauf rechnet
/// (AEC3 → NS). Ist nur NR gewählt, fehlt der Echounterdrücker ganz: dann
/// braucht es keine Referenz.
fn konfiguration(echo: bool, nr: bool, stufe: u8) -> Config {
    Config {
        echo_canceller: echo.then(EchoCanceller::default),
        noise_suppression: nr.then(|| NoiseSuppression {
            level: stufe_aus(stufe),
            ..Default::default()
        }),
        ..Default::default()
    }
}

/// Stufe der Rauschunterdrückung: 0 niedrig (~6 dB), 1 mittel (~12 dB,
/// Vorgabe), 2 hoch (~18 dB), 3 sehr hoch (~21 dB).
pub fn stufe_aus(stufe: u8) -> NoiseSuppressionLevel {
    match stufe {
        0 => NoiseSuppressionLevel::Low,
        2 => NoiseSuppressionLevel::High,
        3 => NoiseSuppressionLevel::VeryHigh,
        _ => NoiseSuppressionLevel::Moderate,
    }
}

/// Länge eines Rahmens, den AEC3 verarbeitet: 10 ms.
pub const RAHMEN_MS: u32 = 10;
/// Größter Rahmen (10 ms bei 96 kHz) – Puffergröße des Sammlers.
pub const RAHMEN_MAX: usize = 960;

/// Was AEC3 selbst an Verzögerung mitbringt: der Puffer, mit dem es Referenz
/// und Mikrofon zur Deckung bringt (bei 48 kHz gemessen 432 Samples = 9 ms,
/// unabhängig von Inhalt und Raum). Zusammen mit dem Sammler sind das 19 ms.
pub const AEC3_VERZOEGERUNG: usize = 432;

/// Unter diesem Pegel (Spitze im Rahmen, −80 dBFS) gilt die Referenz als
/// still: es spielt nichts, es gibt nichts auszurechnen.
const STILL_SCHWELLE: f32 = 1.0e-4;

/// So viele stille Rahmen (10 ms) am Stück, bis durchgeschaltet wird. Kürzer
/// wäre falsch: der Nachhall eines Tons läuft nach, wenn die Referenz schon
/// still ist.
const STILL_RAHMEN: u32 = 20;

/// Länge der Brücke (Mikrofon unverändert, nur verzögert): die Ausgabe des
/// Echounterdrückers kommt um so viele Samples später.
const BRUECKE_MAX: usize = RAHMEN_MAX + AEC3_VERZOEGERUNG;

/// Echounterdrücker für einen Kanal.
pub struct Echounterdruecker {
    /// Der Rechner von `sonora`; entsteht in [`Self::neu`].
    apm: AudioProcessing,
    /// Samples je Rahmen (10 ms bei der Abtastrate der Engine).
    rahmen: usize,
    /// Sammler für das, was die Boxen spielen (Referenz).
    referenz: [f32; RAHMEN_MAX],
    referenz_pos: usize,
    /// Sammler für das Mikrofon.
    mikro: [f32; RAHMEN_MAX],
    mikro_pos: usize,
    /// Ergebnis des letzten Rahmens; wird Sample für Sample ausgegeben.
    aus: [f32; RAHMEN_MAX],
    aus_pos: usize,
    /// Arbeitspuffer für sonora (die Render-Ausgabe wird nicht gebraucht).
    scratch: [f32; RAHMEN_MAX],
    /// Brücke: das Mikrofon unverändert, nur verzögert. Sie ist die Ausgabe,
    /// solange nichts spielt (siehe [`STILL_SCHWELLE`]).
    bruecke: [f32; BRUECKE_MAX],
    bruecke_pos: usize,
    bruecke_laenge: usize,
    /// So viele Rahmen war die Referenz am Stück still.
    still_seit: u32,
    /// Läuft die Echounterdrückung (`echo`)?
    echo: bool,
    /// Läuft die Rauschunterdrückung (`nr`)?
    nr: bool,
    /// Stufe der Rauschunterdrückung (0..3).
    stufe: u8,
}

impl Echounterdruecker {
    /// Legt den Rechner an. **Außerhalb des Audio-Threads aufrufen** – hier
    /// wird allokiert.
    ///
    /// `abtastrate` bestimmt die Rahmengröße (10 ms). AEC3 selbst rechnet
    /// intern mit 16 kHz und passt sich an.
    pub fn neu(abtastrate: u32) -> Echounterdruecker {
        let rahmen = (abtastrate as usize / 1000) * RAHMEN_MS as usize;
        let rahmen = rahmen.clamp(80, RAHMEN_MAX);
        // Beide Filter sind zunächst aus: ECHO soll den Systemton
        // herausrechnen, NR soll aufräumen – beides nur auf Wunsch. Sie lassen
        // sich mit [`Echounterdruecker::set_filter`] zuschalten.
        let mut apm = AudioProcessing::builder()
            .config(konfiguration(false, false, 1))
            .capture_config(StreamConfig::new(abtastrate, 1))
            .render_config(StreamConfig::new(abtastrate, 1))
            .build();
        // Die Laufzeit schätzt AEC3 selbst; ein grober Wert genügt als Start.
        let _ = apm.set_stream_delay_ms(0);
        let mut e = Echounterdruecker {
            apm,
            rahmen,
            referenz: [0.0; RAHMEN_MAX],
            referenz_pos: 0,
            mikro: [0.0; RAHMEN_MAX],
            mikro_pos: 0,
            aus: [0.0; RAHMEN_MAX],
            // Kein Rahmen fertig: die ersten Samples sind Stille, damit die
            // Ausgabe um genau einen Rahmen verzögert ist.
            aus_pos: RAHMEN_MAX,
            scratch: [0.0; RAHMEN_MAX],
            bruecke: [0.0; BRUECKE_MAX],
            bruecke_pos: 0,
            bruecke_laenge: 0,
            // Am Anfang läuft nichts: erst wenn die Referenz laut wird,
            // rechnet AEC3 mit.
            still_seit: STILL_RAHMEN,
            echo: false,
            nr: false,
            stufe: 1,
        };
        e.bruecke_laenge = e.rahmen + AEC3_VERZOEGERUNG;
        e
    }

    /// Samples, um die die Ausgabe später kommt: der Sammler (ein Rahmen =
    /// 10 ms) und die Verzögerung von AEC3 selbst (gemessen 19 ms zusammen).
    pub fn verzoegerung(&self) -> usize {
        self.rahmen + AEC3_VERZOEGERUNG
    }

    /// Echounterdrückung und Rauschunterdrückung schalten (`aec`, `nr`).
    ///
    /// **Keine Echtzeit-Operation:** beim Umschalten baut `sonora` die
    /// betroffenen Teile einmal neu – das allokiert. Danach ist `process`
    /// wieder frei von Allokationen (`tests/allokation.rs`). Aufgerufen wird
    /// das aus [`crate::engine::EngineDsp::reconfigure`], und nur wenn sich
    /// Schalter oder Stufe gegenüber dem letzten Block geändert haben.
    pub fn set_filter(&mut self, echo: bool, nr: bool, stufe: u8) {
        let stufe = stufe.min(3);
        if echo == self.echo && nr == self.nr && (!nr || stufe == self.stufe) {
            return;
        }
        self.apm.apply_config(konfiguration(echo, nr, stufe));
        self.echo = echo;
        self.nr = nr;
        self.stufe = stufe;
    }

    /// Nur die Rauschunterdrückung schalten (alter Einstieg für Messungen).
    pub fn set_ns(&mut self, an: bool) {
        self.set_filter(self.echo, an, self.stufe);
    }

    /// Wird für dieses Sample überhaupt gerechnet? Ohne NR nur, wenn die
    /// Referenz etwas spielt (sonst ist das Mikrofon besser unverändert, siehe
    /// [`STILL_SCHWELLE`]).
    #[inline]
    fn verarbeiten(&self) -> bool {
        self.nr || (self.echo && self.still_seit < STILL_RAHMEN)
    }

    /// Länge der Brücke in Samples (= [`Echounterdruecker::verzoegerung`]).
    fn bruecke_laenge(&self) -> usize {
        self.bruecke_laenge
    }

    /// Puffer leeren. Der gelernte Filter in AEC3 bleibt stehen und passt sich
    /// dem neuen Raum innerhalb weniger hundert Millisekunden an; ihn zu
    /// verwerfen hieße, `sonora` neu zu bauen – und das allokiert. Allokiert
    /// hier nichts.
    pub fn reset(&mut self) {
        self.referenz.fill(0.0);
        self.referenz_pos = 0;
        self.mikro.fill(0.0);
        self.mikro_pos = 0;
        self.aus.fill(0.0);
        self.aus_pos = RAHMEN_MAX;
        self.bruecke.fill(0.0);
        self.bruecke_pos = 0;
        self.still_seit = STILL_RAHMEN;
    }

    /// Ein Sample: `mic` mit Echo hinein, Referenz `referenz` (was die Boxen
    /// spielen) dazu; heraus kommt das bereinigte Sample, einen Rahmen später.
    ///
    /// Spielt nichts (Referenz still), kommt das Mikrofon unverändert durch –
    /// nur um dieselbe Zeit verzögert. Ohne diesen Weg würde AEC3 auch ein
    /// Mikrofon ohne Echo verbiegen: gemessen bleibt der Pegel gleich, die
    /// Wellenform aber nicht.
    #[inline]
    pub fn process(&mut self, mic: f32, referenz: f32) -> f32 {
        // (0) Brücke: das trockene Mikrofon, um die Ausgabeverzögerung später.
        let laenge = self.bruecke_laenge();
        let trocken = self.bruecke[self.bruecke_pos];
        self.bruecke[self.bruecke_pos] = mic;
        self.bruecke_pos += 1;
        if self.bruecke_pos >= laenge {
            self.bruecke_pos = 0;
        }

        // (1) Referenz einsammeln; ist ein Rahmen voll, lernt AEC3 daran.
        self.referenz[self.referenz_pos] = referenz;
        self.referenz_pos += 1;
        if self.referenz_pos == self.rahmen {
            self.referenz_pos = 0;
            let spitze = self.referenz[..self.rahmen]
                .iter()
                .fold(0.0f32, |a, v| a.max(v.abs()));
            self.still_seit = if spitze < STILL_SCHWELLE {
                (self.still_seit + 1).min(STILL_RAHMEN)
            } else {
                0
            };
            if self.echo && self.still_seit < STILL_RAHMEN {
                let _ = self.apm.process_render_f32(
                    &[&self.referenz[..self.rahmen]],
                    &mut [&mut self.scratch[..self.rahmen]],
                );
            }
        }

        // (2) Mikrofon einsammeln; ist ein Rahmen voll, wird er bereinigt und
        // als nächste Ausgabe abgelegt.
        self.mikro[self.mikro_pos] = mic;
        self.mikro_pos += 1;
        if self.mikro_pos == self.rahmen {
            self.mikro_pos = 0;
            if self.verarbeiten() {
                let _ = self.apm.process_capture_f32(
                    &[&self.mikro[..self.rahmen]],
                    &mut [&mut self.scratch[..self.rahmen]],
                );
                self.aus[..self.rahmen].copy_from_slice(&self.scratch[..self.rahmen]);
                self.aus_pos = 0;
            } else {
                // Nichts zu rechnen: die Brücke ist die Ausgabe.
                self.aus_pos = RAHMEN_MAX;
            }
        }

        // (3) Ausgeben, was vom letzten fertigen Rahmen übrig ist.
        if self.verarbeiten() && self.aus_pos < self.rahmen {
            let y = self.aus[self.aus_pos];
            self.aus_pos += 1;
            y
        } else {
            trocken
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

    /// Ein Echounterdrücker mit **eingeschalteter** Echounterdrückung, wie ihn
    /// der Kern nach dem ersten Block hat.
    fn neu() -> Box<Echounterdruecker> {
        // Der Zustand von AEC3 ist groß: auf dem Heap bauen.
        std::thread::Builder::new()
            .stack_size(16 * 1024 * 1024)
            .spawn(|| {
                let mut d = Box::new(Echounterdruecker::neu(48_000));
                d.set_filter(true, false, 1);
                d
            })
            .unwrap()
            .join()
            .unwrap()
    }

    #[test]
    fn rauschunterdrueckung_ohne_echounterdrueckung() {
        // NR ist ein eigener Filter: ohne `echo` läuft nur `sonora`s
        // Rauschunterdrückung, ohne Referenz. Zum Vergleich dieselbe Strecke
        // mit ausgeschaltetem NR (dann geht das Mikrofon unverändert durch).
        let sr = 48_000usize;
        let mut r = Rauschen(7);
        let eingang: Vec<f32> = (0..sr * 3).map(|_| 0.03 * r.next()).collect();
        let messen = |nr: bool| {
            let mut d = neu();
            d.set_filter(false, nr, 2);
            let mut summe = 0.0f64;
            let mut n = 0usize;
            for (i, x) in eingang.iter().enumerate() {
                let y = d.process(*x, 0.0);
                if i >= sr * 2 {
                    summe += (y * y) as f64;
                    n += 1;
                }
            }
            (summe / n as f64).sqrt() as f32
        };
        let ohne = messen(false);
        let mit = messen(true);
        assert!(ohne > 0.01, "trockener Weg war still: {ohne}");
        assert!(
            mit < ohne * 0.5,
            "NR hat nichts weggenommen: {ohne:.4} → {mit:.4}"
        );
    }

    #[test]
    fn reines_echo_wird_stark_gesenkt() {
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
        let erle = 10.0 * (leistung(&mic_letzte) / leistung(&aus_letzte).max(1e-20)).log10();
        eprintln!("ERLE {erle:.1} dB");
        assert!(erle > 20.0, "ERLE nur {erle:.1} dB");
    }

    #[test]
    fn stille_referenz_laesst_das_mikrofon_durch() {
        let mut aec = neu();
        let mut r = Rauschen(7);
        let verzoegerung = aec.verzoegerung();
        let eingang: Vec<f32> = (0..24_000).map(|_| 0.2 * r.next()).collect();
        let aus: Vec<f32> = eingang.iter().map(|&x| aec.process(x, 0.0)).collect();
        // Nur verzögert, sonst unverändert: die Rauschunterdrückung ist aus,
        // der Hochpass von AEC3 lässt nur die tiefsten Frequenzen weg.
        let fehler: f32 = (verzoegerung..eingang.len())
            .map(|n| (aus[n] - eingang[n - verzoegerung]).abs())
            .fold(0.0, f32::max);
        let pegel = eingang.iter().fold(0.0f32, |a, v| a.max(v.abs()));
        eprintln!("größte Abweichung {fehler:.4} bei Pegel {pegel:.2}");
        assert!(fehler < 0.05 * pegel, "Mikrofon wird verändert");
    }

    #[test]
    fn stimme_bleibt_beim_doppelsprechen_erhalten() {
        let mut aec = neu();
        let mut raum = Raum::neu();
        let mut r = Rauschen(3);
        let sr = 48_000;
        // Erst lernen (nur Systemton) …
        let (mut echo_nur, mut aus_nur) = (Vec::new(), Vec::new());
        for i in 0..sr * 4 {
            let boxen = 0.3 * r.next();
            let m = raum.process(boxen);
            let a = aec.process(m, boxen);
            if i >= sr * 3 {
                echo_nur.push(m);
                aus_nur.push(a);
            }
        }
        // … dann spricht jemand dazu: die Stimme muss durchkommen, das Echo
        // nicht. Die Stimme ist ein Ton mit Vibrato (kein Rauschen), sonst
        // wäre sie vom Rauschen des Systemtons nicht zu unterscheiden.
        let mut stimme = Vec::new();
        let mut aus = Vec::new();
        for i in 0..sr {
            let boxen = 0.3 * r.next();
            let t = i as f32 / sr as f32;
            let v = 0.15
                * (2.0 * std::f32::consts::PI * (220.0 + 4.0 * (t * 60.0).sin()) * i as f32
                    / sr as f32)
                    .sin();
            stimme.push(v);
            aus.push(aec.process(raum.process(boxen) + v, boxen));
        }
        // Ausgabe ist verzögert; verglichen wird der Pegel, nicht das Sample –
        // die Verzögerung von AEC3 ist nicht auf das Sample genau festgelegt.
        let stimme_db = 10.0 * leistung(&stimme).log10();
        let doppel_db = 10.0 * leistung(&aus[aus.len() / 2..]).log10();
        let echo_db = 10.0 * leistung(&aus_nur[aus_nur.len() / 2..]).log10();
        eprintln!("Stimme {stimme_db:.1} dB, Ausgang beim Doppelsprechen {doppel_db:.1} dB (vorher {echo_db:.1} dB)");
        assert!(
            (doppel_db - stimme_db).abs() < 5.0,
            "Stimme kommt nicht durch: {doppel_db:.1} dB statt {stimme_db:.1} dB"
        );
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
        for i in 0..sr * 5 {
            let b = 0.3 * r.next();
            let m = raum.process(b);
            let a = aec.process(m, b);
            if i >= sr * 4 {
                mic.push(m);
                aus.push(a);
            }
        }
        let erle = 10.0 * (leistung(&mic) / leistung(&aus).max(1e-20)).log10();
        eprintln!("ERLE nach Wechsel {erle:.1} dB");
        assert!(erle > 15.0, "ERLE nach Wechsel nur {erle:.1} dB");
    }

    #[test]
    fn reset_laesst_nichts_alten_durch() {
        let mut aec = neu();
        let mut r = Rauschen(5);
        for _ in 0..48_000 {
            let b = 0.3 * r.next();
            aec.process(0.5 * b, b);
        }
        aec.reset();
        assert!(aec.referenz_pos == 0 && aec.mikro_pos == 0);
        // Nach dem Leeren kommt erst einmal Stille (kein Rahmen fertig).
        assert_eq!(aec.process(0.5, 0.5), 0.0);
    }
}
