//! Zweck:    Berechnung der DOP-Werte (GDOP, PDOP, HDOP, VDOP, TDOP) aus Azimut und Elevation.
//! Hardware: keine
//! Autor:    Johann Gerner
//!
//! Übernommen aus gnss-vermessung 0.4.0 (src/dop.rs), eigenständig gepflegt in Feldbuch-NT.

use crate::gnss::kern::ubxmsg::{Dop, Sat};

/// Pivotelemente darunter gelten als singuläre Geometrie.
const SINGULAR: f64 = 1e-9;

/// Richtung zu einem Satelliten; `system` unterscheidet die Uhrunbekannten.
#[derive(Debug, Clone, Copy)]
pub struct Direction {
    pub azim_deg: f64,
    pub elev_deg: f64,
    pub system: u8,
}

/// Zeitsystem eines Satelliten: SBAS und QZSS laufen auf GPS-Zeit.
pub fn clock_group(gnss: u8) -> u8 {
    match gnss {
        1 | 5 => 0,
        g => g,
    }
}

/// Richtungen aller Satelliten mit bekannter Lage oberhalb der Maske.
pub fn directions(sats: &[Sat], mask_deg: f64, used_only: bool) -> Vec<Direction> {
    sats.iter()
        .filter(|s| !used_only || s.used)
        .filter_map(|s| {
            let elev = s.elev.filter(|e| *e >= mask_deg)?;
            Some(Direction {
                azim_deg: s.azim,
                elev_deg: elev,
                system: clock_group(s.gnss),
            })
        })
        .collect()
}

/// Invertiert eine quadratische Matrix (Gauß-Jordan mit Spaltenpivot).
fn invert(mut m: Vec<Vec<f64>>) -> Option<Vec<Vec<f64>>> {
    let n = m.len();
    let mut inv: Vec<Vec<f64>> = (0..n)
        .map(|i| (0..n).map(|j| f64::from(u8::from(i == j))).collect())
        .collect();
    for c in 0..n {
        let p = (c..n).max_by(|&a, &b| m[a][c].abs().total_cmp(&m[b][c].abs()))?;
        if m[p][c].abs() < SINGULAR {
            return None;
        }
        m.swap(c, p);
        inv.swap(c, p);
        let d = m[c][c];
        for j in 0..n {
            m[c][j] /= d;
            inv[c][j] /= d;
        }
        for r in 0..n {
            if r != c {
                let f = m[r][c];
                for j in 0..n {
                    m[r][j] -= f * m[c][j];
                    inv[r][j] -= f * inv[c][j];
                }
            }
        }
    }
    Some(inv)
}

/// DOP-Werte; `None` bei zu wenigen Satelliten oder singulärer Geometrie.
/// Mit `clock_per_system` erhält jedes weitere Zeitsystem eine eigene Uhrunbekannte,
/// TDOP bezieht sich dann auf das Bezugssystem (kleinste Systemkennung).
pub fn compute(dirs: &[Direction], clock_per_system: bool) -> Option<Dop> {
    let mut systems: Vec<u8> = dirs.iter().map(|d| d.system).collect();
    systems.sort_unstable();
    systems.dedup();
    let extra: &[u8] = if clock_per_system && systems.len() > 1 {
        &systems[1..]
    } else {
        &[]
    };
    let n = 4 + extra.len();
    if dirs.len() < n {
        return None;
    }
    let mut ata = vec![vec![0.0; n]; n];
    for d in dirs {
        let (az, el) = (d.azim_deg.to_radians(), d.elev_deg.to_radians());
        let mut row = vec![-el.cos() * az.sin(), -el.cos() * az.cos(), -el.sin(), 1.0];
        row.extend(extra.iter().map(|&s| f64::from(u8::from(s == d.system))));
        for i in 0..n {
            for j in 0..n {
                ata[i][j] += row[i] * row[j];
            }
        }
    }
    let q = invert(ata)?;
    let sum = |idx: &[usize]| idx.iter().map(|&i| q[i][i]).sum::<f64>().sqrt();
    Some(Dop {
        g: sum(&[0, 1, 2, 3]),
        p: sum(&[0, 1, 2]),
        h: sum(&[0, 1]),
        v: sum(&[2]),
        t: sum(&[3]),
    })
}

/// Einheitsvektor (Ost, Nord, Hoch) zum Satelliten.
pub fn unit_vector(d: &Direction) -> [f64; 3] {
    let (az, el) = (d.azim_deg.to_radians(), d.elev_deg.to_radians());
    [el.cos() * az.sin(), el.cos() * az.cos(), el.sin()]
}

/// Elevation, bei der eine Pyramide aus vier Satelliten im Abstand von 90° Azimut
/// (Spitze beim Empfänger) das größte Volumen hat: tan² e = 1/2.
pub fn best_ring_elevation_deg() -> f64 {
    (0.5_f64).sqrt().atan().to_degrees()
}

/// Vier Satelliten im Abstand von 90° Azimut auf gleicher Elevation: ideale Schnittwinkel.
pub fn ring_pyramid(elev_deg: f64) -> [Direction; 4] {
    [0.0, 90.0, 180.0, 270.0].map(|azim_deg| Direction { azim_deg, elev_deg, system: 0 })
}

/// Die vier Richtungen nach Azimut geordnet (Umlauf der Grundfläche).
pub fn by_azimuth(p: &[Direction; 4]) -> [Direction; 4] {
    let mut v = *p;
    v.sort_by(|a, b| a.azim_deg.rem_euclid(360.0).total_cmp(&b.azim_deg.rem_euclid(360.0)));
    v
}

/// Kleinster Azimutabstand benachbarter Satelliten [°]; ideal sind 90°.
pub fn smallest_gap_deg(p: &[Direction; 4]) -> f64 {
    let v = by_azimuth(p);
    (0..4)
        .map(|i| (v[(i + 1) % 4].azim_deg - v[i].azim_deg).rem_euclid(360.0))
        .fold(f64::INFINITY, f64::min)
}

/// Volumen der Pyramide mit der Spitze beim Empfänger und den vier Satelliten
/// (Einheitsvektoren) als Grundfläche.
pub fn volume(p: &[Direction; 4]) -> f64 {
    let v = by_azimuth(p).map(|d| unit_vector(&d));
    let tetra = |a: [f64; 3], b: [f64; 3], c: [f64; 3]| {
        (a[0] * (b[1] * c[2] - b[2] * c[1]) - a[1] * (b[0] * c[2] - b[2] * c[0]) + a[2] * (b[0] * c[1] - b[1] * c[0])).abs() / 6.0
    };
    tetra(v[0], v[1], v[2]) + tetra(v[0], v[2], v[3])
}

/// Lage-Kennzahl aus den Schnittwinkeln: DOP nur der beiden Lagekomponenten
/// (ohne Höhe und Uhr); kleiner ist besser. Ring auf Elevation e: 1 / cos e.
pub fn horizontal_figure(p: &[Direction]) -> Option<f64> {
    let (mut a, mut b, mut c) = (0.0, 0.0, 0.0);
    for d in p {
        let u = unit_vector(d);
        a += u[0] * u[0];
        b += u[0] * u[1];
        c += u[1] * u[1];
    }
    let det = a * c - b * b;
    (det > SINGULAR).then(|| ((a + c) / det).sqrt())
}

/// Die vier Satelliten mit den besten Schnittwinkeln in der Draufsicht: größter kleinster
/// Azimutabstand, bei Gleichstand die kleinere Lage-Kennzahl.
pub fn best_intersection(dirs: &[Direction]) -> Option<[usize; 4]> {
    let n = dirs.len();
    let mut best: Option<(f64, f64, [usize; 4])> = None;
    for a in 0..n {
        for b in a + 1..n {
            for c in b + 1..n {
                for d in c + 1..n {
                    let four = [dirs[a], dirs[b], dirs[c], dirs[d]];
                    let gap = smallest_gap_deg(&four);
                    let figure = horizontal_figure(&four).unwrap_or(f64::INFINITY);
                    let better = best.is_none_or(|x| gap > x.0 + 1e-9 || ((gap - x.0).abs() <= 1e-9 && figure < x.1));
                    if better {
                        best = Some((gap, figure, [a, b, c, d]));
                    }
                }
            }
        }
    }
    best.map(|x| x.2)
}

/// Bewertung nach Richtwerten.
pub fn rating(gdop: f64) -> &'static str {
    match gdop {
        g if g < 2.0 => "sehr gut",
        g if g < 4.0 => "gut",
        g if g < 6.0 => "ausreichend",
        _ => "ungünstig",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dir(azim_deg: f64, elev_deg: f64, system: u8) -> Direction {
        Direction {
            azim_deg,
            elev_deg,
            system,
        }
    }

    /// Zenit und drei Satelliten am Horizont: analytisch HDOP² = 4/3, VDOP² = 4/3, TDOP² = 1/3.
    #[test]
    fn analytic_geometry() {
        let d = [
            dir(0.0, 90.0, 0),
            dir(0.0, 0.0, 0),
            dir(120.0, 0.0, 0),
            dir(240.0, 0.0, 0),
        ];
        let r = compute(&d, false).unwrap();
        assert!((r.h * r.h - 4.0 / 3.0).abs() < 1e-9);
        assert!((r.v * r.v - 4.0 / 3.0).abs() < 1e-9);
        assert!((r.t * r.t - 1.0 / 3.0).abs() < 1e-9);
        assert!((r.g * r.g - 3.0).abs() < 1e-9);
    }

    #[test]
    fn pyramids() {
        // Quadratpyramide: Volumen 2/3 · cos² e · sin e, größtes Volumen bei tan² e = 1/2
        let e = best_ring_elevation_deg();
        let v = |deg: f64| volume(&ring_pyramid(deg));
        let (c, s) = (e.to_radians().cos(), e.to_radians().sin());
        assert!((v(e) - 2.0 / 3.0 * c * c * s).abs() < 1e-12);
        assert!(v(e) > v(e - 5.0) && v(e) > v(e + 5.0));
        assert!((horizontal_figure(&ring_pyramid(60.0)).unwrap() - 2.0).abs() < 1e-9);
        // vier Satelliten auf gleicher Elevation: GDOP nicht bestimmbar (Höhe und Uhr)
        assert!(compute(&ring_pyramid(e), false).is_none());
        // Auswahl: gleichmäßig verteilte Azimute schlagen einen Satelliten im Zenit
        let all = [dir(0.0, 88.0, 0), dir(10.0, 40.0, 0), dir(95.0, 30.0, 0), dir(185.0, 50.0, 0), dir(280.0, 25.0, 0), dir(20.0, 70.0, 0)];
        let idx = best_intersection(&all).unwrap();
        assert_eq!(idx, [1, 2, 3, 4]);
        assert!(best_intersection(&all[..3]).is_none());
    }

    #[test]
    fn singular_and_too_few() {
        assert!(compute(&[dir(0.0, 45.0, 0); 6], false).is_none());
        assert!(compute(&[dir(0.0, 45.0, 0); 3], false).is_none());
        // zwei Systeme brauchen mit eigener Uhr mindestens fünf Satelliten
        let d = [
            dir(0.0, 90.0, 0),
            dir(0.0, 10.0, 0),
            dir(120.0, 10.0, 2),
            dir(240.0, 10.0, 2),
        ];
        assert!(compute(&d, true).is_none());
        assert!(compute(&d, false).is_some());
    }
}
