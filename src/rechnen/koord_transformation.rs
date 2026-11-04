// Zweck:    Koordinatentransformation: 2D-Helmert (4 Parameter, optional fester Maßstab), 3D-Helmert / 7-Parameter,
//           9-Parameter (getrennte Maßstäbe) und 6-Parameter (fester Maßstab); Ausgleichung nach kleinsten Quadraten
// Hardware: keine
// Autor:    Johann Gerner

use std::f64::consts::PI;

use crate::rechnen::gauss::kleinste_quadrate;

/// Radiant → Gon.
const RAD_ZU_GON: f64 = 200.0 / PI;
/// Gon → Radiant.
const GON_ZU_RAD: f64 = PI / 200.0;
/// Vollkreis in gon.
const VOLLKREIS_GON: f64 = 400.0;
/// Meter → Millimeter.
const MM_PRO_M: f64 = 1000.0;
/// Höchstzahl der Iterationen (2D mit festem Maßstab).
const MAX_ITERATIONEN: usize = 20;
/// Abbruchgrenze der Iteration.
const ITERATIONS_EPS: f64 = 1e-10;
/// Mindestanzahl Punktpaare bei festem Maßstab.
const MIN_PUNKTE_FEST: usize = 2;
/// Mindestanzahl Punktpaare bei freiem Maßstab.
const MIN_PUNKTE_2D: usize = 2;
const MIN_PUNKTE_3D: usize = 3;

/// Transformationsart.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransformationsTyp {
    Helmert2D,
    Helmert3D,
    Parameter7,
    Parameter9,
}

impl TransformationsTyp {
    /// Anzeigename.
    pub fn name(self) -> &'static str {
        match self {
            Self::Helmert2D => "2D Helmert (4 Parameter)",
            Self::Helmert3D => "3D Helmert (7 Parameter)",
            Self::Parameter7 => "7-Parameter Bursa-Wolf",
            Self::Parameter9 => "9-Parameter (getrennte Maßstäbe)",
        }
    }

    /// Mindestzahl der Punktpaare.
    pub fn min_punkte(self, fester_massstab: bool) -> usize {
        if fester_massstab {
            return MIN_PUNKTE_FEST;
        }
        match self {
            Self::Helmert2D => MIN_PUNKTE_2D,
            _ => MIN_PUNKTE_3D,
        }
    }
}

/// Punkt für die Transformation.
#[derive(Clone, Debug, PartialEq)]
pub struct TransformPunkt {
    pub punkt_nr: String,
    /// Rechtswert.
    pub x: f64,
    /// Hochwert.
    pub y: f64,
    /// Höhe.
    pub z: f64,
    /// Höhengleichung in der Ausgleichung berücksichtigen.
    pub use_z: bool,
}

impl TransformPunkt {
    pub fn neu(nr: &str, x: f64, y: f64, z: f64) -> Self {
        Self { punkt_nr: nr.to_string(), x, y, z, use_z: true }
    }
}

/// Transformationsparameter.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Parameter {
    pub dx: f64,
    pub dy: f64,
    pub dz: f64,
    /// 2D: a = m·cos α.
    pub a: f64,
    /// 2D: b = m·sin α.
    pub b: f64,
    pub alpha_gon: f64,
    pub massstab_2d: f64,
    pub rx_rad: f64,
    pub ry_rad: f64,
    pub rz_rad: f64,
    /// Maßstabskorrektion (7 Parameter).
    pub m: f64,
    pub mx: f64,
    pub my: f64,
    pub mz: f64,
}

/// Residuum eines Punktes [mm].
#[derive(Clone, Debug, PartialEq)]
pub struct Residuum {
    pub punkt_nr: String,
    pub vx_mm: f64,
    pub vy_mm: f64,
    pub vz_mm: f64,
    pub v_gesamt_mm: f64,
    pub hoehe_aktiv: bool,
}

/// Ergebnis der Transformation.
#[derive(Clone, Debug, PartialEq)]
pub struct Ergebnis {
    pub typ: TransformationsTyp,
    pub parameter: Parameter,
    pub s0_mm: f64,
    pub redundanz: i32,
    pub fester_massstab: bool,
    pub residuen: Vec<Residuum>,
    pub transformierte_punkte: Vec<TransformPunkt>,
}

/// Maßstabsmodell der 3D-Varianten.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Massstab {
    /// Ein gemeinsamer Maßstab (7 Parameter).
    Ein,
    /// Je Achse ein Maßstab (9 Parameter).
    Drei,
    /// Maßstab 1 fest (6 Parameter).
    Fest,
}

/// Berechnet die Transformationsparameter aus Punktpaaren (`quelle[i]` ↔ `ziel[i]`).
pub fn berechnen(typ: TransformationsTyp, quelle: &[TransformPunkt], ziel: &[TransformPunkt], fester_massstab: bool) -> Result<Ergebnis, String> {
    if quelle.len() != ziel.len() {
        return Err(format!("Interne Inkonsistenz: Quell- ({}) ≠ Ziel-Punkte ({}).", quelle.len(), ziel.len()));
    }
    let min = typ.min_punkte(fester_massstab);
    if quelle.len() < min {
        return Err(format!("Mindestens {min} Punktpaare erforderlich ({} vorhanden).", quelle.len()));
    }
    let mut ergebnis = match (typ, fester_massstab) {
        (TransformationsTyp::Helmert2D, false) => helmert_2d(quelle, ziel)?,
        (TransformationsTyp::Helmert2D, true) => helmert_2d_fest(quelle, ziel)?,
        (_, true) => helmert_3d(quelle, ziel, typ, Massstab::Fest)?,
        (TransformationsTyp::Parameter9, false) => helmert_3d(quelle, ziel, typ, Massstab::Drei)?,
        (_, false) => helmert_3d(quelle, ziel, typ, Massstab::Ein)?,
    };
    ergebnis.fester_massstab = fester_massstab;
    Ok(ergebnis)
}

/// Wendet berechnete Parameter auf einen Punkt an.
pub fn transformiere(typ: TransformationsTyp, p: &Parameter, s: &TransformPunkt) -> TransformPunkt {
    let (x, y, z) = match typ {
        TransformationsTyp::Helmert2D => (p.a * s.x - p.b * s.y + p.dx, p.b * s.x + p.a * s.y + p.dy, s.z),
        TransformationsTyp::Helmert3D | TransformationsTyp::Parameter7 => anwenden_3d(p, (p.m, p.m, p.m), s),
        TransformationsTyp::Parameter9 => anwenden_3d(p, (p.mx, p.my, p.mz), s),
    };
    TransformPunkt { punkt_nr: s.punkt_nr.clone(), x, y, z, use_z: s.use_z }
}

fn anwenden_3d(p: &Parameter, (mx, my, mz): (f64, f64, f64), s: &TransformPunkt) -> (f64, f64, f64) {
    (
        s.x + p.dx + mx * s.x - p.rz_rad * s.y + p.ry_rad * s.z,
        s.y + p.dy + p.rz_rad * s.x + my * s.y - p.rx_rad * s.z,
        s.z + p.dz - p.ry_rad * s.x + p.rx_rad * s.y + mz * s.z,
    )
}

/// Standardabweichung der Gewichtseinheit [mm].
fn s0(res: &[Residuum], nobs: usize, nun: usize) -> f64 {
    let vtv: f64 = res.iter().map(|r| r.vx_mm.powi(2) + r.vy_mm.powi(2) + r.vz_mm.powi(2)).sum();
    if nobs <= nun {
        return 0.0;
    }
    (vtv / (nobs - nun) as f64).sqrt()
}

fn norm_gon(winkel_rad: f64) -> f64 {
    (winkel_rad * RAD_ZU_GON).rem_euclid(VOLLKREIS_GON)
}

fn residuum_2d(nr: &str, berechnet: (f64, f64), soll: &TransformPunkt) -> Residuum {
    let vx = (berechnet.0 - soll.x) * MM_PRO_M;
    let vy = (berechnet.1 - soll.y) * MM_PRO_M;
    Residuum { punkt_nr: nr.to_string(), vx_mm: vx, vy_mm: vy, vz_mm: 0.0, v_gesamt_mm: vx.hypot(vy), hoehe_aktiv: true }
}

/// 2D-Ähnlichkeitstransformation mit vier Parametern.
fn helmert_2d(src: &[TransformPunkt], dst: &[TransformPunkt]) -> Result<Ergebnis, String> {
    let n = src.len();
    let mut a = Vec::with_capacity(2 * n);
    let mut l = Vec::with_capacity(2 * n);
    for (s, d) in src.iter().zip(dst) {
        a.push(vec![s.x, -s.y, 1.0, 0.0]);
        l.push(d.x);
        a.push(vec![s.y, s.x, 0.0, 1.0]);
        l.push(d.y);
    }
    let x = kleinste_quadrate(&a, &l).ok_or("Normalgleichungen singulär (linear abhängige Punkte?).")?;
    let (pa, pb, dx, dy) = (x[0], x[1], x[2], x[3]);
    let parameter = Parameter { dx, dy, a: pa, b: pb, alpha_gon: norm_gon(pb.atan2(pa)), massstab_2d: pa.hypot(pb), ..Parameter::default() };
    Ok(ergebnis_2d(src, dst, parameter, 4))
}

fn ergebnis_2d(src: &[TransformPunkt], dst: &[TransformPunkt], parameter: Parameter, nun: usize) -> Ergebnis {
    let (a, b) = (parameter.a, parameter.b);
    let abbilden = |p: &TransformPunkt| (a * p.x - b * p.y + parameter.dx, b * p.x + a * p.y + parameter.dy);
    let residuen: Vec<Residuum> = src.iter().zip(dst).map(|(s, d)| residuum_2d(&d.punkt_nr, abbilden(s), d)).collect();
    let nobs = 2 * src.len();
    let transformiert = src
        .iter()
        .map(|p| {
            let (x, y) = abbilden(p);
            TransformPunkt { punkt_nr: p.punkt_nr.clone(), x, y, z: p.z, use_z: p.use_z }
        })
        .collect();
    Ergebnis {
        typ: TransformationsTyp::Helmert2D,
        s0_mm: s0(&residuen, nobs, nun),
        redundanz: nobs as i32 - nun as i32,
        parameter,
        fester_massstab: false,
        residuen,
        transformierte_punkte: transformiert,
    }
}

/// 2D-Transformation mit festem Maßstab 1 (iterative Linearisierung, Startwert aus der freien Lösung).
fn helmert_2d_fest(src: &[TransformPunkt], dst: &[TransformPunkt]) -> Result<Ergebnis, String> {
    let frei = helmert_2d(src, dst)?;
    let mut alpha = frei.parameter.alpha_gon * GON_ZU_RAD;
    let (mut dx, mut dy) = (frei.parameter.dx, frei.parameter.dy);
    for _ in 0..MAX_ITERATIONEN {
        let (cos_a, sin_a) = (alpha.cos(), alpha.sin());
        let mut a = Vec::with_capacity(2 * src.len());
        let mut l = Vec::with_capacity(2 * src.len());
        for (s, d) in src.iter().zip(dst) {
            a.push(vec![-sin_a * s.x - cos_a * s.y, 1.0, 0.0]);
            l.push(d.x - (cos_a * s.x - sin_a * s.y + dx));
            a.push(vec![cos_a * s.x - sin_a * s.y, 0.0, 1.0]);
            l.push(d.y - (sin_a * s.x + cos_a * s.y + dy));
        }
        let korrektur = kleinste_quadrate(&a, &l).ok_or("Normalgleichungen singulär (fester Maßstab 2D).")?;
        alpha += korrektur[0];
        dx += korrektur[1];
        dy += korrektur[2];
        if korrektur.iter().map(|k| k.abs()).sum::<f64>() < ITERATIONS_EPS {
            break;
        }
    }
    let parameter = Parameter { dx, dy, a: alpha.cos(), b: alpha.sin(), alpha_gon: norm_gon(alpha), massstab_2d: 1.0, ..Parameter::default() };
    Ok(ergebnis_2d(src, dst, parameter, 3))
}

/// 3D-Transformation (linearisiertes Bursa-Wolf-Modell) mit gewähltem Maßstabsmodell.
fn helmert_3d(src: &[TransformPunkt], dst: &[TransformPunkt], typ: TransformationsTyp, modell: Massstab) -> Result<Ergebnis, String> {
    let nun = match modell {
        Massstab::Ein => 7,
        Massstab::Drei => 9,
        Massstab::Fest => 6,
    };
    let mut a: Vec<Vec<f64>> = Vec::new();
    let mut l = Vec::new();
    for (s, d) in src.iter().zip(dst) {
        let (xi, yi, zi) = (s.x, s.y, s.z);
        let massstab = |achse: usize| -> Vec<f64> {
            match modell {
                Massstab::Ein => vec![[xi, yi, zi][achse]],
                Massstab::Drei => {
                    let mut v = vec![0.0; 3];
                    v[achse] = [xi, yi, zi][achse];
                    v
                }
                Massstab::Fest => Vec::new(),
            }
        };
        let zeile = |basis: [f64; 6], achse: usize| -> Vec<f64> {
            let mut v = basis.to_vec();
            v.extend(massstab(achse));
            v
        };
        a.push(zeile([1.0, 0.0, 0.0, 0.0, zi, -yi], 0));
        l.push(d.x - xi);
        a.push(zeile([0.0, 1.0, 0.0, -zi, 0.0, xi], 1));
        l.push(d.y - yi);
        if s.use_z {
            a.push(zeile([0.0, 0.0, 1.0, yi, -xi, 0.0], 2));
            l.push(d.z - zi);
        }
    }
    let nobs = a.len();
    let x = kleinste_quadrate(&a, &l).ok_or("Normalgleichungen singulär.")?;
    let (mx, my, mz, m) = match modell {
        Massstab::Ein => (x[6], x[6], x[6], x[6]),
        Massstab::Drei => (x[6], x[7], x[8], 0.0),
        Massstab::Fest => (0.0, 0.0, 0.0, 0.0),
    };
    let parameter = Parameter {
        dx: x[0],
        dy: x[1],
        dz: x[2],
        rx_rad: x[3],
        ry_rad: x[4],
        rz_rad: x[5],
        m,
        mx: if modell == Massstab::Drei { mx } else { 0.0 },
        my: if modell == Massstab::Drei { my } else { 0.0 },
        mz: if modell == Massstab::Drei { mz } else { 0.0 },
        ..Parameter::default()
    };
    let residuen: Vec<Residuum> = src
        .iter()
        .zip(dst)
        .map(|(s, d)| {
            let (xc, yc, zc) = anwenden_3d(&parameter, (mx, my, mz), s);
            let (vx, vy) = ((xc - d.x) * MM_PRO_M, (yc - d.y) * MM_PRO_M);
            let vz = if s.use_z { (zc - d.z) * MM_PRO_M } else { 0.0 };
            Residuum { punkt_nr: d.punkt_nr.clone(), vx_mm: vx, vy_mm: vy, vz_mm: vz, v_gesamt_mm: (vx * vx + vy * vy + vz * vz).sqrt(), hoehe_aktiv: s.use_z }
        })
        .collect();
    let transformiert = src
        .iter()
        .map(|p| {
            let (x, y, z) = anwenden_3d(&parameter, (mx, my, mz), p);
            TransformPunkt { punkt_nr: p.punkt_nr.clone(), x, y, z, use_z: p.use_z }
        })
        .collect();
    Ok(Ergebnis {
        typ,
        s0_mm: s0(&residuen, nobs, nun),
        redundanz: nobs as i32 - nun as i32,
        parameter,
        fester_massstab: false,
        residuen,
        transformierte_punkte: transformiert,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const TYP_2D: TransformationsTyp = TransformationsTyp::Helmert2D;

    fn quellpunkte() -> Vec<TransformPunkt> {
        vec![
            TransformPunkt::neu("A", 100.0, 200.0, 10.0),
            TransformPunkt::neu("B", 400.0, 250.0, 25.0),
            TransformPunkt::neu("C", 300.0, 500.0, 5.0),
            TransformPunkt::neu("D", 50.0, 450.0, 40.0),
        ]
    }

    fn zielpunkte(typ: TransformationsTyp, p: &Parameter, src: &[TransformPunkt]) -> Vec<TransformPunkt> {
        src.iter().map(|s| transformiere(typ, p, s)).collect()
    }

    #[test]
    fn helmert_2d_findet_parameter() {
        let alpha: f64 = 0.3;
        let (m, dx, dy) = (1.0002, 500_000.0, 5_400_000.0);
        let p = Parameter { a: m * alpha.cos(), b: m * alpha.sin(), dx, dy, ..Parameter::default() };
        let src = quellpunkte();
        let dst = zielpunkte(TYP_2D, &p, &src);
        let e = berechnen(TYP_2D, &src, &dst, false).unwrap();
        assert!((e.parameter.massstab_2d - m).abs() < 1e-9);
        assert!((e.parameter.alpha_gon - alpha * RAD_ZU_GON).abs() < 1e-7);
        assert!((e.parameter.dx - dx).abs() < 1e-5 && (e.parameter.dy - dy).abs() < 1e-5);
        assert_eq!(e.redundanz, 4);
        assert!(e.s0_mm < 1e-3);
    }

    #[test]
    fn helmert_2d_fester_massstab() {
        let alpha: f64 = -0.2;
        let p = Parameter { a: alpha.cos(), b: alpha.sin(), dx: 1000.0, dy: -2000.0, ..Parameter::default() };
        let src = quellpunkte();
        let dst = zielpunkte(TYP_2D, &p, &src);
        let e = berechnen(TYP_2D, &src, &dst, true).unwrap();
        assert!(e.fester_massstab);
        assert!((e.parameter.massstab_2d - 1.0).abs() < 1e-12);
        assert!((e.parameter.dx - 1000.0).abs() < 1e-5 && (e.parameter.dy + 2000.0).abs() < 1e-5);
        assert_eq!(e.redundanz, 5);
        assert!(e.s0_mm < 1e-3);
    }

    #[test]
    fn sieben_parameter_modell() {
        let p = Parameter { dx: 120.0, dy: -80.0, dz: 15.0, rx_rad: 2e-5, ry_rad: -3e-5, rz_rad: 4e-5, m: 5e-6, ..Parameter::default() };
        let src = quellpunkte();
        for typ in [TransformationsTyp::Helmert3D, TransformationsTyp::Parameter7] {
            let dst = zielpunkte(typ, &p, &src);
            let e = berechnen(typ, &src, &dst, false).unwrap();
            assert!((e.parameter.dx - 120.0).abs() < 1e-6 && (e.parameter.m - 5e-6).abs() < 1e-12);
            assert!((e.parameter.rz_rad - 4e-5).abs() < 1e-11);
            assert_eq!(e.redundanz, 12 - 7);
            assert!(e.s0_mm < 1e-3);
        }
    }

    #[test]
    fn neun_parameter_modell() {
        let p = Parameter { dx: 10.0, dy: 20.0, dz: 30.0, rx_rad: 1e-5, ry_rad: 2e-5, rz_rad: 3e-5, mx: 4e-6, my: 5e-6, mz: 6e-6, ..Parameter::default() };
        let src = quellpunkte();
        let dst = zielpunkte(TransformationsTyp::Parameter9, &p, &src);
        let e = berechnen(TransformationsTyp::Parameter9, &src, &dst, false).unwrap();
        assert!((e.parameter.mx - 4e-6).abs() < 1e-11 && (e.parameter.my - 5e-6).abs() < 1e-11 && (e.parameter.mz - 6e-6).abs() < 1e-11);
        assert_eq!(e.redundanz, 12 - 9);
    }

    #[test]
    fn sechs_parameter_bei_festem_massstab() {
        let p = Parameter { dx: 5.0, dy: 6.0, dz: 7.0, rx_rad: 1e-5, ry_rad: 1e-5, rz_rad: 2e-5, ..Parameter::default() };
        let src = quellpunkte();
        let dst = zielpunkte(TransformationsTyp::Helmert3D, &p, &src);
        let e = berechnen(TransformationsTyp::Helmert3D, &src, &dst, true).unwrap();
        assert_eq!(e.redundanz, 12 - 6);
        assert_eq!(e.parameter.m, 0.0);
        assert!(e.s0_mm < 1e-3);
    }

    #[test]
    fn hoehe_kann_abgeschaltet_werden() {
        let p = Parameter { dx: 1.0, dy: 2.0, dz: 3.0, m: 1e-6, ..Parameter::default() };
        let mut src = quellpunkte();
        let dst = zielpunkte(TransformationsTyp::Helmert3D, &p, &src);
        src[0].use_z = false;
        src[1].use_z = false;
        let e = berechnen(TransformationsTyp::Helmert3D, &src, &dst, false).unwrap();
        assert_eq!(e.redundanz, (2 + 2 + 3 + 3) - 7);
        assert!(!e.residuen[0].hoehe_aktiv && e.residuen[0].vz_mm == 0.0);
    }

    #[test]
    fn fehlerfaelle() {
        let src = quellpunkte();
        assert!(berechnen(TYP_2D, &src[..1], &src[..1], false).is_err());
        assert!(berechnen(TransformationsTyp::Helmert3D, &src[..2], &src[..2], false).is_err());
        assert!(berechnen(TYP_2D, &src, &src[..2], false).is_err());
        let gleich = vec![TransformPunkt::neu("A", 1.0, 1.0, 0.0), TransformPunkt::neu("B", 1.0, 1.0, 0.0)];
        assert!(berechnen(TYP_2D, &gleich, &gleich, false).is_err());
    }
}
