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

/// Die Version, die das Programm anzeigt: sie steht im **Wurzelpaket**
/// (`Cargo.toml` oben, AGENTS.md §6) und wird im Betrieb vom Binary ins Modell
/// gelegt. `env!("CARGO_PKG_VERSION")` wäre hier die Version dieses Crates –
/// deshalb gelesen, nicht abgefragt.
fn version_der_wurzel() -> String {
    let pfad = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../Cargo.toml");
    let text = std::fs::read_to_string(&pfad).unwrap_or_else(|e| panic!("{}: {e}", pfad.display()));
    text.lines()
        .skip_while(|z| z.trim() != "[package]")
        .skip(1)
        .find_map(|z| z.trim().strip_prefix("version = "))
        .map(|v| v.trim().trim_matches('"').to_string())
        .expect("version im [package] der Wurzel-Cargo.toml")
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
        version: Some(version_der_wurzel()),
        ..Oberflaeche::default()
    };
    o.geraete = Geraete::Fertig(Box::new(Bestand {
        quellen: vec![eintrag(
            "alsa_input.usb-Rode_NT-USB-00.analog-stereo",
            "NT-USB",
        )],
        // Wie auf einem echten Rechner: lokale Ausgänge, dazwischen
        // Crosspoints eigene Sinks und eine AirPlay-Senke.
        senken: vec![
            eintrag("alsa_output.pci-0000_00_1f.3.analog-stereo", "Lautsprecher"),
            eintrag(
                "alsa_output.pci-0000_03_00.1.hdmi-stereo-extra2",
                "Navi 48 HDMI/DP Audio Controller Digital Stereo (HDMI 3) [AG326UD]",
            ),
            eintrag(
                "alsa_output.usb-Sennheiser_Profile_0226-00.analog-stereo",
                "Sennheiser Profile Analoges Stereo",
            ),
            eintrag("crosspoint_in_1", "CROSSPOINT-Input-1"),
            eintrag("crosspoint_out_B1", "CROSSPOINT-Output-B1"),
            eintrag("crosspoint_out_B2", "CROSSPOINT-Output-B2"),
            eintrag("raop_sink.Wohnzimmer", "Wohnzimmer"),
            eintrag("raop_output.MacBook_Pro", "MacBook Pro von Patrick"),
        ],
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

/// Alle Flächen einer Farbe, die im letzten Bild gezeichnet wurden. Geprüft
/// wird damit, was gemalt ist – ohne Bildschirm und ohne GPU.
fn gezeichnete_flaechen(h: &Harness<'static, Fenster>, farbe: egui::Color32) -> Vec<egui::Rect> {
    h.output()
        .shapes
        .iter()
        .filter_map(|cs| match &cs.shape {
            egui::Shape::Rect(r) if r.fill == farbe => Some(r.rect),
            _ => None,
        })
        .collect()
}

/// Die Farbe, in der ein Text gemalt wurde – so lässt sich prüfen, dass eine
/// Zeile den Token für gedämpfte Schrift trägt und nicht den der Bedienung.
fn textfarbe(h: &Harness<'static, Fenster>, inhalt: &str) -> Option<egui::Color32> {
    fn suche(shape: &egui::Shape, inhalt: &str) -> Option<egui::Color32> {
        match shape {
            egui::Shape::Text(t) if t.galley.text() == inhalt => {
                t.galley.job.sections.first().map(|s| s.format.color)
            }
            egui::Shape::Vec(teile) => teile.iter().find_map(|s| suche(s, inhalt)),
            _ => None,
        }
    }
    h.output()
        .shapes
        .iter()
        .find_map(|cs| suche(&cs.shape, inhalt))
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
        h.query_by_label("FILTER · B1 · Discord").is_some(),
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
    assert!(h.query_by_label("FILTER · B1 · Discord").is_some());
    assert!(h.query_by_label("ECHO · ECHOUNTERDRÜCKUNG").is_some());
    assert!(h
        .query_by_label("Rechnet den Ton der Boxen aus dem Mikrofon heraus.")
        .is_some());
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
    assert!(h.query_by_label("FILTER · B1 · Discord").is_some());

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

/// Keine Scrollleisten (ADR 0007): bei Mindestgröße liegt jede Bedienung
/// vollständig im Fenster, und nirgends steht ein Scrollbalken.
#[test]
fn keine_scrollleisten_bei_mindestgroesse() {
    let min = crosspoint_fenster::logik::mindestfenster();
    // egui_kittest setzt das Pult in ein CentralPanel mit 8 px Außenabstand;
    // im Betrieb füllt es das Fenster ohne Rand (eframe-`App::ui`).
    let rand = 2.0 * 8.0;
    let groesse = egui::vec2(min.x + rand, min.y + rand);
    let fenster = egui::Rect::from_min_size(egui::Pos2::ZERO, groesse);
    let (f, _) = beispiel(Thema::Dunkel);
    let mut h = harness(f, (groesse.x, groesse.y));
    h.run_steps(6);
    assert_eq!(
        h.query_all_by_role(egui::accesskit::Role::ScrollBar)
            .count(),
        0,
        "Scrollbalken im Pult"
    );
    // Erster und letzter Zug, die Zuordnung, der Filterbus mit seiner
    // Lämpchenzeile, die Kopfleiste und die eingeklappte Makroleiste.
    for name in [
        "Mikro Fader dB",
        "MAS A1 Fader dB",
        "Filteranzeige B1",
        "FILTER",
        "A1",
        "B3",
        "PRESETS",
        "AUFKLAPPEN",
    ] {
        let knoten: Vec<_> = h.get_all_by_label(name).collect();
        assert!(!knoten.is_empty(), "{name} fehlt");
        for n in knoten {
            let r = n.rect();
            assert!(
                fenster.contains_rect(r),
                "{name} liegt außerhalb: {r:?} (Fenster {fenster:?})"
            );
        }
    }
    // Und die Zusage der Maße gilt wirklich: bei Mindestgröße bekommt der
    // Fader seine Mindesthöhe und -breite, die Skala steht in voller Schrift.
    use crosspoint_fenster::logik as mass;
    for name in ["Mikro Fader dB", "Discord Fader dB", "MAS A1 Fader dB"] {
        let r = h.get_by_label(name).rect();
        assert!(
            r.height() >= mass::fader_block_mindesthoehe() - 0.5,
            "{name}: Faderblock nur {:.1} px hoch",
            r.height()
        );
        assert!(
            r.width() >= mass::fader_block_mindestbreite() - 0.5,
            "{name}: Faderblock nur {:.1} px breit",
            r.width()
        );
        assert!(
            mass::fader_skala_schrift(r.width()) >= crosspoint_fenster::thema::FONT_SIZE_SM,
            "{name}: Skala in {:.1} px statt in der kleinen Schrift",
            mass::fader_skala_schrift(r.width())
        );
        assert!(
            mass::fader_laenge(r.height()) >= mass::fader_mindestlaenge() - 0.5,
            "{name}: Faderbahn nur {:.1} px",
            mass::fader_laenge(r.height())
        );
    }
    // Der Kanalzug ist breit genug für die Zuordnungsknöpfe A1..B3.
    let a1 = h.get_all_by_label("A1").next().unwrap().rect();
    assert!(a1.width() >= 15.6, "A1-Knopf nur {:.1} px", a1.width());
}

#[test]
fn bilder_in_allen_themes() {
    let ziel = std::env::var_os("CROSSPOINT_BILDER").map(PathBuf::from);
    let min = mindestgroesse();
    for thema in Thema::ALLE {
        // Zwei große Fenster und das kleinste, das das Pult zulässt.
        // Das Fenster, in dem das Pult im Alltag steht (1590×640 auf einem
        // 2560×1440-Bildschirm), dazu die Mindestgröße – die drei Größen, an
        // denen die Maße geprüft werden.
        for (breite, hoehe, name) in [
            (2560.0, 1440.0, "2560"),
            (1920.0, 1080.0, "1920"),
            (ALLTAG.x, ALLTAG.y, "1590"),
            (min.x, min.y, "min"),
        ] {
            let (f, _) = beispiel(thema);
            let mut h = harness(f, (breite, hoehe));
            h.run_steps(6);
            assert!(
                h.query_by_label("Mikro Fader dB").is_some(),
                "{}",
                thema.name()
            );
            assert_eq!(
                h.query_all_by_role(egui::accesskit::Role::ScrollBar)
                    .count(),
                0,
                "{} bei {name}: Scrollbalken",
                thema.name()
            );
            if let Some(ziel) = &ziel {
                let bild = h.render().expect("zeichnen (wgpu)");
                std::fs::create_dir_all(ziel).unwrap();
                bild.save(ziel.join(format!("pult-{}-{name}.png", thema.name())))
                    .unwrap();
            }
        }
        // Das FILTER-Modal: die ganze Kette in vier Spalten, bei 1920 und im
        // Alltagsfenster und im kleinsten Fenster (die Lämpchenreihe am Zug
        // ist mit drauf).
        for (breite, hoehe, name) in [
            (1920.0, 1080.0, ""),
            (ALLTAG.x, ALLTAG.y, "-1590"),
            (min.x, min.y, "-min"),
        ] {
            let (mut f, params) = beispiel(thema);
            // Ein paar Filter an, ein paar aus: die Bilder sollen zeigen, wie
            // An und Aus am Schalter aussehen (das Beispielmodell hat LC, AGC
            // und PTT aus, ECHO und GATE an).
            for (k, v) in [("nr", 1.0), ("comp", 1.0), ("limiter", 1.0), ("duck", 1.0)] {
                params.set("bus", 5, k, v);
            }
            // Pegel vor und nach den Filtern (schreibt sonst der Audio-Thread).
            params.buses[5].roh.store(0.4f32.to_bits(), Relaxed);
            params.buses[5].level.store(0.12f32.to_bits(), Relaxed);
            f.ausfuehren(crosspoint_fenster::modell::Aktion::Aufklappen(Some(
                crosspoint_fenster::modell::Aufklapp::Filter(5),
            )));
            let mut h = harness(f, (breite, hoehe));
            h.run_steps(6);
            assert!(h.query_by_label("FILTER · B1 · Discord").is_some());
            assert_eq!(
                h.query_all_by_role(egui::accesskit::Role::ScrollBar)
                    .count(),
                0,
                "{}: Scrollbalken im Modal",
                thema.name()
            );
            if let Some(ziel) = &ziel {
                let bild = h.render().expect("zeichnen (wgpu)");
                std::fs::create_dir_all(ziel).unwrap();
                bild.save(ziel.join(format!("filter-modal-{}{name}.png", thema.name())))
                    .unwrap();
            }
        }
        // Der Darstellung-Dialog zeigt Sonne, Mond und Kontrast — ein Bild
        // je Theme, damit die Icons geprüft sind.
        let (mut f, _) = beispiel(thema);
        f.ausfuehren(crosspoint_fenster::modell::Aktion::Dialog(Some(
            crosspoint_fenster::modell::Dialog::Darstellung,
        )));
        let mut h = harness(f, (min.x, min.y));
        h.run_steps(6);
        assert!(h.query_by_label("SCHLIESSEN").is_some(), "Schließen fehlt");
        if let Some(ziel) = &ziel {
            let bild = h.render().expect("zeichnen (wgpu)");
            std::fs::create_dir_all(ziel).unwrap();
            bild.save(ziel.join(format!("darstellung-{}.png", thema.name())))
                .unwrap();
        }
        // Der MIDI-Dialog mit seinen Abschnitten als Reiter.
        let (mut f, _) = beispiel(thema);
        f.ausfuehren(crosspoint_fenster::modell::Aktion::Dialog(Some(
            crosspoint_fenster::modell::Dialog::Midi,
        )));
        let mut h = harness(f, (min.x, min.y));
        h.run_steps(6);
        assert!(h.query_by_label("FREIE CC").is_some(), "Reiter fehlen");
        if let Some(ziel) = &ziel {
            let bild = h.render().expect("zeichnen (wgpu)");
            bild.save(ziel.join(format!("midi-{}.png", thema.name())))
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

/// Größe des Testfensters für „Mindestgröße": egui_kittest setzt das Pult in
/// ein CentralPanel mit 8 px Außenabstand, im Betrieb füllt es das Fenster
/// ohne Rand (eframe-`App::ui`).
fn mindestgroesse() -> egui::Vec2 {
    let min = crosspoint_fenster::logik::mindestfenster();
    egui::vec2(min.x + 16.0, min.y + 16.0)
}

/// Das Fenster, in dem das Pult beim Nutzer steht: 1590×640 auf einem
/// 2560×1440-Bildschirm – knapp über der Mindestgröße und damit die Lage, in
/// der sich alles drängen muss.
const ALLTAG: egui::Vec2 = egui::vec2(1590.0, 640.0);

/// Alle Knoten, die zu einem Namen gehören, liegen im Fenster; es gibt keinen
/// Scrollbalken.
fn liegt_im_fenster(h: &Harness<'static, Fenster>, namen: &[&str]) -> egui::Rect {
    let groesse = mindestgroesse();
    let fenster = egui::Rect::from_min_size(egui::Pos2::ZERO, groesse);
    assert_eq!(
        h.query_all_by_role(egui::accesskit::Role::ScrollBar)
            .count(),
        0,
        "Scrollbalken im Fenster"
    );
    for name in namen {
        let knoten: Vec<_> = h.get_all_by_label(name).collect();
        assert!(!knoten.is_empty(), "{name} fehlt");
        for n in knoten {
            let r = n.rect();
            assert!(
                r.intersect(fenster) == r,
                "{name} liegt außerhalb: {r:?} (Fenster {fenster:?})"
            );
        }
    }
    fenster
}

/// Das FILTER-Modal steht im Fenster: bei der Alltagsgröße (1590×640) und im
/// kleinsten Fenster liegt der ganze Dialog mit Rand im Fenster, jede Karte im
/// Dialog und jede Zeile in ihrer Karte – ohne Scrollbalken. Dazu vier Spalten,
/// die die Innenbreite genau füllen und zwei volle Zeilen ergeben.
#[test]
fn filter_modal_passt_ins_fenster() {
    use egui_kittest::kittest::NodeT;
    let min = mindestgroesse();
    for (breite, hoehe, name) in [(ALLTAG.x, ALLTAG.y, "1590"), (min.x, min.y, "Mindestgröße")] {
        let (f, _) = filter_offen(Thema::Dunkel);
        let mut h = harness(f, (breite, hoehe));
        h.run_steps(6);
        assert_eq!(
            h.query_all_by_role(egui::accesskit::Role::ScrollBar)
                .count(),
            0,
            "{name}: Scrollbalken"
        );
        // Der Dialog steht mit Luft zum Fensterrand, nicht am Anschlag.
        let fenster = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(breite, hoehe));
        let dialog = h.get_by_label("Dialog FILTER · B1 · Discord").rect();
        let rand = crosspoint_fenster::thema::SPACE_2;
        assert!(
            dialog.min.x >= rand && dialog.min.y >= rand,
            "{name}: Dialog beginnt bei {dialog:?} (Fenster {fenster:?})"
        );
        assert!(
            dialog.max.x <= fenster.max.x - rand && dialog.max.y <= fenster.max.y - rand,
            "{name}: Dialog endet bei {dialog:?} (Fenster {fenster:?})"
        );
        // Jede Karte steht ganz im Dialog …
        let karten: Vec<egui::Rect> = h
            .get_all_by_label_contains("Filterkarte ")
            .map(|n| n.rect())
            .collect();
        assert_eq!(karten.len(), 8, "{name}: nicht acht Karten");
        for karte in &karten {
            assert!(
                dialog.contains_rect(*karte),
                "{name}: Karte {karte:?} ragt aus dem Dialog {dialog:?}"
            );
        }
        // … und alles, was in einer Karte bedient wird, in seiner Karte: kein
        // Regler, kein Wertfeld, kein Schalter steht auf dem Rahmen. Was unter
        // dem Dialog liegt (das Pult), zählt nicht mit.
        let dialog_knoten = h.get_by_label("Dialog FILTER · B1 · Discord");
        let drin: Vec<(egui::accesskit::Role, egui::Rect)> = dialog_knoten
            .children_recursive()
            .filter_map(|n| {
                let rolle = NodeT::accesskit_node(&n).role();
                matches!(
                    rolle,
                    egui::accesskit::Role::Slider
                        | egui::accesskit::Role::SpinButton
                        | egui::accesskit::Role::ComboBox
                        | egui::accesskit::Role::CheckBox
                )
                .then(|| (rolle, n.rect()))
            })
            .collect();
        assert!(
            drin.len() >= 40,
            "{name}: nur {} Bedienelemente",
            drin.len()
        );
        for (rolle, r) in &drin {
            if let Some(karte) = karten.iter().find(|k| k.intersects(*r)) {
                assert!(
                    karte.contains_rect(*r),
                    "{name}: {rolle:?} {r:?} ragt aus der Karte {karte:?}"
                );
            }
        }
    }
}

/// Presets: mehr Einträge als eine Seite – geblättert wird, nicht gescrollt.
#[test]
fn preset_liste_blaettert_statt_zu_scrollen() {
    use crosspoint_fenster::logik::PRESET_ZEILEN;
    use crosspoint_fenster::modell::{Aktion, Dialog};
    let ordner = std::env::temp_dir().join(format!("cp-test-pager-{}", std::process::id()));
    std::fs::create_dir_all(&ordner).unwrap();
    for i in 0..(PRESET_ZEILEN + 2) {
        std::fs::write(ordner.join(format!("preset-{i:02}.json")), "{}").unwrap();
    }
    let (mut f, _) = beispiel(Thema::Dunkel);
    f.kern.preset_ordner = ordner.clone();
    f.ausfuehren(Aktion::Dialog(Some(Dialog::Presets)));
    let g = mindestgroesse();
    let mut h = harness(f, (g.x, g.y));
    h.run_steps(6);
    liegt_im_fenster(&h, &["SPEICHERN", "LADEN", "SEITE 1/2"]);
    assert!(h.query_by_label("preset-10").is_none(), "Seite 2 schon da");
    h.get_by_label("WEITER >>").click();
    h.run_steps(3);
    liegt_im_fenster(&h, &["SPEICHERN", "SEITE 2/2", "preset-10", "preset-11"]);
    assert!(h.query_by_label("preset-00").is_none(), "Seite 1 noch da");
    let _ = std::fs::remove_dir_all(&ordner);
}

/// MIDI: die letzte Seite hat 56 Zeilen (Control Change 0..55) – sie kommt in
/// Blättern, nicht in einem Scrollbereich.
#[test]
fn midi_liste_kommt_in_blaettern() {
    use crosspoint_fenster::modell::{Aktion, Dialog};
    let (mut f, _) = beispiel(Thema::Dunkel);
    f.ausfuehren(Aktion::Dialog(Some(Dialog::Midi)));
    let g = mindestgroesse();
    let mut h = harness(f, (g.x, g.y));
    h.run_steps(6);
    assert!(h.query_by_label("SAVE MIDI MAP").is_some());
    // Feedback zu 2.0.0: Seiten und Blätter waren unübersichtlich. Jetzt
    // stehen die Abschnitte als Reiter da; wer nur acht Ziele hat, blättert
    // gar nicht.
    liegt_im_fenster(&h, &["SAVE MIDI MAP", "KANAL-FADER", "FREIE CC"]);
    assert!(h.query_by_label("Gain Fader Strip #8").is_some());
    assert!(
        h.query_by_label("WEITER >>").is_none(),
        "acht Zeilen, ein Blatt"
    );
    h.get_by_label("FREIE CC").click();
    h.run_steps(3);
    assert!(h.query_by_label("Control Change 0").is_some());
    assert!(
        h.query_by_label("Control Change 55").is_none(),
        "alle 56 Zeilen auf einem Blatt"
    );
    for _ in 0..6 {
        h.get_by_label("WEITER >>").click();
        h.run_steps(3);
    }
    // Am Ende bleibt es stehen – kein Sprung in einen anderen Abschnitt.
    liegt_im_fenster(&h, &["BLATT 6/6", "Control Change 50"]);
    assert!(h.query_by_label("Control Change 55").is_some());
}

/// Auch die übrigen Dialoge stehen im kleinsten Fenster und im
/// Alltagsfenster ohne Scrollen da – und mit Luft zum Fensterrand.
#[test]
fn dialoge_passen_ins_fenster() {
    use crosspoint_fenster::modell::{Aktion, Dialog};
    use crosspoint_fenster::thema;
    let min = mindestgroesse();
    for (breite, hoehe, name) in [(ALLTAG.x, ALLTAG.y, "1590"), (min.x, min.y, "Mindestgröße")] {
        for d in Dialog::ALLE {
            let (mut f, _) = beispiel(Thema::Dunkel);
            f.ausfuehren(Aktion::Dialog(Some(d)));
            let mut h = harness(f, (breite, hoehe));
            h.run_steps(6);
            assert!(h.query_by_label("SCHLIESSEN").is_some(), "{d:?}");
            liegt_im_fenster(&h, &["SCHLIESSEN"]);
            // Der ganze Dialog steht im Fenster, mit einem Rand.
            let dialog = h.get_by_label(&format!("Dialog {}", d.titel())).rect();
            let fenster = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(breite, hoehe));
            let rand = thema::SPACE_2;
            assert!(
                dialog.min.x >= rand
                    && dialog.min.y >= rand
                    && dialog.max.x <= fenster.max.x - rand
                    && dialog.max.y <= fenster.max.y - rand,
                "{name}/{d:?}: Dialog {dialog:?} steht nicht mit Rand im Fenster {fenster:?}"
            );
        }
    }
}

/// Öffnet das FILTER-Modal an B1 (mit ein paar Filtern an) und gibt das
/// Fenster samt Kern zurück.
fn filter_offen(thema: Thema) -> (Fenster, Arc<Params>) {
    use crosspoint_fenster::modell::{Aktion, Aufklapp};
    let (mut f, params) = beispiel(thema);
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
    f.ausfuehren(Aktion::Aufklappen(Some(Aufklapp::Filter(5))));
    (f, params)
}

/// Eine Meldung der Kopfzeile als Text.
fn meldung(f: &Fenster) -> String {
    match &f.o.meldung {
        crosspoint_fenster::modell::Meldung::Hinweis(t)
        | crosspoint_fenster::modell::Meldung::Fehler(t) => t.clone(),
        crosspoint_fenster::modell::Meldung::Keine => String::new(),
    }
}

/// Alle Dialoge (FILTER-Modal und die vier Fenster) tragen dasselbe
/// Schließen-X: genau einmal, gleicher zugänglicher Name, in der Titelleiste
/// rechts neben dem Titel – und ein Klick darauf schließt.
#[test]
fn jeder_dialog_hat_dasselbe_schliessen_x() {
    use crosspoint_fenster::modell::{Aktion, Dialog};
    /// Wie ein Dialog geöffnet wird – für die Schleife über alle.
    type Oeffnen = fn(&mut Fenster);
    let dialoge: [(&str, Oeffnen); 5] = [
        ("PRESETS", |f| {
            f.ausfuehren(Aktion::Dialog(Some(Dialog::Presets)))
        }),
        ("VBAN", |f| f.ausfuehren(Aktion::Dialog(Some(Dialog::Vban)))),
        ("MIDI", |f| f.ausfuehren(Aktion::Dialog(Some(Dialog::Midi)))),
        ("DARSTELLUNG", |f| {
            f.ausfuehren(Aktion::Dialog(Some(Dialog::Darstellung)))
        }),
        ("FILTER · B1 · Discord", |f| {
            f.ausfuehren(Aktion::Aufklappen(Some(
                crosspoint_fenster::modell::Aufklapp::Filter(5),
            )))
        }),
    ];
    let mut groessen = Vec::new();
    for (titel, oeffnen) in dialoge {
        let (mut f, _) = beispiel(Thema::Dunkel);
        oeffnen(&mut f);
        let g = mindestgroesse();
        let mut h = harness(f, (g.x, g.y));
        h.run_steps(6);
        // Genau ein Schließen-X, mit demselben Namen wie in allen anderen.
        assert_eq!(
            h.get_all_by_label("SCHLIESSEN").count(),
            1,
            "{titel}: Schließen-X fehlt oder doppelt"
        );
        let x = h.get_by_label("SCHLIESSEN").rect();
        groessen.push(x.size());
        // In derselben Zeile steht der Titel links davon.
        let title = h
            .get_all_by_label(titel)
            .map(|n| n.rect())
            .find(|r| (r.center().y - x.center().y).abs() < 8.0);
        let title = title.unwrap_or_else(|| panic!("{titel}: Titel nicht in der Titelleiste"));
        assert!(
            title.right() < x.left(),
            "{titel}: X steht nicht rechts vom Titel ({title:?} {x:?})"
        );
        // Klick auf das X schließt.
        h.get_by_label("SCHLIESSEN").click();
        h.run_steps(3);
        assert!(
            h.query_by_label("SCHLIESSEN").is_none(),
            "{titel}: X schließt nicht"
        );
    }
    // Gleiche Größe überall: es ist derselbe Baustein.
    for gros in &groessen {
        assert_eq!(*gros, groessen[0], "{groessen:?}");
    }
}

/// Das Raster im FILTER-Modal: gleiche Spaltenbreiten, und jede Zeile füllt
/// die Innenbreite des Modals von der ersten bis zur letzten Karte – ohne
/// Lücke am rechten Rand und ohne leere Zelle.
#[test]
fn filter_raster_hat_gleiche_spalten_und_fuellt_die_breite() {
    let min = mindestgroesse();
    for (breite, hoehe, name) in [
        (ALLTAG.x, ALLTAG.y, "1590"),
        (min.x, min.y, "Mindestgröße"),
        (1920.0, 1080.0, "1920"),
        (2560.0, 1440.0, "2560"),
    ] {
        let (f, _) = filter_offen(Thema::Dunkel);
        let mut h = harness(f, (breite, hoehe));
        h.run_steps(6);
        let raster = h.get_by_label("Filterraster").rect();
        let mut karten: Vec<egui::Rect> = h
            .get_all_by_label_contains("Filterkarte ")
            .map(|n| n.rect())
            .collect();
        assert_eq!(karten.len(), 8, "{name}: nicht acht Karten");
        // Das Raster füllt die Innenbreite des Dialogs: gleiche Kante links
        // und rechts, keine Lücke zum Rahmen.
        let dialog = h.get_by_label("Dialog FILTER · B1 · Discord").rect();
        assert!(
            (raster.left() - dialog.left()).abs() < 0.5,
            "{name}: Raster beginnt bei {} statt {}",
            raster.left(),
            dialog.left()
        );
        assert!(
            (raster.right() - dialog.right()).abs() < 0.5,
            "{name}: Raster endet bei {} statt {}",
            raster.right(),
            dialog.right()
        );
        // Alle Karten gleich breit.
        let kante = karten[0].width();
        for r in &karten {
            assert!(
                (r.width() - kante).abs() < 0.5,
                "{name}: Kartenbreiten {} und {kante}",
                r.width()
            );
        }
        // Zeilenweise: erste Karte am linken, letzte am rechten Rand.
        karten.sort_by(|a, b| {
            a.min
                .y
                .total_cmp(&b.min.y)
                .then(a.min.x.total_cmp(&b.min.x))
        });
        let mut zeilen: Vec<Vec<egui::Rect>> = Vec::new();
        for r in karten {
            match zeilen.last_mut() {
                Some(z) if (z[0].min.y - r.min.y).abs() < 0.5 => z.push(r),
                _ => zeilen.push(vec![r]),
            }
        }
        assert!(zeilen.len() >= 2, "{name}: nur eine Zeile");
        // Die Spaltenkanten stehen in jeder Zeile an derselben Stelle.
        let spalten = zeilen[0].len();
        let kanten: Vec<f32> = zeilen[0].iter().map(|r| r.min.x).collect();
        for zeile in &zeilen {
            let erste = zeile.first().unwrap();
            assert!(
                (erste.min.x - raster.min.x).abs() < 0.5,
                "{name}: Zeile beginnt bei {} statt {}",
                erste.min.x,
                raster.min.x
            );
            for r in zeile {
                assert!(
                    kanten.iter().any(|x| (x - r.min.x).abs() < 0.5),
                    "{name}: Karte bei {} steht nicht in der Spalte",
                    r.min.x
                );
                // Gleiche Höhe innerhalb der Zeile.
                assert!(
                    (r.height() - erste.height()).abs() < 0.5,
                    "{name}: Zeilenhöhen {} und {}",
                    r.height(),
                    erste.height()
                );
            }
        }
        // Eine volle Zeile füllt die Innenbreite genau: Karten plus Lücken.
        let voll = zeilen
            .iter()
            .find(|z| z.len() == spalten)
            .expect("volle Zeile");
        let luecke = voll[1].min.x - voll[0].max.x;
        let gesamt = voll.iter().map(|r| r.width()).sum::<f32>() + luecke * (spalten - 1) as f32;
        assert!(
            (gesamt - raster.width()).abs() < 0.5,
            "{name}: Karten füllen {gesamt} von {}",
            raster.width()
        );
        assert!(
            (voll.last().unwrap().max.x - raster.max.x).abs() < 0.5,
            "{name}: letzte Spalte endet bei {} statt {}",
            voll.last().unwrap().max.x,
            raster.max.x
        );
        // Vier Spalten überall: acht Karten sind dann zwei volle Zeilen, keine
        // Zelle bleibt leer.
        for zeile in &zeilen {
            assert_eq!(
                zeile.len(),
                4,
                "{name}: Zeile mit {} Karten statt vier",
                zeile.len()
            );
        }
        assert_eq!(zeilen.len(), 2, "{name}: nicht zwei Zeilen");
    }
}

/// Die ECHO-Auswahl bietet nur lokale Ausgabegeräte an: keine eigenen Sinks
/// (Schleife Bus → Referenz → Bus) und keine Netzwerk-/AirPlay-Ziele.
#[test]
fn echo_liste_ohne_eigene_und_netzwerk_geraete() {
    let (f, _) = filter_offen(Thema::Dunkel);
    let min = mindestgroesse();
    let mut h = harness(f, (min.x, min.y));
    h.run_steps(6);
    for name in [
        "CROSSPOINT-Input-1",
        "CROSSPOINT-Output-B1",
        "CROSSPOINT-Output-B2",
        "Wohnzimmer",
        "MacBook Pro von Patrick",
    ] {
        assert!(
            h.query_by_label(name).is_none(),
            "{name} steht in der ECHO-Liste"
        );
    }
    for name in [
        "Lautsprecher",
        "Sennheiser Profile Analoges Stereo",
        "Navi 48 HDMI/DP Audio Controller Digital Stereo (HDMI 3) [AG326UD]",
    ] {
        assert!(
            h.query_by_label(name).is_some(),
            "{name} fehlt in der ECHO-Liste"
        );
    }
    assert!(h.state().o.echo_geraete.is_empty());
}

/// Die Meldung der Kopfzeile nennt den Gerätenamen, nicht die Kennung:
/// „Echo-Referenz: HDMI … an“ statt „0000_03_00 an“.
#[test]
fn echo_meldung_nennt_den_geraetenamen() {
    let (f, _) = filter_offen(Thema::Dunkel);
    let min = mindestgroesse();
    let mut h = harness(f, (min.x, min.y));
    h.run_steps(6);
    h.get_by_label("Navi 48 HDMI/DP Audio Controller Digital Stereo (HDMI 3) [AG326UD]")
        .click();
    h.run_steps(3);
    let text = meldung(h.state());
    assert_eq!(
        text,
        "Echo-Referenz: Navi 48 HDMI/DP Audio Controller Digital Stereo (HDMI 3) [AG326UD] an"
    );
    assert_eq!(
        h.state().o.echo_geraete,
        vec!["alsa_output.pci-0000_03_00.1.hdmi-stereo-extra2".to_string()]
    );
}

/// Regler und Werte stehen bündig: in jeder Karte sind alle Schieber gleich
/// breit, beginnen an derselben Kante und reichen bis an die Wertspalte; alle
/// Wertfelder sind gleich breit und enden am Innenrand der Karte. Die Auswahl
/// (Zielbus in DUCK) nimmt dieselbe Spalte ein wie die Schieber.
#[test]
fn filter_karten_haben_buendige_spalten() {
    use crosspoint_fenster::thema;
    use egui::accesskit::Role;
    use egui_kittest::kittest::NodeT;
    /// Die Schieber der Karten tragen den Namen ihrer Zeile …
    const REGLER: [&str; 11] = [
        "Grenze",
        "Schwelle",
        "Tiefe",
        "Attack",
        "Hold",
        "Release",
        "Ratio",
        "Make-up",
        "Zielpegel",
        "Absenkung",
        "Decke",
    ];
    /// … die Wertfelder ihren Suffix.
    const WERTE: [&str; 5] = ["Hz", "dBFS", "dB", "ms", ": 1"];
    let min = mindestgroesse();
    for (breite, hoehe, name) in [
        (min.x, min.y, "Mindestgröße"),
        (1920.0, 1080.0, "1920"),
        (2560.0, 1440.0, "2560"),
    ] {
        let (f, _) = filter_offen(Thema::Dunkel);
        let mut h = harness(f, (breite, hoehe));
        h.run_steps(6);
        let karten: Vec<egui::Rect> = h
            .get_all_by_label_contains("Filterkarte ")
            .map(|n| n.rect())
            .collect();
        assert_eq!(karten.len(), 8, "{name}: nicht acht Karten");
        for karte in &karten {
            // Nur Knoten, die ganz in der Karte liegen und eine Beschriftung
            // der Zeilen tragen: das Pult darunter hat eigene Schieber (Fader)
            // und Wertfelder.
            let drin = |r: &egui::Rect| karte.contains_rect(*r);
            let heisst = |n: &egui_kittest::Node, namen: &[&str]| {
                NodeT::accesskit_node(n)
                    .label()
                    .is_some_and(|l| namen.contains(&l.as_str()))
            };
            let schieber: Vec<egui::Rect> = h
                .query_all_by_role(Role::Slider)
                .filter(|n| heisst(n, &REGLER))
                .map(|n| n.rect())
                .filter(drin)
                .collect();
            let felder: Vec<egui::Rect> = h
                .query_all_by_role(Role::SpinButton)
                .filter(|n| heisst(n, &WERTE))
                .map(|n| n.rect())
                .filter(drin)
                .collect();
            assert_eq!(
                schieber.len(),
                felder.len(),
                "{name}: {karte:?} hat Schieber ohne Wertfeld"
            );
            if schieber.is_empty() {
                continue;
            }
            // Jedes Wertfeld endet am Innenrand der Karte, alle gleich breit.
            let kante = karte.right() - thema::SPACE_2;
            for feld in &felder {
                assert!(
                    (feld.right() - kante).abs() < 0.5,
                    "{name}: Wertfeld endet bei {} statt bei {kante} ({feld:?})",
                    feld.right()
                );
                assert!(
                    (feld.width() - felder[0].width()).abs() < 0.5,
                    "{name}: Wertfelder sind {} und {} breit",
                    feld.width(),
                    felder[0].width()
                );
            }
            // Jeder Schieber gleich lang, gleiche Kante, bis an die Wertspalte.
            for r in &schieber {
                assert!(
                    (r.width() - schieber[0].width()).abs() < 0.5,
                    "{name}: Schieber sind {} und {} lang",
                    r.width(),
                    schieber[0].width()
                );
                assert!(
                    (r.left() - schieber[0].left()).abs() < 0.5,
                    "{name}: Schieber beginnen bei {} und {}",
                    r.left(),
                    schieber[0].left()
                );
                assert!(
                    (felder[0].left() - r.right() - thema::SPACE_1).abs() < 0.5,
                    "{name}: Schieber endet bei {}, Wertspalte beginnt bei {}",
                    r.right(),
                    felder[0].left()
                );
            }
        }
        // Die Zielbus-Auswahl in DUCK nimmt die volle Reglerspalte ein: so
        // breit wie die Schieber der Karte, an derselben linken Kante.
        let auswahl = h
            .query_all_by_role(Role::ComboBox)
            .find(|n| NodeT::accesskit_node(n).label().as_deref() == Some("Zielbus"))
            .unwrap_or_else(|| panic!("{name}: Zielbus-Auswahl fehlt"))
            .rect();
        let karte = karten
            .iter()
            .find(|k| k.contains_rect(auswahl))
            .unwrap_or_else(|| panic!("{name}: Auswahl steht in keiner Karte"));
        let schieber = h
            .query_all_by_role(Role::Slider)
            .map(|n| n.rect())
            .find(|r| karte.contains_rect(*r))
            .unwrap_or_else(|| panic!("{name}: kein Schieber neben der Auswahl"));
        assert!(
            (auswahl.width() - schieber.width()).abs() < 0.5,
            "{name}: Auswahl {:.1} px breit, Schieber {:.1} px",
            auswahl.width(),
            schieber.width()
        );
        assert!(
            (auswahl.left() - schieber.left()).abs() < 0.5,
            "{name}: Auswahl beginnt bei {} statt {}",
            auswahl.left(),
            schieber.left()
        );
    }
}

/// An und Aus sind an jedem Filtersschalter ablesbar: im Beispiel sind nur
/// ECHO und GATE an, also melden genau diese beiden `selected` und alle
/// anderen nicht. (`WidgetInfo::selected` landet in AccessKit als `toggled`;
/// gefüllt wird der Schalter nach demselben Wert.)
#[test]
fn filter_schalter_melden_an_und_aus() {
    use crosspoint_fenster::modell::{Aktion, Aufklapp};
    use egui::accesskit::Toggled;
    use egui_kittest::kittest::NodeT;
    let (mut f, _) = beispiel(Thema::Dunkel);
    f.ausfuehren(Aktion::Aufklappen(Some(Aufklapp::Filter(5))));
    let mut h = harness(f, (1920.0, 1080.0));
    h.run_steps(6);
    for name in [
        "LC", "ECHO", "NR", "GATE", "COMP", "AGC", "DUCK", "PTT", "LIMITER",
    ] {
        let erwartet = if matches!(name, "ECHO" | "GATE") {
            Toggled::True
        } else {
            Toggled::False
        };
        let knoten = h.get_by_label(name);
        assert_eq!(
            NodeT::accesskit_node(&knoten).toggled(),
            Some(erwartet),
            "{name}"
        );
    }
}

/// Die Schiene eines Reglers ist gezeichnet, und zwar über die **ganze**
/// Spalte: vom Ende der Beschriftung bis an die Wertspalte. Vorher malte der
/// Standardregler von egui seine Schiene in der Fläche der Karte und sie
/// verschwand darin – sichtbar blieb nur der gefüllte Teil bis zum Griff.
///
/// Geprüft wird, was gezeichnet wurde (`FullOutput`), nicht was ein Bildschirm
/// daraus macht: kein GPU, kein Bild.
#[test]
fn filter_schiene_ist_ueber_die_ganze_spalte_gezeichnet() {
    use crosspoint_fenster::thema;
    use egui_kittest::kittest::NodeT;
    let (f, _) = filter_offen(Thema::Dunkel);
    let mut h = harness(f, (ALLTAG.x, ALLTAG.y));
    h.run_steps(6);
    let farben = Thema::Dunkel.farben();
    let schienen = gezeichnete_flaechen(&h, farben.border_strong);
    let fuellungen = gezeichnete_flaechen(&h, farben.accent_default);
    let regler: Vec<egui::Rect> = h
        .query_all_by_role(egui::accesskit::Role::Slider)
        .filter(|n| {
            NodeT::accesskit_node(n)
                .label()
                .is_some_and(|l| l == "Grenze" || l == "Schwelle" || l == "Zielpegel")
        })
        .map(|n| n.rect())
        .collect();
    // Grenze (LC), Schwelle (GATE, COMP, DUCK), Zielpegel (AGC): fünf Zeilen
    // mit Reglern, verteilt über alle vier Spalten.
    assert_eq!(regler.len(), 5, "nicht fünf Regler: {}", regler.len());
    for r in &regler {
        // Dieselbe Spalte und dieselbe Zeile: die Karten einer Spalte haben
        // gleiche Kanten, nur die Höhe unterscheidet die Regler.
        let schiene = schienen.iter().find(|s| {
            (s.left() - r.left()).abs() < 0.5
                && (s.right() - r.right()).abs() < 0.5
                && (s.center().y - r.center().y).abs() < 0.5
        });
        let schiene = schiene.unwrap_or_else(|| {
            panic!("keine Schiene über die ganze Spalte bei {r:?} (gefunden: {schienen:?})")
        });
        // Die Schiene ist so hoch wie die Bahn des Faders.
        assert!(
            (schiene.height() - thema::SIZE_FADER_BAHN).abs() < 0.5,
            "Schiene {schiene:?} ist nicht {0} hoch",
            thema::SIZE_FADER_BAHN
        );
        // Der gefüllte Teil steht im Akzent und beginnt an der Schiene.
        assert!(
            fuellungen
                .iter()
                .any(|f| (f.left() - schiene.left()).abs() < 0.5
                    && (f.top() - schiene.top()).abs() < 0.5
                    && f.right() <= schiene.right()),
            "keine Füllung auf der Schiene {schiene:?}"
        );
    }
}

/// Die Zeilen, die keine Regler sind, stehen in derselben Spalte: die vier
/// Stufen der Rauschunterdrückung teilen sich die Breite gleichmäßig, und die
/// Zielbus-Auswahl im DUCKING ist so breit wie die Regler darüber und darunter.
#[test]
fn filter_stufen_und_auswahl_stehen_in_der_reglerspalte() {
    use crosspoint_fenster::thema;
    use egui_kittest::kittest::NodeT;
    let (f, _) = filter_offen(Thema::Dunkel);
    let mut h = harness(f, (ALLTAG.x, ALLTAG.y));
    h.run_steps(6);
    let karten: Vec<egui::Rect> = h
        .get_all_by_label_contains("Filterkarte ")
        .map(|n| n.rect())
        .collect();
    let raster = h.get_by_label("Filterraster").rect();
    // NR: vier Stufenknöpfe, gleich breit, vom Ende der Beschriftung bis an
    // den Innenrand der Karte.
    let stufen: Vec<egui::Rect> = ["niedrig", "mittel", "hoch", "sehr hoch"]
        .iter()
        .map(|name| h.get_by_label(name).rect())
        .collect();
    let breite = stufen[0].width();
    for r in &stufen {
        assert!(
            (r.width() - breite).abs() < 0.5,
            "Stufen sind {} und {breite} breit",
            r.width()
        );
    }
    assert!(
        stufen[0].left() < stufen[1].left() && stufen[1].left() < stufen[2].left(),
        "Stufen stehen nicht nebeneinander"
    );
    let nr = karten
        .iter()
        .find(|k| k.contains_rect(stufen[0]))
        .expect("Stufen stehen in keiner Karte");
    assert!(
        (stufen[3].right() - (nr.right() - thema::SPACE_2)).abs() < 0.5,
        "Stufen enden bei {} statt am Kartenrand {}",
        stufen[3].right(),
        nr.right() - thema::SPACE_2
    );
    // Die Zielbus-Auswahl nimmt dieselbe Spalte ein wie die Regler der Karte.
    let auswahl = h
        .query_all_by_role(egui::accesskit::Role::ComboBox)
        .find(|n| NodeT::accesskit_node(n).label().as_deref() == Some("Zielbus"))
        .expect("Zielbus-Auswahl fehlt")
        .rect();
    let duck = karten
        .iter()
        .find(|k| k.contains_rect(auswahl))
        .expect("Auswahl steht in keiner Karte");
    let schieber = h
        .query_all_by_role(egui::accesskit::Role::Slider)
        .map(|n| n.rect())
        .find(|r| duck.contains_rect(*r))
        .expect("kein Regler neben der Auswahl");
    assert!(
        (auswahl.width() - schieber.width()).abs() < 0.5
            && (auswahl.left() - schieber.left()).abs() < 0.5,
        "Auswahl {auswahl:?} steht nicht in der Reglerspalte {schieber:?}"
    );
    // Auch die PTT-Zeile endet am Innenrand ihrer Karte.
    let offen = h.get_by_label("OFFEN").rect();
    let ptt = karten
        .iter()
        .find(|k| k.contains_rect(offen))
        .expect("OFFEN steht in keiner Karte");
    assert!(
        offen.left() >= ptt.left() && offen.right() < ptt.right(),
        "OFFEN {offen:?} steht nicht in der Karte {ptt:?}"
    );
    let _ = raster;
}

/// DARSTELLUNG nimmt seine Breite aus dem Inhalt: der längste Text (Autostart)
/// endet mit Abstand vor dem Rahmen, die Bedienung steht in einem Raster auf
/// einer Kante, und die Makrofeld-Knöpfe sind gleich breit.
#[test]
fn darstellung_hat_rand_und_gleiche_kanten() {
    use crosspoint_fenster::modell::{Aktion, Dialog};
    use crosspoint_fenster::thema;
    let min = mindestgroesse();
    for (breite, hoehe, name) in [(ALLTAG.x, ALLTAG.y, "1590"), (min.x, min.y, "Mindestgröße")] {
        let (mut f, _) = beispiel(Thema::Dunkel);
        f.ausfuehren(Aktion::Dialog(Some(Dialog::Darstellung)));
        let mut h = harness(f, (breite, hoehe));
        h.run_steps(6);
        let dialog = h.get_by_label("Dialog DARSTELLUNG").rect();
        // Der Autostart-Text stößt nirgends an: er endet mit einem Rand vor
        // der Kante des Dialogs.
        let text = h
            .get_all_by_label_contains("Beim Anmelden starten")
            .next()
            .expect("Autostart-Text fehlt")
            .rect();
        assert!(
            text.right() <= dialog.right() - thema::SPACE_2,
            "{name}: Text endet bei {} (Dialog bis {})",
            text.right(),
            dialog.right()
        );
        assert!(
            text.left() > dialog.left(),
            "{name}: Text beginnt vor dem Dialog"
        );
        // Theme-Knöpfe und Makrofelder: eine linke Kante, gleiche Breite je
        // Reihe, und beide Reihen enden auf derselben Kante.
        let themen: Vec<egui::Rect> = ["dunkel", "hell", "mono"]
            .iter()
            .map(|n| h.get_by_label(n).rect())
            .collect();
        let makros: Vec<egui::Rect> = ["5", "8", "10", "16", "40", "80"]
            .iter()
            .map(|n| h.get_by_label(n).rect())
            .collect();
        for reihe in [&themen, &makros] {
            let kante = reihe[0].left();
            let breite_je = reihe[0].width();
            for r in reihe.iter() {
                assert!(
                    (r.left() - kante).abs() < 0.5 || r.left() > kante,
                    "{name}: Reihe steht nicht von links nach rechts"
                );
                assert!(
                    (r.width() - breite_je).abs() < 0.5,
                    "{name}: Knöpfe sind {} und {breite_je} breit",
                    r.width()
                );
                assert!(
                    (r.center().y - reihe[0].center().y).abs() < 0.5,
                    "{name}: Knopf steht nicht in seiner Reihe"
                );
            }
        }
        let links = [themen[0].left(), makros[0].left(), text.left()];
        for x in links {
            assert!(
                (x - links[0]).abs() < 0.5,
                "{name}: die Zeilen beginnen bei {links:?}"
            );
        }
        let rechts = [themen[2].right(), makros[5].right(), text.right()];
        for x in rechts {
            assert!(
                (x - rechts[0]).abs() < 0.5,
                "{name}: die Zeilen enden bei {rechts:?}"
            );
        }
        assert!(
            rechts[0] <= dialog.right() - thema::SPACE_2,
            "{name}: die Knöpfe enden am Rahmen"
        );
    }
}

/// Die Version steht unten in der DARSTELLUNG: gedämpft in der Schrift der
/// Beschriftungen, linksbündig im Rahmen unter dem Autostart, nichts scrollt –
/// und die Zahl kommt aus dem Modell (`betrieb::starten` legt sie hinein,
/// `--version` gibt dieselbe aus).
#[test]
fn version_steht_unten_in_der_darstellung() {
    use crosspoint_fenster::modell::{Aktion, Dialog};
    use crosspoint_fenster::thema;
    let zeile = format!("Crosspoint {}", version_der_wurzel());
    let min = mindestgroesse();
    for (breite, hoehe, name) in [(ALLTAG.x, ALLTAG.y, "1590"), (min.x, min.y, "Mindestgröße")] {
        let (mut f, _) = beispiel(Thema::Dunkel);
        f.ausfuehren(Aktion::Dialog(Some(Dialog::Darstellung)));
        let mut h = harness(f, (breite, hoehe));
        h.run_steps(6);
        let fenster = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(breite, hoehe));
        let dialog = h.get_by_label("Dialog DARSTELLUNG").rect();
        assert!(
            fenster.contains_rect(dialog),
            "{name}: Dialog {dialog:?} läuft aus dem Fenster {fenster:?}"
        );
        // Der zugängliche Name der Zeile ist die Zeile selbst.
        let text = h.get_by_label(&zeile).rect();
        assert!(
            dialog.contains_rect(text),
            "{name}: {text:?} steht nicht im Dialog {dialog:?}"
        );
        // Unten: unter dem Autostart-Kästchen, mit Abstand zum Rahmen und
        // linksbündig wie die Beschriftungen darüber.
        let autostart = h
            .get_all_by_label_contains("Beim Anmelden starten")
            .next()
            .expect("Autostart-Text fehlt")
            .rect();
        assert!(
            text.top() > autostart.bottom(),
            "{name}: die Version steht nicht unter dem Autostart ({text:?} / {autostart:?})"
        );
        assert!(
            text.left() >= dialog.left() && text.right() < dialog.right() - thema::SPACE_2,
            "{name}: die Version stößt an den Rahmen ({text:?} / {dialog:?})"
        );
        assert!(
            text.bottom() <= dialog.bottom(),
            "{name}: die Version läuft unten aus dem Dialog ({text:?} / {dialog:?})"
        );
        // Gedämpft heißt: der Token für zurückgenommene Schrift, nicht der
        // für die Beschriftungen.
        assert_eq!(
            textfarbe(&h, &zeile),
            Some(Thema::Dunkel.farben().text_muted),
            "{name}: die Version steht nicht in der gedämpften Farbe"
        );
        assert_eq!(
            h.query_all_by_role(egui::accesskit::Role::ScrollBar)
                .count(),
            0,
            "{name}: Scrollbalken im Dialog"
        );
    }
}

/// Viele Ausgabegeräte: die ECHO-Karte zeigt so viele Zeilen, wie ins Fenster
/// passen, und nennt den Rest – das Modal bleibt im Fenster, statt unten
/// herauszulaufen.
#[test]
fn filter_echo_liste_kuerzt_sich_statt_aus_dem_fenster_zu_laufen() {
    use crosspoint_fenster::modell::Geraete;
    let (mut f, _) = filter_offen(Thema::Dunkel);
    if let Geraete::Fertig(b) = &mut f.o.geraete {
        let mut bestand = (**b).clone();
        for i in 0..12 {
            bestand.senken.push(eintrag(
                &format!("alsa_output.geraet{i}"),
                &format!("Ausgabegerät {i} mit einem langen Namen"),
            ));
        }
        f.o.geraete = Geraete::Fertig(Box::new(bestand));
    }
    let g = mindestgroesse();
    let mut h = harness(f, (g.x, g.y));
    h.run_steps(6);
    let fenster = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(g.x, g.y));
    let dialog = h.get_by_label("Dialog FILTER · B1 · Discord").rect();
    assert!(
        dialog.max.y <= fenster.max.y - crosspoint_fenster::thema::SPACE_2,
        "Dialog {dialog:?} läuft unten aus dem Fenster {fenster:?}"
    );
    // Die Geräte, die Platz haben, stehen da; der Rest wird gezählt.
    let gezeigt = (0..20)
        .filter(|i| {
            h.query_by_label(&format!("Ausgabegerät {i} mit einem langen Namen"))
                .is_some()
        })
        .count();
    let hinweis = (0..20)
        .filter_map(|rest| {
            h.query_by_label(&format!("… {rest} weitere Geräte"))
                .map(|_| rest)
        })
        .next();
    assert!(
        gezeigt < 12,
        "alle zwölf Geräte in der Karte, obwohl das Fenster klein ist"
    );
    assert_eq!(
        hinweis,
        Some(12 - gezeigt),
        "der Hinweis nennt nicht den Rest ({gezeigt} gezeigt)"
    );
}

/// Der Regler im FILTER-Modal lässt sich bedienen: ein Klick setzt den Wert an
/// die Stelle des Zeigers, die Pfeiltasten gehen in kleinen Schritten weiter.
/// Die Schiene ist also nicht nur gemalt, sie trägt auch die Bedienung.
#[test]
fn filter_regler_setzt_den_wert() {
    use crosspoint_fenster::logik::filter_werte;
    use egui_kittest::kittest::NodeT;
    let (f, p) = filter_offen(Thema::Dunkel);
    let mut h = harness(f, (ALLTAG.x, ALLTAG.y));
    h.run_steps(6);
    let wert = || filter_werte(p.buses[5].filter.as_ref().unwrap()).lc_freq;
    let vorher = wert();
    // Grenze steht auf 80 Hz (40..200): ein Klick in die Mitte setzt sie
    // dorthin, nicht an den Anfang. Gesucht ist der Regler, nicht die
    // Beschriftung daneben – beide heißen „Grenze“.
    let regler = h
        .query_all_by_role(egui::accesskit::Role::Slider)
        .find(|n| NodeT::accesskit_node(n).label().as_deref() == Some("Grenze"))
        .expect("Regler Grenze fehlt");
    regler.click();
    h.run_steps(3);
    let nach_klick = wert();
    assert!(
        (nach_klick - 120.0).abs() <= 12.0,
        "Klick setzt {nach_klick} Hz statt etwa 120 (vorher {vorher})"
    );
    // Pfeiltasten: ein Schritt ist der hundertste Teil der Spanne.
    h.key_press(egui::Key::ArrowRight);
    h.run_steps(3);
    assert!(
        wert() > nach_klick,
        "Pfeil rechts erhöht nicht: {} nach {nach_klick}",
        wert()
    );
    h.key_press(egui::Key::ArrowLeft);
    h.key_press(egui::Key::ArrowLeft);
    h.run_steps(3);
    assert!(
        wert() < nach_klick,
        "Pfeil links senkt nicht: {} nach {nach_klick}",
        wert()
    );
}

/// Das TRIM·DELAY-Menü am Master: beide Regler tragen einen Namen (vorher
/// meldete egui sie ohne Beschriftung, weil das Wort nur in der Überschrift
/// stand), und ihre Schiene ist gezeichnet wie im FILTER-Modal. Der Lauf prüft
/// über `Harness`, dass jedes Bedienelement einen Namen hat.
#[test]
fn trim_delay_menue_hat_namen_und_schienen() {
    use crosspoint_fenster::modell::{Aktion, Aufklapp};
    use crosspoint_fenster::thema;
    use egui_kittest::kittest::NodeT;
    let (mut f, _) = beispiel(Thema::Dunkel);
    f.ausfuehren(Aktion::Aufklappen(Some(Aufklapp::TrimDelay)));
    let mut h = harness(f, (1920.0, 1080.0));
    h.run_steps(6);
    assert!(h.unnamed_widgets().is_empty(), "{:?}", h.unnamed_widgets());
    let farben = Thema::Dunkel.farben();
    let schienen = gezeichnete_flaechen(&h, farben.border_strong);
    for name in ["TRIM", "DELAY"] {
        let regler = h
            .query_all_by_role(egui::accesskit::Role::Slider)
            .find(|n| NodeT::accesskit_node(n).label().as_deref() == Some(name))
            .unwrap_or_else(|| panic!("{name}: Regler fehlt oder hat keinen Namen"))
            .rect();
        // Die Schiene beginnt an der linken Kante des Reglers; rechts endet
        // sie, wo das Wertfeld des Menüs anfängt.
        assert!(
            schienen
                .iter()
                .any(|s| (s.left() - regler.left()).abs() < 0.5
                    && (s.center().y - regler.center().y).abs() < 0.5
                    && s.width() > thema::SIZE_FADER_BAHN * 5.0
                    && (s.height() - thema::SIZE_FADER_BAHN).abs() < 0.5),
            "{name}: keine sichtbare Schiene ({regler:?}, {schienen:?})"
        );
    }
}

/// Umbenennen per Doppelklick: Enter und ein Klick daneben übernehmen den
/// Namen, Esc verwirft ihn (Feedback zu 2.0.0: nur Esc schloss das Feld).
#[test]
fn umbenennen_endet_mit_enter_klick_daneben_und_esc() {
    let (f, params) = beispiel(Thema::Dunkel);
    let mut h = harness(f, (1920.0, 1080.0));
    h.run_steps(4);
    let umbenennen = |h: &mut Harness<'static, Fenster>, alt: &str, neu: &str| {
        // Abstand zum letzten Klick, sonst zählt egui ihn zum Doppelklick.
        h.run_steps(12);
        let name = format!("{alt} (Doppelklick: umbenennen)");
        h.get_by_label(&name).click();
        h.run_steps(1);
        h.get_by_label(&name).click();
        h.run_steps(3);
        h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::A);
        h.run_steps(1);
        h.get_by_role(egui::accesskit::Role::TextInput)
            .type_text(neu);
        h.run_steps(2);
    };

    umbenennen(&mut h, "Mikro", "Gesang");
    h.key_press(egui::Key::Enter);
    h.run_steps(3);
    assert_eq!(params.label("strip", 0).as_deref(), Some("Gesang"), "Enter");
    assert!(h.query_by_role(egui::accesskit::Role::TextInput).is_none());

    umbenennen(&mut h, "Gesang", "Sprecher");
    h.get_all_by_label("M").next().expect("M").hover();
    let wo = h.get_all_by_label("M").next().expect("M").rect().center();
    h.event(egui::Event::PointerButton {
        pos: wo,
        button: egui::PointerButton::Primary,
        pressed: true,
        modifiers: egui::Modifiers::NONE,
    });
    h.event(egui::Event::PointerButton {
        pos: wo,
        button: egui::PointerButton::Primary,
        pressed: false,
        modifiers: egui::Modifiers::NONE,
    });
    h.run_steps(3);
    assert_eq!(
        params.label("strip", 0).as_deref(),
        Some("Sprecher"),
        "Klick daneben"
    );
    assert!(h.query_by_role(egui::accesskit::Role::TextInput).is_none());

    umbenennen(&mut h, "Sprecher", "Weg");
    h.key_press(egui::Key::Escape);
    h.run_steps(3);
    assert_eq!(params.label("strip", 0).as_deref(), Some("Sprecher"), "Esc");
    assert!(h.query_by_role(egui::accesskit::Role::TextInput).is_none());
}

/// Feedback zu 2.0.0: „dunkel, hell, mono“ stand an zwei Stellen. Gewählt
/// wird nur noch in DARSTELLUNG; der MIDI-Dialog verlinkt MIDIMonster.
#[test]
fn theme_nur_in_darstellung_und_midimonster_verlinkt() {
    use crosspoint_fenster::modell::{Aktion, Dialog};
    let (f, _) = beispiel(Thema::Dunkel);
    let mut h = harness(f, (1920.0, 1080.0));
    h.run_steps(4);
    for t in ["dunkel", "hell", "mono"] {
        assert!(h.query_by_label(t).is_none(), "{t} steht im Kopf");
    }
    let (mut f, _) = beispiel(Thema::Dunkel);
    f.ausfuehren(Aktion::Dialog(Some(Dialog::Darstellung)));
    let mut h = harness(f, (1920.0, 1080.0));
    h.run_steps(4);
    for t in ["dunkel", "hell", "mono"] {
        assert_eq!(h.get_all_by_label(t).count(), 1, "{t} einmal");
    }
    let (mut f, _) = beispiel(Thema::Dunkel);
    f.ausfuehren(Aktion::Dialog(Some(Dialog::Midi)));
    let mut h = harness(f, (1920.0, 1080.0));
    h.run_steps(4);
    assert!(h.query_by_label("midimonster.net").is_some());
}

/// Feedback zu 2.0.0: in der Startgröße standen A1..B3 nur als „…“ da. Die
/// Zuordnung steht jetzt als Spalte neben dem Fader; jedes Kürzel ist in
/// allen acht Kanälen ausgeschrieben – im Alltag und bei Mindestgröße.
#[test]
fn zuordnung_ist_ausgeschrieben() {
    fn zaehle(shape: &egui::Shape, inhalt: &str) -> usize {
        match shape {
            egui::Shape::Text(t) => usize::from(t.galley.text() == inhalt),
            egui::Shape::Vec(teile) => teile.iter().map(|s| zaehle(s, inhalt)).sum(),
            _ => 0,
        }
    }
    let min = mindestgroesse();
    for (breite, hoehe) in [(ALLTAG.x, ALLTAG.y), (min.x, min.y)] {
        let (f, _) = beispiel(Thema::Dunkel);
        let mut h = harness(f, (breite, hoehe));
        h.run_steps(4);
        for kuerzel in ["A2", "A4", "B2"] {
            let n: usize = h
                .output()
                .shapes
                .iter()
                .map(|cs| zaehle(&cs.shape, kuerzel))
                .sum();
            // Acht Kanäle; Busse tragen ihren Namen zusätzlich im Kopf.
            assert!(n >= 8, "{breite}×{hoehe}: {kuerzel} nur {n}-mal");
        }
    }
}
