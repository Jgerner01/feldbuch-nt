// Zweck:    Lesen und Schreiben des Standard-CSV-Formats (Koordinaten *-kor.csv, Messdaten *-dat.csv) mit
//           Metadaten-Kopf sowie Import und Export der Textformate KOR und DAT
// Hardware: keine
// Autor:    Johann Gerner

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use chrono::Local;

use crate::grundlagen::datei_speicher;

/// Datentyp Koordinaten.
pub const TYP_KOORDINATEN: &str = "Koordinaten";
/// Datentyp Messdaten.
pub const TYP_MESSDATEN: &str = "Messdaten";
/// Standardspalten der Koordinatendatei.
pub const SPALTEN_KOR: [&str; 5] = ["PunktNr", "X", "Y", "Z", "Code"];
/// Standardspalten der Messdatei.
pub const SPALTEN_DAT: [&str; 11] =
    ["PunktNr", "HZ_gon", "V_gon", "Strecke_m", "Zielhoehe_m", "Code", "Zeitstempel", "ExzLaengs_m", "ExzQuer_m", "StandpunktNr", "Typ"];
/// Vorgabe für den Standpunkt, wenn keiner angegeben ist.
const STANDARD_STANDPUNKT: &str = "STP";
const STANDARD_INSTR_HOEHE: &str = "0.000";
/// Nullwert für Exzentrizitäten beim DAT-Import.
const EXZ_NULL: &str = "0.000";
/// Mindestanzahl Felder einer KOR-Zeile (Nr, X, Y, Z) bzw. DAT-Zeile (Nr, Hz, V, Strecke, Zielhöhe).
const MIN_FELDER_KOR: usize = 4;
const MIN_FELDER_DAT: usize = 5;

/// Standard-CSV-Datei mit Metadaten.
#[derive(Clone, Debug, PartialEq)]
pub struct CsvDatei {
    pub projekt: String,
    pub sensor: String,
    pub bearbeiter: String,
    pub datum: String,
    pub datentyp: String,
    /// Nur Messdaten: Standpunktzeile.
    pub standpunkt_nr: String,
    pub instr_hoehe: String,
    pub standpunkt_code: String,
    pub spalten: Vec<String>,
    pub zeilen: Vec<Vec<String>>,
    pub pfad: PathBuf,
}

impl CsvDatei {
    /// Neue leere Datei mit den Standardspalten des Datentyps.
    pub fn neu(datentyp: &str) -> Self {
        let spalten: &[&str] = if datentyp == TYP_MESSDATEN { &SPALTEN_DAT } else { &SPALTEN_KOR };
        Self {
            projekt: String::new(),
            sensor: String::new(),
            bearbeiter: String::new(),
            datum: Local::now().format("%Y-%m-%d").to_string(),
            datentyp: datentyp.to_string(),
            standpunkt_nr: String::new(),
            instr_hoehe: String::new(),
            standpunkt_code: String::new(),
            spalten: spalten.iter().map(|s| (*s).to_string()).collect(),
            zeilen: Vec::new(),
            pfad: PathBuf::new(),
        }
    }

    pub fn ist_messdaten(&self) -> bool {
        self.datentyp == TYP_MESSDATEN
    }

    /// Fügt „-kor" oder „-dat" vor „.csv" ein, wenn der Dateiname noch nicht so endet.
    pub fn normalisiere_pfad(pfad: &Path, datentyp: &str) -> PathBuf {
        let basis = pfad.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
        let klein = basis.to_lowercase();
        if klein.ends_with("-kor") || klein.ends_with("-dat") {
            return pfad.to_path_buf();
        }
        let suffix = if datentyp == TYP_MESSDATEN { "-dat" } else { "-kor" };
        pfad.with_file_name(format!("{basis}{suffix}.csv"))
    }

    fn csv_pfad_zu(pfad: &Path, datentyp: &str) -> PathBuf {
        let stamm = pfad.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
        Self::normalisiere_pfad(&pfad.with_file_name(format!("{stamm}.csv")), datentyp)
    }

    /// Importiert eine KOR-Textdatei (Nr X Y Z [Code], leerzeichengetrennt, ohne Kopf).
    pub fn importiere_kor(pfad: &Path) -> io::Result<Self> {
        let mut d = Self::neu(TYP_KOORDINATEN);
        d.pfad = Self::csv_pfad_zu(pfad, TYP_KOORDINATEN);
        for roh in fs::read_to_string(pfad)?.lines() {
            let zeile = roh.trim();
            if zeile.is_empty() || zeile.starts_with(['%', '#', ';']) {
                continue;
            }
            let p: Vec<&str> = zeile.split_whitespace().collect();
            if p.len() < MIN_FELDER_KOR {
                continue;
            }
            d.zeilen.push(vec![p[0].into(), p[1].into(), p[2].into(), p[3].into(), p.get(4).map_or_else(String::new, |c| (*c).to_string())]);
        }
        Ok(d)
    }

    /// Importiert eine DAT-Textdatei (erste Zeile `-<PunktNr> <InstrHöhe> [Code]`, dann Messzeilen).
    pub fn importiere_dat(pfad: &Path) -> io::Result<Self> {
        let mut d = Self::neu(TYP_MESSDATEN);
        d.pfad = Self::csv_pfad_zu(pfad, TYP_MESSDATEN);
        let zeitstempel = Local::now().format("%Y-%m-%d %H:%M").to_string();
        let mut erste = true;
        for roh in fs::read_to_string(pfad)?.lines() {
            let zeile = roh.trim();
            if zeile.is_empty() {
                continue;
            }
            if erste {
                erste = false;
                if zeile.starts_with('-') {
                    let p: Vec<&str> = zeile.trim_start_matches('-').split_whitespace().collect();
                    d.standpunkt_nr = p.first().map_or_else(String::new, |s| (*s).to_string());
                    d.instr_hoehe = p.get(1).map_or_else(String::new, |s| (*s).to_string());
                    d.standpunkt_code = p.get(2).map_or_else(String::new, |s| (*s).to_string());
                }
                continue;
            }
            let t: Vec<&str> = zeile.split_whitespace().collect();
            if t.len() < MIN_FELDER_DAT {
                continue;
            }
            d.zeilen.push(vec![
                t[0].into(),
                t[1].into(),
                t[2].into(),
                t[3].into(),
                t[4].into(),
                t.get(5).map_or_else(String::new, |c| (*c).to_string()),
                zeitstempel.clone(),
                EXZ_NULL.into(),
                EXZ_NULL.into(),
                d.standpunkt_nr.clone(),
                String::new(),
            ]);
        }
        Ok(d)
    }

    fn spalten_index(&self, name: &str) -> Option<usize> {
        self.spalten.iter().position(|s| s.eq_ignore_ascii_case(name))
    }

    fn zelle<'a>(&self, zeile: &'a [String], name: &str) -> &'a str {
        self.spalten_index(name).and_then(|i| zeile.get(i)).map_or("", String::as_str)
    }

    /// Exportiert als KOR-Textdatei (Spalten fest ausgerichtet).
    pub fn exportiere_kor(&self, pfad: &Path) -> io::Result<()> {
        let mut text = String::new();
        for z in &self.zeilen {
            text.push_str(&format!(
                "{:<8}{:>16}{:>16}{:>10}    {}\r\n",
                self.zelle(z, "PunktNr"),
                self.zelle(z, "X"),
                self.zelle(z, "Y"),
                self.zelle(z, "Z"),
                self.zelle(z, "Code")
            ));
        }
        datei_speicher::schreibe_text(pfad, &text)
    }

    /// Exportiert als DAT-Textdatei mit Standpunktzeile.
    pub fn exportiere_dat(&self, pfad: &Path) -> io::Result<()> {
        let nr = if self.standpunkt_nr.is_empty() { STANDARD_STANDPUNKT } else { &self.standpunkt_nr };
        let h = if self.instr_hoehe.is_empty() { STANDARD_INSTR_HOEHE } else { &self.instr_hoehe };
        let code = if self.standpunkt_code.is_empty() { String::new() } else { format!(" {}", self.standpunkt_code) };
        let mut text = format!("-{nr} {h}{code}\r\n");
        for z in &self.zeilen {
            text.push_str(&format!(
                " {:>3}   {:>9}   {:>9}   {:>9}   {:>7}  {}\r\n",
                self.zelle(z, "PunktNr"),
                self.zelle(z, "HZ_gon"),
                self.zelle(z, "V_gon"),
                self.zelle(z, "Strecke_m"),
                self.zelle(z, "Zielhoehe_m"),
                self.zelle(z, "Code")
            ));
        }
        datei_speicher::schreibe_text(pfad, &text)
    }

    /// Lädt eine Standard-CSV-Datei.
    pub fn laden(pfad: &Path) -> io::Result<Self> {
        let mut d = Self::neu(TYP_KOORDINATEN);
        d.spalten.clear();
        d.pfad = pfad.to_path_buf();
        let mut sektion = String::new();
        let mut daten_kopf = false;
        for roh in fs::read_to_string(pfad)?.lines() {
            let zeile = roh.trim();
            if zeile.is_empty() {
                continue;
            }
            if zeile.starts_with('#') {
                sektion = zeile.trim_start_matches('#').trim().to_uppercase();
                daten_kopf = false;
                continue;
            }
            if zeile == "---" {
                continue;
            }
            match sektion.as_str() {
                "METADATA" => {
                    let Some(i) = zeile.find(':') else {
                        continue;
                    };
                    let (schluessel, wert) = (zeile[..i].trim().to_lowercase(), zeile[i + 1..].trim().to_string());
                    match schluessel.as_str() {
                        "projekt" => d.projekt = wert,
                        "sensor" => d.sensor = wert,
                        "bearbeiter" => d.bearbeiter = wert,
                        "datum" => d.datum = wert,
                        "standpunktnr" => d.standpunkt_nr = wert,
                        "instrumentenhoehe" => d.instr_hoehe = wert,
                        "standpunktcode" => d.standpunkt_code = wert,
                        _ => {}
                    }
                }
                "DATENTYP" => {
                    if !zeile.to_lowercase().starts_with("format") {
                        d.datentyp = zeile.to_string();
                    }
                }
                "DATEN" => {
                    let felder: Vec<String> = zeile.split(';').map(|s| s.trim().to_string()).collect();
                    if daten_kopf {
                        d.zeilen.push(felder);
                    } else {
                        d.spalten = felder;
                        daten_kopf = true;
                    }
                }
                _ => {}
            }
        }
        Ok(d)
    }

    /// Schreibt die Datei an `pfad`; ohne Pfad geschieht nichts.
    pub fn speichern(&self) -> io::Result<()> {
        if self.pfad.as_os_str().is_empty() {
            return Ok(());
        }
        let mut text = format!(
            "# METADATA\r\nProjekt: {}\r\nSensor: {}\r\nBearbeiter: {}\r\nDatum: {}\r\n",
            self.projekt, self.sensor, self.bearbeiter, self.datum
        );
        if self.ist_messdaten() {
            text.push_str(&format!("StandpunktNr: {}\r\nInstrumentenHoehe: {}\r\nStandpunktCode: {}\r\n", self.standpunkt_nr, self.instr_hoehe, self.standpunkt_code));
        }
        text.push_str(&format!("---\r\n# DATENTYP\r\n{}\r\n# DATEN\r\n", self.datentyp));
        if !self.spalten.is_empty() {
            text.push_str(&format!("{}\r\n", self.spalten.join("; ")));
        }
        for zeile in &self.zeilen {
            let mut felder = zeile.clone();
            while felder.len() < self.spalten.len() {
                felder.push(String::new());
            }
            text.push_str(&format!("{}\r\n", felder.join("; ")));
        }
        datei_speicher::schreibe_text(&self.pfad, &text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("fb-csv-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn pfad_wird_normalisiert() {
        let p = |s: &str, t: &str| CsvDatei::normalisiere_pfad(Path::new(s), t).to_string_lossy().replace('\\', "/");
        assert_eq!(p("a/Demo.csv", TYP_KOORDINATEN), "a/Demo-kor.csv");
        assert_eq!(p("a/Demo.csv", TYP_MESSDATEN), "a/Demo-dat.csv");
        assert_eq!(p("a/Demo-KOR.csv", TYP_MESSDATEN), "a/Demo-KOR.csv");
    }

    #[test]
    fn speichern_und_laden_rund() {
        let dir = temp("rund");
        let mut d = CsvDatei::neu(TYP_MESSDATEN);
        d.pfad = dir.join("T-dat.csv");
        d.projekt = "Demo".into();
        d.sensor = "TS06".into();
        d.standpunkt_nr = "SP1".into();
        d.instr_hoehe = "1.556".into();
        d.zeilen.push(vec!["1".into(), "10.0".into(), "100.0".into(), "25.5".into(), "1.5".into(), "Baum".into()]);
        d.speichern().unwrap();
        let g = CsvDatei::laden(&d.pfad).unwrap();
        assert_eq!((g.projekt.as_str(), g.sensor.as_str(), g.standpunkt_nr.as_str(), g.instr_hoehe.as_str()), ("Demo", "TS06", "SP1", "1.556"));
        assert_eq!(g.datentyp, TYP_MESSDATEN);
        assert_eq!(g.spalten, d.spalten);
        assert_eq!(g.zeilen.len(), 1);
        assert_eq!(g.zeilen[0][0], "1");
        assert_eq!(g.zeilen[0][5], "Baum");
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn kor_import_und_export() {
        let dir = temp("kor");
        let quelle = dir.join("Punkte.kor");
        fs::write(&quelle, "% Kommentar\n1   100.000  200.000  10.000  Baum\n2 1 2 3\nzu kurz 1\n").unwrap();
        let d = CsvDatei::importiere_kor(&quelle).unwrap();
        assert_eq!(d.zeilen.len(), 2);
        assert_eq!(d.zeilen[1], vec!["2", "1", "2", "3", ""]);
        assert!(d.pfad.to_string_lossy().ends_with("Punkte-kor.csv"));
        let ziel = dir.join("Ausgabe.kor");
        d.exportiere_kor(&ziel).unwrap();
        let text = fs::read_to_string(&ziel).unwrap();
        assert_eq!(text.lines().next().unwrap(), format!("{:<8}{:>16}{:>16}{:>10}    Baum", "1", "100.000", "200.000", "10.000"));
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn dat_import_und_export() {
        let dir = temp("dat");
        let quelle = dir.join("Messung.dat");
        fs::write(&quelle, "-STP5 1.600 Stand\n 1  10.5  100.0  25.0  1.5  Baum\n 2  20.5  99.0  30.0  1.5\n kurz 1 2\n").unwrap();
        let d = CsvDatei::importiere_dat(&quelle).unwrap();
        assert_eq!((d.standpunkt_nr.as_str(), d.instr_hoehe.as_str(), d.standpunkt_code.as_str()), ("STP5", "1.600", "Stand"));
        assert_eq!(d.zeilen.len(), 2);
        assert_eq!(d.zeilen[0][9], "STP5");
        assert_eq!(d.zeilen[0].len(), SPALTEN_DAT.len());
        let ziel = dir.join("Aus.dat");
        d.exportiere_dat(&ziel).unwrap();
        let text = fs::read_to_string(&ziel).unwrap();
        let mut zeilen = text.lines();
        assert_eq!(zeilen.next().unwrap(), "-STP5 1.600 Stand");
        assert!(zeilen.next().unwrap().ends_with("Baum"));
        let _ = fs::remove_dir_all(dir);
    }
}
