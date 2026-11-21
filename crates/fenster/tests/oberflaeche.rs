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
        for (breite, hoehe, name) in [
            (2560.0, 1440.0, "2560"),
            (1920.0, 1080.0, "1920"),
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
        // Das FILTER-Modal: die ganze Kette in drei Spalten, bei 1920 und im
        // kleinsten Fenster (die Lämpchenreihe am Zug ist mit drauf).
        for (breite, hoehe, name) in [(1920.0, 1080.0, ""), (min.x, min.y, "-min")] {
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

/// Das FILTER-Modal in drei Spalten: im kleinsten Fenster steht die ganze
/// Kette ohne Scrollen da.
#[test]
fn filter_modal_passt_ins_kleinste_fenster() {
    use crosspoint_fenster::modell::{Aktion, Aufklapp};
    let (mut f, params) = beispiel(Thema::Dunkel);
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
    let g = mindestgroesse();
    let mut h = harness(f, (g.x, g.y));
    h.run_steps(6);
    liegt_im_fenster(
        &h,
        &[
            "FILTER · B1 · Discord",
            "SCHLIESSEN",
            "LC",
            "ECHO",
            "NR",
            "GATE",
            "COMP",
            "LIMITER",
            "AGC",
            "DUCK",
            "PTT",
            "OFFEN",
            "Make-up",
            "Decke",
            "Release",
        ],
    );
    // Drei Spalten: die Karten der ersten Zeile stehen nebeneinander, die
    // zweite Zeile beginnt wieder links (Reihenfolge der Signalkette).
    let spalte = |name: &str| h.get_by_label(name).rect().min.x;
    let (lc, echo, nr, gate) = (
        spalte("LC · LOW-CUT"),
        spalte("ECHO · ECHOUNTERDRÜCKUNG"),
        spalte("NR · RAUSCHUNTERDRÜCKUNG"),
        spalte("GATE · NOISE GATE"),
    );
    assert!(
        lc < echo && echo < nr,
        "nicht drei Spalten: {lc} {echo} {nr}"
    );
    assert!(
        (lc - gate).abs() < 0.5,
        "zweite Zeile beginnt nicht links: {lc} {gate}"
    );
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
    f.ausfuehren(Aktion::MidiSeite(5));
    let g = mindestgroesse();
    let mut h = harness(f, (g.x, g.y));
    h.run_steps(6);
    assert!(h.query_by_label("SAVE MIDI MAP").is_some());
    liegt_im_fenster(&h, &["SAVE MIDI MAP", "SEITE 6/6 · BLATT 1/6"]);
    assert!(h.query_by_label("Control Change 0").is_some());
    assert!(
        h.query_by_label("Control Change 55").is_none(),
        "alle 56 Zeilen auf einem Blatt"
    );
    for _ in 0..5 {
        h.get_by_label("WEITER >>").click();
        h.run_steps(3);
    }
    liegt_im_fenster(&h, &["SEITE 6/6 · BLATT 6/6", "Control Change 50"]);
    assert!(h.query_by_label("Control Change 55").is_some());
    // Über das Ende hinaus geht es vorn weiter.
    h.get_by_label("WEITER >>").click();
    h.run_steps(3);
    liegt_im_fenster(&h, &["SEITE 1/6 · BLATT 1/1"]);
}

/// Auch die übrigen Dialoge stehen im kleinsten Fenster ohne Scrollen da.
#[test]
fn dialoge_passen_ins_kleinste_fenster() {
    use crosspoint_fenster::modell::{Aktion, Dialog};
    for d in Dialog::ALLE {
        let (mut f, _) = beispiel(Thema::Dunkel);
        f.ausfuehren(Aktion::Dialog(Some(d)));
        let g = mindestgroesse();
        let mut h = harness(f, (g.x, g.y));
        h.run_steps(6);
        assert!(h.query_by_label("SCHLIESSEN").is_some(), "{d:?}");
        liegt_im_fenster(&h, &["SCHLIESSEN"]);
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
/// die Innenbreite des Modals von der ersten bis zur letzten Karte.
#[test]
fn filter_raster_hat_gleiche_spalten_und_fuellt_die_breite() {
    let min = mindestgroesse();
    for (breite, hoehe, name) in [
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
        if name == "Mindestgröße" {
            assert_eq!(zeilen[0].len(), 3, "nicht drei Spalten bei Mindestgröße");
        }
        if name == "1920" {
            assert!(zeilen[0].len() >= 3, "zu wenige Spalten bei 1920");
        }
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
