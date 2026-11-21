//! Verzeichnis der installierten LV2-Plugins – ohne `liblilv`.
//!
//! Für die Insert-Ketten braucht Crosspoint je Plugin nur wenig: URI, Name,
//! die Symbole der Audio-Ein- und -Ausgänge (daraus heißen die JACK-Ports von
//! mod-host: `effect_<n>:<symbol>`) und die Regler mit Bereich und Vorgabe.
//! Das steht in den Turtle-Dateien der Bundles; ein kleiner Leser reicht,
//! eine ganze RDF-Bibliothek wäre für diese Auswahl zu viel.
//!
//! Gesucht wird in `LV2_PATH` (wie bei jedem LV2-Host), sonst in den
//! üblichen Orten von Fedora, Debian und Flatpak. Was der Leser nicht
//! versteht, wird übersprungen – ein seltsames Bundle verhindert nie die
//! Liste der anderen.

use std::path::{Path, PathBuf};

use crate::json::Value;

/// Ein Regler (Steuereingang) eines Plugins.
#[derive(Clone, Debug, PartialEq)]
pub struct Regler {
    pub symbol: String,
    pub name: String,
    pub min: f32,
    pub max: f32,
    pub vorgabe: f32,
}

/// Ein Plugin aus dem Verzeichnis.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Plugin {
    pub uri: String,
    pub name: String,
    /// Audio-Eingänge in Indexreihenfolge (Symbole).
    pub ein: Vec<String>,
    /// Audio-Ausgänge in Indexreihenfolge (Symbole).
    pub aus: Vec<String>,
    pub regler: Vec<Regler>,
}

impl Plugin {
    /// Taugt es als Insert? Mindestens ein Ein- und ein Ausgang.
    pub fn ist_effekt(&self) -> bool {
        !self.ein.is_empty() && !self.aus.is_empty()
    }

    pub fn als_json(&self) -> Value {
        Value::obj(vec![
            ("uri", Value::str(&self.uri)),
            ("name", Value::str(&self.name)),
            ("ein", Value::Arr(self.ein.iter().map(Value::str).collect())),
            ("aus", Value::Arr(self.aus.iter().map(Value::str).collect())),
            (
                "regler",
                Value::Arr(
                    self.regler
                        .iter()
                        .map(|r| {
                            Value::obj(vec![
                                ("symbol", Value::str(&r.symbol)),
                                ("name", Value::str(&r.name)),
                                ("min", Value::num(r.min as f64)),
                                ("max", Value::num(r.max as f64)),
                                ("vorgabe", Value::num(r.vorgabe as f64)),
                            ])
                        })
                        .collect(),
                ),
            ),
        ])
    }
}

/// Suchorte: `LV2_PATH`, sonst die üblichen Verzeichnisse.
pub fn suchpfade() -> Vec<PathBuf> {
    if let Some(p) = std::env::var_os("LV2_PATH").filter(|p| !p.is_empty()) {
        return std::env::split_paths(&p).collect();
    }
    let mut v = Vec::new();
    if let Some(home) = std::env::var_os("HOME") {
        v.push(Path::new(&home).join(".lv2"));
    }
    for p in [
        "/usr/local/lib64/lv2",
        "/usr/local/lib/lv2",
        "/usr/lib64/lv2",
        "/usr/lib/lv2",
        "/usr/lib/x86_64-linux-gnu/lv2",
        "/app/extensions/Plugins/lv2",
        "/app/lib/lv2",
    ] {
        v.push(PathBuf::from(p));
    }
    v
}

/// Alle Effekt-Plugins aus den Suchorten, nach Name sortiert, ohne Doppelte.
pub fn verzeichnis() -> Vec<Plugin> {
    verzeichnis_in(&suchpfade())
}

pub fn verzeichnis_in(pfade: &[PathBuf]) -> Vec<Plugin> {
    let mut alle: Vec<Plugin> = Vec::new();
    for basis in pfade {
        let Ok(eintraege) = std::fs::read_dir(basis) else {
            continue;
        };
        let mut bundles: Vec<PathBuf> = eintraege.flatten().map(|e| e.path()).collect();
        bundles.sort();
        for bundle in bundles {
            for p in bundle_lesen(&bundle) {
                if p.ist_effekt() && !alle.iter().any(|a| a.uri == p.uri) {
                    alle.push(p);
                }
            }
        }
    }
    alle.sort_by_key(|p| p.name.to_lowercase());
    alle
}

/// Liest ein Bundle: `manifest.ttl` und die dort genannten Dateien.
pub fn bundle_lesen(bundle: &Path) -> Vec<Plugin> {
    let Ok(manifest) = std::fs::read_to_string(bundle.join("manifest.ttl")) else {
        return Vec::new();
    };
    let mut texte = vec![manifest.clone()];
    for datei in see_also(&manifest) {
        // Nur Dateien im Bundle; absolute URIs zeigen woandershin.
        if datei.contains(':') || datei.contains("..") {
            continue;
        }
        if let Ok(t) = std::fs::read_to_string(bundle.join(&datei)) {
            texte.push(t);
        }
    }
    plugins_aus(&texte)
}

/// `rdfs:seeAlso <datei.ttl>` aus einem Manifest.
fn see_also(text: &str) -> Vec<String> {
    let token = tokens(text);
    let mut v = Vec::new();
    for w in token.windows(2) {
        if ist_praedikat(&w[0], "seeAlso") {
            if let Tok::Iri(d) = &w[1] {
                if !v.contains(d) {
                    v.push(d.clone());
                }
            }
        }
    }
    v
}

/// Baut die Plugins aus mehreren Turtle-Texten zusammen (Manifest und
/// Beschreibung dürfen Aussagen über dasselbe Subjekt verteilen).
pub fn plugins_aus(texte: &[String]) -> Vec<Plugin> {
    let mut plugins: Vec<Plugin> = Vec::new();
    let mut ist_plugin: Vec<String> = Vec::new();
    // Audio-Ports je Plugin mit Index; Turtle nennt sie nicht sortiert.
    let mut audio: Vec<(AudioPorts, AudioPorts)> = Vec::new();
    for text in texte {
        for aussage in aussagen(&tokens(text)) {
            let Some(Tok::Iri(subjekt)) = aussage.first() else {
                continue;
            };
            let subjekt = subjekt.clone();
            let idx = match plugins.iter().position(|p| p.uri == subjekt) {
                Some(i) => i,
                None => {
                    plugins.push(Plugin {
                        uri: subjekt.clone(),
                        ..Plugin::default()
                    });
                    audio.push((Vec::new(), Vec::new()));
                    plugins.len() - 1
                }
            };
            let p = &mut plugins[idx];
            let mut i = 1;
            while i < aussage.len() {
                let praed = &aussage[i];
                // Objekte bis zum nächsten `;` (oder Ende).
                let mut j = i + 1;
                let mut tiefe = 0;
                while j < aussage.len() {
                    match aussage[j] {
                        Tok::Auf => tiefe += 1,
                        Tok::Zu => tiefe -= 1,
                        Tok::Semikolon if tiefe == 0 => break,
                        _ => {}
                    }
                    j += 1;
                }
                let objekte = &aussage[i + 1..j];
                if matches!(praed, Tok::Wort(w) if w == "a") || ist_praedikat(praed, "type") {
                    if objekte.iter().any(|o| ist_klasse(o, "Plugin"))
                        && !ist_plugin.contains(&subjekt)
                    {
                        ist_plugin.push(subjekt.clone());
                    }
                } else if ist_praedikat(praed, "name") {
                    if let Some(Tok::Text(n)) = objekte.first() {
                        if p.name.is_empty() {
                            p.name = n.clone();
                        }
                    }
                } else if ist_praedikat(praed, "port") {
                    for block in bloecke(objekte) {
                        match port_lesen(block) {
                            Some(Port::Ein(n, sym)) => audio[idx].0.push((n, sym)),
                            Some(Port::Aus(n, sym)) => audio[idx].1.push((n, sym)),
                            Some(Port::Regler(r)) => p.regler.push(r),
                            None => {}
                        }
                    }
                }
                i = j + 1;
            }
        }
    }
    for (p, (mut ein, mut aus)) in plugins.iter_mut().zip(audio) {
        ein.sort_by(|a, b| a.0.total_cmp(&b.0));
        aus.sort_by(|a, b| a.0.total_cmp(&b.0));
        p.ein = ein.into_iter().map(|(_, s)| s).collect();
        p.aus = aus.into_iter().map(|(_, s)| s).collect();
    }
    let mut ergebnis: Vec<Plugin> = plugins
        .into_iter()
        .filter(|p| ist_plugin.contains(&p.uri))
        .collect();
    for p in ergebnis.iter_mut() {
        if p.name.is_empty() {
            p.name = p
                .uri
                .rsplit(['/', '#'])
                .next()
                .unwrap_or(&p.uri)
                .to_string();
        }
    }
    ergebnis
}

/// Audio-Ports eines Plugins mit ihrem LV2-Index (zum Sortieren).
type AudioPorts = Vec<(f32, String)>;

/// Ein gelesener Port.
enum Port {
    Ein(f32, String),
    Aus(f32, String),
    Regler(Regler),
}

/// Ein Port-Block `[ a lv2:InputPort, lv2:AudioPort ; lv2:symbol "in" … ]`.
fn port_lesen(block: &[Tok]) -> Option<Port> {
    let mut klassen: Vec<&Tok> = Vec::new();
    let mut symbol = None;
    let mut name = None;
    let mut index = None;
    let (mut min, mut max, mut vorgabe) = (None, None, None);
    let mut i = 0;
    while i < block.len() {
        let praed = &block[i];
        let mut j = i + 1;
        while j < block.len() && block[j] != Tok::Semikolon {
            j += 1;
        }
        let objekte = &block[i + 1..j];
        let zahl = || objekte.first().and_then(Tok::zahl);
        if matches!(praed, Tok::Wort(w) if w == "a") {
            klassen.extend(objekte.iter().filter(|o| !matches!(o, Tok::Komma)));
        } else if ist_praedikat(praed, "symbol") {
            if let Some(Tok::Text(s)) = objekte.first() {
                symbol = Some(s.clone());
            }
        } else if ist_praedikat(praed, "name") {
            if let Some(Tok::Text(s)) = objekte.first() {
                name.get_or_insert(s.clone());
            }
        } else if ist_praedikat(praed, "index") {
            index = zahl();
        } else if ist_praedikat(praed, "minimum") {
            min = zahl();
        } else if ist_praedikat(praed, "maximum") {
            max = zahl();
        } else if ist_praedikat(praed, "default") {
            vorgabe = zahl();
        }
        i = j + 1;
    }
    let symbol = symbol?;
    let hat = |k: &str| klassen.iter().any(|t| ist_klasse(t, k));
    let eingang = hat("InputPort");
    let index = index.unwrap_or(f32::MAX);
    if hat("AudioPort") {
        if eingang {
            Some(Port::Ein(index, symbol))
        } else if hat("OutputPort") {
            Some(Port::Aus(index, symbol))
        } else {
            None
        }
    } else if hat("ControlPort") && eingang {
        let min = min.unwrap_or(0.0);
        let max = max.unwrap_or(1.0).max(min);
        Some(Port::Regler(Regler {
            name: name.unwrap_or_else(|| symbol.clone()),
            symbol,
            min,
            max,
            vorgabe: vorgabe.unwrap_or(min).clamp(min, max),
        }))
    } else {
        None
    }
}

// ------------------------------------------------------------- Turtle-Leser

#[derive(Clone, Debug, PartialEq)]
enum Tok {
    /// `<…>`
    Iri(String),
    /// `"…"` (Sprach- oder Typangabe abgeschnitten)
    Text(String),
    /// Präfixname, Zahl oder `a`
    Wort(String),
    Auf,
    Zu,
    Semikolon,
    Komma,
    Punkt,
}

impl Tok {
    fn zahl(&self) -> Option<f32> {
        match self {
            Tok::Wort(w) | Tok::Text(w) => w.parse().ok(),
            _ => None,
        }
    }
}

/// Endet ein Prädikat (Präfixname oder IRI) auf `name`?
fn ist_praedikat(t: &Tok, name: &str) -> bool {
    match t {
        Tok::Wort(w) => w.rsplit(':').next() == Some(name),
        Tok::Iri(i) => i.ends_with(&format!("#{name}")) || i.ends_with(&format!("/{name}")),
        _ => false,
    }
}

fn ist_klasse(t: &Tok, name: &str) -> bool {
    ist_praedikat(t, name)
}

fn tokens(text: &str) -> Vec<Tok> {
    let z: Vec<char> = text.chars().collect();
    let mut v = Vec::new();
    let mut i = 0;
    while i < z.len() {
        let c = z[i];
        match c {
            c if c.is_whitespace() => i += 1,
            '#' => {
                while i < z.len() && z[i] != '\n' {
                    i += 1;
                }
            }
            '<' => {
                let start = i + 1;
                while i < z.len() && z[i] != '>' {
                    i += 1;
                }
                v.push(Tok::Iri(z[start..i.min(z.len())].iter().collect()));
                i += 1;
            }
            '"' | '\'' => {
                let lang = i + 2 < z.len() && z[i + 1] == c && z[i + 2] == c;
                let mut s = String::new();
                if lang {
                    i += 3;
                    while i < z.len()
                        && !(z[i] == c && i + 2 < z.len() && z[i + 1] == c && z[i + 2] == c)
                    {
                        s.push(z[i]);
                        i += 1;
                    }
                    i += 3;
                } else {
                    i += 1;
                    while i < z.len() && z[i] != c {
                        if z[i] == '\\' && i + 1 < z.len() {
                            i += 1;
                        }
                        s.push(z[i]);
                        i += 1;
                    }
                    i += 1;
                }
                // `@de` oder `^^xsd:…` gehört zum Literal, nicht zur Aussage.
                if i < z.len() && z[i] == '@' {
                    while i < z.len() && (z[i].is_alphanumeric() || z[i] == '-' || z[i] == '@') {
                        i += 1;
                    }
                } else if i + 1 < z.len() && z[i] == '^' && z[i + 1] == '^' {
                    i += 2;
                    while i < z.len() && !z[i].is_whitespace() && !";,.]".contains(z[i]) {
                        i += 1;
                    }
                }
                v.push(Tok::Text(s));
            }
            '[' => {
                v.push(Tok::Auf);
                i += 1;
            }
            ']' => {
                v.push(Tok::Zu);
                i += 1;
            }
            ';' => {
                v.push(Tok::Semikolon);
                i += 1;
            }
            ',' => {
                v.push(Tok::Komma);
                i += 1;
            }
            '(' | ')' => i += 1,
            _ => {
                let start = i;
                while i < z.len() && !z[i].is_whitespace() && !";,[]()<\"".contains(z[i]) {
                    // Ein Punkt beendet die Aussage, außer innerhalb einer Zahl.
                    if z[i] == '.' && !(i + 1 < z.len() && z[i + 1].is_ascii_digit() && i > start) {
                        break;
                    }
                    i += 1;
                }
                if i > start {
                    v.push(Tok::Wort(z[start..i].iter().collect()));
                } else if z[i] == '.' {
                    v.push(Tok::Punkt);
                    i += 1;
                } else {
                    i += 1;
                }
            }
        }
    }
    v
}

/// Teilt in Aussagen (bis `.` auf oberster Ebene); `@prefix` fällt weg.
fn aussagen(token: &[Tok]) -> Vec<Vec<Tok>> {
    let mut v = Vec::new();
    let mut akt = Vec::new();
    let mut tiefe = 0i32;
    for t in token {
        match t {
            Tok::Auf => tiefe += 1,
            Tok::Zu => tiefe -= 1,
            _ => {}
        }
        if *t == Tok::Punkt && tiefe <= 0 {
            if !matches!(akt.first(), Some(Tok::Wort(w)) if w.starts_with('@') || w.eq_ignore_ascii_case("prefix"))
            {
                v.push(std::mem::take(&mut akt));
            }
            akt.clear();
            continue;
        }
        akt.push(t.clone());
    }
    v
}

/// Die `[ … ]`-Blöcke einer Objektliste (Inhalt ohne Klammern).
fn bloecke(objekte: &[Tok]) -> Vec<&[Tok]> {
    let mut v = Vec::new();
    let mut tiefe = 0;
    let mut start = 0;
    for (i, t) in objekte.iter().enumerate() {
        match t {
            Tok::Auf => {
                if tiefe == 0 {
                    start = i + 1;
                }
                tiefe += 1;
            }
            Tok::Zu => {
                tiefe -= 1;
                if tiefe == 0 {
                    v.push(&objekte[start..i]);
                }
            }
            _ => {}
        }
    }
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    const MANIFEST: &str = r#"
@prefix lv2:  <http://lv2plug.in/ns/lv2core#> .
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .

<http://example.org/plugins/hall>
    a lv2:Plugin ;
    lv2:binary <hall.so> ;
    rdfs:seeAlso <hall.ttl> .
"#;

    const PLUGIN: &str = r#"
@prefix doap: <http://usefulinc.com/ns/doap#> .
@prefix lv2:  <http://lv2plug.in/ns/lv2core#> .

# Ports absichtlich nicht in Indexreihenfolge.
<http://example.org/plugins/hall>
    a lv2:Plugin, lv2:ReverbPlugin ;
    doap:name "Testhall"@de ;
    lv2:port [
        a lv2:AudioPort, lv2:OutputPort ;
        lv2:index 3 ;
        lv2:symbol "out_r" ;
        lv2:name "Out R"
    ] , [
        a lv2:AudioPort , lv2:InputPort ;
        lv2:index 0 ;
        lv2:symbol "in_l" ;
        lv2:name "In L"
    ] , [
        a lv2:InputPort, lv2:AudioPort ;
        lv2:index 1 ;
        lv2:symbol "in_r" ;
        lv2:name "In R"
    ] , [
        a lv2:AudioPort, lv2:OutputPort ;
        lv2:index 2 ;
        lv2:symbol "out_l" ;
        lv2:name "Out L"
    ] , [
        a lv2:InputPort, lv2:ControlPort ;
        lv2:index 4 ;
        lv2:symbol "size" ;
        lv2:name "Room size" ;
        lv2:default 0.5 ;
        lv2:minimum 0.0 ;
        lv2:maximum 1.0
    ] , [
        a lv2:OutputPort, lv2:ControlPort ;
        lv2:index 5 ;
        lv2:symbol "latency" ;
        lv2:name "Latency"
    ] .
"#;

    #[test]
    fn plugin_mit_ports_in_indexreihenfolge() {
        let p = plugins_aus(&[MANIFEST.to_string(), PLUGIN.to_string()]);
        assert_eq!(p.len(), 1, "{p:?}");
        let p = &p[0];
        assert_eq!(p.uri, "http://example.org/plugins/hall");
        assert_eq!(p.name, "Testhall");
        assert_eq!(p.ein, vec!["in_l", "in_r"]);
        assert_eq!(p.aus, vec!["out_l", "out_r"]);
        assert_eq!(p.regler.len(), 1, "Ausgangsregler zaehlt nicht");
        assert_eq!(p.regler[0].symbol, "size");
        assert_eq!(p.regler[0].vorgabe, 0.5);
        assert!(p.ist_effekt());
    }

    #[test]
    fn bundle_von_der_platte() {
        let dir = std::env::temp_dir().join(format!("cp-lv2-{}", std::process::id()));
        let bundle = dir.join("hall.lv2");
        std::fs::create_dir_all(&bundle).expect("Verzeichnis");
        std::fs::write(bundle.join("manifest.ttl"), MANIFEST).expect("manifest");
        std::fs::write(bundle.join("hall.ttl"), PLUGIN).expect("ttl");
        // Ein kaputtes Bundle daneben stoert nicht.
        std::fs::create_dir_all(dir.join("kaputt.lv2")).expect("Verzeichnis");
        std::fs::write(dir.join("kaputt.lv2/manifest.ttl"), "<x> a [ ; .").expect("kaputt");
        let v = verzeichnis_in(std::slice::from_ref(&dir));
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].name, "Testhall");
    }

    #[test]
    fn langer_text_volle_iris_und_zahlen() {
        let ttl = r#"
<urn:x:mono> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <http://lv2plug.in/ns/lv2core#Plugin> ;
  <http://usefulinc.com/ns/doap#name> """Mono "Gate" mit Halt""" ;
  <http://lv2plug.in/ns/lv2core#port> [
     a <http://lv2plug.in/ns/lv2core#AudioPort>, <http://lv2plug.in/ns/lv2core#InputPort> ;
     <http://lv2plug.in/ns/lv2core#symbol> "in" ; <http://lv2plug.in/ns/lv2core#index> 0
  ], [
     a <http://lv2plug.in/ns/lv2core#AudioPort>, <http://lv2plug.in/ns/lv2core#OutputPort> ;
     <http://lv2plug.in/ns/lv2core#symbol> "out" ; <http://lv2plug.in/ns/lv2core#index> 1
  ], [
     a <http://lv2plug.in/ns/lv2core#ControlPort>, <http://lv2plug.in/ns/lv2core#InputPort> ;
     <http://lv2plug.in/ns/lv2core#symbol> "thr" ;
     <http://lv2plug.in/ns/lv2core#minimum> -60.5 ; <http://lv2plug.in/ns/lv2core#maximum> 0 ;
     <http://lv2plug.in/ns/lv2core#default> -99
  ] .
"#;
        let p = plugins_aus(&[ttl.to_string()]);
        assert_eq!(p.len(), 1, "{p:?}");
        assert_eq!(p[0].name, "Mono \"Gate\" mit Halt");
        assert_eq!(p[0].ein, vec!["in"]);
        assert_eq!(p[0].aus, vec!["out"]);
        // Vorgabe ausserhalb des Bereichs wird eingefangen.
        assert_eq!(p[0].regler[0].min, -60.5);
        assert_eq!(p[0].regler[0].vorgabe, -60.5);
    }

    #[test]
    fn instrumente_ohne_eingang_sind_keine_effekte() {
        let ttl = r#"@prefix lv2: <http://lv2plug.in/ns/lv2core#> .
<urn:synth> a lv2:Plugin, lv2:InstrumentPlugin ;
  lv2:port [ a lv2:AudioPort, lv2:OutputPort ; lv2:symbol "out" ; lv2:index 0 ] ."#;
        let p = plugins_aus(&[ttl.to_string()]);
        assert_eq!(p.len(), 1);
        assert!(!p[0].ist_effekt());
    }
}
