// Zweck:    Höheneinpassung der GNSS-Einpassung: Höhenverschiebung (1 Parameter) oder geneigte Ebene
//           (Verschiebung und Neigung nach Ost und Nord, 3 Parameter), Ausgleichung nach kleinsten Quadraten
// Hardware: keine
// Autor:    Johann Gerner

use crate::rechnen::gauss::kleinste_quadrate;

/// Meter → Millimeter.
const MM_PRO_M: f64 = 1000.0;
/// Mindestzahl der Punkte je Modell.
const MIN_PUNKTE_VERSCHIEBUNG: usize = 1;
const MIN_PUNKTE_EBENE: usize = 3;

/// Modell der Höheneinpassung.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HoehenModell {
    /// Soll = Ist + a0
    Verschiebung,
    /// Soll = Ist + a0 + aE·(E − E0) + aN·(N − N0)
    Ebene,
}

impl HoehenModell {
    pub fn min_punkte(self) -> usize {
        match self {
            Self::Verschiebung => MIN_PUNKTE_VERSCHIEBUNG,
            Self::Ebene => MIN_PUNKTE_EBENE,
        }
    }

    fn unbekannte(self) -> usize {
        match self {
            Self::Verschiebung => 1,
            Self::Ebene => 3,
        }
    }
}

/// Ein Punkt der Höheneinpassung: Lage (Ost, Nord), gemessene und Sollhöhe [m].
#[derive(Clone, Debug, PartialEq)]
pub struct HoehenPunkt {
    pub nr: String,
    pub e: f64,
    pub n: f64,
    pub ist: f64,
    pub soll: f64,
}

/// Ergebnis der Höheneinpassung.
#[derive(Clone, Debug, PartialEq)]
pub struct HoehenErgebnis {
    pub modell: HoehenModell,
    pub a0: f64,
    /// Neigung nach Ost und Nord [m/m]
    pub a_e: f64,
    pub a_n: f64,
    /// Schwerpunkt der Lage (Bezug der Neigung)
    pub e0: f64,
    pub n0: f64,
    /// Standardabweichung der Gewichtseinheit [mm] (0 ohne Überbestimmung)
    pub s0_mm: f64,
    /// Residuen v = berechnet − Soll [mm] je Punkt
    pub residuen_mm: Vec<(String, f64)>,
}

impl HoehenErgebnis {
    /// Höhe im Zielsystem aus der gemessenen Höhe.
    pub fn anwenden(&self, e: f64, n: f64, ist: f64) -> f64 {
        ist + self.a0 + self.a_e * (e - self.e0) + self.a_n * (n - self.n0)
    }
}

/// Berechnet die Höheneinpassung.
pub fn berechne(modell: HoehenModell, punkte: &[HoehenPunkt]) -> Result<HoehenErgebnis, String> {
    if punkte.len() < modell.min_punkte() {
        return Err(format!("Höheneinpassung: mindestens {} Punkte mit Höhe erforderlich ({} vorhanden).", modell.min_punkte(), punkte.len()));
    }
    let anzahl = punkte.len() as f64;
    let e0 = punkte.iter().map(|p| p.e).sum::<f64>() / anzahl;
    let n0 = punkte.iter().map(|p| p.n).sum::<f64>() / anzahl;
    let a: Vec<Vec<f64>> = punkte
        .iter()
        .map(|p| match modell {
            HoehenModell::Verschiebung => vec![1.0],
            HoehenModell::Ebene => vec![1.0, p.e - e0, p.n - n0],
        })
        .collect();
    let l: Vec<f64> = punkte.iter().map(|p| p.soll - p.ist).collect();
    let x = kleinste_quadrate(&a, &l).ok_or("Höheneinpassung: Punkte liegen auf einer Linie (Ebene nicht bestimmbar).")?;
    let (a0, a_e, a_n) = match modell {
        HoehenModell::Verschiebung => (x[0], 0.0, 0.0),
        HoehenModell::Ebene => (x[0], x[1], x[2]),
    };
    let mut ergebnis = HoehenErgebnis { modell, a0, a_e, a_n, e0, n0, s0_mm: 0.0, residuen_mm: Vec::new() };
    ergebnis.residuen_mm = punkte.iter().map(|p| (p.nr.clone(), (ergebnis.anwenden(p.e, p.n, p.ist) - p.soll) * MM_PRO_M)).collect();
    let redundanz = punkte.len().saturating_sub(modell.unbekannte());
    if redundanz > 0 {
        let vtv: f64 = ergebnis.residuen_mm.iter().map(|(_, v)| v * v).sum();
        ergebnis.s0_mm = (vtv / redundanz as f64).sqrt();
    }
    Ok(ergebnis)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(nr: &str, e: f64, n: f64, ist: f64, soll: f64) -> HoehenPunkt {
        HoehenPunkt { nr: nr.into(), e, n, ist, soll }
    }

    #[test]
    fn verschiebung_ist_mittel_der_differenzen() {
        let erg = berechne(HoehenModell::Verschiebung, &[p("1", 0.0, 0.0, 400.0, 352.10), p("2", 10.0, 0.0, 401.0, 353.12)]).unwrap();
        assert!((erg.a0 + 47.89).abs() < 1e-9);
        assert!((erg.residuen_mm[0].1 - 10.0).abs() < 1e-6);
        assert!((erg.s0_mm - 200f64.sqrt()).abs() < 1e-6);
    }

    #[test]
    fn ebene_wird_exakt_wiedergefunden() {
        // Soll = Ist − 48 + 0,001·(E − E0) − 0,002·(N − N0)
        let soll = |e: f64, n: f64, ist: f64| ist - 48.0 + 0.001 * (e - 50.0) - 0.002 * (n - 50.0);
        let punkte: Vec<HoehenPunkt> = [(0.0, 0.0), (100.0, 0.0), (0.0, 100.0), (100.0, 100.0)]
            .iter()
            .enumerate()
            .map(|(i, &(e, n))| p(&i.to_string(), e, n, 400.0 + e * 0.01, soll(e, n, 400.0 + e * 0.01)))
            .collect();
        let erg = berechne(HoehenModell::Ebene, &punkte).unwrap();
        assert!((erg.a_e - 0.001).abs() < 1e-12 && (erg.a_n + 0.002).abs() < 1e-12);
        assert!(erg.s0_mm < 1e-6);
        assert!((erg.anwenden(30.0, 70.0, 410.0) - soll(30.0, 70.0, 410.0)).abs() < 1e-9);
    }

    #[test]
    fn zu_wenige_oder_kollineare_punkte() {
        assert!(berechne(HoehenModell::Ebene, &[p("1", 0.0, 0.0, 1.0, 1.0), p("2", 1.0, 0.0, 1.0, 1.0)]).is_err());
        let linie = [p("1", 0.0, 0.0, 1.0, 1.0), p("2", 1.0, 1.0, 1.0, 1.0), p("3", 2.0, 2.0, 1.0, 1.0)];
        assert!(berechne(HoehenModell::Ebene, &linie).is_err());
    }
}
