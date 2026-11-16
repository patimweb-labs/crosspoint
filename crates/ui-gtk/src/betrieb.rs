//! Ein Prozess: Engine und Fenster zusammen.
//!
//! Der Ablauf ist bewusst kurz und in dieser Reihenfolge:
//!
//! 1. Engine starten (`crosspoint::lauf::starte`) – sie legt ihre Threads an
//!    (Audio-Callback, HTTP, OSC, Tape, VBAN, Makros, Autosave).
//! 2. GTK im **Hauptthread** starten; das Fenster bekommt die Handles der
//!    Engine ([`crate::Anbindung`]) und schreibt/liest direkt darin.
//! 3. Beenden in beide Richtungen:
//!    * Fenster zu → `Lauf::abschalten`: Aufnahme schliessen, OSC anhalten,
//!      virtuelle Geraete abraeumen, JACK-Client deaktivieren.
//!    * Engine zu (Signal, Leerlauf) → `app.quit()`: das Fenster schliesst.
//!
//! Scheitert der Audio-Client, erscheint statt des Pultes ein Fenster mit der
//! Fehlermeldung – die Oberflaeche zeigt nie ein Pult, das nicht rechnet.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use crosspoint::lauf::Konfig as LaufKonfig;
use gtk::prelude::*;

use crate::{config, dialogs, shell, theme, Anbindung};

/// Kommandozeile (bewusst ohne zusaetzliche Crate).
#[derive(Clone, Debug)]
pub struct Args {
    pub theme: Option<String>,
    pub layout: Option<String>,
    /// Dialog beim Start oeffnen (fuer Belege und zum Ausprobieren).
    pub dialog: Option<String>,
    pub breite: i32,
    pub hoehe: i32,
    /// JACK-Clientname, an dem die Portnamen haengen.
    pub client: String,
    /// Adresse des HTTP-Adapters (bleibt Rand, nicht Bedienweg).
    pub listen: String,
    /// Selbsttest: Fenster nach so vielen Sekunden schliessen. Nimmt denselben
    /// Weg wie der Schliessen-Knopf (`GtkWindow::close` -> `close_request` ->
    /// `Lauf::abschalten`), damit sich das Aufraeumen ohne Fenstermanager
    /// belegen laesst (ein gesendetes WM_DELETE_WINDOW verwirft GDK).
    pub schliessen_nach: Option<f64>,
    /// Ohne Fenster starten, nur Tray-Icon (Autostart). Das Fenster
    /// verschwindet dann beim Schliessen in den Tray, statt zu beenden.
    pub minimiert: bool,
}

impl Default for Args {
    fn default() -> Args {
        Args {
            theme: None,
            layout: None,
            dialog: None,
            breite: 1600,
            hoehe: 920,
            client: "crosspoint".to_string(),
            listen: "127.0.0.1:8770".to_string(),
            schliessen_nach: None,
            minimiert: false,
        }
    }
}

pub const HILFE: &str = "crosspoint [--theme NAME] [--layout NAME] \
[--dialog vban|midi|darstellung] [--size 1600x920] [--client NAME] [--listen ADDR] \
[--minimized (nur Tray-Icon)] [--schliessen-nach SEKUNDEN (Selbsttest)]";

pub fn args_lesen() -> Args {
    let mut a = Args::default();
    let mut it = std::env::args().skip(1);
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--theme" => a.theme = it.next(),
            "--layout" => a.layout = it.next(),
            "--dialog" => a.dialog = it.next(),
            "--client" => {
                if let Some(v) = it.next() {
                    a.client = v;
                }
            }
            "--listen" => {
                if let Some(v) = it.next() {
                    a.listen = v;
                }
            }
            "--size" => {
                if let Some(v) = it.next() {
                    if let Some((w, h)) = v.split_once('x') {
                        a.breite = w.parse().unwrap_or(a.breite);
                        a.hoehe = h.parse().unwrap_or(a.hoehe);
                    }
                }
            }
            "--schliessen-nach" => {
                a.schliessen_nach = it.next().and_then(|v| v.parse::<f64>().ok());
            }
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

/// Startet Engine und Fenster im selben Prozess. Kehrt zurueck, wenn das
/// Fenster zu ist und die Engine abgeschaltet wurde.
pub fn starten(app_id: &str, args: Args) {
    let app = gtk::Application::builder().application_id(app_id).build();
    let args = Rc::new(RefCell::new(args));
    // Ein `activate` kann mehrfach kommen (zweiter Start derselben App);
    // die Engine darf nur einmal hochfahren.
    let gestartet = Rc::new(Cell::new(false));

    {
        let args = Rc::clone(&args);
        let gestartet = Rc::clone(&gestartet);
        app.connect_activate(move |app| {
            if gestartet.replace(true) {
                // Zweiter Start (Menue, Launcher) waehrend das Programm im
                // Tray laeuft: GApplication reicht ihn hierher weiter - dann
                // das vorhandene Fenster zeigen statt einer zweiten Engine.
                if let Some(f) = app.windows().first() {
                    f.set_visible(true);
                    f.present();
                }
                return;
            }
            let a = args.borrow().clone();

            // 1. Engine: eigener Satz Threads, kein Fenster noetig.
            let mut lk = LaufKonfig::vorgabe().dauerbetrieb();
            lk.client_name = a.client.clone();
            lk.listen = a.listen.clone();
            let lauf = match crosspoint::lauf::starte(lk) {
                Ok(l) => Arc::new(l),
                Err(e) => {
                    eprintln!("crosspoint: {e}");
                    shell::fehler_fenster(app, &e);
                    let app = app.clone();
                    gtk::glib::timeout_add_local_once(Duration::from_millis(200), move || {
                        app.quit();
                    });
                    return;
                }
            };

            // 2. Konfiguration der Oberflaeche (Theme/Layout aus ui.json).
            let mut konfig = config::Konfiguration::laden(theme::konfig_pfad());
            if let Some(name) = a.theme.as_deref() {
                let log = konfig.theme.aktiv_setzen(name);
                konfig.log.extend(log);
            }
            if let Some(name) = a.layout.as_deref() {
                let log = konfig.layout.aktiv_setzen(name);
                konfig.log.extend(log);
            }
            for z in &konfig.log {
                eprintln!("crosspoint: {z}");
            }

            // 3. Fenster auf denselben Handles.
            let s = shell::Shell::neu(app, konfig, Anbindung::aus_lauf(&lauf));
            s.fenster.set_default_size(a.breite, a.hoehe);
            // Mindestgroesse bewusst klein: wird das Fenster schmaler als das
            // Pult (Mindestbreite siehe layout::spaltenbreiten), scrollt die
            // Oberflaeche, statt Zuege abzuschneiden. Eine hohe Untergrenze
            // hier wuerde genau das verhindern.
            s.fenster.set_size_request(720, 480);
            // Minimiert: erst das Tray-Icon, das Fenster bleibt verborgen.
            // Nimmt kein Watcher das Icon an, kommt das Fenster doch - ein
            // verborgenes Fenster ohne Tray waere unerreichbar.
            let im_tray = Rc::new(Cell::new(false));
            let tray = if a.minimiert {
                tray_starten(app, &s, &lauf, Rc::clone(&im_tray))
            } else {
                None
            };
            if tray.is_none() {
                s.fenster.present();
            }

            // Selbsttest (Belege): Fenster nach N Sekunden schliessen - derselbe
            // Weg wie der Schliessen-Knopf, inklusive Aufraeumen.
            if let Some(nach) = a.schliessen_nach.filter(|s| *s > 0.0) {
                let f = s.fenster.clone();
                gtk::glib::timeout_add_local_once(
                    Duration::from_millis((nach * 1000.0) as u64),
                    move || {
                        eprintln!("Selbsttest: Fenster schliessen nach {nach} s");
                        f.close();
                    },
                );
            }

            // Fenster zu -> Engine und Geraete weg. Mit Tray: nur verbergen.
            {
                let lauf = Arc::clone(&lauf);
                let im_tray = Rc::clone(&im_tray);
                s.fenster.connect_close_request(move |f| {
                    if im_tray.get() {
                        f.set_visible(false);
                        return gtk::glib::Propagation::Stop;
                    }
                    lauf.abschalten();
                    gtk::glib::Propagation::Proceed
                });
            }
            // Das Tray lebt so lange wie die Anwendung: der Rueckruf haelt es.
            if let Some(t) = tray {
                app.connect_shutdown(move |_| {
                    let _ = &t;
                });
            }
            // Sicherheitsnetz: endet die Anwendung anders (z. B. per Signal),
            // wird trotzdem abgeschaltet.
            {
                let lauf = Arc::clone(&lauf);
                app.connect_shutdown(move |_| {
                    lauf.abschalten();
                });
            }
            // Engine zu (Signal, Leerlauf) -> Fenster zu.
            {
                let lauf = Arc::clone(&lauf);
                let app = app.clone();
                gtk::glib::timeout_add_local(Duration::from_millis(250), move || {
                    if lauf.beendet() {
                        app.quit();
                        gtk::glib::ControlFlow::Break
                    } else {
                        gtk::glib::ControlFlow::Continue
                    }
                });
            }

            if let Some(was) = a.dialog.clone() {
                let s2 = Rc::clone(&s);
                gtk::glib::timeout_add_local_once(Duration::from_millis(600), move || {
                    match was.as_str() {
                        "vban" => {
                            dialogs::vban_fenster(&s2.fenster, &s2.vban);
                        }
                        "midi" => {
                            dialogs::midi_fenster(&s2.fenster);
                        }
                        "darstellung" => {
                            dialogs::darstellung_fenster(&s2);
                        }
                        _ => eprintln!("crosspoint: unbekannter Dialog '{was}'"),
                    }
                });
            }
        });
    }

    // GTK soll die eigenen Argumente nicht pruefen: sie sind oben gelesen.
    // (`--minimized` steht in `args`.)
    let _ = app.run_with_args::<&str>(&[]);
}

/// Tray-Icon fuer den minimierten Start. Menuepunkte wirken direkt auf die
/// Engine; `im_tray` meldet, ob ein Watcher das Icon angenommen hat.
fn tray_starten(
    app: &gtk::Application,
    s: &Rc<shell::Shell>,
    lauf: &Arc<crosspoint::lauf::Lauf>,
    im_tray: Rc<Cell<bool>>,
) -> Option<Rc<crate::tray::Tray>> {
    use crate::tray::{Eintrag, MenueZustand, Rueckrufe};
    let params = lauf.params.clone();
    let zustand = {
        let params = params.clone();
        move || {
            let mutes: Vec<bool> = params
                .strips
                .iter()
                .map(|x| x.mute.load(std::sync::atomic::Ordering::Relaxed))
                .collect();
            MenueZustand {
                alle_stumm: !crate::tray::stumm_ziel(&mutes),
                autostart: crosspoint::autostart::enabled(),
            }
        }
    };
    let aktion = {
        let fenster = s.fenster.clone();
        let app = app.clone();
        let lauf = Arc::clone(lauf);
        move |e: Eintrag| match e {
            Eintrag::Fenster => {
                fenster.set_visible(true);
                fenster.present();
            }
            Eintrag::Stumm => {
                let mutes: Vec<bool> = params
                    .strips
                    .iter()
                    .map(|x| x.mute.load(std::sync::atomic::Ordering::Relaxed))
                    .collect();
                let ziel = crate::tray::stumm_ziel(&mutes);
                for i in 0..mutes.len() {
                    params.set("strip", i, "mute", if ziel { 1.0 } else { 0.0 });
                }
            }
            Eintrag::Preset => {
                if !crosspoint::restore::preset_laden(&params, &lauf.preset) {
                    eprintln!("crosspoint: kein Preset unter {}", lauf.preset.display());
                }
            }
            Eintrag::Autostart => {
                let an = !crosspoint::autostart::enabled();
                if let Err(e) = crosspoint::autostart::set(an) {
                    eprintln!("crosspoint: Autostart: {e}");
                }
            }
            Eintrag::Beenden => {
                lauf.abschalten();
                app.quit();
            }
        }
    };
    let fenster = s.fenster.clone();
    let angenommen = move |ok: bool| {
        im_tray.set(ok);
        if !ok && !fenster.is_visible() {
            fenster.present();
        }
    };
    match crate::tray::starten(
        Rueckrufe {
            zustand: Box::new(zustand),
            aktion: Box::new(aktion),
        },
        angenommen,
    ) {
        Ok(t) => Some(t),
        Err(e) => {
            eprintln!("crosspoint: kein Tray ({e}) - Fenster wird gezeigt");
            None
        }
    }
}
