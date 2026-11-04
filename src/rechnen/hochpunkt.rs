// Zweck:    Hochpunktherablegung: Lage (Vorwärtsschnitt) und Höhe (trigonometrisch) eines unzugänglichen
//           Punktes aus mindestens zwei Stationen mit Hz und Zenitwinkel
// Hardware: keine
// Autor:    Johann Gerner

use std::f64::consts::PI;

use super::gauss::loese_2x2;

/// Umrechnung gon → rad.
const GON2RAD: f64 = PI / 200.0;
/// Umrechnung cc → rad.
const CC2RAD: f64 = PI / 2_000_000.0;
/// Umrechnung rad → cc.
const RAD2CC: f64 = 2_000_000.0 / PI;
/// Maximale Iterationen.
const MAX_ITERATIONEN: usize = 50;
/// Konvergenzschwelle [m].
const KONVERGENZ_M: f64 = 1e-7;
/// Kleinste Strecke, die noch zählt [m].
const MIN_STRECKE_M: f64 = 1e-6;
/// Kleinster Sinus des Zenitwinkels für die Höhenübertragung.
const MIN_SIN_V: f64 = 1e-6;
/// Untergrenze der Strecke im Höhengewicht [m].
const MIN_GEWICHT_STRECKE_M: f64 = 0.01;
/// Determinantenschwelle beim Strahlenschnitt.
const DET_SCHWELLE: f64 = 1e-9;
/// Millimeter pro Meter.
const MM_PRO_M: f64 = 1000.0;

/// Messung von einer Station zum Hochpunkt.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct HochpunktMessung {
    pub punkt_nr: String,
    pub r: f64,
    pub h: f64,
    pub hoehe: f64,
    /// Instrumentenhöhe [m].
    pub ih: f64,
    /// Horizontalrichtung zum Hochpunkt [gon].
    pub hz: f64,
    /// Orientierungsunbekannte der Station [gon].
    pub z: f64,
    /// Zenitwinkel [gon].
    pub v: f64,
    /// Zielhöhe am Hochpunkt [m].
    pub zielhoehe: f64,
}

/// Residuen einer Station.
#[derive(Clone, Debug, PartialEq)]
pub struct HochpunktResiduum {
    pub punkt_nr: String,
    pub s_horiz: f64,
    pub v_dir_cc: f64,
    pub hoehe_pi: f64,
    pub v_h_mm: f64,
    pub aktiv_dir: bool,
    pub aktiv_h: bool,
}

/// Ergebnis der Hochpunktherablegung.
#[derive(Clone, Debug, PartialEq)]
pub struct HochpunktErgebnis {
    pub r: f64,
    pub h: f64,
    pub hoehe: f64,
    pub s0_dir_mm: f64,
    pub s0_h_mm: f64,
    pub redundanz_dir: i32,
    pub redundanz_h: i32,
    pub iterationen: usize,
    pub konvergiert: bool,
    pub residuen: Vec<HochpunktResiduum>,
}

fn norm_winkel(mut a: f64) -> f64 {
    while a > PI {
        a -= 2.0 * PI;
    }
    while a < -PI {
        a += 2.0 * PI;
    }
    a
}

fn norm_winkel_2pi(a: f64) -> f64 {
    let w = a % (2.0 * PI);
    if w < 0.0 { w + 2.0 * PI } else { w }
}

/// Schnitt zweier Strahlen (Station, Richtungswinkel).
fn schnitt_2_strahlen(ra: f64, ha: f64, t_a: f64, rb: f64, hb: f64, t_b: f64) -> Option<(f64, f64)> {
    let det = t_a.sin() * (-t_b.cos()) - (-t_b.sin()) * t_a.cos();
    if det.abs() < DET_SCHWELLE {
        return None;
    }
    let lam = ((rb - ra) * (-t_b.cos()) - (hb - ha) * (-t_b.sin())) / det;
    Some((ra + lam * t_a.sin(), ha + lam * t_a.cos()))
}

/// Berechnet Lage und Höhe des Hochpunkts.
pub fn berechnen(
    messungen: &[HochpunktMessung],
    aktiv: Option<&[bool]>,
    m_r_cc: f64,
    m0_mm: f64,
) -> Result<HochpunktErgebnis, String> {
    let n = messungen.len();
    if n < 2 {
        return Err("Mindestens 2 Stationen erforderlich.".to_string());
    }
    let akt: Vec<bool> = aktiv.map_or_else(|| vec![true; n], <[bool]>::to_vec);
    if akt.len() != n {
        return Err("Die Aktivierungsliste passt nicht zur Punktanzahl.".to_string());
    }
    let n_akt = akt.iter().filter(|b| **b).count();
    if n_akt < 2 {
        return Err(format!("Mindestens 2 aktive Stationen erforderlich (aktiv: {n_akt})."));
    }

    let t_obs: Vec<f64> = messungen.iter().map(|m| norm_winkel_2pi((m.hz + m.z) * GON2RAD)).collect();
    let v_rad: Vec<f64> = messungen.iter().map(|m| m.v * GON2RAD).collect();

    // Schritt 1: Vorwärtsschnitt (Planimetrie)
    let aktive: Vec<usize> = (0..n).filter(|i| akt[*i]).collect();
    let (i0, i1) = (aktive[0], aktive[1]);
    let (mut r_p, mut h_p) = schnitt_2_strahlen(
        messungen[i0].r, messungen[i0].h, t_obs[i0], messungen[i1].r, messungen[i1].h, t_obs[i1],
    )
    .unwrap_or_else(|| {
        (
            aktive.iter().map(|i| messungen[*i].r).sum::<f64>() / n_akt as f64,
            aktive.iter().map(|i| messungen[*i].h).sum::<f64>() / n_akt as f64,
        )
    });

    let m0_m = m0_mm / MM_PRO_M;
    let m_r_rad = m_r_cc * CC2RAD;
    let mut iterationen = 0;
    let mut konvergiert = false;
    for iter in 0..MAX_ITERATIONEN {
        let mut normal = [[0.0_f64; 2]; 2];
        let mut b = [0.0_f64; 2];
        for (i, m) in messungen.iter().enumerate() {
            if !akt[i] {
                continue;
            }
            let (d_r, d_h) = (r_p - m.r, h_p - m.h);
            let s = d_r.hypot(d_h);
            if s < MIN_STRECKE_M {
                continue;
            }
            let t0 = d_r.atan2(d_h);
            let l = norm_winkel(t_obs[i] - t0) * s;
            let a = [d_h / s, -d_r / s];
            let sig = m_r_rad * s;
            let p = (m0_m * m0_m) / (sig * sig);
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

    // Schritt 2: Höhe (gewichtetes Mittel)
    let (mut summe_wh, mut summe_w) = (0.0, 0.0);
    for (i, m) in messungen.iter().enumerate() {
        if !akt[i] {
            continue;
        }
        let sin_v = v_rad[i].sin();
        if sin_v.abs() < MIN_SIN_V {
            continue;
        }
        let s = (r_p - m.r).hypot(h_p - m.h);
        let d_hi = s * v_rad[i].cos() / sin_v;
        let h_pi = m.hoehe + m.ih + d_hi - m.zielhoehe;
        let w = sin_v * sin_v / s.max(MIN_GEWICHT_STRECKE_M);
        summe_wh += w * h_pi;
        summe_w += w;
    }
    let hoehe_p = if summe_w > 0.0 { summe_wh / summe_w } else { 0.0 };

    // Residuen und s0
    let r_dir = n_akt as i32 - 2;
    let (mut vtpv_dir, mut vtpv_h, mut n_h) = (0.0, 0.0, 0);
    let mut residuen = Vec::with_capacity(n);
    for (i, m) in messungen.iter().enumerate() {
        let (d_r, d_h) = (r_p - m.r, h_p - m.h);
        let s = d_r.hypot(d_h);
        let t0 = d_r.atan2(d_h);
        let v_dir = norm_winkel(t_obs[i] - t0) * RAD2CC;

        let (mut h_pi, mut v_h, mut hat_h) = (f64::NAN, f64::NAN, false);
        let sin_v = v_rad[i].sin();
        if akt[i] && sin_v.abs() > MIN_SIN_V && s > MIN_STRECKE_M {
            let d_hi = s * v_rad[i].cos() / sin_v;
            h_pi = m.hoehe + m.ih + d_hi - m.zielhoehe;
            v_h = (h_pi - hoehe_p) * MM_PRO_M;
            vtpv_h += (sin_v * sin_v / s) * v_h * v_h;
            hat_h = true;
            n_h += 1;
        }
        if akt[i] && s > MIN_STRECKE_M {
            let sig = m_r_rad * s;
            let p = (m0_m * m0_m) / (sig * sig);
            let v_rad_dir = norm_winkel(t_obs[i] - t0);
            vtpv_dir += p * (v_rad_dir * s) * (v_rad_dir * s);
        }
        residuen.push(HochpunktResiduum {
            punkt_nr: m.punkt_nr.clone(),
            s_horiz: s,
            v_dir_cc: v_dir,
            hoehe_pi: h_pi,
            v_h_mm: v_h,
            aktiv_dir: akt[i],
            aktiv_h: akt[i] && hat_h,
        });
    }
    let r_h = (n_h - 1).max(0);
    let s0_dir_mm = if r_dir > 0 { (vtpv_dir / f64::from(r_dir)).sqrt() * MM_PRO_M } else { 0.0 };
    let s0_h_mm = if r_h > 0 { (vtpv_h / f64::from(r_h)).sqrt() } else { 0.0 };

    Ok(HochpunktErgebnis {
        r: r_p,
        h: h_p,
        hoehe: hoehe_p,
        s0_dir_mm,
        s0_h_mm,
        redundanz_dir: r_dir,
        redundanz_h: r_h,
        iterationen,
        konvergiert,
        residuen,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const PR: f64 = 1000.0;
    const PH: f64 = 2000.0;
    const P_HOEHE: f64 = 130.0;

    /// Station mit exakter Messung zum Hochpunkt.
    fn station(nr: &str, r: f64, h: f64, hoehe: f64, ih: f64, z: f64) -> HochpunktMessung {
        let (d_r, d_h) = (PR - r, PH - h);
        let s = d_r.hypot(d_h);
        let t_gon = d_r.atan2(d_h) / GON2RAD;
        let hz = ((t_gon - z) % 400.0 + 400.0) % 400.0;
        let d_hoehe = P_HOEHE - (hoehe + ih);
        let v = s.atan2(d_hoehe) / GON2RAD;
        HochpunktMessung { punkt_nr: nr.into(), r, h, hoehe, ih, hz, z, v, zielhoehe: 0.0 }
    }

    #[test]
    fn lage_und_hoehe_werden_exakt_bestimmt() {
        let m = vec![
            station("A", 900.0, 1900.0, 100.0, 1.5, 12.0),
            station("B", 1130.0, 1880.0, 101.0, 1.6, 55.0),
            station("C", 1010.0, 2150.0, 99.0, 1.4, 301.0),
        ];
        let e = berechnen(&m, None, 9.0, 1.0).unwrap();
        assert!(e.konvergiert);
        assert!((e.r - PR).abs() < 1e-6 && (e.h - PH).abs() < 1e-6);
        assert!((e.hoehe - P_HOEHE).abs() < 1e-6, "Höhe {}", e.hoehe);
        assert_eq!(e.redundanz_dir, 1);
        assert_eq!(e.redundanz_h, 2);
        assert!(e.s0_dir_mm < 1e-3 && e.s0_h_mm < 1e-3);
    }

    #[test]
    fn zwei_stationen_reichen() {
        let m = vec![station("A", 900.0, 1900.0, 100.0, 1.5, 0.0), station("B", 1130.0, 1880.0, 101.0, 1.6, 0.0)];
        let e = berechnen(&m, None, 9.0, 1.0).unwrap();
        assert!((e.r - PR).abs() < 1e-6);
        assert_eq!(e.redundanz_dir, 0);
    }

    #[test]
    fn zu_wenige_stationen_werden_abgelehnt() {
        let m = vec![station("A", 900.0, 1900.0, 100.0, 1.5, 0.0)];
        assert!(berechnen(&m, None, 9.0, 1.0).is_err());
        let m = vec![station("A", 900.0, 1900.0, 100.0, 1.5, 0.0), station("B", 1130.0, 1880.0, 101.0, 1.6, 0.0)];
        assert!(berechnen(&m, Some(&[true, false]), 9.0, 1.0).is_err());
    }
}
