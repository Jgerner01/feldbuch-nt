// Zweck:    Bogenschnitt: Neupunkt aus mindestens zwei gemessenen Horizontalstrecken zu bekannten Punkten
//           (gewichtete Ausgleichung, Näherung über Kreisschnitt)
// Hardware: keine
// Autor:    Johann Gerner

use super::gauss::loese_2x2;

/// Standard-Streckenkonstante [mm].
const STANDARD_MSK_MM: f64 = 3.0;
/// Standard-Maßstabsanteil [ppm].
const STANDARD_MSV_PPM: f64 = 2.0;
/// Standard-Gewichtseinheit [mm].
const STANDARD_M0_MM: f64 = 1.0;
/// Maximale Iterationen.
const MAX_ITERATIONEN: usize = 50;
/// Konvergenzschwelle [m].
const KONVERGENZ_M: f64 = 1e-7;
/// Kleinste Strecke, die noch zur Ausgleichung zählt [m].
const MIN_STRECKE_M: f64 = 1e-6;
/// Toleranz beim Kreisschnitt [m].
const KREIS_TOLERANZ_M: f64 = 1e-6;
/// Millimeter pro Meter.
const MM_PRO_M: f64 = 1000.0;

/// Gemessene Strecke zu einem bekannten Punkt.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct BogenschnittMessung {
    pub punkt_nr: String,
    pub r: f64,
    pub h: f64,
    /// Horizontalstrecke Station → Neupunkt [m].
    pub strecke: f64,
}

/// Residuum einer Strecke.
#[derive(Clone, Debug, PartialEq)]
pub struct BogenschnittResiduum {
    pub punkt_nr: String,
    pub v_strecke_mm: f64,
    pub aktiv: bool,
}

/// Ergebnis des Bogenschnitts.
#[derive(Clone, Debug, PartialEq)]
pub struct BogenschnittErgebnis {
    pub r: f64,
    pub h: f64,
    pub s0_mm: f64,
    pub redundanz: i32,
    pub iterationen: usize,
    pub konvergiert: bool,
    /// Bei r = 0 existieren zwei geometrische Lösungen.
    pub zwei_loesungen: bool,
    pub r2: f64,
    pub h2: f64,
    pub residuen: Vec<BogenschnittResiduum>,
}

/// Genauigkeitsannahmen der Streckenmessung.
#[derive(Clone, Copy, Debug)]
pub struct StreckenGenauigkeit {
    pub msk_mm: f64,
    pub msv_ppm: f64,
    pub m0_mm: f64,
}

impl Default for StreckenGenauigkeit {
    fn default() -> Self {
        Self { msk_mm: STANDARD_MSK_MM, msv_ppm: STANDARD_MSV_PPM, m0_mm: STANDARD_M0_MM }
    }
}

fn abstand(r1: f64, h1: f64, r2: f64, h2: f64) -> f64 {
    (r1 - r2).hypot(h1 - h2)
}

/// Schnitt zweier Kreise: beide Schnittpunkte oder `None`.
fn kreis_schnitt(ra: f64, ha: f64, s_a: f64, rb: f64, hb: f64, s_b: f64) -> Option<((f64, f64), (f64, f64))> {
    let (d_r, d_h) = (rb - ra, hb - ha);
    let d = d_r.hypot(d_h);
    if d < 1e-9 || s_a + s_b < d - KREIS_TOLERANZ_M || (s_a - s_b).abs() > d + KREIS_TOLERANZ_M {
        return None;
    }
    let a = (s_a * s_a - s_b * s_b + d * d) / (2.0 * d);
    let h2 = s_a * s_a - a * a;
    let h = if h2 > 0.0 { h2.sqrt() } else { 0.0 };
    let (u_r, u_h) = (d_r / d, d_h / d);
    let (pm_r, pm_h) = (ra + a * u_r, ha + a * u_h);
    Some(((pm_r - h * u_h, pm_h + h * u_r), (pm_r + h * u_h, pm_h - h * u_r)))
}

/// Gewicht einer Strecke nach Entfernung.
fn gewicht(s0: f64, g: &StreckenGenauigkeit) -> f64 {
    let m0_m = g.m0_mm / MM_PRO_M;
    let sig = (g.msk_mm + s0 * g.msv_ppm / MM_PRO_M) / MM_PRO_M;
    (m0_m * m0_m) / (sig * sig)
}

/// Berechnet den Bogenschnitt.
pub fn berechnen(
    messungen: &[BogenschnittMessung],
    aktiv: Option<&[bool]>,
    genauigkeit: StreckenGenauigkeit,
) -> Result<BogenschnittErgebnis, String> {
    let n = messungen.len();
    if n < 2 {
        return Err("Mindestens 2 Anschlusspunkte erforderlich.".to_string());
    }
    let akt: Vec<bool> = aktiv.map_or_else(|| vec![true; n], <[bool]>::to_vec);
    if akt.len() != n {
        return Err("Die Aktivierungsliste passt nicht zur Punktanzahl.".to_string());
    }
    let n_akt = akt.iter().filter(|b| **b).count();
    if n_akt < 2 {
        return Err(format!("Mindestens 2 aktive Anschlusspunkte erforderlich (aktiv: {n_akt})."));
    }

    // Näherung: Kreisschnitt der ersten beiden aktiven Punkte
    let aktive: Vec<usize> = (0..n).filter(|i| akt[*i]).collect();
    let (i0, i1) = (aktive[0], aktive[1]);
    let (m0, m1) = (&messungen[i0], &messungen[i1]);

    let mut zwei_loesungen = false;
    let (mut r2_alt, mut h2_alt) = (0.0, 0.0);
    let (mut r_p, mut h_p);
    match kreis_schnitt(m0.r, m0.h, m0.strecke, m1.r, m1.h, m1.strecke) {
        None => {
            // Kreise schneiden sich nicht: Schwerpunkt als Notlösung
            r_p = aktive.iter().map(|i| messungen[*i].r).sum::<f64>() / n_akt as f64;
            h_p = aktive.iter().map(|i| messungen[*i].h).sum::<f64>() / n_akt as f64;
        }
        Some(((r1, h1), (rx, hx))) => {
            r_p = r1;
            h_p = h1;
            if n_akt == 2 {
                zwei_loesungen = true;
                r2_alt = rx;
                h2_alt = hx;
            } else if let Some(i2) = aktive.get(2) {
                let m2 = &messungen[*i2];
                let d1 = (abstand(m2.r, m2.h, r_p, h_p) - m2.strecke).abs();
                let d2 = (abstand(m2.r, m2.h, rx, hx) - m2.strecke).abs();
                if d2 < d1 {
                    r_p = rx;
                    h_p = hx;
                }
            }
        }
    }

    // Iterative Ausgleichung
    let mut iterationen = 0;
    let mut konvergiert = false;
    for iter in 0..MAX_ITERATIONEN {
        iterationen = iter;
        let mut normal = [[0.0_f64; 2]; 2];
        let mut b = [0.0_f64; 2];
        for (i, m) in messungen.iter().enumerate() {
            if !akt[i] {
                continue;
            }
            let (d_r, d_h) = (r_p - m.r, h_p - m.h);
            let s0 = d_r.hypot(d_h);
            if s0 < MIN_STRECKE_M {
                continue;
            }
            let l = m.strecke - s0;
            let a = [d_r / s0, d_h / s0];
            let p = gewicht(s0, &genauigkeit);
            for j in 0..2 {
                b[j] += a[j] * p * l;
                for k in 0..2 {
                    normal[j][k] += a[j] * p * a[k];
                }
            }
        }
        let Some(dx) = loese_2x2(&normal, &b) else {
            break;
        };
        r_p += dx[0];
        h_p += dx[1];
        iterationen = iter + 1;
        if dx[0].abs() < KONVERGENZ_M && dx[1].abs() < KONVERGENZ_M {
            konvergiert = true;
            break;
        }
    }

    // s0 und Residuen
    let r = n_akt as i32 - 2;
    let mut vtpv = 0.0;
    let mut residuen = Vec::with_capacity(n);
    for (i, m) in messungen.iter().enumerate() {
        let s0 = (r_p - m.r).hypot(h_p - m.h);
        let v_m = m.strecke - s0;
        if akt[i] && s0 > MIN_STRECKE_M {
            vtpv += gewicht(s0, &genauigkeit) * v_m * v_m;
        }
        residuen.push(BogenschnittResiduum { punkt_nr: m.punkt_nr.clone(), v_strecke_mm: v_m * MM_PRO_M, aktiv: akt[i] });
    }
    let s0_mm = if r > 0 { (vtpv / f64::from(r)).sqrt() * MM_PRO_M } else { 0.0 };

    Ok(BogenschnittErgebnis {
        r: r_p,
        h: h_p,
        s0_mm,
        redundanz: r,
        iterationen,
        konvergiert,
        zwei_loesungen,
        r2: r2_alt,
        h2: h2_alt,
        residuen,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const PR: f64 = 1000.0;
    const PH: f64 = 2000.0;

    fn messung(nr: &str, r: f64, h: f64) -> BogenschnittMessung {
        BogenschnittMessung { punkt_nr: nr.into(), r, h, strecke: abstand(r, h, PR, PH) }
    }

    #[test]
    fn drei_strecken_ergeben_den_wahren_punkt() {
        let m = vec![messung("A", 900.0, 2100.0), messung("B", 1120.0, 2080.0), messung("C", 1050.0, 1900.0)];
        let e = berechnen(&m, None, StreckenGenauigkeit::default()).unwrap();
        assert!(e.konvergiert);
        assert!((e.r - PR).abs() < 1e-6 && (e.h - PH).abs() < 1e-6, "{} {}", e.r, e.h);
        assert_eq!(e.redundanz, 1);
        assert!(e.s0_mm < 1e-3);
        assert!(!e.zwei_loesungen);
    }

    #[test]
    fn zwei_strecken_liefern_zwei_loesungen() {
        let m = vec![messung("A", 900.0, 2100.0), messung("B", 1120.0, 2080.0)];
        let e = berechnen(&m, None, StreckenGenauigkeit::default()).unwrap();
        assert!(e.zwei_loesungen);
        assert_eq!(e.redundanz, 0);
        let nahe = |r: f64, h: f64| (r - PR).abs() < 1e-6 && (h - PH).abs() < 1e-6;
        assert!(nahe(e.r, e.h) || nahe(e.r2, e.h2));
    }

    #[test]
    fn grober_fehler_zeigt_sich_im_residuum() {
        let mut m = vec![messung("A", 900.0, 2100.0), messung("B", 1120.0, 2080.0), messung("C", 1050.0, 1900.0)];
        m[1].strecke += 0.05;
        let e = berechnen(&m, None, StreckenGenauigkeit::default()).unwrap();
        assert!(e.s0_mm > 1.0);
    }

    #[test]
    fn zu_wenige_aktive_werden_abgelehnt() {
        let m = vec![messung("A", 900.0, 2100.0), messung("B", 1120.0, 2080.0), messung("C", 1050.0, 1900.0)];
        assert!(berechnen(&m[..1], None, StreckenGenauigkeit::default()).is_err());
        assert!(berechnen(&m, Some(&[true, false, false]), StreckenGenauigkeit::default()).is_err());
    }

    #[test]
    fn sich_nicht_schneidende_kreise_nutzen_den_schwerpunkt() {
        let m = vec![
            BogenschnittMessung { punkt_nr: "A".into(), r: 0.0, h: 0.0, strecke: 1.0 },
            BogenschnittMessung { punkt_nr: "B".into(), r: 100.0, h: 0.0, strecke: 1.0 },
        ];
        // darf nicht abstürzen
        let _ = berechnen(&m, None, StreckenGenauigkeit::default());
    }
}
