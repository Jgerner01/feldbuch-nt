// Zweck:    Lesen und Schreiben des Standard-CSV-Formats (Koordinaten *-kor.csv, Messdaten *-dat.csv) mit
//           Metadaten-Kopf sowie Import und Export der Textformate KOR und DAT
// Hardware: keine
// Autor:    Johann Gerner

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use super::csv_basis::{self, fehler};
use chrono::Local;

use crate::grundlagen::datei_speicher;

/// Datentyp Koordinaten.
pub const TYP_KOORDINATEN: &str = "Koordinaten";
/// Datentyp Messdaten.
pub const TYP_MESSDATEN: &str = "Messdaten";
pub const TYP_STANDPUNKT: &str = "Standpunkt";
pub const SPALTEN_STP: [&str; 13] = ["PunktNr", "X", "Y", "Z", "HZ_gon", "V_gon", "Strecke_m", "Zielhoehe_m", "Code", "Zeitstempel", "ExzLaengs_m", "ExzQuer_m", "Typ"];
/// Standardspalten der Koordinatendatei.
pub const SPALTEN_KOR: [&str; 5] = ["PunktNr", "X", "Y", "Z", "Code"];
/// Standardspalten der Messdatei.
pub const SPALTEN_DAT: [&str; 13] =
    ["PunktNr", "HZ_gon", "V_gon", "Strecke_m", "Zielhoehe_m", "Code", "Zeitstempel", "ExzLaengs_m", "ExzQuer_m", "StandpunktNr", "Typ", "AufstellungId", "InstrumentenHoehe_m"];
/// Vorgabe für den Standpunkt, wenn keiner angegeben ist.
const STANDARD_STANDPUNKT: &str = "STP1";
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
    pub metadaten: Vec<(String, String)>,
    /// Kommentare an ihrer Position zwischen den Datenzeilen.
    pub kommentare: Vec<(usize, String)>,
    pub stationierung: Vec<Vec<String>>,
    pub zeilennummern: Vec<usize>,
}

impl CsvDatei {
    /// Neue leere Datei mit den Standardspalten des Datentyps.
    pub fn neu(datentyp: &str) -> Self {
        let spalten: &[&str] = match datentyp {
            TYP_MESSDATEN => &SPALTEN_DAT,
            TYP_STANDPUNKT => &SPALTEN_STP,
            _ => &SPALTEN_KOR,
        };
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
            metadaten: vec![("Formatversion".into(), "1.0".into())],
            kommentare: Vec::new(),
            stationierung: Vec::new(),
            zeilennummern: Vec::new(),
        }
    }

    pub fn ist_messdaten(&self) -> bool {
        self.datentyp == TYP_MESSDATEN
    }

    /// Fügt „-kor" oder „-dat" vor „.csv" ein, wenn der Dateiname noch nicht so endet.
    pub fn normalisiere_pfad(pfad: &Path, datentyp: &str) -> PathBuf {
        let mut basis = pfad.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
        while ["-kor", "-dat", "-stp"].iter().any(|s| basis.to_lowercase().ends_with(s)) {
            basis.truncate(basis.len() - 4);
        }
        let suffix = match datentyp {
            TYP_MESSDATEN => "-dat",
            TYP_STANDPUNKT => "-stp",
            _ => "-kor",
        };
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
        let mut erste = true;
        for roh in fs::read_to_string(pfad)?.lines() {
            let zeile = roh.trim();
            if zeile.is_empty() || zeile.starts_with(['%', '#']) {
                continue;
            }
            if zeile.starts_with("STANDPUNKT ") {
                return Err(fehler("Diese DAT-Variante enthält Koordinaten statt polarer Messungen. Bitte als Koordinatenliste konvertieren."));
            }
            if erste {
                erste = false;
                if zeile.starts_with('-') {
                    let p: Vec<&str> = zeile.trim_start_matches('-').split_whitespace().collect();
                    d.standpunkt_nr = p.first().map_or_else(String::new, |s| (*s).to_string());
                    d.instr_hoehe = p.get(1).map_or_else(String::new, |s| (*s).to_string());
                    d.standpunkt_code = p.get(2).map_or_else(String::new, |s| (*s).to_string());
                    continue;
                }
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
                String::new(),
                EXZ_NULL.into(),
                EXZ_NULL.into(),
                d.standpunkt_nr.clone(),
                String::new(),
                String::new(),
                d.instr_hoehe.clone(),
            ]);
        }
        Ok(d)
    }

    pub fn spalten_index(&self, name: &str) -> Option<usize> {
        csv_basis::index(&self.spalten, name)
    }

    pub fn zelle<'a>(&self, zeile: &'a [String], name: &str) -> &'a str {
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

    /// Standarddateien und alte Tabellen mit Komma/Semikolon.
    pub fn laden(pfad: &Path) -> io::Result<Self> {
        let mut d = Self::parse(&fs::read_to_string(pfad)?)?;
        d.pfad = pfad.to_path_buf();
        Ok(d)
    }

    pub fn parse(text: &str) -> io::Result<Self> {
        let mut d = Self::neu(TYP_KOORDINATEN);
        d.spalten.clear();
        d.metadaten.clear();
        let (mut sektion, mut trenner, mut typ_explizit) = (String::new(), ';', false);
        for (nr, roh) in csv_basis::saetze(text)? {
            let z = roh.trim();
            if z.is_empty() || z == "---" {
                continue;
            }
            if z.starts_with('#') {
                let marker = z.trim_start_matches('#').trim_end_matches([';', ',']).trim().to_uppercase();
                if ["METADATA", "DATENTYP", "DATEN", "STATIONIERUNG"].contains(&marker.as_str()) {
                    sektion = marker;
                } else {
                    d.kommentare.push((d.zeilen.len(), roh));
                }
                continue;
            }
            if z.starts_with('%') {
                d.kommentare.push((d.zeilen.len(), roh));
                continue;
            }
            if sektion == "METADATA" || (d.spalten.is_empty() && z.contains(':')) {
                if let Some((k, v)) = z.split_once(':') {
                    let wert = v.trim();
                    let wert = if wert.ends_with(",,") || wert.ends_with(";;") { wert.trim_end_matches([',', ';']).trim() } else { wert };
                    d.set_meta(k.trim(), wert);
                }
                continue;
            }
            if sektion == "DATENTYP" {
                if !z.to_lowercase().starts_with("format") {
                    d.datentyp = z.to_string();
                    typ_explizit = true;
                }
                continue;
            }
            if sektion == "STATIONIERUNG" {
                d.stationierung.push(csv_basis::felder(&roh, csv_basis::trenner(&roh)).map_err(|e| fehler(format!("Zeile {nr}: {e}")))?);
                continue;
            }
            if d.spalten.is_empty() {
                trenner = csv_basis::trenner(&roh);
                let spalten = csv_basis::felder(&roh, trenner)?;
                if !spalten.iter().any(|s| csv_basis::name(s) == "punktnr") {
                    continue;
                }
                csv_basis::pruefe_spalten(&spalten)?;
                d.spalten = spalten;
            } else {
                let mut felder = csv_basis::felder(&roh, trenner).map_err(|e| fehler(format!("Zeile {nr}: {e}")))?;
                if felder.len() > d.spalten.len() {
                    return Err(fehler(format!("Zeile {nr}: {} Felder, erwartet höchstens {}.", felder.len(), d.spalten.len())));
                }
                felder.resize(d.spalten.len(), String::new());
                d.zeilen.push(felder);
                d.zeilennummern.push(nr);
            }
        }
        if d.spalten.is_empty() {
            return Err(fehler("Keine CSV-Kopfzeile mit PunktNr gefunden."));
        }
        if !typ_explizit {
            d.datentyp = if d.spalten_index("HZ_gon").is_some() { if d.spalten_index("X").is_some() { TYP_STANDPUNKT } else { TYP_MESSDATEN } } else { TYP_KOORDINATEN }.into();
        }
        if ![TYP_KOORDINATEN, TYP_MESSDATEN, TYP_STANDPUNKT].contains(&d.datentyp.as_str()) {
            return Err(fehler(format!("Unbekannter Datentyp: {}", d.datentyp)));
        }
        if let Some(v) = d.meta("Formatversion").filter(|v| !v.is_empty()) {
            if v != "1.0" {
                return Err(fehler(format!("Nicht unterstützte CSV-Formatversion: {v}")));
            }
        }
        Ok(d)
    }

    pub fn meta(&self, k: &str) -> Option<&str> {
        match csv_basis::name(k).as_str() {
            "projekt" => Some(&self.projekt),
            "sensor" => Some(&self.sensor),
            "bearbeiter" => Some(&self.bearbeiter),
            "datum" => Some(&self.datum),
            "standpunktnr" => Some(&self.standpunkt_nr),
            "instrumentenhoehe" => Some(&self.instr_hoehe),
            "standpunktcode" => Some(&self.standpunkt_code),
            _ => self.metadaten.iter().find(|(n, _)| csv_basis::name(n) == csv_basis::name(k)).map(|(_, v)| v.as_str()),
        }
    }

    pub fn set_meta(&mut self, k: &str, wert: &str) {
        match csv_basis::name(k).as_str() {
            "projekt" => self.projekt = wert.into(),
            "sensor" => self.sensor = wert.into(),
            "bearbeiter" => self.bearbeiter = wert.into(),
            "datum" => self.datum = wert.into(),
            "standpunktnr" => self.standpunkt_nr = wert.into(),
            "instrumentenhoehe" => self.instr_hoehe = wert.into(),
            "standpunktcode" => self.standpunkt_code = wert.into(),
            _ => match self.metadaten.iter_mut().find(|(n, _)| csv_basis::name(n) == csv_basis::name(k)) {
                Some((_, v)) => *v = wert.into(),
                None => self.metadaten.push((k.into(), wert.into())),
            },
        }
    }

    pub fn zahl(&self, zeile: usize, spalte: &str) -> io::Result<f64> {
        let wert = self.zeilen.get(zeile).map(|z| self.zelle(z, spalte)).unwrap_or("");
        crate::grundlagen::format::parse_zahl(wert).ok_or_else(|| {
            fehler(format!("Zeile {}, Spalte {spalte}: ungültiger oder fehlender Zahlenwert '{wert}'.", self.zeilennummern.get(zeile).copied().unwrap_or(zeile + 1)))
        })
    }

    pub fn set_zelle(&mut self, zeile: usize, spalte: &str, wert: String) {
        let idx = match self.spalten_index(spalte) {
            Some(i) => i,
            None => {
                self.spalten.push(spalte.into());
                self.spalten.len() - 1
            }
        };
        for z in &mut self.zeilen {
            z.resize(self.spalten.len(), String::new());
        }
        self.zeilen[zeile][idx] = wert;
    }

    pub fn text(&self) -> io::Result<String> {
        csv_basis::pruefe_spalten(&self.spalten)?;
        let standard: &[&str] = match self.datentyp.as_str() {
            TYP_MESSDATEN => &SPALTEN_DAT,
            TYP_STANDPUNKT => &SPALTEN_STP,
            _ => &SPALTEN_KOR,
        };
        let mut ausgabe: Vec<(Option<usize>, String)> = standard.iter().map(|s| (self.spalten_index(s), s.to_string())).collect();
        for (i, spalte) in self.spalten.iter().enumerate() {
            if !ausgabe.iter().any(|(j, _)| *j == Some(i)) {
                ausgabe.push((Some(i), spalte.clone()));
            }
        }
        let mut text = String::from("# METADATA\r\nFormatversion: 1.0\r\n");
        let mut metadaten = vec![("Projekt", self.projekt.as_str()), ("Sensor", &self.sensor), ("Bearbeiter", &self.bearbeiter), ("Datum", &self.datum)];
        if self.ist_messdaten() || self.datentyp == TYP_STANDPUNKT {
            metadaten.extend([("StandpunktNr", self.standpunkt_nr.as_str()), ("InstrumentenHoehe", &self.instr_hoehe), ("StandpunktCode", &self.standpunkt_code)]);
        }
        metadaten.extend(self.metadaten.iter().filter(|(k, _)| csv_basis::name(k) != "formatversion").map(|(k, v)| (k.as_str(), v.as_str())));
        for (k, v) in metadaten {
            if k.contains(['\r', '\n', ':']) || v.contains(['\r', '\n']) {
                return Err(fehler(format!("Metadatum {k}: Zeilenumbrüche sind nicht erlaubt.")));
            }
            text.push_str(&format!("{k}: {v}\r\n"));
        }
        text.push_str(&format!("---\r\n# DATENTYP\r\n{}\r\n# DATEN\r\n{}\r\n", self.datentyp, csv_basis::zeile(ausgabe.iter().map(|(_, s)| s), ';')));
        for i in 0..=self.zeilen.len() {
            for (_, kommentar) in self.kommentare.iter().filter(|(pos, _)| (*pos).min(self.zeilen.len()) == i) {
                text.push_str(kommentar);
                text.push_str("\r\n");
            }
            if let Some(z) = self.zeilen.get(i) {
                if z.len() > self.spalten.len() {
                    return Err(fehler(format!("Datensatz {}: mehr Felder als Spalten.", i + 1)));
                }
                let mut felder = z.clone();
                felder.resize(self.spalten.len(), String::new());
                let aus: Vec<_> = ausgabe
                    .iter()
                    .map(|(j, name)| csv_basis::zahlenfeld(name, j.and_then(|j| felder.get(j)).map_or("", String::as_str)).map_err(|e| fehler(format!("Datensatz {}: {e}", i + 1))))
                    .collect::<io::Result<_>>()?;
                text.push_str(&csv_basis::zeile(&aus, ';'));
                text.push_str("\r\n");
            }
        }
        if !self.stationierung.is_empty() {
            text.push_str("# STATIONIERUNG\r\n");
            for z in &self.stationierung {
                text.push_str(&csv_basis::zeile(z, ';'));
                text.push_str("\r\n");
            }
        }
        Ok(text)
    }

    pub fn speichern(&self) -> io::Result<()> {
        if self.pfad.as_os_str().is_empty() {
            return Ok(());
        }
        datei_speicher::schreibe_text(&self.pfad, &self.text()?)
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
        assert_eq!(p("a/Demo-KOR.csv", TYP_MESSDATEN), "a/Demo-dat.csv");
        assert_eq!(p("a/Demo-kor-DAT.CSV", TYP_STANDPUNKT), "a/Demo-stp.csv");
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
