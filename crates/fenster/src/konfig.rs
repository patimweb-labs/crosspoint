//! Einstellungen des Fensters: `~/.config/crosspoint/fenster.json`.
//!
//! Nur was das Fenster betrifft (Theme, Master-Bus, Makros). Der
//! Mischzustand steht im Preset/Autosave des Kerns. Unlesbare oder fehlende
//! Dateien ergeben die Vorgaben – ein Tippfehler startet nie ein leeres Fenster.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::modell::Oberflaeche;
use crate::thema::Thema;

/// Konfigurationsordner (`$XDG_CONFIG_HOME/crosspoint`, sonst `~/.config/crosspoint`).
pub fn ordner() -> PathBuf {
    if let Some(xdg) = std::env::var_os("XDG_CONFIG_HOME").filter(|x| !x.is_empty()) {
        PathBuf::from(xdg).join("crosspoint")
    } else if let Some(home) = std::env::var_os("HOME") {
        PathBuf::from(home).join(".config").join("crosspoint")
    } else {
        PathBuf::from(".crosspoint")
    }
}

pub fn pfad() -> PathBuf {
    ordner().join("fenster.json")
}

/// Ordner der Presets (wie bisher `~/.config/crosspoint/presets`).
pub fn preset_ordner() -> PathBuf {
    ordner().join("presets")
}

/// Inhalt der Datei.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Datei {
    pub thema: String,
    pub master_bus: usize,
    pub makros_eingeklappt: bool,
    pub makro_anzahl: usize,
}

impl Default for Datei {
    fn default() -> Self {
        Datei::aus(&Oberflaeche::default())
    }
}

impl Datei {
    pub fn aus(o: &Oberflaeche) -> Datei {
        Datei {
            thema: o.thema.name().to_string(),
            master_bus: o.master_bus,
            makros_eingeklappt: o.makros_eingeklappt,
            makro_anzahl: o.makro_anzahl,
        }
    }

    /// Werte ins Modell übernehmen (geklemmt).
    pub fn anwenden(&self, o: &mut Oberflaeche) {
        o.thema = Thema::aus_name(&self.thema);
        o.master_bus = self.master_bus.min(crosspoint::BUSES - 1);
        o.makros_eingeklappt = self.makros_eingeklappt;
        o.makro_anzahl = crate::logik::makro_anzahl_pruefen(self.makro_anzahl);
    }

    pub fn laden(pfad: &Path) -> Datei {
        std::fs::read_to_string(pfad)
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default()
    }

    pub fn speichern(&self, pfad: &Path) -> Result<(), String> {
        if let Some(d) = pfad.parent() {
            std::fs::create_dir_all(d).map_err(|e| e.to_string())?;
        }
        let text = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        std::fs::write(pfad, text).map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rundreise_und_kaputte_datei() {
        let dir = std::env::temp_dir().join(format!("cp-fenster-{}", std::process::id()));
        let pfad = dir.join("fenster.json");
        let mut o = Oberflaeche {
            thema: Thema::Mono,
            master_bus: 6,
            ..Oberflaeche::default()
        };
        Datei::aus(&o).speichern(&pfad).unwrap();
        let mut neu = Oberflaeche::default();
        Datei::laden(&pfad).anwenden(&mut neu);
        assert_eq!(neu.thema, Thema::Mono);
        assert_eq!(neu.master_bus, 6);

        std::fs::write(&pfad, "{ kaputt").unwrap();
        Datei::laden(&pfad).anwenden(&mut o);
        assert_eq!(o.thema, Thema::Dunkel);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn werte_werden_geklemmt() {
        let mut o = Oberflaeche::default();
        Datei {
            master_bus: 99,
            makro_anzahl: 7,
            ..Datei::default()
        }
        .anwenden(&mut o);
        assert_eq!(o.master_bus, crosspoint::BUSES - 1);
        assert_eq!(o.makro_anzahl, 5);
    }
}
