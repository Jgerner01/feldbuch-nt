// Zweck:    RTF-Protokollgenerator: liest eine XML-Steuerdatei (Vorlage) und erzeugt daraus ein RTF-Dokument.
//           Abschnittstypen: Titel, Zeile, Monoblock, Tabelle, Leerzeile, Trennlinie; Farben, Querformat, bedingte
//           Abschnitte und Spalten (zeigenWenn), Ampelfärbung der Tabellenzeilen über den Schlüssel „_ampel".
// Hardware: keine
// Autor:    Johann Gerner

use std::collections::HashMap;

use roxmltree::{Document, Node};

/// Einfache Felder: Schlüssel → vorformatierter Text.
pub type Felder = HashMap<String, String>;
/// Eine Tabellenzeile: Spaltenschlüssel → Text.
pub type Zeile = HashMap<String, String>;
/// Benannte Tabellen; die namenlose Tabelle liegt unter dem Schlüssel "".
pub type Tabellen = HashMap<String, Vec<Zeile>>;

/// Twips je Millimeter.
const TWIPS_PRO_MM: f64 = 56.69;
/// Standardwerte der Vorlage.
const STANDARD_SCHRIFT: i32 = 1;
const STANDARD_GROESSE: i32 = 10;
const STANDARD_SPALTENBREITE: usize = 8;
const STANDARD_BREITE_MM: i32 = 210;
const STANDARD_HOEHE_MM: i32 = 297;
const STANDARD_RAND_MM: i32 = 20;
/// RTF-Halbpunkte je Punkt.
const HALBPUNKTE: i32 = 2;
/// Zeilenende der erzeugten Datei.
const NEUE_ZEILE: &str = "\r\n";

fn zahl(knoten: Node, name: &str, standard: i32) -> i32 {
    knoten.attribute(name).and_then(|w| w.trim().parse().ok()).unwrap_or(standard)
}

fn ist_wahr(knoten: Node, name: &str) -> bool {
    knoten.attribute(name) == Some("true")
}

fn mm(knoten: Node, name: &str, standard: i32) -> i32 {
    (f64::from(zahl(knoten, name, standard)) * TWIPS_PRO_MM).round() as i32
}

/// Maskiert Text für RTF: Sonderzeichen, Umlaute und alle Nicht-ASCII-Zeichen.
pub fn maskiere(text: &str) -> String {
    let mut aus = String::with_capacity(text.len() + 16);
    for einheit in text.encode_utf16() {
        match einheit {
            0x5C => aus.push_str("\\\\"),
            0x7B => aus.push_str("\\{"),
            0x7D => aus.push_str("\\}"),
            0xE4 => aus.push_str("\\'e4"),
            0xF6 => aus.push_str("\\'f6"),
            0xFC => aus.push_str("\\'fc"),
            0xC4 => aus.push_str("\\'c4"),
            0xD6 => aus.push_str("\\'d6"),
            0xDC => aus.push_str("\\'dc"),
            0xDF => aus.push_str("\\'df"),
            e if e > 127 => aus.push_str(&format!("\\u{}?", e as i16)),
            e => aus.push(char::from(e as u8)),
        }
    }
    aus
}

fn format_gruppe(schrift: i32, groesse: i32, fett: bool, kursiv: bool) -> String {
    let mut f = format!("\\f{schrift}\\fs{groesse}");
    if fett {
        f.push_str("\\b");
    }
    if kursiv {
        f.push_str("\\i");
    }
    f
}

fn links(text: &str, breite: usize) -> String {
    let n = text.chars().count();
    format!("{text}{}", " ".repeat(breite.saturating_sub(n)))
}

fn rechts(text: &str, breite: usize) -> String {
    let n = text.chars().count();
    format!("{}{text}", " ".repeat(breite.saturating_sub(n)))
}

/// Zeigt einen Abschnitt oder eine Spalte nur, wenn das Feld der Bedingung den Wert "1" hat.
fn sichtbar(knoten: Node, felder: &Felder) -> bool {
    match knoten.attribute("zeigenWenn") {
        Some(k) if !k.is_empty() => felder.get(k).is_some_and(|w| w == "1"),
        _ => true,
    }
}

fn feld_wert(feld: Node, felder: &Felder) -> String {
    if let Some(text) = feld.attribute("text") {
        return text.to_string();
    }
    let var = feld.attribute("var").unwrap_or("");
    let wert = felder.get(var).cloned().unwrap_or_else(|| format!("[{var}]"));
    format!("{}{wert}{}", feld.attribute("label").unwrap_or(""), feld.attribute("einheit").unwrap_or(""))
}

fn untertag<'a>(knoten: Node<'a, 'a>, name: &'a str) -> impl Iterator<Item = Node<'a, 'a>> {
    knoten.children().filter(move |k| k.is_element() && k.tag_name().name() == name)
}

/// Erzeugt den RTF-Text aus Vorlage, Feldern und Tabellen.
pub fn erzeuge(vorlage: &str, felder: &Felder, tabellen: &Tabellen) -> Result<String, String> {
    let vorlage = vorlage.trim_start_matches('\u{feff}');
    let dok = Document::parse(vorlage).map_err(|e| format!("Protokollvorlage ungültig: {e}"))?;
    let wurzel = dok.root_element();
    let mut rtf = String::with_capacity(8192);
    let zeile = |rtf: &mut String, text: &str| {
        rtf.push_str(text);
        rtf.push_str(NEUE_ZEILE);
    };

    zeile(&mut rtf, "{\\rtf1\\ansi\\ansicpg1252\\deff0");

    // Schriftentabelle
    rtf.push_str("{\\fonttbl");
    if let Some(schriften) = untertag(wurzel, "Schriften").next() {
        for s in untertag(schriften, "Schrift") {
            let typ = match s.attribute("typ") {
                Some("swiss") => "\\fswiss",
                Some("modern") => "\\fmodern",
                Some("roman") => "\\froman",
                _ => "\\fnil",
            };
            rtf.push_str(&format!("{{\\f{}{typ} {};}}", s.attribute("id").unwrap_or("0"), s.attribute("name").unwrap_or("")));
        }
    }
    zeile(&mut rtf, "}");

    // Farbtabelle (optional)
    if let Some(farben) = untertag(wurzel, "Farben").next() {
        rtf.push_str("{\\colortbl;");
        for fc in untertag(farben, "Farbe") {
            rtf.push_str(&format!("\\red{}\\green{}\\blue{};", zahl(fc, "r", 0), zahl(fc, "g", 0), zahl(fc, "b", 0)));
        }
        zeile(&mut rtf, "}");
    }

    // Seitenformat
    if let Some(seite) = untertag(wurzel, "Seite").next() {
        rtf.push_str(&format!(
            "\\paperw{}\\paperh{}\\margl{}\\margr{}\\margt{}\\margb{}",
            mm(seite, "breite", STANDARD_BREITE_MM),
            mm(seite, "hoehe", STANDARD_HOEHE_MM),
            mm(seite, "randLinks", STANDARD_RAND_MM),
            mm(seite, "randRechts", STANDARD_RAND_MM),
            mm(seite, "randOben", STANDARD_RAND_MM),
            mm(seite, "randUnten", STANDARD_RAND_MM)
        ));
        if ist_wahr(seite, "querformat") {
            rtf.push_str("\\landscape");
        }
        rtf.push_str(NEUE_ZEILE);
    }
    zeile(&mut rtf, "\\widowctrl\\hyphauto");

    // Inhalt
    let Some(inhalt) = untertag(wurzel, "Inhalt").next() else {
        rtf.push('}');
        return Ok(rtf);
    };
    for a in untertag(inhalt, "Abschnitt") {
        if !sichtbar(a, felder) {
            continue;
        }
        let f = zahl(a, "schrift", STANDARD_SCHRIFT);
        let fs = zahl(a, "groesse", STANDARD_GROESSE) * HALBPUNKTE;
        let fett = ist_wahr(a, "fett");
        let kursiv = ist_wahr(a, "kursiv");
        let sa = zahl(a, "abstandNach", 0);
        let a_farbe = zahl(a, "farbe", 0);
        let rand = if ist_wahr(a, "unterlinie") { "\\brdrb\\brdrs\\brdrw15\\brsp20" } else { "" };

        match a.attribute("typ").unwrap_or("") {
            "Leerzeile" => zeile(&mut rtf, &format!("\\pard\\sa{sa} {{\\f{f}\\fs{fs} }}\\par")),
            "Trennlinie" => zeile(&mut rtf, &format!("\\pard\\brdrb\\brdrs\\brdrw15\\brsp20\\sa{sa} {{\\f{f}\\fs{fs} }}\\par")),
            "Titel" | "Zeile" => {
                rtf.push_str(&format!("\\pard{rand}\\sa{sa} "));
                for feld in untertag(a, "Feld") {
                    let f_farbe = if feld.has_attribute("farbe") { zahl(feld, "farbe", 0) } else { a_farbe };
                    let f_groesse = if feld.has_attribute("groesse") { zahl(feld, "groesse", STANDARD_GROESSE) * HALBPUNKTE } else { fs };
                    let f_fett = if feld.has_attribute("fett") { ist_wahr(feld, "fett") } else { fett };
                    let f_kursiv = if feld.has_attribute("kursiv") { ist_wahr(feld, "kursiv") } else { kursiv };
                    let mut fmt = format_gruppe(f, f_groesse, f_fett, f_kursiv);
                    if f_farbe > 0 {
                        fmt.push_str(&format!("\\cf{f_farbe}"));
                    }
                    rtf.push_str(&format!("{{{fmt} {}}}", maskiere(&feld_wert(feld, felder))));
                }
                zeile(&mut rtf, "\\par");
            }
            "Monoblock" => {
                let inhalt = felder.get(a.attribute("var").unwrap_or("")).cloned().unwrap_or_default();
                if a_farbe > 0 {
                    zeile(&mut rtf, &format!("{{\\cf{a_farbe}"));
                }
                for z in inhalt.split('\n') {
                    rtf.push_str(&format!("\\pard\\sa0 {{{} ", format_gruppe(f, fs, fett, kursiv)));
                    rtf.push_str(&maskiere(z.trim_end_matches('\r')));
                    zeile(&mut rtf, "}\\par");
                }
                if a_farbe > 0 {
                    zeile(&mut rtf, "\\cf0}");
                }
            }
            "Tabelle" => {
                let kopf_fett = ist_wahr(a, "kopfFett");
                let ampel_aktiv = ist_wahr(a, "ampel");
                let leer = Vec::new();
                let zeilen = tabellen.get(a.attribute("name").unwrap_or("")).unwrap_or(&leer);
                let spalten: Vec<Node> = untertag(a, "Spalte").filter(|s| sichtbar(*s, felder)).collect();
                let breite = |s: Node| usize::try_from(zahl(s, "breite", STANDARD_SPALTENBREITE as i32)).unwrap_or(STANDARD_SPALTENBREITE);
                let rechtsbuendig = |s: Node| s.attribute("ausrichtung") == Some("R");

                let kopf: Vec<String> = spalten
                    .iter()
                    .map(|s| {
                        let h = s.attribute("header").unwrap_or("");
                        if rechtsbuendig(*s) { rechts(h, breite(*s)) } else { links(h, breite(*s)) }
                    })
                    .collect();
                rtf.push_str(&format!("\\pard\\sa0 {{{} ", format_gruppe(f, fs, kopf_fett, false)));
                rtf.push_str(&maskiere(&kopf.join(" ")));
                zeile(&mut rtf, "}\\par");

                let gesamt: usize = spalten.iter().map(|s| breite(*s)).sum::<usize>() + spalten.len().saturating_sub(1);
                rtf.push_str(&format!("\\pard\\sa0 {{\\f{f}\\fs{fs} "));
                rtf.push_str(&maskiere(&"-".repeat(gesamt)));
                zeile(&mut rtf, "}\\par");

                for z in zeilen {
                    let ampel = if ampel_aktiv { z.get("_ampel").map_or("", String::as_str) } else { "" };
                    let (vor, nach) = match ampel {
                        "1" => ("\\cf2 ", "\\cf0"),
                        "2" => ("\\cf3 ", "\\cf0"),
                        "3" => ("\\cf4\\b ", "\\b0\\cf0"),
                        _ => ("", ""),
                    };
                    let felder_text: Vec<String> = spalten
                        .iter()
                        .map(|s| {
                            let w = z.get(s.attribute("var").unwrap_or("")).map_or("", String::as_str);
                            if rechtsbuendig(*s) { rechts(w, breite(*s)) } else { links(w, breite(*s)) }
                        })
                        .collect();
                    rtf.push_str(&format!("\\pard\\sa0 {{{} {vor}", format_gruppe(f, fs, false, false)));
                    rtf.push_str(&maskiere(&felder_text.join(" ")));
                    rtf.push_str(nach);
                    zeile(&mut rtf, "}\\par");
                }
            }
            _ => {}
        }
    }
    rtf.push('}');
    Ok(rtf)
}

#[cfg(test)]
mod tests {
    use super::*;

    const VORLAGE: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<Protokoll>
  <Seite breite="297" hoehe="210" randLinks="15" randRechts="15" randOben="18" randUnten="18" querformat="true"/>
  <Schriften><Schrift id="0" name="Arial Narrow" typ="swiss"/><Schrift id="1" name="Courier New" typ="modern"/></Schriften>
  <Farben><Farbe r="42" g="72" b="130"/><Farbe r="200" g="100" b="0"/></Farben>
  <Inhalt>
    <Abschnitt typ="Titel" schrift="0" groesse="13" fett="true" unterlinie="true" abstandNach="80">
      <Feld text="Rückwärts"/><Feld var="Name" label="Name: " einheit=" m" farbe="1"/>
    </Abschnitt>
    <Abschnitt typ="Zeile" zeigenWenn="Zeigen"><Feld text="sichtbar"/></Abschnitt>
    <Abschnitt typ="Zeile" zeigenWenn="Versteckt"><Feld text="versteckt"/></Abschnitt>
    <Abschnitt typ="Monoblock" var="Block" fett="true"/>
    <Abschnitt typ="Tabelle" name="T" ampel="true" kopfFett="true">
      <Spalte var="A" header="Punkt" breite="6" ausrichtung="L"/>
      <Spalte var="B" header="Wert" breite="7" ausrichtung="R"/>
      <Spalte var="C" header="Zusatz" breite="5" zeigenWenn="Mit"/>
    </Abschnitt>
    <Abschnitt typ="Leerzeile"/><Abschnitt typ="Trennlinie"/>
  </Inhalt>
</Protokoll>"#;

    fn felder() -> Felder {
        [("Name", "Grüße {x}"), ("Zeigen", "1"), ("Versteckt", "0"), ("Block", "eins\r\nzwei"), ("Mit", "0")]
            .iter()
            .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
            .collect()
    }

    fn zeile_aus(paare: &[(&str, &str)]) -> Zeile {
        paare.iter().map(|(k, v)| ((*k).to_string(), (*v).to_string())).collect()
    }

    #[test]
    fn maskiert_sonderzeichen() {
        assert_eq!(maskiere("a\\b{c}"), "a\\\\b\\{c\\}");
        assert_eq!(maskiere("äöüÄÖÜß"), "\\'e4\\'f6\\'fc\\'c4\\'d6\\'dc\\'df");
        assert_eq!(maskiere("–"), "\\u8211?");
        assert_eq!(maskiere("Δ"), "\\u916?");
    }

    #[test]
    fn erzeugt_kopf_und_abschnitte() {
        let mut tabellen = Tabellen::new();
        tabellen.insert("T".to_string(), vec![zeile_aus(&[("A", "P1"), ("B", "1.5"), ("_ampel", "3")]), zeile_aus(&[("A", "P2"), ("B", "2.5")])]);
        let rtf = erzeuge(VORLAGE, &felder(), &tabellen).unwrap();
        assert!(rtf.starts_with("{\\rtf1\\ansi\\ansicpg1252\\deff0\r\n{\\fonttbl{\\f0\\fswiss Arial Narrow;}{\\f1\\fmodern Courier New;}}\r\n"));
        assert!(rtf.contains("{\\colortbl;\\red42\\green72\\blue130;\\red200\\green100\\blue0;}"));
        assert!(rtf.contains("\\paperw16837\\paperh11905\\margl850\\margr850\\margt1020\\margb1020\\landscape"));
        assert!(rtf.contains("\\pard\\brdrb\\brdrs\\brdrw15\\brsp20\\sa80 {\\f0\\fs26\\b R\\'fcckw\\'e4rts}"), "{rtf}");
        assert!(rtf.contains("{\\f0\\fs26\\b\\cf1 Name: Gr\\'fc\\'dfe \\{x\\} m}"));
        assert!(rtf.contains("sichtbar") && !rtf.contains("versteckt"));
        assert!(rtf.contains("eins}\\par") && rtf.contains("zwei}\\par"));
        // Kopfzeile fett, Spalte C ausgeblendet, Linie über die Gesamtbreite 6 + 1 + 7
        assert!(rtf.contains(&format!("{{\\f1\\fs20\\b Punkt{}Wert}}\\par", " ".repeat(5))), "{rtf}");
        assert!(rtf.contains(&format!("{{\\f1\\fs20 {}}}\\par", "-".repeat(14))));
        // Ampelzeile 3: tiefrot und fett; Spalte A linksbündig (6), Spalte B rechtsbündig (7)
        assert!(rtf.contains("{\\f1\\fs20 \\cf4\\b P1         1.5\\b0\\cf0}\\par"), "{rtf}");
        assert!(rtf.contains("{\\f1\\fs20 P2         2.5}\\par"));
        assert!(rtf.ends_with('}'));
    }

    #[test]
    fn fehlende_felder_und_ungueltige_vorlage() {
        let vorlage = r#"<Protokoll><Inhalt><Abschnitt typ="Zeile"><Feld var="Fehlt"/></Abschnitt></Inhalt></Protokoll>"#;
        let rtf = erzeuge(vorlage, &Felder::new(), &Tabellen::new()).unwrap();
        assert!(rtf.contains("[Fehlt]"));
        assert!(erzeuge("<Protokoll>", &Felder::new(), &Tabellen::new()).is_err());
    }
}
