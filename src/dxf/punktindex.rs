// Zweck:    Punkt-Index der DXF-Koordinaten: dedupliziert Koordinaten (3 mm), vergibt fortlaufende Nummern,
//           bleibt über erneutes Laden erhalten ({Projekt}-PunktIndex.json)
// Hardware: keine
// Autor:    Johann Gerner

use std::collections::{HashMap, HashSet};
use std::f64::consts::PI;
use std::path::Path;

use serde::{Deserialize, Serialize};

use super::entity::{Art, DxfEntity};
use crate::grundlagen::datei_speicher;

/// Toleranz der Deduplizierung [m].
pub const TOLERANZ_M: f64 = 0.003;

/// Persistenter Eintrag des Index.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct PunktEintrag {
    #[serde(rename = "PunktNr")]
    pub punkt_nr: String,
    #[serde(rename = "R")]
    pub r: f64,
    #[serde(rename = "H")]
    pub h: f64,
}

/// Index: gerundete Koordinate → Punktnummer.
#[derive(Default, Clone)]
pub struct PunktIndex {
    index: HashMap<(i64, i64), String>,
    pub eintraege: Vec<PunktEintrag>,
}

fn schluessel(r: f64, h: f64) -> (i64, i64) {
    ((r / TOLERANZ_M).round() as i64, (h / TOLERANZ_M).round() as i64)
}

/// Alle relevanten Koordinaten einer Entity (Linien-Endpunkte, Mittelpunkte, Eckpunkte, Bogen-Endpunkte).
pub fn entity_punkte(e: &DxfEntity) -> Vec<(f64, f64)> {
    match &e.art {
        Art::Line { x1, y1, x2, y2 } => vec![(*x1, *y1), (*x2, *y2)],
        Art::Insert { x, y, .. } | Art::Point { x, y } => vec![(*x, *y)],
        Art::Circle { cx, cy, .. } => vec![(*cx, *cy)],
        Art::LwPolyline { punkte, .. } => punkte.clone(),
        Art::Arc { cx, cy, radius, start, ende } => {
            let (sr, er) = (start * PI / 180.0, ende * PI / 180.0);
            vec![
                (cx + radius * sr.cos(), cy + radius * sr.sin()),
                (cx + radius * er.cos(), cy + radius * er.sin()),
            ]
        }
        _ => Vec::new(),
    }
}

impl PunktIndex {
    /// Baut einen neuen Index (Nummerierung ab 1, sortiert nach R, dann H).
    pub fn aufbauen(entities: &[DxfEntity]) -> Self {
        Self::aufbauen_mit_merge(entities, None)
    }

    /// Baut den Index und übernimmt bestehende Nummern; neue Punkte erhalten fortlaufende Nummern,
    /// alte Einträge, die nicht mehr in der DXF vorkommen, bleiben als Historie erhalten.
    pub fn aufbauen_mit_merge(entities: &[DxfEntity], bestehend: Option<&[PunktEintrag]>) -> Self {
        let mut punkte: HashMap<(i64, i64), (f64, f64)> = HashMap::new();
        for e in entities {
            for (r, h) in entity_punkte(e) {
                punkte.entry(schluessel(r, h)).or_insert((r, h));
            }
        }
        let mut sortiert: Vec<((i64, i64), (f64, f64))> = punkte.into_iter().collect();
        sortiert.sort_by_key(|(k, _)| *k);

        let bestehende_nummern: HashMap<(i64, i64), &str> = bestehend
            .unwrap_or(&[])
            .iter()
            .map(|b| (schluessel(b.r, b.h), b.punkt_nr.as_str()))
            .collect();
        let mut max_nr = bestehend
            .unwrap_or(&[])
            .iter()
            .filter_map(|b| b.punkt_nr.parse::<i64>().ok())
            .max()
            .unwrap_or(0);

        let mut ergebnis = Self::default();
        let mut vergeben: HashSet<String> = HashSet::new();
        for (key, (r, h)) in sortiert {
            let nr = match bestehende_nummern.get(&key) {
                Some(nr) => (*nr).to_string(),
                None => {
                    max_nr += 1;
                    max_nr.to_string()
                }
            };
            vergeben.insert(nr.clone());
            ergebnis.index.insert(key, nr.clone());
            ergebnis.eintraege.push(PunktEintrag { punkt_nr: nr, r, h });
        }
        for b in bestehend.unwrap_or(&[]) {
            if !vergeben.contains(&b.punkt_nr) {
                ergebnis.eintraege.push(b.clone());
            }
        }
        ergebnis
    }

    /// Punktnummer zu einer Koordinate.
    pub fn get_punkt_nr(&self, r: f64, h: f64) -> Option<&str> {
        self.index.get(&schluessel(r, h)).map(String::as_str)
    }

    /// Anzahl der eindeutigen Punkte.
    pub fn anzahl(&self) -> usize {
        self.index.len()
    }
}

/// Lädt den gespeicherten Index. Eine defekte Datei wird beiseitegelegt, eine nicht lesbare gesperrt.
pub fn lade(pfad: &Path) -> Option<Vec<PunktEintrag>> {
    if !pfad.exists() {
        datei_speicher::laden_erfolgreich(pfad);
        return None;
    }
    let text = match std::fs::read_to_string(pfad) {
        Ok(t) => t,
        Err(e) => {
            datei_speicher::melde_ladefehler(pfad, &e);
            return None;
        }
    };
    match serde_json::from_str::<Vec<PunktEintrag>>(text.trim_start_matches('\u{feff}')) {
        Ok(daten) => {
            datei_speicher::laden_erfolgreich(pfad);
            Some(daten)
        }
        Err(fehler) => {
            if let Err(e) = datei_speicher::lege_defekte_datei_beiseite(
                pfad,
                &fehler,
                "Punktnummern können sich dadurch ändern. Bitte prüfen.",
            ) {
                datei_speicher::melde_ladefehler(pfad, &e);
            }
            None
        }
    }
}

/// Speichert den Index.
pub fn speichere(eintraege: &[PunktEintrag], pfad: &Path) -> std::io::Result<()> {
    let json = serde_json::to_string_pretty(eintraege).map_err(std::io::Error::other)?;
    datei_speicher::schreibe_text(pfad, &json)
}

/// Löscht die Indexdatei.
pub fn loesche(pfad: &Path) {
    let _ = std::fs::remove_file(pfad);
    datei_speicher::laden_erfolgreich(pfad);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn linie(x1: f64, y1: f64, x2: f64, y2: f64) -> DxfEntity {
        DxfEntity { layer: "0".into(), farbe: None, art: Art::Line { x1, y1, x2, y2 } }
    }

    #[test]
    fn dedupliziert_und_nummeriert_nach_r_dann_h() {
        let e = vec![linie(10.0, 5.0, 20.0, 5.0), linie(20.0001, 5.0, 30.0, 5.0)];
        let idx = PunktIndex::aufbauen(&e);
        assert_eq!(idx.anzahl(), 3);
        assert_eq!(idx.get_punkt_nr(10.0, 5.0), Some("1"));
        assert_eq!(idx.get_punkt_nr(20.0, 5.0), Some("2"));
        assert_eq!(idx.get_punkt_nr(20.0001, 5.0), Some("2"));
        assert_eq!(idx.get_punkt_nr(30.0, 5.0), Some("3"));
        assert_eq!(idx.get_punkt_nr(99.0, 5.0), None);
    }

    #[test]
    fn merge_behaelt_nummern_und_vergibt_neue_fortlaufend() {
        let alt = vec![
            PunktEintrag { punkt_nr: "7".into(), r: 20.0, h: 5.0 },
            PunktEintrag { punkt_nr: "9".into(), r: 500.0, h: 5.0 },
        ];
        let e = vec![linie(10.0, 5.0, 20.0, 5.0)];
        let idx = PunktIndex::aufbauen_mit_merge(&e, Some(&alt));
        assert_eq!(idx.get_punkt_nr(20.0, 5.0), Some("7"));
        assert_eq!(idx.get_punkt_nr(10.0, 5.0), Some("10"));
        // Historie bleibt erhalten
        assert!(idx.eintraege.iter().any(|x| x.punkt_nr == "9"));
    }
}
