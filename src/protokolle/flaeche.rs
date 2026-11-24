// Zweck:    Protokoll der Flächenberechnung (RTF), wahlweise mit eingebettetem Lageplan (PNG mit gefüllter Fläche,
//           Umriss und Punktnummern)
// Hardware: keine
// Autor:    Johann Gerner

use std::io::Cursor;

use chrono::NaiveDateTime;
use image::{ImageFormat, Rgba, RgbaImage};

use super::fl_teilung::{fuelle, kreis, linie};
use super::rtf::Tabellen;
use super::{Protokoll, VORLAGE_FLAECHENBERECHNUNG, felder, zeile, zeitpunkt_text};
use crate::grundlagen::format::f;
use crate::rechnen::flaeche as rechnung;

/// Bildgröße des Lageplans [px] (Seitenverhältnis 4:3, passend zu 165 × 124 mm der Vorlage).
const BREITE: u32 = 1100;
const HOEHE: u32 = 825;
/// Rand im Bild [px] (Platz für die Punktnummern).
const RAND: f64 = 90.0;
const HINTERGRUND: Rgba<u8> = Rgba([250, 250, 252, 255]);
const UMRISS: Rgba<u8> = Rgba([20, 30, 60, 255]);
const PUNKT_FUELLUNG: Rgba<u8> = Rgba([255, 255, 0, 255]);
const PUNKT_RAND: Rgba<u8> = Rgba([200, 90, 0, 255]);
const TEXT_FARBE: Rgba<u8> = Rgba([10, 10, 10, 255]);
const TEXT_SAUM: Rgba<u8> = Rgba([255, 255, 255, 255]);
/// Durchsichtigkeit der Flächenfüllung (0–255).
const FUELL_ALPHA: u8 = 110;
const PUNKT_RADIUS: f64 = 8.0;
/// Vergrößerung der 5×7-Schrift.
const SCHRIFT_FAKTOR: i64 = 3;

/// Ein Eckpunkt der Fläche.
#[derive(Clone, Debug, PartialEq)]
pub struct FlaechenPunkt {
    pub nr: String,
    pub r: f64,
    pub h: f64,
}

/// Meldung zu einer ungültigen Fläche („" wenn in Ordnung).
pub fn hinweis(punkte: &[FlaechenPunkt]) -> String {
    let p: Vec<_> = punkte.iter().map(|q| (q.r, q.h)).collect();
    if p.len() < 3 {
        "Zu wenige Eckpunkte: mindestens 3 sind nötig.".to_string()
    } else if rechnung::selbstschnitt(&p) {
        "ACHTUNG: Die Seiten der Fläche überschneiden sich – Fläche nicht eindeutig. Reihenfolge der Punkte prüfen!".to_string()
    } else {
        String::new()
    }
}

/// Flächenprotokoll; mit `mit_grafik` wird der Lageplan in der gewählten Farbe eingebettet.
pub fn protokoll(punkte: &[FlaechenPunkt], farbe: [u8; 3], mit_grafik: bool, bearbeiter: &str, jetzt: NaiveDateTime) -> Protokoll {
    let p: Vec<(f64, f64)> = punkte.iter().map(|q| (q.r, q.h)).collect();
    let a = rechnung::flaeche(&p);
    let s = rechnung::schwerpunkt(&p);
    let hinweis = hinweis(punkte);
    let felder = felder(&[
        ("Bearbeiter", bearbeiter.to_string()),
        ("Zeitpunkt", zeitpunkt_text(jetzt)),
        ("Flaeche_m2", format!("{} m²", f(a, 3))),
        ("Flaeche_ha", format!("{} ha", f(a / 10_000.0, 6))),
        ("Umfang", format!("{} m", f(rechnung::umfang(&p), 3))),
        ("Anzahl", punkte.len().to_string()),
        ("Schwerpunkt", format!("R {}   H {}", f(s.0, 3), f(s.1, 3))),
        ("HatHinweis", if hinweis.is_empty() { "0" } else { "1" }.to_string()),
        ("Hinweis", hinweis),
        ("MitGrafik", if mit_grafik { "1" } else { "0" }.to_string()),
        ("Lageplan", if mit_grafik { png_hex(&lageplan(punkte, farbe)) } else { String::new() }),
    ]);
    let zeilen: Vec<_> = punkte
        .iter()
        .enumerate()
        .map(|(i, q)| zeile(&[("Lfd", (i + 1).to_string()), ("Nr", q.nr.clone()), ("R", f(q.r, 3)), ("H", f(q.h, 3))]))
        .collect();
    Protokoll { vorlage: VORLAGE_FLAECHENBERECHNUNG, praefix: "Flaechenberechnung", felder, tabellen: Tabellen::from([(String::new(), zeilen)]), zeitpunkt: jetzt }
}

/// PNG-Daten als Hex-Text (für RTF `\pngblip`).
fn png_hex(bild: &RgbaImage) -> String {
    let mut daten = Cursor::new(Vec::new());
    if bild.write_to(&mut daten, ImageFormat::Png).is_err() {
        return String::new();
    }
    daten.into_inner().iter().map(|b| format!("{b:02x}")).collect()
}

/// Zeichnet den Lageplan: gefüllte Fläche, Umriss, Eckpunkte mit Punktnummern, Flächenangabe im Schwerpunkt.
pub fn lageplan(punkte: &[FlaechenPunkt], farbe: [u8; 3]) -> RgbaImage {
    let mut bild = RgbaImage::from_pixel(BREITE, HOEHE, HINTERGRUND);
    if punkte.is_empty() {
        return bild;
    }
    let p: Vec<(f64, f64)> = punkte.iter().map(|q| (q.r, q.h)).collect();
    let (r_min, r_max) = p.iter().fold((f64::MAX, f64::MIN), |(lo, hi), q| (lo.min(q.0), hi.max(q.0)));
    let (h_min, h_max) = p.iter().fold((f64::MAX, f64::MIN), |(lo, hi), q| (lo.min(q.1), hi.max(q.1)));
    let (breite_welt, hoehe_welt) = ((r_max - r_min).max(1e-9), (h_max - h_min).max(1e-9));
    // Gleicher Maßstab in beiden Achsen, Zeichnung zentriert
    let massstab = ((f64::from(BREITE) - 2.0 * RAND) / breite_welt).min((f64::from(HOEHE) - 2.0 * RAND) / hoehe_welt);
    let (ox, oy) = (
        (f64::from(BREITE) - breite_welt * massstab) / 2.0 - r_min * massstab,
        (f64::from(HOEHE) + hoehe_welt * massstab) / 2.0 + h_min * massstab,
    );
    let bild_pos = |q: (f64, f64)| (q.0 * massstab + ox, -q.1 * massstab + oy);
    let px: Vec<(f64, f64)> = p.iter().map(|q| bild_pos(*q)).collect();

    fuelle(&mut bild, &px, Rgba([farbe[0], farbe[1], farbe[2], FUELL_ALPHA]));
    if px.len() >= 2 {
        for i in 0..px.len() {
            if px.len() == 2 && i == 1 {
                break;
            }
            linie(&mut bild, px[i], px[(i + 1) % px.len()], UMRISS, false);
        }
    }
    for (q, punkt) in px.iter().zip(punkte) {
        kreis(&mut bild, *q, PUNKT_RADIUS, PUNKT_FUELLUNG, PUNKT_RAND);
        text(&mut bild, (q.0 as i64 + 12, q.1 as i64 - 12 - 7 * SCHRIFT_FAKTOR), &punkt.nr);
    }
    if p.len() >= 3 {
        let a = rechnung::flaeche(&p);
        let s = bild_pos(rechnung::schwerpunkt(&p));
        let beschriftung = format!("{} M2", f(a, 2));
        let halbe_breite = text_breite(&beschriftung) / 2;
        text(&mut bild, (s.0 as i64 - halbe_breite, s.1 as i64 - 7 * SCHRIFT_FAKTOR / 2), &beschriftung);
    }
    bild
}

// ── Schrift ──────────────────────────────────────────────────────────────────

/// Zeilenmuster (5 Bit breit, 7 Zeilen) eines Zeichens; unbekannte Zeichen bleiben leer.
fn glyphe(c: char) -> [u8; 7] {
    match c.to_ascii_uppercase() {
        '0' => [0b01110, 0b10001, 0b10011, 0b10101, 0b11001, 0b10001, 0b01110],
        '1' => [0b00100, 0b01100, 0b00100, 0b00100, 0b00100, 0b00100, 0b01110],
        '2' => [0b01110, 0b10001, 0b00001, 0b00010, 0b00100, 0b01000, 0b11111],
        '3' => [0b11110, 0b00001, 0b00001, 0b01110, 0b00001, 0b00001, 0b11110],
        '4' => [0b00010, 0b00110, 0b01010, 0b10010, 0b11111, 0b00010, 0b00010],
        '5' => [0b11111, 0b10000, 0b11110, 0b00001, 0b00001, 0b10001, 0b01110],
        '6' => [0b00110, 0b01000, 0b10000, 0b11110, 0b10001, 0b10001, 0b01110],
        '7' => [0b11111, 0b00001, 0b00010, 0b00100, 0b01000, 0b01000, 0b01000],
        '8' => [0b01110, 0b10001, 0b10001, 0b01110, 0b10001, 0b10001, 0b01110],
        '9' => [0b01110, 0b10001, 0b10001, 0b01111, 0b00001, 0b00010, 0b01100],
        'A' => [0b01110, 0b10001, 0b10001, 0b11111, 0b10001, 0b10001, 0b10001],
        'B' => [0b11110, 0b10001, 0b10001, 0b11110, 0b10001, 0b10001, 0b11110],
        'C' => [0b01110, 0b10001, 0b10000, 0b10000, 0b10000, 0b10001, 0b01110],
        'D' => [0b11110, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b11110],
        'E' => [0b11111, 0b10000, 0b10000, 0b11110, 0b10000, 0b10000, 0b11111],
        'F' => [0b11111, 0b10000, 0b10000, 0b11110, 0b10000, 0b10000, 0b10000],
        'G' => [0b01110, 0b10001, 0b10000, 0b10111, 0b10001, 0b10001, 0b01111],
        'H' => [0b10001, 0b10001, 0b10001, 0b11111, 0b10001, 0b10001, 0b10001],
        'I' => [0b01110, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b01110],
        'J' => [0b00111, 0b00010, 0b00010, 0b00010, 0b00010, 0b10010, 0b01100],
        'K' => [0b10001, 0b10010, 0b10100, 0b11000, 0b10100, 0b10010, 0b10001],
        'L' => [0b10000, 0b10000, 0b10000, 0b10000, 0b10000, 0b10000, 0b11111],
        'M' => [0b10001, 0b11011, 0b10101, 0b10101, 0b10001, 0b10001, 0b10001],
        'N' => [0b10001, 0b11001, 0b10101, 0b10011, 0b10001, 0b10001, 0b10001],
        'O' => [0b01110, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01110],
        'P' => [0b11110, 0b10001, 0b10001, 0b11110, 0b10000, 0b10000, 0b10000],
        'Q' => [0b01110, 0b10001, 0b10001, 0b10001, 0b10101, 0b10010, 0b01101],
        'R' => [0b11110, 0b10001, 0b10001, 0b11110, 0b10100, 0b10010, 0b10001],
        'S' => [0b01111, 0b10000, 0b10000, 0b01110, 0b00001, 0b00001, 0b11110],
        'T' => [0b11111, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100],
        'U' => [0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01110],
        'V' => [0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01010, 0b00100],
        'W' => [0b10001, 0b10001, 0b10001, 0b10101, 0b10101, 0b10101, 0b01010],
        'X' => [0b10001, 0b10001, 0b01010, 0b00100, 0b01010, 0b10001, 0b10001],
        'Y' => [0b10001, 0b10001, 0b01010, 0b00100, 0b00100, 0b00100, 0b00100],
        'Z' => [0b11111, 0b00001, 0b00010, 0b00100, 0b01000, 0b10000, 0b11111],
        '-' => [0, 0, 0, 0b11111, 0, 0, 0],
        '.' => [0, 0, 0, 0, 0, 0b01100, 0b01100],
        ',' => [0, 0, 0, 0, 0b01100, 0b00100, 0b01000],
        '_' => [0, 0, 0, 0, 0, 0, 0b11111],
        '/' => [0b00001, 0b00010, 0b00010, 0b00100, 0b01000, 0b01000, 0b10000],
        ':' => [0, 0b01100, 0b01100, 0, 0b01100, 0b01100, 0],
        '+' => [0, 0b00100, 0b00100, 0b11111, 0b00100, 0b00100, 0],
        _ => [0; 7],
    }
}

/// Breite des Textes [px].
fn text_breite(t: &str) -> i64 {
    t.chars().count() as i64 * 6 * SCHRIFT_FAKTOR
}

/// Zeichnet Text mit weißem Saum (lesbar auf Fläche und Linien); `pos` ist die linke obere Ecke.
fn text(bild: &mut RgbaImage, pos: (i64, i64), t: &str) {
    for (dx, dy) in [(-2, 0), (2, 0), (0, -2), (0, 2), (-2, -2), (2, 2), (-2, 2), (2, -2)] {
        zeichne_text(bild, (pos.0 + dx, pos.1 + dy), t, TEXT_SAUM);
    }
    zeichne_text(bild, pos, t, TEXT_FARBE);
}

fn zeichne_text(bild: &mut RgbaImage, pos: (i64, i64), t: &str, farbe: Rgba<u8>) {
    for (n, c) in t.chars().enumerate() {
        let x0 = pos.0 + n as i64 * 6 * SCHRIFT_FAKTOR;
        for (zeile, bits) in glyphe(c).iter().enumerate() {
            for spalte in 0..5 {
                if bits >> (4 - spalte) & 1 == 0 {
                    continue;
                }
                for sx in 0..SCHRIFT_FAKTOR {
                    for sy in 0..SCHRIFT_FAKTOR {
                        let (x, y) = (x0 + spalte as i64 * SCHRIFT_FAKTOR + sx, pos.1 + zeile as i64 * SCHRIFT_FAKTOR + sy);
                        if x >= 0 && y >= 0 && x < i64::from(BREITE) && y < i64::from(HOEHE) {
                            bild.put_pixel(x as u32, y as u32, farbe);
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::daten::modelle::jetzt;
    use crate::protokolle::pruefe_vollstaendig;

    fn l_form() -> Vec<FlaechenPunkt> {
        [("A1", 0.0, 0.0), ("A2", 10.0, 0.0), ("A3", 10.0, 4.0), ("A4", 4.0, 4.0), ("A5", 4.0, 10.0), ("A6", 0.0, 10.0)]
            .iter()
            .map(|(n, r, h)| FlaechenPunkt { nr: (*n).to_string(), r: *r, h: *h })
            .collect()
    }

    #[test]
    fn protokoll_mit_grafik_enthaelt_bild_und_werte() {
        let p = protokoll(&l_form(), [0, 150, 200], true, "Tester", jetzt());
        let rtf = pruefe_vollstaendig(&p);
        assert_eq!(p.felder["Flaeche_m2"], "64.000 m²");
        assert_eq!(p.felder["Flaeche_ha"], "0.006400 ha");
        assert_eq!(p.felder["Umfang"], "40.000 m");
        assert_eq!(p.tabellen[""].len(), 6);
        assert!(rtf.contains("\\pict\\pngblip"));
        // PNG-Signatur 89 50 4E 47
        assert!(p.felder["Lageplan"].starts_with("89504e47"));
        assert!(!rtf.contains("überschneiden"));
    }

    #[test]
    fn protokoll_ohne_grafik_hat_kein_bild() {
        let p = protokoll(&l_form(), [0, 150, 200], false, "Tester", jetzt());
        let rtf = pruefe_vollstaendig(&p);
        assert!(!rtf.contains("\\pict"));
        assert!(p.felder["Lageplan"].is_empty());
    }

    #[test]
    fn selbstschnitt_wird_im_protokoll_gemeldet() {
        let mut punkte = l_form();
        punkte.truncate(4);
        punkte.swap(1, 2);
        punkte[0].nr = "X".into();
        let schleife = vec![
            FlaechenPunkt { nr: "1".into(), r: 0.0, h: 0.0 },
            FlaechenPunkt { nr: "2".into(), r: 10.0, h: 10.0 },
            FlaechenPunkt { nr: "3".into(), r: 10.0, h: 0.0 },
            FlaechenPunkt { nr: "4".into(), r: 0.0, h: 10.0 },
        ];
        let p = protokoll(&schleife, [1, 2, 3], false, "T", jetzt());
        assert!(pruefe_vollstaendig(&p).contains("berschneiden"));
        assert!(hinweis(&punkte[..2]).contains("mindestens 3"));
    }

    #[test]
    fn lageplan_zeigt_flaeche_und_punktnummern() {
        let bild = lageplan(&l_form(), [200, 0, 0]);
        assert_eq!(bild.dimensions(), (BREITE, HOEHE));
        assert!(bild.pixels().any(|p| *p == TEXT_FARBE), "Text gezeichnet");
        assert!(bild.pixels().any(|p| p.0[0] > 200 && p.0[1] < 200 && p.0[2] < 200), "Füllung in der gewählten Farbe");
        // Leere Liste zeichnet nichts und stürzt nicht ab
        assert!(lageplan(&[], [0, 0, 0]).pixels().all(|p| *p == HINTERGRUND));
    }
}
