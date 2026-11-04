// Zweck:    Schreibt und ergänzt Koordinatendateien im Format {Projekt}-kor.csv (Metadaten-Kopf, Spalten
//           "PunktNr; X; Y; Z; Code", Trennzeichen "; ", Dezimalpunkt). X = Rechtswert, Y = Hochwert, Z = Höhe.
// Hardware: keine
// Autor:    Johann Gerner

use std::fs;
use std::io;
use std::path::Path;

use chrono::Local;

use crate::grundlagen::datei_speicher;
use crate::grundlagen::format::{f, parse_zahl};

const SPALTENZEILE: &str = "PunktNr; X; Y; Z; Code";
const SPALTE_NR: usize = 0;
const SPALTE_X: usize = 1;
const SPALTE_Y: usize = 2;
const SPALTE_Z: usize = 3;
const SPALTE_CODE: usize = 4;
const NACHKOMMASTELLEN: usize = 3;

/// Ein Punkt der Koordinatendatei.
#[derive(Clone, Debug, PartialEq)]
pub struct KorPunkt {
    pub punkt_nr: String,
    pub r: f64,
    pub h: f64,
    pub hoehe: Option<f64>,
    pub code: String,
}

/// Erzeugt den vollständigen Dateitext.
pub fn erzeuge(projekt: &str, sensor: &str, datum: &str, punkte: &[KorPunkt]) -> String {
    let mut text = format!(
        "# METADATA\r\nProjekt: {projekt}\r\nSensor: {sensor}\r\nBearbeiter: \r\nDatum: {datum}\r\n---\r\n# DATENTYP\r\nKoordinaten\r\n# DATEN\r\n{SPALTENZEILE}\r\n"
    );
    for p in punkte {
        let z = p.hoehe.map_or_else(String::new, |h| f(h, NACHKOMMASTELLEN));
        text.push_str(&format!("{}; {}; {}; {z}; {}\r\n", p.punkt_nr, f(p.r, NACHKOMMASTELLEN), f(p.h, NACHKOMMASTELLEN), p.code));
    }
    text
}

/// Liest die Datenzeilen einer vorhandenen -kor.csv (Zeilen mit lesbaren X/Y).
pub fn lese_punkte(pfad: &Path) -> io::Result<Vec<KorPunkt>> {
    let text = fs::read_to_string(pfad)?;
    let mut liste = Vec::new();
    let (mut in_daten, mut kopf_gelesen) = (false, false);
    for roh in text.lines() {
        let zeile = roh.trim();
        if zeile.is_empty() || zeile == "---" {
            continue;
        }
        if zeile.starts_with('#') {
            in_daten = zeile.trim_start_matches('#').trim().eq_ignore_ascii_case("DATEN");
            kopf_gelesen = false;
            continue;
        }
        if !in_daten {
            continue;
        }
        if !kopf_gelesen {
            kopf_gelesen = true;
            continue;
        }
        let t: Vec<&str> = zeile.split(';').map(str::trim).collect();
        if t.len() <= SPALTE_Y {
            continue;
        }
        let (Some(x), Some(y)) = (parse_zahl(t[SPALTE_X]), parse_zahl(t[SPALTE_Y])) else {
            continue;
        };
        liste.push(KorPunkt {
            punkt_nr: t[SPALTE_NR].to_string(),
            r: x,
            h: y,
            hoehe: t.get(SPALTE_Z).and_then(|z| parse_zahl(z)),
            code: t.get(SPALTE_CODE).map_or_else(String::new, |c| (*c).to_string()),
        });
    }
    Ok(liste)
}

/// Schreibt die Punkte in die Datei. Ist `vorhandene_erhalten` gesetzt und die Datei vorhanden, ersetzen neue Punkte
/// gleicher Nummer die alten (an gleicher Stelle), alle anderen bleiben erhalten. Liefert die Anzahl der Punkte.
pub fn schreibe(pfad: &Path, projekt: &str, sensor: &str, neu: &[KorPunkt], vorhandene_erhalten: bool) -> io::Result<usize> {
    let mut ergebnis = if vorhandene_erhalten && pfad.exists() { lese_punkte(pfad)? } else { Vec::new() };
    for p in neu {
        match ergebnis.iter().position(|e| e.punkt_nr.eq_ignore_ascii_case(&p.punkt_nr)) {
            Some(i) => ergebnis[i] = p.clone(),
            None => ergebnis.push(p.clone()),
        }
    }
    let datum = Local::now().format("%Y-%m-%d").to_string();
    datei_speicher::schreibe_text(pfad, &erzeuge(projekt, sensor, &datum, &ergebnis))?;
    Ok(ergebnis.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn punkt(nr: &str, r: f64, h: f64, hoehe: Option<f64>, code: &str) -> KorPunkt {
        KorPunkt { punkt_nr: nr.to_string(), r, h, hoehe, code: code.to_string() }
    }

    #[test]
    fn schreibt_liest_und_ergaenzt() {
        let dir = std::env::temp_dir().join(format!("fb-kor-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let pfad = dir.join("Demo-kor.csv");

        let n = schreibe(&pfad, "Demo", "LeicaTs06", &[punkt("1", 10.0, 20.0, Some(5.0), "Baum"), punkt("2", 11.0, 21.0, None, "")], false).unwrap();
        assert_eq!(n, 2);
        let gelesen = lese_punkte(&pfad).unwrap();
        assert_eq!(gelesen[0], punkt("1", 10.0, 20.0, Some(5.0), "Baum"));
        assert_eq!(gelesen[1].hoehe, None);

        // Ergänzen: Punkt 2 wird ersetzt, 3 kommt dazu, 1 bleibt erhalten
        let n = schreibe(&pfad, "Demo", "", &[punkt("2", 99.0, 98.0, None, "neu"), punkt("3", 1.0, 2.0, None, "")], true).unwrap();
        assert_eq!(n, 3);
        let gelesen = lese_punkte(&pfad).unwrap();
        assert_eq!(gelesen.iter().map(|p| p.punkt_nr.as_str()).collect::<Vec<_>>(), ["1", "2", "3"]);
        assert_eq!(gelesen[1].r, 99.0);

        // Neu anlegen verwirft die alten Punkte
        let n = schreibe(&pfad, "Demo", "", &[punkt("9", 1.0, 1.0, None, "")], false).unwrap();
        assert_eq!(n, 1);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn kopf_enthaelt_projekt_und_sensor() {
        let text = erzeuge("Demo", "Sensor", "2026-01-02", &[]);
        assert!(text.starts_with("# METADATA\r\nProjekt: Demo\r\nSensor: Sensor\r\n"));
        assert!(text.contains("# DATEN\r\nPunktNr; X; Y; Z; Code\r\n"));
    }
}
