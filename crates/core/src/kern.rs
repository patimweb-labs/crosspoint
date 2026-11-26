//! Der Linux-Kern als Schnittstelle fürs Fenster (ADR 0012, Stufe 2/3).
//!
//! Hier wird das, was bisher im Fenster stand (PipeWire-Graph lesen, Zuordnungen
//! schreiben, Presets, Autostart, Engine-Start), zu einer Implementierung der
//! Traits aus `crosspoint-schnittstelle`. Das Fenster kennt danach nur noch die
//! Traits – ein Kern für Windows oder macOS füllt dieselben Methoden.

use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering::Relaxed;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crosspoint_modell::json::Value;
use crosspoint_modell::params::Params;
use crosspoint_modell::pegel::Pegelbank;
use crosspoint_schnittstelle::ablage::Ablage;
use crosspoint_schnittstelle::betrieb::{Betrieb, Cc, MidiCc};
use crosspoint_schnittstelle::geraete::{Bestand, Eintrag, Geraete, Lautstaerke, Ziel, Zuordner};
use crosspoint_schnittstelle::haende::{Band, Makros, Netz};
use crosspoint_schnittstelle::Schnittstelle;

use crate::devices::{self, DeviceMap};
use crate::inputs;
use crate::lauf::Lauf;
use crate::macros_io::MacroControl;
use crate::osc::CcKanal;
use crate::pw::{Graph, Lautstaerke as PwLautstaerke, PipeWire, Zugriff};
use crate::tape_io::TapeControl;
use crate::vban_io::{VbanControl, VbanRecvControl};
use crate::BUS_NAMES;

/// Wie lange ohne Blockzuwachs der Kern als „steht“ gilt.
const LEBT_NOCH: Duration = Duration::from_secs(1);

// --------------------------------------------------------------- Geräte ----

/// Der PipeWire-Weg für die Geräteliste – die Linux-Fassung von [`Geraete`].
#[derive(Default)]
pub struct LinuxGeraete;

impl Geraete for LinuxGeraete {
    fn bestand(&self) -> Bestand {
        let pw = PipeWire::new();
        let streifen = inputs::mapping().to_vec();
        let map = DeviceMap::load_default();
        let busse: Vec<(String, Option<String>)> = BUS_NAMES[..5]
            .iter()
            .map(|bus| (bus.to_string(), map.get(bus).map(str::to_string)))
            .collect();
        bestand_aus(Zugriff::graph(&pw), &streifen, &busse)
    }

    fn zuordner(&self) -> &dyn Zuordner {
        self
    }

    fn aktuell(&self, ziel: Ziel<'_>) -> Option<String> {
        match ziel {
            Ziel::Streifen(i) => inputs::mapping()
                .get(i.saturating_sub(1))
                .cloned()
                .flatten(),
            Ziel::Bus(b) => DeviceMap::load_default().get(b).map(str::to_string),
        }
    }
}

impl Zuordner for LinuxGeraete {
    fn streifen(&self, strip: usize, quelle: Option<&str>) -> Result<(), String> {
        inputs::assign(&PipeWire::new(), strip, quelle)
    }

    fn bus(&self, bus: &str, senke: Option<&str>) -> Result<(), String> {
        let mut map = DeviceMap::load_default();
        match senke.map(str::trim).filter(|s| !s.is_empty()) {
            Some(sink) => map.assign(&PipeWire::new(), bus, sink),
            None => map.clear(&PipeWire::new(), bus),
        }
    }

    fn lautstaerke(
        &self,
        senke: &str,
        volumen: Option<f32>,
        stumm: Option<bool>,
    ) -> Result<(), String> {
        // Derselbe Weg wie die Zuordnung: ein Auftrag an den Registry-Thread,
        // kein `wpctl` von außen.
        Zugriff::lautstaerke_setzen(&PipeWire::new(), senke, volumen, stumm)
    }

    fn echo(&self, geraete: &[String]) -> Result<(), String> {
        match crate::echo::setzen(&PipeWire::new(), geraete) {
            Ok(fehler) if fehler.is_empty() => Ok(()),
            Ok(fehler) => Err(fehler.join("; ")),
            Err(e) => Err(e),
        }
    }
}

/// Taugt dieses Gerät als Echo-Referenz?
///
/// Eigene Geräte (Crosspoint selbst) sind keine Referenz, und was in einen
/// anderen Raum spielt (Netzwerk, AirPlay), taugt nicht als Referenz für den
/// Schall am Mikrofon.
fn ist_echo_kandidat(name: &str) -> bool {
    if inputs::ist_eigen(name) {
        return false;
    }
    let klein = name.to_ascii_lowercase();
    !NETZ_ZIELE.iter().any(|z| klein.contains(z))
}

/// Netzwerk- und AirPlay-Ziele: was dort hinausgeht, spielt in einem anderen
/// Raum und taugt nicht als Echo-Referenz.
const NETZ_ZIELE: [&str; 8] = [
    "airplay",
    "raop",
    "module-rtp",
    "vban",
    "crosspoint",
    "network",
    "http-stream",
    "rtsp",
];

/// Kürzt einen Gerätenamen für Anzeige und Meldung.
fn kurzname(name: &str) -> String {
    const MAX: usize = 28;
    let name = name.trim();
    if name.chars().count() <= MAX {
        return name.to_string();
    }
    let kurz: String = name.chars().take(MAX - 1).collect();
    format!("{}…", kurz.trim_end())
}

/// Bestand aus einem Graphen lesen (rein, damit ohne PipeWire prüfbar).
pub fn bestand_aus(
    graph: Result<Graph, String>,
    streifen: &[Option<String>],
    busse: &[(String, Option<String>)],
) -> Bestand {
    let mut b = Bestand {
        streifen: streifen.to_vec(),
        busse: busse.to_vec(),
        ..Bestand::default()
    };
    let graph = match graph {
        Ok(g) => g,
        Err(e) => {
            b.fehler = Some(e);
            return b;
        }
    };

    b.quellen = inputs::quellen(&graph, true)
        .into_iter()
        .map(|q| Eintrag {
            echo_kandidat: false,
            name: q.name,
            anzeige: q.desc,
            fehlt: false,
        })
        .collect();
    b.senken = devices::bus_ziele(&graph, false)
        .into_iter()
        .map(|s| Eintrag {
            echo_kandidat: ist_echo_kandidat(&s.name),
            name: s.name,
            anzeige: s.desc,
            fehlt: false,
        })
        .collect();

    b.echo = crate::echo::auswahl();

    // Gemerkte, aber fehlende Geräte: in die Liste aufnehmen und markieren.
    b.streifen_fehlt = streifen
        .iter()
        .map(|q| q.as_ref().is_some_and(|n| graph.knoten(n).is_none()))
        .collect();
    b.busse_fehlt = busse
        .iter()
        .map(|(_, s)| s.as_ref().is_some_and(|n| graph.knoten(n).is_none()))
        .collect();

    for (i, quelle) in streifen.iter().enumerate() {
        let (Some(name), Some(true)) = (quelle, b.streifen_fehlt.get(i).copied()) else {
            continue;
        };
        b.quellen.push(Eintrag {
            name: name.clone(),
            anzeige: kurzname(name),
            fehlt: true,
            echo_kandidat: false,
        });
    }
    for (i, (_, senke)) in busse.iter().enumerate() {
        let (Some(name), Some(true)) = (senke, b.busse_fehlt.get(i).copied()) else {
            continue;
        };
        b.senken.push(Eintrag {
            name: name.clone(),
            anzeige: kurzname(name),
            fehlt: true,
            echo_kandidat: ist_echo_kandidat(name),
        });
    }

    // Gerätelautstärke je Bus (nur bei vorhandenem Gerät) – neutral übertragen.
    b.busse_lautstaerke = busse
        .iter()
        .map(|(_, s)| {
            s.as_ref()
                .and_then(|n| graph.lautstaerke(n))
                .map(|l: PwLautstaerke| Lautstaerke {
                    volumen: l.volumen,
                    stumm: l.stumm,
                })
        })
        .collect();

    for (i, (bus, senke)) in busse.iter().enumerate() {
        if b.busse_fehlt.get(i).copied().unwrap_or(false) {
            if let Some(name) = senke {
                b.fehlend.push(format!("Bus {bus} ({})", kurzname(name)));
            }
        }
    }
    for (i, quelle) in streifen.iter().enumerate() {
        if b.streifen_fehlt.get(i).copied().unwrap_or(false) {
            if let Some(name) = quelle {
                b.fehlend
                    .push(format!("Streifen {} ({})", i + 1, kurzname(name)));
            }
        }
    }
    b
}

// ---------------------------------------------------------------- Ablage ----

/// Dateien: Presets und Autostart (Linux: `.desktop` in `~/.config/autostart`).
#[derive(Default)]
pub struct LinuxAblage;

impl Ablage for LinuxAblage {
    fn preset_laden(&self, params: &Arc<Params>, pfad: &Path) -> bool {
        crate::restore::preset_laden(params, pfad)
    }

    fn preset_speichern(&self, params: &Arc<Params>, pfad: &Path) -> Result<(), String> {
        crate::restore::preset_speichern(params, pfad)
    }

    fn autostart(&self) -> bool {
        crate::autostart::enabled()
    }

    fn autostart_setzen(&self, an: bool) -> Result<(), String> {
        // `autostart::set` meldet den Pfad des Eintrags; den braucht das
        // Fenster nicht, nur ob es geklappt hat.
        crate::autostart::set(an).map(|_| ())
    }
}

// --------------------------------------------------------------- Betrieb ----

/// Der laufende Linux-Kern (Engine, Adapter, Audio-Thread).
pub struct LinuxKern {
    lauf: Arc<Lauf>,
    /// Letzter Blockzuwachs der Pegelbank: daran hängt „läuft noch“.
    stand: Mutex<(u64, Instant)>,
    geraete: LinuxGeraete,
    ablage: LinuxAblage,
    netz: VbanHandles,
}

impl LinuxKern {
    /// Den laufenden Kern einwickeln. Das Fenster sieht danach nur die Traits.
    pub fn neu(lauf: Arc<Lauf>) -> LinuxKern {
        let netz = VbanHandles {
            send: Arc::clone(&lauf.vban),
            recv: Arc::clone(&lauf.vban_recv),
        };
        LinuxKern {
            lauf,
            stand: Mutex::new((0, Instant::now())),
            geraete: LinuxGeraete,
            ablage: LinuxAblage,
            netz,
        }
    }

    /// Der rohe Lauf – nur für den, der den Kern gebaut hat (Binary, Tests).
    pub fn lauf(&self) -> &Arc<Lauf> {
        &self.lauf
    }
}

impl Betrieb for LinuxKern {
    fn laeuft(&self) -> bool {
        if self.lauf.beendet() {
            return false;
        }
        // Läuft heißt: der Audio-Callback zählt Blöcke.
        let b = self.lauf.pegel.bloecke();
        let Ok(mut s) = self.stand.lock() else {
            return false;
        };
        if b != s.0 {
            *s = (b, Instant::now());
        }
        s.1.elapsed() < LEBT_NOCH
    }

    fn beenden(&self) {
        self.lauf.abschalten();
    }

    fn fenster_gewuenscht(&self) -> bool {
        crate::http::fenster_gewuenscht()
    }

    fn echo_auswahl(&self) -> Vec<String> {
        crate::echo::auswahl()
    }
}

impl Schnittstelle for LinuxKern {
    fn params(&self) -> &Arc<Params> {
        &self.lauf.params
    }

    fn pegel(&self) -> &Arc<Pegelbank> {
        &self.lauf.pegel
    }

    fn sample_rate(&self) -> u32 {
        self.lauf.sample_rate
    }

    fn tape_ordner(&self) -> PathBuf {
        self.lauf.tape_ordner.clone()
    }

    fn preset_ordner(&self) -> PathBuf {
        self.lauf
            .preset
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from("."))
    }

    fn preset_pfad(&self) -> PathBuf {
        self.lauf.preset.clone()
    }

    fn midi_pfad(&self) -> PathBuf {
        crate::pfade::datei("midimonster.cfg")
    }

    fn geraete(&self) -> &dyn Geraete {
        &self.geraete
    }

    fn ablage(&self) -> &dyn Ablage {
        &self.ablage
    }

    fn betrieb(&self) -> &dyn Betrieb {
        self
    }

    fn band(&self) -> Option<&dyn Band> {
        Some(self.lauf.tape.as_ref())
    }

    fn makros(&self) -> Option<&dyn Makros> {
        Some(self.lauf.makros.as_ref())
    }

    fn netz(&self) -> Option<&dyn Netz> {
        Some(&self.netz)
    }

    fn cc(&self) -> Option<&dyn MidiCc> {
        Some(crate::osc::cc_kanal())
    }
}

// ------------------------------------------------------------ Bedienhände ----

impl Band for TapeControl {
    fn aufnahme(&self, an: bool, status: &str) {
        self.recording.store(an, Relaxed);
        self.set_status(status);
    }

    fn status(&self) -> String {
        TapeControl::status(self)
    }
}

impl Makros for MacroControl {
    fn druck(&self, index: usize) -> Result<(), String> {
        self.press(index).and_then(|_| self.release(index))
    }
}

/// VBAN senden und empfangen als ein Griff.
pub struct VbanHandles {
    send: Arc<VbanControl>,
    recv: Arc<VbanRecvControl>,
}

impl Netz for VbanHandles {
    fn stand(&self, sample_rate: u32) -> Value {
        let text = crate::vban_io::konfig_json(&self.send, &self.recv, sample_rate);
        crate::json::parse(&text).unwrap_or(Value::Null)
    }

    fn anwenden(&self, json: &Value) -> Result<(), String> {
        crate::vban_io::wende_konfig_an(&self.send, &self.recv, json)
    }
}

impl MidiCc for CcKanal {
    fn abholen(&self) -> Vec<Cc> {
        CcKanal::abholen(self)
            .into_iter()
            .map(|c| Cc {
                kanal: c.kanal,
                cc: c.cc,
                wert: c.wert,
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kurzname_kuerzt_nur_wenn_noetig() {
        assert_eq!(kurzname("Mikro"), "Mikro");
        let lang = "Navi 48 HDMI/DP Audio Controller Digital Stereo (HDMI 2)";
        let k = kurzname(lang);
        assert!(k.chars().count() <= 28, "{k}");
        assert!(k.ends_with('…'), "{k}");
    }

    #[test]
    fn netzziele_sind_keine_echo_kandidaten() {
        assert!(ist_echo_kandidat("Sennheiser Profile Analog Stereo"));
        assert!(!ist_echo_kandidat("AirPlay Sink"));
        assert!(!ist_echo_kandidat("CROSSPOINT-Output-B1"));
    }
}
