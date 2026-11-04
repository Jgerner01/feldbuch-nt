// Zweck:    Minimaler XML-Leser und -Schreiber für flache Einstellungsdateien (Wurzel + Textelemente)
// Hardware: keine
// Autor:    Johann Gerner

use std::collections::HashMap;

/// Gelesenes Dokument: Name des Wurzelelements und Textwerte der direkten Kindelemente.
pub struct XmlDokument {
    pub wurzel: String,
    werte: HashMap<String, String>,
}

impl XmlDokument {
    /// Liefert den Text eines Kindelements.
    pub fn text(&self, tag: &str) -> Option<&str> {
        self.werte.get(tag).map(String::as_str)
    }
}

/// Maskiert die XML-Sonderzeichen.
fn maskiere(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

/// Hebt die Maskierung wieder auf.
fn entmaskiere(text: &str) -> String {
    text.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&amp;", "&")
}

/// Erzeugt ein XML-Dokument mit Wurzel und flachen Textelementen.
pub fn schreibe(wurzel: &str, eintraege: &[(&str, String)]) -> String {
    let mut ausgabe = String::from("<?xml version=\"1.0\" encoding=\"utf-8\"?>\r\n");
    ausgabe.push_str(&format!("<{wurzel}>\r\n"));
    for (tag, wert) in eintraege {
        ausgabe.push_str(&format!("  <{tag}>{}</{tag}>\r\n", maskiere(wert)));
    }
    ausgabe.push_str(&format!("</{wurzel}>"));
    ausgabe
}

/// Liest ein flaches XML-Dokument. Ungültiger Aufbau ergibt einen Fehlertext.
pub fn lese(inhalt: &str) -> Result<XmlDokument, String> {
    let text = inhalt.trim_start_matches('\u{feff}').trim();
    let mut rest = text;
    if rest.starts_with("<?") {
        let ende = rest.find("?>").ok_or("Ungültige XML-Deklaration.")?;
        rest = rest[ende + 2..].trim_start();
    }
    let (wurzel, mut rest) = oeffnendes_tag(rest).ok_or("Kein Wurzelelement gefunden.")?;
    let schluss = format!("</{wurzel}>");
    if !text.trim_end().ends_with(&schluss) {
        return Err(format!("Wurzelelement <{wurzel}> ist nicht geschlossen."));
    }

    let mut werte = HashMap::new();
    loop {
        rest = rest.trim_start();
        if rest.starts_with(&schluss) || rest.is_empty() {
            break;
        }
        if rest.starts_with("<!--") {
            let ende = rest.find("-->").ok_or("Kommentar nicht geschlossen.")?;
            rest = &rest[ende + 3..];
            continue;
        }
        let (tag, nach_tag, leer) = lese_tag(rest).ok_or("Ungültiges Element.")?;
        if leer {
            werte.insert(tag.to_string(), String::new());
            rest = nach_tag;
            continue;
        }
        let ende_tag = format!("</{tag}>");
        let ende = nach_tag
            .find(&ende_tag)
            .ok_or_else(|| format!("Element <{tag}> ist nicht geschlossen."))?;
        werte.insert(tag.to_string(), entmaskiere(&nach_tag[..ende]));
        rest = &nach_tag[ende + ende_tag.len()..];
    }
    Ok(XmlDokument {
        wurzel: wurzel.to_string(),
        werte,
    })
}

/// Liest ein öffnendes Tag am Anfang und liefert Namen und Rest.
fn oeffnendes_tag(text: &str) -> Option<(&str, &str)> {
    let (name, rest, _) = lese_tag(text)?;
    Some((name, rest))
}

/// Liest ein Tag am Anfang: Name, Rest nach `>` und Kennzeichen für selbstschließende Elemente.
fn lese_tag(text: &str) -> Option<(&str, &str, bool)> {
    let text = text.strip_prefix('<')?;
    let ende = text.find('>')?;
    let inhalt = &text[..ende];
    let selbst_schliessend = inhalt.ends_with('/');
    let name = inhalt.trim_end_matches('/').split_whitespace().next()?;
    if name.is_empty() || name.starts_with('/') {
        return None;
    }
    Some((name, &text[ende + 1..], selbst_schliessend))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schreibt_und_liest_zurueck() {
        let xml = schreibe(
            "Test",
            &[
                ("Name", "A & B <c>".to_string()),
                ("Zahl", "1.5".to_string()),
                ("Leer", String::new()),
            ],
        );
        let doc = lese(&xml).unwrap();
        assert_eq!(doc.wurzel, "Test");
        assert_eq!(doc.text("Name"), Some("A & B <c>"));
        assert_eq!(doc.text("Zahl"), Some("1.5"));
        assert_eq!(doc.text("Leer"), Some(""));
        assert_eq!(doc.text("Fehlt"), None);
    }

    #[test]
    fn defektes_xml_wird_abgelehnt() {
        assert!(lese("<A><B>1</B>").is_err());
        assert!(lese("kein xml").is_err());
    }
}
