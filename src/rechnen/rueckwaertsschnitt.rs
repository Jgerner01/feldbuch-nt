// Zweck:    Rückwärtsschnitt: unbekannter Standpunkt aus mindestens drei gemessenen Horizontalrichtungen
//           zu bekannten Anschlusspunkten (Ausgleichung nach vermittelnden Beobachtungen)
// Hardware: keine
// Autor:    Johann Gerner

use std::f64::consts::PI;

use crate::rechnen::gauss;

/// Gon → Radiant.
const GON_ZU_RAD: f64 = PI / 200.0;
/// Radiant → Gon.
const RAD_ZU_GON: f64 = 200.0 / PI;
/// Zentesimalsekunden (cc) → Radiant.
const CC_ZU_RAD: f64 = PI / 2_000_000.0;
/// Radiant → cc.
const RAD_ZU_CC: f64 = 2_000_000.0 / PI;
/// Vollkreis in gon.
const VOLLKREIS_GON: f64 = 400.0;
/// Höchstzahl der Iterationen.
const MAX_ITERATIONEN: usize = 50;
/// Mindestanzahl aktiver Anschlusspunkte.
pub const MIN_PUNKTE: usize = 3;
/// Streckenschwelle: kürzere Strecken werden ignoriert [m].
const MIN_STRECKE_M: f64 = 1e-6;
/// Konvergenzgrenzen: Lage [m] und Orientierung [rad].
const KONV_LAGE_M: f64 = 1e-7;
const KONV_ORIENT_RAD: f64 = 1e-9;
/// Schwelle für den kritischen Kreis: Anteil des Umkreisradius.
const KRITISCH_ANTEIL: f64 = 0.02;
/// Untergrenze des Umkreisradius beim Vergleich [m].
const MIN_UMKREIS_M: f64 = 1.0;
/// Determinantenschwelle: darunter gelten drei Punkte als kollinear.
const KOLLINEAR_SCHWELLE: f64 = 1e-10;
/// Standard-Richtungsgenauigkeit [cc] und Standardabweichung der Gewichtseinheit [mm].
pub const STANDARD_RICHTUNG_CC: f64 = 9.0;
pub const STANDARD_M0_MM: f64 = 1.0;

/// Anschlusspunkt mit gemessener Richtung.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RueckwaertsPunkt {
    pub punkt_nr: String,
    pub r: f64,
    pub h: f64,
    /// Horizontalrichtung [gon].
    pub hz: f64,
}

/// Residuum je Anschlusspunkt.
#[derive(Clone, Debug, PartialEq)]
pub struct RueckwaertsResiduum {
    pub punkt_nr: String,
    /// Horizontalstrecke zum Anschlusspunkt [m].
    pub strecke_h: f64,
    /// Richtungsresiduum [cc].
    pub v_winkel_cc: f64,
    pub aktiv: bool,
}

/// Ergebnis des Rückwärtsschnitts.
#[derive(Clone, Debug, PartialEq)]
pub struct RueckwaertsErgebnis {
    pub r: f64,
    pub h: f64,
    pub orientierung_gon: f64,
    pub s0_mm: f64,
    pub redundanz: i32,
    pub iterationen: usize,
    pub konvergiert: bool,
    /// Leer = kein Problem, sonst Geometriewarnung.
    pub kritischer_kreis: String,
    pub residuen: Vec<RueckwaertsResiduum>,
}

/// Normiert einen Winkel in den Bereich −π … +π.
fn norm_winkel(a: f64) -> f64 {
    let mut w = a % (2.0 * PI);
    if w > PI {
        w -= 2.0 * PI;
    } else if w < -PI {
        w += 2.0 * PI;
    }
    w
}

/// Berechnet den Standpunkt.
///
/// `aktiv`: Auswahl der verwendeten Punkte (`None` = alle). `m_r_cc`: Richtungsgenauigkeit [cc],
/// `m0_mm`: Gewichtseinheit [mm].
pub fn berechnen(punkte: &[RueckwaertsPunkt], aktiv: Option<&[bool]>, m_r_cc: f64, m0_mm: f64) -> Result<RueckwaertsErgebnis, String> {
    let n = punkte.len();
    if n < MIN_PUNKTE {
        return Err(format!("Mindestens {MIN_PUNKTE} Anschlusspunkte erforderlich."));
    }
    let akt: Vec<bool> = aktiv.map_or_else(|| vec![true; n], <[bool]>::to_vec);
    if akt.len() != n {
        return Err("Die Aktiv-Auswahl passt nicht zur Punktanzahl.".to_string());
    }
    let n_akt = akt.iter().filter(|a| **a).count();
    if n_akt < MIN_PUNKTE {
        return Err(format!("Mindestens {MIN_PUNKTE} aktive Anschlusspunkte erforderlich (aktiv: {n_akt})."));
    }
    let hz: Vec<f64> = punkte.iter().map(|p| p.hz * GON_ZU_RAD).collect();
    let aktive = || punkte.iter().enumerate().filter(|(i, _)| akt[*i]);

    // Näherung: Schwerpunkt der Anschlusspunkte, mittlere Orientierung
    let mut r_p = aktive().map(|(_, p)| p.r).sum::<f64>() / n_akt as f64;
    let mut h_p = aktive().map(|(_, p)| p.h).sum::<f64>() / n_akt as f64;
    let summe_z: f64 = aktive().map(|(i, p)| norm_winkel((p.r - r_p).atan2(p.h - h_p) - hz[i])).sum();
    let mut z = norm_winkel(summe_z / n_akt as f64);

    let m0_m = m0_mm / 1000.0;
    let m_r_rad = m_r_cc * CC_ZU_RAD;
    let gewicht = |s: f64| {
        let sigma = m_r_rad * s;
        (m0_m * m0_m) / (sigma * sigma)
    };

    let mut iterationen = 0;
    let mut konvergiert = false;
    while iterationen < MAX_ITERATIONEN {
        iterationen += 1;
        let mut normal = [[0.0; 3]; 3];
        let mut rechts = [0.0; 3];
        for (i, p) in aktive() {
            let (dr, dh) = (p.r - r_p, p.h - h_p);
            let s = dr.hypot(dh);
            if s < MIN_STRECKE_M {
                continue;
            }
            let t = dr.atan2(dh);
            let l = norm_winkel(t - hz[i] - z) * s;
            let a = [dh / s, -dr / s, s];
            let g = gewicht(s);
            for j in 0..3 {
                rechts[j] += a[j] * g * l;
                for k in 0..3 {
                    normal[j][k] += a[j] * g * a[k];
                }
            }
        }
        let Some(dx) = gauss::loese(&normal, &rechts) else {
            break;
        };
        r_p += dx[0];
        h_p += dx[1];
        z = norm_winkel(z + dx[2]);
        if dx[0].abs() < KONV_LAGE_M && dx[1].abs() < KONV_LAGE_M && dx[2].abs() < KONV_ORIENT_RAD {
            konvergiert = true;
            break;
        }
    }

    // Standardabweichung der Gewichtseinheit und Residuen
    let redundanz = n_akt as i32 - 3;
    let mut v_pv = 0.0;
    let mut residuen = Vec::with_capacity(n);
    for (i, p) in punkte.iter().enumerate() {
        let (dr, dh) = (p.r - r_p, p.h - h_p);
        let s = dr.hypot(dh);
        let v_rad = norm_winkel(dr.atan2(dh) - hz[i] - z);
        if akt[i] && s > MIN_STRECKE_M {
            v_pv += gewicht(s) * (v_rad * s).powi(2);
        }
        residuen.push(RueckwaertsResiduum { punkt_nr: p.punkt_nr.clone(), strecke_h: s, v_winkel_cc: v_rad * RAD_ZU_CC, aktiv: akt[i] });
    }
    let s0_mm = if redundanz > 0 { (v_pv / f64::from(redundanz)).sqrt() * 1000.0 } else { 0.0 };
    let orientierung_gon = (z * RAD_ZU_GON).rem_euclid(VOLLKREIS_GON);

    Ok(RueckwaertsErgebnis {
        r: r_p,
        h: h_p,
        orientierung_gon,
        s0_mm,
        redundanz,
        iterationen,
        konvergiert,
        kritischer_kreis: pruefe_kritischen_kreis(punkte, &akt, r_p, h_p),
        residuen,
    })
}

/// Der Rückwärtsschnitt ist singulär, wenn der Standpunkt auf dem Umkreis dreier Anschlusspunkte liegt.
fn pruefe_kritischen_kreis(punkte: &[RueckwaertsPunkt], akt: &[bool], r_p: f64, h_p: f64) -> String {
    let idx: Vec<usize> = (0..punkte.len()).filter(|i| akt[*i]).collect();
    for a in 0..idx.len().saturating_sub(2) {
        for b in a + 1..idx.len() - 1 {
            for c in b + 1..idx.len() {
                let (pa, pb, pc) = (&punkte[idx[a]], &punkte[idx[b]], &punkte[idx[c]]);
                let Some((cx, cy, radius)) = umkreis(pa.r, pa.h, pb.r, pb.h, pc.r, pc.h) else {
                    continue;
                };
                let abstand = (r_p - cx).hypot(h_p - cy);
                let relativ = (abstand - radius).abs() / radius.max(MIN_UMKREIS_M);
                if relativ < KRITISCH_ANTEIL {
                    return format!(
                        "Kritischer Kreis! Standpunkt liegt nahe am Umkreis von {}–{}–{} (Abstand {:.1} % des Umkreisradius). \
                         Ergebnis geometrisch unzuverlässig – andere Punktauswahl wählen!",
                        pa.punkt_nr,
                        pb.punkt_nr,
                        pc.punkt_nr,
                        relativ * 100.0
                    );
                }
            }
        }
    }
    String::new()
}

/// Umkreis durch drei Punkte: (Mittelpunkt R, Mittelpunkt H, Radius); `None` bei kollinearen Punkten.
fn umkreis(x1: f64, y1: f64, x2: f64, y2: f64, x3: f64, y3: f64) -> Option<(f64, f64, f64)> {
    let (ax, ay) = (x2 - x1, y2 - y1);
    let (bx, by) = (x3 - x1, y3 - y1);
    let d = 2.0 * (ax * by - ay * bx);
    if d.abs() < KOLLINEAR_SCHWELLE {
        return None;
    }
    let ux = (by * (ax * ax + ay * ay) - ay * (bx * bx + by * by)) / d + x1;
    let uy = (ax * (bx * bx + by * by) - bx * (ax * ax + ay * ay)) / d + y1;
    Some((ux, uy, (ux - x1).hypot(uy - y1)))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Richtung (gon) vom Standpunkt zum Punkt, orientiert um `ori`.
    fn richtung(r_p: f64, h_p: f64, r: f64, h: f64, ori: f64) -> f64 {
        let t = (r - r_p).atan2(h - h_p) * RAD_ZU_GON;
        (t - ori).rem_euclid(VOLLKREIS_GON)
    }

    fn punkte(r_p: f64, h_p: f64, ori: f64, koord: &[(&str, f64, f64)]) -> Vec<RueckwaertsPunkt> {
        koord
            .iter()
            .map(|(nr, r, h)| RueckwaertsPunkt { punkt_nr: (*nr).to_string(), r: *r, h: *h, hz: richtung(r_p, h_p, *r, *h, ori) })
            .collect()
    }

    #[test]
    fn findet_standpunkt_fehlerfrei() {
        let p = punkte(1050.0, 2040.0, 37.5, &[("A", 1000.0, 2100.0), ("B", 1120.0, 2090.0), ("C", 1100.0, 1960.0), ("D", 980.0, 1990.0)]);
        let e = berechnen(&p, None, STANDARD_RICHTUNG_CC, STANDARD_M0_MM).unwrap();
        assert!(e.konvergiert);
        assert!((e.r - 1050.0).abs() < 1e-5 && (e.h - 2040.0).abs() < 1e-5, "R={} H={}", e.r, e.h);
        assert!((e.orientierung_gon - 37.5).abs() < 1e-5);
        assert_eq!(e.redundanz, 1);
        assert!(e.s0_mm < 1e-3);
        assert!(e.kritischer_kreis.is_empty());
    }

    #[test]
    fn zu_wenige_punkte_sind_fehler() {
        let p = punkte(0.0, 0.0, 0.0, &[("A", 10.0, 50.0), ("B", 60.0, 40.0)]);
        assert!(berechnen(&p, None, STANDARD_RICHTUNG_CC, STANDARD_M0_MM).is_err());
        let p = punkte(0.0, 0.0, 0.0, &[("A", 10.0, 50.0), ("B", 60.0, 40.0), ("C", -40.0, 30.0)]);
        assert!(berechnen(&p, Some(&[true, false, true]), STANDARD_RICHTUNG_CC, STANDARD_M0_MM).is_err());
    }

    #[test]
    fn inaktive_punkte_werden_nicht_verwendet() {
        let mut p = punkte(500.0, 500.0, 120.0, &[("A", 450.0, 560.0), ("B", 560.0, 540.0), ("C", 540.0, 440.0), ("X", 400.0, 400.0)]);
        p[3].hz += 5.0;
        let e = berechnen(&p, Some(&[true, true, true, false]), STANDARD_RICHTUNG_CC, STANDARD_M0_MM).unwrap();
        assert!((e.r - 500.0).abs() < 1e-5 && (e.h - 500.0).abs() < 1e-5);
        assert!(!e.residuen[3].aktiv);
    }

    #[test]
    fn erkennt_kritischen_kreis() {
        // Standpunkt liegt auf dem Umkreis (Einheitskreis um den Ursprung)
        let r = 100.0;
        let w = |g: f64| ((g * GON_ZU_RAD).sin() * r, (g * GON_ZU_RAD).cos() * r);
        let (a, b, c, s) = (w(0.0), w(100.0), w(250.0), w(330.0));
        let p = punkte(s.0, s.1, 0.0, &[("A", a.0, a.1), ("B", b.0, b.1), ("C", c.0, c.1)]);
        let text = pruefe_kritischen_kreis(&p, &[true, true, true], s.0, s.1);
        assert!(text.starts_with("Kritischer Kreis"));
        let innen = pruefe_kritischen_kreis(&p, &[true, true, true], 0.0, 0.0);
        assert!(innen.is_empty());
    }

    #[test]
    fn kollineare_punkte_haben_keinen_umkreis() {
        assert!(umkreis(0.0, 0.0, 1.0, 1.0, 2.0, 2.0).is_none());
    }
}
