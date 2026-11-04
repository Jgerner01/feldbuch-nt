// Zweck:    Liest die Messdaten-Datei ({Projekt}-dat.csv) für das Polarmodul – alte Dateien mit 7 Spalten und neue
//           mit 11 Spalten (Exzentrizitäten, Standpunkt, Typ). Die Datei wird dabei nicht verändert.
// Hardware: keine
// Autor:    Johann Gerner

use std::fs;
use std::io;
use std::path::Path;

use crate::daten::messdaten::{TYP_ANSCHLUSSPUNKT, TYP_NEUPUNKT};
use crate::grundlagen::format::parse_zahl;

/// Spaltenindizes der Messdatei.
const SPALTE_PUNKTNR: usize = 0;
const SPALTE_HZ: usize = 1;
const SPALTE_V: usize = 2;
const SPALTE_STRECKE: usize = 3;
const SPALTE_ZIELH: usize = 4;
const SPALTE_CODE: usize = 5;
const SPALTE_ZEIT: usize = 6;
const SPALTE_EXZ_L: usize = 7;
const SPALTE_EXZ_Q: usize = 8;
const SPALTE_SP: usize = 9;
const SPALTE_TYP: usize = 10;

const KOMMENTAR_STATIONIERUNG: &str = "# STATIONIERUNG:";
const KOMMENTAR_SP_SCHLUESSEL: &str = "SP=";

/// Eine Messzeile der dat.csv.
#[derive(Clone, Debug, PartialEq)]
pub struct PolarMessung {
    /// 0-basierter Zeilenindex in der Datei (für das Rückschreiben).
    pub zeilen_index: usize,
    pub punkt_nr: String,
    pub hz_gon: f64,
    pub v_gon: f64,
    pub strecke_m: f64,
    pub zielhoehe_m: f64,
    pub code: String,
    pub zeitstempel: String,
    pub exz_laengs_m: f64,
    pub exz_quer_m: f64,
    pub standpunkt_nr: String,
    /// "AP" (Anschlusspunkt) oder "NP" (Neupunkt).
    pub typ: String,
    /// Der Typ stand nicht in der Datei, sondern wurde aus den Koordinaten abgeleitet.
    pub typ_geschaetzt: bool,
}

impl PolarMessung {
    pub fn ist_anschlusspunkt(&self) -> bool {
        self.typ == TYP_ANSCHLUSSPUNKT
    }
}

/// Inhalt einer dat.csv.
#[derive(Clone, Debug, Default)]
pub struct PolarMessdaten {
    pub kopf_standpunkt_nr: String,
    pub kopf_instr_hoehe: f64,
    pub messungen: Vec<PolarMessung>,
}

impl PolarMessdaten {
    /// Standpunkte in der Reihenfolge ihres ersten Auftretens.
    pub fn standpunkte(&self) -> Vec<String> {
        let mut liste: Vec<String> = Vec::new();
        for m in &self.messungen {
            if !m.standpunkt_nr.is_empty() && !liste.contains(&m.standpunkt_nr) {
                liste.push(m.standpunkt_nr.clone());
            }
        }
        liste
    }
}

/// Liest eine Datei; `ist_koordinatenpunkt` leitet den Typ für alte Dateien ab.
pub fn lese(pfad: &Path, ist_koordinatenpunkt: impl Fn(&str) -> bool) -> io::Result<PolarMessdaten> {
    let text = fs::read_to_string(pfad)?;
    let zeilen: Vec<String> = text.lines().map(str::to_string).collect();
    Ok(parse(&zeilen, ist_koordinatenpunkt))
}

/// Parst die Zeilen einer dat.csv (ohne Dateizugriff testbar).
pub fn parse(zeilen: &[String], ist_koordinatenpunkt: impl Fn(&str) -> bool) -> PolarMessdaten {
    let mut daten = PolarMessdaten::default();
    let mut sektion = String::new();
    let mut daten_kopf = false;
    let mut kommentare: Vec<(usize, String)> = Vec::new();

    for (i, roh) in zeilen.iter().enumerate() {
        let zeile = roh.trim();
        if zeile.is_empty() || zeile == "---" {
            continue;
        }
        if zeile.to_uppercase().starts_with(&KOMMENTAR_STATIONIERUNG.to_uppercase()) {
            let sp = lese_sp_aus_kommentar(zeile);
            if !sp.is_empty() {
                kommentare.push((i, sp));
            }
            continue;
        }
        if zeile.starts_with('#') {
            sektion = zeile.trim_start_matches('#').trim().to_uppercase();
            daten_kopf = false;
            continue;
        }
        if sektion == "METADATA" {
            let Some(dp) = zeile.find(':') else {
                continue;
            };
            let schluessel = zeile[..dp].trim().to_lowercase();
            let wert = zeile[dp + 1..].trim();
            if schluessel == "standpunktnr" {
                daten.kopf_standpunkt_nr = wert.to_string();
            } else if schluessel == "instrumentenhoehe" {
                if let Some(ih) = parse_zahl(wert) {
                    daten.kopf_instr_hoehe = ih;
                }
            }
            continue;
        }
        if sektion != "DATEN" {
            continue;
        }
        if !daten_kopf {
            daten_kopf = true;
            continue;
        }
        if let Some(m) = lese_zeile(i, zeile, &ist_koordinatenpunkt) {
            daten.messungen.push(m);
        }
    }
    weise_standpunkte_zu(&mut daten, &kommentare);
    daten
}

fn feld<'a>(t: &'a [String], index: usize) -> &'a str {
    t.get(index).map_or("", String::as_str)
}

fn lese_zeile(index: usize, zeile: &str, ist_koordinatenpunkt: &impl Fn(&str) -> bool) -> Option<PolarMessung> {
    let t: Vec<String> = zeile.split(';').map(|s| s.trim().to_string()).collect();
    if t.len() <= SPALTE_ZIELH {
        return None;
    }
    let (hz, v, s) = (parse_zahl(&t[SPALTE_HZ])?, parse_zahl(&t[SPALTE_V])?, parse_zahl(&t[SPALTE_STRECKE])?);
    let mut m = PolarMessung {
        zeilen_index: index,
        punkt_nr: t[SPALTE_PUNKTNR].clone(),
        hz_gon: hz,
        v_gon: v,
        strecke_m: s,
        zielhoehe_m: parse_zahl(&t[SPALTE_ZIELH]).unwrap_or(0.0),
        code: feld(&t, SPALTE_CODE).to_string(),
        zeitstempel: feld(&t, SPALTE_ZEIT).to_string(),
        exz_laengs_m: parse_zahl(feld(&t, SPALTE_EXZ_L)).unwrap_or(0.0),
        exz_quer_m: parse_zahl(feld(&t, SPALTE_EXZ_Q)).unwrap_or(0.0),
        standpunkt_nr: feld(&t, SPALTE_SP).to_string(),
        typ: TYP_NEUPUNKT.to_string(),
        typ_geschaetzt: false,
    };
    let typ = feld(&t, SPALTE_TYP).to_uppercase();
    if typ == TYP_ANSCHLUSSPUNKT || typ == TYP_NEUPUNKT {
        m.typ = typ;
    } else {
        m.typ_geschaetzt = true;
        m.typ = if ist_koordinatenpunkt(&m.punkt_nr) { TYP_ANSCHLUSSPUNKT } else { TYP_NEUPUNKT }.to_string();
    }
    Some(m)
}

/// Alte Dateien ohne Standpunktspalte: Anschlusspunkte gehören zum nächsten „# STATIONIERUNG"-Kommentar,
/// Neupunkte zum vorherigen.
fn weise_standpunkte_zu(daten: &mut PolarMessdaten, kommentare: &[(usize, String)]) {
    let kopf = daten.kopf_standpunkt_nr.clone();
    for m in daten.messungen.iter_mut().filter(|m| m.standpunkt_nr.is_empty()) {
        let mut davor: Option<&str> = None;
        let mut danach: Option<&str> = None;
        for (index, sp) in kommentare {
            if *index < m.zeilen_index {
                davor = Some(sp);
            } else if danach.is_none() {
                danach = Some(sp);
            }
        }
        m.standpunkt_nr = if m.typ == TYP_ANSCHLUSSPUNKT {
            danach.or(davor).unwrap_or(&kopf).to_string()
        } else {
            davor.unwrap_or(&kopf).to_string()
        };
    }
}

fn lese_sp_aus_kommentar(zeile: &str) -> String {
    let Some(i) = zeile.to_uppercase().find(KOMMENTAR_SP_SCHLUESSEL) else {
        return String::new();
    };
    let rest = &zeile[i + KOMMENTAR_SP_SCHLUESSEL.len()..];
    let ende = rest.find([' ', '\t']).unwrap_or(rest.len());
    rest[..ende].trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn zeilen(text: &str) -> Vec<String> {
        text.lines().map(str::to_string).collect()
    }

    const NEU: &str = "# METADATA\nProjekt: Demo\nStandpunktNr: SP1\nInstrumentenHoehe: 1.556\n---\n# DATENTYP\nMessdaten\n# DATEN\n\
PunktNr; HZ_gon; V_gon; Strecke_m; Zielhoehe_m; Code; Zeitstempel; ExzLaengs_m; ExzQuer_m; StandpunktNr; Typ\n\
A1; 10.0000; 100.0000; 50.000; 1.500; ; 2026-01-01 10:00; 0.000; 0.000; SP1; AP\n\
A2; 110,5; 99.0; 60.000; 1.500; ; 2026-01-01 10:01; 0.000; 0.000; SP1; AP\n\
7; 200.0000; 95.0000; 20.000; 0.000; Baum; 2026-01-01 10:05; 0.500; -0.250; SP1; NP\n\
# STATIONIERUNG: SP=SP1  IH=1.556 m\n";

    #[test]
    fn liest_neues_format() {
        let d = parse(&zeilen(NEU), |_| false);
        assert_eq!(d.kopf_standpunkt_nr, "SP1");
        assert_eq!(d.kopf_instr_hoehe, 1.556);
        assert_eq!(d.messungen.len(), 3);
        assert!(d.messungen[0].ist_anschlusspunkt() && !d.messungen[0].typ_geschaetzt);
        assert_eq!(d.messungen[1].hz_gon, 110.5);
        let np = &d.messungen[2];
        assert_eq!((np.punkt_nr.as_str(), np.exz_laengs_m, np.exz_quer_m, np.code.as_str()), ("7", 0.5, -0.25, "Baum"));
        assert_eq!(d.standpunkte(), vec!["SP1".to_string()]);
    }

    #[test]
    fn altes_format_leitet_typ_und_standpunkt_ab() {
        let text = "# METADATA\nStandpunktNr: KOPF\n---\n# DATEN\nPunktNr; HZ_gon; V_gon; Strecke_m; Zielhoehe_m; Code; Zeitstempel\n\
# STATIONIERUNG: SP=ALT  IH=1.5 m\n\
N1; 1; 100; 10; 0; ; t\n\
A1; 2; 100; 10; 0; ; t\n\
A2; 3; 100; 10; 0; ; t\n\
# STATIONIERUNG: SP=NEU  IH=1.5 m\n\
N2; 4; 100; 10; 0; ; t\n";
        let d = parse(&zeilen(text), |nr| nr.starts_with('A'));
        let sp: Vec<(&str, &str, bool)> = d.messungen.iter().map(|m| (m.punkt_nr.as_str(), m.standpunkt_nr.as_str(), m.typ_geschaetzt)).collect();
        // Neupunkt N1 gehört zum vorherigen Kommentar (ALT), die Anschlusspunkte zum nächsten (NEU)
        assert_eq!(sp[0], ("N1", "ALT", true));
        assert_eq!(sp[1], ("A1", "NEU", true));
        assert_eq!(sp[2], ("A2", "NEU", true));
        assert_eq!(sp[3], ("N2", "NEU", true));
        assert!(d.messungen[1].ist_anschlusspunkt() && !d.messungen[0].ist_anschlusspunkt());
    }

    #[test]
    fn ueberspringt_ungueltige_zeilen() {
        let text = "# DATEN\nkopf\nA; x; 1; 2; 3\nB; 1; 2\nC; 1; 2; 3; 4\n";
        let d = parse(&zeilen(text), |_| false);
        assert_eq!(d.messungen.len(), 1);
        assert_eq!(d.messungen[0].punkt_nr, "C");
    }
}
