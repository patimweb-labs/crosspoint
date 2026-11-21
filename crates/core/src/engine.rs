//! Echtzeit-Kern: Parameter als Atome, DSP-Zustand ausschließlich im Audio-Thread.
//!
//! Signalkette je Streifen:
//!   Eingang -> Mono -> LOWCUT -> TK -> GATE -> NR (spektral) -> EQ -> Compressor
//!   -> Denoiser ("einfach") -> Gain -> Pan -> Routing-Matrix (8 Busse)
//!
//! Die Reihenfolge folgt der Sprache, nicht dem Original: erst Rumpeln weg
//! (Lowcut), dann der Transienten-Unterdruecker (Klackser/Poltern, bevor das
//! Gate ueber Pausen entscheidet), dann entscheidet das Gate **am
//! unveraenderten Pegel**, ob eine Pause vorliegt, dann senkt die NR das
//! Rauschen unter dem Signal, und erst danach formen EQ und Compressor den
//! Klang. Stuende der Compressor vor dem Gate, haenge die Gate-Schwelle von
//! seiner Kennlinie und seinem Makeup ab; hinter dem Gate arbeitet er auf dem
//! schon bereinigten Signal.
//! Je Bus: Summe -> Mischmodus (mode/cross) -> Limiter -> Delay pro Kanal
//!   -> Trim -> Gain/Mute -> Ausgang.
//!
//! Es gibt **keinen** Mutex im Audio-Pfad: die Steuerung schreibt Atome, der
//! Callback liest sie mit `Relaxed`.

use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering, Ordering::Relaxed};
use std::sync::Mutex;

use crate::dsp::*;
use crate::effekte::{self, Echo, Hall, Karaoke, Klangfarbe, FX_WEGE};
use crate::json::Value;
use crate::{BUSES, BUS_NAMES, EQ_CELLS, STRIPS, STRIP_NAMES};

#[inline]
fn fget(a: &AtomicU32) -> f32 {
    f32::from_bits(a.load(Relaxed))
}
#[inline]
fn fset(a: &AtomicU32, v: f32) {
    a.store(v.to_bits(), Relaxed)
}

pub struct EqParam {
    pub on: AtomicBool,
    pub kind: AtomicU32,
    pub freq: AtomicU32,
    pub q: AtomicU32,
    pub gain: AtomicU32,
}

impl EqParam {
    fn new() -> EqParam {
        EqParam {
            on: AtomicBool::new(false),
            kind: AtomicU32::new(0),
            freq: AtomicU32::new(1000.0f32.to_bits()),
            q: AtomicU32::new(0.7f32.to_bits()),
            gain: AtomicU32::new(0.0f32.to_bits()),
        }
    }
}

pub struct CompParam {
    pub on: AtomicBool,
    pub threshold: AtomicU32,
    pub ratio: AtomicU32,
    pub attack: AtomicU32,
    pub release: AtomicU32,
    pub knee: AtomicU32,
    pub gain_in: AtomicU32,
    pub gain_out: AtomicU32,
}

impl CompParam {
    fn new() -> CompParam {
        CompParam {
            on: AtomicBool::new(false),
            threshold: AtomicU32::new((-18.0f32).to_bits()),
            ratio: AtomicU32::new(1.0f32.to_bits()),
            attack: AtomicU32::new(20.0f32.to_bits()),
            release: AtomicU32::new(200.0f32.to_bits()),
            knee: AtomicU32::new(0.0f32.to_bits()),
            gain_in: AtomicU32::new(0.0f32.to_bits()),
            gain_out: AtomicU32::new(0.0f32.to_bits()),
        }
    }
}

pub struct GateParam {
    pub on: AtomicBool,
    /// Schwelle in dBFS (`gate_schwelle`, auch `gate_threshold`).
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

pub struct StripParam {
    pub gain: AtomicU32,
    pub limit: AtomicU32,
    pub pan: AtomicU32,
    pub mute: AtomicBool,
    pub solo: AtomicBool,
    pub mono: AtomicBool,
    pub sends: [AtomicU32; BUSES],
    pub eq_on: AtomicBool,
    pub eq: [[EqParam; EQ_CELLS]; 2],
    pub comp: CompParam,
    pub gate: GateParam,
    pub dn_on: AtomicBool,
    pub dn_floor: AtomicU32,
    pub dn_threshold: AtomicU32,
    /// Lowcut in Hz (0 = aus, sonst 60/80/120/160).
    pub lc_hz: AtomicU32,
    /// Transienten-Unterdruecker (`tk`): 0 = aus, sonst an.
    pub tk: AtomicBool,
    /// Verhaeltnis schnell/langsam, ab dem der TK absenkt (0..20 dB).
    pub tk_schwelle: AtomicU32,
    /// Wie weit der TK absenkt (0..30 dB, Vorgabe 15).
    pub tk_tiefe: AtomicU32,
    /// Staerke der Rauschunterdrueckung 0..1 (0 = aus).
    pub nr: AtomicU32,
    /// Schwelle des NR-Expanders in dBFS (-60..-20).
    pub nr_schwelle: AtomicU32,
    /// 0 = eigene Berechnung, 1 = einfacher Denoiser, 2 = RNNoise (Zugabe).
    pub nr_modus: AtomicU32,
    /// Expander innerhalb der NR zuschalten (sonst nur spektrale Subtraktion).
    pub nr_gate: AtomicBool,
    /// Effekt-Sends in dB (`send_r`, `send_d`, `send_fx1`, `send_fx2`);
    /// [`effekte::AUS_DB`] = aus.
    pub fx_send: [AtomicU32; FX_WEGE],
    /// Send hinter dem Fader (`post_<weg>`); sonst vor dem Fader.
    pub fx_post: [AtomicBool; FX_WEGE],
    /// Karaoke-Modus (`karaoke`, siehe `effekte::KARAOKE_*`).
    pub karaoke: AtomicU32,
    /// Mitte stumm (`muc`).
    pub muc: AtomicBool,
    /// Klangfarbe −1..1 (`farbe`, Intellipan/Voice Color).
    pub farbe: AtomicU32,
    /// Green Mode (`green`): Bearbeitungskette umgehen, nur Fader/Pan/Routing.
    pub green: AtomicBool,
    /// Insert-Weg einschalten (`insert`); wirkt erst, wenn [`ins_bereit`].
    ///
    /// [`ins_bereit`]: StripParam::ins_bereit
    pub insert: AtomicBool,
    /// Sind Insert-Ausgang und -Rückweg verbunden? Setzt der Port-Wächter,
    /// nicht die Bedienung; ohne Rückweg bliebe der Streifen sonst stumm.
    pub ins_bereit: AtomicBool,
    /// Gewählte Programme für Compressor, Gate, Denoiser (`prg_c/g/d`,
    /// 0 = eigene Einstellung).
    pub prg: [AtomicU32; 3],
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
            limit: AtomicU32::new(12.0f32.to_bits()),
            pan: AtomicU32::new(0f32.to_bits()),
            mute: AtomicBool::new(false),
            solo: AtomicBool::new(false),
            mono: AtomicBool::new(false),
            sends,
            eq_on: AtomicBool::new(false),
            eq: std::array::from_fn(|_| std::array::from_fn(|_| EqParam::new())),
            comp: CompParam::new(),
            gate: GateParam::new(),
            dn_on: AtomicBool::new(false),
            dn_floor: AtomicU32::new((-30.0f32).to_bits()),
            dn_threshold: AtomicU32::new((-55.0f32).to_bits()),
            lc_hz: AtomicU32::new(0f32.to_bits()),
            tk: AtomicBool::new(false),
            tk_schwelle: AtomicU32::new(6.0f32.to_bits()),
            tk_tiefe: AtomicU32::new(15.0f32.to_bits()),
            nr: AtomicU32::new(0f32.to_bits()),
            nr_schwelle: AtomicU32::new((-45.0f32).to_bits()),
            nr_modus: AtomicU32::new(0),
            nr_gate: AtomicBool::new(false),
            fx_send: std::array::from_fn(|_| AtomicU32::new(effekte::AUS_DB.to_bits())),
            fx_post: std::array::from_fn(|_| AtomicBool::new(true)),
            karaoke: AtomicU32::new(0),
            muc: AtomicBool::new(false),
            farbe: AtomicU32::new(0f32.to_bits()),
            green: AtomicBool::new(false),
            insert: AtomicBool::new(false),
            ins_bereit: AtomicBool::new(false),
            prg: std::array::from_fn(|_| AtomicU32::new(0)),
            level: AtomicU32::new(0f32.to_bits()),
            label: Mutex::new(String::new()),
        }
    }

    /// Ist im FX-Bereich etwas eingestellt (Send offen, Klang verändert,
    /// Green Mode, Insert)? Die Oberfläche markiert dann den FX-Knopf.
    pub fn fx_belegt(&self) -> bool {
        self.fx_send.iter().any(|a| fget(a) > effekte::AUS_DB)
            || fget(&self.farbe) != 0.0
            || self.karaoke.load(Relaxed) != 0
            || self.muc.load(Relaxed)
            || self.green.load(Relaxed)
            || self.insert.load(Relaxed)
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
    pub limit: AtomicU32,
    pub mute: AtomicBool,
    pub mono: AtomicBool,
    /// Mischmodus, siehe `bus_mix` und `BUS_MODE_*`.
    pub mode: AtomicU32,
    /// Kreuzmischung: tauscht L und R am Bus-Eingang (`set_bus("cross", ...)`).
    pub cross: AtomicBool,
    pub trim: [AtomicU32; 2],
    pub delay_ms: [AtomicU32; 2],
    pub eq_on: AtomicBool,
    pub eq: [[EqParam; EQ_CELLS]; 2],
    pub limiter_on: AtomicBool,
    pub c5_on: AtomicBool,
    pub c5_threshold: AtomicU32,
    pub c5_ceiling: AtomicU32,
    /// Effekt-Returns in dB (`ret_r`, `ret_d`, `ret_fx1`, `ret_fx2`).
    pub fx_ret: [AtomicU32; FX_WEGE],
    /// Gewählter Bus (`sel`, höchstens einer): Ziel des Master-Zugs.
    pub sel: AtomicBool,
    /// Abhören (`monitor`, höchstens einer): A1 gibt statt seiner eigenen
    /// Mischung diesen Bus aus – „was hört der Stream?“.
    pub monitor: AtomicBool,
    /// Green Mode (`green`): EQ, Limiter und C5 umgehen.
    pub green: AtomicBool,
    /// Insert-Weg (`insert`), wie beim Streifen.
    pub insert: AtomicBool,
    pub ins_bereit: AtomicBool,
    /// Bus zusätzlich als eigenes virtuelles Gerät (`vaio`, nur A1–A5;
    /// B1–B3 sind es ohnehin).
    pub vaio: AtomicBool,
    pub level: AtomicU32,
    /// Frei wählbarer Name (`/api/set` mit `param:label`); leer = Vorgabename.
    pub label: Mutex<String>,
}

impl BusParam {
    /// Bus `index`: A1 bekommt die Effekt-Returns ab Werk auf 0 dB, damit
    /// ein aufgedrehter Send sofort hörbar ist.
    fn new(index: usize) -> BusParam {
        let ret = if index == 0 { 0.0f32 } else { effekte::AUS_DB };
        BusParam {
            gain: AtomicU32::new(0f32.to_bits()),
            limit: AtomicU32::new(0.0f32.to_bits()),
            mute: AtomicBool::new(false),
            mono: AtomicBool::new(false),
            mode: AtomicU32::new(BUS_MODE_NORMAL),
            cross: AtomicBool::new(false),
            trim: std::array::from_fn(|_| AtomicU32::new(0f32.to_bits())),
            delay_ms: std::array::from_fn(|_| AtomicU32::new(0f32.to_bits())),
            eq_on: AtomicBool::new(false),
            eq: std::array::from_fn(|_| std::array::from_fn(|_| EqParam::new())),
            limiter_on: AtomicBool::new(false),
            c5_on: AtomicBool::new(false),
            c5_threshold: AtomicU32::new((-18.0f32).to_bits()),
            c5_ceiling: AtomicU32::new((-0.3f32).to_bits()),
            fx_ret: std::array::from_fn(|_| AtomicU32::new(ret.to_bits())),
            sel: AtomicBool::new(index == 0),
            monitor: AtomicBool::new(false),
            green: AtomicBool::new(false),
            insert: AtomicBool::new(false),
            ins_bereit: AtomicBool::new(false),
            vaio: AtomicBool::new(false),
            level: AtomicU32::new(0f32.to_bits()),
            label: Mutex::new(String::new()),
        }
    }

    /// Wie [`StripParam::fx_belegt`]: Abhören, `vaio`, Green Mode, Insert.
    /// Die Returns zählen nicht – sie sind ab Werk auf A1 offen.
    pub fn fx_belegt(&self) -> bool {
        self.monitor.load(Relaxed)
            || self.vaio.load(Relaxed)
            || self.green.load(Relaxed)
            || self.insert.load(Relaxed)
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

/// Einstellungen der eingebauten Send-Effekte (Ziel `fx`, Index 0).
pub struct FxParam {
    /// Raumgröße des Halls 0..1 (`hall_raum`).
    pub hall_raum: AtomicU32,
    /// Höhendämpfung des Halls 0..1 (`hall_daempfung`).
    pub hall_daempfung: AtomicU32,
    /// Echozeit in ms (`echo_zeit`, 1..2000).
    pub echo_zeit: AtomicU32,
    /// Rückkopplung 0..0,9 (`echo_rueck`).
    pub echo_rueck: AtomicU32,
    /// Höhendämpfung der Wiederholungen 0..1 (`echo_daempfung`).
    pub echo_daempfung: AtomicU32,
}

impl FxParam {
    fn new() -> FxParam {
        FxParam {
            hall_raum: AtomicU32::new(0.5f32.to_bits()),
            hall_daempfung: AtomicU32::new(0.5f32.to_bits()),
            echo_zeit: AtomicU32::new(300.0f32.to_bits()),
            echo_rueck: AtomicU32::new(0.35f32.to_bits()),
            echo_daempfung: AtomicU32::new(0.3f32.to_bits()),
        }
    }

    fn set(&self, param: &str, v: f32) -> bool {
        match param {
            "hall_raum" => fset(&self.hall_raum, v.clamp(0.0, 1.0)),
            "hall_daempfung" => fset(&self.hall_daempfung, v.clamp(0.0, 1.0)),
            "echo_zeit" => fset(&self.echo_zeit, v.clamp(1.0, effekte::ECHO_MAX_MS)),
            "echo_rueck" => fset(&self.echo_rueck, v.clamp(0.0, 0.9)),
            "echo_daempfung" => fset(&self.echo_daempfung, v.clamp(0.0, 1.0)),
            _ => return false,
        }
        true
    }

    /// Namen und Werte, wie sie im Preset stehen.
    pub fn werte(&self) -> [(&'static str, f32); 5] {
        [
            ("hall_raum", fget(&self.hall_raum)),
            ("hall_daempfung", fget(&self.hall_daempfung)),
            ("echo_zeit", fget(&self.echo_zeit)),
            ("echo_rueck", fget(&self.echo_rueck)),
            ("echo_daempfung", fget(&self.echo_daempfung)),
        ]
    }
}

pub struct Params {
    pub strips: [StripParam; STRIPS],
    pub buses: [BusParam; BUSES],
    pub fx: FxParam,
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
            fx: FxParam::new(),
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
            "strip" => {
                if let Some(modul) = crate::programme::Modul::aus_param(param) {
                    return value >= 0.0
                        && crate::programme::waehlen(self, index, modul, value as u32);
                }
                match self.strips.get(index) {
                    Some(s) => {
                        let ok = set_strip(s, param, value);
                        // Ein von Hand geaenderter Wert hebt das Programm auf.
                        if ok {
                            if let Some(m) = crate::programme::Modul::von_wert(param) {
                                s.prg[m as usize].store(0, Relaxed);
                            }
                        }
                        ok
                    }
                    None => false,
                }
            }
            // `sel` und `monitor` gelten für höchstens einen Bus: Einschalten
            // nimmt sie den anderen. `sel` lässt sich nicht abwählen – ein
            // Bus ist immer der gewählte.
            "bus" if matches!(param, "sel" | "monitor") => {
                if index >= self.buses.len() {
                    return false;
                }
                let an = value != 0.0;
                fn feld(b: &BusParam, sel: bool) -> &AtomicBool {
                    if sel {
                        &b.sel
                    } else {
                        &b.monitor
                    }
                }
                let sel = param == "sel";
                if !an {
                    // Ausschalten betrifft nur diesen Bus (bei `sel` nichts).
                    if !sel {
                        self.buses[index].monitor.store(false, Relaxed);
                    }
                    return true;
                }
                for (i, b) in self.buses.iter().enumerate() {
                    feld(b, sel).store(an && i == index, Relaxed);
                }
                true
            }
            "bus" => match self.buses.get(index) {
                Some(b) => set_bus(b, param, value),
                None => false,
            },
            "fx" if index == 0 => self.fx.set(param, value),
            _ => false,
        }
    }

    /// Index des gewählten Busses (`sel`).
    pub fn sel_bus(&self) -> usize {
        self.buses
            .iter()
            .position(|b| b.sel.load(Relaxed))
            .unwrap_or(0)
    }

    /// Bus, den A1 gerade abhört (`monitor`), falls einer.
    pub fn monitor_bus(&self) -> Option<usize> {
        self.buses.iter().position(|b| b.monitor.load(Relaxed))
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

    /// EQ-Zellen eines Streifens als JSON (Kanal 0; die Zellen sind fuer beide
    /// Kanaele gleich belegt, nur der Speicher ist getrennt).
    pub fn eq_json(&self, strip: usize) -> Option<crate::json::Value> {
        use std::sync::atomic::Ordering::Relaxed;
        let s = self.strips.get(strip)?;
        let cells: Vec<crate::json::Value> = s.eq[0]
            .iter()
            .map(|c| {
                crate::json::Value::Obj(vec![
                    ("on".into(), crate::json::Value::Bool(c.on.load(Relaxed))),
                    (
                        "kind".into(),
                        crate::json::Value::Num(c.kind.load(Relaxed) as f64),
                    ),
                    (
                        "freq".into(),
                        crate::json::Value::Num(f32::from_bits(c.freq.load(Relaxed)) as f64),
                    ),
                    (
                        "q".into(),
                        crate::json::Value::Num(f32::from_bits(c.q.load(Relaxed)) as f64),
                    ),
                    (
                        "gain".into(),
                        crate::json::Value::Num(f32::from_bits(c.gain.load(Relaxed)) as f64),
                    ),
                ])
            })
            .collect();
        Some(crate::json::Value::Obj(vec![
            ("strip".into(), crate::json::Value::Num(strip as f64)),
            ("cells".into(), crate::json::Value::Arr(cells)),
        ]))
    }

    pub fn to_json(&self) -> Value {
        Value::obj(vec![
            (
                "strips",
                Value::Arr(
                    self.strips
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
                                ("eq", Value::Bool(s.eq_on.load(Relaxed))),
                                ("comp", Value::Bool(s.comp.on.load(Relaxed))),
                                ("gate", Value::Bool(s.gate.on.load(Relaxed))),
                                ("denoiser", Value::Bool(s.dn_on.load(Relaxed))),
                                ("lowcut", Value::num(fget(&s.lc_hz) as f64)),
                                ("tk", Value::Bool(s.tk.load(Relaxed))),
                                ("tk_schwelle", Value::num(fget(&s.tk_schwelle) as f64)),
                                ("tk_tiefe", Value::num(fget(&s.tk_tiefe) as f64)),
                                ("nr", Value::num(fget(&s.nr) as f64)),
                                ("nr_schwelle", Value::num(fget(&s.nr_schwelle) as f64)),
                                ("nr_modus", Value::num(s.nr_modus.load(Relaxed) as f64)),
                                ("nr_gate", Value::Bool(s.nr_gate.load(Relaxed))),
                                ("gate_schwelle", Value::num(fget(&s.gate.threshold) as f64)),
                                ("gate_hold", Value::num(fget(&s.gate.hold) as f64)),
                                ("gate_tiefe", Value::num(fget(&s.gate.tiefe) as f64)),
                                ("gate_attack", Value::num(fget(&s.gate.attack) as f64)),
                                ("gate_release", Value::num(fget(&s.gate.release) as f64)),
                                ("comp_threshold", Value::num(fget(&s.comp.threshold) as f64)),
                                ("comp_ratio", Value::num(fget(&s.comp.ratio) as f64)),
                                ("comp_attack", Value::num(fget(&s.comp.attack) as f64)),
                                ("comp_release", Value::num(fget(&s.comp.release) as f64)),
                                ("comp_knee", Value::num(fget(&s.comp.knee) as f64)),
                                ("comp_gain_in", Value::num(fget(&s.comp.gain_in) as f64)),
                                ("comp_gain_out", Value::num(fget(&s.comp.gain_out) as f64)),
                                (
                                    "fx_send",
                                    Value::Arr(
                                        s.fx_send
                                            .iter()
                                            .map(|a| Value::num(fget(a) as f64))
                                            .collect(),
                                    ),
                                ),
                                (
                                    "fx_post",
                                    Value::Arr(
                                        s.fx_post
                                            .iter()
                                            .map(|a| Value::Bool(a.load(Relaxed)))
                                            .collect(),
                                    ),
                                ),
                                ("karaoke", Value::num(s.karaoke.load(Relaxed) as f64)),
                                ("muc", Value::Bool(s.muc.load(Relaxed))),
                                ("farbe", Value::num(fget(&s.farbe) as f64)),
                                ("green", Value::Bool(s.green.load(Relaxed))),
                                ("insert", Value::Bool(s.insert.load(Relaxed))),
                                (
                                    "prg",
                                    Value::Arr(
                                        s.prg
                                            .iter()
                                            .map(|a| Value::num(a.load(Relaxed) as f64))
                                            .collect(),
                                    ),
                                ),
                                // EQ-Baender (Kanal 0; beide Kanaele sind gleich belegt).
                                (
                                    "eq_cells",
                                    Value::Arr(
                                        s.eq[0]
                                            .iter()
                                            .map(|c| {
                                                Value::obj(vec![
                                                    ("on", Value::Bool(c.on.load(Relaxed))),
                                                    (
                                                        "type",
                                                        Value::num(c.kind.load(Relaxed) as f64),
                                                    ),
                                                    ("freq", Value::num(fget(&c.freq) as f64)),
                                                    ("q", Value::num(fget(&c.q) as f64)),
                                                    ("gain", Value::num(fget(&c.gain) as f64)),
                                                ])
                                            })
                                            .collect(),
                                    ),
                                ),
                            ])
                        })
                        .collect(),
                ),
            ),
            (
                "buses",
                Value::Arr(
                    self.buses
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
                                ("eq", Value::Bool(b.eq_on.load(Relaxed))),
                                ("limiter", Value::Bool(b.limiter_on.load(Relaxed))),
                                ("delay_ms", Value::num(fget(&b.delay_ms[0]) as f64)),
                                ("trim", Value::num(fget(&b.trim[0]) as f64)),
                                ("c5", Value::Bool(b.c5_on.load(Relaxed))),
                                ("c5_threshold", Value::num(fget(&b.c5_threshold) as f64)),
                                ("c5_ceiling", Value::num(fget(&b.c5_ceiling) as f64)),
                                (
                                    "fx_ret",
                                    Value::Arr(
                                        b.fx_ret
                                            .iter()
                                            .map(|a| Value::num(fget(a) as f64))
                                            .collect(),
                                    ),
                                ),
                                ("sel", Value::Bool(b.sel.load(Relaxed))),
                                ("monitor", Value::Bool(b.monitor.load(Relaxed))),
                                ("green", Value::Bool(b.green.load(Relaxed))),
                                ("insert", Value::Bool(b.insert.load(Relaxed))),
                                ("vaio", Value::Bool(b.vaio.load(Relaxed))),
                            ])
                        })
                        .collect(),
                ),
            ),
            (
                "fx",
                Value::Obj(
                    self.fx
                        .werte()
                        .iter()
                        .map(|(k, v)| (k.to_string(), Value::num(*v as f64)))
                        .collect(),
                ),
            ),
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
        "gain" => {
            fset(&s.gain, v.clamp(-60.0, 12.0));
            true
        }
        "pan" => {
            fset(&s.pan, v.clamp(-1.0, 1.0));
            true
        }
        "mute" => {
            s.mute.store(v != 0.0, Relaxed);
            true
        }
        "solo" => {
            s.solo.store(v != 0.0, Relaxed);
            true
        }
        "mono" => {
            s.mono.store(v != 0.0, Relaxed);
            true
        }
        "eq" => {
            s.eq_on.store(v != 0.0, Relaxed);
            true
        }
        "comp" => {
            s.comp.on.store(v != 0.0, Relaxed);
            true
        }
        "gate" => {
            s.gate.on.store(v != 0.0, Relaxed);
            true
        }
        "denoiser" => {
            s.dn_on.store(v != 0.0, Relaxed);
            true
        }
        // Lowcut: 0 = aus, sonst auf 60/80/120/160 Hz gerastet.
        "lowcut" => {
            fset(&s.lc_hz, crate::dsp::lowcut_raster(v.clamp(0.0, 160.0)));
            true
        }
        // Transienten-Unterdruecker: 0 = aus, sonst an; Schwelle als
        // Pegelverhaeltnis (0..20 dB), Tiefe 0..30 dB.
        "tk" => {
            s.tk.store(v != 0.0, Relaxed);
            true
        }
        "tk_schwelle" => {
            fset(&s.tk_schwelle, v.clamp(0.0, 20.0));
            true
        }
        "tk_tiefe" => {
            fset(&s.tk_tiefe, v.clamp(0.0, 30.0));
            true
        }
        "nr" => {
            fset(&s.nr, v.clamp(0.0, 1.0));
            true
        }
        "nr_schwelle" => {
            fset(&s.nr_schwelle, v.clamp(-60.0, -20.0));
            true
        }
        "nr_modus" => {
            s.nr_modus.store(v.clamp(0.0, 2.0) as u32, Relaxed);
            true
        }
        "nr_gate" => {
            s.nr_gate.store(v != 0.0, Relaxed);
            true
        }
        "comp_threshold" => {
            fset(&s.comp.threshold, v);
            true
        }
        "comp_ratio" => {
            fset(&s.comp.ratio, v.max(1.0));
            true
        }
        "comp_attack" => {
            fset(&s.comp.attack, v);
            true
        }
        "comp_release" => {
            fset(&s.comp.release, v);
            true
        }
        // Die beiden Gain-Stufen des Compressors waren bisher nur intern
        // gesetzt; hier sind sie ueber /api/set erreichbar.
        "comp_gain_in" => {
            fset(&s.comp.gain_in, v.clamp(-24.0, 24.0));
            true
        }
        "comp_gain_out" => {
            fset(&s.comp.gain_out, v.clamp(-24.0, 24.0));
            true
        }
        "comp_knee" => {
            fset(&s.comp.knee, v);
            true
        }
        // `gate_schwelle` ist der Name aus der Oberflaeche, `gate_threshold`
        // der alte; beide schreiben dasselbe Atom.
        "gate_threshold" | "gate_schwelle" => {
            fset(&s.gate.threshold, v.clamp(-60.0, -20.0));
            true
        }
        "gate_tiefe" => {
            fset(&s.gate.tiefe, v.clamp(0.0, 80.0));
            true
        }
        "gate_attack" => {
            fset(&s.gate.attack, v.clamp(0.1, 500.0));
            true
        }
        "gate_hold" => {
            fset(&s.gate.hold, v.clamp(0.0, 500.0));
            true
        }
        "gate_release" => {
            fset(&s.gate.release, v.clamp(1.0, 5000.0));
            true
        }
        _ => set_strip_ext(s, param, v),
    }
}

/// Sends (`send0..send7`), Effekt-Sends (`send_r` …, `post_r` …), die
/// Klangbausteine und EQ-Zellen (`eq<zelle>_<feld>`).
fn set_strip_ext(s: &StripParam, param: &str, v: f32) -> bool {
    if let Some(weg) = param.strip_prefix("send_").and_then(effekte::weg_index) {
        fset(&s.fx_send[weg], v.clamp(effekte::AUS_DB, effekte::MAX_DB));
        return true;
    }
    if let Some(weg) = param.strip_prefix("post_").and_then(effekte::weg_index) {
        s.fx_post[weg].store(v != 0.0, Relaxed);
        return true;
    }
    match param {
        "karaoke" => {
            s.karaoke
                .store(v.clamp(0.0, effekte::KARAOKE_MAX as f32) as u32, Relaxed);
            return true;
        }
        "muc" => {
            s.muc.store(v != 0.0, Relaxed);
            return true;
        }
        "farbe" => {
            fset(&s.farbe, v.clamp(-1.0, 1.0));
            return true;
        }
        "green" => {
            s.green.store(v != 0.0, Relaxed);
            return true;
        }
        "insert" => {
            s.insert.store(v != 0.0, Relaxed);
            return true;
        }
        _ => {}
    }
    if let Some(rest) = param.strip_prefix("send") {
        return match rest.parse::<usize>().ok().and_then(|b| s.sends.get(b)) {
            Some(a) => {
                fset(a, v.clamp(0.0, 2.0));
                true
            }
            None => false,
        };
    }
    if let Some(rest) = param.strip_prefix("eq") {
        let (cell_s, field) = match rest.split_once('_') {
            Some(x) => x,
            None => return false,
        };
        let c: usize = match cell_s.parse() {
            Ok(c) => c,
            Err(_) => return false,
        };
        for ch in 0..2 {
            match s.eq[ch].get(c) {
                Some(cell) => {
                    if !set_eq_cell(cell, field, v) {
                        return false;
                    }
                }
                None => return false,
            }
        }
        return true;
    }
    false
}

fn set_eq_cell(cell: &EqParam, field: &str, v: f32) -> bool {
    match field {
        "on" => cell.on.store(v != 0.0, Relaxed),
        "type" => cell.kind.store(v.max(0.0) as u32, Relaxed),
        "freq" => fset(&cell.freq, v.clamp(10.0, 24000.0)),
        "q" => fset(&cell.q, v.clamp(0.05, 20.0)),
        "gain" => fset(&cell.gain, v.clamp(-24.0, 24.0)),
        _ => return false,
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
        "eq" => b.eq_on.store(v != 0.0, Relaxed),
        "limiter" => b.limiter_on.store(v != 0.0, Relaxed),
        "c5" => b.c5_on.store(v != 0.0, Relaxed),
        "c5_threshold" => fset(&b.c5_threshold, v.clamp(-60.0, 0.0)),
        "c5_ceiling" => fset(&b.c5_ceiling, v.clamp(-24.0, 0.0)),
        "trim" => {
            fset(&b.trim[0], v);
            fset(&b.trim[1], v);
        }
        "delay" => {
            fset(&b.delay_ms[0], v.clamp(0.0, 2000.0));
            fset(&b.delay_ms[1], v.clamp(0.0, 2000.0));
        }
        "green" => b.green.store(v != 0.0, Relaxed),
        "insert" => b.insert.store(v != 0.0, Relaxed),
        "vaio" => b.vaio.store(v != 0.0, Relaxed),
        _ => match param.strip_prefix("ret_").and_then(effekte::weg_index) {
            Some(weg) => fset(&b.fx_ret[weg], v.clamp(effekte::AUS_DB, effekte::MAX_DB)),
            None => return false,
        },
    }
    true
}

/// −3 dB als Linearwert; Dämpfung für Composite und TV-Mix.
const MINUS_3DB: f32 = 0.707_945_8;

/// Kanalmischung eines Busses am Bus-Eingang, vor Limiter/Delay/Trim.
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
/// auf Center, LFE und Rear. Ein Bus ist hier immer stereo — es gibt nur
/// `outs[bi][0]` und `outs[bi][1]`, die zusätzlichen Lautsprecherkanäle
/// existieren also nicht. Es bleibt die Monosumme, die auf beide
/// Ausgangskanäle gelegt wird; die Modi unterscheiden sich daher im Ergebnis
/// (noch) nicht. Sobald mehrkanalige Busse dazukommen, wird hier verzweigt.
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
///
/// Nullzeiger sind erlaubt: ein fehlender Eingang liest Stille, ein fehlender
/// Ausgang wird übersprungen (Tests brauchen nur `ins`/`outs`).
pub struct Io {
    pub ins: [[*const f32; 2]; STRIPS],
    pub outs: [[*mut f32; 2]; BUSES],
    /// Externe Effekt-Sends FX1/FX2 (`fx1_L/R`, `fx2_L/R`).
    pub fx_out: [[*mut f32; 2]; 2],
    /// Rückwege der externen Effekte (`fx1_ret_L/R`, `fx2_ret_L/R`).
    pub fx_ret: [[*const f32; 2]; 2],
    /// Insert-Ausgang und -Rückweg je Streifen (`strip1_ins_L` → … →
    /// `strip1_ret_L`).
    pub strip_ins: [[*mut f32; 2]; STRIPS],
    pub strip_ret: [[*const f32; 2]; STRIPS],
    /// Dasselbe je Bus (`busA1_ins_L`, `busA1_ret_L`).
    pub bus_ins: [[*mut f32; 2]; BUSES],
    pub bus_ret: [[*const f32; 2]; BUSES],
}

impl Io {
    /// Alle Zeiger null.
    pub fn leer() -> Io {
        Io {
            ins: [[std::ptr::null(); 2]; STRIPS],
            outs: [[std::ptr::null_mut(); 2]; BUSES],
            fx_out: [[std::ptr::null_mut(); 2]; 2],
            fx_ret: [[std::ptr::null(); 2]; 2],
            strip_ins: [[std::ptr::null_mut(); 2]; STRIPS],
            strip_ret: [[std::ptr::null(); 2]; STRIPS],
            bus_ins: [[std::ptr::null_mut(); 2]; BUSES],
            bus_ret: [[std::ptr::null(); 2]; BUSES],
        }
    }
}

/// Liest Sample `n` – Nullzeiger ist Stille.
///
/// # Safety
/// `p` ist null oder zeigt auf mindestens `n + 1` Samples.
#[inline]
unsafe fn lies(p: *const f32, n: usize) -> f32 {
    if p.is_null() {
        0.0
    } else {
        unsafe { *p.add(n) }
    }
}

/// Schreibt Sample `n` – Nullzeiger wird übersprungen.
///
/// # Safety
/// `p` ist null oder zeigt auf mindestens `n + 1` Samples.
#[inline]
unsafe fn schreib(p: *mut f32, n: usize, x: f32) {
    if !p.is_null() {
        unsafe { *p.add(n) = x };
    }
}

/// Wie lange ein Send-Effekt nach dem letzten aktiven Send weiterläuft
/// (Sekunden) – lang genug für jeden Nachhall, danach spart er Rechenzeit.
const FX_NACHLAUF_S: f32 = 8.0;

/// DSP-Zustand, ausschließlich vom Audio-Thread benutzt.
pub struct EngineDsp {
    strip_lc: [[LowCut; 2]; STRIPS],
    /// Transienten-Unterdruecker je Kanal (Lookahead-Ring in `new` allokiert).
    strip_tk: [[TransientenUnterdruecker; 2]; STRIPS],
    strip_nr: [[NoiseReduction; 2]; STRIPS],
    /// RNNoise-Kanaele, nur wenn die Bibliothek geladen werden konnte.
    strip_rn: [[Option<crate::rnnoise::Kanal>; 2]; STRIPS],
    /// Laeuft der RNNoise-Modus gerade (Modus 2 und Bibliothek da)?
    rn_on: [[bool; 2]; STRIPS],
    strip_eq: [[[Biquad; EQ_CELLS]; 2]; STRIPS],
    strip_comp: [[Compressor; 2]; STRIPS],
    strip_gate: [[Gate; 2]; STRIPS],
    strip_dn: [[Denoiser; 2]; STRIPS],
    bus_eq: [[[Biquad; EQ_CELLS]; 2]; BUSES],
    bus_lim: [[Limiter; 2]; BUSES],
    bus_c5: [C5Limiter; BUSES],
    bus_delay: [[DelayLine; 2]; BUSES],
    strip_farbe: [[Klangfarbe; 2]; STRIPS],
    strip_karaoke: [Karaoke; STRIPS],
    hall: Hall,
    echo: Echo,
    /// Restliche Samples, die Hall bzw. Echo noch nachklingen dürfen.
    fx_nachlauf: [usize; 2],
    prev_strip_gain: [f32; STRIPS],
    prev_bus_gain: [f32; BUSES],
    last_sr: f32,
    pub blocks: u64,
}

impl EngineDsp {
    pub fn new(max_block: usize) -> EngineDsp {
        EngineDsp {
            strip_lc: [[LowCut::new(); 2]; STRIPS],
            // Der TK bringt je Kanal einen Lookahead-Ring mit; er entsteht
            // hier, ausserhalb des Callbacks.
            strip_tk: std::array::from_fn(|_| {
                [
                    TransientenUnterdruecker::new(48000.0),
                    TransientenUnterdruecker::new(48000.0),
                ]
            }),
            // Fenster, Tabellen und Ringpuffer entstehen hier, einmal, mit
            // fester Groesse - nicht im Audio-Callback.
            strip_nr: std::array::from_fn(|_| {
                [NoiseReduction::new(48000.0), NoiseReduction::new(48000.0)]
            }),
            // Nur wenn librnnoise.so.0 da ist, entstehen Kanaele - und zwar
            // hier, ausserhalb des Callbacks.
            strip_rn: std::array::from_fn(|_| {
                [crate::rnnoise::Kanal::neu(), crate::rnnoise::Kanal::neu()]
            }),
            rn_on: [[false; 2]; STRIPS],
            strip_eq: std::array::from_fn(|_| {
                std::array::from_fn(|_| [Biquad::bypass(); EQ_CELLS])
            }),
            strip_comp: [[Compressor::default(); 2]; STRIPS],
            strip_gate: [[Gate::default(); 2]; STRIPS],
            strip_dn: [[Denoiser::default(); 2]; STRIPS],
            bus_eq: std::array::from_fn(|_| std::array::from_fn(|_| [Biquad::bypass(); EQ_CELLS])),
            bus_lim: [[Limiter::default(); 2]; BUSES],
            bus_c5: std::array::from_fn(|_| C5Limiter::default()),
            bus_delay: std::array::from_fn(|_| {
                [DelayLine::new(max_block * 4), DelayLine::new(max_block * 4)]
            }),
            strip_farbe: [[Klangfarbe::default(); 2]; STRIPS],
            strip_karaoke: [Karaoke::default(); STRIPS],
            hall: Hall::neu(),
            echo: Echo::neu(),
            fx_nachlauf: [0; 2],
            prev_strip_gain: [1.0; STRIPS],
            prev_bus_gain: [1.0; BUSES],
            last_sr: 0.0,
            blocks: 0,
        }
    }

    fn reconfigure(&mut self, p: &Params, sr: f32) {
        let need_eq = (self.last_sr - sr).abs() > 0.5 || self.blocks == 0;
        for i in 0..STRIPS {
            let s = &p.strips[i];
            for ch in 0..2 {
                if need_eq || s.eq_on.load(Relaxed) {
                    for c in 0..EQ_CELLS {
                        let cell = &s.eq[ch][c];
                        let on = cell.on.load(Relaxed) && s.eq_on.load(Relaxed);
                        let b = &mut self.strip_eq[i][ch][c];
                        if on {
                            b.coeffs(
                                FilterType::from_u8(cell.kind.load(Relaxed) as u8),
                                fget(&cell.freq),
                                fget(&cell.q),
                                fget(&cell.gain),
                                sr,
                            );
                        }
                    }
                }
                self.strip_comp[i][ch].enabled = s.comp.on.load(Relaxed);
                self.strip_comp[i][ch].set_times(fget(&s.comp.attack), fget(&s.comp.release), sr);
                let lc = fget(&s.lc_hz);
                self.strip_lc[i][ch].set_hz(lc, sr);
                // TK: Schalter und Rate je Block; der Ring bleibt im DSP-Zustand.
                self.strip_tk[i][ch].setup(s.tk.load(Relaxed), sr);
                self.strip_gate[i][ch].enabled = s.gate.on.load(Relaxed);
                self.strip_gate[i][ch].set_tiefe(fget(&s.gate.tiefe));
                self.strip_gate[i][ch].set_times(
                    fget(&s.gate.attack),
                    fget(&s.gate.hold),
                    fget(&s.gate.release),
                    sr,
                );
                // NR: Stufe 'eigen' (Modus 0) ist die eigene Berechnung,
                // 'einfach' (1) benutzt den vorhandenen Denoiser, 'rnnoise'
                // (2) nur, wenn die Bibliothek geladen ist (sonst Modus 0).
                let nr_st = fget(&s.nr);
                let modus = s.nr_modus.load(Relaxed);
                let nr = &mut self.strip_nr[i][ch];
                nr.gate_on = s.nr_gate.load(Relaxed);
                // Modus 2 nur bei 48 kHz und vorhandener Bibliothek; sonst
                // faellt der Streifen still auf die eigene Berechnung zurueck.
                let rn_geht =
                    self.strip_rn[i][ch].is_some() && (sr - 48000.0).abs() < 0.5 && modus == 2;
                self.rn_on[i][ch] = nr_st > 0.0 && rn_geht;
                nr.enabled = nr_st > 0.0 && (modus == 0 || (modus == 2 && !rn_geht));
                self.strip_dn[i][ch].enabled = s.dn_on.load(Relaxed) || (nr_st > 0.0 && modus == 1);
                self.strip_dn[i][ch].set_sample_rate(sr);
                self.strip_farbe[i][ch].setup(fget(&s.farbe), sr);
            }
            self.strip_karaoke[i].set_sample_rate(sr);
        }
        self.hall.set_sample_rate(sr);
        for i in 0..BUSES {
            let b = &p.buses[i];
            for ch in 0..2 {
                if need_eq || b.eq_on.load(Relaxed) {
                    for c in 0..EQ_CELLS {
                        let cell = &b.eq[ch][c];
                        let on = cell.on.load(Relaxed) && b.eq_on.load(Relaxed);
                        let f = &mut self.bus_eq[i][ch][c];
                        if on {
                            f.coeffs(
                                FilterType::from_u8(cell.kind.load(Relaxed) as u8),
                                fget(&cell.freq),
                                fget(&cell.q),
                                fget(&cell.gain),
                                sr,
                            );
                        }
                    }
                }
                self.bus_lim[i][ch].enabled = b.limiter_on.load(Relaxed);
                self.bus_lim[i][ch].set_release(50.0, sr);
                self.bus_delay[i][ch].set_ms(fget(&b.delay_ms[ch]), sr);
            }
        }
        self.last_sr = sr;
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

        // Je Block einmal: Sends, Returns, Schalter. Im Sample-Takt wird nur
        // noch gelesen, nicht umgerechnet.
        let mut send_lin = [[0f32; FX_WEGE]; STRIPS];
        let mut send_post = [[true; FX_WEGE]; STRIPS];
        let mut strip_green = [false; STRIPS];
        let mut strip_ins = [false; STRIPS];
        let mut fx_aktiv = [false; FX_WEGE];
        for si in 0..STRIPS {
            let sp = &p.strips[si];
            for w in 0..FX_WEGE {
                send_lin[si][w] = effekte::pegel_lin(fget(&sp.fx_send[w]));
                send_post[si][w] = sp.fx_post[w].load(Relaxed);
                fx_aktiv[w] |= send_lin[si][w] > 0.0;
            }
            strip_green[si] = sp.green.load(Relaxed);
            strip_ins[si] = sp.insert.load(Relaxed)
                && sp.ins_bereit.load(Relaxed)
                && !io.strip_ins[si][0].is_null()
                && !io.strip_ret[si][0].is_null();
        }
        let mut ret_lin = [[0f32; FX_WEGE]; BUSES];
        let mut bus_green = [false; BUSES];
        let mut bus_ins = [false; BUSES];
        for bi in 0..BUSES {
            let bp = &p.buses[bi];
            for w in 0..FX_WEGE {
                ret_lin[bi][w] = effekte::pegel_lin(fget(&bp.fx_ret[w]));
            }
            bus_green[bi] = bp.green.load(Relaxed);
            bus_ins[bi] = bp.insert.load(Relaxed)
                && bp.ins_bereit.load(Relaxed)
                && !io.bus_ins[bi][0].is_null()
                && !io.bus_ret[bi][0].is_null();
        }
        // A1 hoert einen anderen Bus ab (A1 selbst abhoeren aendert nichts).
        let monitor = p.monitor_bus().filter(|&m| m != 0);
        // Hall und Echo laufen, solange ein Send offen ist, und danach noch
        // so lange nach, dass kein Nachhall abreisst.
        let nachlauf = (FX_NACHLAUF_S * sr) as usize;
        for (w, rest) in self.fx_nachlauf.iter_mut().enumerate() {
            if fx_aktiv[w] {
                *rest = nachlauf;
            } else {
                *rest = rest.saturating_sub(frames);
            }
        }
        let hall_an = self.fx_nachlauf[0] > 0;
        let echo_an = self.fx_nachlauf[1] > 0;
        let hall_raum = fget(&p.fx.hall_raum);
        let hall_daempfung = fget(&p.fx.hall_daempfung);
        let echo_zeit = fget(&p.fx.echo_zeit);
        let echo_rueck = fget(&p.fx.echo_rueck);
        let echo_daempfung = fget(&p.fx.echo_daempfung);

        for n in 0..frames {
            let mut acc = [[0f32; 2]; BUSES];
            let mut fx_acc = [[0f32; 2]; FX_WEGE];
            for si in 0..STRIPS {
                let sp = &p.strips[si];
                let audible = !sp.mute.load(Relaxed) && (!any_solo || sp.solo.load(Relaxed));
                let gain = if audible {
                    db_to_lin(fget(&sp.gain))
                } else {
                    0.0
                };
                let pan = fget(&sp.pan).clamp(-1.0, 1.0);
                let mono = sp.mono.load(Relaxed);
                let mut s = unsafe { [lies(io.ins[si][0], n), lies(io.ins[si][1], n)] };

                if mono {
                    let m = 0.5 * (s[0] + s[1]);
                    s = [m, m];
                }
                // Green Mode: keine Bearbeitung, nur Fader, Pan und Routing.
                if !strip_green[si] {
                    let karaoke = sp.karaoke.load(Relaxed);
                    let muc = sp.muc.load(Relaxed);
                    if karaoke != 0 || muc {
                        s = self.strip_karaoke[si].process(s, karaoke, muc);
                    }
                    self.strip_kette(sp, si, &mut s, sr);
                }
                // Insert: vor dem Fader hinaus, ueber den Rueckweg wieder herein.
                for ch in 0..2 {
                    unsafe {
                        if strip_ins[si] {
                            schreib(io.strip_ins[si][ch], n, s[ch]);
                            s[ch] = lies(io.strip_ret[si][ch], n);
                        } else {
                            schreib(io.strip_ins[si][ch], n, 0.0);
                        }
                    }
                }
                // Vor dem Fader (fuer Pre-Sends): Stumm/Solo gelten trotzdem.
                let an = if audible { 1.0 } else { 0.0 };
                let pre = [s[0] * an, s[1] * an];
                // (6) Fader und Pan.
                let g = [1.0 - pan, 1.0 + pan];
                let l = s[0] * gain * g[0].min(1.0);
                let r = s[1] * gain * g[1].min(1.0);
                let level = l.abs().max(r.abs());
                if level > peak_s[si] {
                    peak_s[si] = level;
                }
                for bi in 0..BUSES {
                    let send = fget(&sp.sends[bi]);
                    if send != 0.0 {
                        acc[bi][0] += l * send;
                        acc[bi][1] += r * send;
                    }
                }
                for w in 0..FX_WEGE {
                    let lvl = send_lin[si][w];
                    if lvl > 0.0 {
                        let quelle = if send_post[si][w] { [l, r] } else { pre };
                        fx_acc[w][0] += quelle[0] * lvl;
                        fx_acc[w][1] += quelle[1] * lvl;
                    }
                }
            }

            // Send-Effekte: Hall und Echo intern, FX1/FX2 ueber die Ports.
            let mut rueck = [[0f32; 2]; FX_WEGE];
            if hall_an {
                rueck[0] = self.hall.process(fx_acc[0], hall_raum, hall_daempfung);
            }
            if echo_an {
                rueck[1] = self
                    .echo
                    .process(fx_acc[1], echo_zeit, echo_rueck, echo_daempfung, sr);
            }
            for ext in 0..2 {
                for ch in 0..2 {
                    unsafe {
                        schreib(io.fx_out[ext][ch], n, fx_acc[2 + ext][ch]);
                        rueck[2 + ext][ch] = lies(io.fx_ret[ext][ch], n);
                    }
                }
            }

            let mut aus = [[0f32; 2]; BUSES];
            for bi in 0..BUSES {
                let bp = &p.buses[bi];
                let gain = if bp.mute.load(Relaxed) {
                    0.0
                } else {
                    db_to_lin(fget(&bp.gain))
                };
                let mono = bp.mono.load(Relaxed);
                let mode = bp.mode.load(Relaxed);
                let cross = bp.cross.load(Relaxed);
                let mut v = acc[bi];
                for w in 0..FX_WEGE {
                    let r = ret_lin[bi][w];
                    if r > 0.0 {
                        v[0] += rueck[w][0] * r;
                        v[1] += rueck[w][1] * r;
                    }
                }
                if mono {
                    let m = 0.5 * (v[0] + v[1]);
                    v = [m, m];
                }
                v = bus_mix(v, mode, cross);
                let green = bus_green[bi];
                for ch in 0..2 {
                    let mut x = v[ch];
                    if !green && bp.eq_on.load(Relaxed) {
                        for c in 0..EQ_CELLS {
                            x = self.bus_eq[bi][ch][c].process(x);
                        }
                    }
                    unsafe {
                        if bus_ins[bi] {
                            schreib(io.bus_ins[bi][ch], n, x);
                            x = lies(io.bus_ret[bi][ch], n);
                        } else {
                            schreib(io.bus_ins[bi][ch], n, 0.0);
                        }
                    }
                    if !green {
                        x = self.bus_lim[bi][ch].process(x, fget(&bp.limit));
                        if bp.c5_on.load(Relaxed) {
                            let th = fget(&bp.c5_threshold);
                            let ce = fget(&bp.c5_ceiling);
                            let cfg = C5Config {
                                enabled: true,
                                threshold_db: [th; 3],
                                ceiling_db: [ce; 3],
                                ..C5Config::default()
                            };
                            x = self.bus_c5[bi].process(x, &cfg, sr);
                        }
                    }
                    x = self.bus_delay[bi][ch].process(x);
                    x *= db_to_lin(fget(&bp.trim[ch]));
                    x *= gain;
                    v[ch] = x;
                }
                aus[bi] = v;
                let level = v[0].abs().max(v[1].abs());
                if level > peak_b[bi] {
                    peak_b[bi] = level;
                }
            }
            for bi in 0..BUSES {
                // Abhoeren: A1 gibt den gewaehlten Bus aus statt seiner Mischung.
                let v = match monitor {
                    Some(m) if bi == 0 => aus[m],
                    _ => aus[bi],
                };
                unsafe {
                    schreib(io.outs[bi][0], n, v[0]);
                    schreib(io.outs[bi][1], n, v[1]);
                }
            }

            // Die akkumulierten Werte gelten fuer alle Streifen dieses Frames.
            if n + 1 == frames {
                for si in 0..STRIPS {
                    let old = fget(&p.strips[si].level);
                    fset(&p.strips[si].level, peak_s[si].max(old * 0.72));
                }
                for bi in 0..BUSES {
                    let old = fget(&p.buses[bi].level);
                    fset(&p.buses[bi].level, peak_b[bi].max(old * 0.72));
                }
                // Letzte Handlung des Blocks: der Zaehler zeigt an, dass die
                // Pegel oben vollstaendig geschrieben sind. Nur ein Store,
                // keine Allokation, kein Lock.
                p.frames.fetch_add(frames as u64, Ordering::Release);
            }
        }
        self.prev_strip_gain = std::array::from_fn(|i| db_to_lin(fget(&p.strips[i].gain)));
        self.prev_bus_gain = std::array::from_fn(|i| db_to_lin(fget(&p.buses[i].gain)));
        self.blocks += 1;
    }

    /// Bearbeitungskette eines Streifens (ein Stereo-Sample), ohne Fader:
    /// Lowcut, TK, Gate, NR, EQ, Klangfarbe, Compressor, Denoiser.
    #[inline]
    // Je Kanal mehrere parallele Felder (Filter, Zustand, Sample) – wie in
    // `process` liest der Index sich hier besser als ein Iterator.
    #[allow(clippy::needless_range_loop)]
    fn strip_kette(&mut self, sp: &StripParam, si: usize, s: &mut [f32; 2], sr: f32) {
        let nr_st = fget(&sp.nr);
        let nr_modus = sp.nr_modus.load(Relaxed);
        let nr_schwelle = fget(&sp.nr_schwelle);
        let tk_schwelle = fget(&sp.tk_schwelle);
        let tk_tiefe = fget(&sp.tk_tiefe);
        for ch in 0..2 {
            let mut x = s[ch];
            // (1) LOWCUT: Rumpeln weg, bevor irgendetwas entscheidet.
            x = self.strip_lc[si][ch].process(x);
            // (1b) TK: Klackser und Poltern am Verhaeltnis zweier
            //      Huellkurven, mit 5 ms Vorlauf. Aus = bit-gleich durch.
            let tk = &mut self.strip_tk[si][ch];
            if tk.enabled {
                x = tk.process(x, tk_schwelle, tk_tiefe);
            }
            s[ch] = x;
        }
        // (2) GATE: entscheidet am unveraenderten Pegel ueber Pausen,
        //     fuer beide Kanaele gemeinsam (gekoppelter Detektor).
        let gate_key = s[0].abs().max(s[1].abs());
        let gate_schwelle = fget(&sp.gate.threshold);
        for ch in 0..2 {
            let mut x = self.strip_gate[si][ch].process_key(s[ch], gate_key, gate_schwelle);
            // (3) NR: Rauschen unter dem Signal.
            if self.rn_on[si][ch] {
                if let Some(k) = self.strip_rn[si][ch].as_mut() {
                    x = k.process(x);
                }
            } else if nr_st > 0.0 && nr_modus == 0 {
                x = self.strip_nr[si][ch].process(x, nr_st, nr_schwelle, sr);
            }
            // (4) EQ: formt erst nach dem Bereinigen.
            if sp.eq_on.load(Relaxed) {
                for c in 0..EQ_CELLS {
                    x = self.strip_eq[si][ch][c].process(x);
                }
            }
            // (4b) Klangfarbe (Voice Color); neutral = bit-gleich durch.
            x = self.strip_farbe[si][ch].process(x);
            // (5) COMP: sieht das Gate-Ergebnis, nicht umgekehrt.
            let prefix = db_to_lin(fget(&sp.comp.gain_in));
            x *= prefix;
            let comp = &mut self.strip_comp[si][ch];
            x = comp.process(
                x,
                fget(&sp.comp.threshold),
                fget(&sp.comp.ratio),
                fget(&sp.comp.knee),
                auto_makeup(sp),
            );
            // Stufe 'einfach' der Rauschunterdrueckung: der vorhandene
            // Denoiser, mit Schwelle/Staerke der NR angesteuert.
            let dn = &mut self.strip_dn[si][ch];
            if nr_st > 0.0 && nr_modus == 1 {
                x = dn.process(x, -(6.0 + 24.0 * nr_st), nr_schwelle);
            } else if sp.dn_on.load(Relaxed) {
                x = dn.process(x, fget(&sp.dn_floor), fget(&sp.dn_threshold));
            }
            s[ch] = x;
        }
    }
}

fn auto_makeup(s: &StripParam) -> f32 {
    fget(&s.comp.gain_out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    /// Testpuffer: 2 Kanäle je Streifen/Bus, alle mit demselben Pegel gefüllt.
    struct Buf {
        ins: Vec<Vec<f32>>,
        outs: Vec<Vec<f32>>,
    }

    impl Buf {
        /// Nur Streifen 0 wird angeregt, alle anderen bleiben still.
        fn new(level: f32, frames: usize) -> Buf {
            let mut b = Buf {
                ins: vec![vec![0.0f32; frames]; STRIPS * 2],
                outs: vec![vec![0.0f32; frames]; BUSES * 2],
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

        fn io(&mut self) -> Io {
            let mut io = Io::leer();
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
            .spawn(|| Box::new(EngineDsp::new(64)))
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

    #[test]
    fn c5_limiter_haelt_bus_decke() {
        let p = params();
        let mut dsp = dsp_neu();
        p.set("bus", 0, "c5", 1.0);
        p.set("bus", 0, "c5_ceiling", -6.0);
        p.set("bus", 0, "c5_threshold", 0.0);
        p.set("strip", 0, "gain", 12.0); // 0.5 * 4 = 2.0 in den Bus
        let mut buf = Buf::new(0.5, 64);
        let mut peak = 0f32;
        for _ in 0..40 {
            let io = buf.io();
            dsp.process(&p, &io, 64);
            peak = peak.max(buf.bus(0).abs());
        }
        let db = crate::dsp::lin_to_db(peak);
        assert!(
            db <= -4.0,
            "Bus-Spitze war {db} dB, soll unter -6 dB liegen"
        );
    }

    #[test]
    fn eq_und_eq_parameter() {
        let p = params();
        let mut dsp = dsp_neu();
        p.set("strip", 0, "eq", 1.0);
        assert!(p.set("strip", 0, "eq0_gain", 12.0));
        assert!(p.set("strip", 0, "eq0_freq", 1000.0));
        assert!(p.set("strip", 0, "eq0_on", 1.0));
        assert!(!p.set("strip", 0, "eq9_gain", 1.0));
        let mut buf = Buf::new(0.1, 64);
        let io = buf.io();
        dsp.process(&p, &io, 64);
        assert!(buf.bus(0).is_finite());
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
    fn gate_regelt_links_und_rechts_gemeinsam() {
        // Links laut, rechts leise (unter der Schwelle): mit gekoppeltem
        // Detektor bleibt das Gate fuer beide offen - der leise Kanal faellt
        // nicht um 60 dB weg, nur weil er allein leise ist.
        let p = params();
        assert!(p.set("strip", 0, "gate", 1.0));
        assert!(p.set("strip", 0, "gate_schwelle", -45.0));
        let mut dsp = dsp_neu();
        let mut buf = Buf::new(0.0, 64);
        buf.set_strip_ch(0, 0, 0.5);
        buf.set_strip_ch(0, 1, 0.001);
        for _ in 0..40 {
            let io = buf.io();
            dsp.process(&p, &io, 64);
        }
        let r = buf.out(0, 1);
        assert!((r - 0.001).abs() < 1e-4, "rechts war {r}, soll 0.001");
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

    // ----- Effektwege, Inserts, Green Mode, Abhoeren -----------------------

    /// Zusatzpuffer fuer die Effekt- und Insert-Ports eines Blocks.
    struct Zusatz {
        fx_out: Vec<Vec<f32>>,
        fx_ret: Vec<Vec<f32>>,
        ins: Vec<Vec<f32>>,
        ret: Vec<Vec<f32>>,
    }

    impl Zusatz {
        fn neu(frames: usize) -> Zusatz {
            Zusatz {
                fx_out: vec![vec![9.0; frames]; 4],
                fx_ret: vec![vec![0.0; frames]; 4],
                ins: vec![vec![9.0; frames]; 2],
                ret: vec![vec![0.0; frames]; 2],
            }
        }

        /// Haengt FX1/FX2 und den Insert von Streifen 0 an das Io.
        fn an(&mut self, io: &mut Io) {
            for ext in 0..2 {
                for ch in 0..2 {
                    io.fx_out[ext][ch] = self.fx_out[ext * 2 + ch].as_mut_ptr();
                    io.fx_ret[ext][ch] = self.fx_ret[ext * 2 + ch].as_ptr();
                }
            }
            for ch in 0..2 {
                io.strip_ins[0][ch] = self.ins[ch].as_mut_ptr();
                io.strip_ret[0][ch] = self.ret[ch].as_ptr();
            }
        }
    }

    #[test]
    fn fx1_send_vor_und_hinter_dem_fader() {
        let p = params();
        let mut dsp = dsp_neu();
        let mut buf = Buf::new(0.5, 64);
        let mut z = Zusatz::neu(64);
        p.set("strip", 0, "gain", -60.0);
        p.set("strip", 0, "send_fx1", 0.0);
        // Hinter dem Fader (Vorgabe): der Fader steht unten, kaum Signal.
        let mut io = buf.io();
        z.an(&mut io);
        dsp.process(&p, &io, 64);
        assert!(z.fx_out[0][10].abs() < 1e-3, "post: {}", z.fx_out[0][10]);
        // FX2 ist aus: der Port fuehrt Stille statt alter Pufferreste.
        assert_eq!(z.fx_out[2][10], 0.0);
        // Vor dem Fader: volles Signal unabhaengig vom Fader.
        p.set("strip", 0, "post_fx1", 0.0);
        dsp.process(&p, &io, 64);
        assert!(
            (z.fx_out[0][10] - 0.5).abs() < 1e-6,
            "pre: {}",
            z.fx_out[0][10]
        );
        // Stumm gilt auch vor dem Fader.
        p.set("strip", 0, "mute", 1.0);
        dsp.process(&p, &io, 64);
        assert_eq!(z.fx_out[0][10], 0.0);
    }

    #[test]
    fn rueckweg_landet_nur_auf_bussen_mit_return() {
        let p = params();
        let mut dsp = dsp_neu();
        let mut buf = Buf::new(0.0, 64);
        let mut z = Zusatz::neu(64);
        z.fx_ret[2].iter_mut().for_each(|x| *x = 0.25); // FX2 links
        let mut io = buf.io();
        z.an(&mut io);
        dsp.process(&p, &io, 64);
        // A1 hat ab Werk alle Returns auf 0 dB, A2 keinen.
        assert!((buf.out(0, 0) - 0.25).abs() < 1e-6, "A1 {}", buf.out(0, 0));
        assert_eq!(buf.out(1, 0), 0.0);
        p.set("bus", 1, "ret_fx2", -6.0);
        p.set("bus", 0, "ret_fx2", -60.0);
        dsp.process(&p, &io, 64);
        assert_eq!(buf.out(0, 0), 0.0);
        assert!((buf.out(1, 0) - 0.25 * db_to_lin(-6.0)).abs() < 1e-5);
    }

    #[test]
    fn hall_send_klingt_auf_a1() {
        let p = params();
        let mut dsp = dsp_neu();
        // Ein Impuls auf Streifen 0, der nur in den Hall geht (Routing aus).
        p.set("strip", 0, "send0", 0.0);
        p.set("strip", 0, "send_r", 0.0);
        let mut energie = 0.0f32;
        for block in 0..200 {
            let mut buf = Buf::new(0.0, 64);
            if block == 0 {
                buf.ins[0][0] = 1.0;
                buf.ins[1][0] = 1.0;
            }
            let io = buf.io();
            dsp.process(&p, &io, 64);
            energie += buf.outs[0].iter().map(|x| x * x).sum::<f32>();
        }
        assert!(energie > 1e-4, "Hall auf A1: {energie}");
        // Ohne Return auf A1 bleibt A1 still.
        let p = params();
        let mut dsp = dsp_neu();
        p.set("strip", 0, "send0", 0.0);
        p.set("strip", 0, "send_r", 0.0);
        p.set("bus", 0, "ret_r", -60.0);
        let mut buf = Buf::new(0.5, 64);
        let io = buf.io();
        for _ in 0..50 {
            dsp.process(&p, &io, 64);
        }
        assert_eq!(buf.bus(0), 0.0);
    }

    #[test]
    fn insert_wirkt_erst_mit_verbundenem_rueckweg() {
        let p = params();
        let mut dsp = dsp_neu();
        let mut buf = Buf::new(0.5, 64);
        let mut z = Zusatz::neu(64);
        z.ret[0].iter_mut().for_each(|x| *x = 0.1);
        z.ret[1].iter_mut().for_each(|x| *x = 0.1);
        let mut io = buf.io();
        z.an(&mut io);
        // Eingeschaltet, aber nicht verbunden: Durchgang, Insert-Ausgang still.
        p.set("strip", 0, "insert", 1.0);
        dsp.process(&p, &io, 64);
        assert!((buf.bus(0) - 0.5).abs() < 1e-6);
        assert_eq!(z.ins[0][5], 0.0);
        // Verbunden: hinaus geht das Signal, herein kommt der Rueckweg.
        p.strips[0].ins_bereit.store(true, Relaxed);
        dsp.process(&p, &io, 64);
        assert!((z.ins[0][5] - 0.5).abs() < 1e-6, "hinaus {}", z.ins[0][5]);
        assert!((buf.bus(0) - 0.1).abs() < 1e-6, "herein {}", buf.bus(0));
        // Ausgeschaltet: wieder Durchgang.
        p.set("strip", 0, "insert", 0.0);
        dsp.process(&p, &io, 64);
        assert!((buf.bus(0) - 0.5).abs() < 1e-6);
    }

    #[test]
    fn green_mode_umgeht_die_kette() {
        let p = params();
        let mut dsp = dsp_neu();
        // Ein harter Compressor und ein stark gekippter Klang …
        p.set("strip", 0, "comp", 1.0);
        p.set("strip", 0, "comp_threshold", -40.0);
        p.set("strip", 0, "comp_ratio", 20.0);
        p.set("strip", 0, "karaoke", 2.0);
        let mut buf = Buf::new(0.5, 64);
        buf.set_strip_ch(0, 1, 0.0);
        let io = buf.io();
        for _ in 0..20 {
            dsp.process(&p, &io, 64);
        }
        assert!(buf.out(0, 0) < 0.3, "Kette wirkt: {}", buf.out(0, 0));
        // … und im Green Mode zaehlt nur der Fader.
        p.set("strip", 0, "green", 1.0);
        dsp.process(&p, &io, 64);
        assert!((buf.out(0, 0) - 0.5).abs() < 1e-6, "{}", buf.out(0, 0));
        assert_eq!(buf.out(0, 1), 0.0);
    }

    #[test]
    fn a1_hoert_den_abgehoerten_bus() {
        let p = params();
        let mut dsp = dsp_neu();
        let mut buf = Buf::new(0.0, 64);
        buf.set_strip(1, 0.3);
        // Streifen 2 nur auf B1 (Bus 5), Streifen 1 (leer) auf A1.
        p.set("strip", 1, "send0", 0.0);
        p.set("strip", 1, "send5", 1.0);
        let io = buf.io();
        dsp.process(&p, &io, 64);
        assert_eq!(buf.bus(0), 0.0);
        assert!((buf.bus(5) - 0.3).abs() < 1e-6);
        p.set("bus", 5, "monitor", 1.0);
        dsp.process(&p, &io, 64);
        assert!(
            (buf.bus(0) - 0.3).abs() < 1e-6,
            "A1 hoert B1: {}",
            buf.bus(0)
        );
        assert!((buf.bus(5) - 0.3).abs() < 1e-6, "B1 bleibt");
        p.set("bus", 5, "monitor", 0.0);
        dsp.process(&p, &io, 64);
        assert_eq!(buf.bus(0), 0.0);
    }

    #[test]
    fn sel_und_monitor_gelten_fuer_hoechstens_einen_bus() {
        let p = params();
        assert_eq!(p.sel_bus(), 0);
        assert!(p.set("bus", 3, "sel", 1.0));
        assert_eq!(p.sel_bus(), 3);
        assert_eq!(p.buses.iter().filter(|b| b.sel.load(Relaxed)).count(), 1);
        // Abwaehlen geht nicht: ein Bus ist immer gewaehlt.
        assert!(p.set("bus", 3, "sel", 0.0));
        assert_eq!(p.sel_bus(), 3);
        assert_eq!(p.monitor_bus(), None);
        p.set("bus", 6, "monitor", 1.0);
        p.set("bus", 7, "monitor", 1.0);
        assert_eq!(p.monitor_bus(), Some(7));
        assert_eq!(
            p.buses.iter().filter(|b| b.monitor.load(Relaxed)).count(),
            1
        );
        p.set("bus", 7, "monitor", 0.0);
        assert_eq!(p.monitor_bus(), None);
        assert!(!p.set("bus", 99, "sel", 1.0));
    }

    #[test]
    fn neue_parameter_werden_geklemmt() {
        let p = params();
        assert!(p.set("strip", 0, "send_d", 40.0));
        assert_eq!(fget(&p.strips[0].fx_send[1]), effekte::MAX_DB);
        assert!(p.set("strip", 0, "farbe", -3.0));
        assert_eq!(fget(&p.strips[0].farbe), -1.0);
        assert!(p.set("strip", 0, "karaoke", 7.0));
        assert_eq!(p.strips[0].karaoke.load(Relaxed), effekte::KARAOKE_MAX);
        assert!(p.set("bus", 2, "ret_fx1", -99.0));
        assert_eq!(fget(&p.buses[2].fx_ret[2]), effekte::AUS_DB);
        assert!(p.set("fx", 0, "echo_rueck", 2.0));
        assert_eq!(fget(&p.fx.echo_rueck), 0.9);
        assert!(!p.set("fx", 1, "echo_rueck", 0.5));
        assert!(!p.set("strip", 0, "send_x", 0.0));
        assert!(!p.set("bus", 0, "ret_q", 0.0));
        // Die alten Bus-Sends bleiben erreichbar.
        assert!(p.set("strip", 0, "send3", 1.0));
    }

    #[test]
    fn preset_rundlauf_der_neuen_felder() {
        let p = Arc::new(params());
        p.set("strip", 2, "send_r", -6.0);
        p.set("strip", 2, "post_d", 0.0);
        p.set("strip", 2, "farbe", 0.5);
        p.set("strip", 2, "karaoke", 1.0);
        p.set("strip", 2, "muc", 1.0);
        p.set("strip", 2, "green", 1.0);
        p.set("strip", 2, "insert", 1.0);
        p.set("strip", 2, "prg_c", 3.0);
        p.set("bus", 4, "ret_d", -3.0);
        p.set("bus", 4, "sel", 1.0);
        p.set("bus", 6, "monitor", 1.0);
        p.set("bus", 1, "vaio", 1.0);
        p.set("bus", 1, "green", 1.0);
        p.set("bus", 1, "mode", 3.0);
        p.set("bus", 1, "c5", 1.0);
        p.set("fx", 0, "hall_raum", 0.8);
        let bild = p.to_json();
        let q = Arc::new(params());
        crate::http::apply_preset(&q, &bild);
        assert_eq!(q.to_json().to_json(), bild.to_json());
        assert_eq!(
            crate::programme::aktuell(&q, 2, crate::programme::Modul::Comp),
            3
        );
        assert_eq!(q.sel_bus(), 4);
        assert_eq!(q.monitor_bus(), Some(6));
    }
}
