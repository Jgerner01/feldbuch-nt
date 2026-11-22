// Zweck:    Liest die Messdaten-Datei ({Projekt}-dat.csv) für das Polarmodul – alte Dateien mit 7 Spalten und neue
//           mit 11 Spalten (Exzentrizitäten, Standpunkt, Typ). Die Datei wird dabei nicht verändert.
// Hardware: keine
// Autor:    Johann Gerner

use std::fs;
use std::io;
use std::path::Path;

use crate::daten::messdaten::{TYP_ANSCHLUSSPUNKT, TYP_KONTROLLPUNKT, TYP_NEUPUNKT};
use crate::grundlagen::format::parse_zahl;

/// Spaltenindizes der Messdatei.

const KOMMENTAR_STATIONIERUNG: &str = "# STATIONIERUNG:";
const KOMMENTAR_SP_SCHLUESSEL: &str = "SP=";

/// Eine Messzeile der dat.csv.
#[derive(Clone, Debug, PartialEq)]
pub struct PolarMessung {
    pub exz_unbekannt: bool,
    pub aufstellung_id: String,
    pub instrumenten_hoehe: Option<f64>,
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
    pub fn gruppe(&self) -> String {
        if self.aufstellung_id.is_empty() { self.standpunkt_nr.clone() } else { format!("{} [{}]", self.standpunkt_nr, self.aufstellung_id) }
    }
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
    /// Auffälligkeiten beim Lesen, die das Einlesen nicht verhindern.
    pub hinweise: Vec<String>,
}

impl PolarMessdaten {
    /// Standpunkte in der Reihenfolge ihres ersten Auftretens.
    pub fn standpunkte(&self) -> Vec<String> {
        let mut liste: Vec<String> = Vec::new();
        for m in &self.messungen {
            let gruppe = m.gruppe();
            if !m.standpunkt_nr.is_empty() && !liste.contains(&gruppe) {
                liste.push(gruppe);
            }
        }
        liste
    }
}

/// Liest Messwerte nach Namen; ungültige Werte melden statt still überspringen.
pub fn lese(pfad: &Path, ist_koordinatenpunkt: impl Fn(&str) -> bool) -> io::Result<PolarMessdaten> {
    let text = fs::read_to_string(pfad)?;
    parse(&text.lines().map(str::to_string).collect::<Vec<_>>(), ist_koordinatenpunkt)
}

pub fn parse(zeilen: &[String], ist_koordinatenpunkt: impl Fn(&str) -> bool) -> io::Result<PolarMessdaten> {
    let d = crate::daten::csv_datei::CsvDatei::parse(&zeilen.join("\n"))?;
    let mut daten = PolarMessdaten { kopf_standpunkt_nr: d.standpunkt_nr.clone(), kopf_instr_hoehe: parse_zahl(&d.instr_hoehe).unwrap_or(0.0), messungen: Vec::new(), hinweise: Vec::new() };
    let mut kommentare = Vec::new();
    for (i, zeile) in zeilen.iter().enumerate() {
        if zeile.trim().to_uppercase().starts_with(KOMMENTAR_STATIONIERUNG) {
            let sp = lese_sp_aus_kommentar(zeile);
            if !sp.is_empty() {
                kommentare.push((i, sp));
            }
        }
    }
    for (i, z) in d.zeilen.iter().enumerate() {
        let nr = d.zelle(z, "PunktNr");
        if nr.is_empty() {
            return Err(crate::daten::csv_basis::fehler(format!("Datensatz {}: PunktNr fehlt.", i + 1)));
        }
        let typ = d.zelle(z, "Typ").to_uppercase();
        // Kontrollmessungen (Orientierungskontrolle) gehören weder zur Stationierung noch zu den Polarpunkten
        if typ == TYP_KONTROLLPUNKT {
            continue;
        }
        let geschaetzt = typ.is_empty();
        if !typ.is_empty() && typ != TYP_ANSCHLUSSPUNKT && typ != TYP_NEUPUNKT {
            return Err(crate::daten::csv_basis::fehler(format!("Datensatz {}: unbekannter Messtyp {typ}.", i + 1)));
        }
        let exz = |name: &str| if d.zelle(z, name).is_empty() { Ok(0.0) } else { d.zahl(i, name) };
        let ih = if d.zelle(z, "InstrumentenHoehe_m").is_empty() { parse_zahl(&d.instr_hoehe) } else { Some(d.zahl(i, "InstrumentenHoehe_m")?) };
        if ih.is_some_and(|h| h < 0.0) {
            return Err(crate::daten::csv_basis::fehler(format!("Datensatz {}: negative Instrumentenhöhe.", i + 1)));
        }
        let strecke = d.zahl(i, "Strecke_m")?;
        if strecke < 0.0 {
            return Err(crate::daten::csv_basis::fehler(format!("Datensatz {}: negative Schrägstrecke.", i + 1)));
        }
        daten.messungen.push(PolarMessung {
            zeilen_index: d.zeilennummern[i] - 1,
            exz_unbekannt: d.zelle(z, "ExzLaengs_m").is_empty() || d.zelle(z, "ExzQuer_m").is_empty(),
            punkt_nr: nr.into(),
            hz_gon: d.zahl(i, "HZ_gon")?,
            v_gon: d.zahl(i, "V_gon")?,
            strecke_m: strecke,
            zielhoehe_m: d.zahl(i, "Zielhoehe_m")?,
            code: d.zelle(z, "Code").into(),
            zeitstempel: d.zelle(z, "Zeitstempel").into(),
            exz_laengs_m: exz("ExzLaengs_m")?,
            exz_quer_m: exz("ExzQuer_m")?,
            standpunkt_nr: d.zelle(z, "StandpunktNr").into(),
            aufstellung_id: if d.zelle(z, "AufstellungId").is_empty() { d.meta("AufstellungId").unwrap_or("").into() } else { d.zelle(z, "AufstellungId").into() },
            instrumenten_hoehe: ih,
            typ: if geschaetzt { if ist_koordinatenpunkt(nr) { TYP_ANSCHLUSSPUNKT } else { TYP_NEUPUNKT }.into() } else { typ },
            typ_geschaetzt: geschaetzt,
        });
    }
    weise_standpunkte_zu(&mut daten, &kommentare);
    // Eine Aufstellung hat genau eine Instrumentenhöhe (sonst Fehler). Unterschiedliche Standpunktnummern
    // entstehen beim Umbenennen des Standpunkts; sie werden nur gemeldet und bilden eigene Gruppen.
    let mut aufstellungen: std::collections::HashMap<String, (Vec<String>, Option<f64>)> = std::collections::HashMap::new();
    for m in &daten.messungen {
        if m.aufstellung_id.is_empty() {
            continue;
        }
        let eintrag = aufstellungen.entry(m.aufstellung_id.clone()).or_insert_with(|| (Vec::new(), m.instrumenten_hoehe));
        if eintrag.1 != m.instrumenten_hoehe {
            return Err(crate::daten::csv_basis::fehler(format!("Aufstellung {} enthält widersprüchliche Instrumentenhöhen.", m.aufstellung_id)));
        }
        if !eintrag.0.contains(&m.standpunkt_nr) {
            eintrag.0.push(m.standpunkt_nr.clone());
        }
    }
    let mut mehrdeutig: Vec<_> = aufstellungen.into_iter().filter(|(_, (nrn, _))| nrn.len() > 1).collect();
    mehrdeutig.sort_by(|a, b| a.0.cmp(&b.0));
    for (id, (nrn, _)) in mehrdeutig {
        daten.hinweise.push(format!("Aufstellung {id} enthält mehrere Standpunktnummern ({}). Zuordnung prüfen.", nrn.join(", ")));
    }
    Ok(daten)
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
        m.standpunkt_nr = if m.typ == TYP_ANSCHLUSSPUNKT { danach.or(davor).unwrap_or(&kopf).to_string() } else { davor.unwrap_or(&kopf).to_string() };
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
        let d = parse(&zeilen(NEU), |_| false).unwrap();
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
        let d = parse(&zeilen(text), |nr| nr.starts_with('A')).unwrap();
        let sp: Vec<(&str, &str, bool)> = d.messungen.iter().map(|m| (m.punkt_nr.as_str(), m.standpunkt_nr.as_str(), m.typ_geschaetzt)).collect();
        // Neupunkt N1 gehört zum vorherigen Kommentar (ALT), die Anschlusspunkte zum nächsten (NEU)
        assert_eq!(sp[0], ("N1", "ALT", true));
        assert_eq!(sp[1], ("A1", "NEU", true));
        assert_eq!(sp[2], ("A2", "NEU", true));
        assert_eq!(sp[3], ("N2", "NEU", true));
        assert!(d.messungen[1].ist_anschlusspunkt() && !d.messungen[0].ist_anschlusspunkt());
    }

    #[test]
    fn umbenannter_standpunkt_in_einer_aufstellung_ist_nur_ein_hinweis() {
        let text = "# DATEN\nPunktNr;HZ_gon;V_gon;Strecke_m;Zielhoehe_m;Code;Zeitstempel;ExzLaengs_m;ExzQuer_m;StandpunktNr;Typ;AufstellungId;InstrumentenHoehe_m\n\
11;1;100;0;0;;t;0;0;STP1;NP;ID;0.000\n\
12;2;100;10;0;;t;0;0;STP2;NP;ID;0.000\n";
        let d = parse(&zeilen(text), |_| false).unwrap();
        assert_eq!(d.messungen.len(), 2);
        assert_eq!(d.standpunkte().len(), 2);
        assert_eq!(d.hinweise.len(), 1);
        assert!(d.hinweise[0].contains("STP1, STP2"));
    }

    #[test]
    fn kontrollmessungen_werden_uebersprungen() {
        let text = "# DATEN
PunktNr;HZ_gon;V_gon;Strecke_m;Zielhoehe_m;Code;Zeitstempel;ExzLaengs_m;ExzQuer_m;StandpunktNr;Typ
\nA1;1;100;10;0;;t;0;0;S1;KP
\n7;2;100;10;0;;t;0;0;S1;NP
";
        let d = parse(&zeilen(text), |_| false).unwrap();
        assert_eq!(d.messungen.len(), 1);
        assert_eq!(d.messungen[0].punkt_nr, "7");
    }

    #[test]
    fn meldet_ungueltige_zeilen() {
        let text = "# DATEN\nkopf\nA; x; 1; 2; 3\nB; 1; 2\nC; 1; 2; 3; 4\n";
        assert!(parse(&zeilen(text), |_| false).is_err());
    }
}
