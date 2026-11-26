//! Das Parametermodell: alles, was die Oberfläche und jeder Kern gemeinsam
//! benutzen (ADR 0012).
//!
//! Diese Datei enthält **keine** Audioverarbeitung und keine Plattform: nur
//! Atome, die die Steuerung schreibt und der Audio-Thread liest, dazu Regeln,
//! was ein gültiger Wert ist (`set`), und die Form, in der das Ganze als Preset
//! steht (`to_json`).
//!
//! Was hier bewusst **nicht** steht: Bus-Mischung, Filter-DSP, Ports, Geräte.
//! Das gehört in den Kern (heute `crates/core`).

use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering, Ordering::Relaxed};
use std::sync::Mutex;

use crate::einheiten::lin_to_db;
use crate::json::Value;
use crate::{bus_kind, BusKind, BUSES, BUS_NAMES, STRIPS, STRIP_NAMES};

/// Erster virtueller Bus (B1). A1–A5 sind Hardware-Ausgänge.
pub const ERSTER_VIRTUELLER: usize = 5;
/// Anzahl der virtuellen Busse (B1..B3).
pub const VIRTUELLE: usize = BUSES - ERSTER_VIRTUELLER;

/// Ein f32 in einem Atom (die Steuerung schreibt, der Audio-Thread liest).
#[inline]
pub fn fget(a: &AtomicU32) -> f32 {
    f32::from_bits(a.load(Relaxed))
}

/// Schreibt einen f32 in ein Atom.
#[inline]
pub fn fset(a: &AtomicU32, v: f32) {
    a.store(v.to_bits(), Relaxed)
}

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

/// Low-Cut (Hochpass 2. Ordnung) vor der Echounterdrückung (`lc`, `lc_freq`).
///
/// Er steht **vor** AEC3: was der Filter wegnimmt (Rumpeln, Trittschall,
/// Lüfterbrummen unter 80 Hz), muss die Echounterdrückung nicht mehr
/// modellieren, und der Filter kann den Echo-Pfad nicht verstimmen.
pub struct LcParam {
    pub on: AtomicBool,
    /// Grenzfrequenz in Hz (`lc_freq`, 40..200, Vorgabe 80).
    pub freq: AtomicU32,
}

impl LcParam {
    fn new() -> LcParam {
        LcParam {
            on: AtomicBool::new(false),
            freq: AtomicU32::new(80.0f32.to_bits()),
        }
    }
}

/// Rauschunterdrückung (`nr`, `nr_stufe`): Lüfter, Tastatur, Maus.
///
/// Eigener Filter, unabhängig von `aec`. Die Stufe ist `sonora`s
/// `NoiseSuppressionLevel` (0 niedrig … 3 sehr hoch). Der alte Parameter
/// `aec_ns` schaltet dieselbe Rauschunterdrückung ein; er wird weiter
/// angenommen, damit alte Presets laden.
pub struct NrParam {
    pub on: AtomicBool,
    /// Stufe 0..3 (`nr_stufe`), Vorgabe 1 (mittel).
    pub stufe: AtomicU32,
}

impl NrParam {
    fn new() -> NrParam {
        NrParam {
            on: AtomicBool::new(false),
            stufe: AtomicU32::new(1),
        }
    }
}

/// Kompressor und Limiter (`comp*`, `limiter`, `limiter_decke`).
///
/// Der Kompressor sitzt hinter dem Gate, der Limiter ganz am Ende der
/// Filterkette – vor Delay/Trim/Gain, damit der Fader den Limiter nicht
/// aushebeln kann.
pub struct CompParam {
    pub on: AtomicBool,
    /// Schwelle in dBFS (`comp_schwelle`, −60..0, Vorgabe −18).
    pub schwelle: AtomicU32,
    /// Verhältnis (`comp_ratio`, 1..20, Vorgabe 3).
    pub ratio: AtomicU32,
    /// Attacke in ms (`comp_attack`, Vorgabe 10).
    pub attack: AtomicU32,
    /// Freigabe in ms (`comp_release`, Vorgabe 150).
    pub release: AtomicU32,
    /// Make-up in dB (`comp_makeup`, 0..24, Vorgabe 0).
    pub makeup: AtomicU32,
    /// Limiter an (`limiter`), getrennt vom Kompressor schaltbar.
    pub limiter: AtomicBool,
    /// Decke des Limiters in dBFS (`limiter_decke`, −12..0, Vorgabe −1).
    pub decke: AtomicU32,
}

impl CompParam {
    fn new() -> CompParam {
        CompParam {
            on: AtomicBool::new(false),
            schwelle: AtomicU32::new((-18.0f32).to_bits()),
            ratio: AtomicU32::new(3.0f32.to_bits()),
            attack: AtomicU32::new(10.0f32.to_bits()),
            release: AtomicU32::new(150.0f32.to_bits()),
            makeup: AtomicU32::new(0.0f32.to_bits()),
            limiter: AtomicBool::new(false),
            decke: AtomicU32::new((-1.0f32).to_bits()),
        }
    }
}

/// Automatische Pegelanpassung (`agc`, `agc_ziel`): AGC2 aus `sonora`.
pub struct AgcParam {
    pub on: AtomicBool,
    /// Zielpegel in dBFS (`agc_ziel`, −40..0, Vorgabe −12).
    pub ziel: AtomicU32,
}

impl AgcParam {
    fn new() -> AgcParam {
        AgcParam {
            on: AtomicBool::new(false),
            ziel: AtomicU32::new((-12.0f32).to_bits()),
        }
    }
}

/// Ducking (`duck*`): dieser Bus senkt einen anderen ab, solange er selbst
/// über der Schwelle liegt.
pub struct DuckParam {
    pub on: AtomicBool,
    /// Zielbus 0..7 (`duck_ziel`, Vorgabe 0 = A1).
    pub ziel: AtomicU32,
    /// Absenkung in dB (`duck_tiefe`, 0..48, Vorgabe 12).
    pub tiefe: AtomicU32,
    /// Schwelle in dBFS (`duck_schwelle`, −60..0, Vorgabe −30).
    pub schwelle: AtomicU32,
    pub attack: AtomicU32,
    pub release: AtomicU32,
}

impl DuckParam {
    fn new() -> DuckParam {
        DuckParam {
            on: AtomicBool::new(false),
            ziel: AtomicU32::new(0),
            tiefe: AtomicU32::new(12.0f32.to_bits()),
            schwelle: AtomicU32::new((-30.0f32).to_bits()),
            attack: AtomicU32::new(50.0f32.to_bits()),
            release: AtomicU32::new(400.0f32.to_bits()),
        }
    }
}

/// Push-to-Talk (`ptt`, `ptt_offen`): ist `ptt` an, ist der Bus stumm, solange
/// `ptt_offen` nicht gedrückt ist.
pub struct PttParam {
    pub on: AtomicBool,
    pub offen: AtomicBool,
}

impl PttParam {
    fn new() -> PttParam {
        PttParam {
            on: AtomicBool::new(false),
            offen: AtomicBool::new(false),
        }
    }
}

/// Filter eines virtuellen Ausgangs.
///
/// Reihenfolge im Callback (siehe `EngineDsp::process` und ADR 0006):
/// LC → ECHO → NR → GATE → COMP → AGC → Limiter → DUCK → PTT.
pub struct AusgangsFilter {
    pub gate: GateParam,
    /// Low-Cut (`lc`, `lc_freq`).
    pub lc: LcParam,
    /// Echounterdrückung an (`aec`).
    pub aec: AtomicBool,
    /// Rauschunterdrückung (`nr`, `nr_stufe`).
    pub nr: NrParam,
    /// Kompressor und Limiter (`comp*`, `limiter`, `limiter_decke`).
    pub comp: CompParam,
    /// Automatische Pegelanpassung (`agc`, `agc_ziel`).
    pub agc: AgcParam,
    /// Ducking (`duck*`).
    pub duck: DuckParam,
    /// Push-to-Talk (`ptt`, `ptt_offen`).
    pub ptt: PttParam,
}

impl AusgangsFilter {
    fn new() -> AusgangsFilter {
        AusgangsFilter {
            gate: GateParam::new(),
            lc: LcParam::new(),
            aec: AtomicBool::new(false),
            nr: NrParam::new(),
            comp: CompParam::new(),
            agc: AgcParam::new(),
            duck: DuckParam::new(),
            ptt: PttParam::new(),
        }
    }

    fn set(&self, param: &str, v: f32) -> bool {
        let g = &self.gate;
        let c = &self.comp;
        let d = &self.duck;
        match param {
            "gate" => g.on.store(v != 0.0, Relaxed),
            "gate_schwelle" => fset(&g.threshold, v.clamp(-80.0, -10.0)),
            "gate_tiefe" => fset(&g.tiefe, v.clamp(0.0, 80.0)),
            "gate_attack" => fset(&g.attack, v.clamp(0.1, 500.0)),
            "gate_hold" => fset(&g.hold, v.clamp(0.0, 2000.0)),
            "gate_release" => fset(&g.release, v.clamp(1.0, 5000.0)),
            "aec" => self.aec.store(v != 0.0, Relaxed),
            // `aec_ns` ist der alte Name der Rauschunterdrückung (sie hing bis
            // ADR 0005 an ECHO). Er schaltet jetzt NR.
            "aec_ns" | "nr" => self.nr.on.store(v != 0.0, Relaxed),
            "nr_stufe" => self.nr.stufe.store(v.clamp(0.0, 3.0) as u32, Relaxed),
            "lc" => self.lc.on.store(v != 0.0, Relaxed),
            "lc_freq" => fset(&self.lc.freq, v.clamp(40.0, 200.0)),
            "comp" => c.on.store(v != 0.0, Relaxed),
            "comp_schwelle" => fset(&c.schwelle, v.clamp(-60.0, 0.0)),
            "comp_ratio" => fset(&c.ratio, v.clamp(1.0, 20.0)),
            "comp_attack" => fset(&c.attack, v.clamp(0.1, 200.0)),
            "comp_release" => fset(&c.release, v.clamp(5.0, 4000.0)),
            "comp_makeup" => fset(&c.makeup, v.clamp(0.0, 24.0)),
            "limiter" => c.limiter.store(v != 0.0, Relaxed),
            "limiter_decke" => fset(&c.decke, v.clamp(-12.0, 0.0)),
            "agc" => self.agc.on.store(v != 0.0, Relaxed),
            "agc_ziel" => fset(&self.agc.ziel, v.clamp(-40.0, 0.0)),
            "duck" => d.on.store(v != 0.0, Relaxed),
            "duck_ziel" => d
                .ziel
                .store(v.clamp(0.0, (BUSES - 1) as f32).round() as u32, Relaxed),
            "duck_tiefe" => fset(&d.tiefe, v.clamp(0.0, 48.0)),
            "duck_schwelle" => fset(&d.schwelle, v.clamp(-60.0, 0.0)),
            "duck_attack" => fset(&d.attack, v.clamp(0.5, 2000.0)),
            "duck_release" => fset(&d.release, v.clamp(5.0, 8000.0)),
            "ptt" => self.ptt.on.store(v != 0.0, Relaxed),
            "ptt_offen" => self.ptt.offen.store(v != 0.0, Relaxed),
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
        let c = &self.comp;
        let d = &self.duck;
        let z = |b: &AtomicBool| Value::Bool(b.load(Relaxed));
        Value::obj(vec![
            ("gate", z(&g.on)),
            ("gate_schwelle", Value::num(fget(&g.threshold) as f64)),
            ("gate_tiefe", Value::num(fget(&g.tiefe) as f64)),
            ("gate_attack", Value::num(fget(&g.attack) as f64)),
            ("gate_hold", Value::num(fget(&g.hold) as f64)),
            ("gate_release", Value::num(fget(&g.release) as f64)),
            ("lc", z(&self.lc.on)),
            ("lc_freq", Value::num(fget(&self.lc.freq) as f64)),
            ("aec", z(&self.aec)),
            ("nr", z(&self.nr.on)),
            ("nr_stufe", Value::num(self.nr.stufe.load(Relaxed) as f64)),
            // Alter Name, gleicher Schalter: alte Leser finden ihn weiter.
            ("aec_ns", z(&self.nr.on)),
            ("comp", z(&c.on)),
            ("comp_schwelle", Value::num(fget(&c.schwelle) as f64)),
            ("comp_ratio", Value::num(fget(&c.ratio) as f64)),
            ("comp_attack", Value::num(fget(&c.attack) as f64)),
            ("comp_release", Value::num(fget(&c.release) as f64)),
            ("comp_makeup", Value::num(fget(&c.makeup) as f64)),
            ("limiter", z(&c.limiter)),
            ("limiter_decke", Value::num(fget(&c.decke) as f64)),
            ("agc", z(&self.agc.on)),
            ("agc_ziel", Value::num(fget(&self.agc.ziel) as f64)),
            ("duck", z(&d.on)),
            ("duck_ziel", Value::num(d.ziel.load(Relaxed) as f64)),
            ("duck_tiefe", Value::num(fget(&d.tiefe) as f64)),
            ("duck_schwelle", Value::num(fget(&d.schwelle) as f64)),
            ("duck_attack", Value::num(fget(&d.attack) as f64)),
            ("duck_release", Value::num(fget(&d.release) as f64)),
            ("ptt", z(&self.ptt.on)),
            ("ptt_offen", z(&self.ptt.offen)),
        ])
    }
}

/// Wert eines Filterparameters (`None` = unbekannter Name), Gegenstück zu
/// [`AusgangsFilter::set`] für OSC und die Remote-API.
pub fn filter_get(f: &AusgangsFilter, param: &str) -> Option<f32> {
    let g = &f.gate;
    let c = &f.comp;
    let d = &f.duck;
    let schalter = |b: &AtomicBool| if b.load(Relaxed) { 1.0 } else { 0.0 };
    Some(match param {
        "gate" => schalter(&g.on),
        "gate_schwelle" => fget(&g.threshold),
        "gate_tiefe" => fget(&g.tiefe),
        "gate_attack" => fget(&g.attack),
        "gate_hold" => fget(&g.hold),
        "gate_release" => fget(&g.release),
        "lc" => schalter(&f.lc.on),
        "lc_freq" => fget(&f.lc.freq),
        "aec" => schalter(&f.aec),
        "nr" | "aec_ns" => schalter(&f.nr.on),
        "nr_stufe" => f.nr.stufe.load(Relaxed) as f32,
        "comp" => schalter(&c.on),
        "comp_schwelle" => fget(&c.schwelle),
        "comp_ratio" => fget(&c.ratio),
        "comp_attack" => fget(&c.attack),
        "comp_release" => fget(&c.release),
        "comp_makeup" => fget(&c.makeup),
        "limiter" => schalter(&c.limiter),
        "limiter_decke" => fget(&c.decke),
        "agc" => schalter(&f.agc.on),
        "agc_ziel" => fget(&f.agc.ziel),
        "duck" => schalter(&d.on),
        "duck_ziel" => d.ziel.load(Relaxed) as f32,
        "duck_tiefe" => fget(&d.tiefe),
        "duck_schwelle" => fget(&d.schwelle),
        "duck_attack" => fget(&d.attack),
        "duck_release" => fget(&d.release),
        "ptt" => schalter(&f.ptt.on),
        "ptt_offen" => schalter(&f.ptt.offen),
        _ => return None,
    })
}

/// Namen der Filterparameter (für Presets, OSC und Oberfläche).
///
/// `aec_ns` steht weiter in der Liste: alte Presets schreiben ihn, er meint
/// heute die Rauschunterdrückung (`nr`).
pub const FILTER_PARAMS: [&str; 30] = [
    "gate",
    "gate_schwelle",
    "gate_tiefe",
    "gate_attack",
    "gate_hold",
    "gate_release",
    "lc",
    "lc_freq",
    "aec",
    "nr",
    "nr_stufe",
    "aec_ns",
    "comp",
    "comp_schwelle",
    "comp_ratio",
    "comp_attack",
    "comp_release",
    "comp_makeup",
    "limiter",
    "limiter_decke",
    "agc",
    "agc_ziel",
    "duck",
    "duck_ziel",
    "duck_tiefe",
    "duck_schwelle",
    "duck_attack",
    "duck_release",
    "ptt",
    "ptt_offen",
];

/// Stufen der Rauschunterdrückung, wie sie in der Oberfläche stehen.
pub const NR_STUFEN: [&str; 4] = ["niedrig", "mittel", "hoch", "sehr hoch"];

pub struct StripParam {
    pub gain: AtomicU32,
    pub pan: AtomicU32,
    pub mute: AtomicBool,
    pub solo: AtomicBool,
    pub mono: AtomicBool,
    pub sends: [AtomicU32; BUSES],
    /// Filter des Kanalzugs (ADR 0011): dieselbe Kette wie an B1–B3, aber
    /// zwischen Eingang und Fader – also **vor** dem Routing.
    pub filter: AusgangsFilter,
    /// Pegel nach Mono und **vor** der Filterkette („roh“ im FILTER-Modal).
    pub roh: AtomicU32,
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
            filter: AusgangsFilter::new(),
            roh: AtomicU32::new(0f32.to_bits()),
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
    /// Pegel der Bus-**Summe vor den Filtern** (für „roh“ im FILTER-Modal).
    pub roh: AtomicU32,
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
            roh: AtomicU32::new(0f32.to_bits()),
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
                    ("filter", s.filter.to_json()),
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
                // Kein `send`: die Filterkette des Kanalzugs (ADR 0011).
                None => s.filter.set(param, v),
            };
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
        // Filter gibt es an den virtuellen Bussen; A1–A5 nicht (ADR 0002).
        _ => return b.filter.as_ref().is_some_and(|f| f.set(param, v)),
    }
    true
}
