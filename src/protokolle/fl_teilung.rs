// Zweck:    Protokoll der Flächenteilung (RTF) mit Lageplanskizze als PNG
// Hardware: keine
// Autor:    Johann Gerner

use std::path::{Path, PathBuf};

use chrono::NaiveDateTime;
use image::{Rgba, RgbaImage};

use super::rtf::Tabellen;
use super::{Protokoll, VORLAGE_FLAECHENTEILUNG, felder, signiert, zeile, zeitpunkt_text};
use crate::grundlagen::format::f;
use crate::rechnen::flaechenteilung::{Ergebnis, Punkt};

/// Bildgröße des Lageplans.
const BREITE: u32 = 800;
const HOEHE: u32 = 600;
/// Rand im Bild [px].
const RAND: f64 = 50.0;
/// Farben des Lageplans.
const HINTERGRUND: Rgba<u8> = Rgba([240, 240, 248, 255]);
const UMRISS: Rgba<u8> = Rgba([30, 60, 130, 255]);
const TEIL1: Rgba<u8> = Rgba([173, 216, 230, 110]);
const TEIL2: Rgba<u8> = Rgba([144, 238, 144, 110]);
const TRENNLINIE: Rgba<u8> = Rgba([255, 0, 0, 255]);
const GRENZPUNKT_FUELLUNG: Rgba<u8> = Rgba([255, 255, 0, 255]);
const GRENZPUNKT_RAND: Rgba<u8> = Rgba([255, 140, 0, 255]);
/// Radius der Grenzpunkte [px] und Strichlänge der Trennlinie.
const GRENZPUNKT_RADIUS: f64 = 7.0;
const STRICH: f64 = 8.0;
const LUECKE: f64 = 5.0;

/// Protokoll der Flächenteilung.
pub fn protokoll(polygon: &[Punkt], erg: &Ergebnis, bearbeiter: &str, jetzt: NaiveDateTime) -> Protokoll {
    let png = png_name(&Protokoll { vorlage: "", praefix: "Flächenteilung", felder: Default::default(), tabellen: Tabellen::new(), zeitpunkt: jetzt });
    let felder = felder(&[
        ("Bearbeiter", bearbeiter.to_string()),
        ("Zeitpunkt", zeitpunkt_text(jetzt)),
        ("Verfahren", erg.verfahren.clone()),
        ("A_gesamt", format!("{} m²", f(erg.a_gesamt, 4))),
        ("A1_soll", format!("{} m²", f(erg.a1_soll, 4))),
        ("A1_ist", format!("{} m²", f(erg.a1_ist, 4))),
        ("A2", format!("{} m²", f(erg.a2, 4))),
        ("Differenz", format!("{} m²", signiert(erg.differenz(), 4))),
        ("Iterationen", erg.iterationen.to_string()),
        ("LageplanHinweis", format!("Lageplanskizze: {png}")),
    ]);
    let mut zeilen: Vec<_> = polygon
        .iter()
        .enumerate()
        .map(|(i, p)| zeile(&[("Nr", (i + 1).to_string()), ("R", f(p.0, 3)), ("H", f(p.1, 3)), ("Art", String::new())]))
        .collect();
    zeilen.extend(
        erg.neue_grenzpunkte
            .iter()
            .map(|(nr, r, h)| zeile(&[("Nr", nr.clone()), ("R", f(*r, 3)), ("H", f(*h, 3)), ("Art", "Grenzpunkt".to_string())])),
    );
    Protokoll { vorlage: VORLAGE_FLAECHENTEILUNG, praefix: "Flächenteilung", felder, tabellen: Tabellen::from([(String::new(), zeilen)]), zeitpunkt: jetzt }
}

fn png_name(p: &Protokoll) -> String {
    let rtf = p.pfad(Path::new(""));
    format!("{}_lageplan.png", rtf.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default())
}

/// Schreibt RTF und Lageplan; liefert beide Pfade.
pub fn schreibe(polygon: &[Punkt], erg: &Ergebnis, bearbeiter: &str, jetzt: NaiveDateTime, verzeichnis: &Path) -> Result<(PathBuf, PathBuf), String> {
    let p = protokoll(polygon, erg, bearbeiter, jetzt);
    let rtf = p.schreibe(verzeichnis)?;
    let png = verzeichnis.join(png_name(&p));
    lageplan(polygon, erg).save(&png).map_err(|e| format!("Lageplan konnte nicht geschrieben werden: {e}"))?;
    Ok((rtf, png))
}

/// Mischt eine Farbe mit Alpha über den Hintergrund.
fn mische(bild: &mut RgbaImage, x: i64, y: i64, farbe: Rgba<u8>) {
    if x < 0 || y < 0 || x >= i64::from(BREITE) || y >= i64::from(HOEHE) {
        return;
    }
    let (x, y) = (x as u32, y as u32);
    let alt = *bild.get_pixel(x, y);
    let a = f64::from(farbe.0[3]) / 255.0;
    let kanal = |i: usize| (f64::from(farbe.0[i]) * a + f64::from(alt.0[i]) * (1.0 - a)).round() as u8;
    bild.put_pixel(x, y, Rgba([kanal(0), kanal(1), kanal(2), 255]));
}

/// Füllt ein Polygon (gerade-ungerade-Regel, Scanline).
fn fuelle(bild: &mut RgbaImage, punkte: &[(f64, f64)], farbe: Rgba<u8>) {
    if punkte.len() < 3 {
        return;
    }
    for y in 0..HOEHE {
        let yc = f64::from(y) + 0.5;
        let mut xs: Vec<f64> = Vec::new();
        for i in 0..punkte.len() {
            let (a, b) = (punkte[i], punkte[(i + 1) % punkte.len()]);
            if (a.1 <= yc && b.1 > yc) || (b.1 <= yc && a.1 > yc) {
                xs.push(a.0 + (yc - a.1) / (b.1 - a.1) * (b.0 - a.0));
            }
        }
        xs.sort_by(f64::total_cmp);
        for paar in xs.chunks_exact(2) {
            for x in paar[0].round() as i64..paar[1].round() as i64 {
                mische(bild, x, i64::from(y), farbe);
            }
        }
    }
}

/// Zeichnet eine Linie der Stärke 2 px; mit `gestrichelt` abwechselnd Strich und Lücke.
fn linie(bild: &mut RgbaImage, a: (f64, f64), b: (f64, f64), farbe: Rgba<u8>, gestrichelt: bool) {
    let laenge = (b.0 - a.0).hypot(b.1 - a.1);
    let schritte = laenge.ceil().max(1.0) as usize;
    for i in 0..=schritte {
        let weg = laenge * i as f64 / schritte as f64;
        if gestrichelt && weg % (STRICH + LUECKE) > STRICH {
            continue;
        }
        let t = i as f64 / schritte as f64;
        let (x, y) = (a.0 + t * (b.0 - a.0), a.1 + t * (b.1 - a.1));
        for dx in 0..2 {
            for dy in 0..2 {
                mische(bild, x as i64 + dx, y as i64 + dy, farbe);
            }
        }
    }
}

fn kreis(bild: &mut RgbaImage, m: (f64, f64), radius: f64, fuellung: Rgba<u8>, rand: Rgba<u8>) {
    let r = radius.ceil() as i64 + 1;
    for dy in -r..=r {
        for dx in -r..=r {
            let d = ((dx * dx + dy * dy) as f64).sqrt();
            let (x, y) = (m.0 as i64 + dx, m.1 as i64 + dy);
            if d <= radius - 1.0 {
                mische(bild, x, y, fuellung);
            } else if d <= radius {
                mische(bild, x, y, rand);
            }
        }
    }
}

/// Zeichnet die Lageplanskizze (Teilflächen, Umriss, Trennlinie, Grenzpunkte; ohne Beschriftung).
pub fn lageplan(polygon: &[Punkt], erg: &Ergebnis) -> RgbaImage {
    let mut bild = RgbaImage::from_pixel(BREITE, HOEHE, HINTERGRUND);
    if polygon.len() < 2 {
        return bild;
    }
    let (r_min, r_max) = polygon.iter().fold((f64::MAX, f64::MIN), |(lo, hi), p| (lo.min(p.0), hi.max(p.0)));
    let (h_min, h_max) = polygon.iter().fold((f64::MAX, f64::MIN), |(lo, hi), p| (lo.min(p.1), hi.max(p.1)));
    let spanne = (r_max - r_min).max(h_max - h_min);
    let spanne = if spanne < 1e-9 { 1.0 } else { spanne };
    let massstab = (f64::from(BREITE.min(HOEHE)) - 2.0 * RAND) / spanne;
    let off_r = RAND + (f64::from(BREITE) - 2.0 * RAND - (r_max - r_min) * massstab) / 2.0 - r_min * massstab;
    let off_h = f64::from(HOEHE) - RAND - (f64::from(HOEHE) - 2.0 * RAND - (h_max - h_min) * massstab) / 2.0 + h_min * massstab;
    let bildpunkt = |p: Punkt| (p.0 * massstab + off_r, -p.1 * massstab + off_h);
    let punkte = |liste: &[Punkt]| -> Vec<(f64, f64)> { liste.iter().map(|p| bildpunkt(*p)).collect() };

    fuelle(&mut bild, &punkte(&erg.polygon1), TEIL1);
    fuelle(&mut bild, &punkte(&erg.polygon2), TEIL2);
    if polygon.len() >= 3 {
        let p = punkte(polygon);
        for i in 0..p.len() {
            linie(&mut bild, p[i], p[(i + 1) % p.len()], UMRISS, false);
        }
    }
    if erg.neue_grenzpunkte.len() >= 2 {
        let (a, b) = (&erg.neue_grenzpunkte[0], &erg.neue_grenzpunkte[1]);
        linie(&mut bild, bildpunkt((a.1, a.2)), bildpunkt((b.1, b.2)), TRENNLINIE, true);
    }
    for (_, r, h) in &erg.neue_grenzpunkte {
        kreis(&mut bild, bildpunkt((*r, *h)), GRENZPUNKT_RADIUS, GRENZPUNKT_FUELLUNG, GRENZPUNKT_RAND);
    }
    bild
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::daten::modelle::jetzt;
    use crate::protokolle::pruefe_vollstaendig;
    use crate::rechnen::flaechenteilung as ft;

    fn quadrat() -> Vec<Punkt> {
        vec![(0.0, 0.0), (100.0, 0.0), (100.0, 100.0), (0.0, 100.0)]
    }

    #[test]
    fn protokoll_enthaelt_ecken_und_grenzpunkte() {
        let q = quadrat();
        let erg = ft::parallele(&q, 0, 2500.0).unwrap();
        let p = protokoll(&q, &erg, "Tester", jetzt());
        let rtf = pruefe_vollstaendig(&p);
        assert_eq!(p.tabellen[""].len(), 4 + 2);
        assert_eq!(p.tabellen[""][5]["Art"], "Grenzpunkt");
        assert!(p.felder["A1_ist"].starts_with("2500.0") || p.felder["A1_ist"].starts_with("2499.9"), "{}", p.felder["A1_ist"]);
        assert!(p.felder["A1_ist"].ends_with(" m²"));
        assert!(rtf.contains("Lageplanskizze"));
        assert!(p.felder["LageplanHinweis"].ends_with("_lageplan.png"));
    }

    #[test]
    fn lageplan_ist_gezeichnet_und_wird_geschrieben() {
        let q = quadrat();
        let erg = ft::parallele(&q, 0, 2500.0).unwrap();
        let bild = lageplan(&q, &erg);
        assert_eq!(bild.dimensions(), (BREITE, HOEHE));
        assert!(bild.pixels().any(|p| *p != HINTERGRUND), "es wurde etwas gezeichnet");
        let dir = std::env::temp_dir().join(format!("fb-fl-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let (rtf, png) = schreibe(&q, &erg, "Tester", jetzt(), &dir).unwrap();
        assert!(rtf.exists() && png.exists());
        assert!(std::fs::metadata(&png).unwrap().len() > 100);
        let _ = std::fs::remove_dir_all(dir);
    }
}
