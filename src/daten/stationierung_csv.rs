// Liest Stationierungsdatensätze einschließlich Koordinaten und Messwerten.
use crate::daten::modelle::StationierungsPunkt;
use crate::grundlagen::format::parse_zahl;

#[derive(Debug)]
pub struct StationierungsImport {
    pub punkte: Vec<StationierungsPunkt>,
    pub standpunkt: Option<String>,
    pub instrumentenhoehe: Option<f64>,
}

fn name(text: &str) -> String {
    text.trim_start_matches('\u{feff}')
        .to_lowercase()
        .replace('ö', "oe")
        .chars()
        .filter(|c| c.is_alphanumeric())
        .collect()
}

/// CSV-Felder mit Anführungszeichen und verdoppelten Anführungszeichen.
fn felder(zeile: &str, trenner: char) -> Result<Vec<String>, String> {
    let mut chars = zeile.chars().peekable();
    let mut zitiert = false;
    let mut feld = String::new();
    let mut ausgabe = Vec::new();
    while let Some(c) = chars.next() {
        if c == '"' {
            if zitiert && chars.peek() == Some(&'"') {
                feld.push('"');
                chars.next();
            } else {
                zitiert = !zitiert;
            }
        } else if c == trenner && !zitiert {
            ausgabe.push(feld.trim().to_owned());
            feld.clear();
        } else {
            feld.push(c);
        }
    }
    if zitiert {
        return Err("Nicht geschlossenes Anführungszeichen.".into());
    }
    ausgabe.push(feld.trim().to_owned());
    Ok(ausgabe)
}

pub fn parse(text: &str) -> Result<StationierungsImport, String> {
    const ALIASE: [&[&str]; 8] = [
        &["punktnr", "punktnummer", "nr"],
        &["r", "rm", "rechtswert", "x"],
        &["h", "hm", "hochwert", "y"],
        &["hoehe", "hoehem", "z"],
        &["hz", "hzgon"],
        &["v", "vgon"],
        &["strecke", "streckem", "schraegstrecke"],
        &["zielhoehe", "zielhoehem", "zh", "zhm"],
    ];
    const TITEL: [&str; 8] = [
        "PunktNr",
        "R",
        "H",
        "Hoehe",
        "HZ",
        "V",
        "Strecke",
        "Zielhoehe",
    ];
    let mut import = StationierungsImport {
        punkte: Vec::new(),
        standpunkt: None,
        instrumentenhoehe: None,
    };
    let mut kopf: Option<([usize; 8], char)> = None;
    for (i, roh) in text.trim_start_matches('\u{feff}').lines().enumerate() {
        let zeile = roh.trim();
        if zeile.is_empty() || zeile.starts_with('#') || zeile == "---" {
            continue;
        }
        if kopf.is_none() {
            if let Some((schluessel, wert)) = zeile.split_once(':') {
                // Excel hängt beim Speichern leere Spalten an ("Wert,,,,,,"): Trennzeichen am Ende entfernen
                let wert = wert.trim().trim_end_matches([',', ';']).trim();
                match name(schluessel).as_str() {
                    "standpunktnr" | "standpunkt" => {
                        import.standpunkt = Some(wert.to_owned())
                    }
                    "instrumentenhoehe" | "instrhoehe" | "ih" => {
                        import.instrumentenhoehe = Some(
                            parse_zahl(wert)
                                .filter(|n| n.is_finite() && *n >= 0.0)
                                .ok_or_else(|| {
                                    format!("Zeile {}: ungültige Instrumentenhöhe.", i + 1)
                                })?,
                        );
                    }
                    _ => {}
                }
                continue;
            }
            let trenner = if zeile.contains(';') { ';' } else { ',' };
            let spalten = felder(zeile, trenner).map_err(|e| format!("Zeile {}: {e}", i + 1))?;
            let namen: Vec<_> = spalten.iter().map(|s| name(s)).collect();
            if !namen.iter().any(|n| ALIASE[0].contains(&n.as_str())) {
                continue;
            }
            let mut indizes = [0; 8];
            for j in 0..8 {
                indizes[j] = namen
                    .iter()
                    .position(|n| ALIASE[j].contains(&n.as_str()))
                    .ok_or_else(|| {
                        format!(
                            "Die CSV-Kopfzeile enthält keine Spalte „{}“. Erwartet: {}.",
                            TITEL[j],
                            TITEL.join(", ")
                        )
                    })?;
            }
            kopf = Some((indizes, trenner));
            continue;
        }
        let (idx, trenner) = kopf.unwrap();
        let spalten = felder(zeile, trenner).map_err(|e| format!("Zeile {}: {e}", i + 1))?;
        let feld = |j: usize| spalten.get(idx[j]).map(String::as_str).unwrap_or("");
        let zahl = |j: usize| {
            parse_zahl(feld(j))
                .filter(|n| n.is_finite())
                .ok_or_else(|| {
                    format!(
                        "Zeile {}: ungültiger oder fehlender Wert für {}.",
                        i + 1,
                        TITEL[j]
                    )
                })
        };
        if feld(0).is_empty() {
            return Err(format!("Zeile {}: Punktnummer fehlt.", i + 1));
        }
        import.punkte.push(StationierungsPunkt {
            punkt_nr: feld(0).to_owned(),
            r: zahl(1)?,
            h: zahl(2)?,
            hoehe: zahl(3)?,
            hz: zahl(4)?,
            v: zahl(5)?,
            strecke: zahl(6)?,
            zielhoehe: zahl(7)?,
        });
    }
    if kopf.is_none() {
        return Err(format!(
            "Keine Stationierungs-Kopfzeile gefunden. Erwartete Spalten: {}.",
            TITEL.join(", ")
        ));
    }
    if import.punkte.is_empty() {
        return Err("Die CSV-Datei enthält keine Stationierungsdaten.".into());
    }
    Ok(import)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn viewer_format_mit_mehrfachmessungen() {
        let d=parse("PunktNr,R,H,Hoehe,HZ,V,Strecke,Zielhoehe\n7,100,200,300,45,100,20,1.7\n7,100,200,300,245,300,20,1.7").unwrap();
        assert_eq!(d.punkte.len(), 2);
        assert_eq!(d.punkte[1].hz, 245.0);
        assert_eq!(d.punkte[0].zielhoehe, 1.7);
        assert!(d.standpunkt.is_none());
    }
    #[test]
    fn semikolon_dezimalkomma_metadaten_und_spaltenreihenfolge() {
        let d=parse("\u{feff}# METADATA\nStandpunktNr: SP7\nInstrumentenhoehe: 1,5\n# DATEN\nHZ_gon;Punkt-Nr;H [m];R [m];Höhe;V;Strecke_m;ZH\n45;\"P;7\";200,25;100,5;300;100;20,5;1,7").unwrap();
        assert_eq!(d.standpunkt.as_deref(), Some("SP7"));
        assert_eq!(d.instrumentenhoehe, Some(1.5));
        assert_eq!(d.punkte[0].punkt_nr, "P;7");
        assert_eq!(d.punkte[0].r, 100.5);
        assert_eq!(d.punkte[0].h, 200.25);
    }
    #[test]
    fn excel_dateien_mit_angehaengten_leeren_spalten() {
        let d = parse("# METADATA,,,,,,,
StandpunktNr: 100,,,,,,,
Instrumentenhoehe: 1.500,,,,,,,
# DATEN,,,,,,,
PunktNr,R,H,Hoehe,HZ,V,Strecke,Zielhoehe
1641,647878.77,5473201.04,0,263.6864,90.8284,1.454,0
").unwrap();
        assert_eq!(d.standpunkt.as_deref(), Some("100"));
        assert_eq!(d.instrumentenhoehe, Some(1.5));
        assert_eq!(d.punkte.len(), 1);
        assert_eq!(d.punkte[0].punkt_nr, "1641");
    }
    #[test]
    fn ungueltige_und_reine_koordinatendateien_werden_abgewiesen() {
        assert!(
            parse("PunktNr;R;H;Hoehe\n1;100;200;300")
                .unwrap_err()
                .contains("HZ")
        );
        let e = parse("PunktNr,R,H,Hoehe,HZ,V,Strecke,Zielhoehe\n1,100,200,300,45,100,NaN,1.7")
            .unwrap_err();
        assert!(e.contains("Zeile 2") && e.contains("Strecke"));
        assert!(parse("PunktNr,R,H,Hoehe,HZ,V,Strecke,Zielhoehe\n").is_err());
    }
}
