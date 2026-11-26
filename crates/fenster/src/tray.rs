//! Tray-Symbol (StatusNotifierItem) für den minimierten Start.
//!
//! Die Menüpunkte wirken direkt auf die Engine (Stumm, Preset, Autostart);
//! was das Fenster betrifft (zeigen, beenden), geht als [`Befehl`] an das
//! Fenster, das ihn im nächsten Bild ausführt. Das Symbol wird aus dem
//! eingebetteten PNG erzeugt – kein Pfadsuchen zur Laufzeit.

use std::sync::atomic::{AtomicBool, Ordering::Relaxed};
use std::sync::mpsc::{Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crosspoint_schnittstelle::Schnittstelle;
use ksni::blocking::TrayMethods;

/// Was das Fenster tun soll.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Befehl {
    Zeigen,
    Beenden,
}

/// „Alles stumm“ schaltet um: sind alle stumm, werden alle wieder hörbar,
/// sonst werden alle stumm. Liefert den neuen Mute-Wert.
pub fn stumm_ziel(mutes: &[bool]) -> bool {
    !(!mutes.is_empty() && mutes.iter().all(|m| *m))
}

/// PNG (RGBA) → ARGB32 in Netzwerk-Bytefolge, wie StatusNotifierItem es will.
pub fn argb(rgba: &[u8]) -> Vec<u8> {
    rgba.as_chunks::<4>()
        .0
        .iter()
        .flat_map(|p| [p[3], p[0], p[1], p[2]])
        .collect()
}

struct Symbol {
    kern: Arc<dyn Schnittstelle>,
    befehle: Sender<Befehl>,
    wecker: Arc<Mutex<Option<egui::Context>>>,
    icon: ksni::Icon,
}

impl Symbol {
    fn senden(&self, b: Befehl) {
        let _ = self.befehle.send(b);
        if let Ok(w) = self.wecker.lock() {
            if let Some(ctx) = w.as_ref() {
                ctx.request_repaint();
            }
        }
    }

    fn mutes(&self) -> Vec<bool> {
        self.kern
            .params()
            .strips
            .iter()
            .map(|s| s.mute.load(Relaxed))
            .collect()
    }
}

impl ksni::Tray for Symbol {
    fn id(&self) -> String {
        "de.weppelmann.Crosspoint".into()
    }

    fn title(&self) -> String {
        "Crosspoint".into()
    }

    fn icon_pixmap(&self) -> Vec<ksni::Icon> {
        vec![self.icon.clone()]
    }

    fn activate(&mut self, _x: i32, _y: i32) {
        self.senden(Befehl::Zeigen);
    }

    fn menu(&self) -> Vec<ksni::MenuItem<Self>> {
        use ksni::menu::{CheckmarkItem, StandardItem};
        let alle_stumm = !stumm_ziel(&self.mutes());
        let autostart = self.kern.ablage().autostart();
        vec![
            StandardItem {
                label: "Fenster zeigen".into(),
                activate: Box::new(|t: &mut Self| t.senden(Befehl::Zeigen)),
                ..Default::default()
            }
            .into(),
            CheckmarkItem {
                label: "Alles stumm".into(),
                checked: alle_stumm,
                activate: Box::new(|t: &mut Self| {
                    let ziel = stumm_ziel(&t.mutes());
                    let params = Arc::clone(t.kern.params());
                    for i in 0..crosspoint_modell::STRIPS {
                        params.set("strip", i, "mute", if ziel { 1.0 } else { 0.0 });
                    }
                }),
                ..Default::default()
            }
            .into(),
            StandardItem {
                label: "Letztes Preset laden".into(),
                activate: Box::new(|t: &mut Self| {
                    let params = Arc::clone(t.kern.params());
                    let pfad = t.kern.preset_pfad();
                    if !t.kern.ablage().preset_laden(&params, &pfad) {
                        eprintln!("crosspoint: kein Preset unter {}", pfad.display());
                    }
                }),
                ..Default::default()
            }
            .into(),
            CheckmarkItem {
                label: "Autostart".into(),
                checked: autostart,
                activate: Box::new(|t: &mut Self| {
                    let an = !t.kern.ablage().autostart();
                    if let Err(e) = t.kern.ablage().autostart_setzen(an) {
                        eprintln!("crosspoint: Autostart: {e}");
                    }
                }),
                ..Default::default()
            }
            .into(),
            ksni::MenuItem::Separator,
            StandardItem {
                label: "Beenden".into(),
                activate: Box::new(|t: &mut Self| t.senden(Befehl::Beenden)),
                ..Default::default()
            }
            .into(),
        ]
    }
}

/// Laufendes Tray-Symbol. Fällt es weg, verschwindet das Symbol.
pub struct Tray {
    _handle: ksni::blocking::Handle<Symbol>,
    befehle: Mutex<Receiver<Befehl>>,
    wecker: Arc<Mutex<Option<egui::Context>>>,
    beenden: AtomicBool,
}

impl Tray {
    /// Das offene Fenster weckt sich, wenn ein Menüpunkt etwas will
    /// (`None`, wenn kein Fenster offen ist).
    pub fn wecker(&self, ctx: Option<egui::Context>) {
        if let Ok(mut w) = self.wecker.lock() {
            *w = ctx;
        }
    }

    /// Nächster Befehl, höchstens `dauer` warten. Ein vorgemerktes
    /// „Beenden“ (aus dem offenen Fenster) kommt hier wieder heraus.
    pub fn warte(&self, dauer: Duration) -> Option<Befehl> {
        if self.beenden.swap(false, Relaxed) {
            return Some(Befehl::Beenden);
        }
        let rx = self.befehle.lock().ok()?;
        if dauer.is_zero() {
            rx.try_recv().ok()
        } else {
            rx.recv_timeout(dauer).ok()
        }
    }

    /// „Beenden“ kam, während ein Fenster offen war: das Fenster schließt,
    /// danach endet das Programm.
    pub fn beenden_vormerken(&self) {
        self.beenden.store(true, Relaxed);
    }
}

/// Symbol anmelden. Fehler (kein D-Bus, kein Watcher) kommen zurück; dann
/// zeigt der Aufrufer das Fenster.
pub fn starten(kern: Arc<dyn Schnittstelle>) -> Result<Tray, String> {
    let bild = eframe::icon_data::from_png_bytes(include_bytes!(
        "../../../data/icons/hicolor/64x64/apps/de.weppelmann.Crosspoint.png"
    ))
    .map_err(|e| e.to_string())?;
    let (tx, rx) = std::sync::mpsc::channel();
    let wecker = Arc::new(Mutex::new(None));
    let symbol = Symbol {
        kern,
        befehle: tx,
        wecker: Arc::clone(&wecker),
        icon: ksni::Icon {
            width: bild.width as i32,
            height: bild.height as i32,
            data: argb(&bild.rgba),
        },
    };
    // Im Flatpak darf kein eigener Busname belegt werden (Sandbox).
    let im_flatpak = std::path::Path::new("/.flatpak-info").exists();
    let handle = symbol
        .disable_dbus_name(im_flatpak)
        .spawn()
        .map_err(|e| e.to_string())?;
    Ok(Tray {
        _handle: handle,
        befehle: Mutex::new(rx),
        wecker,
        beenden: AtomicBool::new(false),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn alles_stumm_schaltet_um() {
        assert!(stumm_ziel(&[false, true]));
        assert!(!stumm_ziel(&[true, true]));
        assert!(stumm_ziel(&[]));
    }

    #[test]
    fn argb_in_netzwerkfolge() {
        assert_eq!(
            argb(&[1, 2, 3, 4, 5, 6, 7, 8]),
            vec![4, 1, 2, 3, 8, 5, 6, 7]
        );
    }
}
