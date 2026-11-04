// Zweck:    Vorwärtsschnitt: unbekannter Neupunkt aus mindestens zwei Stationen mit orientierten
//           Richtungswinkeln (Ausgleichung nach vermittelnden Beobachtungen)
// Hardware: keine
// Autor:    Johann Gerner

use std::f64::consts::PI;

use crate::rechnen::gauss;

/// Gon → Radiant.
const GON_ZU_RAD: f64 = PI / 200.0;
/// Zentesimalsekunden (cc) → Radiant.
const CC_ZU_RAD: f64 = PI / 2_000_000.0;
/// Radiant → cc.
const RAD_ZU_CC: f64 = 2_000_000.0 / PI;
/// Höchstzahl der Iterationen.
const MAX_ITERATIONEN: usize = 50;
/// Mindestanzahl aktiver Stationen.
pub const MIN_STATIONEN: usize = 2;
/// Streckenschwelle: kürzere Strecken werden ignoriert [m].
const MIN_STRECKE_M: f64 = 1e-6;
/// Konvergenzgrenze der Lage [m].
const KONV_LAGE_M: f64 = 1e-7;
/// Determinantenschwelle: darunter gelten die Strahlen als parallel.
const PARALLEL_SCHWELLE: f64 = 1e-9;

/// Messung einer Station zum Neupunkt.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct VorwaertsMessung {
    pub punkt_nr: String,
    /// Rechtswert der Station [m].
    pub r: f64,
    /// Hochwert der Station [m].
    pub h: f64,
    /// Horizontalrichtung von der Station zum Neupunkt [gon].
    pub hz: f64,
    /// Orientierungsunbekannte der Station [gon]; Richtungswinkel = Hz + z.
    pub z: f64,
}

/// Residuum je Station.
#[derive(Clone, Debug, PartialEq)]
pub struct VorwaertsResiduum {
    pub punkt_nr: String,
    pub strecke_h: f64,
    /// Richtungsresiduum [cc].
    pub v_winkel_cc: f64,
    pub aktiv: bool,
}

/// Ergebnis des Vorwärtsschnitts.
#[derive(Clone, Debug, PartialEq)]
pub struct VorwaertsErgebnis {
    pub r: f64,
    pub h: f64,
    pub s0_mm: f64,
    pub redundanz: i32,
    pub iterationen: usize,
    pub konvergiert: bool,
    pub residuen: Vec<VorwaertsResiduum>,
}

fn norm_winkel(a: f64) -> f64 {
    let mut w = a % (2.0 * PI);
    if w > PI {
        w -= 2.0 * PI;
    } else if w < -PI {
        w += 2.0 * PI;
    }
    w
}

/// Berechnet den Neupunkt.
///
/// `aktiv`: Auswahl der verwendeten Stationen (`None` = alle). `m_r_cc`: Richtungsgenauigkeit [cc],
/// `m0_mm`: Gewichtseinheit [mm].
pub fn berechnen(messungen: &[VorwaertsMessung], aktiv: Option<&[bool]>, m_r_cc: f64, m0_mm: f64) -> Result<VorwaertsErgebnis, String> {
    let n = messungen.len();
    if n < MIN_STATIONEN {
        return Err(format!("Mindestens {MIN_STATIONEN} Stationen erforderlich."));
    }
    let akt: Vec<bool> = aktiv.map_or_else(|| vec![true; n], <[bool]>::to_vec);
    if akt.len() != n {
        return Err("Die Aktiv-Auswahl passt nicht zur Stationsanzahl.".to_string());
    }
    let n_akt = akt.iter().filter(|a| **a).count();
    if n_akt < MIN_STATIONEN {
        return Err(format!("Mindestens {MIN_STATIONEN} aktive Stationen erforderlich (aktiv: {n_akt})."));
    }
    let t_obs: Vec<f64> = messungen.iter().map(|m| ((m.hz + m.z) * GON_ZU_RAD).rem_euclid(2.0 * PI)).collect();
    let aktive: Vec<usize> = (0..n).filter(|i| akt[*i]).collect();

    // Näherung: Schnitt der ersten beiden aktiven Strahlen, sonst Schwerpunkt
    let (i0, i1) = (aktive[0], aktive[1]);
    let (mut r_p, mut h_p) = schnitt_zweier_strahlen(
        (messungen[i0].r, messungen[i0].h, t_obs[i0]),
        (messungen[i1].r, messungen[i1].h, t_obs[i1]),
    )
    .unwrap_or_else(|| {
        let anzahl = n_akt as f64;
        (
            aktive.iter().map(|i| messungen[*i].r).sum::<f64>() / anzahl,
            aktive.iter().map(|i| messungen[*i].h).sum::<f64>() / anzahl,
        )
    });

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
        let mut normal = [[0.0; 2]; 2];
        let mut rechts = [0.0; 2];
        for &i in &aktive {
            let (dr, dh) = (r_p - messungen[i].r, h_p - messungen[i].h);
            let s = dr.hypot(dh);
            if s < MIN_STRECKE_M {
                continue;
            }
            let l = norm_winkel(t_obs[i] - dr.atan2(dh)) * s;
            let a = [dh / s, -dr / s];
            let g = gewicht(s);
            for j in 0..2 {
                rechts[j] += a[j] * g * l;
                for k in 0..2 {
                    normal[j][k] += a[j] * g * a[k];
                }
            }
        }
        let Some(dx) = gauss::loese_2x2(&normal, &rechts) else {
            break;
        };
        r_p += dx[0];
        h_p += dx[1];
        if dx[0].abs() < KONV_LAGE_M && dx[1].abs() < KONV_LAGE_M {
            konvergiert = true;
            break;
        }
    }

    let redundanz = n_akt as i32 - 2;
    let mut v_pv = 0.0;
    let mut residuen = Vec::with_capacity(n);
    for (i, m) in messungen.iter().enumerate() {
        let (dr, dh) = (r_p - m.r, h_p - m.h);
        let s = dr.hypot(dh);
        let v_rad = norm_winkel(t_obs[i] - dr.atan2(dh));
        if akt[i] && s > MIN_STRECKE_M {
            v_pv += gewicht(s) * (v_rad * s).powi(2);
        }
        residuen.push(VorwaertsResiduum { punkt_nr: m.punkt_nr.clone(), strecke_h: s, v_winkel_cc: v_rad * RAD_ZU_CC, aktiv: akt[i] });
    }
    let s0_mm = if redundanz > 0 { (v_pv / f64::from(redundanz)).sqrt() * 1000.0 } else { 0.0 };

    Ok(VorwaertsErgebnis { r: r_p, h: h_p, s0_mm, redundanz, iterationen, konvergiert, residuen })
}

/// Schnitt zweier orientierter Strahlen (R, H, Richtungswinkel [rad]); `None` bei parallelen Strahlen.
fn schnitt_zweier_strahlen(a: (f64, f64, f64), b: (f64, f64, f64)) -> Option<(f64, f64)> {
    let (r_a, h_a, t_a) = a;
    let (r_b, h_b, t_b) = b;
    // det = sin(t_b − t_a)
    let det = t_a.sin() * (-t_b.cos()) + t_b.sin() * t_a.cos();
    if det.abs() < PARALLEL_SCHWELLE {
        return None;
    }
    let (dr, dh) = (r_b - r_a, h_b - h_a);
    let lambda = (dr * (-t_b.cos()) + dh * t_b.sin()) / det;
    Some((r_a + lambda * t_a.sin(), h_a + lambda * t_a.cos()))
}

#[cfg(test)]
mod tests {
    use super::*;

    const RAD_ZU_GON: f64 = 200.0 / PI;

    fn messung(nr: &str, r: f64, h: f64, z: f64, ziel: (f64, f64)) -> VorwaertsMessung {
        let t = (ziel.0 - r).atan2(ziel.1 - h) * RAD_ZU_GON;
        VorwaertsMessung { punkt_nr: nr.to_string(), r, h, hz: (t - z).rem_euclid(400.0), z }
    }

    #[test]
    fn zwei_strahlen_schneiden_sich_exakt() {
        let ziel = (1234.5, 2345.6);
        let m = [messung("A", 1000.0, 2000.0, 10.0, ziel), messung("B", 1500.0, 2100.0, -20.0, ziel)];
        let e = berechnen(&m, None, 9.0, 1.0).unwrap();
        assert!(e.konvergiert);
        assert!((e.r - ziel.0).abs() < 1e-6 && (e.h - ziel.1).abs() < 1e-6, "R={} H={}", e.r, e.h);
        assert_eq!(e.redundanz, 0);
        assert_eq!(e.s0_mm, 0.0);
    }

    #[test]
    fn drei_strahlen_haben_redundanz_eins() {
        let ziel = (500.0, 800.0);
        let m = [
            messung("A", 400.0, 700.0, 0.0, ziel),
            messung("B", 650.0, 720.0, 0.0, ziel),
            messung("C", 480.0, 950.0, 0.0, ziel),
        ];
        let e = berechnen(&m, None, 9.0, 1.0).unwrap();
        assert_eq!(e.redundanz, 1);
        assert!((e.r - ziel.0).abs() < 1e-6 && (e.h - ziel.1).abs() < 1e-6);
        assert!(e.s0_mm < 1e-3);
    }

    #[test]
    fn fehlerhafte_richtung_ergibt_residuum() {
        let ziel = (500.0, 800.0);
        let mut m = vec![
            messung("A", 400.0, 700.0, 0.0, ziel),
            messung("B", 650.0, 720.0, 0.0, ziel),
            messung("C", 480.0, 950.0, 0.0, ziel),
        ];
        m[2].hz += 0.01;
        let e = berechnen(&m, None, 9.0, 1.0).unwrap();
        assert!(e.s0_mm > 0.1);
        assert!(e.residuen.iter().any(|r| r.v_winkel_cc.abs() > 10.0));
    }

    #[test]
    fn parallele_strahlen_fallen_auf_schwerpunkt_zurueck() {
        assert!(schnitt_zweier_strahlen((0.0, 0.0, 0.0), (10.0, 0.0, 0.0)).is_none());
    }

    #[test]
    fn zu_wenige_stationen() {
        let m = [VorwaertsMessung::default()];
        assert!(berechnen(&m, None, 9.0, 1.0).is_err());
        let m = [VorwaertsMessung::default(), VorwaertsMessung::default()];
        assert!(berechnen(&m, Some(&[true, false]), 9.0, 1.0).is_err());
    }
}
