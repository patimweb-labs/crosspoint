//! Fertige Programme für Compressor, Gate und Rauschunterdrückung
//! (`prg_c`, `prg_g`, `prg_d` im Original).
//!
//! Ein Programm ist nur ein Satz Parameterwerte: Wählen schreibt sie über
//! [`Params::set`] und merkt die Nummer im Streifen. Wer danach einen
//! einzelnen Wert ändert, landet wieder bei „eigene Einstellung“ (0) – die
//! Anzeige zeigt nie ein Programm an, das nicht mehr gilt.

use std::sync::atomic::Ordering::Relaxed;

use crate::engine::Params;

/// Die drei Module mit Programmen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Modul {
    Comp = 0,
    Gate = 1,
    Rauschen = 2,
}

impl Modul {
    pub const ALLE: [Modul; 3] = [Modul::Comp, Modul::Gate, Modul::Rauschen];

    /// Parametername (`prg_c`, `prg_g`, `prg_d`).
    pub fn param(self) -> &'static str {
        match self {
            Modul::Comp => "prg_c",
            Modul::Gate => "prg_g",
            Modul::Rauschen => "prg_d",
        }
    }

    pub fn aus_param(name: &str) -> Option<Modul> {
        Modul::ALLE.into_iter().find(|m| m.param() == name)
    }

    /// Zu welchem Modul gehört ein einzelner Parameter? Ändert ihn jemand von
    /// Hand, gilt das Programm nicht mehr.
    pub fn von_wert(param: &str) -> Option<Modul> {
        if param == "comp" || param.starts_with("comp_") {
            Some(Modul::Comp)
        } else if param == "gate" || param.starts_with("gate_") {
            Some(Modul::Gate)
        } else if param == "nr" || param.starts_with("nr_") {
            Some(Modul::Rauschen)
        } else {
            None
        }
    }

    pub fn programme(self) -> &'static [Programm] {
        match self {
            Modul::Comp => COMP,
            Modul::Gate => GATE,
            Modul::Rauschen => RAUSCHEN,
        }
    }
}

/// Ein Programm: Name und die Werte, die es setzt.
pub struct Programm {
    pub name: &'static str,
    pub werte: &'static [(&'static str, f32)],
}

const COMP: &[Programm] = &[
    Programm {
        name: "Sprache sanft",
        werte: &[
            ("comp", 1.0),
            ("comp_threshold", -20.0),
            ("comp_ratio", 2.0),
            ("comp_attack", 15.0),
            ("comp_release", 150.0),
            ("comp_knee", 6.0),
            ("comp_gain_in", 0.0),
            ("comp_gain_out", 2.0),
        ],
    },
    Programm {
        name: "Sprache kräftig",
        werte: &[
            ("comp", 1.0),
            ("comp_threshold", -24.0),
            ("comp_ratio", 4.0),
            ("comp_attack", 5.0),
            ("comp_release", 120.0),
            ("comp_knee", 3.0),
            ("comp_gain_in", 0.0),
            ("comp_gain_out", 5.0),
        ],
    },
    Programm {
        name: "Radio",
        werte: &[
            ("comp", 1.0),
            ("comp_threshold", -28.0),
            ("comp_ratio", 6.0),
            ("comp_attack", 3.0),
            ("comp_release", 80.0),
            ("comp_knee", 6.0),
            ("comp_gain_in", 0.0),
            ("comp_gain_out", 8.0),
        ],
    },
    Programm {
        name: "Musik glätten",
        werte: &[
            ("comp", 1.0),
            ("comp_threshold", -16.0),
            ("comp_ratio", 1.8),
            ("comp_attack", 30.0),
            ("comp_release", 300.0),
            ("comp_knee", 9.0),
            ("comp_gain_in", 0.0),
            ("comp_gain_out", 1.0),
        ],
    },
    Programm {
        name: "Begrenzer",
        werte: &[
            ("comp", 1.0),
            ("comp_threshold", -6.0),
            ("comp_ratio", 20.0),
            ("comp_attack", 1.0),
            ("comp_release", 60.0),
            ("comp_knee", 0.0),
            ("comp_gain_in", 0.0),
            ("comp_gain_out", 0.0),
        ],
    },
];

const GATE: &[Programm] = &[
    Programm {
        name: "Ruhiger Raum",
        werte: &[
            ("gate", 1.0),
            ("gate_schwelle", -50.0),
            ("gate_tiefe", 20.0),
            ("gate_attack", 2.0),
            ("gate_hold", 150.0),
            ("gate_release", 250.0),
        ],
    },
    Programm {
        name: "Normaler Raum",
        werte: &[
            ("gate", 1.0),
            ("gate_schwelle", -42.0),
            ("gate_tiefe", 30.0),
            ("gate_attack", 1.0),
            ("gate_hold", 120.0),
            ("gate_release", 200.0),
        ],
    },
    Programm {
        name: "Lauter Raum",
        werte: &[
            ("gate", 1.0),
            ("gate_schwelle", -34.0),
            ("gate_tiefe", 40.0),
            ("gate_attack", 0.5),
            ("gate_hold", 80.0),
            ("gate_release", 150.0),
        ],
    },
    Programm {
        name: "Nur absenken",
        werte: &[
            ("gate", 1.0),
            ("gate_schwelle", -45.0),
            ("gate_tiefe", 10.0),
            ("gate_attack", 5.0),
            ("gate_hold", 200.0),
            ("gate_release", 400.0),
        ],
    },
];

const RAUSCHEN: &[Programm] = &[
    Programm {
        name: "Leicht",
        werte: &[
            ("nr", 0.3),
            ("nr_schwelle", -50.0),
            ("nr_modus", 0.0),
            ("nr_gate", 0.0),
        ],
    },
    Programm {
        name: "Mittel",
        werte: &[
            ("nr", 0.6),
            ("nr_schwelle", -45.0),
            ("nr_modus", 0.0),
            ("nr_gate", 0.0),
        ],
    },
    Programm {
        name: "Stark",
        werte: &[
            ("nr", 1.0),
            ("nr_schwelle", -40.0),
            ("nr_modus", 0.0),
            ("nr_gate", 1.0),
        ],
    },
    Programm {
        name: "RNNoise",
        werte: &[
            ("nr", 1.0),
            ("nr_schwelle", -45.0),
            ("nr_modus", 2.0),
            ("nr_gate", 0.0),
        ],
    },
];

/// Name eines Programms; 0 ist „eigene Einstellung“.
pub fn name(modul: Modul, nummer: u32) -> &'static str {
    match nummer {
        0 => "Eigene",
        n => modul
            .programme()
            .get(n as usize - 1)
            .map(|p| p.name)
            .unwrap_or("Eigene"),
    }
}

/// Wählt Programm `nummer` (1-basiert) für einen Streifen. 0 setzt nur die
/// Anzeige auf „eigene Einstellung“ zurück und lässt die Werte stehen.
pub fn waehlen(params: &Params, strip: usize, modul: Modul, nummer: u32) -> bool {
    let Some(s) = params.strips.get(strip) else {
        return false;
    };
    if nummer > 0 {
        let Some(p) = modul.programme().get(nummer as usize - 1) else {
            return false;
        };
        for (param, wert) in p.werte {
            params.set("strip", strip, param, *wert);
        }
    }
    // Nach den Werten: deren Setzen hat die Nummer auf 0 gestellt.
    s.prg[modul as usize].store(nummer, Relaxed);
    true
}

/// Aktuelle Programmnummer eines Moduls.
pub fn aktuell(params: &Params, strip: usize, modul: Modul) -> u32 {
    params
        .strips
        .get(strip)
        .map(|s| s.prg[modul as usize].load(Relaxed))
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn programm_setzt_werte_und_merkt_die_nummer() {
        let p = Params::new(48_000.0, 64);
        assert!(waehlen(&p, 2, Modul::Comp, 2));
        let s = &p.strips[2];
        assert!(s.comp.on.load(Relaxed));
        assert_eq!(f32::from_bits(s.comp.ratio.load(Relaxed)), 4.0);
        assert_eq!(aktuell(&p, 2, Modul::Comp), 2);
        assert_eq!(name(Modul::Comp, 2), "Sprache kräftig");
    }

    #[test]
    fn handaenderung_hebt_das_programm_auf() {
        let p = Params::new(48_000.0, 64);
        waehlen(&p, 0, Modul::Gate, 3);
        assert_eq!(aktuell(&p, 0, Modul::Gate), 3);
        p.set("strip", 0, "gate_hold", 300.0);
        assert_eq!(aktuell(&p, 0, Modul::Gate), 0);
        // Andere Module bleiben unberuehrt.
        waehlen(&p, 0, Modul::Rauschen, 1);
        p.set("strip", 0, "comp_ratio", 3.0);
        assert_eq!(aktuell(&p, 0, Modul::Rauschen), 1);
    }

    #[test]
    fn ueber_set_waehlbar_und_unbekanntes_abgelehnt() {
        let p = Params::new(48_000.0, 64);
        assert!(p.set("strip", 1, "prg_d", 3.0));
        assert_eq!(aktuell(&p, 1, Modul::Rauschen), 3);
        assert!(p.strips[1].nr_gate.load(Relaxed));
        assert!(!p.set("strip", 1, "prg_d", 99.0));
        assert_eq!(aktuell(&p, 1, Modul::Rauschen), 3);
    }

    #[test]
    fn alle_programme_setzen_nur_bekannte_parameter() {
        let p = Params::new(48_000.0, 64);
        for m in Modul::ALLE {
            for (i, prg) in m.programme().iter().enumerate() {
                for (param, wert) in prg.werte {
                    assert!(p.set("strip", 0, param, *wert), "{} {param}", prg.name);
                    assert_eq!(Modul::von_wert(param), Some(m), "{param}");
                }
                assert!(waehlen(&p, 0, m, i as u32 + 1));
            }
        }
    }
}
