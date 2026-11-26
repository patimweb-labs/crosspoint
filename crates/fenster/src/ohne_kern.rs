//! Ein Kern ohne Plattform – für Oberflächentests und Vorschauen.
//!
//! Die Oberflächentests sollen ohne PipeWire, JACK und Prozessstart laufen
//! (AGENTS.md §4). Dafür braucht das Fenster einen Kern, der die Naht
//! ([`Schnittstelle`]) erfüllt, aber nichts tut: Gerätezuordnungen werden
//! gemerkt, Presets gibt es keine, die Engine „läuft“.
//!
//! Das ist **kein** Ersatz für einen Kern im Betrieb – nur ein Prüfstand.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use crosspoint_modell::json::Value;
use crosspoint_modell::params::Params;
use crosspoint_modell::pegel::Pegelbank;
use crosspoint_schnittstelle::ablage::Ablage;
use crosspoint_schnittstelle::betrieb::{Betrieb, Cc, MidiCc};
use crosspoint_schnittstelle::geraete::{Bestand, Geraete, Ziel, Zuordner};
use crosspoint_schnittstelle::haende::Netz;
use crosspoint_schnittstelle::Schnittstelle;

/// Ein Kern, der nichts tut, aber alles beantwortet.
pub struct OhneKern {
    params: Arc<Params>,
    pegel: Arc<Pegelbank>,
    /// Was der Prüfstand an Zuordnungen mitgeschrieben hat.
    pub zuordnungen: Mutex<Vec<String>>,
    /// Gerätebestand, den `bestand()` zurückgibt.
    pub bestand: Mutex<Bestand>,
    /// „Engine läuft“ – Tests dürfen das umschalten.
    pub laeuft: std::sync::atomic::AtomicBool,
    /// Meldungen von `beenden()` und die Echo-Auswahl.
    pub echo: Mutex<Vec<String>>,
}

impl OhneKern {
    pub fn neu(params: Arc<Params>) -> OhneKern {
        OhneKern {
            params,
            pegel: Arc::new(Pegelbank::neu()),
            zuordnungen: Mutex::new(Vec::new()),
            bestand: Mutex::new(Bestand::default()),
            laeuft: std::sync::atomic::AtomicBool::new(true),
            echo: Mutex::new(Vec::new()),
        }
    }

    /// Die Pegelbank des Prüfstands (Tests schreiben Werte hinein).
    pub fn pegel(&self) -> &Arc<Pegelbank> {
        &self.pegel
    }
}

impl Geraete for OhneKern {
    fn bestand(&self) -> Bestand {
        self.bestand.lock().map(|b| b.clone()).unwrap_or_default()
    }

    fn zuordner(&self) -> &dyn Zuordner {
        self
    }

    fn aktuell(&self, _ziel: Ziel<'_>) -> Option<String> {
        None
    }
}

impl Zuordner for OhneKern {
    fn streifen(&self, strip: usize, quelle: Option<&str>) -> Result<(), String> {
        self.merken(format!("streifen {strip} {quelle:?}"));
        Ok(())
    }

    fn bus(&self, bus: &str, senke: Option<&str>) -> Result<(), String> {
        self.merken(format!("bus {bus} {senke:?}"));
        Ok(())
    }

    fn lautstaerke(
        &self,
        senke: &str,
        volumen: Option<f32>,
        stumm: Option<bool>,
    ) -> Result<(), String> {
        self.merken(format!("lautstaerke {senke} {volumen:?} {stumm:?}"));
        Ok(())
    }

    fn echo(&self, geraete: &[String]) -> Result<(), String> {
        self.merken(format!("echo {geraete:?}"));
        if let Ok(mut e) = self.echo.lock() {
            *e = geraete.to_vec();
        }
        Ok(())
    }
}

impl OhneKern {
    fn merken(&self, eintrag: String) {
        if let Ok(mut g) = self.zuordnungen.lock() {
            g.push(eintrag);
        }
    }
}

impl Ablage for OhneKern {
    /// Der Prüfstand lädt keine Presets: das Anwenden ist Sache des Kerns
    /// (Tests, die das prüfen wollen, nehmen `crosspoint::restore`).
    fn preset_laden(&self, _params: &Arc<Params>, _pfad: &Path) -> bool {
        false
    }

    /// Schreiben kann er: das Preset ist das Parameterbild des Modells, dafür
    /// braucht es keinen Kern.
    fn preset_speichern(&self, params: &Arc<Params>, pfad: &Path) -> Result<(), String> {
        if let Some(dir) = pfad.parent() {
            std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        }
        std::fs::write(pfad, params.to_json().to_json())
            .map_err(|e| format!("{}: {e}", pfad.display()))
    }

    fn autostart(&self) -> bool {
        false
    }

    fn autostart_setzen(&self, _an: bool) -> Result<(), String> {
        Ok(())
    }
}

impl Betrieb for OhneKern {
    fn laeuft(&self) -> bool {
        self.laeuft.load(std::sync::atomic::Ordering::Relaxed)
    }

    fn beenden(&self) {
        self.laeuft
            .store(false, std::sync::atomic::Ordering::Relaxed);
    }

    fn fenster_gewuenscht(&self) -> bool {
        false
    }

    fn echo_auswahl(&self) -> Vec<String> {
        self.echo.lock().map(|e| e.clone()).unwrap_or_default()
    }
}

impl MidiCc for OhneKern {
    fn abholen(&self) -> Vec<Cc> {
        Vec::new()
    }
}

impl Netz for OhneKern {
    fn stand(&self, _sample_rate: u32) -> Value {
        Value::Null
    }

    fn anwenden(&self, _json: &Value) -> Result<(), String> {
        Ok(())
    }
}

impl Schnittstelle for OhneKern {
    fn params(&self) -> &Arc<Params> {
        &self.params
    }

    fn pegel(&self) -> &Arc<Pegelbank> {
        &self.pegel
    }

    fn sample_rate(&self) -> u32 {
        48_000
    }

    fn tape_ordner(&self) -> PathBuf {
        PathBuf::from("/tmp/crosspoint-band")
    }

    fn preset_ordner(&self) -> PathBuf {
        std::env::temp_dir().join("crosspoint-presets")
    }

    fn preset_pfad(&self) -> PathBuf {
        std::env::temp_dir()
            .join("crosspoint-presets")
            .join("letztes.json")
    }

    fn midi_pfad(&self) -> PathBuf {
        std::env::temp_dir().join("crosspoint-midimonster.cfg")
    }

    fn geraete(&self) -> &dyn Geraete {
        self
    }

    fn ablage(&self) -> &dyn Ablage {
        self
    }

    fn betrieb(&self) -> &dyn Betrieb {
        self
    }

    fn netz(&self) -> Option<&dyn Netz> {
        Some(self)
    }

    fn cc(&self) -> Option<&dyn MidiCc> {
        Some(self)
    }
}
