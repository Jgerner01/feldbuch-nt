// Zweck:    Daten des Aufnahmetests: lokale Koordinaten aus Hz/V/Strecke (Instrument = Ursprung), Richtung aus Koordinaten,
//           Speicherung als aufnahmetest.json (Format der C#-Version) und Export als KOR-/DAT-Datei
// Hardware: keine
// Autor:    Johann Gerner

use std::f64::consts::PI;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::json_speicher;
use crate::grundlagen::format::f;

const GON_ZU_RAD: f64 = PI / 200.0;
const RAD_ZU_GON: f64 = 200.0 / PI;
const VOLLKREIS: f64 = 400.0;
/// Zenitwinkel einer waagerechten Richtung [gon].
pub const V_WAAGERECHT: f64 = 100.0;
/// Name der Datei im Projektverzeichnis.
const DATEINAME: &str = "aufnahmetest.json";

/// Ein aufgenommener Punkt (lokales System: X = Ost, Y = Nord, Z = Höhe über dem Instrument).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct AufnahmePunkt {
    #[serde(default)]
    pub nr: i64,
    #[serde(rename = "punktNr", default)]
    pub punkt_nr: String,
    #[serde(default)]
    pub code: String,
    #[serde(default)]
    pub x: f64,
    #[serde(default)]
    pub y: f64,
    #[serde(default)]
    pub z: f64,
    /// Schrägstrecke [m].
    #[serde(default)]
    pub d: f64,
    #[serde(default)]
    pub hz: f64,
    #[serde(default)]
    pub v: f64,
}

/// Rundet auf `stellen` Nachkommastellen; ein negatives Null („-0.0000“) wird zu 0.
fn runde(wert: f64, stellen: i32) -> f64 {
    let faktor = 10f64.powi(stellen);
    (wert * faktor).round() / faktor + 0.0
}

/// Lokale Koordinaten (X, Y, Z) aus Hz, V [gon] und Schrägstrecke [m], auf 4 Stellen gerundet.
pub fn lokale_koordinaten(hz_gon: f64, v_gon: f64, strecke: f64) -> (f64, f64, f64) {
    let (hz, v) = (hz_gon * GON_ZU_RAD, v_gon * GON_ZU_RAD);
    let horizontal = strecke * v.sin();
    (runde(horizontal * hz.sin(), 4), runde(horizontal * hz.cos(), 4), runde(strecke * v.cos(), 4))
}

/// Umkehrung: (Strecke, Hz, V) aus lokalen Koordinaten; Strecke auf 3, Winkel auf 4 Stellen gerundet.
pub fn richtung_aus_koordinaten(x: f64, y: f64, z: f64) -> (f64, f64, f64) {
    let horizontal = x.hypot(y);
    let strecke = horizontal.hypot(z);
    if strecke < 1e-12 {
        return (0.0, 0.0, V_WAAGERECHT);
    }
    let hz = (x.atan2(y) * RAD_ZU_GON).rem_euclid(VOLLKREIS);
    let v = horizontal.atan2(z) * RAD_ZU_GON;
    (runde(strecke, 3), runde(hz, 4), runde(v, 4))
}

impl AufnahmePunkt {
    /// Punkt aus einer Messung (Hz, V, Schrägstrecke).
    pub fn aus_messung(nr: i64, punkt_nr: &str, code: &str, hz: f64, v: f64, strecke: f64) -> Self {
        let (x, y, z) = lokale_koordinaten(hz, v, strecke);
        Self { nr, punkt_nr: punkt_nr.to_string(), code: code.to_string(), x, y, z, d: runde(strecke, 3), hz: runde(hz, 4), v: runde(v, 4) }
    }

    /// Leerer Punkt (alle Koordinaten 0) mit gegebenem Zenitwinkel.
    pub fn leer(nr: i64, v: f64) -> Self {
        Self { nr, punkt_nr: format!("P{nr:03}"), v, ..Self::default() }
    }

    /// Berechnet Strecke, Hz und V aus den Koordinaten neu (nach Bearbeiten von X, Y oder Z).
    pub fn berechne_richtung(&mut self) {
        let (d, hz, v) = richtung_aus_koordinaten(self.x, self.y, self.z);
        self.d = d;
        self.hz = hz;
        self.v = v;
    }
}

/// Nummeriert die Zeilen neu (1, 2, 3, …).
pub fn nummeriere(punkte: &mut [AufnahmePunkt]) {
    for (i, p) in punkte.iter_mut().enumerate() {
        p.nr = i as i64 + 1;
    }
}

/// Pfad der Datei: Projektverzeichnis, sonst Programmordner.
pub fn pfad(projekt_verzeichnis: Option<&Path>, basis: &Path) -> PathBuf {
    projekt_verzeichnis.unwrap_or(basis).join(DATEINAME)
}

/// Lädt die Punkte; eine fehlende oder defekte Datei ergibt eine leere Liste.
pub fn lade(pfad: &Path) -> Vec<AufnahmePunkt> {
    if !pfad.exists() {
        return Vec::new();
    }
    json_speicher::lade::<Vec<AufnahmePunkt>>(pfad).unwrap_or_default()
}

/// Speichert die Punkte.
pub fn speichere(pfad: &Path, punkte: &[AufnahmePunkt]) -> std::io::Result<()> {
    json_speicher::speichere(pfad, &punkte, "Aufnahmetest.speichern")
}

/// Kopfangaben der Exportdateien.
pub struct ExportKopf<'a> {
    pub projekt: &'a str,
    pub sensor: &'a str,
    pub bearbeiter: &'a str,
    /// Datum (yyyy-MM-dd).
    pub datum: &'a str,
    /// Zeitstempel (yyyy-MM-dd HH:mm).
    pub zeitstempel: &'a str,
}

/// Gemeinsame Koordinaten- und Messdatenschemata, lokaler Aufnahmekontext.
pub fn kor_zeilen(punkte: &[AufnahmePunkt], k: &ExportKopf) -> Vec<String> {
    let mut d = export_datei(k, crate::daten::csv_datei::TYP_KOORDINATEN);
    d.zeilen = punkte.iter().map(|p| vec![p.punkt_nr.clone(), f(p.x, 4), f(p.y, 4), f(p.z, 4), p.code.clone()]).collect();
    vec![d.text().expect("Feste Exportspalten")]
}
fn export_datei(k: &ExportKopf, typ: &str) -> crate::daten::csv_datei::CsvDatei {
    let mut d = crate::daten::csv_datei::CsvDatei::neu(typ);
    d.projekt = k.projekt.into();
    d.sensor = k.sensor.into();
    d.bearbeiter = k.bearbeiter.into();
    d.datum = k.datum.into();
    d.set_meta("Koordinatensystem", "Lokal");
    d
}
pub fn dat_zeilen(punkte: &[AufnahmePunkt], k: &ExportKopf) -> Vec<String> {
    let mut d = export_datei(k, crate::daten::csv_datei::TYP_MESSDATEN);
    let id = format!("Aufnahmetest-{}", k.zeitstempel);
    d.zeilen = punkte
        .iter()
        .map(|p| {
            vec![
                p.punkt_nr.clone(),
                f(p.hz, 4),
                f(p.v, 4),
                f(p.d, 3),
                "0.000".into(),
                p.code.clone(),
                k.zeitstempel.into(),
                "0.000".into(),
                "0.000".into(),
                "Lokal".into(),
                "NP".into(),
                id.clone(),
                "0.000".into(),
            ]
        })
        .collect();
    d.set_meta("Quelle", "Aufnahmetest (synthetische lokale Aufstellung)");
    vec![d.text().expect("Feste Exportspalten")]
}
pub fn export_pfade(kor_pfad: &Path) -> (PathBuf, PathBuf) {
    use crate::daten::csv_datei::{CsvDatei, TYP_KOORDINATEN, TYP_MESSDATEN};
    (CsvDatei::normalisiere_pfad(kor_pfad, TYP_KOORDINATEN), CsvDatei::normalisiere_pfad(kor_pfad, TYP_MESSDATEN))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn koordinaten_aus_hz_v_und_strecke() {
        // Osten, waagerecht, 10 m
        let (x, y, z) = lokale_koordinaten(100.0, 100.0, 10.0);
        assert_eq!((x, y, z), (10.0, 0.0, 0.0));
        // Norden, 45° nach oben (V = 50 gon): horizontal 10·sin 50 gon
        let (x, y, z) = lokale_koordinaten(0.0, 50.0, 10.0);
        assert_eq!(x, 0.0);
        assert!((y - 7.0711).abs() < 1e-9 && (z - 7.0711).abs() < 1e-9, "{y} {z}");
        // Zenit
        assert_eq!(lokale_koordinaten(123.0, 0.0, 5.0), (0.0, 0.0, 5.0));
    }

    #[test]
    fn richtung_ist_die_umkehrung() {
        for (hz, v, d) in [(0.0, 100.0, 10.0), (100.0, 95.0, 12.345), (250.5, 110.0, 3.0), (399.9, 60.0, 50.0)] {
            let (x, y, z) = lokale_koordinaten(hz, v, d);
            let (d2, hz2, v2) = richtung_aus_koordinaten(x, y, z);
            assert!((d2 - d).abs() < 0.001, "d {d} {d2}");
            assert!((hz2 - hz).abs() < 0.0005, "hz {hz} {hz2}");
            assert!((v2 - v).abs() < 0.0005, "v {v} {v2}");
        }
        assert_eq!(richtung_aus_koordinaten(0.0, 0.0, 0.0), (0.0, 0.0, V_WAAGERECHT));
        // Hz wird in 0 … 400 gehalten (Westen = 300 gon)
        assert!((richtung_aus_koordinaten(-5.0, 0.0, 0.0).1 - 300.0).abs() < 1e-9);
    }

    #[test]
    fn punkte_aus_messung_und_leer() {
        let p = AufnahmePunkt::aus_messung(3, "P7", "KS", 100.00004, 100.0, 10.1234);
        assert_eq!((p.nr, p.punkt_nr.as_str(), p.code.as_str()), (3, "P7", "KS"));
        assert_eq!((p.d, p.hz, p.v), (10.123, 100.0, 100.0));
        assert!((p.x - 10.1234).abs() < 1e-9);
        let l = AufnahmePunkt::leer(5, 100.0);
        assert_eq!((l.punkt_nr.as_str(), l.v, l.x), ("P005", 100.0, 0.0));
        let mut e = AufnahmePunkt { x: 0.0, y: 10.0, z: 0.0, ..AufnahmePunkt::leer(1, 0.0) };
        e.berechne_richtung();
        assert_eq!((e.d, e.hz, e.v), (10.0, 0.0, 100.0));
    }

    #[test]
    fn nummerieren_und_pfade() {
        let mut p = vec![AufnahmePunkt::leer(9, 100.0), AufnahmePunkt::leer(9, 100.0)];
        nummeriere(&mut p);
        assert_eq!((p[0].nr, p[1].nr), (1, 2));
        assert_eq!(pfad(Some(Path::new("C:/Projekt")), Path::new("C:/Basis")), Path::new("C:/Projekt").join("aufnahmetest.json"));
        assert_eq!(pfad(None, Path::new("C:/Basis")), Path::new("C:/Basis").join("aufnahmetest.json"));
    }

    #[test]
    fn json_hat_das_format_der_csharp_version() {
        let dir = std::env::temp_dir().join(format!("fb-aufn-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let datei = dir.join("aufnahmetest.json");
        assert!(lade(&datei).is_empty());
        let p = vec![AufnahmePunkt::aus_messung(1, "P001", "A", 100.0, 100.0, 10.0)];
        speichere(&datei, &p).unwrap();
        let text = std::fs::read_to_string(&datei).unwrap();
        for schluessel in ["\"nr\"", "\"punktNr\"", "\"code\"", "\"x\"", "\"y\"", "\"z\"", "\"d\"", "\"hz\"", "\"v\""] {
            assert!(text.contains(schluessel), "{schluessel} fehlt: {text}");
        }
        assert_eq!(lade(&datei), p);
        // Datei der C#-Version (BOM, fehlende Felder) bleibt lesbar; defekte Datei ergibt eine leere Liste
        std::fs::write(&datei, "\u{feff}[{\"nr\":2,\"punktNr\":\"X\",\"x\":1.5}]").unwrap();
        let l = lade(&datei);
        assert_eq!((l[0].nr, l[0].punkt_nr.as_str(), l[0].x, l[0].hz), (2, "X", 1.5, 0.0));
        std::fs::write(&datei, "kaputt").unwrap();
        assert!(lade(&datei).is_empty());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn exportzeilen_im_gemeinsamen_format() {
        let k = ExportKopf { projekt: "Test", sensor: "Leica TS06", bearbeiter: "jg", datum: "2026-11-04", zeitstempel: "2026-11-04T19:30:00+01:00" };
        let p = vec![AufnahmePunkt::aus_messung(1, "P001", "KS;A", 100.0, 100.0, 10.0)];
        let kor = crate::daten::csv_datei::CsvDatei::parse(&kor_zeilen(&p, &k).join("\n")).unwrap();
        assert_eq!(kor.zelle(&kor.zeilen[0], "X"), "10.0000");
        assert_eq!(kor.zelle(&kor.zeilen[0], "Code"), "KS;A");
        assert_eq!(kor.meta("Koordinatensystem"), Some("Lokal"));
        let dat = crate::daten::csv_datei::CsvDatei::parse(&dat_zeilen(&p, &k).join("\n")).unwrap();
        assert_eq!(dat.spalten.len(), 13);
        assert_eq!(dat.zelle(&dat.zeilen[0], "StandpunktNr"), "Lokal");
        assert_eq!(dat.zelle(&dat.zeilen[0], "Typ"), "NP");
        assert_eq!(dat.zelle(&dat.zeilen[0], "HZ_gon"), "100.0000");
    }

    #[test]
    fn exportpfade_ersetzen_kor_endung() {
        let (kor, dat) = export_pfade(Path::new("C:/x/aufnahme-kor.csv"));
        assert_eq!(kor, Path::new("C:/x/aufnahme-kor.csv"));
        assert_eq!(dat, Path::new("C:/x").join("aufnahme-dat.csv"));
        assert_eq!(export_pfade(Path::new("C:/x/Test-KOR.csv")).1, Path::new("C:/x").join("Test-dat.csv"));
        assert_eq!(export_pfade(Path::new("C:/x/messung.csv")).1, Path::new("C:/x").join("messung-dat.csv"));
        assert_eq!(export_pfade(Path::new("C:/x/ä.csv")).1, Path::new("C:/x").join("ä-dat.csv"));
    }
}
