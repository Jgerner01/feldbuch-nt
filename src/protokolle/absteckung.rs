// Zweck:    Protokolle der Absteckung (Punkt-, Achs-, Raster-, Profilabsteckung, Schnurgerüst) mit Lageplan als PNG
// Hardware: keine
// Autor:    Johann Gerner

use std::path::{Path, PathBuf};

use chrono::NaiveDateTime;
use image::{Rgba, RgbaImage};

use super::fl_teilung::{kreis, linie};
use super::{
    Protokoll, STRICH, VORLAGE_ACHSABSTECKUNG, VORLAGE_PROFILABSTECKUNG, VORLAGE_PUNKTABSTECKUNG, VORLAGE_RASTERABSTECKUNG,
    VORLAGE_SCHNURGERUEST, felder, signiert, zeile, zeitpunkt_text,
};
use crate::grundlagen::format::f;
use crate::rechnen::absteckung::{Abbildung, AbsteckPunkt, ProfilAbsteckPunkt, StandpunktInfo};

/// Bildgröße des Lageplans.
const BREITE: u32 = 640;
const HOEHE: u32 = 480;
/// Farben des Lageplans (wie in der Kartenansicht).
const HINTERGRUND: Rgba<u8> = Rgba([245, 248, 252, 255]);
const RASTER: Rgba<u8> = Rgba([210, 220, 230, 255]);
const SOLLPUNKT: Rgba<u8> = Rgba([65, 105, 225, 255]);
const ABGESTECKT: Rgba<u8> = Rgba([46, 139, 87, 255]);
const STANDPUNKT_FUELLUNG: Rgba<u8> = Rgba([220, 20, 60, 255]);
const STANDPUNKT_RAND: Rgba<u8> = Rgba([139, 0, 0, 255]);
const POLYGON: Rgba<u8> = Rgba([255, 0, 0, 255]);
const SCHNURGERUEST: Rgba<u8> = Rgba([0, 0, 255, 255]);
const ACHSE: Rgba<u8> = Rgba([255, 165, 0, 255]);
const RAHMEN: Rgba<u8> = Rgba([160, 170, 190, 255]);
/// Radien der Marken [px].
const PUNKT_RADIUS: f64 = 5.0;
const STANDPUNKT_RADIUS: f64 = 8.0;

/// Ein Protokoll mit optionalem Lageplan.
#[derive(Clone, Debug)]
pub struct AbsteckProtokoll {
    pub protokoll: Protokoll,
    pub lageplan: Option<RgbaImage>,
    /// Dateinamenendung des Lageplans, z. B. „_lageplan.png".
    pub bild_endung: &'static str,
}

impl AbsteckProtokoll {
    fn png_name(&self) -> String {
        let rtf = self.protokoll.pfad(Path::new(""));
        format!("{}{}", rtf.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default(), self.bild_endung)
    }

    /// Schreibt RTF und, falls vorhanden, den Lageplan; liefert die Pfade.
    pub fn schreibe(&self, verzeichnis: &Path) -> Result<(PathBuf, Option<PathBuf>), String> {
        let rtf = self.protokoll.schreibe(verzeichnis)?;
        let png = match &self.lageplan {
            Some(bild) => {
                let pfad = verzeichnis.join(self.png_name());
                bild.save(&pfad).map_err(|e| format!("Lageplan konnte nicht geschrieben werden: {e}"))?;
                Some(pfad)
            }
            None => None,
        };
        Ok((rtf, png))
    }

    /// Meldungstext nach dem Schreiben.
    pub fn meldung(rtf: &Path, png: Option<&Path>) -> String {
        match png {
            Some(p) => format!("Protokoll gespeichert:\n{}\n\nLageplan:\n{}", rtf.display(), p.display()),
            None => format!("Protokoll gespeichert:\n{}", rtf.display()),
        }
    }
}

/// Standpunktzeile des Protokollkopfs.
fn standpunkt_text(st: Option<&StandpunktInfo>) -> String {
    st.map_or_else(
        || "–".to_string(),
        |s| format!("{}   R={}   H={}   z={} gon", s.punkt_nr, f(s.r, 3), f(s.h, 3), f(s.orientierung_gon, 4)),
    )
}

fn hz_text(st: Option<&StandpunktInfo>, wert: f64) -> String {
    if st.is_some() { f(wert, 4) } else { STRICH.to_string() }
}

fn s_text(st: Option<&StandpunktInfo>, wert: f64) -> String {
    if st.is_some() { f(wert, 3) } else { STRICH.to_string() }
}

fn rtf_protokoll(vorlage: &'static str, praefix: &'static str, felder: super::rtf::Felder, zeilen: Vec<super::rtf::Zeile>, jetzt: NaiveDateTime) -> Protokoll {
    Protokoll { vorlage, praefix, felder, tabellen: super::rtf::Tabellen::from([(String::new(), zeilen)]), zeitpunkt: jetzt }
}

/// Zeilen mit Punktnummer, R, H, Hz, s und Status (Punkt- und Rasterabsteckung).
fn punktzeilen(st: Option<&StandpunktInfo>, punkte: &[AbsteckPunkt]) -> Vec<super::rtf::Zeile> {
    punkte
        .iter()
        .map(|p| {
            zeile(&[
                ("PunktNr", p.punkt_nr.clone()),
                ("R", f(p.r_soll, 3)),
                ("H", f(p.h_soll, 3)),
                ("Hz", hz_text(st, p.hz_soll_gon)),
                ("s", s_text(st, p.s_soll_m)),
                ("Status", p.status.clone()),
            ])
        })
        .collect()
}

/// Protokoll der Punktabsteckung.
pub fn punktabsteckung(st: Option<&StandpunktInfo>, punkte: &[AbsteckPunkt], bearbeiter: &str, jetzt: NaiveDateTime) -> AbsteckProtokoll {
    let mut p = AbsteckProtokoll {
        protokoll: rtf_protokoll(VORLAGE_PUNKTABSTECKUNG, "Punktabsteckung", Default::default(), punktzeilen(st, punkte), jetzt),
        lageplan: Some(lageplan(st, punkte, &[], &[], None)),
        bild_endung: "_lageplan.png",
    };
    p.protokoll.felder = felder(&[
        ("Bearbeiter", bearbeiter.to_string()),
        ("Zeitpunkt", zeitpunkt_text(jetzt)),
        ("Standpunkt", standpunkt_text(st)),
        ("AnzahlPunkte", punkte.len().to_string()),
        ("LageplanHinweis", format!("Lageplanskizze: {}", p.png_name())),
    ]);
    p
}

/// Eingaben der Achsabsteckung für das Protokoll.
pub struct AchsEingabe<'a> {
    pub r_a: &'a str,
    pub h_a: &'a str,
    pub r_e: &'a str,
    pub h_e: &'a str,
    pub intervall: &'a str,
    pub offsets: &'a str,
}

/// Protokoll der Achsabsteckung.
pub fn achsabsteckung(
    st: Option<&StandpunktInfo>,
    punkte: &[AbsteckPunkt],
    achse: Option<((f64, f64), (f64, f64))>,
    e: &AchsEingabe,
    bearbeiter: &str,
    jetzt: NaiveDateTime,
) -> AbsteckProtokoll {
    let zeilen = punkte
        .iter()
        .map(|p| {
            zeile(&[
                ("PunktNr", p.punkt_nr.clone()),
                ("Station", p.label.clone()),
                ("R", f(p.r_soll, 3)),
                ("H", f(p.h_soll, 3)),
                ("Abs", p.abszisse_m.map_or_else(String::new, |v| f(v, 3))),
                ("Ord", p.ordinate_m.map_or_else(String::new, |v| f(v, 3))),
                ("Hz", hz_text(st, p.hz_soll_gon)),
                ("s", s_text(st, p.s_soll_m)),
            ])
        })
        .collect();
    let mut p = AbsteckProtokoll {
        protokoll: rtf_protokoll(VORLAGE_ACHSABSTECKUNG, "Achsabsteckung", Default::default(), zeilen, jetzt),
        lageplan: Some(lageplan(st, punkte, &[], &[], achse)),
        bild_endung: "_lageplan.png",
    };
    p.protokoll.felder = felder(&[
        ("Bearbeiter", bearbeiter.to_string()),
        ("Zeitpunkt", zeitpunkt_text(jetzt)),
        ("Standpunkt", standpunkt_text(st)),
        ("AchseA", format!("R={}  H={}", e.r_a, e.h_a)),
        ("AchseE", format!("R={}  H={}", e.r_e, e.h_e)),
        ("Intervall", format!("{} m", e.intervall)),
        ("Offsets", e.offsets.to_string()),
        ("AnzahlPunkte", punkte.len().to_string()),
        ("LageplanHinweis", format!("Lageplanskizze: {}", p.png_name())),
    ]);
    p
}

/// Eingaben der Rasterabsteckung für das Protokoll.
pub struct RasterEingabe<'a> {
    pub r0: &'a str,
    pub h0: &'a str,
    pub richtung: &'a str,
    pub d_s: &'a str,
    pub d_q: &'a str,
    pub zeilen: &'a str,
    pub spalten: &'a str,
}

/// Protokoll der Rasterabsteckung.
pub fn rasterabsteckung(st: Option<&StandpunktInfo>, punkte: &[AbsteckPunkt], e: &RasterEingabe, bearbeiter: &str, jetzt: NaiveDateTime) -> AbsteckProtokoll {
    let mut p = AbsteckProtokoll {
        protokoll: rtf_protokoll(VORLAGE_RASTERABSTECKUNG, "Rasterabsteckung", Default::default(), punktzeilen(st, punkte), jetzt),
        lageplan: Some(lageplan(st, punkte, &[], &[], None)),
        bild_endung: "_lageplan.png",
    };
    p.protokoll.felder = felder(&[
        ("Bearbeiter", bearbeiter.to_string()),
        ("Zeitpunkt", zeitpunkt_text(jetzt)),
        ("Standpunkt", standpunkt_text(st)),
        ("Ursprung", format!("R={}  H={}", e.r0, e.h0)),
        ("Richtung", format!("{} gon", e.richtung)),
        ("Raster", format!("ΔS={} m  ΔQ={} m  {}×{}", e.d_s, e.d_q, e.zeilen, e.spalten)),
        ("AnzahlPunkte", punkte.len().to_string()),
        ("LageplanHinweis", format!("Lageplan: {}", p.png_name())),
    ]);
    p
}

/// Eingaben der Profilabsteckung für das Protokoll.
pub struct ProfilEingabe<'a> {
    pub r_a: &'a str,
    pub h_a: &'a str,
    pub r_e: &'a str,
    pub h_e: &'a str,
    pub intervall: &'a str,
    pub halbbreite: &'a str,
    pub boeschung: &'a str,
}

/// Protokoll der Profilabsteckung (ohne Lageplan).
pub fn profilabsteckung(st: Option<&StandpunktInfo>, profile: &[ProfilAbsteckPunkt], e: &ProfilEingabe, bearbeiter: &str, jetzt: NaiveDateTime) -> AbsteckProtokoll {
    let zeilen = profile
        .iter()
        .map(|p| {
            let art = match p.delta_h_m {
                Some(d) if d >= 0.0 => "Auftrag",
                Some(_) => "Aushub",
                None => STRICH,
            };
            zeile(&[
                ("PunktNr", p.punkt_nr.clone()),
                ("Station", f(p.station_m, 2)),
                ("H_Plan", f(p.h_plan, 3)),
                ("H_Gel", p.h_gelaende.map_or_else(|| STRICH.to_string(), |h| f(h, 3))),
                ("DeltaH", p.delta_h_m.map_or_else(|| STRICH.to_string(), |d| signiert(d, 3))),
                ("Typ", art.to_string()),
                ("Boesch", p.boesch_links_m.map_or_else(|| STRICH.to_string(), |b| f(b, 2))),
                ("Hz", hz_text(st, p.hz_soll_gon)),
                ("s", s_text(st, p.s_soll_m)),
            ])
        })
        .collect();
    let felder = felder(&[
        ("Bearbeiter", bearbeiter.to_string()),
        ("Zeitpunkt", zeitpunkt_text(jetzt)),
        ("Standpunkt", standpunkt_text(st)),
        ("AchseA", format!("R={}  H={}", e.r_a, e.h_a)),
        ("AchseE", format!("R={}  H={}", e.r_e, e.h_e)),
        ("Intervall", format!("{} m", e.intervall)),
        ("Halbbreite", format!("{} m", e.halbbreite)),
        ("Boesch", format!("1:{}", e.boeschung)),
    ]);
    AbsteckProtokoll {
        protokoll: rtf_protokoll(VORLAGE_PROFILABSTECKUNG, "Profilabsteckung", felder, zeilen, jetzt),
        lageplan: None,
        bild_endung: "",
    }
}

/// Protokoll des Schnurgerüsts mit Skizze (Gebäude rot, Schnurgerüst blau gestrichelt).
pub fn schnurgeruest(
    st: Option<&StandpunktInfo>,
    polygon: &[(f64, f64)],
    sg_punkte: &[AbsteckPunkt],
    abstand: &str,
    bearbeiter: &str,
    jetzt: NaiveDateTime,
) -> AbsteckProtokoll {
    let zeilen = sg_punkte
        .iter()
        .map(|p| {
            zeile(&[
                ("PunktNr", p.punkt_nr.clone()),
                ("R", f(p.r_soll, 3)),
                ("H", f(p.h_soll, 3)),
                ("Hz", hz_text(st, p.hz_soll_gon)),
                ("s", s_text(st, p.s_soll_m)),
            ])
        })
        .collect();
    let mut p = AbsteckProtokoll {
        protokoll: rtf_protokoll(VORLAGE_SCHNURGERUEST, "Schnurgeruest", Default::default(), zeilen, jetzt),
        lageplan: Some(lageplan(st, &[], polygon, sg_punkte, None)),
        bild_endung: "_skizze.png",
    };
    p.protokoll.felder = felder(&[
        ("Bearbeiter", bearbeiter.to_string()),
        ("Zeitpunkt", zeitpunkt_text(jetzt)),
        ("Standpunkt", standpunkt_text(st)),
        ("Abstand", format!("{abstand} m")),
        ("AnzahlEcken", polygon.len().to_string()),
        ("LageplanHinweis", format!("Skizze: {}", p.png_name())),
    ]);
    p
}

/// Zeichnet den Lageplan: Raster, Achse, Gebäudepolygon, Schnurgerüst, Sollpunkte und Standpunkt (ohne Beschriftung).
pub fn lageplan(
    st: Option<&StandpunktInfo>,
    punkte: &[AbsteckPunkt],
    polygon: &[(f64, f64)],
    schnurgeruest: &[AbsteckPunkt],
    achse: Option<((f64, f64), (f64, f64))>,
) -> RgbaImage {
    let mut bild = RgbaImage::from_pixel(BREITE, HOEHE, HINTERGRUND);
    let mut alle: Vec<(f64, f64)> = punkte.iter().chain(schnurgeruest).map(|p| (p.r_soll, p.h_soll)).collect();
    alle.extend(polygon.iter().copied());
    if let Some(s) = st {
        alle.push((s.r, s.h));
    }
    if let Some((a, e)) = achse {
        alle.push(a);
        alle.push(e);
    }
    if let Some(abb) = Abbildung::neu(&alle, f64::from(BREITE), f64::from(HOEHE)) {
        zeichne_raster(&mut bild, &abb);
        if let Some((a, e)) = achse {
            linie(&mut bild, abb.bild(a.0, a.1), abb.bild(e.0, e.1), ACHSE, true);
        }
        let bildpunkte = |liste: &[(f64, f64)]| -> Vec<(f64, f64)> { liste.iter().map(|p| abb.bild(p.0, p.1)).collect() };
        if polygon.len() >= 2 {
            let p = bildpunkte(polygon);
            for i in 0..p.len() {
                linie(&mut bild, p[i], p[(i + 1) % p.len()], POLYGON, false);
            }
        }
        if schnurgeruest.len() >= 2 {
            let p: Vec<(f64, f64)> = schnurgeruest.iter().map(|q| abb.bild(q.r_soll, q.h_soll)).collect();
            for i in 0..p.len() {
                linie(&mut bild, p[i], p[(i + 1) % p.len()], SCHNURGERUEST, true);
            }
        }
        for p in punkte.iter().chain(schnurgeruest) {
            let farbe = if p.ist_abgesteckt() { ABGESTECKT } else { SOLLPUNKT };
            kreis(&mut bild, abb.bild(p.r_soll, p.h_soll), PUNKT_RADIUS, farbe, Rgba([105, 105, 105, 255]));
        }
        if let Some(s) = st {
            kreis(&mut bild, abb.bild(s.r, s.h), STANDPUNKT_RADIUS, STANDPUNKT_FUELLUNG, STANDPUNKT_RAND);
        }
    }
    // Rahmen
    for x in 0..BREITE {
        bild.put_pixel(x, 0, RAHMEN);
        bild.put_pixel(x, HOEHE - 1, RAHMEN);
    }
    for y in 0..HOEHE {
        bild.put_pixel(0, y, RAHMEN);
        bild.put_pixel(BREITE - 1, y, RAHMEN);
    }
    bild
}

/// Gestrichelte Rasterlinien im Abstand von 10 m (bei großen Ausdehnungen 100 m, 1000 m …).
fn zeichne_raster(bild: &mut RgbaImage, abb: &Abbildung) {
    let (min_r, max_r, min_h, max_h) = abb.grenzen();
    let weite = abb.raster_weite();
    let mut r = (min_r / weite).ceil() * weite;
    while r <= max_r {
        linie_punktiert(bild, abb.bild(r, min_h), abb.bild(r, max_h));
        r += weite;
    }
    let mut h = (min_h / weite).ceil() * weite;
    while h <= max_h {
        linie_punktiert(bild, abb.bild(min_r, h), abb.bild(max_r, h));
        h += weite;
    }
}

fn linie_punktiert(bild: &mut RgbaImage, a: (f64, f64), b: (f64, f64)) {
    let laenge = (b.0 - a.0).hypot(b.1 - a.1);
    let schritte = laenge.ceil().max(1.0) as usize;
    for i in (0..=schritte).step_by(3) {
        let t = i as f64 / schritte as f64;
        let (x, y) = (a.0 + t * (b.0 - a.0), a.1 + t * (b.1 - a.1));
        if x >= 0.0 && y >= 0.0 && (x as u32) < bild.width() && (y as u32) < bild.height() {
            bild.put_pixel(x as u32, y as u32, RASTER);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::daten::modelle::jetzt;
    use crate::protokolle::pruefe_vollstaendig;
    use crate::rechnen::absteckung as ab;

    fn standpunkt() -> StandpunktInfo {
        StandpunktInfo { punkt_nr: "S1".into(), r: 100.0, h: 200.0, orientierung_gon: 12.0 }
    }

    fn punkte(st: Option<&StandpunktInfo>) -> Vec<AbsteckPunkt> {
        let mut p = vec![AbsteckPunkt::neu("P1", 110.0, 220.0), AbsteckPunkt::neu("P2", 120.0, 230.0)];
        p[1].status = ab::STATUS_ABGESTECKT.to_string();
        if let Some(st) = st {
            for q in &mut p {
                ab::berechne_polare_absteckung(st, q);
            }
        }
        p
    }

    #[test]
    fn punktprotokoll_ist_vollstaendig() {
        let st = standpunkt();
        let p = punktabsteckung(Some(&st), &punkte(Some(&st)), "Tester", jetzt());
        let rtf = pruefe_vollstaendig(&p.protokoll);
        assert!(rtf.contains("Punktabsteckung"));
        assert_eq!(p.protokoll.tabellen[""].len(), 2);
        assert_eq!(p.protokoll.tabellen[""][1]["Status"], "abgesteckt");
        assert!(p.protokoll.felder["Standpunkt"].starts_with("S1   R=100.000   H=200.000   z=12.0000 gon"));
        assert!(p.protokoll.felder["LageplanHinweis"].ends_with("_lageplan.png"));
        // Ohne Standpunkt: Strich statt Werte
        let ohne = punktabsteckung(None, &punkte(None), "Tester", jetzt());
        assert_eq!(ohne.protokoll.tabellen[""][0]["Hz"], "-");
        assert_eq!(ohne.protokoll.felder["Standpunkt"], "–");
    }

    #[test]
    fn achsprotokoll_ist_vollstaendig() {
        let st = standpunkt();
        let achse = ((0.0, 0.0), (0.0, 20.0));
        let pkt = ab::berechne_achspunkte(achse.0, achse.1, 10.0, &[0.0, 1.0], Some(&st));
        let e = AchsEingabe { r_a: "0", h_a: "0", r_e: "0", h_e: "20", intervall: "10", offsets: "0, 1" };
        let p = achsabsteckung(Some(&st), &pkt, Some(achse), &e, "Tester", jetzt());
        pruefe_vollstaendig(&p.protokoll);
        assert_eq!(p.protokoll.tabellen[""].len(), pkt.len());
        assert_eq!(p.protokoll.tabellen[""][0]["Station"], "0+000.0");
        assert_eq!(p.protokoll.felder["Intervall"], "10 m");
    }

    #[test]
    fn rasterprotokoll_ist_vollstaendig() {
        let pkt = ab::berechne_raster(0.0, 0.0, 0.0, 10.0, 5.0, 2, 2, None);
        let e = RasterEingabe { r0: "0", h0: "0", richtung: "0", d_s: "10", d_q: "5", zeilen: "2", spalten: "2" };
        let p = rasterabsteckung(None, &pkt, &e, "Tester", jetzt());
        pruefe_vollstaendig(&p.protokoll);
        assert_eq!(p.protokoll.felder["Raster"], "ΔS=10 m  ΔQ=5 m  2×2");
    }

    #[test]
    fn profilprotokoll_ist_vollstaendig_und_ohne_bild() {
        let st = standpunkt();
        let mut prof = ab::berechne_profilpunkte((0.0, 0.0), (0.0, 20.0), 10.0, &[100.0, 101.0, 102.0], Some(&st));
        ab::aktualisiere_profilpunkt(&mut prof[0], 99.5, 3.0, 1.5);
        ab::aktualisiere_profilpunkt(&mut prof[1], 102.0, 3.0, 1.5);
        let e = ProfilEingabe { r_a: "0", h_a: "0", r_e: "0", h_e: "20", intervall: "10", halbbreite: "3", boeschung: "1.5" };
        let p = profilabsteckung(Some(&st), &prof, &e, "Tester", jetzt());
        pruefe_vollstaendig(&p.protokoll);
        assert!(p.lageplan.is_none());
        let z = &p.protokoll.tabellen[""];
        assert_eq!(z[0]["DeltaH"], "+0.500");
        assert_eq!(z[0]["Typ"], "Auftrag");
        assert_eq!(z[1]["DeltaH"], "-1.000");
        assert_eq!(z[1]["Typ"], "Aushub");
        assert_eq!(z[2]["DeltaH"], "-");
        assert_eq!(z[2]["Boesch"], "-");
        assert_eq!(p.protokoll.felder["Boesch"], "1:1.5");
    }

    #[test]
    fn schnurgeruestprotokoll_hat_skizze() {
        let st = standpunkt();
        let poly = [(0.0, 0.0), (10.0, 0.0), (10.0, 8.0), (0.0, 8.0)];
        let sg = ab::berechne_schnurgeruest(&poly, 2.0, Some(&st));
        let p = schnurgeruest(Some(&st), &poly, &sg, "2.0", "Tester", jetzt());
        pruefe_vollstaendig(&p.protokoll);
        assert!(p.protokoll.felder["LageplanHinweis"].ends_with("_skizze.png"));
        assert_eq!(p.protokoll.felder["AnzahlEcken"], "4");
        let bild = p.lageplan.as_ref().unwrap();
        assert!(bild.pixels().any(|px| *px == POLYGON), "Gebäude gezeichnet");
        assert!(bild.pixels().any(|px| *px == SCHNURGERUEST), "Schnurgerüst gezeichnet");
    }

    #[test]
    fn lageplan_zeigt_punkte_standpunkt_und_achse() {
        let st = standpunkt();
        let bild = lageplan(Some(&st), &punkte(Some(&st)), &[], &[], Some(((100.0, 200.0), (120.0, 230.0))));
        assert_eq!(bild.dimensions(), (BREITE, HOEHE));
        for farbe in [SOLLPUNKT, ABGESTECKT, STANDPUNKT_FUELLUNG, ACHSE] {
            assert!(bild.pixels().any(|px| *px == farbe), "Farbe {farbe:?} fehlt");
        }
        // Leerer Plan besteht nur aus Hintergrund und Rahmen
        let leer = lageplan(None, &[], &[], &[], None);
        assert!(leer.pixels().all(|px| *px == HINTERGRUND || *px == RAHMEN));
    }

    #[test]
    fn schreibt_rtf_und_bild() {
        let dir = std::env::temp_dir().join(format!("fb-abst-prot-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let st = standpunkt();
        let p = punktabsteckung(Some(&st), &punkte(Some(&st)), "Tester", jetzt());
        let (rtf, png) = p.schreibe(&dir).unwrap();
        let png = png.unwrap();
        assert!(rtf.exists() && png.exists());
        assert!(png.to_string_lossy().ends_with("_lageplan.png"));
        let e = ProfilEingabe { r_a: "0", h_a: "0", r_e: "0", h_e: "20", intervall: "10", halbbreite: "3", boeschung: "1.5" };
        let (rtf, png) = profilabsteckung(None, &[], &e, "Tester", jetzt()).schreibe(&dir).unwrap();
        assert!(rtf.exists() && png.is_none());
        let _ = std::fs::remove_dir_all(dir);
    }
}
