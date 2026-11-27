// Zweck:    Ergebnisdatei der GNSS-Punkte eines Projekts ({Projekt}-gnss.csv): Plankoordinaten, Rohwerte (UTM,
//           ellipsoidische Höhe), Streuung, Epochen, Antenne, Höhenbezug und Einpassung je Punkt
// Hardware: keine (Messwerte vom GNSS-Empfänger u-blox ZED-F9P)
// Autor:    Johann Gerner

use std::io;
use std::path::{Path, PathBuf};

use crate::daten::csv_basis;
use crate::daten::einstellungen::Einstellungen;
use crate::grundlagen::datei_speicher;
use crate::grundlagen::format::{f, parse_zahl};

/// Trennzeichen.
const TRENNER: char = ';';
/// Spalten der Datei.
pub const SPALTEN: [&str; 22] = [
    "PunktNr", "Code", "R", "H", "Hoehe", "E_UTM", "N_UTM", "h_ell", "H_Bezug", "sR_mm", "sH_mm", "sHoehe_mm", "Epochen", "Dauer_s",
    "Stabhoehe_m", "PCO_m", "Hoehenbezug", "EinpassungId", "Zeit", "Quelle", "Zeit_GNSS", "Belegungen",
];

/// Ein gemessener GNSS-Punkt.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct GnssPunkt {
    pub nr: String,
    pub code: String,
    /// Plankoordinaten (nach Einpassung bzw. UTM)
    pub r: f64,
    pub h: f64,
    pub hoehe: Option<f64>,
    /// Rohwerte: UTM32 ohne Zonenkennzahl, ellipsoidische Höhe und Höhe im GNSS-Höhenbezug (Bodenpunkt)
    pub e_utm: f64,
    pub n_utm: f64,
    pub h_ell: f64,
    pub h_bezug: Option<f64>,
    pub s_r_mm: f64,
    pub s_h_mm: f64,
    pub s_hoehe_mm: f64,
    pub epochen: usize,
    pub dauer_s: f64,
    pub stab_m: f64,
    pub pco_m: f64,
    pub hoehenbezug: String,
    /// leer: ohne Einpassung (UTM)
    pub einpassung_id: String,
    pub zeit: String,
    /// „GNSS RTK“, „WIEDERGABE“ oder „EXZENTRUM“
    pub quelle: String,
    /// UTC-Zeit des Empfängers am Ende der Messung (unabhängig von der PC-Uhr)
    pub zeit_gnss: String,
    /// Anzahl gemittelter Belegungen (0 bei berechneten Punkten)
    pub belegungen: usize,
}

impl GnssPunkt {
    fn felder(&self) -> Vec<String> {
        let opt = |w: Option<f64>, s: usize| w.map_or_else(String::new, |w| f(w, s));
        vec![
            self.nr.clone(),
            self.code.clone(),
            f(self.r, 4),
            f(self.h, 4),
            opt(self.hoehe, 4),
            f(self.e_utm, 4),
            f(self.n_utm, 4),
            f(self.h_ell, 4),
            opt(self.h_bezug, 4),
            f(self.s_r_mm, 1),
            f(self.s_h_mm, 1),
            f(self.s_hoehe_mm, 1),
            self.epochen.to_string(),
            f(self.dauer_s, 1),
            f(self.stab_m, 4),
            f(self.pco_m, 4),
            self.hoehenbezug.clone(),
            self.einpassung_id.clone(),
            self.zeit.clone(),
            self.quelle.clone(),
            self.zeit_gnss.clone(),
            self.belegungen.to_string(),
        ]
    }

    fn aus_felder(spalten: &[String], werte: &[String]) -> Option<Self> {
        let t = |name: &str| csv_basis::index(spalten, name).and_then(|i| werte.get(i)).map_or("", |s| s.as_str()).trim().to_string();
        let z = |name: &str| parse_zahl(&t(name));
        let nr = t("PunktNr");
        if nr.is_empty() {
            return None;
        }
        Some(Self {
            nr,
            code: t("Code"),
            r: z("R")?,
            h: z("H")?,
            hoehe: z("Hoehe"),
            e_utm: z("E_UTM").unwrap_or(0.0),
            n_utm: z("N_UTM").unwrap_or(0.0),
            h_ell: z("h_ell").unwrap_or(0.0),
            h_bezug: z("H_Bezug"),
            s_r_mm: z("sR_mm").unwrap_or(0.0),
            s_h_mm: z("sH_mm").unwrap_or(0.0),
            s_hoehe_mm: z("sHoehe_mm").unwrap_or(0.0),
            epochen: t("Epochen").parse().unwrap_or(0),
            dauer_s: z("Dauer_s").unwrap_or(0.0),
            stab_m: z("Stabhoehe_m").unwrap_or(0.0),
            pco_m: z("PCO_m").unwrap_or(0.0),
            hoehenbezug: t("Hoehenbezug"),
            einpassung_id: t("EinpassungId"),
            zeit: t("Zeit"),
            quelle: t("Quelle"),
            zeit_gnss: t("Zeit_GNSS"),
            belegungen: t("Belegungen").parse().unwrap_or(1),
        })
    }
}

/// Pfad der GNSS-Punktdatei des Projekts.
pub fn pfad(e: &Einstellungen) -> PathBuf {
    e.get_pfad(&format!("{}-gnss.csv", e.projekt_name))
}

/// Liest alle Punkte; eine fehlende Datei ergibt eine leere Liste.
pub fn lese(pfad: &Path) -> io::Result<Vec<GnssPunkt>> {
    let text = match std::fs::read_to_string(pfad) {
        Ok(t) => t,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(e),
    };
    let mut zeilen = text.lines().filter(|z| !z.trim().is_empty() && !z.starts_with('#'));
    let Some(kopf) = zeilen.next() else {
        return Ok(Vec::new());
    };
    let spalten = csv_basis::felder(kopf, TRENNER)?;
    let mut punkte = Vec::new();
    for z in zeilen {
        if let Some(p) = GnssPunkt::aus_felder(&spalten, &csv_basis::felder(z, TRENNER)?) {
            punkte.push(p);
        }
    }
    Ok(punkte)
}

/// Schreibt alle Punkte (ersetzt die Datei).
pub fn schreibe(pfad: &Path, punkte: &[GnssPunkt]) -> io::Result<()> {
    let mut text = format!("{}\r\n", csv_basis::zeile(SPALTEN, TRENNER));
    for p in punkte {
        text.push_str(&csv_basis::zeile(p.felder(), TRENNER));
        text.push_str("\r\n");
    }
    datei_speicher::schreibe_text(pfad, &text)
}

/// Ergänzt einen Punkt oder ersetzt einen mit gleicher Nummer; liefert `true`, wenn ersetzt wurde.
pub fn setze(punkte: &mut Vec<GnssPunkt>, p: GnssPunkt) -> bool {
    match punkte.iter_mut().find(|q| q.nr.eq_ignore_ascii_case(&p.nr)) {
        Some(alt) => {
            *alt = p;
            true
        }
        None => {
            punkte.push(p);
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rundlauf_und_ersetzen() {
        let ordner = std::env::temp_dir().join(format!("feldbuch_gnss_{}", std::process::id()));
        std::fs::create_dir_all(&ordner).unwrap();
        let pfad = ordner.join("p-gnss.csv");
        let a = GnssPunkt {
            nr: "100".into(),
            code: "GR; Stein".into(),
            r: 690_001.2345,
            h: 5_430_002.5,
            hoehe: Some(400.123),
            e_utm: 690_001.2,
            n_utm: 5_430_002.5,
            h_ell: 447.9,
            h_bezug: None,
            epochen: 30,
            hoehenbezug: "ellipsoidisch GRS80".into(),
            zeit: "2026-11-26T10:00:00".into(),
            quelle: "GNSS RTK".into(),
            ..Default::default()
        };
        let mut punkte = vec![a.clone()];
        assert!(!setze(&mut punkte, GnssPunkt { nr: "101".into(), ..a.clone() }));
        assert!(setze(&mut punkte, GnssPunkt { r: 1.0, ..a.clone() }));
        schreibe(&pfad, &punkte).unwrap();
        let gelesen = lese(&pfad).unwrap();
        assert_eq!(gelesen.len(), 2);
        assert_eq!(gelesen[0].code, "GR; Stein");
        assert_eq!(gelesen[0].r, 1.0);
        assert_eq!(gelesen[1].hoehe, Some(400.123));
        assert_eq!(gelesen[1].h_bezug, None);
        let _ = std::fs::remove_dir_all(&ordner);
        assert!(lese(&ordner.join("fehlt.csv")).unwrap().is_empty());
    }
}
