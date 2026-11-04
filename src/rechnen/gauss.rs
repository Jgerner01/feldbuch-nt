// Zweck:    Gauß-Elimination mit Spaltenpivotsuche für kleine lineare Gleichungssysteme (2×2)
// Hardware: keine
// Autor:    Johann Gerner

/// Pivot-Schwelle: darunter gilt die Matrix als singulär.
const PIVOT_SCHWELLE: f64 = 1e-14;

/// Löst ein 2×2-System; `None` bei singulärer Matrix.
pub fn loese_2x2(a: &[[f64; 2]; 2], b: &[f64; 2]) -> Option<[f64; 2]> {
    let mut m = [[a[0][0], a[0][1], b[0]], [a[1][0], a[1][1], b[1]]];
    for spalte in 0..2 {
        let mut pivot = spalte;
        for zeile in spalte + 1..2 {
            if m[zeile][spalte].abs() > m[pivot][spalte].abs() {
                pivot = zeile;
            }
        }
        m.swap(spalte, pivot);
        if m[spalte][spalte].abs() < PIVOT_SCHWELLE {
            return None;
        }
        for zeile in 0..2 {
            if zeile == spalte {
                continue;
            }
            let faktor = m[zeile][spalte] / m[spalte][spalte];
            for k in spalte..3 {
                m[zeile][k] -= faktor * m[spalte][k];
            }
        }
    }
    Some([m[0][2] / m[0][0], m[1][2] / m[1][1]])
}

/// Kleinste Quadrate über die Normalgleichungen AᵀA·x = Aᵀl; `None` bei singulärer Normalmatrix.
pub fn kleinste_quadrate(a: &[Vec<f64>], l: &[f64]) -> Option<Vec<f64>> {
    let nun = a.first()?.len();
    let mut m = vec![vec![0.0; nun + 1]; nun];
    for (i, zeile) in m.iter_mut().enumerate() {
        for j in 0..nun {
            zeile[j] = a.iter().map(|r| r[i] * r[j]).sum();
        }
        zeile[nun] = a.iter().zip(l).map(|(r, li)| r[i] * li).sum();
    }
    for spalte in 0..nun {
        let mut pivot = spalte;
        for zeile in spalte + 1..nun {
            if m[zeile][spalte].abs() > m[pivot][spalte].abs() {
                pivot = zeile;
            }
        }
        m.swap(spalte, pivot);
        if m[spalte][spalte].abs() < PIVOT_SCHWELLE {
            return None;
        }
        for zeile in spalte + 1..nun {
            let faktor = m[zeile][spalte] / m[spalte][spalte];
            for k in spalte..=nun {
                m[zeile][k] -= faktor * m[spalte][k];
            }
        }
    }
    let mut x = vec![0.0; nun];
    for i in (0..nun).rev() {
        let rest: f64 = (i + 1..nun).map(|j| m[i][j] * x[j]).sum();
        x[i] = (m[i][nun] - rest) / m[i][i];
    }
    Some(x)
}

/// Löst ein N×N-System mit Spaltenpivotsuche (Gauß-Jordan); `None` bei singulärer Matrix.
pub fn loese<const N: usize>(a: &[[f64; N]; N], b: &[f64; N]) -> Option<[f64; N]> {
    let mut m = vec![vec![0.0; N + 1]; N];
    for i in 0..N {
        m[i][..N].copy_from_slice(&a[i]);
        m[i][N] = b[i];
    }
    for spalte in 0..N {
        let mut pivot = spalte;
        for zeile in spalte + 1..N {
            if m[zeile][spalte].abs() > m[pivot][spalte].abs() {
                pivot = zeile;
            }
        }
        m.swap(spalte, pivot);
        if m[spalte][spalte].abs() < PIVOT_SCHWELLE {
            return None;
        }
        for zeile in 0..N {
            if zeile == spalte {
                continue;
            }
            let faktor = m[zeile][spalte] / m[spalte][spalte];
            for k in spalte..=N {
                m[zeile][k] -= faktor * m[spalte][k];
            }
        }
    }
    let mut x = [0.0; N];
    for i in 0..N {
        x[i] = m[i][N] / m[i][i];
    }
    Some(x)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loest_3x3() {
        let a = [[2.0, 1.0, 0.0], [1.0, 3.0, 1.0], [0.0, 1.0, 4.0]];
        let b = [3.0, 10.0, 14.0];
        let x = loese(&a, &b).unwrap();
        for (zeile, rechts) in a.iter().zip(b) {
            let links: f64 = zeile.iter().zip(x).map(|(k, v)| k * v).sum();
            assert!((links - rechts).abs() < 1e-12);
        }
        assert!(loese(&[[1.0, 2.0, 3.0], [2.0, 4.0, 6.0], [1.0, 0.0, 1.0]], &[1.0, 2.0, 3.0]).is_none());
    }

    #[test]
    fn loest_system_und_erkennt_singulaer() {
        let x = loese_2x2(&[[2.0, 1.0], [1.0, 3.0]], &[5.0, 10.0]).unwrap();
        assert!((x[0] - 1.0).abs() < 1e-12 && (x[1] - 3.0).abs() < 1e-12);
        assert!(loese_2x2(&[[1.0, 2.0], [2.0, 4.0]], &[1.0, 2.0]).is_none());
    }
}
