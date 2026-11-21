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
//! Busse B1..B3 (virtuell):  Summe → Mono/Modus → **Echounterdrückung** →
//!   **Noise Gate** → Delay → Trim → Gain/Mute.
//!   Die Echounterdrückung zieht ab, was die Boxen spielen. Die Referenz kommt
//!   von außen: an den Eingängen `echo_ref_L/R` hängen die Monitore der
//!   gewählten Ausgabegeräte (siehe [`crate::echo`]) – also genau der Ton, den
//!   die Boxen spielen, auch wenn ein Programm an Crosspoint vorbei auf das
//!   Gerät spielt (Chromium auf HDMI). Das Gate entscheidet danach am
//!   echofreien Signal über Pausen.
//!
//! Die Filter stehen im Modell als [`AusgangsFilter`] – nur die virtuellen
//! Busse haben einen; ein Hardware-Bus nimmt `gate`/`aec` nicht an.
//!
//! Es gibt **keinen** Mutex im Audio-Pfad: die Steuerung schreibt Atome, der
//! Callback liest sie mit `Relaxed`.

use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering, Ordering::Relaxed};
use std::sync::Mutex;

use crate::aec::Echounterdruecker;
use crate::dsp::*;
use crate::json::Value;
use crate::{bus_kind, BusKind, BUSES, BUS_NAMES, STRIPS, STRIP_NAMES};

#[inline]
fn fget(a: &AtomicU32) -> f32 {
    f32::from_bits(a.load(Relaxed))
}
#[inline]
fn fset(a: &AtomicU32, v: f32) {
    a.store(v.to_bits(), Relaxed)
}

/// Erster virtueller Bus (B1).
pub const ERSTER_VIRTUELLER: usize = 5;
/// Anzahl der virtuellen Busse (B1..B3).
pub const VIRTUELLE: usize = BUSES - ERSTER_VIRTUELLER;

pub struct GateParam {
    pub on: AtomicBool,
    /// Schwelle in dBFS (`gate_schwelle`).
    pub threshold: AtomicU32,
    pub attack: AtomicU32,
    pub hold: AtomicU32,
    pub release: AtomicU32,
    /// Tiefe in dB: wie weit das Gate zumacht (`gate_tiefe`).
    pub tiefe: AtomicU32,
}

impl GateParam {
    fn new() -> GateParam {
        GateParam {
            on: AtomicBool::new(false),
            threshold: AtomicU32::new((-45.0f32).to_bits()),
            attack: AtomicU32::new(5.0f32.to_bits()),
            hold: AtomicU32::new(80.0f32.to_bits()),
            release: AtomicU32::new(200.0f32.to_bits()),
            tiefe: AtomicU32::new(60.0f32.to_bits()),
        }
    }
}

/// Filter eines virtuellen Ausgangs: Echounterdrückung und Noise Gate.
pub struct AusgangsFilter {
    pub gate: GateParam,
    /// Echounterdrückung an (`aec`).
    pub aec: AtomicBool,
    /// Rauschunterdrückung hinter der Echounterdrückung an (`aec_ns`) – nimmt
    /// den Restecho mit, dreht aber am Mikrofonklang. Nur mit `aec` wirksam.
    pub aec_ns: AtomicBool,
}

impl AusgangsFilter {
    fn new() -> AusgangsFilter {
        AusgangsFilter {
            gate: GateParam::new(),
            aec: AtomicBool::new(false),
            aec_ns: AtomicBool::new(false),
        }
    }

    fn set(&self, param: &str, v: f32) -> bool {
        let g = &self.gate;
        match param {
            "gate" => g.on.store(v != 0.0, Relaxed),
            "gate_schwelle" => fset(&g.threshold, v.clamp(-80.0, -10.0)),
            "gate_tiefe" => fset(&g.tiefe, v.clamp(0.0, 80.0)),
            "gate_attack" => fset(&g.attack, v.clamp(0.1, 500.0)),
            "gate_hold" => fset(&g.hold, v.clamp(0.0, 2000.0)),
            "gate_release" => fset(&g.release, v.clamp(1.0, 5000.0)),
            "aec" => self.aec.store(v != 0.0, Relaxed),
            "aec_ns" => self.aec_ns.store(v != 0.0, Relaxed),
            // `aec_ref` (bis ADR 0002: welcher Hardware-Bus auf den Boxen
            // liegt) gibt es nicht mehr: die Referenz kommt jetzt vom
            // Ausgabegerät. Alte Presets laden weiter, der Wert wird
            // übergangen und nicht abgelehnt.
            "aec_ref" => {}
            _ => return false,
        }
        true
    }

    /// Alle Werte, wie sie im Zustand und im Preset stehen.
    pub fn to_json(&self) -> Value {
        let g = &self.gate;
        Value::obj(vec![
            ("gate", Value::Bool(g.on.load(Relaxed))),
            ("gate_schwelle", Value::num(fget(&g.threshold) as f64)),
            ("gate_tiefe", Value::num(fget(&g.tiefe) as f64)),
            ("gate_attack", Value::num(fget(&g.attack) as f64)),
            ("gate_hold", Value::num(fget(&g.hold) as f64)),
            ("gate_release", Value::num(fget(&g.release) as f64)),
            ("aec", Value::Bool(self.aec.load(Relaxed))),
            ("aec_ns", Value::Bool(self.aec_ns.load(Relaxed))),
        ])
    }
}

/// Wert eines Filterparameters (`None` = unbekannter Name), Gegenstück zu
/// [`AusgangsFilter::set`] für OSC und die Remote-API.
pub fn filter_get(f: &AusgangsFilter, param: &str) -> Option<f32> {
    let g = &f.gate;
    let schalter = |b: &AtomicBool| if b.load(Relaxed) { 1.0 } else { 0.0 };
    Some(match param {
        "gate" => schalter(&g.on),
        "gate_schwelle" => fget(&g.threshold),
        "gate_tiefe" => fget(&g.tiefe),
        "gate_attack" => fget(&g.attack),
        "gate_hold" => fget(&g.hold),
        "gate_release" => fget(&g.release),
        "aec" => schalter(&f.aec),
        "aec_ns" => schalter(&f.aec_ns),
        _ => return None,
    })
}

/// Namen der Filterparameter (für Presets, OSC und Oberfläche).
pub const FILTER_PARAMS: [&str; 8] = [
    "gate",
    "gate_schwelle",
    "gate_tiefe",
    "gate_attack",
    "gate_hold",
    "gate_release",
    "aec",
    "aec_ns",
];

pub struct StripParam {
    pub gain: AtomicU32,
    pub pan: AtomicU32,
    pub mute: AtomicBool,
    pub solo: AtomicBool,
    pub mono: AtomicBool,
    pub sends: [AtomicU32; BUSES],
    pub level: AtomicU32,
    /// Frei wählbarer Name (`/api/set` mit `param:label`); leer = Vorgabename.
    pub label: Mutex<String>,
}

impl StripParam {
    fn new() -> StripParam {
        let mut sends = std::array::from_fn(|_| AtomicU32::new(0f32.to_bits()));
        sends[0] = AtomicU32::new(1.0f32.to_bits()); // Vorgabe: Streifen -> A1
        StripParam {
            gain: AtomicU32::new(0f32.to_bits()),
            pan: AtomicU32::new(0f32.to_bits()),
            mute: AtomicBool::new(false),
            solo: AtomicBool::new(false),
            mono: AtomicBool::new(false),
            sends,
            level: AtomicU32::new(0f32.to_bits()),
            label: Mutex::new(String::new()),
        }
    }

    /// Anzeigename (leer = Vorgabename aus `STRIP_NAMES`).
    pub fn label(&self) -> String {
        match self.label.lock() {
            Ok(g) => g.clone(),
            Err(e) => e.into_inner().clone(),
        }
    }
}

/// Mischmodi eines Busses, Werte von `BusParam::mode` (`set_bus("mode", ...)`).
///
/// 0 Normal, 1 DownmixA, 2 DownmixB, 3 StereoRepeat, 4 Composite, 5 TV-Mix,
/// 6 Upmix 2.1, 7 Upmix 4.1, 8 Upmix 6.1.
pub const BUS_MODE_NORMAL: u32 = 0;
pub const BUS_MODE_DOWNMIX_A: u32 = 1;
pub const BUS_MODE_DOWNMIX_B: u32 = 2;
pub const BUS_MODE_STEREO_REPEAT: u32 = 3;
pub const BUS_MODE_COMPOSITE: u32 = 4;
pub const BUS_MODE_TV_MIX: u32 = 5;
pub const BUS_MODE_UPMIX_21: u32 = 6;
pub const BUS_MODE_UPMIX_41: u32 = 7;
pub const BUS_MODE_UPMIX_61: u32 = 8;
/// Höchster gültiger Modus (Upmix 6.1).
pub const BUS_MODE_MAX: u32 = BUS_MODE_UPMIX_61;

pub struct BusParam {
    pub gain: AtomicU32,
    pub mute: AtomicBool,
    pub mono: AtomicBool,
    /// Mischmodus, siehe `bus_mix` und `BUS_MODE_*`.
    pub mode: AtomicU32,
    /// Kreuzmischung: tauscht L und R am Bus-Eingang (`set_bus("cross", ...)`).
    pub cross: AtomicBool,
    pub trim: [AtomicU32; 2],
    pub delay_ms: [AtomicU32; 2],
    /// Filter – nur virtuelle Busse (B1..B3) haben welche.
    pub filter: Option<AusgangsFilter>,
    pub level: AtomicU32,
    /// Frei wählbarer Name (`/api/set` mit `param:label`); leer = Vorgabename.
    pub label: Mutex<String>,
}

impl BusParam {
    fn new(index: usize) -> BusParam {
        BusParam {
            gain: AtomicU32::new(0f32.to_bits()),
            mute: AtomicBool::new(false),
            mono: AtomicBool::new(false),
            mode: AtomicU32::new(BUS_MODE_NORMAL),
            cross: AtomicBool::new(false),
            trim: std::array::from_fn(|_| AtomicU32::new(0f32.to_bits())),
            delay_ms: std::array::from_fn(|_| AtomicU32::new(0f32.to_bits())),
            filter: (bus_kind(index) == BusKind::Virtual).then(AusgangsFilter::new),
            level: AtomicU32::new(0f32.to_bits()),
            label: Mutex::new(String::new()),
        }
    }

    /// Anzeigename (leer = Vorgabename aus `BUS_NAMES`).
    pub fn label(&self) -> String {
        match self.label.lock() {
            Ok(g) => g.clone(),
            Err(e) => e.into_inner().clone(),
        }
    }
}

/// Längste erlaubte Anzeigenamen für Streifen und Busse (Zeichen).
pub const MAX_LABEL: usize = 24;

pub struct Params {
    pub strips: [StripParam; STRIPS],
    pub buses: [BusParam; BUSES],
    pub sample_rate: AtomicU32,
    pub block: AtomicU32,
    /// Monoton steigender Zaehler der im Callback verarbeiteten Audio-Frames.
    ///
    /// Wird als Letztes je Block geschrieben (`Release`), nachdem die Pegel
    /// gespeichert sind. Der Pegel-Sender erkennt daran, ob es neue Daten
    /// gibt, statt blind zu schlafen; `Acquire` beim Lesen sorgt dafuer, dass
    /// er die zugehoerigen Pegel auch wirklich sieht.
    pub frames: AtomicU64,
}

impl Params {
    pub fn new(sample_rate: f32, block: u32) -> Params {
        Params {
            strips: std::array::from_fn(|_| StripParam::new()),
            buses: std::array::from_fn(BusParam::new),
            sample_rate: AtomicU32::new(sample_rate.to_bits()),
            block: AtomicU32::new(block),
            frames: AtomicU64::new(0),
        }
    }

    /// Stand des Frame-Zaehlers (siehe [`Params::frames`]).
    #[inline]
    pub fn frame_counter(&self) -> u64 {
        self.frames.load(Ordering::Acquire)
    }

    pub fn set(&self, target: &str, index: usize, param: &str, value: f32) -> bool {
        match target {
            "strip" => match self.strips.get(index) {
                Some(s) => set_strip(s, param, value),
                None => false,
            },
            "bus" => match self.buses.get(index) {
                Some(b) => set_bus(b, param, value),
                None => false,
            },
            _ => false,
        }
    }

    /// Textparameter setzen – derzeit nur `label` (Anzeigename) je Streifen
    /// und Bus. Der Name steht im Kanalzug anstelle von „HW In 1“/„A1“.
    ///
    /// Leere Eingabe löscht den Namen; Steuerzeichen und übermäßig lange
    /// Namen werden abgelehnt, damit die Kopfzeile nicht zerbricht.
    pub fn set_text(&self, target: &str, index: usize, param: &str, text: &str) -> bool {
        let ziel: &Mutex<String> = match (target, param) {
            ("strip", "label") => match self.strips.get(index) {
                Some(s) => &s.label,
                None => return false,
            },
            ("bus", "label") => match self.buses.get(index) {
                Some(b) => &b.label,
                None => return false,
            },
            _ => return false,
        };
        let sauber = text.trim();
        if sauber.chars().count() > MAX_LABEL || sauber.chars().any(|c| c.is_control()) {
            return false;
        }
        match ziel.lock() {
            Ok(mut g) => *g = sauber.to_string(),
            Err(e) => *e.into_inner() = sauber.to_string(),
        }
        true
    }

    /// Anzeigename eines Streifens oder Busses (leer = Vorgabename).
    pub fn label(&self, target: &str, index: usize) -> Option<String> {
        let ziel = match target {
            "strip" => &self.strips.get(index)?.label,
            "bus" => &self.buses.get(index)?.label,
            _ => return None,
        };
        Some(match ziel.lock() {
            Ok(g) => g.clone(),
            Err(e) => e.into_inner().clone(),
        })
    }

    pub fn to_json(&self) -> Value {
        let strips = self
            .strips
            .iter()
            .enumerate()
            .map(|(i, s)| {
                let sends: Vec<Value> =
                    s.sends.iter().map(|a| Value::num(fget(a) as f64)).collect();
                Value::obj(vec![
                    ("name", Value::str(STRIP_NAMES[i])),
                    ("label", Value::Str(s.label())),
                    ("gain", Value::num(fget(&s.gain) as f64)),
                    ("pan", Value::num(fget(&s.pan) as f64)),
                    ("mute", Value::Bool(s.mute.load(Relaxed))),
                    ("solo", Value::Bool(s.solo.load(Relaxed))),
                    ("mono", Value::Bool(s.mono.load(Relaxed))),
                    ("sends", Value::Arr(sends)),
                ])
            })
            .collect();
        let buses = self
            .buses
            .iter()
            .enumerate()
            .map(|(i, b)| {
                Value::obj(vec![
                    ("name", Value::str(BUS_NAMES[i])),
                    ("label", Value::Str(b.label())),
                    ("gain", Value::num(fget(&b.gain) as f64)),
                    ("mute", Value::Bool(b.mute.load(Relaxed))),
                    ("mono", Value::Bool(b.mono.load(Relaxed))),
                    ("mode", Value::num(b.mode.load(Relaxed) as f64)),
                    ("cross", Value::Bool(b.cross.load(Relaxed))),
                    ("delay_ms", Value::num(fget(&b.delay_ms[0]) as f64)),
                    ("trim", Value::num(fget(&b.trim[0]) as f64)),
                    (
                        "filter",
                        b.filter
                            .as_ref()
                            .map_or(Value::Null, AusgangsFilter::to_json),
                    ),
                ])
            })
            .collect();
        Value::obj(vec![
            ("strips", Value::Arr(strips)),
            ("buses", Value::Arr(buses)),
            ("sample_rate", Value::num(fget_sr(self) as f64)),
            ("block", Value::num(self.block.load(Relaxed) as f64)),
        ])
    }

    pub fn levels_json(&self) -> String {
        let mut s = String::with_capacity(256);
        self.levels_into(&mut s);
        s
    }

    /// Schreibt die Pegel in einen vorhandenen Text-Puffer (wird geleert).
    ///
    /// Kein Vec, keine Einzel-`format!`-Allokationen: der Sender baut damit
    /// einen einzigen Text je Takt und haengt ihn an alle Clients. `f` ist der
    /// Stand des Frame-Zaehlers – daran sieht der Empfaenger, ob der Frame neu
    /// ist, und Tests koennen die Frische belegen.
    pub fn levels_into(&self, out: &mut String) {
        use std::fmt::Write as _;
        out.clear();
        out.push_str("{\"strips\":[");
        for (i, x) in self.strips.iter().enumerate() {
            if i > 0 {
                out.push(',');
            }
            let _ = write!(out, "{:.1}", lin_to_db(fget(&x.level)));
        }
        out.push_str("],\"buses\":[");
        for (i, x) in self.buses.iter().enumerate() {
            if i > 0 {
                out.push(',');
            }
            let _ = write!(out, "{:.1}", lin_to_db(fget(&x.level)));
        }
        let _ = write!(out, "],\"f\":{}}}", self.frame_counter());
    }
}

fn fget_sr(p: &Params) -> u32 {
    fget(&p.sample_rate) as u32
}

fn set_strip(s: &StripParam, param: &str, v: f32) -> bool {
    match param {
        "gain" => fset(&s.gain, v.clamp(-60.0, 12.0)),
        "pan" => fset(&s.pan, v.clamp(-1.0, 1.0)),
        "mute" => s.mute.store(v != 0.0, Relaxed),
        "solo" => s.solo.store(v != 0.0, Relaxed),
        "mono" => s.mono.store(v != 0.0, Relaxed),
        _ => {
            return match param
                .strip_prefix("send")
                .and_then(|r| r.parse::<usize>().ok())
                .and_then(|b| s.sends.get(b))
            {
                Some(a) => {
                    fset(a, v.clamp(0.0, 2.0));
                    true
                }
                None => false,
            }
        }
    }
    true
}

fn set_bus(b: &BusParam, param: &str, v: f32) -> bool {
    match param {
        "gain" => fset(&b.gain, v.clamp(-60.0, 12.0)),
        "mute" => b.mute.store(v != 0.0, Relaxed),
        "mono" => b.mono.store(v != 0.0, Relaxed),
        // 0..8 wie auf `BusParam::mode` dokumentiert; alles andere wird geklemmt.
        "mode" => b
            .mode
            .store(v.clamp(0.0, BUS_MODE_MAX as f32) as u32, Relaxed),
        "cross" => b.cross.store(v != 0.0, Relaxed),
        "trim" => {
            fset(&b.trim[0], v);
            fset(&b.trim[1], v);
        }
        "delay" => {
            fset(&b.delay_ms[0], v.clamp(0.0, 2000.0));
            fset(&b.delay_ms[1], v.clamp(0.0, 2000.0));
        }
        // Filter gibt es nur an virtuellen Bussen.
        _ => return b.filter.as_ref().is_some_and(|f| f.set(param, v)),
    }
    true
}

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

/// Filterzustand eines virtuellen Busses (beide Kanäle).
struct FilterDsp {
    gate: [Gate; 2],
    aec: [Echounterdruecker; 2],
    /// War die Echounterdrückung im vorigen Block an? Beim Einschalten wird
    /// sie zurückgesetzt, damit kein alter Raum nachwirkt.
    aec_war_an: bool,
    /// War die Rauschunterdrückung im vorigen Block an (`aec_ns`)?
    ns_war_an: bool,
}

/// DSP-Zustand, ausschließlich vom Audio-Thread benutzt.
pub struct EngineDsp {
    bus_delay: [[DelayLine; 2]; BUSES],
    filter: Vec<FilterDsp>,
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
            // Die Echounterdrücker bringen ihre Puffer mit; sie entstehen hier,
            // außerhalb des Callbacks.
            filter: (0..VIRTUELLE)
                .map(|_| FilterDsp {
                    gate: [Gate::default(); 2],
                    aec: [
                        Echounterdruecker::neu(abtastrate),
                        Echounterdruecker::neu(abtastrate),
                    ],
                    aec_war_an: false,
                    ns_war_an: false,
                })
                .collect(),
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
            let g = &fp.gate;
            for gate in f.gate.iter_mut() {
                gate.enabled = g.on.load(Relaxed);
                gate.set_tiefe(fget(&g.tiefe));
                gate.set_times(fget(&g.attack), fget(&g.hold), fget(&g.release), sr);
            }
            let an = fp.aec.load(Relaxed);
            if an && !f.aec_war_an {
                f.aec.iter_mut().for_each(Echounterdruecker::reset);
            }
            f.aec_war_an = an;
            // Nur bei Änderung umschalten: `set_ns` baut den Rauschunterdrücker
            // einmal neu und allokiert dabei (siehe `Echounterdruecker::set_ns`).
            let ns = fp.aec_ns.load(Relaxed);
            if ns != f.ns_war_an {
                f.aec.iter_mut().for_each(|a| a.set_ns(ns));
                f.ns_war_an = ns;
            }
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

        let any_solo = p.strips.iter().any(|s| s.solo.load(Relaxed));
        let mut peak_s = [0f32; STRIPS];
        let mut peak_b = [0f32; BUSES];

        for n in 0..frames {
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

            // Alle Busse in einer Reihe; die Referenz der Echounterdrückung
            // kommt von den Ports `echo_ref_L/R` – dort hängen die Monitore der
            // gewählten Ausgabegeräte, unabhängig davon, welcher Bus sie spielt.
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
            for bi in 0..BUSES {
                let bp = &p.buses[bi];
                let mut v = acc[bi];
                if bp.mono.load(Relaxed) {
                    let m = 0.5 * (v[0] + v[1]);
                    v = [m, m];
                }
                v = bus_mix(v, bp.mode.load(Relaxed), bp.cross.load(Relaxed));
                if let Some(fp) = bp.filter.as_ref() {
                    let f = &mut self.filter[bi - ERSTER_VIRTUELLER];
                    if f.aec_war_an {
                        for ch in 0..2 {
                            v[ch] = f.aec[ch].process(v[ch], referenz);
                        }
                    }
                    // Gekoppelter Detektor: beide Kanäle öffnen gemeinsam.
                    let key = v[0].abs().max(v[1].abs());
                    let schwelle = fget(&fp.gate.threshold);
                    for ch in 0..2 {
                        v[ch] = f.gate[ch].process_key(v[ch], key, schwelle);
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
        }
        for bi in 0..BUSES {
            let old = fget(&p.buses[bi].level);
            fset(&p.buses[bi].level, peak_b[bi].max(old * 0.72));
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
    fn streifen_kennen_keine_bearbeitung_mehr() {
        let p = params();
        for name in ["eq", "comp", "gate", "nr", "lowcut", "tk", "denoiser"] {
            assert!(!p.set("strip", 0, name, 1.0), "{name} noch am Streifen");
        }
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
    fn restecho_schalter_haengt_an_den_virtuellen_bussen() {
        // `aec_ns` ist ein Zusatz zu `aec`: nur an B1–B3, nie an A1–A5, und es
        // steht im Zustand (also im Preset und in `/api/state`).
        let p = params();
        for bi in 0..ERSTER_VIRTUELLER {
            assert!(
                !p.set("bus", bi, "aec_ns", 1.0),
                "A{} nahm aec_ns an",
                bi + 1
            );
        }
        assert!(p.set("bus", 5, "aec_ns", 1.0));
        let f = p.buses[5].filter.as_ref().unwrap();
        assert_eq!(filter_get(f, "aec_ns"), Some(1.0));
        assert!(f.aec_ns.load(Relaxed));
        let json = p.to_json();
        let text = json.to_json();
        assert!(text.contains("aec_ns"), "aec_ns fehlt im Zustand");
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
}
