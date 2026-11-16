// Zweck:    Schreibt und ergänzt Koordinatendateien im Format {Projekt}-kor.csv (Metadaten-Kopf, Spalten
//           "PunktNr; X; Y; Z; Code", Trennzeichen "; ", Dezimalpunkt). X = Rechtswert, Y = Hochwert, Z = Höhe.
// Hardware: keine
// Autor:    Johann Gerner

#[cfg(test)]
use std::fs;
use std::io;
use std::path::Path;

use chrono::Local;

use crate::grundlagen::format::f;

/// Ein Punkt der Koordinatendatei.
#[derive(Clone, Debug, PartialEq)]
pub struct KorPunkt {
    pub punkt_nr: String,
    pub r: f64,
    pub h: f64,
    pub hoehe: Option<f64>,
    pub code: String,
}

/// Erzeugt den vollständigen Dateitext im gemeinsamen KOR-Schema.
pub fn erzeuge(projekt: &str, sensor: &str, datum: &str, punkte: &[KorPunkt]) -> io::Result<String> {
    let mut d = crate::daten::csv_datei::CsvDatei::neu(crate::daten::csv_datei::TYP_KOORDINATEN);
    d.projekt = projekt.into();
    d.sensor = sensor.into();
    d.datum = datum.into();
    d.zeilen = punkte.iter().map(|p| vec![p.punkt_nr.clone(), f(p.r, 4), f(p.h, 4), p.hoehe.map_or_else(String::new, |h| f(h, 4)), p.code.clone()]).collect();
    d.text()
}

/// Liest Koordinaten nach Spaltennamen, einschließlich Codes und unbekannter Höhe.
pub fn lese_punkte(pfad: &Path) -> io::Result<Vec<KorPunkt>> {
    let d = crate::daten::csv_datei::CsvDatei::laden(pfad)?;
    punkte_aus_datei(&d)
}

pub fn punkte_aus_datei(d: &crate::daten::csv_datei::CsvDatei) -> io::Result<Vec<KorPunkt>> {
    let mut punkte = Vec::new();
    for (i, z) in d.zeilen.iter().enumerate() {
        let nr = d.zelle(z, "PunktNr");
        if nr.trim().is_empty() {
            return Err(crate::daten::csv_basis::fehler(format!("Datensatz {}: PunktNr fehlt.", i + 1)));
        }
        let hoehe = if d.zelle(z, "Z").is_empty() { None } else { Some(d.zahl(i, "Z")?) };
        punkte.push(KorPunkt { punkt_nr: nr.into(), r: d.zahl(i, "X")?, h: d.zahl(i, "Y")?, hoehe, code: d.zelle(z, "Code").into() });
    }
    Ok(punkte)
}

/// Schreibt die Punkte in die Datei. Ist `vorhandene_erhalten` gesetzt und die Datei vorhanden, ersetzen neue Punkte
/// gleicher Nummer die alten (an gleicher Stelle), alle anderen bleiben erhalten. Liefert die Anzahl der Punkte.
pub fn schreibe(pfad: &Path, projekt: &str, sensor: &str, neu: &[KorPunkt], vorhandene_erhalten: bool) -> io::Result<usize> {
    use crate::daten::csv_datei::{CsvDatei, TYP_KOORDINATEN};
    let mut d = if vorhandene_erhalten && pfad.exists() { CsvDatei::laden(pfad)? } else { CsvDatei::neu(TYP_KOORDINATEN) };
    if d.datentyp != TYP_KOORDINATEN {
        return Err(crate::daten::csv_basis::fehler("Die Zieldatei ist keine Koordinatendatei."));
    }
    d.projekt = projekt.into();
    d.sensor = sensor.into();
    d.datum = Local::now().format("%Y-%m-%d").to_string();
    for p in neu {
        let i = match d.zeilen.iter().position(|z| d.zelle(z, "PunktNr").eq_ignore_ascii_case(&p.punkt_nr)) {
            Some(i) => i,
            None => {
                d.zeilen.push(vec![String::new(); d.spalten.len()]);
                d.zeilen.len() - 1
            }
        };
        for (k, v) in [("PunktNr", p.punkt_nr.clone()), ("X", f(p.r, 4)), ("Y", f(p.h, 4)), ("Z", p.hoehe.map_or_else(String::new, |h| f(h, 4))), ("Code", p.code.clone())] {
            d.set_zelle(i, k, v);
        }
    }
    d.pfad = pfad.into();
    d.speichern()?;
    Ok(d.zeilen.len())
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
        let text = erzeuge("Demo", "Sensor", "2026-01-02", &[]).unwrap();
        assert!(text.contains("Projekt: Demo\r\nSensor: Sensor\r\n"));
        assert!(text.contains("# DATEN\r\nPunktNr;X;Y;Z;Code\r\n"));
    }
}
