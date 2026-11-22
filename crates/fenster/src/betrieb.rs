//! Ein Prozess: Engine und Fenster zusammen.
//!
//! 1. Engine starten (`crosspoint::lauf::starte`) – eigene Threads für Audio,
//!    HTTP, OSC, Tape, VBAN, Makros, Autosave.
//! 2. Fenster (eframe) im Hauptthread auf denselben Handles.
//! 3. Beenden in beide Richtungen: Fenster zu → `Lauf::abschalten` (mit Tray:
//!    nur verbergen); Engine zu (Signal, Leerlauf) → Fenster zu.
//!
//! Scheitert der Audio-Client, zeigt das Fenster statt des Pults die
//! Fehlermeldung – nie ein Pult, das nicht rechnet.

use std::sync::Arc;
use std::time::{Duration, Instant};

use eframe::egui;

use crate::app::{Fenster, Umgebung};
use crate::logik::Kern;
use crate::modell::{Dialog, Oberflaeche};
use crate::thema::{self, Thema};
use crate::{geraete, konfig, tray};

/// Kommandozeile (bewusst ohne zusätzliche Crate).
#[derive(Clone, Debug)]
pub struct Args {
    pub theme: Option<String>,
    pub dialog: Option<String>,
    pub breite: f32,
    pub hoehe: f32,
    pub client: String,
    pub listen: String,
    /// Selbsttest: jedes Fenster nach so vielen Sekunden schließen (derselbe
    /// Weg wie der Schließen-Knopf; ohne Tray endet damit das Programm).
    pub schliessen_nach: Option<f64>,
    /// Ohne Fenster starten, nur Tray-Icon (Autostart).
    pub minimiert: bool,
}

impl Default for Args {
    fn default() -> Args {
        Args {
            theme: None,
            dialog: None,
            breite: 1600.0,
            hoehe: 920.0,
            client: "crosspoint".to_string(),
            listen: "127.0.0.1:8770".to_string(),
            schliessen_nach: None,
            minimiert: false,
        }
    }
}

pub const HILFE: &str = "crosspoint [--theme dark|light|mono] \
[--dialog presets|vban|midi|darstellung] [--size 1600x920] [--client NAME] [--listen ADDR] \
[--minimized (nur Tray-Icon)] [--schliessen-nach SEKUNDEN (Selbsttest)] [--version]";

/// `--version`/`-V`: eine Zeile ausgeben und `true` melden – **vor** jeder
/// Wirkung (kein JACK, keine Instanzprüfung, kein Fenster).
///
/// Die Zahl kommt vom Wurzelpaket (`env!("CARGO_PKG_VERSION")` im Binary):
/// dieses Crate kennt seine Verpackung nicht. Ohne die Option wird nichts
/// ausgegeben und `false` gemeldet.
pub fn version_ausgeben(version: &str, args: impl Iterator<Item = String>) -> bool {
    if !args.into_iter().any(|a| a == "--version" || a == "-V") {
        return false;
    }
    println!("crosspoint {version}");
    true
}

pub fn args_aus(mut it: impl Iterator<Item = String>) -> Args {
    let mut a = Args::default();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--theme" => a.theme = it.next(),
            "--dialog" => a.dialog = it.next(),
            "--client" => a.client = it.next().unwrap_or(a.client),
            "--listen" => a.listen = it.next().unwrap_or(a.listen),
            "--size" => {
                if let Some((w, h)) = it.next().as_deref().and_then(|v| v.split_once('x')) {
                    a.breite = w.parse().unwrap_or(a.breite);
                    a.hoehe = h.parse().unwrap_or(a.hoehe);
                }
            }
            "--schliessen-nach" => a.schliessen_nach = it.next().and_then(|v| v.parse().ok()),
            "--minimized" | "-m" => a.minimiert = true,
            "--help" | "-h" => {
                println!("{HILFE}");
                std::process::exit(0);
            }
            _ => {}
        }
    }
    a
}

pub fn args_lesen() -> Args {
    args_aus(std::env::args().skip(1))
}

/// Symbol des Fensters (PNG aus `assets/crosspoint-logo.svg`, ADR 0008).
fn symbol() -> egui::IconData {
    eframe::icon_data::from_png_bytes(include_bytes!(
        "../../../data/icons/hicolor/256x256/apps/de.weppelmann.Crosspoint.png"
    ))
    .unwrap_or_default()
}

/// Was nach dem Schließen eines Fensters geschieht.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Danach {
    /// Fenster wieder öffnen (Tray „Fenster zeigen“, zweiter Start).
    Zeigen,
    /// Programm beenden.
    Ende,
}

/// Entscheidet nach dem Schließen: ohne Tray ist Schluss, mit Tray wird
/// gewartet, bis jemand das Fenster will, „Beenden“ wählt oder die Engine
/// endet. `befehl` liefert den nächsten Tray-Befehl (blockiert kurz).
pub fn danach(
    hat_tray: bool,
    beendet: impl Fn() -> bool,
    mut befehl: impl FnMut() -> Option<tray::Befehl>,
    gewuenscht: impl Fn() -> bool,
) -> Danach {
    if !hat_tray {
        return Danach::Ende;
    }
    loop {
        if beendet() {
            return Danach::Ende;
        }
        if gewuenscht() {
            return Danach::Zeigen;
        }
        match befehl() {
            Some(tray::Befehl::Zeigen) => return Danach::Zeigen,
            Some(tray::Befehl::Beenden) => return Danach::Ende,
            None => {}
        }
    }
}

/// Startet Engine und Fenster. Kehrt zurück, wenn beides zu ist.
///
/// Mit Tray (`--minimized`) schließt der Fensterknopf nur das Fenster; Engine
/// und Symbol laufen weiter, „Fenster zeigen“ öffnet ein neues. So geht es
/// unter Wayland und X11 gleich – ein Fenster *verstecken* kann winit unter
/// Wayland nicht.
pub fn starten(app_id: &str, version: &str, a: Args) {
    // Läuft schon eine Instanz, zeigt sie ihr Fenster – eine zweite Engine
    // gäbe es sonst doppelt (gleicher Port, gleiche virtuelle Geräte).
    if laufende_instanz_wecken(&a.listen, a.minimiert) {
        eprintln!("crosspoint: läuft schon – Fenster der laufenden Instanz");
        return;
    }
    let mut lk = crosspoint::lauf::Konfig::vorgabe().dauerbetrieb();
    lk.client_name = a.client.clone();
    lk.listen = a.listen.clone();
    let lauf = match crosspoint::lauf::starte(lk) {
        Ok(l) => Arc::new(l),
        Err(e) => {
            eprintln!("crosspoint: {e}");
            let thema = a.theme.as_deref().map(Thema::aus_name).unwrap_or_default();
            fenster_laufen(app_id, &a, move |_| Box::new(Fehler { text: e, thema }));
            return;
        }
    };

    // Tray nur beim minimierten Start (wie bisher). Nimmt niemand das Symbol
    // an, kommt das Fenster doch – sonst wäre das Pult unerreichbar.
    let tray = if a.minimiert {
        match tray::starten(Arc::clone(&lauf)) {
            Ok(t) => Some(Arc::new(t)),
            Err(e) => {
                eprintln!("crosspoint: kein Tray ({e}) – Fenster wird gezeigt");
                None
            }
        }
    } else {
        None
    };

    let mut zeigen = tray.is_none();
    let mut erstes = true;
    loop {
        if zeigen {
            // Die Version der Anwendung, nicht die dieses Crates: sie kommt
            // aus dem Binary (Wurzelpaket) und steht unten in der DARSTELLUNG.
            let mut o = Oberflaeche {
                version: Some(version.to_string()),
                ..Oberflaeche::default()
            };
            konfig::Datei::laden(&konfig::pfad()).anwenden(&mut o);
            o.echo_geraete = crosspoint::echo::auswahl();
            if let Some(t) = a.theme.as_deref() {
                o.thema = Thema::aus_name(t);
            }
            // Ein Dialog öffnet über dieselbe Aktion wie der Knopf – nur so
            // füllt er sein Formular aus dem Kern (VBAN, MIDI).
            let mut start = None;
            if erstes {
                if let Some(d) = a.dialog.as_deref() {
                    start = Dialog::aus_name(d);
                    if start.is_none() {
                        eprintln!("crosspoint: unbekannter Dialog '{d}'");
                    }
                }
            }
            // Selbsttest: jedes Fenster schließt nach der Frist (auch ein
            // wieder geöffnetes) – so lässt sich der Tray-Weg prüfen.
            let schliessen = a.schliessen_nach;
            erstes = false;
            let (l, t) = (Arc::clone(&lauf), tray.clone());
            fenster_laufen(app_id, &a, move |ctx| {
                Box::new(Pult::neu(l, o, t, schliessen, start, ctx))
            });
        }
        let weiter = danach(
            tray.is_some(),
            || lauf.beendet(),
            || {
                tray.as_ref()
                    .and_then(|t| t.warte(Duration::from_millis(200)))
            },
            crosspoint::http::fenster_gewuenscht,
        );
        match weiter {
            Danach::Zeigen => zeigen = true,
            Danach::Ende => break,
        }
    }
    lauf.abschalten();
}

/// Fragt die Steuer-API unter `adresse` an. Antwortet eine Instanz, bekommt
/// sie (außer bei `--minimized`) den Fensterwunsch; Ergebnis: es lief schon
/// eine.
pub fn laufende_instanz_wecken(adresse: &str, minimiert: bool) -> bool {
    use std::io::{Read, Write};
    use std::net::{TcpStream, ToSocketAddrs};
    let Some(ziel) = adresse.to_socket_addrs().ok().and_then(|mut a| a.next()) else {
        return false;
    };
    let Ok(mut s) = TcpStream::connect_timeout(&ziel, Duration::from_millis(300)) else {
        return false;
    };
    let _ = s.set_read_timeout(Some(Duration::from_secs(1)));
    let anfrage = if minimiert {
        format!("GET /api/state HTTP/1.1\r\nHost: {adresse}\r\nConnection: close\r\n\r\n")
    } else {
        format!(
            "POST /api/fenster HTTP/1.1\r\nHost: {adresse}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
        )
    };
    if s.write_all(anfrage.as_bytes()).is_err() {
        return false;
    }
    let mut antwort = [0u8; 12];
    matches!(s.read(&mut antwort), Ok(n) if n >= 12 && antwort.starts_with(b"HTTP/1.") && &antwort[9..12] == b"200")
}

/// Ein Fensterlauf (kehrt zurück, wenn das Fenster zu ist).
fn fenster_laufen(
    app_id: &str,
    a: &Args,
    app: impl FnOnce(&egui::Context) -> Box<dyn eframe::App> + 'static,
) {
    let optionen = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Crosspoint")
            .with_app_id(app_id)
            .with_icon(Arc::new(symbol()))
            .with_inner_size([a.breite, a.hoehe])
            // Kein Scrollen (ADR 0007): die Mindestgröße kommt aus den Maßen
            // des Pults (`logik`), nicht aus einer geratenen Zahl.
            .with_min_inner_size(crate::logik::mindestfenster()),
        // Erlaubt einen zweiten Lauf im selben Prozess (Tray → Fenster).
        run_and_return: true,
        ..Default::default()
    };
    let ergebnis = eframe::run_native(
        "Crosspoint",
        optionen,
        Box::new(move |cc| {
            crate::einrichten(&cc.egui_ctx);
            Ok(app(&cc.egui_ctx))
        }),
    );
    if let Err(e) = ergebnis {
        eprintln!("crosspoint: Fenster: {e}");
    }
}

/// Das Pult mit Lebenszyklus (Tray-Befehle, Engine-Ende, Selbsttest).
struct Pult {
    fenster: Fenster,
    lauf: Arc<crosspoint::lauf::Lauf>,
    tray: Option<Arc<tray::Tray>>,
    schliessen_um: Option<Instant>,
    ende_gemeldet: bool,
}

impl Pult {
    fn neu(
        lauf: Arc<crosspoint::lauf::Lauf>,
        o: Oberflaeche,
        tray: Option<Arc<tray::Tray>>,
        schliessen_nach: Option<f64>,
        start: Option<Dialog>,
        ctx: &egui::Context,
    ) -> Pult {
        let kern = Kern {
            params: lauf.params.clone(),
            tape: Some(lauf.tape.clone()),
            makros: Some(lauf.makros.clone()),
            tape_ordner: lauf.tape_ordner.clone(),
            preset_ordner: konfig::preset_ordner(),
            zuordner: Box::new(geraete::KernZuordner),
            vban: Some((lauf.vban.clone(), lauf.vban_recv.clone())),
            sample_rate: lauf.sample_rate,
            midi_pfad: crate::midimonster::pfad(),
        };
        let laeuft = {
            let pegel = lauf.pegel.clone();
            let stand = std::sync::Mutex::new((0u64, Instant::now()));
            Box::new(move || {
                // Läuft heißt: der Audio-Callback zählt Blöcke.
                let b = pegel.bloecke();
                let Ok(mut s) = stand.lock() else {
                    return false;
                };
                if b != s.0 {
                    *s = (b, Instant::now());
                }
                s.1.elapsed() < Duration::from_secs(1)
            }) as Box<dyn Fn() -> bool>
        };
        let umgebung = Umgebung {
            pegel: Some(lauf.pegel.clone()),
            laeuft,
            geraete: Some(geraete::laden),
            konfig_pfad: Some(konfig::pfad()),
        };
        if let Some(t) = &tray {
            t.wecker(Some(ctx.clone()));
        }
        let mut fenster = Fenster::neu(kern, o, umgebung);
        if start.is_some() {
            fenster.ausfuehren(crate::modell::Aktion::Dialog(start));
        }
        Pult {
            ende_gemeldet: false,
            fenster,
            lauf,
            tray,
            schliessen_um: schliessen_nach
                .filter(|s| *s > 0.0)
                .map(|s| Instant::now() + Duration::from_secs_f64(s)),
        }
    }
}

impl Drop for Pult {
    fn drop(&mut self) {
        if let Some(t) = &self.tray {
            t.wecker(None);
        }
    }
}

impl eframe::App for Pult {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        // Engine zu → Fenster zu (mit Grund im Protokoll).
        if self.lauf.beendet() {
            if !self.ende_gemeldet {
                self.ende_gemeldet = true;
                eprintln!("crosspoint: Engine beendet - Fenster schliesst");
            }
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        if self.schliessen_um.is_some_and(|t| Instant::now() >= t) {
            eprintln!("Selbsttest: Fenster schliessen");
            self.schliessen_um = None;
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        let mut nach_vorn = crosspoint::http::fenster_gewuenscht();
        if let Some(t) = &self.tray {
            while let Some(b) = t.warte(Duration::ZERO) {
                match b {
                    tray::Befehl::Zeigen => nach_vorn = true,
                    tray::Befehl::Beenden => {
                        t.beenden_vormerken();
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                }
            }
        }
        if nach_vorn {
            ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
            ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
        }
        self.fenster.bild(ui);
    }
}

/// Fenster mit der Fehlermeldung, wenn die Engine nicht startet.
struct Fehler {
    text: String,
    thema: Thema,
}

impl eframe::App for Fehler {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        thema::anwenden(ui.ctx(), self.thema);
        let f = self.thema.farben();
        egui::CentralPanel::default()
            .frame(
                egui::Frame::new()
                    .fill(f.bg_canvas)
                    .inner_margin(thema::SPACE_5),
            )
            .show(ui, |ui| {
                ui.heading("Crosspoint startet nicht");
                ui.label(&self.text);
                ui.label(
                    egui::RichText::new("Läuft PipeWire mit pipewire-jack?").color(f.text_muted),
                );
            });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ohne_tray_endet_das_programm_mit_dem_fenster() {
        assert_eq!(danach(false, || false, || None, || true), Danach::Ende);
    }

    #[test]
    fn mit_tray_wartet_es_auf_zeigen_oder_beenden() {
        let mut befehle = vec![None, None, Some(tray::Befehl::Zeigen)].into_iter();
        assert_eq!(
            danach(true, || false, || befehle.next().flatten(), || false),
            Danach::Zeigen
        );
        let mut befehle = vec![Some(tray::Befehl::Beenden)].into_iter();
        assert_eq!(
            danach(true, || false, || befehle.next().flatten(), || false),
            Danach::Ende
        );
        // Zweiter Start (POST /api/fenster) öffnet ebenfalls.
        assert_eq!(danach(true, || false, || None, || true), Danach::Zeigen);
        // Engine zu: Schluss, auch mit Tray.
        assert_eq!(danach(true, || true, || None, || true), Danach::Ende);
    }

    #[test]
    fn ohne_laufende_instanz_wird_gestartet() {
        // Freier Port: niemand antwortet.
        let frei = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let adresse = frei.local_addr().unwrap().to_string();
        drop(frei);
        assert!(!laufende_instanz_wecken(&adresse, false));
    }

    #[test]
    fn laufende_instanz_bekommt_den_fensterwunsch() {
        use std::io::{Read, Write};
        let server = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let adresse = server.local_addr().unwrap().to_string();
        let t = std::thread::spawn(move || {
            let (mut c, _) = server.accept().unwrap();
            let mut buf = [0u8; 256];
            let n = c.read(&mut buf).unwrap();
            c.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n")
                .unwrap();
            String::from_utf8_lossy(&buf[..n]).to_string()
        });
        assert!(laufende_instanz_wecken(&adresse, false));
        assert!(t.join().unwrap().starts_with("POST /api/fenster"));
    }

    #[test]
    fn argumente() {
        let a = args_aus(
            [
                "--theme",
                "mono",
                "--size",
                "1200x700",
                "--minimized",
                "--dialog",
                "vban",
            ]
            .into_iter()
            .map(String::from),
        );
        assert_eq!(a.theme.as_deref(), Some("mono"));
        assert_eq!((a.breite, a.hoehe), (1200.0, 700.0));
        assert!(a.minimiert);
        assert_eq!(a.dialog.as_deref(), Some("vban"));
        let leer = args_aus(std::iter::empty());
        assert_eq!(leer.client, "crosspoint");
        assert!(!leer.minimiert);
        // Ohne `--version` gibt `version_ausgeben` nichts aus und meldet
        // `false` – der Start geht weiter.
        assert!(!version_ausgeben(
            "1.0.0",
            ["--theme", "mono"].into_iter().map(String::from)
        ));
    }
}
