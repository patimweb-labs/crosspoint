//! MIDI-Zuordnung als MIDIMonster-Konfiguration.
//!
//! Kein ALSA-Code in der Oberflaeche: die Zuordnung ist eine Datei
//! `~/.config/crosspoint/midimonster.cfg` in MIDIMonster-Syntax
//! (`cc1.<nr>` als Quelle, `/strip/<n>/gain` als Ziel). MIDIMonster liest sie,
//! uebersetzt MIDI nach OSC und schickt alles an den OSC-Eingang des Kerns.

use std::path::{Path, PathBuf};

/// Seiten des Dialogs (wie im Original).
pub const SEITEN: usize = 6;
/// CC-Nummern der Zusatzzeilen "Control Change n".
pub const FREIE_CCS: usize = 56;

/// Ein Ziel mit optionaler CC-Bindung.
#[derive(Clone, Debug, PartialEq)]
pub struct Zeile {
    /// Beschriftung im Dialog, z. B. "Gain Fader Strip #1".
    pub titel: String,
    /// OSC-Ziel im Kern, z. B. `/strip/0/gain`.
    pub ziel: String,
    /// Gebundene CC-Nummer (None = "-").
    pub cc: Option<u8>,
}

impl Zeile {
    pub fn neu(titel: impl Into<String>, ziel: impl Into<String>) -> Zeile {
        Zeile {
            titel: titel.into(),
            ziel: ziel.into(),
            cc: None,
        }
    }
}

/// Ein- und Ausgabegeraet sowie die Zuordnungen.
#[derive(Clone, Debug, PartialEq)]
pub struct Karte {
    pub name: String,
    pub eingang: String,
    pub ausgang: String,
    pub zeilen: Vec<Zeile>,
}

/// Ziele einer Seite - wie im Original benannt.
pub fn seiten_ziele(seite: usize) -> Vec<Zeile> {
    let s = seite.min(SEITEN - 1);
    let mut z = Vec::new();
    match s {
        0 => {
            for i in 0..8 {
                z.push(Zeile::neu(
                    format!("Gain Fader Strip #{}", i + 1),
                    format!("/strip/{i}/gain"),
                ));
            }
        }
        // Crosspoint hat acht Streifen (0..7); die zweite Seite fuehrt
        // deshalb die acht Busse statt weiterer virtueller Streifen.
        1 => {
            for (i, name) in crosspoint::BUS_NAMES.iter().enumerate() {
                z.push(Zeile::neu(
                    format!("Gain Fader Bus {name}"),
                    format!("/bus/{i}/gain"),
                ));
            }
        }
        2 => {
            for i in 0..8 {
                z.push(Zeile::neu(
                    format!("LR PAN Strip #{}", i + 1),
                    format!("/strip/{i}/pan"),
                ));
            }
        }
        3 => {
            for i in 0..8 {
                z.push(Zeile::neu(
                    format!("Mute Strip #{}", i + 1),
                    format!("/strip/{i}/mute"),
                ));
            }
        }
        4 => {
            for i in 0..8 {
                z.push(Zeile::neu(
                    format!("Solo Strip #{}", i + 1),
                    format!("/strip/{i}/solo"),
                ));
            }
        }
        _ => {
            for i in 0..FREIE_CCS {
                z.push(Zeile::neu(
                    format!("Control Change {i}"),
                    format!("/midi/cc/{i}"),
                ));
            }
        }
    }
    z
}

impl Karte {
    /// Leere Karte mit den Zielen der ersten Seite.
    pub fn leer() -> Karte {
        Karte {
            name: "Crosspoint".into(),
            eingang: KEIN_GERAET.into(),
            ausgang: KEIN_GERAET.into(),
            zeilen: seiten_ziele(0),
        }
    }

    /// Alle Zeilen aller Seiten (die Datei kennt keine Seiten).
    pub fn alle(&mut self) {
        if self.zeilen.len() >= SEITEN * 8 {
            return;
        }
        let mut neu = Vec::new();
        for s in 0..SEITEN {
            for z in seiten_ziele(s) {
                neu.push(z);
            }
        }
        // Bereits gebundene CCs uebernehmen (gleiches Ziel).
        for alt in &self.zeilen {
            if alt.cc.is_some() {
                if let Some(n) = neu.iter_mut().find(|n| n.ziel == alt.ziel) {
                    n.cc = alt.cc;
                }
            }
        }
        self.zeilen = neu;
    }

    /// Ziele einer Seite mit den aktuell gebundenen CCs.
    pub fn seite(&self, seite: usize) -> Vec<Zeile> {
        let mut ziele = seiten_ziele(seite);
        for z in ziele.iter_mut() {
            if let Some(gebunden) = self.zeilen.iter().find(|g| g.ziel == z.ziel) {
                z.cc = gebunden.cc;
            }
        }
        ziele
    }

    /// Bindet eine CC an ein Ziel (ersetzt eine alte Bindung derselben CC).
    pub fn binden(&mut self, ziel: &str, cc: u8) {
        for z in self.zeilen.iter_mut() {
            if z.cc == Some(cc) && z.ziel != ziel {
                z.cc = None;
            }
        }
        if let Some(z) = self.zeilen.iter_mut().find(|z| z.ziel == ziel) {
            z.cc = Some(cc);
        }
    }

    /// Loescht die Bindung einer Zeile.
    pub fn loesen(&mut self, ziel: &str) {
        if let Some(z) = self.zeilen.iter_mut().find(|z| z.ziel == ziel) {
            z.cc = None;
        }
    }

    /// Leert alle Bindungen ("Reset Map").
    pub fn zuruecksetzen(&mut self) {
        for z in self.zeilen.iter_mut() {
            z.cc = None;
        }
    }

    /// Anzahl gebundener Zeilen.
    pub fn gebunden(&self) -> usize {
        self.zeilen.iter().filter(|z| z.cc.is_some()).count()
    }
}

/// Standardpfad: `~/.config/crosspoint/midimonster.cfg`.
pub fn pfad() -> PathBuf {
    crate::konfig::ordner().join("midimonster.cfg")
}

/// Wertet `cc1.<nr>` bzw. `ch<kanal>.cc<nr>` als CC-Nummer aus.
fn cc_aus(quelle: &str) -> Option<u8> {
    let teil = quelle.rsplit('.').next()?;
    let nr = teil
        .strip_prefix("cc")
        .or_else(|| teil.strip_prefix("note"))?;
    nr.trim_matches(|c| c == '{' || c == '}').parse().ok()
}

/// Name der MIDIMonster-Instanz in der erzeugten Datei.
///
/// Fest und ohne Leerzeichen: MIDIMonster erwartet einen Bezeichner, der
/// echte Geraetename (oft mit Leerzeichen) steht in `read`/`write`.
pub const INSTANZ: &str = "controller";

/// Platzhalter, wenn noch kein Geraet gewaehlt ist (wie `data/midimonster.cfg`).
pub const PLATZHALTER: &str = "nanoKONTROL2";

/// Kein Geraet gewaehlt.
pub const KEIN_GERAET: &str = "kein Geraet";

/// Wertebereiche der OSC-Ziele. Ohne sie schickt MIDIMonster 0.0..1.0, und
/// ein Fader stuende in der Engine zwischen 0 und 1 dB.
const BEREICHE: &[&str] = &[
    "/strip/[0-7]/gain = f -60.0 12.0",
    "/bus/[0-7]/gain = f -60.0 12.0",
    "/strip/[0-7]/pan = f -1.0 1.0",
    "/strip/[0-7]/mute = f 0.0 1.0",
    "/strip/[0-7]/solo = f 0.0 1.0",
    "/strip/[0-7]/mono = f 0.0 1.0",
    "/bus/[0-7]/mute = f 0.0 1.0",
    // `[a-b]` ist bei MIDIMonster eine Zeichenklasse (ein Zeichen), keine
    // Zahlenspanne: fuer die mehrstelligen CC-Nummern deshalb `*`.
    "/midi/cc/* = f 0.0 127.0",
];

fn gewaehlt(name: &str) -> Option<&str> {
    let n = name.trim();
    if n.is_empty() || n == KEIN_GERAET {
        None
    } else {
        Some(n)
    }
}

impl Karte {
    /// Datei lesen. Fehlt sie, kommt eine leere Karte.
    ///
    /// Versteht die erzeugte Datei (`read`/`write` im `[midi ...]`-Abschnitt,
    /// `; name = ...`) und die mitgelieferte `data/midimonster.cfg`.
    pub fn laden(p: impl AsRef<Path>) -> Karte {
        let mut k = Karte::leer();
        k.alle();
        let Ok(text) = std::fs::read_to_string(p) else {
            return k;
        };
        let mut abschnitt = String::new();
        let mut instanz: Option<String> = None;
        let mut name: Option<String> = None;
        for roh in text.lines() {
            let zeile = roh.trim();
            if let Some(rest) = zeile.strip_prefix("; name =") {
                name = Some(rest.trim().to_string());
                continue;
            }
            if zeile.starts_with(';') || zeile.is_empty() {
                continue;
            }
            if zeile.starts_with('[') {
                abschnitt = zeile.trim_matches(|c| c == '[' || c == ']').to_string();
                let mut teile = abschnitt.split_whitespace();
                if let (Some("midi"), Some(n)) = (teile.next(), teile.next()) {
                    instanz = Some(n.to_string());
                }
                continue;
            }
            if abschnitt.starts_with("midi") {
                if let Some((schluessel, wert)) = zeile.split_once('=') {
                    match schluessel.trim() {
                        "read" => k.eingang = wert.trim().to_string(),
                        "write" => k.ausgang = wert.trim().to_string(),
                        _ => {}
                    }
                }
                continue;
            }
            if !abschnitt.starts_with("map") || !zeile.contains('>') {
                continue;
            }
            let (von, nach) = zeile
                .split_once("<>")
                .or_else(|| zeile.split_once('>'))
                .unwrap_or((zeile, ""));
            let ziel = nach.trim().replace("crosspoint.", "");
            let cc = cc_aus(von.trim());
            if let (Some(cc), true) = (cc, ziel.starts_with('/')) {
                k.binden(&ziel, cc);
            }
        }
        k.name = name.or(instanz).unwrap_or(k.name);
        k
    }

    /// Datei schreiben (nur gebundene Zeilen landen in `[map]`).
    pub fn speichern(&self, p: impl AsRef<Path>) -> Result<(), String> {
        let p = p.as_ref();
        if let Some(dir) = p.parent() {
            std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        }
        std::fs::write(p, self.als_text()).map_err(|e| format!("{}: {e}", p.display()))
    }

    /// Inhalt der Datei in MIDIMonster-Syntax.
    pub fn als_text(&self) -> String {
        let lesen = gewaehlt(&self.eingang).unwrap_or(PLATZHALTER);
        let schreiben = gewaehlt(&self.ausgang);
        let mut s = String::new();
        s.push_str("; Crosspoint: MIDI -> OSC ueber MIDIMonster\n");
        s.push_str("; Erzeugt von der GTK-Oberflaeche (Dialog MIDI-Mapping).\n");
        s.push_str("; Start: crosspoint, dann midimonster <diese Datei>.\n");
        s.push_str(&format!("; name = {}\n\n", self.name.trim()));
        s.push_str(&format!("[midi {INSTANZ}]\n"));
        s.push_str(&format!("read = {lesen}\n"));
        if let Some(w) = schreiben {
            s.push_str(&format!("write = {w}\n"));
        }
        s.push('\n');
        s.push_str("[osc crosspoint]\n");
        s.push_str("bind = 127.0.0.1 9001\n");
        s.push_str("dest = 127.0.0.1 9000\n");
        for b in BEREICHE {
            s.push_str(b);
            s.push('\n');
        }
        s.push('\n');
        s.push_str("[map]\n");
        s.push_str("; Lernmodus: jede CC geht zusaetzlich als /midi/cc/<nr> an die\n");
        s.push_str("; Engine, die Oberflaeche liest daraus die Nummer.\n");
        s.push_str(&format!(
            "{INSTANZ}.ch0.cc{{0..127}} > crosspoint./midi/cc/{{0..127}}\n"
        ));
        for z in self.zeilen.iter() {
            if let Some(cc) = z.cc {
                s.push_str(&format!("{INSTANZ}.ch0.cc{cc} <> crosspoint.{}\n", z.ziel));
            }
        }
        s
    }
}

/// MIDI-Anschluesse aus `/proc/asound/seq/clients`.
///
/// Liefert die Namen der Clients mit mindestens einem Port, ohne die
/// System-Clients (`System`, `Midi Through`, `PipeWire-*`). Diese Namen
/// nimmt MIDIMonster als `read`/`write` (Praefix des Geraetenamens).
pub fn midi_geraete_aus(text: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for zeile in text.lines() {
        let Some(rest) = zeile.strip_prefix("Client") else {
            continue;
        };
        let Some((_, nach)) = rest.split_once(':') else {
            continue;
        };
        let Some(name) = nach.split('"').nth(1) else {
            continue;
        };
        let system = name == "System"
            || name == "Midi Through"
            || name.starts_with("PipeWire")
            || name.is_empty();
        if !system && !out.iter().any(|n| n == name) {
            out.push(name.to_string());
        }
    }
    out
}

/// Angeschlossene MIDI-Geraete (leer, wenn die Datei fehlt).
pub fn midi_geraete() -> Vec<String> {
    std::fs::read_to_string("/proc/asound/seq/clients")
        .map(|t| midi_geraete_aus(&t))
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("crosspoint-midi-{}-{name}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir.join("midimonster.cfg")
    }

    #[test]
    fn rundlauf_schreiben_und_lesen() {
        let p = temp("rundlauf");
        let mut k = Karte::leer();
        k.alle();
        k.name = "Mein Pult".into();
        k.eingang = "nanoKONTROL2".into();
        k.ausgang = "nanoKONTROL2 Out".into();
        k.binden("/strip/0/gain", 0);
        k.binden("/strip/7/pan", 17);
        k.binden("/strip/6/solo", 43);
        k.speichern(&p).unwrap();

        let gelesen = Karte::laden(&p);
        assert_eq!(gelesen.name, "Mein Pult");
        assert_eq!(gelesen.eingang, "nanoKONTROL2");
        assert_eq!(gelesen.ausgang, "nanoKONTROL2 Out");
        assert_eq!(gelesen.zeilen.len(), k.zeilen.len());
        let cc = |ziel: &str| gelesen.zeilen.iter().find(|z| z.ziel == ziel).unwrap().cc;
        assert_eq!(cc("/strip/0/gain"), Some(0));
        assert_eq!(cc("/strip/7/pan"), Some(17));
        assert_eq!(cc("/strip/6/solo"), Some(43));
        assert_eq!(cc("/strip/1/gain"), None);
        let _ = std::fs::remove_dir_all(p.parent().unwrap());
    }

    #[test]
    fn reset_leert_die_karte_und_die_datei() {
        let p = temp("reset");
        let mut k = Karte::leer();
        k.alle();
        k.binden("/strip/0/gain", 5);
        k.binden("/bus/0/mute", 9);
        k.speichern(&p).unwrap();
        assert!(std::fs::read_to_string(&p).unwrap().contains("cc5"));

        k.zuruecksetzen();
        assert_eq!(k.gebunden(), 0);
        k.speichern(&p).unwrap();
        let text = std::fs::read_to_string(&p).unwrap();
        assert!(
            !text.contains("<>"),
            "nach Reset darf keine Bindung bleiben: {text}"
        );
        assert_eq!(Karte::laden(&p).gebunden(), 0);
        let _ = std::fs::remove_dir_all(p.parent().unwrap());
    }

    #[test]
    fn seiten_haben_die_ziele_des_originals() {
        let s0 = seiten_ziele(0);
        assert_eq!(s0[0].titel, "Gain Fader Strip #1");
        assert_eq!(s0[0].ziel, "/strip/0/gain");
        let s2 = seiten_ziele(2);
        assert_eq!(s2[0].titel, "LR PAN Strip #1");
        assert_eq!(s2[0].ziel, "/strip/0/pan");
        let s3 = seiten_ziele(3);
        assert_eq!(s3[0].titel, "Mute Strip #1");
        let s1 = seiten_ziele(1);
        assert_eq!(s1[0].titel, "Gain Fader Bus A1");
        assert_eq!(s1[7].ziel, "/bus/7/gain");
        let s4 = seiten_ziele(4);
        assert_eq!(s4[0].titel, "Solo Strip #1");
        assert_eq!(s4[0].ziel, "/strip/0/solo");
        let s5 = seiten_ziele(5);
        assert_eq!(s5.len(), FREIE_CCS);
        assert_eq!(s5[55].titel, "Control Change 55");
    }

    #[test]
    fn eine_cc_gehoert_nur_zu_einem_ziel() {
        let mut k = Karte::leer();
        k.alle();
        k.binden("/strip/0/gain", 7);
        k.binden("/strip/1/gain", 7);
        assert_eq!(k.gebunden(), 1);
        assert_eq!(
            k.zeilen
                .iter()
                .find(|z| z.ziel == "/strip/0/gain")
                .unwrap()
                .cc,
            None
        );
        k.loesen("/strip/1/gain");
        assert_eq!(k.gebunden(), 0);
    }

    #[test]
    fn kaputte_datei_ergibt_leere_karte() {
        let p = temp("kaputt");
        std::fs::write(&p, "das ist keine cfg").unwrap();
        let k = Karte::laden(&p);
        assert_eq!(k.gebunden(), 0);
        let _ = std::fs::remove_dir_all(p.parent().unwrap());
    }

    #[test]
    fn alle_ziele_liegen_im_gueltigen_bereich() {
        // Acht Streifen, acht Busse: kein Ziel darf darueber hinaus zeigen.
        for seite in 0..SEITEN {
            for z in seiten_ziele(seite) {
                let teile: Vec<&str> = z.ziel.split('/').filter(|t| !t.is_empty()).collect();
                if let [ziel @ ("strip" | "bus"), n, _] = teile.as_slice() {
                    let n: usize = n.parse().unwrap();
                    let grenze = if *ziel == "strip" {
                        crosspoint::STRIPS
                    } else {
                        crosspoint::BUSES
                    };
                    assert!(n < grenze, "{} zeigt ins Leere", z.ziel);
                }
            }
        }
    }

    #[test]
    fn erzeugte_datei_ist_fuer_midimonster_vollstaendig() {
        let mut k = Karte::leer();
        k.alle();
        k.eingang = "Arturia BeatStep".into();
        k.binden("/strip/0/gain", 7);
        let text = k.als_text();
        // Instanzname ohne Leerzeichen, Geraetename in read/write.
        assert!(text.contains("[midi controller]"), "{text}");
        assert!(text.contains("read = Arturia BeatStep"), "{text}");
        // Kein Ausgang gewaehlt: keine write-Zeile.
        assert!(!text.contains("write ="), "{text}");
        // Ohne Wertebereich stuende der Fader zwischen 0 und 1 dB.
        assert!(text.contains("/strip/[0-7]/gain = f -60.0 12.0"), "{text}");
        // Keine mehrstelligen Zeichenklassen: `[0-127]` hiesse "0, 1, 2 oder 7".
        assert!(!text.contains("[0-127]"), "{text}");
        assert!(text.contains("/midi/cc/* = f 0.0 127.0"), "{text}");
        // Lernmodus braucht die Weiterleitung jeder CC.
        assert!(
            text.contains("controller.ch0.cc{0..127} > crosspoint./midi/cc/{0..127}"),
            "{text}"
        );
        assert!(
            text.contains("controller.ch0.cc7 <> crosspoint./strip/0/gain"),
            "{text}"
        );
    }

    #[test]
    fn mitgelieferte_datei_wird_gelesen() {
        let text = include_str!("../../../data/midimonster.cfg");
        let p = temp("mitgeliefert");
        std::fs::write(&p, text).unwrap();
        let k = Karte::laden(&p);
        assert_eq!(k.eingang, "nanoKONTROL2");
        assert_eq!(k.ausgang, "nanoKONTROL2");
        // Einzelzuordnung cc16 -> /bus/0/gain steht auf Seite 2.
        let cc = k
            .zeilen
            .iter()
            .find(|z| z.ziel == "/bus/0/gain")
            .unwrap()
            .cc;
        assert_eq!(cc, Some(16));
        let _ = std::fs::remove_dir_all(p.parent().unwrap());
    }

    #[test]
    fn midi_geraete_ohne_systemclients() {
        let text = "Client info\n  cur  clients : 4\n\
Client   0 : \"System\" [Kernel Legacy]\n  Port   0 : \"Timer\" (Rwe-) [In/Out]\n\
Client  14 : \"Midi Through\" [Kernel Legacy]\n\
Client  24 : \"nanoKONTROL2\" [Kernel Legacy]\n  Port   0 : \"nanoKONTROL2 _ CTRL\" (RWeX) [In/Out]\n\
Client 128 : \"Arturia BeatStep\" [User Legacy]\n\
Client 144 : \"PipeWire-System\" [User Legacy]\n";
        assert_eq!(
            midi_geraete_aus(text),
            vec!["nanoKONTROL2".to_string(), "Arturia BeatStep".to_string()]
        );
        assert!(midi_geraete_aus("").is_empty());
    }
}
