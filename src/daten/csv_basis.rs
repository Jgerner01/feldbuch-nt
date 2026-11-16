//! Gemeinsame CSV-Syntax für Fachdateien und Projektjournal.
use std::io;

pub fn fehler(text: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, text.into())
}

pub fn name(text: &str) -> String {
    let n: String = text.trim_start_matches('\u{feff}').to_lowercase().replace('ö', "oe").chars().filter(|c| c.is_alphanumeric()).collect();
    match n.as_str() {
        "punktnummer" | "nr" | "point" | "name" => "punktnr",
        "r" | "rm" | "rechtswert" | "easting" => "x",
        "h" | "hm" | "hochwert" | "northing" => "y",
        "hoehe" | "hoehem" | "elevation" | "height" => "z",
        "hz" => "hzgon",
        "v" => "vgon",
        "strecke" | "schraegstrecke" => "streckem",
        "zielhoehe" | "zh" | "zhm" => "zielhoehem",
        "punktcode" => "code",
        "standpunkt" => "standpunktnr",
        "instrhoehe" | "ih" | "instrumentenhoehem" => "instrumentenhoehe",
        _ => return n,
    }
    .to_string()
}

pub fn index(spalten: &[String], gesucht: &str) -> Option<usize> {
    let n = name(gesucht);
    spalten.iter().position(|s| name(s) == n)
}

pub fn pruefe_spalten(spalten: &[String]) -> io::Result<()> {
    let mut namen = std::collections::HashSet::new();
    for s in spalten {
        if s.trim().is_empty() || !namen.insert(name(s)) {
            return Err(fehler(format!("Leerer oder doppelter Spaltenname: {s}")));
        }
    }
    Ok(())
}

pub fn maskiere(text: &str, trenner: char) -> String {
    if text.contains([trenner, '"', '\r', '\n']) || text.trim() != text || text.starts_with(['#', '%']) || text == "---" {
        format!("\"{}\"", text.replace('"', "\"\""))
    } else {
        text.to_string()
    }
}

pub fn zeile<I, S>(felder: I, trenner: char) -> String
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    felder.into_iter().map(|s| maskiere(s.as_ref(), trenner)).collect::<Vec<_>>().join(&trenner.to_string())
}

/// Normalisiert ausschließlich bekannte Zahlenspalten; Text und höhere
/// vorhandene Dezimalpräzision werden nicht gerundet.
pub fn zahlenfeld(spalte: &str, wert: &str) -> io::Result<String> {
    let stellen: usize = match name(spalte).as_str() {
        "x" | "y" | "z" | "hzgon" | "vgon" | "orientierunggon" => 4,
        "streckem" | "zielhoehem" | "exzlaengsm" | "exzquerm" | "instrumentenhoehe" => 3,
        _ => return Ok(wert.into()),
    };
    if wert.is_empty() {
        return Ok(String::new());
    }
    crate::grundlagen::format::parse_zahl(wert).ok_or_else(|| fehler(format!("Spalte {spalte}: ungültige Zahl '{wert}'.")))?;
    let mut aus = wert.trim().replace(',', ".");
    if let Some((mantisse, exponent)) = aus.split_once(['e', 'E']) {
        let exponent: i32 = exponent.parse().map_err(|_| fehler("Ungültiger Dezimalexponent."))?;
        if !(-400..=400).contains(&exponent) {
            return Err(fehler("Dezimalexponent außerhalb des unterstützten Bereichs."));
        }
        let sign = if mantisse.starts_with('-') { "-" } else { "" };
        let m = mantisse.trim_start_matches(['+', '-']);
        let stelle = m.find('.').unwrap_or(m.len()) as i32 + exponent;
        let ziffern = m.replace('.', "");
        aus = if stelle <= 0 {
            format!("{sign}0.{}{ziffern}", "0".repeat((-stelle) as usize))
        } else if stelle as usize >= ziffern.len() {
            format!("{sign}{ziffern}{}", "0".repeat(stelle as usize - ziffern.len()))
        } else {
            format!("{sign}{}.{}", &ziffern[..stelle as usize], &ziffern[stelle as usize..])
        };
    }
    if !aus.contains('.') {
        aus.push('.');
    }
    let nachkomma = aus.split_once('.').map_or(0, |(_, rest)| rest.len());
    aus.extend(std::iter::repeat_n('0', stellen.saturating_sub(nachkomma)));
    Ok(aus)
}

/// Logische Datensätze mit physischer Startzeile (1-basiert). Zeilenumbrüche in
/// zitierten Feldern bleiben erhalten; Metadaten/Kommentare sind normale Rohsätze.
pub fn saetze(text: &str) -> io::Result<Vec<(usize, String)>> {
    let mut aus = Vec::new();
    let mut roh = String::new();
    let (mut zitiert, mut feldanfang, mut zeile, mut start) = (false, true, 1, 1);
    let (mut metadaten, mut rohtext) = (false, false);
    let mut chars = text.trim_start_matches('\u{feff}').chars().peekable();
    while let Some(c) = chars.next() {
        if roh.is_empty() && !zitiert {
            let vorschau: String = std::iter::once(c).chain(chars.clone()).take_while(|c| *c != '\n').collect();
            let v = vorschau.trim().to_uppercase();
            if v.starts_with("# METADATA") {
                metadaten = true;
            } else if v.starts_with("# DATEN") || v.starts_with("# DATENTYP") || v == "# STATIONIERUNG" {
                metadaten = false;
            }
            rohtext = metadaten || v.starts_with(['#', '%']);
        }
        if c == '"' && !rohtext {
            if zitiert && chars.peek() == Some(&'"') {
                roh.push(c);
                roh.push(chars.next().unwrap());
                continue;
            }
            if zitiert || feldanfang {
                zitiert = !zitiert;
            }
            feldanfang = false;
        } else if !zitiert && (c == ';' || c == ',') {
            feldanfang = true;
        } else if !zitiert && c != '\r' && c != '\n' && !c.is_whitespace() {
            feldanfang = false;
        }
        if c == '\n' {
            zeile += 1;
            if !zitiert {
                if roh.ends_with('\r') {
                    roh.pop();
                }
                aus.push((start, std::mem::take(&mut roh)));
                start = zeile;
                feldanfang = true;
                continue;
            }
        }
        roh.push(c);
    }
    if zitiert {
        return Err(fehler(format!("Zeile {start}: nicht geschlossenes Anführungszeichen.")));
    }
    if !roh.is_empty() {
        aus.push((start, roh));
    }
    Ok(aus)
}

pub fn felder(text: &str, trenner: char) -> io::Result<Vec<String>> {
    let mut aus = Vec::new();
    let mut feld = String::new();
    let (mut zitiert, mut war_zitiert, mut geschlossen) = (false, false, false);
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if zitiert {
            if c == '"' {
                if chars.peek() == Some(&'"') {
                    feld.push(chars.next().unwrap());
                } else {
                    zitiert = false;
                    geschlossen = true;
                }
            } else {
                feld.push(c);
            }
        } else if c == trenner {
            aus.push(if war_zitiert { feld.clone() } else { feld.trim().to_string() });
            feld.clear();
            war_zitiert = false;
            geschlossen = false;
        } else if c == '"' && feld.trim().is_empty() && !geschlossen {
            feld.clear();
            zitiert = true;
            war_zitiert = true;
        } else if geschlossen {
            if !c.is_whitespace() {
                return Err(fehler("Zeichen hinter geschlossenem CSV-Feld."));
            }
        } else if c == '"' {
            return Err(fehler("Anführungszeichen innerhalb eines unzitierten CSV-Felds."));
        } else {
            feld.push(c);
        }
    }
    if zitiert {
        return Err(fehler("Nicht geschlossenes Anführungszeichen."));
    }
    aus.push(if war_zitiert { feld } else { feld.trim().to_string() });
    Ok(aus)
}

/// Erkennung ausschließlich anhand der Kopfzeile, nicht anhand der Metadaten.
pub fn trenner(kopf: &str) -> char {
    if felder(kopf, ';').is_ok_and(|f| f.len() > 1) { ';' } else { ',' }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn zitate_und_mehrzeilige_felder() {
        let werte = ["001", "Baum; Bestand", "Text \"A\"\r\nZeile 2", "#1", " Rand "];
        let text = zeile(werte, ';');
        let s = saetze(&format!("\u{feff}{text}\r\n2;x\r\n")).unwrap();
        assert_eq!(felder(&s[0].1, ';').unwrap(), werte);
        assert_eq!(s[1].0, 3);
        assert!(saetze("a;\"offen").is_err());
        assert!(felder("a;\"x\"falsch", ';').is_err());
    }
}
