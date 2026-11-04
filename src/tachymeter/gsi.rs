// Zweck:    Leica GSI-8/GSI-16 (Datei und serielle Übertragung) sowie GSI Online (TPS300/700):
//           Parser, Fehlertexte und Befehlsstrings
//           Block: WI(2) + Info(4) + Vorzeichen(1) + Daten(8 bzw. 16)
// Hardware: Leica TPS300/700 (GSI Online), Datenexport anderer Leica-Geräte
// Autor:    Johann Gerner

use std::path::Path;

use super::messung::{MessungsTyp, TachymeterMessung};
use crate::daten::modelle::KonvertierungPunkt;

/// Länge eines GSI-8-Blocks.
const GSI8_LAENGE: usize = 15;
/// Länge eines GSI-16-Blocks.
const GSI16_LAENGE: usize = 23;
/// Mindestlänge eines gültigen Blocks.
const MIN_BLOCK_LAENGE: usize = GSI8_LAENGE;

// ── Befehle GSI Online ────────────────────────────────────────────────────────

/// Messung auslösen: Hz, V, Schrägstrecke.
pub const BEFEHL_MESS_TRIGGER: &str = "GET/M/WI21/WI22/WI31";
/// Momentanwert Hz und V (ohne EDM).
pub const BEFEHL_WINKEL: &str = "GET/I/WI21/WI22";
/// Momentanwert nur Hz (Zielrichtungsanzeige).
pub const BEFEHL_HZ_WINKEL: &str = "GET/I/WI21";
/// Verbindungsprüfung (Winkel ohne EDM).
pub const BEFEHL_PING: &str = "GET/I/WI21";

/// Laserpointer (nur TCR-Modelle).
pub fn laser_befehl(an: bool) -> String {
    format!("SET/36/{}", i32::from(an))
}

/// EDM-Modus: Prisma = IR Standard (0), Reflektorlos = RL Long (6) oder RL Short (7).
pub fn edm_modus_befehl(ziel_typ: i32, edm_prog: i32) -> String {
    let modus = if ziel_typ == 1 { if edm_prog == 6 { 6 } else { 7 } } else { 0 };
    format!("SET/161/{modus}")
}

// ── Dekodierung ───────────────────────────────────────────────────────────────

/// Dekodiert einen Rohwert nach dem Einheitencode (letztes Zeichen des Info-Felds).
fn dekodiere(raw: i64, einheit: char) -> f64 {
    match einheit {
        '0' | '1' => raw as f64 / 1000.0,
        '2' | '3' | '5' | '8' => raw as f64 / 100_000.0,
        '4' => dekodiere_grad_dms(raw),
        '6' | '7' => raw as f64 / 10_000.0,
        _ => raw as f64 / 1000.0,
    }
}

/// DMS-Format DDMMSS.SSSSS → Dezimalgrad.
fn dekodiere_grad_dms(raw: i64) -> f64 {
    let v = raw as f64 / 100_000.0;
    let sek = v % 100.0;
    let rest = (v / 100.0) as i64;
    let min = (rest % 100) as f64;
    let grad = (rest / 100) as f64;
    grad + min / 60.0 + sek / 3600.0
}

/// Zerlegter GSI-Block.
struct Block<'a> {
    wi: u32,
    einheit: char,
    vorzeichen: char,
    daten: &'a str,
}

fn zerlege_block(token: &str) -> Option<Block<'_>> {
    if token.len() < MIN_BLOCK_LAENGE || !token.is_ascii() {
        return None;
    }
    let wi: u32 = token[0..2].parse().ok()?;
    let einheit = token.as_bytes()[5] as char;
    let vorzeichen = token.as_bytes()[6] as char;
    Some(Block { wi, einheit, vorzeichen, daten: &token[7..] })
}

/// Punktnummer aus dem Datenfeld: Ganzzahl ohne führende Nullen, sonst Text ohne führende Nullen.
fn punkt_nr(daten: &str) -> String {
    let roh = daten.trim_end();
    roh.parse::<i64>().map_or_else(|_| roh.trim_start_matches('0').to_string(), |n| n.to_string())
}

fn zeile_ohne_praefix(zeile: &str) -> &str {
    zeile.strip_prefix('*').unwrap_or(zeile)
}

/// Kann diese Zeile als GSI-Datenzeile gelesen werden?
pub fn kann_verarbeiten_gsi(zeile: &str) -> bool {
    let z = zeile_ohne_praefix(zeile.trim_start());
    let Some(t) = z.split_whitespace().next() else {
        return false;
    };
    (t.len() == GSI8_LAENGE || t.len() == GSI16_LAENGE)
        && t.chars().take(2).all(|c| c.is_ascii_digit())
        && t.chars().count() >= 2
}

/// Parst eine GSI-Datenzeile zu einer Messung.
pub fn parse_gsi_zeile(zeile: &str) -> Option<TachymeterMessung> {
    let zeile = zeile.trim();
    if zeile.is_empty() {
        return None;
    }
    let zeile = zeile_ohne_praefix(zeile);
    let tokens: Vec<&str> = zeile.split_whitespace().collect();
    if tokens.is_empty() {
        return None;
    }
    let ist_gsi16 = tokens.iter().any(|t| t.len() >= GSI16_LAENGE);
    let mut m = TachymeterMessung::neu(if ist_gsi16 { "GSI-16" } else { "GSI-8" }, zeile);
    let mut extras: Vec<String> = Vec::new();

    for token in tokens {
        let Some(b) = zerlege_block(token) else {
            continue;
        };
        if b.wi == 11 {
            m.punkt_nr = punkt_nr(b.daten);
            continue;
        }
        let Ok(mut roh) = b.daten.trim().parse::<i64>() else {
            continue;
        };
        if b.vorzeichen == '-' {
            roh = -roh;
        }
        let wert = dekodiere(roh, b.einheit);
        match b.wi {
            21 => m.hz_gon = Some(wert),
            22 => m.v_gon = Some(wert),
            31 => m.schraegstrecke_m = Some(wert),
            32 => {
                m.horizontalstrecke_m.get_or_insert(wert);
            }
            33 => m.hoehenunterschied_m = Some(wert),
            81 => m.e_m = Some(wert),
            82 => m.n_m = Some(wert),
            83 => m.h_m = Some(wert),
            84 => m.standpunkt_e_m = Some(wert),
            85 => m.standpunkt_n_m = Some(wert),
            86 => m.standpunkt_h_m = Some(wert),
            87 => m.zielhoehe_m = Some(wert),
            88 => m.instrumenthoehe_m = Some(wert),
            41..=49 => {
                let code = b.daten.trim_end().trim_start_matches('0').to_string();
                if b.wi == 41 && m.punktcode.is_empty() {
                    m.punktcode = code;
                } else if !code.is_empty() {
                    extras.push(format!("WI{}={code}", b.wi));
                }
            }
            71..=79 => {
                let attr = b.daten.trim_end().trim_start_matches('0');
                if !attr.is_empty() {
                    extras.push(format!("Attr{}={attr}", b.wi));
                }
            }
            _ => {}
        }
    }
    if !extras.is_empty() {
        m.bemerkung = extras.join(", ");
    }

    let hat_koord = m.e_m.is_some() || m.n_m.is_some();
    let hat_winkel = m.hz_gon.is_some() || m.v_gon.is_some();
    let hat_dist = m.schraegstrecke_m.is_some() || m.horizontalstrecke_m.is_some();
    m.typ = if hat_winkel && hat_dist {
        MessungsTyp::Vollmessung
    } else if hat_winkel {
        MessungsTyp::Winkel
    } else if hat_koord {
        MessungsTyp::Koordinate
    } else {
        MessungsTyp::Unbekannt
    };
    if m.typ == MessungsTyp::Unbekannt && m.punkt_nr.is_empty() {
        return None;
    }
    Some(m)
}

/// Liest eine GSI-Datei (Latin-1) als Liste von Messungen.
pub fn importiere_datei(pfad: &Path) -> std::io::Result<Vec<TachymeterMessung>> {
    let bytes = std::fs::read(pfad)?;
    let text: String = bytes.iter().map(|b| *b as char).collect();
    Ok(text.lines().filter_map(parse_gsi_zeile).collect())
}

/// Liest eine GSI-Datei als Konvertierungspunkte (Typ „Import-GSI").
pub fn lese_datei_als_punkte(pfad: &Path) -> std::io::Result<Vec<KonvertierungPunkt>> {
    let bytes = std::fs::read(pfad)?;
    let text: String = bytes.iter().map(|b| *b as char).collect();
    Ok(text.lines().filter_map(|z| parse_punkt_zeile(z.trim())).collect())
}

/// Ältere Zeilenvariante des Datei-Imports (Koordinaten, Messwerte, Bemerkungen).
fn parse_punkt_zeile(zeile: &str) -> Option<KonvertierungPunkt> {
    if zeile.is_empty() {
        return None;
    }
    let zeile = zeile_ohne_praefix(zeile);
    let mut p = KonvertierungPunkt::default();
    let mut bemerkung: Vec<String> = Vec::new();
    for token in zeile.split_whitespace() {
        let Some(b) = zerlege_block(token) else {
            continue;
        };
        if b.wi == 11 {
            p.punkt_nr = punkt_nr(b.daten);
            continue;
        }
        let Ok(mut roh) = b.daten.trim().parse::<i64>() else {
            continue;
        };
        if b.vorzeichen == '-' {
            roh = -roh;
        }
        let w = dekodiere(roh, b.einheit);
        match b.wi {
            21 => p.hz = w,
            22 => p.v = w,
            31 => p.strecke = w,
            32 => {
                if p.strecke == 0.0 {
                    p.strecke = w;
                }
            }
            33 => bemerkung.push(format!("Δh={w:.3}m")),
            81 => p.r = w,
            82 => p.h = w,
            83 => p.hoehe = w,
            84 => bemerkung.push(format!("STP-R={w:.3}m")),
            85 => bemerkung.push(format!("STP-H={w:.3}m")),
            86 => bemerkung.push(format!("STP-Hoehe={w:.3}m")),
            87 => p.zielhoehe = w,
            88 => bemerkung.push(format!("Inst-h={w:.3}m")),
            41..=49 => {
                let code = b.daten.trim_end().trim_start_matches('0').to_string();
                if b.wi == 41 && p.punktcode.is_empty() {
                    p.punktcode = code;
                } else if !code.is_empty() {
                    bemerkung.push(format!("WI{}={code}", b.wi));
                }
            }
            71..=79 => {
                let attr = b.daten.trim_end().trim_start_matches('0');
                if !attr.is_empty() {
                    bemerkung.push(format!("Attr{}={attr}", b.wi));
                }
            }
            _ => {}
        }
    }
    if !bemerkung.is_empty() {
        p.bemerkung = bemerkung.join(", ");
    }
    p.typ = "Import-GSI".to_string();
    let hat_koord = p.r != 0.0 || p.h != 0.0;
    let hat_messung = p.hz != 0.0 || p.v != 0.0 || p.strecke != 0.0;
    if !hat_koord && !hat_messung && p.punkt_nr.is_empty() {
        return None;
    }
    Some(p)
}

// ── GSI Online ────────────────────────────────────────────────────────────────

/// Quellenname des Online-Parsers.
const ONLINE_NAME: &str = "GSI-Online";

/// Kann der Online-Parser diese Zeile verarbeiten?
pub fn kann_verarbeiten_online(zeile: &str) -> bool {
    let t = zeile.trim_start();
    if t.is_empty() {
        return false;
    }
    t == "?" || t.starts_with("@W") || t.starts_with("@E") || t.starts_with("@?") || kann_verarbeiten_gsi(t)
}

/// Parst eine Zeile der GSI-Online-Schnittstelle.
pub fn parse_online_zeile(zeile: &str) -> Option<TachymeterMessung> {
    let t = zeile.trim();
    if t == "?" {
        let mut m = TachymeterMessung::neu(ONLINE_NAME, zeile);
        m.typ = MessungsTyp::Status;
        m.bemerkung = "OK".to_string();
        return Some(m);
    }
    if t.starts_with("@E") || t.starts_with("@W") || t.starts_with("@?") {
        let mut m = TachymeterMessung::neu(ONLINE_NAME, zeile);
        m.typ = MessungsTyp::Fehler;
        m.bemerkung = beschreibe_fehler(t);
        return Some(m);
    }
    parse_gsi_zeile(zeile).map(|mut m| {
        m.quelle = ONLINE_NAME.to_string();
        m
    })
}

fn beschreibe_fehler(code: &str) -> String {
    match code {
        "@W100" => "Gerät beschäftigt – Messung läuft noch",
        "@W127" => "Ungültiger Befehl oder Parameter nicht erkannt",
        "@E139" => "EDM-Fehler: Prisma fehlt oder Signal zu schwach/stark",
        "@E158" => "Kompensator-Fehler: Instrument zu stark geneigt",
        "@?" => "Befehl vom Gerät nicht erkannt",
        anderes => anderes,
    }
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// GSI-8-Zeile mit Punktnummer, Hz, V und Schrägstrecke.
    const GSI8: &str = "110001+00000012 21.022+17920860 22.022+09843660 31..00+00152340";

    #[test]
    fn gsi8_vollmessung_wird_dekodiert() {
        let m = parse_gsi_zeile(GSI8).unwrap();
        assert_eq!(m.quelle, "GSI-8");
        assert_eq!(m.punkt_nr, "12");
        assert!((m.hz_gon.unwrap() - 179.2086).abs() < 1e-9);
        assert!((m.v_gon.unwrap() - 98.4366).abs() < 1e-9);
        // Einheitencode 0 = Millimeter-Auflösung
        assert!((m.schraegstrecke_m.unwrap() - 152.34).abs() < 1e-9);
        assert_eq!(m.typ, MessungsTyp::Vollmessung);
    }

    #[test]
    fn negative_werte_und_koordinaten() {
        let m = parse_gsi_zeile("81..00-00012345 82..00+00054321").unwrap();
        assert_eq!(m.typ, MessungsTyp::Koordinate);
        assert!((m.e_m.unwrap() + 12.345).abs() < 1e-9);
        assert!((m.n_m.unwrap() - 54.321).abs() < 1e-9);
    }

    #[test]
    fn gsi16_mit_praefix() {
        let m = parse_gsi_zeile("*110001+0000000000000007 21.022+0000000010000000").unwrap();
        assert_eq!(m.quelle, "GSI-16");
        assert_eq!(m.punkt_nr, "7");
        assert!((m.hz_gon.unwrap() - 100.0).abs() < 1e-9);
    }

    #[test]
    fn erkennung_und_leere_zeilen() {
        assert!(kann_verarbeiten_gsi(GSI8));
        assert!(!kann_verarbeiten_gsi("%R1P,0,0:0"));
        assert!(!kann_verarbeiten_gsi(""));
        assert!(parse_gsi_zeile("").is_none());
        assert!(parse_gsi_zeile("zu kurz").is_none());
    }

    #[test]
    fn einheitencodes() {
        assert!((dekodiere(1_234_567, '6') - 123.4567).abs() < 1e-9);
        assert!((dekodiere(1_234_567, '8') - 12.34567).abs() < 1e-9);
        // 12°34'56,00"
        assert!((dekodiere(12_345_600_000, '4') - (12.0 + 34.0 / 60.0 + 56.0 / 3600.0)).abs() < 1e-9);
    }

    #[test]
    fn online_bestaetigung_fehler_und_daten() {
        assert_eq!(parse_online_zeile("?").unwrap().bemerkung, "OK");
        let f = parse_online_zeile("@E139").unwrap();
        assert_eq!(f.typ, MessungsTyp::Fehler);
        assert!(f.bemerkung.contains("EDM-Fehler"));
        assert_eq!(parse_online_zeile(GSI8).unwrap().quelle, "GSI-Online");
        assert!(kann_verarbeiten_online("@W100"));
        assert!(!kann_verarbeiten_online("hallo"));
    }

    #[test]
    fn online_befehle() {
        assert_eq!(laser_befehl(true), "SET/36/1");
        assert_eq!(edm_modus_befehl(0, 3), "SET/161/0");
        assert_eq!(edm_modus_befehl(1, 6), "SET/161/6");
        assert_eq!(edm_modus_befehl(1, 5), "SET/161/7");
    }

    #[test]
    fn datei_import_als_punkte() {
        let dir = std::env::temp_dir().join(format!("fb-gsi-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let pfad = dir.join("a.gsi");
        std::fs::write(&pfad, format!("{GSI8}\n\n81..00+00001000 82..00+00002000 83..00+00000500\n")).unwrap();
        let p = lese_datei_als_punkte(&pfad).unwrap();
        assert_eq!(p.len(), 2);
        assert_eq!(p[0].typ, "Import-GSI");
        assert!((p[0].hz - 179.2086).abs() < 1e-9);
        assert!((p[1].r - 1.0).abs() < 1e-9);
        let m = importiere_datei(&pfad).unwrap();
        assert_eq!(m.len(), 2);
        let _ = std::fs::remove_dir_all(dir);
    }
}
