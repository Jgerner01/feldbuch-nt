// Zweck:    Projektdaten.csv lesen und schreiben (Datum, Uhrzeit, Bearbeiter, Kategorie, Parameter, Wert)
// Hardware: keine
// Autor:    Johann Gerner

use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use chrono::Local;

use crate::grundlagen::datei_speicher;

/// Kopfzeile der CSV-Datei.
const KOPFZEILE: [&str; 6] = ["Datum", "Uhrzeit", "Bearbeiter", "Kategorie", "Parameter", "Wert"];
/// Anzahl der Spalten.
const SPALTEN: usize = 6;

/// Ein Eintrag der Projektdaten.
#[derive(Clone, Debug, PartialEq)]
pub struct ProjektEintrag {
    pub datum: String,
    pub uhrzeit: String,
    pub bearbeiter: String,
    pub kategorie: String,
    pub parameter: String,
    pub wert: String,
}

/// Verwaltet die Projektdaten eines Projekts.
pub struct ProjektdatenManager {
    pfad: PathBuf,
    eintraege: Vec<ProjektEintrag>,
    pub bearbeiter: String,
}

impl ProjektdatenManager {
    /// Leerer Manager ohne Datei (kein Projekt geladen).
    pub fn leer() -> Self {
        Self {
            pfad: PathBuf::new(),
            eintraege: Vec::new(),
            bearbeiter: benutzername(),
        }
    }

    /// Lädt die Datei oder legt sie an.
    pub fn initialisiere(pfad: &Path) -> Self {
        let mut m = Self {
            pfad: pfad.to_path_buf(),
            eintraege: Vec::new(),
            bearbeiter: benutzername(),
        };
        if pfad.exists() {
            m.laden();
        } else {
            datei_speicher::laden_erfolgreich(pfad);
            m.intern_add("System", "Erstellt", "Projekt angelegt");
            let _ = m.speichern();
        }
        m
    }

    /// Letzter Wert zu Kategorie und Parameter (ohne Beachtung der Groß-/Kleinschreibung).
    pub fn get_value(&self, kategorie: &str, parameter: &str) -> Option<&str> {
        self.eintraege
            .iter()
            .rev()
            .find(|e| {
                e.kategorie.eq_ignore_ascii_case(kategorie) && e.parameter.eq_ignore_ascii_case(parameter)
            })
            .map(|e| e.wert.as_str())
    }

    /// Alle Einträge.
    pub fn get_all(&self) -> &[ProjektEintrag] {
        &self.eintraege
    }

    /// Schreibt einen Wert.
    pub fn set_value(&mut self, kategorie: &str, parameter: &str, wert: &str) -> io::Result<()> {
        self.set_values(kategorie, &[(parameter, wert.to_string())])
    }

    /// Schreibt zusammengehörige Parameter in einem Dateivorgang.
    pub fn set_values(&mut self, kategorie: &str, werte: &[(&str, String)]) -> io::Result<()> {
        datei_speicher::pruefe_schreibzugriff(&self.pfad)?;
        let vorher = self.eintraege.clone();
        for (parameter, wert) in werte {
            self.intern_add(kategorie, parameter, wert);
        }
        self.speichern().inspect_err(|_| self.eintraege = vorher)
    }

    /// Ersetzt alle Einträge (für den Editor).
    pub fn replace_all(&mut self, liste: Vec<ProjektEintrag>) -> io::Result<()> {
        datei_speicher::pruefe_schreibzugriff(&self.pfad)?;
        let vorher = std::mem::replace(&mut self.eintraege, liste);
        self.speichern().inspect_err(|_| self.eintraege = vorher)
    }

    fn intern_add(&mut self, kategorie: &str, parameter: &str, wert: &str) {
        let jetzt = Local::now();
        self.eintraege.push(ProjektEintrag {
            datum: jetzt.format("%Y-%m-%d").to_string(),
            uhrzeit: jetzt.format("%H:%M:%S").to_string(),
            bearbeiter: self.bearbeiter.clone(),
            kategorie: kategorie.to_string(),
            parameter: parameter.to_string(),
            wert: wert.to_string(),
        });
    }

    fn laden(&mut self) {
        match fs::read_to_string(&self.pfad) {
            Ok(text) => match parse_csv(&text) {
                Ok(e) => {
                    self.eintraege = e;
                    datei_speicher::laden_erfolgreich(&self.pfad);
                }
                Err(fehler) => datei_speicher::melde_ladefehler(&self.pfad, &fehler),
            },
            Err(e) => datei_speicher::melde_ladefehler(&self.pfad, &e),
        }
    }

    /// Speichert die Datei.
    pub fn speichern(&self) -> io::Result<()> {
        if self.pfad.as_os_str().is_empty() {
            return Ok(());
        }
        datei_speicher::pruefe_schreibzugriff(&self.pfad)?;
        let mut text = KOPFZEILE.join(",");
        text.push_str("\r\n");
        for e in &self.eintraege {
            let felder = [&e.datum, &e.uhrzeit, &e.bearbeiter, &e.kategorie, &e.parameter, &e.wert];
            let zeile: Vec<String> = felder.iter().map(|f| maskiere(f)).collect();
            text.push_str(&zeile.join(","));
            text.push_str("\r\n");
        }
        datei_speicher::schreibe_text(&self.pfad, &text)
    }
}

/// Windows-Benutzername als Bearbeiter.
fn benutzername() -> String {
    std::env::var("USERNAME").unwrap_or_else(|_| "unbekannt".to_string())
}

/// Maskiert ein CSV-Feld bei Bedarf.
fn maskiere(text: &str) -> String {
    if text.contains([',', '"', '\r', '\n']) {
        format!("\"{}\"", text.replace('"', "\"\""))
    } else {
        text.to_string()
    }
}

/// Zerlegt CSV-Text in Datensätze (Anführungszeichen, doppelte Anführungszeichen, Zeilenumbrüche in Feldern).
fn zerlege_csv(text: &str) -> Result<Vec<Vec<String>>, String> {
    let text = text.trim_start_matches('\u{feff}');
    let mut datensaetze = Vec::new();
    let mut feld = String::new();
    let mut zeile: Vec<String> = Vec::new();
    let mut in_anfuehrung = false;
    let mut zeichen = text.chars().peekable();
    let mut zeile_offen = false;

    while let Some(c) = zeichen.next() {
        if in_anfuehrung {
            match c {
                '"' if zeichen.peek() == Some(&'"') => {
                    feld.push('"');
                    zeichen.next();
                }
                '"' => in_anfuehrung = false,
                _ => feld.push(c),
            }
            continue;
        }
        match c {
            '"' => {
                in_anfuehrung = true;
                zeile_offen = true;
            }
            ',' => {
                zeile.push(std::mem::take(&mut feld));
                zeile_offen = true;
            }
            '\r' => {}
            '\n' => {
                if zeile_offen || !feld.is_empty() {
                    zeile.push(std::mem::take(&mut feld));
                    datensaetze.push(std::mem::take(&mut zeile));
                }
                zeile_offen = false;
            }
            _ => {
                feld.push(c);
                zeile_offen = true;
            }
        }
    }
    if in_anfuehrung {
        return Err("Nicht geschlossene Anführungszeichen in Projektdaten.csv.".to_string());
    }
    if zeile_offen || !feld.is_empty() {
        zeile.push(feld);
        datensaetze.push(zeile);
    }
    Ok(datensaetze)
}

fn parse_csv(text: &str) -> Result<Vec<ProjektEintrag>, String> {
    let mut saetze = zerlege_csv(text)?.into_iter();
    let kopf = saetze.next().ok_or("Ungültige Kopfzeile in Projektdaten.csv.")?;
    if kopf != KOPFZEILE {
        return Err("Ungültige Kopfzeile in Projektdaten.csv.".to_string());
    }
    let mut eintraege = Vec::new();
    for teile in saetze {
        if teile.len() < SPALTEN {
            return Err("Unvollständiger Projektdatensatz.".to_string());
        }
        // Ältere Dateien konnten ein unquotiertes Komma im letzten Feld enthalten.
        eintraege.push(ProjektEintrag {
            datum: teile[0].clone(),
            uhrzeit: teile[1].clone(),
            bearbeiter: teile[2].clone(),
            kategorie: teile[3].clone(),
            parameter: teile[4].clone(),
            wert: teile[5..].join(","),
        });
    }
    Ok(eintraege)
}

/// Werte als Zuordnung Parameter → Wert für eine Kategorie (letzte Einträge gewinnen).
pub fn kategorie_werte(eintraege: &[ProjektEintrag], kategorie: &str) -> HashMap<String, String> {
    eintraege
        .iter()
        .filter(|e| e.kategorie.eq_ignore_ascii_case(kategorie))
        .map(|e| (e.parameter.clone(), e.wert.clone()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn csv_mit_komma_und_anfuehrung_laeuft_rund() {
        let dir = std::env::temp_dir().join(format!("fb-pd-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let pfad = dir.join("Projektdaten.csv");
        let mut m = ProjektdatenManager::initialisiere(&pfad);
        m.set_value("Test", "Text", "a,b \"c\"\nzweite Zeile").unwrap();
        m.set_value("Test", "Zahl", "1.5").unwrap();

        let neu = ProjektdatenManager::initialisiere(&pfad);
        assert_eq!(neu.get_value("test", "TEXT"), Some("a,b \"c\"\nzweite Zeile"));
        assert_eq!(neu.get_value("Test", "Zahl"), Some("1.5"));
        assert_eq!(neu.get_all().len(), 3);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn letzter_wert_gewinnt_und_kopfzeile_wird_geprueft() {
        let text = "Datum,Uhrzeit,Bearbeiter,Kategorie,Parameter,Wert\r\n\
                    d,u,b,K,P,alt\r\nd,u,b,K,P,neu,mit,komma\r\n";
        let e = parse_csv(text).unwrap();
        assert_eq!(e[1].wert, "neu,mit,komma");
        assert!(parse_csv("falsch\r\n").is_err());
        assert!(parse_csv("Datum,Uhrzeit,Bearbeiter,Kategorie,Parameter,Wert\r\nnur,drei,felder\r\n").is_err());
    }
}
