//! Tray-Icon des nativen Fensters (StatusNotifierItem + com.canonical.dbusmenu).
//!
//! Gegenstueck zu `fenster/fenster.c`, aber im selben Prozess wie Engine und
//! Fenster: die Menuepunkte schreiben direkt in die Parameter statt per HTTP.
//! GDBus kommt mit GTK, eine weitere Bibliothek braucht es nicht.
//!
//! Aufgeteilt in zwei Haelften:
//!
//! * **Modell** ([`Eintrag`], [`MenueZustand`], [`layout`], [`eigenschaften`]):
//!   welche Punkte es gibt, wie sie heissen, welcher Haken gesetzt ist, und
//!   wie das als D-Bus-Wert aussieht. Ohne Bus testbar.
//! * **Anbindung** ([`starten`]): Objekte auf dem Sitzungsbus anmelden und
//!   Klicks an die Aufrufer-Rueckrufe weitergeben.
//!
//! | Menuepunkt            | Wirkung                                        |
//! |-----------------------|------------------------------------------------|
//! | Fenster zeigen        | Fenster einblenden                             |
//! | Alles stumm           | alle Streifen-Mutes umschalten                 |
//! | Letztes Preset laden  | `preset.json` anwenden                         |
//! | Autostart ein/aus     | `~/.config/autostart/…desktop` schreiben/loeschen |
//! | Beenden               | Engine abschalten, Programm beenden            |

use std::rc::Rc;

use gtk::gio;
use gtk::glib;
use gtk::glib::prelude::*;

/// Objektpfad des StatusNotifierItem (so erwartet ihn der Watcher).
pub const SNI_PFAD: &str = "/StatusNotifierItem";
/// Objektpfad des Menues.
pub const MENUE_PFAD: &str = "/MenuBar";
/// Kennung des Programms im Tray.
pub const ID: &str = "de.weppelmann.Crosspoint";

const SNI_XML: &str = "<node><interface name='org.kde.StatusNotifierItem'>\
<property name='Category' type='s' access='read'/>\
<property name='Id' type='s' access='read'/>\
<property name='Title' type='s' access='read'/>\
<property name='Status' type='s' access='read'/>\
<property name='IconName' type='s' access='read'/>\
<property name='IconPixmap' type='a(iiay)' access='read'/>\
<property name='Menu' type='o' access='read'/>\
<property name='ItemIsMenu' type='b' access='read'/>\
<property name='ToolTip' type='(sa(iiay)ss)' access='read'/>\
<method name='ContextMenu'><arg name='x' type='i' direction='in'/><arg name='y' type='i' direction='in'/></method>\
<method name='Activate'><arg name='x' type='i' direction='in'/><arg name='y' type='i' direction='in'/></method>\
<method name='SecondaryActivate'><arg name='x' type='i' direction='in'/><arg name='y' type='i' direction='in'/></method>\
<method name='Scroll'><arg name='delta' type='i' direction='in'/><arg name='orientation' type='s' direction='in'/></method>\
</interface></node>";

const MENUE_XML: &str = "<node><interface name='com.canonical.dbusmenu'>\
<property name='Version' type='u' access='read'/>\
<property name='TextDirection' type='s' access='read'/>\
<property name='Status' type='s' access='read'/>\
<property name='IconThemePath' type='as' access='read'/>\
<method name='GetLayout'><arg type='i' name='parentId' direction='in'/><arg type='i' name='recursionDepth' direction='in'/><arg type='as' name='propertyNames' direction='in'/><arg type='u' name='revision' direction='out'/><arg type='(ia{sv}av)' name='layout' direction='out'/></method>\
<method name='GetGroupProperties'><arg type='ai' name='ids' direction='in'/><arg type='as' name='propertyNames' direction='in'/><arg type='a(ia{sv})' name='properties' direction='out'/></method>\
<method name='GetProperty'><arg type='i' name='id' direction='in'/><arg type='s' name='name' direction='in'/><arg type='v' name='value' direction='out'/></method>\
<method name='Event'><arg type='i' name='id' direction='in'/><arg type='s' name='eventId' direction='in'/><arg type='v' name='data' direction='in'/><arg type='u' name='timestamp' direction='in'/></method>\
<method name='EventGroup'><arg type='a(isvu)' name='events' direction='in'/><arg type='ai' name='idErrors' direction='out'/></method>\
<method name='AboutToShow'><arg type='i' name='id' direction='in'/><arg type='b' name='needUpdate' direction='out'/></method>\
<method name='AboutToShowGroup'><arg type='ai' name='ids' direction='in'/><arg type='ai' name='updatesNeeded' direction='out'/><arg type='ai' name='idErrors' direction='out'/></method>\
<signal name='ItemsPropertiesUpdated'><arg type='a(ia{sv})' name='updatedProps'/><arg type='a(ias)' name='removedProps'/></signal>\
<signal name='LayoutUpdated'><arg type='u' name='revision'/><arg type='i' name='parent'/></signal>\
<signal name='ItemActivationRequested'><arg type='i' name='id'/><arg type='u' name='timestamp'/></signal>\
</interface></node>";

/// Ein Menuepunkt. Die Zahl ist die dbusmenu-Kennung (0 ist die Wurzel).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Eintrag {
    Fenster = 1,
    Stumm = 2,
    Preset = 3,
    Autostart = 4,
    Beenden = 5,
}

impl Eintrag {
    pub const ALLE: [Eintrag; 5] = [
        Eintrag::Fenster,
        Eintrag::Stumm,
        Eintrag::Preset,
        Eintrag::Autostart,
        Eintrag::Beenden,
    ];

    pub fn id(self) -> i32 {
        self as i32
    }

    pub fn aus_id(id: i32) -> Option<Eintrag> {
        Eintrag::ALLE.into_iter().find(|e| e.id() == id)
    }

    pub fn titel(self) -> &'static str {
        match self {
            Eintrag::Fenster => "Fenster zeigen",
            Eintrag::Stumm => "Alles stumm",
            Eintrag::Preset => "Letztes Preset laden",
            Eintrag::Autostart => "Autostart ein/aus",
            Eintrag::Beenden => "Beenden",
        }
    }

    /// Haken-Zustand, falls der Punkt einen Haken hat.
    pub fn haken(self, z: &MenueZustand) -> Option<bool> {
        match self {
            Eintrag::Stumm => Some(z.alle_stumm),
            Eintrag::Autostart => Some(z.autostart),
            _ => None,
        }
    }
}

/// Was das Menue beim Aufklappen zeigen muss.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MenueZustand {
    pub alle_stumm: bool,
    pub autostart: bool,
}

/// "Alles stumm" schaltet um: sind alle stumm, werden alle wieder hoerbar,
/// sonst werden alle stumm. Liefert den neuen Mute-Wert fuer jeden Streifen.
pub fn stumm_ziel(mutes: &[bool]) -> bool {
    !(!mutes.is_empty() && mutes.iter().all(|m| *m))
}

/// Eigenschaften eines Punktes als `a{sv}`. `namen` leer = alle.
pub fn eigenschaften(e: Option<Eintrag>, z: &MenueZustand, namen: &[String]) -> glib::Variant {
    let d = glib::VariantDict::new(None);
    let gefragt = |n: &str| namen.is_empty() || namen.iter().any(|x| x == n);
    match e {
        None => {
            if gefragt("children-display") {
                d.insert_value("children-display", &"submenu".to_variant());
            }
        }
        Some(e) => {
            if gefragt("label") {
                d.insert_value("label", &e.titel().to_variant());
            }
            if gefragt("enabled") {
                d.insert_value("enabled", &true.to_variant());
            }
            if gefragt("visible") {
                d.insert_value("visible", &true.to_variant());
            }
            if let Some(an) = e.haken(z) {
                if gefragt("toggle-type") {
                    d.insert_value("toggle-type", &"checkmark".to_variant());
                }
                if gefragt("toggle-state") {
                    d.insert_value("toggle-state", &i32::from(an).to_variant());
                }
            }
        }
    }
    d.end()
}

fn knoten(id: i32, eig: glib::Variant, kinder: Vec<glib::Variant>) -> glib::Variant {
    let av = glib::Variant::array_from_iter_with_type(
        glib::VariantTy::VARIANT,
        kinder.iter().map(glib::Variant::from_variant),
    );
    glib::Variant::tuple_from_iter([id.to_variant(), eig, av])
}

/// Menuebaum `(ia{sv}av)` ab `eltern` (0 = Wurzel mit allen Punkten).
pub fn layout(eltern: i32, z: &MenueZustand) -> glib::Variant {
    if eltern != 0 {
        let e = Eintrag::aus_id(eltern);
        return knoten(eltern, eigenschaften(e, z, &[]), Vec::new());
    }
    let kinder = Eintrag::ALLE
        .into_iter()
        .map(|e| knoten(e.id(), eigenschaften(Some(e), z, &[]), Vec::new()))
        .collect();
    knoten(0, eigenschaften(None, z, &[]), kinder)
}

/// Rueckrufe in die Anwendung (alle im GTK-Hauptthread).
pub struct Rueckrufe {
    pub zustand: Box<dyn Fn() -> MenueZustand>,
    pub aktion: Box<dyn Fn(Eintrag)>,
}

/// Bild des Icons als `a(iiay)` (ARGB32, Netzwerk-Bytefolge).
fn icon_pixmap() -> glib::Variant {
    let leer = || {
        glib::Variant::array_from_iter_with_type(
            glib::VariantTy::new("(iiay)").expect("Typ"),
            std::iter::empty::<glib::Variant>(),
        )
    };
    let Some(pfad) = icon_pfad() else {
        return leer();
    };
    let Ok(bild) = gtk::gdk_pixbuf::Pixbuf::from_file(&pfad) else {
        return leer();
    };
    let (w, h) = (bild.width(), bild.height());
    let kanaele = bild.n_channels() as usize;
    let schritt = bild.rowstride() as usize;
    let alpha = bild.has_alpha();
    let pixel = bild.read_pixel_bytes();
    let mut argb = Vec::with_capacity((w * h * 4) as usize);
    for y in 0..h as usize {
        for x in 0..w as usize {
            let q = &pixel[y * schritt + x * kanaele..];
            argb.extend_from_slice(&[if alpha { q[3] } else { 0xFF }, q[0], q[1], q[2]]);
        }
    }
    let eintrag = glib::Variant::tuple_from_iter([
        w.to_variant(),
        h.to_variant(),
        glib::Variant::array_from_fixed_array(&argb),
    ]);
    glib::Variant::array_from_iter_with_type(
        glib::VariantTy::new("(iiay)").expect("Typ"),
        [eintrag],
    )
}

/// Icon-Datei: Flatpak-Pfad, installierte Icons, dann das Repo.
pub fn icon_pfad() -> Option<std::path::PathBuf> {
    let name = "de.weppelmann.Crosspoint.png";
    let mut kandidaten = vec![
        std::path::PathBuf::from("/app/share/icons/hicolor/64x64/apps").join(name),
        std::path::PathBuf::from("/usr/share/icons/hicolor/64x64/apps").join(name),
    ];
    if let Ok(exe) = std::env::current_exe() {
        for vorfahr in exe.ancestors().skip(1).take(4) {
            kandidaten.push(vorfahr.join("data/icons/hicolor/64x64/apps").join(name));
        }
    }
    kandidaten.push(std::path::PathBuf::from("data/icons/hicolor/64x64/apps").join(name));
    kandidaten.into_iter().find(|p| p.is_file())
}

/// Laufendes Tray-Icon. Solange es lebt, bleiben die Objekte angemeldet.
pub struct Tray {
    bus: gio::DBusConnection,
    revision: std::cell::Cell<u32>,
}

impl Tray {
    /// Menuezustand hat sich geaendert: Gastgeber holen das Layout neu.
    pub fn menue_geaendert(&self) {
        let r = self.revision.get() + 1;
        self.revision.set(r);
        let _ = self.bus.emit_signal(
            None,
            MENUE_PFAD,
            "com.canonical.dbusmenu",
            "LayoutUpdated",
            Some(&(r, 0i32).to_variant()),
        );
    }
}

/// Meldet das Tray-Icon an. `angenommen` erfaehrt, ob der Watcher es
/// angenommen hat (ohne Watcher waere ein verstecktes Fenster unerreichbar).
pub fn starten(r: Rueckrufe, angenommen: impl Fn(bool) + 'static) -> Result<Rc<Tray>, String> {
    let bus = gio::bus_get_sync(gio::BusType::Session, gio::Cancellable::NONE)
        .map_err(|e| format!("kein Sitzungsbus ({e})"))?;
    let sni = gio::DBusNodeInfo::for_xml(SNI_XML).map_err(|e| e.to_string())?;
    let menue = gio::DBusNodeInfo::for_xml(MENUE_XML).map_err(|e| e.to_string())?;
    let sni_if = sni
        .lookup_interface("org.kde.StatusNotifierItem")
        .ok_or("SNI-Schnittstelle fehlt")?;
    let menue_if = menue
        .lookup_interface("com.canonical.dbusmenu")
        .ok_or("dbusmenu-Schnittstelle fehlt")?;

    let r = Rc::new(r);
    let tray = Rc::new(Tray {
        bus: bus.clone(),
        revision: std::cell::Cell::new(1),
    });

    {
        let r = Rc::clone(&r);
        bus.register_object(SNI_PFAD, &sni_if)
            .method_call(move |_, _, _, _, methode, _, aufruf| {
                if methode == "Activate" || methode == "SecondaryActivate" {
                    (r.aktion)(Eintrag::Fenster);
                }
                aufruf.return_value(None);
            })
            .property(|_, _, _, _, name| match name {
                "Category" => "ApplicationStatus".to_variant(),
                "Id" => ID.to_variant(),
                "Title" => "Crosspoint".to_variant(),
                "Status" => "Active".to_variant(),
                "IconName" => ID.to_variant(),
                "IconPixmap" => icon_pixmap(),
                "Menu" => glib::variant::ObjectPath::try_from(MENUE_PFAD)
                    .expect("Pfad")
                    .to_variant(),
                "ItemIsMenu" => false.to_variant(),
                "ToolTip" => glib::Variant::tuple_from_iter([
                    "Crosspoint".to_variant(),
                    glib::Variant::array_from_iter_with_type(
                        glib::VariantTy::new("(iiay)").expect("Typ"),
                        std::iter::empty::<glib::Variant>(),
                    ),
                    "Crosspoint".to_variant(),
                    "8×8-Mixer".to_variant(),
                ]),
                _ => "".to_variant(),
            })
            .build()
            .map_err(|e| format!("SNI nicht anmeldbar ({e})"))?;
    }
    {
        let r = Rc::clone(&r);
        let t = Rc::downgrade(&tray);
        bus.register_object(MENUE_PFAD, &menue_if)
            .method_call(move |_, _, _, _, methode, p, aufruf| {
                let rev = t.upgrade().map(|t| t.revision.get()).unwrap_or(1);
                match methode {
                    "GetLayout" => {
                        let eltern = p.child_value(0).get::<i32>().unwrap_or(0);
                        let z = (r.zustand)();
                        let antwort =
                            glib::Variant::tuple_from_iter([rev.to_variant(), layout(eltern, &z)]);
                        aufruf.return_value(Some(&antwort));
                    }
                    "GetGroupProperties" => {
                        let ids = p.child_value(0).get::<Vec<i32>>().unwrap_or_default();
                        let namen = p.child_value(1).get::<Vec<String>>().unwrap_or_default();
                        let z = (r.zustand)();
                        let liste = ids.into_iter().filter_map(|id| {
                            let e = Eintrag::aus_id(id);
                            (id == 0 || e.is_some()).then(|| {
                                glib::Variant::tuple_from_iter([
                                    id.to_variant(),
                                    eigenschaften(e, &z, &namen),
                                ])
                            })
                        });
                        let arr = glib::Variant::array_from_iter_with_type(
                            glib::VariantTy::new("(ia{sv})").expect("Typ"),
                            liste,
                        );
                        aufruf.return_value(Some(&glib::Variant::tuple_from_iter([arr])));
                    }
                    "GetProperty" => {
                        let id = p.child_value(0).get::<i32>().unwrap_or(-1);
                        let name = p.child_value(1).get::<String>().unwrap_or_default();
                        let z = (r.zustand)();
                        let eig = eigenschaften(Eintrag::aus_id(id), &z, &[]);
                        let wert = glib::VariantDict::new(Some(&eig))
                            .lookup_value(&name, None)
                            .unwrap_or_else(|| "".to_variant());
                        aufruf.return_value(Some(&glib::Variant::tuple_from_iter([
                            glib::Variant::from_variant(&wert),
                        ])));
                    }
                    "Event" => {
                        let id = p.child_value(0).get::<i32>().unwrap_or(-1);
                        let ereignis = p.child_value(1).get::<String>().unwrap_or_default();
                        aufruf.return_value(None);
                        if ereignis == "clicked" {
                            if let Some(e) = Eintrag::aus_id(id) {
                                (r.aktion)(e);
                                if let Some(t) = t.upgrade() {
                                    t.menue_geaendert();
                                }
                            }
                        }
                    }
                    "EventGroup" => {
                        let leer: Vec<i32> = Vec::new();
                        aufruf.return_value(Some(&(leer,).to_variant()));
                    }
                    "AboutToShow" => aufruf.return_value(Some(&(true,).to_variant())),
                    "AboutToShowGroup" => {
                        let leer: Vec<i32> = Vec::new();
                        aufruf.return_value(Some(&(leer.clone(), leer).to_variant()));
                    }
                    _ => aufruf.return_dbus_error(
                        "org.freedesktop.DBus.Error.UnknownMethod",
                        "unbekannte Methode",
                    ),
                }
            })
            .property(|_, _, _, _, name| match name {
                "Version" => 3u32.to_variant(),
                "TextDirection" => "ltr".to_variant(),
                "Status" => "normal".to_variant(),
                "IconThemePath" => Vec::<String>::new().to_variant(),
                _ => "".to_variant(),
            })
            .build()
            .map_err(|e| format!("Menue nicht anmeldbar ({e})"))?;
    }

    let name = format!("org.kde.StatusNotifierItem-{}-1", std::process::id());
    let angenommen = Rc::new(angenommen);
    let a2 = Rc::clone(&angenommen);
    gio::bus_own_name_on_connection(
        &bus,
        &name,
        gio::BusNameOwnerFlags::NONE,
        move |verbindung, name| {
            let ok = verbindung
                .call_sync(
                    Some("org.kde.StatusNotifierWatcher"),
                    "/StatusNotifierWatcher",
                    "org.kde.StatusNotifierWatcher",
                    "RegisterStatusNotifierItem",
                    Some(&(name,).to_variant()),
                    None,
                    gio::DBusCallFlags::NONE,
                    3000,
                    gio::Cancellable::NONE,
                )
                .map_err(|e| eprintln!("crosspoint: Tray nicht angenommen ({e})"))
                .is_ok();
            if ok {
                eprintln!("crosspoint: Tray-Icon angemeldet als {name}");
            }
            angenommen(ok);
        },
        move |_, _| {
            eprintln!("crosspoint: Tray-Name verloren");
            a2(false);
        },
    );
    Ok(tray)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kennungen_sind_eindeutig_und_umkehrbar() {
        for e in Eintrag::ALLE {
            assert_eq!(Eintrag::aus_id(e.id()), Some(e));
            assert_ne!(e.id(), 0, "0 ist die Wurzel");
        }
        assert_eq!(Eintrag::aus_id(0), None);
        assert_eq!(Eintrag::aus_id(99), None);
    }

    #[test]
    fn stumm_schaltet_um() {
        assert!(
            stumm_ziel(&[false, true, false]),
            "teils stumm -> alle stumm"
        );
        assert!(!stumm_ziel(&[true, true]), "alle stumm -> alle hoerbar");
        assert!(stumm_ziel(&[]));
    }

    #[test]
    fn layout_hat_die_form_von_dbusmenu() {
        let z = MenueZustand {
            alle_stumm: true,
            autostart: false,
        };
        let l = layout(0, &z);
        assert_eq!(l.type_().as_str(), "(ia{sv}av)");
        let kinder = l.child_value(2);
        assert_eq!(kinder.n_children(), Eintrag::ALLE.len());
        // Zweiter Punkt: "Alles stumm" mit gesetztem Haken.
        let stumm = kinder.child_value(1).as_variant().expect("v");
        assert_eq!(stumm.child_value(0).get::<i32>(), Some(Eintrag::Stumm.id()));
        let eig = glib::VariantDict::new(Some(&stumm.child_value(1)));
        assert_eq!(
            eig.lookup_value("label", None)
                .and_then(|v| v.get::<String>()),
            Some("Alles stumm".into())
        );
        assert_eq!(
            eig.lookup_value("toggle-state", None)
                .and_then(|v| v.get::<i32>()),
            Some(1)
        );
    }

    #[test]
    fn eigenschaften_liefern_nur_das_gefragte() {
        let z = MenueZustand::default();
        let eig = eigenschaften(Some(Eintrag::Autostart), &z, &["label".to_string()]);
        let d = glib::VariantDict::new(Some(&eig));
        assert!(d.lookup_value("label", None).is_some());
        assert!(d.lookup_value("toggle-state", None).is_none());
        // Ohne Haken kein toggle-type.
        let eig = eigenschaften(Some(Eintrag::Beenden), &z, &[]);
        let d = glib::VariantDict::new(Some(&eig));
        assert!(d.lookup_value("toggle-type", None).is_none());
    }
}
