// Zweck:    Flächenteilung (Abspaltung) nach sechs Verfahren: Parallele zur Grundseite, durch festen Punkt,
//           gegebene Richtung, Dreiecksabtrennung, Verhältnisteilung, Streifenabtrennung
// Hardware: keine
// Autor:    Johann Gerner

use std::f64::consts::PI;

/// Gon → Radiant.
const GON_ZU_RAD: f64 = PI / 200.0;
/// Radiant → Gon.
const RAD_ZU_GON: f64 = 200.0 / PI;
/// Vollkreis in gon.
const VOLLKREIS_GON: f64 = 400.0;
/// Kleinste zulässige Sollfläche bzw. Restfläche [m²].
const MIN_FLAECHE: f64 = 1e-4;
/// Mindestfläche eines gültigen Polygons [m²].
const MIN_POLYGON_FLAECHE: f64 = 1e-6;
/// Abbruchgrenze der Flächenabweichung [m²].
const FLAECHEN_TOLERANZ: f64 = 1e-4;
/// Höchstzahl der Iterationen.
const MAX_ITERATIONEN: usize = 60;
/// Determinantenschwelle für parallele Linien.
const PARALLEL_SCHWELLE: f64 = 1e-12;
/// Toleranz am Kantenanfang und -ende.
const KANTEN_TOLERANZ: f64 = 1e-9;
/// Kürzeste Kante, deren Normale noch bestimmt wird [m].
const MIN_KANTE_M: f64 = 1e-9;
/// Sicherheitszuschlag auf den größten Abstand der Eckpunkte zur Grundseite.
const ABSTAND_ZUSCHLAG: f64 = 1.001;
/// Winkelabstand, unter dem zwei kritische Richtungen als gleich gelten [gon].
const MIN_WINKEL_GON: f64 = 1e-6;
/// Stützstellen je Intervall zwischen zwei kritischen Richtungen (Teilung durch festen Punkt).
const UNTERTEILUNG: usize = 32;
/// Größte Flächenabweichung, die ein gefundenes Ergebnis noch haben darf [m²].
const ANNAHME_TOLERANZ: f64 = 1e-3;
/// Schwelle der Dreiecksseite.
const MIN_DETERMINANTE: f64 = 1e-12;

/// Punkt (Rechtswert, Hochwert).
pub type Punkt = (f64, f64);

/// Teilungsverfahren.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verfahren {
    ParalleleZurGrundseite = 1,
    DurchFestenPunkt = 2,
    GegebeneRichtung = 3,
    Dreiecksabtrennung = 4,
    Verhaeltnisteilung = 5,
    Streifenabtrennung = 6,
}

/// Verfahren, auf dem die Verhältnisteilung beruht.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Basis {
    Parallele { grundseite: usize },
    FesterPunkt { punkt: Punkt },
    Richtung { gon: f64 },
}

/// Ergebnis einer Teilung.
#[derive(Clone, Debug, PartialEq)]
pub struct Ergebnis {
    pub a_gesamt: f64,
    pub a1_soll: f64,
    pub a1_ist: f64,
    pub a2: f64,
    pub polygon1: Vec<Punkt>,
    pub polygon2: Vec<Punkt>,
    pub neue_grenzpunkte: Vec<(String, f64, f64)>,
    pub iterationen: usize,
    pub verfahren: String,
}

impl Ergebnis {
    /// Abweichung der erreichten von der gewünschten Fläche.
    pub fn differenz(&self) -> f64 {
        self.a1_ist - self.a1_soll
    }
}

/// Gaußsche Flächenformel.
pub fn flaeche(poly: &[Punkt]) -> f64 {
    let n = poly.len();
    if n < 3 {
        return 0.0;
    }
    let summe: f64 = (0..n)
        .map(|i| {
            let (r1, h1) = poly[i];
            let (r2, h2) = poly[(i + 1) % n];
            r1 * h2 - r2 * h1
        })
        .sum();
    summe.abs() * 0.5
}

/// Mittelwert der Eckpunkte.
pub fn schwerpunkt(poly: &[Punkt]) -> Punkt {
    if poly.is_empty() {
        return (0.0, 0.0);
    }
    let n = poly.len() as f64;
    (poly.iter().map(|p| p.0).sum::<f64>() / n, poly.iter().map(|p| p.1).sum::<f64>() / n)
}

/// Schnittpunkt der Linie `p + t·u` mit der Kante A→B (Kantenparameter s im Bereich [0, 1)).
fn linie_kante(linie_p: Punkt, linie_u: Punkt, a: Punkt, b: Punkt) -> Option<(f64, Punkt)> {
    let (d_r, d_h) = (b.0 - a.0, b.1 - a.1);
    let det = linie_u.0 * (-d_h) - linie_u.1 * (-d_r);
    if det.abs() < PARALLEL_SCHWELLE {
        return None;
    }
    let (dx, dy) = (a.0 - linie_p.0, a.1 - linie_p.1);
    let s = (linie_u.0 * dy - linie_u.1 * dx) / det;
    if s < -KANTEN_TOLERANZ || s >= 1.0 - KANTEN_TOLERANZ {
        return None;
    }
    let s = s.clamp(0.0, 1.0 - 1e-12);
    Some((s, (a.0 + s * d_r, a.1 + s * d_h)))
}

/// Schnitt einer Linie mit einer Polygonkante.
#[derive(Clone, Copy)]
struct Treffer {
    kante: usize,
    punkt: Punkt,
}

/// Alle Schnittpunkte Linie × Polygon, sortiert entlang der Linie.
fn schnittpunkte(poly: &[Punkt], linie_p: Punkt, linie_u: Punkt) -> Vec<Treffer> {
    let n = poly.len();
    let mut treffer: Vec<Treffer> = (0..n)
        .filter_map(|i| linie_kante(linie_p, linie_u, poly[i], poly[(i + 1) % n]).map(|(_, punkt)| Treffer { kante: i, punkt }))
        .collect();
    let laenge = linie_u.0.hypot(linie_u.1);
    let t_von = |p: Punkt| {
        if laenge < PARALLEL_SCHWELLE {
            0.0
        } else {
            ((p.0 - linie_p.0) * linie_u.0 + (p.1 - linie_p.1) * linie_u.1) / laenge
        }
    };
    treffer.sort_by(|a, b| t_von(a.punkt).total_cmp(&t_von(b.punkt)));
    treffer
}

/// Teilt das Polygon an zwei Grenzpunkten: Q1 → vorwärts → Q2 und Q2 → vorwärts → Q1.
fn teile_polygon(poly: &[Punkt], e1: usize, q1: Punkt, e2: usize, q2: Punkt) -> (Vec<Punkt>, Vec<Punkt>) {
    let n = poly.len();
    let teil = |start: Punkt, von: usize, bis: usize, ende: Punkt| {
        let mut p = vec![start];
        let mut i = (von + 1) % n;
        for _ in 0..=n {
            p.push(poly[i]);
            if i == bis {
                break;
            }
            i = (i + 1) % n;
        }
        p.push(ende);
        p
    };
    (teil(q1, e1, e2, q2), teil(q2, e2, e1, q1))
}

/// Ordnet die Teilpolygone: zuerst das mit der Fläche näher an der Sollfläche.
fn nach_flaeche(p1: Vec<Punkt>, p2: Vec<Punkt>, a1_soll: f64) -> (Vec<Punkt>, Vec<Punkt>) {
    let (a1, a2) = (flaeche(&p1), flaeche(&p2));
    if (a1 - a1_soll).abs() <= (a2 - a1_soll).abs() { (p1, p2) } else { (p2, p1) }
}

/// Einwärts gerichtete Normale einer Kante.
fn innen_normale(poly: &[Punkt], kante: usize) -> Punkt {
    let n = poly.len();
    let (ra, ha) = poly[kante];
    let (rb, hb) = poly[(kante + 1) % n];
    let (mut ex, mut ey) = (rb - ra, hb - ha);
    let laenge = ex.hypot(ey);
    if laenge < MIN_KANTE_M {
        return (0.0, 1.0);
    }
    ex /= laenge;
    ey /= laenge;
    let (nx, ny) = (-ey, ex);
    let mittel: f64 = poly.iter().map(|p| (p.0 - ra) * nx + (p.1 - ha) * ny).sum::<f64>() / n as f64;
    if mittel >= 0.0 { (nx, ny) } else { (-nx, -ny) }
}

fn begrenze_soll(a_ges: f64, a1_soll: f64) -> f64 {
    a1_soll.clamp(MIN_FLAECHE, (a_ges - MIN_FLAECHE).max(MIN_FLAECHE))
}

/// Teilung an der Linie durch `linie_p` mit Richtung `linie_u`: erster und letzter Schnittpunkt.
struct Teilung {
    p1: Vec<Punkt>,
    p2: Vec<Punkt>,
    e1: usize,
    q1: Punkt,
    e2: usize,
    q2: Punkt,
}

fn teile_an_linie(poly: &[Punkt], linie_p: Punkt, linie_u: Punkt) -> Option<Teilung> {
    let treffer = schnittpunkte(poly, linie_p, linie_u);
    if treffer.len() < 2 {
        return None;
    }
    let (h1, h2) = (treffer[0], treffer[treffer.len() - 1]);
    let (p1, p2) = teile_polygon(poly, h1.kante, h1.punkt, h2.kante, h2.punkt);
    Some(Teilung { p1, p2, e1: h1.kante, q1: h1.punkt, e2: h2.kante, q2: h2.punkt })
}

fn baue_ergebnis(a_ges: f64, a1_soll: f64, teilung: &Teilung, iterationen: usize, verfahren: String) -> Ergebnis {
    let (a1_poly, a2_poly) = nach_flaeche(teilung.p1.clone(), teilung.p2.clone(), a1_soll);
    Ergebnis {
        a_gesamt: a_ges,
        a1_soll,
        a1_ist: flaeche(&a1_poly),
        a2: flaeche(&a2_poly),
        polygon1: a1_poly,
        polygon2: a2_poly,
        neue_grenzpunkte: vec![("GP1".to_string(), teilung.q1.0, teilung.q1.1), ("GP2".to_string(), teilung.q2.0, teilung.q2.1)],
        iterationen,
        verfahren,
    }
}

fn pruefe_polygon(poly: &[Punkt]) -> Result<f64, String> {
    if poly.len() < 3 {
        return Err("Das Polygon braucht mindestens 3 Eckpunkte.".to_string());
    }
    let a = flaeche(poly);
    if a < MIN_POLYGON_FLAECHE {
        return Err("Polygon hat keine Fläche.".to_string());
    }
    Ok(a)
}

const KEINE_TRENNLINIE: &str = "Keine gültige Trennlinie gefunden.";

/// Verfahren 1: Parallele zur Grundseite (Kante `grundseite` → Kante + 1).
pub fn parallele(poly: &[Punkt], grundseite: usize, a1_soll: f64) -> Result<Ergebnis, String> {
    let a_ges = pruefe_polygon(poly)?;
    if grundseite >= poly.len() {
        return Err("Die Grundseite liegt außerhalb des Polygons.".to_string());
    }
    let a1_soll = begrenze_soll(a_ges, a1_soll);
    let (ra, ha) = poly[grundseite];
    let (nx, ny) = innen_normale(poly, grundseite);
    let linie_u = (-ny, nx);
    let d_max = poly.iter().map(|p| (p.0 - ra) * nx + (p.1 - ha) * ny).fold(f64::MIN, f64::max) * ABSTAND_ZUSCHLAG;

    let berechne = |d: f64| teile_an_linie(poly, (ra + d * nx, ha + d * ny), linie_u);
    let (mut lo, mut hi) = (0.0, d_max);
    let mut iterationen = 0;
    while iterationen < MAX_ITERATIONEN {
        let mitte = (lo + hi) * 0.5;
        if let Some(t) = berechne(mitte).filter(|t| t.p1.len() >= 3) {
            let (a1_poly, _) = nach_flaeche(t.p1, t.p2, a1_soll);
            let a = flaeche(&a1_poly);
            if (a - a1_soll).abs() < FLAECHEN_TOLERANZ {
                lo = mitte;
                hi = mitte;
                break;
            }
            if a < a1_soll { lo = mitte } else { hi = mitte }
        }
        iterationen += 1;
    }
    let teilung = berechne((lo + hi) * 0.5).ok_or(KEINE_TRENNLINIE)?;
    Ok(baue_ergebnis(a_ges, a1_soll, &teilung, iterationen, "Parallele zur Grundseite".to_string()))
}

/// Verfahren 2: Teilungslinie durch einen festen Punkt.
pub fn fester_punkt(poly: &[Punkt], fest: Punkt, a1_soll: f64) -> Result<Ergebnis, String> {
    let a_ges = pruefe_polygon(poly)?;
    let a1_soll = begrenze_soll(a_ges, a1_soll);

    // Kritische Richtungen: zu allen Eckpunkten
    let mut winkel: Vec<f64> = poly.iter().map(|p| ((p.0 - fest.0).atan2(p.1 - fest.1) * RAD_ZU_GON).rem_euclid(VOLLKREIS_GON)).collect();
    winkel.sort_by(f64::total_cmp);
    winkel.push(winkel[0] + VOLLKREIS_GON);

    let berechne = |t_gon: f64| -> (f64, Option<Teilung>) {
        let u = ((t_gon * GON_ZU_RAD).sin(), (t_gon * GON_ZU_RAD).cos());
        match teile_an_linie(poly, fest, u) {
            Some(t) => {
                let (a1_poly, _) = nach_flaeche(t.p1.clone(), t.p2.clone(), a1_soll);
                (flaeche(&a1_poly), Some(t))
            }
            None => (0.0, None),
        }
    };
    let abweichung = |t: f64| berechne(t).0 - a1_soll;

    // Stichproben zwischen den kritischen Richtungen (die Fläche kann dazwischen ein Maximum haben)
    let mut stuetzen: Vec<f64> = Vec::new();
    for paar in winkel.windows(2) {
        if (paar[1] - paar[0]).abs() < MIN_WINKEL_GON {
            continue;
        }
        for k in 0..UNTERTEILUNG {
            stuetzen.push(paar[0] + (paar[1] - paar[0]) * k as f64 / UNTERTEILUNG as f64);
        }
    }
    if let Some(letzte) = winkel.last() {
        stuetzen.push(*letzte);
    }

    // Vorzeichenwechsel der Abweichung eingrenzen und durch Halbieren verfeinern
    for paar in stuetzen.windows(2) {
        let (mut lo, mut hi) = (paar[0], paar[1]);
        let (mut f_lo, f_hi) = (abweichung(lo), abweichung(hi));
        if f_lo * f_hi > 0.0 {
            continue;
        }
        let mut iterationen = 0;
        while iterationen < MAX_ITERATIONEN {
            let mitte = (lo + hi) * 0.5;
            let f_mitte = abweichung(mitte);
            if f_mitte.abs() < FLAECHEN_TOLERANZ {
                lo = mitte;
                hi = mitte;
                break;
            }
            if f_lo * f_mitte <= 0.0 {
                hi = mitte;
            } else {
                lo = mitte;
                f_lo = f_mitte;
            }
            iterationen += 1;
        }
        let bester = (lo + hi) * 0.5;
        if abweichung(bester).abs() < ANNAHME_TOLERANZ {
            if let Some(teilung) = berechne(bester).1 {
                return Ok(baue_ergebnis(a_ges, a1_soll, &teilung, iterationen, "Durch festen Punkt".to_string()));
            }
        }
    }
    Err("Die gewünschte Fläche ist mit einer Teilungslinie durch diesen Punkt nicht erreichbar.".to_string())
}

/// Verfahren 3: Teilungslinie in gegebener Richtung.
pub fn gegebene_richtung(poly: &[Punkt], richtung_gon: f64, a1_soll: f64) -> Result<Ergebnis, String> {
    let a_ges = pruefe_polygon(poly)?;
    let a1_soll = begrenze_soll(a_ges, a1_soll);
    let u = ((richtung_gon * GON_ZU_RAD).sin(), (richtung_gon * GON_ZU_RAD).cos());
    // Querrichtung: 90° rechts
    let n = (u.1, -u.0);
    let abstaende = poly.iter().map(|p| p.0 * n.0 + p.1 * n.1);
    let mut lo = abstaende.clone().fold(f64::MAX, f64::min);
    let mut hi = abstaende.fold(f64::MIN, f64::max);

    let mut letzte: Option<Teilung> = None;
    let mut iterationen = 0;
    while iterationen < MAX_ITERATIONEN {
        let mitte = (lo + hi) * 0.5;
        match teile_an_linie(poly, (n.0 * mitte, n.1 * mitte), u) {
            None => lo = (lo + mitte) * 0.5,
            Some(t) => {
                let (a1_poly, _) = nach_flaeche(t.p1.clone(), t.p2.clone(), a1_soll);
                let a = flaeche(&a1_poly);
                letzte = Some(t);
                if (a - a1_soll).abs() < FLAECHEN_TOLERANZ {
                    break;
                }
                if a < a1_soll { lo = mitte } else { hi = mitte }
            }
        }
        iterationen += 1;
    }
    let teilung = letzte.ok_or(KEINE_TRENNLINIE)?;
    Ok(baue_ergebnis(a_ges, a1_soll, &teilung, iterationen, format!("Gegebene Richtung {:.4} gon", richtung_gon)))
}

/// Verfahren 4: Dreiecksabtrennung vom Eckpunkt `ecke`.
pub fn dreieck(poly: &[Punkt], ecke: usize, a1_soll: f64) -> Result<Ergebnis, String> {
    let a_ges = pruefe_polygon(poly)?;
    if ecke >= poly.len() {
        return Err("Der Eckpunkt liegt außerhalb des Polygons.".to_string());
    }
    let a1_soll = begrenze_soll(a_ges, a1_soll);
    let n = poly.len();
    let pk = poly[ecke];

    let mut summe = 0.0;
    let mut ziel: Option<(usize, f64)> = None;
    for schritt in 1..n - 1 {
        let (j, j1) = ((ecke + schritt) % n, (ecke + schritt + 1) % n);
        let (pj, pj1) = (poly[j], poly[j1]);
        let dreieck = ((pj.0 - pk.0) * (pj1.1 - pk.1) - (pj.1 - pk.1) * (pj1.0 - pk.0)).abs() * 0.5;
        if summe + dreieck >= a1_soll - KANTEN_TOLERANZ {
            let d_j = ((pj.0 - pk.0) * (pj1.1 - pj.1) - (pj.1 - pk.1) * (pj1.0 - pj.0)).abs().max(MIN_DETERMINANTE);
            ziel = Some((j, ((a1_soll - summe) * 2.0 / d_j).clamp(0.0, 1.0)));
            break;
        }
        summe += dreieck;
    }
    let (kante, t_q) = ziel.ok_or(KEINE_TRENNLINIE)?;
    let (pte, pte1) = (poly[kante], poly[(kante + 1) % n]);
    let q = (pte.0 + t_q * (pte1.0 - pte.0), pte.1 + t_q * (pte1.1 - pte.1));

    let mut poly1 = vec![pk];
    let mut i = (ecke + 1) % n;
    while i != kante {
        poly1.push(poly[i]);
        i = (i + 1) % n;
    }
    poly1.push(pte);
    poly1.push(q);

    let mut poly2 = vec![q];
    let mut i = (kante + 1) % n;
    while i != ecke {
        poly2.push(poly[i]);
        i = (i + 1) % n;
    }
    poly2.push(pk);

    Ok(Ergebnis {
        a_gesamt: a_ges,
        a1_soll,
        a1_ist: flaeche(&poly1),
        a2: flaeche(&poly2),
        polygon1: poly1,
        polygon2: poly2,
        neue_grenzpunkte: vec![("GP1".to_string(), q.0, q.1)],
        iterationen: 0,
        verfahren: format!("Dreiecksabtrennung von Eckpunkt {}", ecke + 1),
    })
}

/// Verfahren 5: Teilung im Verhältnis a : b nach einem der Grundverfahren.
pub fn verhaeltnis(poly: &[Punkt], a: f64, b: f64, basis: Basis) -> Result<Ergebnis, String> {
    if a + b <= 0.0 || a < 0.0 || b < 0.0 {
        return Err("Das Verhältnis muss positive Zahlen enthalten.".to_string());
    }
    let a_ges = pruefe_polygon(poly)?;
    let a1_soll = a_ges * a / (a + b);
    let mut erg = match basis {
        Basis::FesterPunkt { punkt } => fester_punkt(poly, punkt, a1_soll)?,
        Basis::Richtung { gon } => gegebene_richtung(poly, gon, a1_soll)?,
        Basis::Parallele { grundseite } => parallele(poly, grundseite, a1_soll)?,
    };
    erg.verfahren = format!("Verhältnis {a}:{b} – {}", erg.verfahren);
    Ok(erg)
}

/// Verfahren 6: Streifen der Breite `breite_m` entlang der Grundseite.
pub fn streifen(poly: &[Punkt], grundseite: usize, breite_m: f64) -> Result<Ergebnis, String> {
    let a_ges = pruefe_polygon(poly)?;
    if grundseite >= poly.len() {
        return Err("Die Grundseite liegt außerhalb des Polygons.".to_string());
    }
    let (ra, ha) = poly[grundseite];
    let (nx, ny) = innen_normale(poly, grundseite);
    let linie_u = (-ny, nx);
    let teilung = teile_an_linie(poly, (ra + breite_m * nx, ha + breite_m * ny), linie_u).ok_or("Breite zu groß für dieses Polygon.")?;
    let (a1, a2) = (flaeche(&teilung.p1), flaeche(&teilung.p2));
    let (poly1, poly2) = if a1 <= a2 { (teilung.p1.clone(), teilung.p2.clone()) } else { (teilung.p2.clone(), teilung.p1.clone()) };
    let a1_ist = flaeche(&poly1);
    Ok(Ergebnis {
        a_gesamt: a_ges,
        a1_soll: a1_ist,
        a1_ist,
        a2: flaeche(&poly2),
        polygon1: poly1,
        polygon2: poly2,
        neue_grenzpunkte: vec![("GP1".to_string(), teilung.q1.0, teilung.q1.1), ("GP2".to_string(), teilung.q2.0, teilung.q2.1)],
        iterationen: 0,
        verfahren: format!("Streifenabtrennung b={:.3} m", breite_m),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn quadrat() -> Vec<Punkt> {
        vec![(0.0, 0.0), (100.0, 0.0), (100.0, 100.0), (0.0, 100.0)]
    }

    fn nahe(a: f64, b: f64, eps: f64) -> bool {
        (a - b).abs() < eps
    }

    #[test]
    fn flaeche_und_schwerpunkt() {
        assert!(nahe(flaeche(&quadrat()), 10_000.0, 1e-9));
        assert_eq!(flaeche(&quadrat()[..2]), 0.0);
        assert_eq!(schwerpunkt(&quadrat()), (50.0, 50.0));
        assert_eq!(schwerpunkt(&[]), (0.0, 0.0));
    }

    #[test]
    fn parallele_zur_grundseite() {
        let e = parallele(&quadrat(), 0, 2500.0).unwrap();
        assert!(nahe(e.a1_ist, 2500.0, 1e-3), "A1={}", e.a1_ist);
        assert!(nahe(e.a1_ist + e.a2, 10_000.0, 1e-6));
        assert_eq!(e.neue_grenzpunkte.len(), 2);
        for (_, _, h) in &e.neue_grenzpunkte {
            assert!(nahe(*h, 25.0, 1e-2), "H={h}");
        }
        assert!(e.differenz().abs() < 1e-3);
    }

    #[test]
    fn durch_festen_punkt() {
        let e = fester_punkt(&quadrat(), (20.0, 50.0), 4000.0).unwrap();
        assert!(nahe(e.a1_ist, 4000.0, 1e-2), "A1={}", e.a1_ist);
        assert!(nahe(e.a1_ist + e.a2, 10_000.0, 1e-6));
        // Hälfte: jede Gerade durch den Mittelpunkt teilt das Quadrat zur Hälfte
        let halb = fester_punkt(&quadrat(), (50.0, 50.0), 5000.0).unwrap();
        assert!(nahe(halb.a1_ist, 5000.0, 1e-2));
    }

    #[test]
    fn fester_punkt_unerreichbare_flaeche() {
        // Durch einen Punkt nahe der Ecke lässt sich keine kleine Fläche abtrennen, die kleiner als das Minimum wäre
        assert!(fester_punkt(&quadrat(), (50.0, 50.0), 100.0).is_err());
    }

    #[test]
    fn gegebene_richtung_nordwaerts() {
        let e = gegebene_richtung(&quadrat(), 0.0, 3000.0).unwrap();
        assert!(nahe(e.a1_ist, 3000.0, 1e-2), "A1={}", e.a1_ist);
        for (_, r, _) in &e.neue_grenzpunkte {
            assert!(nahe(*r, 30.0, 1e-2) || nahe(*r, 70.0, 1e-2), "R={r}");
        }
    }

    #[test]
    fn dreiecksabtrennung() {
        let e = dreieck(&quadrat(), 0, 2500.0).unwrap();
        assert!(nahe(e.a1_ist, 2500.0, 1e-9));
        assert_eq!(e.neue_grenzpunkte.len(), 1);
        assert!(nahe(e.neue_grenzpunkte[0].1, 100.0, 1e-9) && nahe(e.neue_grenzpunkte[0].2, 50.0, 1e-9));
        assert!(nahe(e.a1_ist + e.a2, 10_000.0, 1e-9));
    }

    #[test]
    fn verhaeltnisteilung() {
        let e = verhaeltnis(&quadrat(), 1.0, 3.0, Basis::Parallele { grundseite: 0 }).unwrap();
        assert!(nahe(e.a1_ist, 2500.0, 1e-3));
        assert!(e.verfahren.starts_with("Verhältnis 1:3 – "));
        assert!(verhaeltnis(&quadrat(), 0.0, 0.0, Basis::Richtung { gon: 0.0 }).is_err());
    }

    #[test]
    fn streifenabtrennung() {
        let e = streifen(&quadrat(), 0, 10.0).unwrap();
        assert!(nahe(e.a1_ist, 1000.0, 1e-6));
        assert!(nahe(e.a2, 9000.0, 1e-6));
        assert!(streifen(&quadrat(), 0, 500.0).is_err());
    }

    #[test]
    fn ungueltige_polygone() {
        assert!(parallele(&quadrat()[..2], 0, 1.0).is_err());
        assert!(parallele(&[(0.0, 0.0), (1.0, 1.0), (2.0, 2.0)], 0, 1.0).is_err());
        assert!(dreieck(&quadrat(), 9, 100.0).is_err());
    }
}
