//! Standpunktdatensätze auf Basis der gemeinsamen CSV-Syntax.
use crate::daten::csv_datei::{CsvDatei, TYP_STANDPUNKT};
use crate::daten::modelle::StationierungsPunkt;
use crate::grundlagen::format::{f, parse_zahl};

#[derive(Debug)]
pub struct StationierungsImport {
    pub punkte: Vec<StationierungsPunkt>,
    pub standpunkt: Option<String>,
    pub instrumentenhoehe: Option<f64>,
    /// Vollständiges Dokument: auch NP, Codes und unbekannte Zusatzfelder erhalten.
    pub dokument: CsvDatei,
}

pub fn parse(text: &str) -> Result<StationierungsImport, String> {
    let d = CsvDatei::parse(text).map_err(|e| e.to_string())?;
    let ih = if d.instr_hoehe.is_empty() { None } else { Some(parse_zahl(&d.instr_hoehe).filter(|v| *v >= 0.0).ok_or("Ungültige Instrumentenhöhe.")?) };
    let mut punkte = Vec::new();
    for (i, z) in d.zeilen.iter().enumerate() {
        let typ = d.zelle(z, "Typ").to_uppercase();
        if typ == "NP" {
            continue;
        }
        if !typ.is_empty() && typ != "AP" {
            return Err(format!("Datensatz {}: unbekannter Typ {typ}.", i + 1));
        }
        let nr = d.zelle(z, "PunktNr");
        if nr.is_empty() {
            return Err(format!("Datensatz {}: PunktNr fehlt.", i + 1));
        }
        let zahl = |n| d.zahl(i, n).map_err(|e| e.to_string());
        let p = StationierungsPunkt {
            punkt_nr: nr.into(),
            r: zahl("X")?,
            h: zahl("Y")?,
            hoehe: if d.zelle(z, "Z").is_empty() { f64::NAN } else { zahl("Z")? },
            hz: zahl("HZ_gon")?,
            v: zahl("V_gon")?,
            strecke: zahl("Strecke_m")?,
            zielhoehe: zahl("Zielhoehe_m")?,
        };
        if p.strecke < 0.0 {
            return Err(format!("Datensatz {}: negative Schrägstrecke.", i + 1));
        }
        if punkte.iter().any(|a: &StationierungsPunkt| {
            a.punkt_nr == p.punkt_nr && ((a.r - p.r).abs() > 1e-8 || (a.h - p.h).abs() > 1e-8 || (a.hoehe.is_finite() && p.hoehe.is_finite() && (a.hoehe - p.hoehe).abs() > 1e-8))
        }) {
            return Err(format!("Widersprüchliche Anschlusskoordinaten für {}.", p.punkt_nr));
        }
        punkte.push(p);
    }
    if punkte.is_empty() {
        return Err("Die CSV-Datei enthält keine Anschlusspunktmessungen.".into());
    }
    Ok(StationierungsImport { punkte, standpunkt: (!d.standpunkt_nr.is_empty()).then(|| d.standpunkt_nr.clone()), instrumentenhoehe: ih, dokument: d })
}

pub fn zeile(p: &StationierungsPunkt) -> Vec<String> {
    vec![
        p.punkt_nr.clone(),
        f(p.r, 4),
        f(p.h, 4),
        if p.hoehe.is_finite() { f(p.hoehe, 4) } else { String::new() },
        f(p.hz, 4),
        f(p.v, 4),
        f(p.strecke, 3),
        f(p.zielhoehe, 3),
        String::new(),
        String::new(),
        "0.000".into(),
        "0.000".into(),
        "AP".into(),
    ]
}

pub fn datei(standpunkt: &str, ih: f64, punkte: &[StationierungsPunkt]) -> CsvDatei {
    let mut d = CsvDatei::neu(TYP_STANDPUNKT);
    d.standpunkt_nr = standpunkt.into();
    d.instr_hoehe = f(ih, 3);
    d.zeilen = punkte.iter().map(zeile).collect();
    d
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn viewer_format_mit_mehrfachmessungen() {
        let d = parse("PunktNr,R,H,Hoehe,HZ,V,Strecke,Zielhoehe\n7,100,200,300,45,100,20,1.7\n7,100,200,300,245,300,20,1.7").unwrap();
        assert_eq!(d.punkte.len(), 2);
        assert_eq!(d.punkte[1].hz, 245.0);
        assert_eq!(d.punkte[0].zielhoehe, 1.7);
        assert!(d.standpunkt.is_none());
    }
    #[test]
    fn semikolon_dezimalkomma_metadaten_und_spaltenreihenfolge() {
        let d = parse(
            "\u{feff}# METADATA\nStandpunktNr: SP7\nInstrumentenhoehe: 1,5\n# DATEN\nHZ_gon;Punkt-Nr;H [m];R [m];Höhe;V;Strecke_m;ZH\n45;\"P;7\";200,25;100,5;300;100;20,5;1,7",
        )
        .unwrap();
        assert_eq!(d.standpunkt.as_deref(), Some("SP7"));
        assert_eq!(d.instrumentenhoehe, Some(1.5));
        assert_eq!(d.punkte[0].punkt_nr, "P;7");
        assert_eq!(d.punkte[0].r, 100.5);
        assert_eq!(d.punkte[0].h, 200.25);
    }
    #[test]
    fn excel_dateien_mit_angehaengten_leeren_spalten() {
        let d = parse(
            "# METADATA,,,,,,,
StandpunktNr: 100,,,,,,,
Instrumentenhoehe: 1.500,,,,,,,
# DATEN,,,,,,,
PunktNr,R,H,Hoehe,HZ,V,Strecke,Zielhoehe
1641,647878.77,5473201.04,0,263.6864,90.8284,1.454,0
",
        )
        .unwrap();
        assert_eq!(d.standpunkt.as_deref(), Some("100"));
        assert_eq!(d.instrumentenhoehe, Some(1.5));
        assert_eq!(d.punkte.len(), 1);
        assert_eq!(d.punkte[0].punkt_nr, "1641");
    }
    #[test]
    fn ungueltige_und_reine_koordinatendateien_werden_abgewiesen() {
        assert!(parse("PunktNr;R;H;Hoehe\n1;100;200;300").unwrap_err().contains("HZ"));
        let e = parse("PunktNr,R,H,Hoehe,HZ,V,Strecke,Zielhoehe\n1,100,200,300,45,100,NaN,1.7").unwrap_err();
        assert!(e.contains("Zeile 2") && e.contains("Strecke"));
        assert!(parse("PunktNr,R,H,Hoehe,HZ,V,Strecke,Zielhoehe\n").is_err());
    }
}
