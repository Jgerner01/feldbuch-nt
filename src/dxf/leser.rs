// Zweck:    DXF-Leser: liest Gruppencode/Wert-Paare und extrahiert LINE, CIRCLE, ARC, LWPOLYLINE, POINT,
//           TEXT/MTEXT und INSERT (wie die Originalversion; andere Entities werden ignoriert)
// Hardware: keine
// Autor:    Johann Gerner

use std::fs;
use std::path::Path;

use eframe::egui::Color32;

use super::entity::{Art, DxfEntity};

/// Gruppencode des Layernamens.
const CODE_LAYER: i32 = 8;
/// Gruppencode der ACI-Farbe.
const CODE_FARBE: i32 = 62;

/// Liest alle unterstützten Entities aus einer DXF-Datei.
pub fn lese(pfad: &Path) -> Result<Vec<DxfEntity>, String> {
    let bytes = fs::read(pfad).map_err(|e| format!("DXF-Datei konnte nicht geöffnet werden: {e}"))?;
    let text = dekodiere(&bytes);
    Ok(parse_entities(&lese_gruppen(&text)))
}

/// Erkennt UTF-8, UTF-16 LE sowie die deklarierte Windows-1252-Kodierung älterer DXFs.
fn dekodiere(bytes: &[u8]) -> String {
    if bytes.starts_with(&[0xFF, 0xFE]) {
        let einheiten: Vec<u16> = bytes[2..].chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
        return String::from_utf16_lossy(&einheiten);
    }
    let ohne_bom = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(bytes);
    if let Ok(text) = std::str::from_utf8(ohne_bom) {
        return text.to_owned();
    }
    if ohne_bom.windows(b"ANSI_1252".len()).any(|w| w.eq_ignore_ascii_case(b"ANSI_1252")) {
        const C1: [char;32] = [
            '€','\u{81}','‚','ƒ','„','…','†','‡','ˆ','‰','Š','‹','Œ','\u{8d}','Ž','\u{8f}',
            '\u{90}','‘','’','“','”','•','–','—','˜','™','š','›','œ','\u{9d}','ž','Ÿ',
        ];
        return ohne_bom.iter().map(|b| if (0x80..=0x9f).contains(b) {C1[usize::from(*b-0x80)]} else {char::from(*b)}).collect();
    }
    String::from_utf8_lossy(ohne_bom).into_owned()
}

/// Liest Gruppencode-Paare; Paare ohne gültigen Code werden übersprungen.
fn lese_gruppen(text: &str) -> Vec<(i32, String)> {
    let mut zeilen = text.lines();
    let mut ergebnis = Vec::new();
    while let Some(code_zeile) = zeilen.next() {
        let wert = zeilen.next().map_or("", str::trim).to_string();
        if let Ok(code) = code_zeile.trim().parse::<i32>() {
            ergebnis.push((code, wert));
        }
    }
    ergebnis
}

fn parse_entities(gruppen: &[(i32, String)]) -> Vec<DxfEntity> {
    let mut ergebnis = Vec::new();
    let mut in_entities = false;
    let mut i = 0;
    while i < gruppen.len() {
        let (code, wert) = (&gruppen[i].0, gruppen[i].1.as_str());
        match (*code, wert) {
            (0, "SECTION") => {
                i += 1;
                continue;
            }
            (2, "ENTITIES") => {
                in_entities = true;
                i += 1;
                continue;
            }
            (0, "ENDSEC") => {
                in_entities = false;
                i += 1;
                continue;
            }
            (0, "EOF") => break,
            _ => {}
        }
        if !in_entities {
            i += 1;
            continue;
        }
        if *code == 0 {
            let typ = wert;
            i += 1;
            let start = i;
            while i < gruppen.len() && gruppen[i].0 != 0 {
                i += 1;
            }
            if let Some(entity) = parse_entity(typ, &gruppen[start..i]) {
                ergebnis.push(entity);
            }
        } else {
            i += 1;
        }
    }
    ergebnis
}

fn parse_entity(typ: &str, props: &[(i32, String)]) -> Option<DxfEntity> {
    let layer = hole(props, CODE_LAYER).unwrap_or("0").to_string();
    let farbe = aci_farbe(props);
    let art = match typ {
        "LINE" => Art::Line {
            x1: zahl(props, 10),
            y1: zahl(props, 20),
            x2: zahl(props, 11),
            y2: zahl(props, 21),
        },
        "CIRCLE" => Art::Circle { cx: zahl(props, 10), cy: zahl(props, 20), radius: zahl(props, 40) },
        "ARC" => Art::Arc {
            cx: zahl(props, 10),
            cy: zahl(props, 20),
            radius: zahl(props, 40),
            start: zahl(props, 50),
            ende: zahl(props, 51),
        },
        "LWPOLYLINE" => {
            let flags = hole(props, 70).and_then(|t| t.trim().parse::<i32>().ok()).unwrap_or(0);
            let mut punkte = Vec::new();
            let mut vx: Option<f64> = None;
            for (c, v) in props {
                if *c == 10 {
                    vx = Some(parse_zahl(v));
                } else if *c == 20 {
                    if let Some(x) = vx.take() {
                        punkte.push((x, parse_zahl(v)));
                    }
                }
            }
            if punkte.is_empty() {
                return None;
            }
            Art::LwPolyline { punkte, geschlossen: flags & 1 != 0 }
        }
        "POINT" => Art::Point { x: zahl(props, 10), y: zahl(props, 20) },
        "TEXT" | "MTEXT" => Art::Text {
            x: zahl(props, 10),
            y: zahl(props, 20),
            hoehe: zahl(props, 40),
            text: hole(props, 1).unwrap_or("").to_string(),
        },
        "INSERT" => Art::Insert {
            block: hole(props, 2).unwrap_or("").to_string(),
            x: zahl(props, 10),
            y: zahl(props, 20),
        },
        _ => return None,
    };
    Some(DxfEntity { layer, farbe, art })
}

/// Letzter Wert zu einem Gruppencode.
fn hole(props: &[(i32, String)], code: i32) -> Option<&str> {
    props.iter().rev().find(|(c, _)| *c == code).map(|(_, v)| v.as_str())
}

fn zahl(props: &[(i32, String)], code: i32) -> f64 {
    hole(props, code).map_or(0.0, parse_zahl)
}

fn parse_zahl(text: &str) -> f64 {
    text.trim().parse().unwrap_or(0.0)
}

fn aci_farbe(props: &[(i32, String)]) -> Option<Color32> {
    let aci = hole(props, CODE_FARBE).and_then(|t| t.trim().parse::<i32>().ok())?;
    aci_zu_farbe(aci)
}

/// AutoCAD-Farbindex → Farbe (für weißen Hintergrund angepasst).
fn aci_zu_farbe(aci: i32) -> Option<Color32> {
    let rgb = |r, g, b| Some(Color32::from_rgb(r, g, b));
    match aci {
        1 => rgb(255, 0, 0),
        2 => rgb(180, 140, 0),
        3 => rgb(0, 128, 0),
        4 => rgb(0, 128, 128),
        5 => rgb(0, 0, 255),
        6 => rgb(128, 0, 128),
        7 => rgb(0, 0, 0),
        8 => rgb(80, 80, 80),
        9 => rgb(120, 120, 120),
        250 => rgb(100, 100, 100),
        251 => rgb(120, 120, 120),
        252 => rgb(150, 150, 150),
        253 => rgb(170, 170, 170),
        254 => rgb(190, 190, 190),
        255 => rgb(0, 0, 0),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BEISPIEL: &str = "0\nSECTION\n2\nENTITIES\n0\nLINE\n8\nGrenze\n62\n1\n10\n0.0\n20\n0.0\n11\n3.0\n21\n4.0\n\
        0\nLWPOLYLINE\n8\n0\n70\n1\n10\n1\n20\n2\n10\n3\n20\n4\n0\nINSERT\n8\ngrenzpunkt_genau\n2\nBLK\n10\n5\n20\n6\n\
        0\nSPLINE\n8\n0\n0\nENDSEC\n0\nEOF\n";

    #[test]
    fn liest_unterstuetzte_entities_und_ignoriert_andere() {
        let e = parse_entities(&lese_gruppen(BEISPIEL));
        assert_eq!(e.len(), 3);
        assert_eq!(e[0].layer, "Grenze");
        assert_eq!(e[0].farbe, Some(Color32::from_rgb(255, 0, 0)));
        match &e[1].art {
            Art::LwPolyline { punkte, geschlossen } => {
                assert_eq!(punkte, &vec![(1.0, 2.0), (3.0, 4.0)]);
                assert!(*geschlossen);
            }
            _ => panic!("Polylinie erwartet"),
        }
        assert!(matches!(e[2].art, Art::Insert { x, y, .. } if x == 5.0 && y == 6.0));
    }

    #[test]
    fn utf16_wird_erkannt() {
        let mut bytes = vec![0xFF, 0xFE];
        for u in "0\nEOF\n".encode_utf16() {
            bytes.extend(u.to_le_bytes());
        }
        assert_eq!(dekodiere(&bytes), "0\nEOF\n");
    }

    #[test]
    fn deklarierte_windows1252_dxf_behaelt_umlaute_und_sonderzeichen() {
        let bytes = b"0\nSECTION\n2\nHEADER\n9\n$DWGCODEPAGE\n3\nANSI_1252\n0\nENDSEC\n0\nSECTION\n2\nENTITIES\n0\nTEXT\n8\n0\n10\n0\n20\n0\n40\n2\n1\nH\xf6he Stra\xdfe \x80\n0\nENDSEC\n0\nEOF\n";
        let text = dekodiere(bytes);
        assert!(!text.contains('\u{fffd}'));
        let e = parse_entities(&lese_gruppen(&text));
        assert!(matches!(&e[0].art, Art::Text{text,..} if text=="Höhe Straße €"));
        let refs=e.iter().map(|e|(e,Color32::BLACK)).collect::<Vec<_>>();
        let scene=crate::dxf::pdf::erzeuge(&eframe::egui::Context::default(),&crate::daten::rechenparameter::PdfParameter::default(),[0.0,0.0],&refs,"",None);
        assert!(crate::dxf::pdf::pdf(&scene).is_ok(),"Die korrekt dekodierten Beschriftungen müssen als PDF exportierbar sein");
        assert_eq!(dekodiere("ANSI_1252\nHöhe Straße €".as_bytes()),"ANSI_1252\nHöhe Straße €");
    }
}
