//! Das Fenster ohne Engine und ohne PipeWire (AGENTS.md §4): ein
//! Beispielmodell hinein, über AccessKit bedienen (Rollen und Namen müssen
//! stimmen, sonst findet der Test nichts), den Kern prüfen.
//!
//! Mit `CROSSPOINT_BILDER=<verzeichnis>` schreibt der Lauf außerdem die
//! Bildschirmfotos für README und Doku – erzeugt, nicht von Hand abgelegt.

use std::path::PathBuf;
use std::sync::atomic::Ordering::Relaxed;
use std::sync::Arc;

use crosspoint::engine::Params;
use crosspoint::pegel::Pegelbank;
use crosspoint_fenster::app::{Fenster, Umgebung};
use crosspoint_fenster::geraete::{Bestand, Eintrag, Zuordner};
use crosspoint_fenster::logik::Kern;
use crosspoint_fenster::modell::{Geraete, Oberflaeche};
use crosspoint_fenster::thema::Thema;
use egui_kittest::kittest::Queryable;
use egui_kittest::Harness;

struct OhnePipeWire;
impl Zuordner for OhnePipeWire {
    fn streifen(&self, _: usize, _: Option<&str>) -> Result<(), String> {
        Ok(())
    }
    fn bus(&self, _: &str, _: Option<&str>) -> Result<(), String> {
        Ok(())
    }
    fn lautstaerke(&self, _: &str, _: Option<f32>, _: Option<bool>) -> Result<(), String> {
        Ok(())
    }
    fn echo(&self, _: &[String]) -> Result<(), String> {
        Ok(())
    }
}

fn eintrag(name: &str, anzeige: &str) -> Eintrag {
    Eintrag {
        name: name.into(),
        anzeige: anzeige.into(),
        fehlt: false,
    }
}

/// Ein Pult wie im Alltag: Mikro auf A1 und B1, Systemton auf A1, Pegel.
fn beispiel(thema: Thema) -> (Fenster, Arc<Params>) {
    let params = Arc::new(Params::new(48_000.0, 1024));
    params.set_text("strip", 0, "label", "Mikro");
    params.set("strip", 0, "send5", 1.0);
    params.set("strip", 0, "gain", -3.5);
    params.set("strip", 5, "gain", -8.0);
    params.set_text("strip", 5, "label", "System");
    params.set("bus", 5, "gate", 1.0);
    params.set("bus", 5, "aec", 1.0);
    params.set_text("bus", 5, "label", "Discord");
    params.set("bus", 0, "gain", -6.0);
    let pegel = Arc::new(Pegelbank::neu());
    for (i, w) in [(0, 0.35), (5, 0.2)] {
        pegel.schreibe_strip(i, w);
    }
    for (b, w) in [(0, 0.3), (5, 0.25)] {
        pegel.schreibe_bus(b, w);
    }
    let kern = Kern {
        params: params.clone(),
        tape: None,
        makros: None,
        tape_ordner: PathBuf::from("~/Musik/crosspoint"),
        preset_ordner: std::env::temp_dir().join("cp-test-presets"),
        zuordner: Box::new(OhnePipeWire),
        vban: Some((
            Arc::new(crosspoint::vban_io::VbanControl::new()),
            Arc::new(crosspoint::vban_io::VbanRecvControl::new()),
        )),
        sample_rate: 48_000,
        midi_pfad: std::env::temp_dir().join("cp-test-midi.cfg"),
    };
    let mut o = Oberflaeche {
        thema,
        ..Oberflaeche::default()
    };
    o.geraete = Geraete::Fertig(Box::new(Bestand {
        quellen: vec![eintrag(
            "alsa_input.usb-Rode_NT-USB-00.analog-stereo",
            "NT-USB",
        )],
        senken: vec![eintrag(
            "alsa_output.pci-0000_00_1f.3.analog-stereo",
            "Lautsprecher",
        )],
        streifen: vec![
            Some("alsa_input.usb-Rode_NT-USB-00.analog-stereo".into()),
            None,
            None,
            None,
            None,
        ],
        streifen_fehlt: vec![false; 5],
        busse: crosspoint::BUS_NAMES[..5]
            .iter()
            .enumerate()
            .map(|(i, b)| {
                (
                    b.to_string(),
                    (i == 0).then(|| "alsa_output.pci-0000_00_1f.3.analog-stereo".to_string()),
                )
            })
            .collect(),
        busse_fehlt: vec![false; 5],
        busse_lautstaerke: vec![None; 5],
        ..Bestand::default()
    }));
    let umgebung = Umgebung {
        pegel: Some(pegel),
        laeuft: Box::new(|| true),
        geraete: None,
        konfig_pfad: None,
    };
    (Fenster::neu(kern, o, umgebung), params)
}

fn harness(f: Fenster, groesse: (f32, f32)) -> Harness<'static, Fenster> {
    Harness::builder()
        .with_size(groesse)
        .with_step_dt(1.0 / 30.0)
        .with_max_steps(8)
        .with_context_setup(crosspoint_fenster::einrichten)
        .build_ui_state(|ui, f: &mut Fenster| f.bild(ui), f)
}

#[test]
fn mute_und_routing_schreiben_in_den_kern() {
    let (f, params) = beispiel(Thema::Dunkel);
    // Full HD: alles muss ohne Scrollen erreichbar sein (B3 ganz rechts).
    let mut h = harness(f, (1920.0, 1080.0));
    h.run_steps(4);
    // Kein Kanal hat Filterknöpfe; FILTER gibt es nur an B1..B3, die alten
    // Popups für GATE und ECHO nicht mehr.
    assert!(h.query_by_label("EQ").is_none());
    assert!(h.query_by_label("NR").is_none());
    assert!(h.query_by_label("GATE").is_none());
    assert!(h.query_by_label("ECHO").is_none());
    assert_eq!(h.get_all_by_label("FILTER").count(), 3);
    // Erstes M = Kanal 1.
    h.get_all_by_label("M").next().expect("M").click();
    h.run_steps(2);
    assert!(params.strips[0].mute.load(Relaxed));
    // FILTER an B1 öffnet das Modal; dort schaltet das Kürzel GATE den Filter.
    h.get_all_by_label("FILTER").next().expect("FILTER").click();
    h.run_steps(3);
    assert!(
        h.query_by_label("FILTER · B1 Discord").is_some(),
        "Modal fehlt"
    );
    h.get_by_label("GATE").click();
    h.run_steps(2);
    let gates: Vec<bool> = (5..8)
        .map(|b| {
            params.buses[b]
                .filter
                .as_ref()
                .unwrap()
                .gate
                .on
                .load(Relaxed)
        })
        .collect();
    assert_eq!(
        gates,
        vec![false, false, false],
        "B1 aus, B2/B3 unverändert"
    );
}

#[test]
fn hand_ueber_klickbarem_pfeil_sonst() {
    let (f, _) = beispiel(Thema::Dunkel);
    let mut h = harness(f, (1920.0, 1080.0));
    h.run_steps(3);
    let zeiger = |h: &mut Harness<'static, Fenster>| {
        h.run_steps(2);
        h.output().platform_output.cursor_icon
    };
    for name in ["M", "FILTER", "PRESETS", "A1"] {
        h.get_all_by_label(name).next().expect(name).hover();
        assert_eq!(zeiger(&mut h), egui::CursorIcon::PointingHand, "{name}");
    }
    h.get_by_label("Mikro Fader dB").hover();
    assert_eq!(zeiger(&mut h), egui::CursorIcon::ResizeVertical, "Fader");
}

#[test]
fn filter_modal_waehlt_echo_geraete_und_schaltet_filter() {
    use crosspoint_fenster::logik::filter_werte;
    use crosspoint_fenster::modell::{Aktion, Aufklapp};
    let (f, p) = beispiel(Thema::Dunkel);
    let mut h = harness(f, (1920.0, 1800.0));
    h.run_steps(4);
    // Rechtsklick auf FILTER öffnet dasselbe Modal wie der Linksklick.
    h.get_all_by_label("FILTER")
        .next()
        .expect("FILTER")
        .click_secondary();
    h.run_steps(3);
    assert_eq!(h.state().o.offen, Some(Aufklapp::Filter(5)));
    assert!(h.query_by_label("FILTER · B1 Discord").is_some());
    assert!(h.query_by_label("ECHO · ECHOUNTERDRÜCKUNG").is_some());
    assert!(h.query_by_label("Was spielt auf:").is_some());
    assert!(
        h.query_by_label("Lautsprecher").is_some(),
        "Ausgabegerät fehlt"
    );
    // Häkchen setzen: genau dieses Gerät kommt in die Auswahl.
    h.get_by_label("Lautsprecher").click();
    h.run_steps(3);
    assert_eq!(
        h.state().o.echo_geraete,
        vec!["alsa_output.pci-0000_00_1f.3.analog-stereo".to_string()]
    );
    // Das Modal bleibt offen (kein Schließen beim Ankreuzen).
    assert!(h.query_by_label("FILTER · B1 Discord").is_some());

    // Jeder Filter hat einen eigenen Schalter, der im Kern landet.
    assert!(!filter_werte(p.buses[5].filter.as_ref().unwrap()).nr);
    h.get_by_label("NR").click();
    h.run_steps(2);
    assert!(filter_werte(p.buses[5].filter.as_ref().unwrap()).nr);
    h.get_by_label("LC").click();
    h.run_steps(2);
    assert!(filter_werte(p.buses[5].filter.as_ref().unwrap()).lc);
    h.get_by_label("COMP").click();
    h.run_steps(2);
    assert!(filter_werte(p.buses[5].filter.as_ref().unwrap()).comp);
    h.get_by_label("LIMITER").click();
    h.run_steps(2);
    assert!(filter_werte(p.buses[5].filter.as_ref().unwrap()).limiter);
    h.get_by_label("AGC").click();
    h.run_steps(2);
    assert!(filter_werte(p.buses[5].filter.as_ref().unwrap()).agc);
    h.get_by_label("DUCK").click();
    h.run_steps(2);
    assert!(filter_werte(p.buses[5].filter.as_ref().unwrap()).duck);
    h.get_by_label("PTT").click();
    h.run_steps(2);
    assert!(filter_werte(p.buses[5].filter.as_ref().unwrap()).ptt);
    // Das Gate war an (Beispielmodell) → jetzt aus.
    h.get_by_label("GATE").click();
    h.run_steps(2);
    assert!(!filter_werte(p.buses[5].filter.as_ref().unwrap()).gate);
    // Die Lämpchenreihe am Zug nennt den Stand (sie schaltet nicht).
    let _ = Aktion::Aufklappen(None);
    // Escape schließt das Modal.
    h.key_press(egui::Key::Escape);
    h.run_steps(2);
    assert_eq!(h.state().o.offen, None, "Escape schließt nicht");
}

#[test]
fn dialoge_kommen_mit_inhalt() {
    use crosspoint_fenster::modell::{Aktion, Dialog};
    for (dialog, erwartet) in [
        (Dialog::Midi, "SAVE MIDI MAP"),
        (Dialog::Vban, "ÜBERNEHMEN"),
        (Dialog::Presets, "SPEICHERN"),
        (Dialog::Darstellung, "mono"),
    ] {
        let (mut f, _) = beispiel(Thema::Dunkel);
        f.ausfuehren(Aktion::Dialog(Some(dialog)));
        let mut h = harness(f, (1920.0, 1080.0));
        h.run_steps(4);
        assert!(
            h.query_by_label(erwartet).is_some(),
            "{dialog:?}: '{erwartet}' fehlt"
        );
    }
}

#[test]
fn bilder_in_allen_themes() {
    let ziel = std::env::var_os("CROSSPOINT_BILDER").map(PathBuf::from);
    for thema in Thema::ALLE {
        for (breite, hoehe) in [(2560.0, 1440.0), (1920.0, 1080.0), (1280.0, 720.0)] {
            let (f, _) = beispiel(thema);
            let mut h = harness(f, (breite, hoehe));
            h.run_steps(6);
            assert!(
                h.query_by_label("Mikro Fader dB").is_some(),
                "{}",
                thema.name()
            );
            if let Some(ziel) = &ziel {
                let bild = h.render().expect("zeichnen (wgpu)");
                std::fs::create_dir_all(ziel).unwrap();
                bild.save(ziel.join(format!("pult-{}-{breite}.png", thema.name())))
                    .unwrap();
            }
        }
        // Das FILTER-Modal ist neu: ein Bild je Theme, damit man es im PR
        // sieht (und die Lämpchenreihe am Zug mit).
        let (mut f, params) = beispiel(thema);
        // Ein paar Filter an, damit die Lämpchen und Karten etwas zeigen.
        for (k, v) in [
            ("lc", 1.0),
            ("nr", 1.0),
            ("comp", 1.0),
            ("limiter", 1.0),
            ("agc", 1.0),
            ("duck", 1.0),
            ("ptt", 1.0),
        ] {
            params.set("bus", 5, k, v);
        }
        // Pegel vor und nach den Filtern (schreibt sonst der Audio-Thread).
        params.buses[5].roh.store(0.4f32.to_bits(), Relaxed);
        params.buses[5].level.store(0.12f32.to_bits(), Relaxed);
        f.ausfuehren(crosspoint_fenster::modell::Aktion::Aufklappen(Some(
            crosspoint_fenster::modell::Aufklapp::Filter(5),
        )));
        let mut h = harness(f, (1920.0, 1080.0));
        h.run_steps(6);
        assert!(h.query_by_label("FILTER · B1 Discord").is_some());
        if let Some(ziel) = &ziel {
            let bild = h.render().expect("zeichnen (wgpu)");
            std::fs::create_dir_all(ziel).unwrap();
            bild.save(ziel.join(format!("filter-modal-{}.png", thema.name())))
                .unwrap();
        }
        // Der Darstellung-Dialog zeigt Sonne, Mond und Kontrast — ein Bild
        // je Theme, damit die Icons geprüft sind.
        let (mut f, _) = beispiel(thema);
        f.ausfuehren(crosspoint_fenster::modell::Aktion::Dialog(Some(
            crosspoint_fenster::modell::Dialog::Darstellung,
        )));
        let mut h = harness(f, (1280.0, 720.0));
        h.run_steps(6);
        assert!(h.query_by_label("SCHLIESSEN").is_some(), "Schließen fehlt");
        if let Some(ziel) = &ziel {
            let bild = h.render().expect("zeichnen (wgpu)");
            std::fs::create_dir_all(ziel).unwrap();
            bild.save(ziel.join(format!("darstellung-{}.png", thema.name())))
                .unwrap();
        }
    }
}

/// Icons stehen vor dem Wort, nicht statt seiner: jeder Kopf-, Tape- und
/// Makroknopf ist weiter über seinen Namen erreichbar (AccessKit liest ihn
/// vor), und der Name bleibt eindeutig.
#[test]
fn icons_lassen_die_namen_stehen() {
    let (f, _) = beispiel(Thema::Dunkel);
    let mut h = harness(f, (1920.0, 1080.0));
    h.run_steps(4);
    for name in ["PRESETS", "VBAN", "MIDI", "DARSTELLUNG", "REC", "STOP"] {
        assert_eq!(
            h.get_all_by_label(name).count(),
            1,
            "{name}: nicht genau einmal auffindbar"
        );
    }
    // Die Makroleiste klappt über den Namen um, nicht über das gezeichnete
    // Zeichen — das Icon ist nur das Bild dazu.
    h.get_by_label("EINKLAPPEN").click();
    h.run_steps(3);
    assert!(
        h.query_by_label("AUFKLAPPEN").is_some(),
        "Einklappen hat nicht umgeschaltet"
    );
    // Die Gerätewahl trägt Mikro bzw. Lautsprecher, der Name bleibt der des
    // Geräts.
    assert!(h.query_by_label("NT-USB").is_some());
}
