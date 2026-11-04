// Zweck:    Konvertierung von Punktdaten: Import aus GSI (Leica GSI-8/-16) und DXF, Duplikaterkennung,
//           Nummernvergabe sowie Export als CSV, KOR und DAT
// Hardware: keine
// Autor:    Johann Gerner

use std::fs;
use std::path::Path;

use crate::dxf::entity::Art;
use crate::dxf::leser;
use crate::grundlagen::format::f;

/// Duplikat-Toleranz für Koordinaten [m].
pub const DUPLIKAT_EPS_M: f64 = 0.001;
/// Typ des Standpunkts für den DAT-Export.
const TYP_STANDPUNKT: &str = "Standpunkt";
/// Vorgaben des DAT-Exports ohne Standpunkt.
const STANDARD_STANDPUNKT: &str = "STP1";
const STANDARD_STANDPUNKT_CODE: &str = "005";
/// Mindestlänge eines GSI-Blocks (GSI-8: WI 2 + Info 4 + Vorzeichen 1 + Daten 8).
const GSI_MIN_BLOCKLAENGE: usize = 15;
/// Blocklänge, ab der GSI-16 vorliegt (wird nur zur Unterscheidung benötigt).
const GSI_16_BLOCKLAENGE: usize = 23;

/// Ein Punkt der Konvertierungstabelle.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct KonvertierungPunkt {
    pub punkt_nr: String,
    pub typ: String,
    pub r: f64,
    pub h: f64,
    pub hoehe: f64,
    pub hz: f64,
    pub v: f64,
    pub strecke: f64,
    pub zielhoehe: f64,
    pub punktcode: String,
    pub bemerkung: String,
}

// ── GSI ──────────────────────────────────────────────────────────────────────

/// Liest eine GSI-Datei (Latin-1).
pub fn gsi_lese(pfad: &Path) -> std::io::Result<Vec<KonvertierungPunkt>> {
    let text: String = fs::read(pfad)?.iter().map(|b| char::from(*b)).collect();
    Ok(gsi_parse(&text))
}

/// Parst den Text einer GSI-Datei.
pub fn gsi_parse(text: &str) -> Vec<KonvertierungPunkt> {
    text.lines().map(str::trim).filter(|z| !z.is_empty()).filter_map(gsi_zeile).collect()
}

fn dekodiere(roh: i64, einheit: char) -> f64 {
    let r = roh as f64;
    match einheit {
        '2' | '3' | '5' | '8' => r / 100_000.0,
        '4' => dekodiere_dms(roh),
        '6' | '7' => r / 10_000.0,
        _ => r / 1000.0,
    }
}

/// DMS-Format DDMMSS.SSSSS → Dezimalgrad.
fn dekodiere_dms(roh: i64) -> f64 {
    let v = roh as f64 / 100_000.0;
    let sekunden = v % 100.0;
    let rest = (v / 100.0) as i64;
    let minuten = (rest % 100) as f64;
    let grad = (rest / 100) as f64;
    grad + minuten / 60.0 + sekunden / 3600.0
}

fn gsi_zeile(zeile: &str) -> Option<KonvertierungPunkt> {
    let zeile = zeile.strip_prefix('*').unwrap_or(zeile);
    let tokens: Vec<Vec<char>> = zeile.split_whitespace().map(|t| t.chars().collect()).collect();
    if tokens.is_empty() {
        return None;
    }
    let _gsi16 = tokens.iter().any(|t| t.len() >= GSI_16_BLOCKLAENGE);
    let mut p = KonvertierungPunkt::default();
    let mut bemerkung: Vec<String> = Vec::new();
    for token in &tokens {
        if token.len() < GSI_MIN_BLOCKLAENGE {
            continue;
        }
        let wi_text: String = token[..2].iter().collect();
        let Ok(wi) = wi_text.parse::<i32>() else {
            continue;
        };
        let einheit = token[5];
        let vorzeichen = token[6];
        let daten: String = token[7..].iter().collect();

        if wi == 11 {
            let roh = daten.trim_end();
            p.punkt_nr = match roh.parse::<i64>() {
                Ok(n) => n.to_string(),
                Err(_) => roh.trim_start_matches('0').to_string(),
            };
            continue;
        }
        let Ok(mut roh) = daten.trim().parse::<i64>() else {
            continue;
        };
        if vorzeichen == '-' {
            roh = -roh;
        }
        let wert = dekodiere(roh, einheit);
        match wi {
            21 => p.hz = wert,
            22 => p.v = wert,
            31 => p.strecke = wert,
            32 => {
                if p.strecke == 0.0 {
                    p.strecke = wert;
                }
            }
            33 => bemerkung.push(format!("Δh={}m", f(wert, 3))),
            81 => p.r = wert,
            82 => p.h = wert,
            83 => p.hoehe = wert,
            84 => bemerkung.push(format!("STP-R={}m", f(wert, 3))),
            85 => bemerkung.push(format!("STP-H={}m", f(wert, 3))),
            86 => bemerkung.push(format!("STP-Hoehe={}m", f(wert, 3))),
            87 => p.zielhoehe = wert,
            88 => bemerkung.push(format!("Inst-h={}m", f(wert, 3))),
            41..=49 => {
                let code = daten.trim_end().trim_start_matches('0').to_string();
                if wi == 41 && p.punktcode.is_empty() {
                    p.punktcode = code;
                } else if !code.is_empty() {
                    bemerkung.push(format!("WI{wi}={code}"));
                }
            }
            71..=79 => {
                let attribut = daten.trim_end().trim_start_matches('0');
                if !attribut.is_empty() {
                    bemerkung.push(format!("Attr{wi}={attribut}"));
                }
            }
            _ => {}
        }
    }
    p.bemerkung = bemerkung.join(", ");
    p.typ = "Import-GSI".to_string();
    let hat_koordinaten = p.r != 0.0 || p.h != 0.0;
    let hat_messung = p.hz != 0.0 || p.v != 0.0 || p.strecke != 0.0;
    if !hat_koordinaten && !hat_messung && p.punkt_nr.is_empty() {
        return None;
    }
    Some(p)
}

// ── DXF ──────────────────────────────────────────────────────────────────────

/// Extrahiert Koordinaten aus einer DXF-Datei (Linienendpunkte, Symbole, Punkte, Kreismittelpunkte).
pub fn dxf_import(pfad: &Path) -> Result<Vec<KonvertierungPunkt>, String> {
    let entities = leser::lese(pfad)?;
    let mut ergebnis = Vec::new();
    let punkt = |typ: &str, r: f64, h: f64, code: &str, bem: String| KonvertierungPunkt {
        typ: typ.to_string(),
        r,
        h,
        punktcode: code.to_string(),
        bemerkung: bem,
        ..KonvertierungPunkt::default()
    };
    for e in &entities {
        match &e.art {
            Art::Line { x1, y1, x2, y2 } => {
                ergebnis.push(punkt("DXF-Linie", *x1, *y1, "", format!("Layer={}, Linienendpunkt 1", e.layer)));
                ergebnis.push(punkt("DXF-Linie", *x2, *y2, "", format!("Layer={}, Linienendpunkt 2", e.layer)));
            }
            Art::Insert { block, x, y } => ergebnis.push(punkt("DXF-Symbol", *x, *y, block, format!("Layer={}, Block={block}", e.layer))),
            Art::Point { x, y } => ergebnis.push(punkt("DXF-Punkt", *x, *y, "", format!("Layer={}", e.layer))),
            Art::Circle { cx, cy, radius } => ergebnis.push(punkt("DXF-Kreis", *cx, *cy, "", format!("Layer={}, R={}m", e.layer, f(*radius, 3)))),
            _ => {}
        }
    }
    Ok(ergebnis)
}

// ── Tabellenlogik ────────────────────────────────────────────────────────────

/// Ist der neue Punkt schon vorhanden (gleiche Nummer oder gleiche Koordinate innerhalb der Toleranz)?
pub fn ist_doppelt(neu: &KonvertierungPunkt, vorhandene: &[KonvertierungPunkt]) -> bool {
    vorhandene.iter().any(|v| {
        let gleiche_nr = !neu.punkt_nr.is_empty() && !v.punkt_nr.is_empty() && v.punkt_nr == neu.punkt_nr;
        let gleiche_lage = (v.r - neu.r).abs() < DUPLIKAT_EPS_M && (v.h - neu.h).abs() < DUPLIKAT_EPS_M && (neu.r != 0.0 || neu.h != 0.0);
        gleiche_nr || gleiche_lage
    })
}

/// Behält die Nummer, wenn sie frei ist; sonst die nächste freie fortlaufende Ganzzahl.
pub fn naechste_punkt_nr(vorschlag: &str, vorhandene: &[KonvertierungPunkt]) -> String {
    let hoechste = vorhandene.iter().filter_map(|p| p.punkt_nr.parse::<i32>().ok()).max().unwrap_or(0).max(0);
    if !vorschlag.is_empty() && !vorhandene.iter().any(|p| p.punkt_nr == vorschlag) {
        return vorschlag.to_string();
    }
    (hoechste + 1).to_string()
}

/// Fügt importierte Punkte hinzu; liefert (hinzugefügt, doppelt).
pub fn importiere_in(tabelle: &mut Vec<KonvertierungPunkt>, neue: Vec<KonvertierungPunkt>) -> (usize, usize) {
    let (mut hinzugefuegt, mut doppelt) = (0, 0);
    for mut p in neue {
        if ist_doppelt(&p, tabelle) {
            doppelt += 1;
            continue;
        }
        p.punkt_nr = naechste_punkt_nr(&p.punkt_nr, tabelle);
        tabelle.push(p);
        hinzugefuegt += 1;
    }
    (hinzugefuegt, doppelt)
}

// ── Export ───────────────────────────────────────────────────────────────────

/// CSV (Semikolon-getrennt).
pub fn csv_text(punkte: &[KonvertierungPunkt]) -> String {
    let mut text = "PunktNr;Typ;R;H;Hoehe;HZ;V;Strecke;Zielhoehe;Punktcode;Bemerkung\r\n".to_string();
    for z in punkte {
        text.push_str(&format!(
            "{};{};{};{};{};{};{};{};{};{};{}\r\n",
            z.punkt_nr,
            z.typ,
            f(z.r, 3),
            f(z.h, 3),
            f(z.hoehe, 3),
            f(z.hz, 4),
            f(z.v, 4),
            f(z.strecke, 3),
            f(z.zielhoehe, 3),
            z.punktcode,
            z.bemerkung
        ));
    }
    text
}

fn kopf(titel: &str, datum: &str, projekt: &str) -> String {
    format!("% {titel}\r\n% Datum:   {datum}\r\n% Projekt: {projekt}\r\n%\r\n")
}

/// KOR-Koordinatenliste.
pub fn kor_text(punkte: &[KonvertierungPunkt], datum: &str, projekt: &str) -> String {
    let mut text = kopf("Koordinatenliste – Feldbuch Export", datum, projekt);
    text.push_str("% PunktNr        Rechtswert [m]    Hochwert [m]      Hoehe [m]    Code  Bemerkung\r\n");
    for z in punkte {
        text.push_str(&format!("{:<16}{:>16}{:>16}{:>14}  {:<8}  {}\r\n", z.punkt_nr, f(z.r, 3), f(z.h, 3), f(z.hoehe, 3), z.punktcode, z.bemerkung));
    }
    text
}

/// DAT-Felddatenformat; liefert Text und Kennzeichen, ob ein Standpunkt gefunden wurde.
pub fn dat_text(punkte: &[KonvertierungPunkt], datum: &str, projekt: &str) -> (String, bool) {
    let standpunkt = punkte.iter().find(|p| p.typ.eq_ignore_ascii_case(TYP_STANDPUNKT));
    let nr = standpunkt.map_or(STANDARD_STANDPUNKT, |s| s.punkt_nr.as_str());
    let hoehe = standpunkt.map_or_else(|| f(0.0, 3), |s| f(s.hoehe, 3));
    let mut text = kopf("Feldbuch Export – DAT Format", datum, projekt);
    text.push_str(&format!("STANDPUNKT {nr} {hoehe} {STANDARD_STANDPUNKT_CODE}\r\n%\r\n"));
    text.push_str("% PunktNr        Rechtswert [m]    Hochwert [m]      Hoehe [m]    Code  Typ\r\n");
    for z in punkte {
        text.push_str(&format!("{:<16}{:>16}{:>16}{:>14}  {:<8}  {}\r\n", z.punkt_nr, f(z.r, 3), f(z.h, 3), f(z.hoehe, 3), z.punktcode, z.typ));
    }
    (text, standpunkt.is_some())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Baut einen GSI-8-Block: WI(2) + Info(4) + Vorzeichen + 8 Datenstellen.
    fn block(wi: &str, einheit: char, wert: i64) -> String {
        format!("{wi}000{einheit}{}{:08}", if wert < 0 { '-' } else { '+' }, wert.abs())
    }

    #[test]
    fn gsi_messzeile() {
        // Punkt 12, Hz 123.4567 gon, V 98.7654 gon, Strecke 25.123 m, Zielhöhe 1.500 m
        let zeile = format!("*{} {} {} {} {}", format!("110001+{:08}", 12), block("21", '2', 12_345_670), block("22", '2', 9_876_540), block("31", '0', 25_123), block("87", '0', 1_500));
        let p = gsi_parse(&zeile);
        assert_eq!(p.len(), 1);
        let p = &p[0];
        assert_eq!(p.punkt_nr, "12");
        assert!((p.hz - 123.4567).abs() < 1e-9 && (p.v - 98.7654).abs() < 1e-9);
        assert!((p.strecke - 25.123).abs() < 1e-9 && (p.zielhoehe - 1.5).abs() < 1e-9);
        assert_eq!(p.typ, "Import-GSI");
    }

    #[test]
    fn gsi_koordinaten_negativ_und_bemerkung() {
        let zeile = format!("{} {} {} {} {}", "110001+0000A001", block("81", '0', 4_500_123_456), block("82", '6', -53_000_001_234), block("83", '0', 450_500), block("33", '0', -1_250));
        let p = gsi_parse(&zeile);
        assert_eq!(p.len(), 1, "{zeile}");
        assert_eq!(p[0].punkt_nr, "A001");
        assert!((p[0].h + 5_300_000.1234).abs() < 1e-6, "{}", p[0].h);
        assert!(p[0].bemerkung.contains("Δh=-1.250m"));
    }

    #[test]
    fn gsi_ueberspringt_leere_und_ungueltige_zeilen() {
        assert!(gsi_parse("\n   \nkurz abc\n").is_empty());
        // Nur Nummer: wird übernommen
        assert_eq!(gsi_parse("110001+00000007").len(), 1);
    }

    #[test]
    fn gsi_dms_und_einheiten() {
        assert!((dekodiere(12_345_670, '2') - 123.4567).abs() < 1e-9);
        assert!((dekodiere(25_123, '0') - 25.123).abs() < 1e-12);
        assert!((dekodiere(251_234, '6') - 25.1234).abs() < 1e-12);
        // 12°34'56.5" als 123456.50000
        assert!((dekodiere(12_345_650_000, '4') - (12.0 + 34.0 / 60.0 + 56.5 / 3600.0)).abs() < 1e-9);
    }

    fn pkt(nr: &str, r: f64, h: f64) -> KonvertierungPunkt {
        KonvertierungPunkt { punkt_nr: nr.into(), typ: "Test".into(), r, h, ..KonvertierungPunkt::default() }
    }

    #[test]
    fn duplikate_und_nummernvergabe() {
        let mut tabelle = vec![pkt("1", 10.0, 20.0), pkt("A", 30.0, 40.0)];
        assert!(ist_doppelt(&pkt("1", 99.0, 99.0), &tabelle), "gleiche Nummer");
        assert!(ist_doppelt(&pkt("", 10.0005, 20.0005), &tabelle), "gleiche Lage");
        assert!(!ist_doppelt(&pkt("", 0.0, 0.0), &[pkt("", 0.0, 0.0)]), "(0,0) zählt nicht");
        assert_eq!(naechste_punkt_nr("7", &tabelle), "7");
        assert_eq!(naechste_punkt_nr("1", &tabelle), "2");
        assert_eq!(naechste_punkt_nr("", &tabelle), "2");
        // Nummer 1 ist vergeben (Duplikat), (10/20) liegt auf Punkt 1 (Duplikat), nur der dritte Punkt kommt hinzu
        let (neu, doppelt) = importiere_in(&mut tabelle, vec![pkt("1", 50.0, 60.0), pkt("", 10.0, 20.0), pkt("", 70.0, 80.0)]);
        assert_eq!((neu, doppelt), (1, 2));
        assert_eq!(tabelle[2].punkt_nr, "2");
        let (neu, _) = importiere_in(&mut tabelle, vec![pkt("", 90.0, 91.0)]);
        assert_eq!(neu, 1);
        assert_eq!(tabelle[3].punkt_nr, "3");
    }

    #[test]
    fn exporte() {
        let mut p = pkt("1", 100.5, 200.25);
        p.hoehe = 12.0;
        p.punktcode = "Baum".into();
        p.bemerkung = "x".into();
        let csv = csv_text(&[p.clone()]);
        assert!(csv.starts_with("PunktNr;Typ;R;H;Hoehe;HZ;V;Strecke;Zielhoehe;Punktcode;Bemerkung\r\n"));
        assert!(csv.contains("1;Test;100.500;200.250;12.000;0.0000;0.0000;0.000;0.000;Baum;x"));
        let kor = kor_text(&[p.clone()], "2026-01-01", "Demo");
        assert!(kor.contains("% Projekt: Demo"));
        assert!(kor.contains(&format!("{:<16}{:>16}{:>16}{:>14}  {:<8}  x", "1", "100.500", "200.250", "12.000", "Baum")));
        let (dat, mit) = dat_text(&[p.clone()], "2026-01-01", "Demo");
        assert!(!mit && dat.contains("STANDPUNKT STP1 0.000 005"));
        let mut stp = pkt("S9", 1.0, 2.0);
        stp.typ = "standpunkt".into();
        stp.hoehe = 3.5;
        let (dat, mit) = dat_text(&[stp, p], "2026-01-01", "Demo");
        assert!(mit && dat.contains("STANDPUNKT S9 3.500 005"));
    }

    #[test]
    fn dxf_import_aus_datei() {
        let dir = std::env::temp_dir().join(format!("fb-konv-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let pfad = dir.join("t.dxf");
        let dxf = "0\nSECTION\n2\nENTITIES\n0\nLINE\n8\nGrenze\n10\n1.0\n20\n2.0\n11\n3.0\n21\n4.0\n0\nPOINT\n8\nP\n10\n5.0\n20\n6.0\n0\nCIRCLE\n8\nK\n10\n7.0\n20\n8.0\n40\n1.5\n0\nENDSEC\n0\nEOF\n";
        fs::write(&pfad, dxf).unwrap();
        let p = dxf_import(&pfad).unwrap();
        assert_eq!(p.len(), 4, "{p:?}");
        assert_eq!((p[0].typ.as_str(), p[0].r, p[1].h), ("DXF-Linie", 1.0, 4.0));
        assert_eq!(p[0].bemerkung, "Layer=Grenze, Linienendpunkt 1");
        assert_eq!(p[2].typ, "DXF-Punkt");
        assert_eq!(p[3].bemerkung, "Layer=K, R=1.500m");
        let _ = fs::remove_dir_all(dir);
    }
}
