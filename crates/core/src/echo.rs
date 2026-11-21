//! Echo-Referenz: welche Ausgabegeräte spielen auf den Boxen?
//!
//! Die Echounterdrückung an B1–B3 muss wissen, was aus den Lautsprechern kommt.
//! Programme wie Chromium spielen aber **an Crosspoint vorbei** direkt auf das
//! Gerät (HDMI-Monitor, USB-Boxen); der Ausgang eines A-Busses ist dort nicht
//! die Wahrheit. Deshalb hängen die Monitore der gewählten Ausgabegeräte am
//! Mixer selbst: `<sink>:monitor_FL/FR` → `crosspoint:echo_ref_L/R`.
//!
//! * Mehrere Geräte gleichzeitig sind erlaubt (Boxen und Kopfhörer): jedes
//!   bekommt eigene Links auf dieselben Ports – JACK summiert sie.
//! * Gespeichert wird nur der technische Name (`node.name`) in
//!   `$XDG_CONFIG_HOME/crosspoint/echo.json`, wie die Gerätezuordnung in
//!   `devices.json`. Beim Start werden die Links neu gelegt.
//! * Ist kein Gerät gewählt, bleiben `echo_ref_L/R` still; die
//!   Echounterdrückung lässt das Mikrofon dann unverändert (siehe
//!   [`crate::engine`]).

use std::path::{Path, PathBuf};

use crate::json::Value;
use crate::pw::{self, Graph, Richtung, Zugriff};

/// Client-Name des Mixers im Graphen.
pub const CLIENT: &str = "crosspoint";

/// Kanäle der Referenz: Mixer-Portsuffix → Suffix am Monitor des Sinks.
const KANAELE: [(&str, &str); 2] = [("L", "monitor_FL"), ("R", "monitor_FR")];

/// Datei mit der Auswahl (eine Liste technischer Namen).
const CONFIG_FILE: &str = "echo.json";

/// Port des Mixers für einen Kanal der Referenz, z. B. `crosspoint:echo_ref_L`.
pub fn port(kanal: &str) -> String {
    format!("{CLIENT}:echo_ref_{kanal}")
}

/// Speicherort der Auswahl: `$CROSSPOINT_ECHO_FILE` (Tests), sonst
/// `~/.config/crosspoint/echo.json`.
pub fn datei_pfad() -> PathBuf {
    if let Some(p) = std::env::var_os("CROSSPOINT_ECHO_FILE").filter(|p| !p.is_empty()) {
        return PathBuf::from(p);
    }
    crate::pfade::datei(CONFIG_FILE)
}

// ---------------------------------------------------------------------------
// Merkliste
// ---------------------------------------------------------------------------

/// Gelesene Auswahl. Eine unlesbare oder fehlende Datei ergibt eine leere
/// Liste (erster Start), statt den Start scheitern zu lassen.
///
/// Gelesen wird `{"geraete":["<node.name>", …]}`; eine reine Liste von Namen
/// tut es auch, damit eine handgeschriebene Datei nicht umsonst ist.
pub fn auswahl_mit(pfad: &Path) -> Vec<String> {
    let Ok(text) = std::fs::read_to_string(pfad) else {
        return Vec::new();
    };
    let wert = crate::json::parse(&text).unwrap_or(Value::Null);
    let liste = match &wert {
        Value::Arr(a) => a.clone(),
        _ => wert
            .get("geraete")
            .and_then(|v| match v {
                Value::Arr(a) => Some(a.clone()),
                _ => None,
            })
            .unwrap_or_default(),
    };
    let mut out: Vec<String> = Vec::new();
    for eintrag in liste {
        let name = match eintrag {
            Value::Str(s) => s,
            _ => continue,
        };
        let name = name.trim();
        if name.is_empty() || out.iter().any(|n| n == name) {
            continue;
        }
        out.push(name.to_string());
    }
    out
}

pub fn auswahl() -> Vec<String> {
    auswahl_mit(&datei_pfad())
}

/// Schreibt die Auswahl. Ein nicht anlegbares Verzeichnis ist ein Fehler, eine
/// fehlgeschlagene Verbindung dagegen nicht.
pub fn speichern_mit(pfad: &Path, geraete: &[String]) -> Result<(), String> {
    if let Some(dir) = pfad.parent() {
        std::fs::create_dir_all(dir)
            .map_err(|e| format!("{} nicht anlegbar: {e}", dir.display()))?;
    }
    let json = Value::obj(vec![(
        "geraete",
        Value::Arr(geraete.iter().map(|n| Value::str(n.clone())).collect()),
    )])
    .to_json();
    std::fs::write(pfad, json).map_err(|e| format!("{} nicht schreibbar: {e}", pfad.display()))
}

// ---------------------------------------------------------------------------
// Ports und Verbinden
// ---------------------------------------------------------------------------

/// Monitor-Ports eines Sinks (`<sink>:monitor_FL/FR` bzw. der vorhandene
/// Monokanal).
pub fn monitor_ports(graph: &Graph, sink: &str) -> Vec<String> {
    match graph.knoten(sink) {
        Some(knoten) => pw::kanalpaar(&graph.pfade(knoten.id, Richtung::Out)),
        None => Vec::new(),
    }
}

/// Portpaare Sink → Mixer für einen gemerkten Eintrag – auch dann brauchbar,
/// wenn das Gerät gerade fehlt (dann steht kein Link im Graphen, an dem sich
/// die Verbindung festmachen ließe).
pub fn ports_zu(sink: &str) -> Vec<(String, String)> {
    KANAELE
        .iter()
        .map(|(kanal, ziel)| (format!("{sink}:{ziel}"), port(kanal)))
        .collect()
}

/// Ist der Sink laut Graph mit der Echo-Referenz verbunden?
pub fn ist_verbunden(graph: &Graph, sink: &str) -> bool {
    let praefix = format!("{sink}:");
    graph
        .offene_links(&[port("L"), port("R")])
        .iter()
        .any(|v| v.von.starts_with(&praefix))
}

/// Verbindet einen Sink mit den Referenzports. Ein Monogerät (ein Monitorport)
/// wird auf beide Seiten gelegt.
pub fn verbinden(zugriff: &dyn Zugriff, graph: &Graph, sink: &str) -> Result<(), String> {
    if graph.knoten(sink).is_none() {
        return Err(format!("Sink '{sink}' steht nicht im PipeWire-Graph"));
    }
    let ports = monitor_ports(graph, sink);
    match ports.len() {
        0 => Err(format!("Sink '{sink}' hat keinen Monitorport")),
        1 => {
            zugriff.verbinden(&ports[0], &port("L"))?;
            zugriff.verbinden(&ports[0], &port("R"))
        }
        _ => {
            zugriff.verbinden(&ports[0], &port("L"))?;
            zugriff.verbinden(&ports[1], &port("R"))
        }
    }
}

/// Alle Links lösen, die in die Referenzports hineinführen – egal von wem sie
/// stammen. Damit ist ein Wechsel der Auswahl kein Sammelsurium.
fn referenz_loesen(zugriff: &dyn Zugriff, graph: &Graph) {
    for verbindung in graph.offene_links(&[port("L"), port("R")]) {
        let _ = zugriff.trennen(&verbindung.von, &verbindung.nach);
    }
}

// ---------------------------------------------------------------------------
// Auswahl setzen
// ---------------------------------------------------------------------------

/// Setzt die Auswahl: löst alle bisherigen Referenz-Links und verbindet die
/// genannten Sinks neu. Ein Gerät, das gerade fehlt, bleibt in der Auswahl
/// stehen (Absicht, kein Ist-Zustand) – der Wächter legt es nach, sobald es
/// wieder da ist.
///
/// Die Auswahl wird auch geschrieben, wenn ein Gerät nicht verbunden werden
/// konnte; der Fehler kommt als Meldung zurück.
pub fn setzen_mit(
    zugriff: &dyn Zugriff,
    pfad: &Path,
    geraete: &[String],
) -> Result<Vec<String>, String> {
    let graph = zugriff.graph()?;
    referenz_loesen(zugriff, &graph);
    let mut fehler: Vec<String> = Vec::new();
    for sink in geraete {
        if let Err(e) = verbinden(zugriff, &graph, sink) {
            fehler.push(e);
        }
    }
    speichern_mit(pfad, geraete)?;
    Ok(fehler)
}

/// Wie [`setzen_mit`], am Standardpfad.
pub fn setzen(zugriff: &dyn Zugriff, geraete: &[String]) -> Result<Vec<String>, String> {
    setzen_mit(zugriff, &datei_pfad(), geraete)
}

/// Legt die gemerkte Auswahl beim Start wieder an. Fehlt ein Gerät, wird das
/// gemeldet, nicht verworfen.
pub fn beim_start() {
    let pfad = datei_pfad();
    if let Err(e) = beim_start_mit(&crate::pw::PipeWire, &pfad) {
        eprintln!("Echo-Referenz: {e}");
    }
}

pub fn beim_start_mit(zugriff: &dyn Zugriff, pfad: &Path) -> Result<(), String> {
    let geraete = auswahl_mit(pfad);
    if geraete.is_empty() {
        return Ok(());
    }
    let graph = zugriff.graph()?;
    referenz_loesen(zugriff, &graph);
    let mut gelegt = 0;
    for sink in &geraete {
        match verbinden(zugriff, &graph, sink) {
            Ok(()) => gelegt += 1,
            Err(e) => eprintln!("Echo-Referenz: {e}"),
        }
    }
    println!(
        "Echo-Referenz: {gelegt} von {} Geraet(en) verbunden",
        geraete.len()
    );
    Ok(())
}

// ---------------------------------------------------------------------------
// Zustand für die Oberfläche und die API
// ---------------------------------------------------------------------------

/// `{"geraete":[…],"ausgabegeraete":[…],"links":[…],"error":…}`
///
/// * `geraete` – die Auswahl als technische Namen.
/// * `ausgabegeraete` – alle Sinks aus dem Graphen mit `gewaehlt`,
///   `vorhanden` und `verbunden`; die Oberfläche setzt daraus die Häkchen.
/// * `links` – was laut Graph wirklich an `echo_ref_L/R` hängt.
pub fn status_json_mit(zugriff: &dyn Zugriff, pfad: &Path) -> String {
    let auswahl = auswahl_mit(pfad);
    let graph = zugriff.graph();
    let (senken, fehler) = match &graph {
        Ok(g) => (
            // Die eigenen virtuellen Geräte taugen nicht als Referenz: sie
            // tragen das, was Crosspoint gerade ausgibt (Rückkopplung).
            crate::devices::sinks(g, false)
                .into_iter()
                .filter(|s| !crate::inputs::ist_eigen(&s.name))
                .collect(),
            Value::Null,
        ),
        Err(e) => (Vec::new(), Value::str(e.clone())),
    };

    let mut geraete: Vec<Value> = senken
        .iter()
        .map(|s| {
            let verbunden = graph
                .as_ref()
                .map(|g| ist_verbunden(g, &s.name))
                .unwrap_or(false);
            Value::obj(vec![
                ("name", Value::str(s.name.clone())),
                ("desc", Value::str(s.desc.clone())),
                ("gewaehlt", Value::Bool(auswahl.contains(&s.name))),
                ("vorhanden", Value::Bool(true)),
                ("verbunden", Value::Bool(verbunden)),
            ])
        })
        .collect();
    // Gemerkte, aber gerade fehlende Geräte stehen mit `vorhanden: false` in
    // der Liste – sonst verschwände die Auswahl aus der Oberfläche.
    for name in &auswahl {
        if senken.iter().any(|s| s.name == *name) {
            continue;
        }
        geraete.push(Value::obj(vec![
            ("name", Value::str(name.clone())),
            ("desc", Value::str(kurzname(name))),
            ("gewaehlt", Value::Bool(true)),
            ("vorhanden", Value::Bool(false)),
            ("verbunden", Value::Bool(false)),
        ]));
    }

    let links: Vec<Value> = graph
        .as_ref()
        .map(|g| {
            g.offene_links(&[port("L"), port("R")])
                .into_iter()
                .map(|v| {
                    Value::obj(vec![
                        ("von", Value::str(v.von)),
                        ("nach", Value::str(v.nach)),
                        ("zustand", Value::str(v.zustand)),
                    ])
                })
                .collect()
        })
        .unwrap_or_default();

    Value::obj(vec![
        (
            "geraete",
            Value::Arr(auswahl.iter().map(|n| Value::str(n.clone())).collect()),
        ),
        ("ausgabegeraete", Value::Arr(geraete)),
        ("links", Value::Arr(links)),
        ("error", fehler),
    ])
    .to_json()
}

pub fn status_json(zugriff: &dyn Zugriff) -> String {
    status_json_mit(zugriff, &datei_pfad())
}

/// Kurzform eines technischen Namens für Meldungen (alles nach dem letzten
/// Punkt fällt weg, `alsa_output.usb-…` wird zu `usb-…`).
pub fn kurzname(name: &str) -> String {
    name.split('.').next().unwrap_or(name).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pw::tests::Wandelzugriff;

    /// HDMI-Monitor (Sink mit Monitorports) und der Mixer mit den beiden
    /// Referenzports – der kleinste Graph, an dem sich das Verlinken zeigt.
    const GRAPH: &str = r#"[
      {"id": 1, "type": "PipeWire:Interface:Node",
       "info": {"props": {"media.class": "Audio/Sink", "node.name": "alsa_output.hdmi-stereo",
                          "node.description": "HDMI-Monitor"}}},
      {"id": 2, "type": "PipeWire:Interface:Node",
       "info": {"props": {"node.name": "crosspoint"}}},
      {"id": 3, "type": "PipeWire:Interface:Port",
       "info": {"direction": "output", "props": {"node.id": 1, "port.name": "monitor_FL"}}},
      {"id": 4, "type": "PipeWire:Interface:Port",
       "info": {"direction": "output", "props": {"node.id": 1, "port.name": "monitor_FR"}}},
      {"id": 5, "type": "PipeWire:Interface:Port",
       "info": {"direction": "input", "props": {"node.id": 2, "port.name": "echo_ref_L"}}},
      {"id": 6, "type": "PipeWire:Interface:Port",
       "info": {"direction": "input", "props": {"node.id": 2, "port.name": "echo_ref_R"}}}
    ]"#;

    const HDMI: &str = "alsa_output.hdmi-stereo";

    fn zugriff() -> Wandelzugriff {
        Wandelzugriff::neu(Graph::parse(GRAPH).expect("Fixture"))
    }

    fn tmp(tag: &str) -> PathBuf {
        let p =
            std::env::temp_dir().join(format!("crosspoint-echo-{}-{tag}.json", std::process::id()));
        let _ = std::fs::remove_file(&p);
        p
    }

    #[test]
    fn leere_datei_ergibt_keine_auswahl() {
        assert!(auswahl_mit(&tmp("leer")).is_empty());
    }

    #[test]
    fn auswahl_wird_geschrieben_und_gelesen() {
        let pfad = tmp("schreiben");
        let geraete = vec![HDMI.to_string(), "alsa_output.pci-hdmi".to_string()];
        speichern_mit(&pfad, &geraete).unwrap();
        assert_eq!(auswahl_mit(&pfad), geraete);
        // Doppelte und leere Namen fallen weg, eine reine Liste geht auch.
        std::fs::write(&pfad, r#"["a", "a", "  ", "b"]"#).unwrap();
        assert_eq!(auswahl_mit(&pfad), vec!["a".to_string(), "b".to_string()]);
        let _ = std::fs::remove_file(&pfad);
    }

    #[test]
    fn setzen_legt_die_links_an_den_referenzports() {
        let pfad = tmp("setzen");
        let zugriff = zugriff();
        let fehler = setzen_mit(&zugriff, &pfad, &[HDMI.to_string()]).unwrap();
        assert!(fehler.is_empty(), "{fehler:?}");
        assert_eq!(auswahl_mit(&pfad), vec![HDMI.to_string()]);
        let an = zugriff.aktionen();
        assert!(
            an.contains(&format!(
                "verbinden {HDMI}:monitor_FL crosspoint:echo_ref_L"
            )),
            "{an:?}"
        );
        assert!(
            an.contains(&format!(
                "verbinden {HDMI}:monitor_FR crosspoint:echo_ref_R"
            )),
            "{an:?}"
        );
        // Und im Graphen hängen sie wirklich.
        let graph = zugriff.graph().unwrap();
        assert!(ist_verbunden(&graph, HDMI));
        let _ = std::fs::remove_file(&pfad);
    }

    #[test]
    fn neue_auswahl_loest_die_alte() {
        let pfad = tmp("wechsel");
        let zugriff = zugriff();
        setzen_mit(&zugriff, &pfad, &[HDMI.to_string()]).unwrap();
        setzen_mit(&zugriff, &pfad, &[]).unwrap();
        assert!(auswahl_mit(&pfad).is_empty());
        let graph = zugriff.graph().unwrap();
        assert!(!ist_verbunden(&graph, HDMI), "Link blieb stehen");
        let _ = std::fs::remove_file(&pfad);
    }

    #[test]
    fn mehrere_geraete_haengen_gleichzeitig() {
        // Kopfhörer und Monitor: beide legen ihre Monitore auf dieselben
        // Referenzports; JACK summiert sie.
        let pfad = tmp("mehrere");
        let zugriff = zugriff();
        let fehler = setzen_mit(&zugriff, &pfad, &[HDMI.to_string()]).unwrap();
        assert!(fehler.is_empty());
        let graph = zugriff.graph().unwrap();
        assert_eq!(
            graph.offene_links(&[port("L"), port("R")]).len(),
            2,
            "zwei Links (L und R)"
        );
        let _ = std::fs::remove_file(&pfad);
    }

    #[test]
    fn unbekanntes_geraet_wird_gemeldet_bleibt_aber_gemerkt() {
        let pfad = tmp("fehlt");
        let zugriff = zugriff();
        let fehler = setzen_mit(&zugriff, &pfad, &["alsa_output.gibtsnicht".to_string()]).unwrap();
        assert_eq!(fehler.len(), 1);
        assert!(fehler[0].contains("gibtsnicht"), "{fehler:?}");
        assert_eq!(
            auswahl_mit(&pfad),
            vec!["alsa_output.gibtsnicht".to_string()]
        );
        let _ = std::fs::remove_file(&pfad);
    }

    #[test]
    fn status_zeigt_gewaehlte_und_fehlende_geraete() {
        let pfad = tmp("status");
        let zugriff = zugriff();
        speichern_mit(&pfad, &[HDMI.to_string(), "alsa_output.gibtsnicht".into()]).unwrap();
        let text = status_json_mit(&zugriff, &pfad);
        let wert = crate::json::parse(&text).unwrap();
        let liste = wert.get("ausgabegeraete").unwrap().as_arr().unwrap();
        let gewaehlt: Vec<&Value> = liste
            .iter()
            .filter(|e| e.get("gewaehlt") == Some(&Value::Bool(true)))
            .collect();
        assert_eq!(gewaehlt.len(), 2, "{text}");
        assert_eq!(gewaehlt[0].get("vorhanden"), Some(&Value::Bool(true)));
        assert_eq!(gewaehlt[1].get("vorhanden"), Some(&Value::Bool(false)));
        let _ = std::fs::remove_file(&pfad);
    }

    #[test]
    fn eigene_geraete_sind_kein_ausgabegeraet() {
        // `crosspoint_in_*` u. Ä. tragen das, was Crosspoint selbst ausgibt –
        // als Referenz ergäbe das eine Rückkopplung und bleibt außen vor.
        let pfad = tmp("eigen");
        let zugriff = zugriff();
        let text = status_json_mit(&zugriff, &pfad);
        let wert = crate::json::parse(&text).unwrap();
        let liste = wert.get("ausgabegeraete").unwrap().as_arr().unwrap();
        assert!(
            liste
                .iter()
                .all(|e| e.get("name") != Some(&Value::str("crosspoint_in_1"))),
            "{text}"
        );
    }

    #[test]
    fn beim_start_legt_die_gemerkte_auswahl_wieder_an() {
        let pfad = tmp("start");
        speichern_mit(&pfad, &[HDMI.to_string()]).unwrap();
        let zugriff = zugriff();
        beim_start_mit(&zugriff, &pfad).unwrap();
        let graph = zugriff.graph().unwrap();
        assert!(ist_verbunden(&graph, HDMI));
        let _ = std::fs::remove_file(&pfad);
    }
}
