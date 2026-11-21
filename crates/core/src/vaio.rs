//! `vaio`: ein Hardware-Bus (A1–A5) zusätzlich als eigenes virtuelles Gerät.
//!
//! B1–B3 sind ab Start virtuelle Geräte (`crosspoint_out_B1` …, siehe
//! `scripts/crosspoint-wire.sh`). Mit `vaio` bekommt auch ein A-Bus ein
//! solches Paar: eine Senke `crosspoint_out_A2`, an die der Bus zusätzlich
//! zu seinem Hardware-Ziel angeschlossen wird, und eine echte Aufnahmequelle
//! `crosspoint_mic_A2` – damit lässt sich z. B. die Kopfhörer-Mischung in
//! OBS aufnehmen, ohne den Bus umzustecken.
//!
//! Der Schalter steht im Modell (`bus.vaio`); ein Wächter gleicht den
//! Graphen alle 500 ms daran an. So wirkt er gleich, ob er aus der
//! Oberfläche, über OSC, die HTTP-API oder ein Preset kommt.
//!
//! Abgeräumt wird beim Ausschalten (Module entladen) und beim Beenden über
//! den allgemeinen Sweep: alle Namen beginnen mit `crosspoint_`.

use std::process::Command;
use std::sync::atomic::Ordering::Relaxed;
use std::sync::Arc;
use std::time::Duration;

use crate::engine::Params;
use crate::BUS_NAMES;

/// Busse, die `vaio` kennen (A1–A5).
pub const VAIO_BUSSE: usize = 5;

/// Knotennamen eines Busses: (Senke, Quelle, Beschreibung).
pub fn namen(bus: usize) -> (String, String, String) {
    let b = BUS_NAMES[bus];
    (
        format!("crosspoint_out_{b}"),
        format!("crosspoint_mic_{b}"),
        format!("CROSSPOINT-Output-{b}"),
    )
}

/// Ein Schritt beim Anlegen. Reine Daten, damit sich der Plan prüfen lässt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Schritt {
    /// `pactl load-module …`; die Modulnummer wird gemerkt.
    Modul(Vec<String>),
    /// `pw-link von nach`.
    Link(String, String),
}

/// Was für Bus `bus` zu tun ist, damit er als Gerät erscheint.
pub fn plan(client: &str, bus: usize) -> Vec<Schritt> {
    let (senke, quelle, titel) = namen(bus);
    let b = BUS_NAMES[bus];
    vec![
        Schritt::Modul(vec![
            "module-null-sink".into(),
            format!("sink_name={senke}"),
            format!("sink_properties=device.description=\"{titel}\""),
        ]),
        Schritt::Link(format!("{client}:bus{b}_L"), format!("{senke}:playback_FL")),
        Schritt::Link(format!("{client}:bus{b}_R"), format!("{senke}:playback_FR")),
        Schritt::Modul(vec![
            "module-remap-source".into(),
            format!("master={senke}.monitor"),
            format!("source_name={quelle}"),
            format!("source_properties=device.description={titel}"),
        ]),
    ]
}

/// Abweichung zwischen Wunsch (Modell) und Bestand: (anlegen, entfernen).
pub fn abgleich(wunsch: &[bool], bestand: &[bool]) -> (Vec<usize>, Vec<usize>) {
    let mut an = Vec::new();
    let mut ab = Vec::new();
    for (i, (w, b)) in wunsch.iter().zip(bestand).enumerate() {
        match (w, b) {
            (true, false) => an.push(i),
            (false, true) => ab.push(i),
            _ => {}
        }
    }
    (an, ab)
}

/// Führt die Schritte aus und liefert die geladenen Modulnummern.
fn anlegen(client: &str, bus: usize) -> Vec<u32> {
    let mut module = Vec::new();
    for schritt in plan(client, bus) {
        match schritt {
            Schritt::Modul(args) => {
                let mut cmd = Command::new("pactl");
                cmd.arg("load-module").args(&args);
                match cmd.output() {
                    Ok(o) if o.status.success() => {
                        if let Ok(id) = String::from_utf8_lossy(&o.stdout).trim().parse() {
                            module.push(id);
                        }
                    }
                    Ok(o) => eprintln!(
                        "vaio {}: pactl {}: {}",
                        BUS_NAMES[bus],
                        args[0],
                        String::from_utf8_lossy(&o.stderr).trim()
                    ),
                    Err(e) => eprintln!("vaio: pactl nicht ausführbar: {e}"),
                }
                // Der Knoten braucht einen Moment, bevor er Ports hat.
                std::thread::sleep(Duration::from_millis(300));
            }
            Schritt::Link(von, nach) => {
                let _ = Command::new("pw-link").arg(&von).arg(&nach).output();
            }
        }
    }
    println!(
        "vaio: {} als virtuelles Gerät ({} Module)",
        BUS_NAMES[bus],
        module.len()
    );
    module
}

fn entfernen(bus: usize, module: &[u32]) {
    // Rückwärts: erst die Quelle, dann die Senke, an der sie hängt.
    for id in module.iter().rev() {
        let _ = Command::new("pactl")
            .args(["unload-module", &id.to_string()])
            .output();
    }
    println!("vaio: {} als Gerät entfernt", BUS_NAMES[bus]);
}

/// Startet den Wächter. `geschuetzt` führt eine Änderung am Graphen unter
/// derselben Sperre aus wie `wire up/down` und liefert `false`, sobald
/// abgeschaltet wird – dann legt der Wächter nichts mehr an und endet (das
/// Abräumen übernimmt der Sweep).
pub fn spawn(params: Arc<Params>, client: String, geschuetzt: fn(&mut dyn FnMut()) -> bool) {
    if std::env::var_os("CROSSPOINT_NO_WIRE").is_some() {
        return;
    }
    std::thread::spawn(move || {
        let mut module: Vec<Vec<u32>> = vec![Vec::new(); VAIO_BUSSE];
        let mut bestand = [false; VAIO_BUSSE];
        loop {
            let wunsch: [bool; VAIO_BUSSE] =
                std::array::from_fn(|i| params.buses[i].vaio.load(Relaxed));
            let (an, ab) = abgleich(&wunsch, &bestand);
            let weiter = geschuetzt(&mut || {
                for &i in &ab {
                    entfernen(i, &module[i]);
                    module[i].clear();
                    bestand[i] = false;
                }
                for &i in &an {
                    module[i] = anlegen(&client, i);
                    bestand[i] = true;
                }
            });
            if !weiter {
                break;
            }
            std::thread::sleep(Duration::from_millis(500));
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plan_legt_senke_links_und_quelle_an() {
        let p = plan("crosspoint", 1);
        assert_eq!(p.len(), 4);
        assert_eq!(
            p[1],
            Schritt::Link(
                "crosspoint:busA2_L".into(),
                "crosspoint_out_A2:playback_FL".into()
            )
        );
        match &p[0] {
            Schritt::Modul(a) => {
                assert_eq!(a[0], "module-null-sink");
                assert!(a.contains(&"sink_name=crosspoint_out_A2".to_string()));
            }
            _ => panic!("erst die Senke"),
        }
        match &p[3] {
            Schritt::Modul(a) => {
                assert!(a.contains(&"master=crosspoint_out_A2.monitor".to_string()))
            }
            _ => panic!("zuletzt die Quelle"),
        }
    }

    #[test]
    fn namen_beginnen_mit_crosspoint_damit_der_sweep_sie_findet() {
        for i in 0..VAIO_BUSSE {
            let (s, q, _) = namen(i);
            assert!(s.starts_with("crosspoint_") && q.starts_with("crosspoint_"));
        }
    }

    #[test]
    fn abgleich_findet_nur_aenderungen() {
        let (an, ab) = abgleich(&[true, false, true, false], &[true, true, false, false]);
        assert_eq!(an, vec![2]);
        assert_eq!(ab, vec![1]);
    }
}
