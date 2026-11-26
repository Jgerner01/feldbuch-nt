//! Height reference: ellipsoidal (GRS80) -> DHHN2016 normal height via quasigeoid.
//!
//! The official quasigeoid GCG2016 is a BKG product (licensed, not bundled).
//! Supported input: ASCII grid, one point per line "lat lon zeta" (degrees, metres),
//! regular spacing, comment lines starting with '#'. Convert other GCG2016
//! delivery formats to this layout once.
//!
//! Hardware: simpleRTK2B (u-blox ZED-F9P)
//! Autor:    Johann Gerner
//!
//! Übernommen aus gnss-vermessung 0.4.0 (src/height.rs), eigenständig gepflegt in Feldbuch-NT.

use anyhow::{Context, Result, bail};
use std::path::Path;

pub enum HeightModel {
    /// no conversion, H = h (ellipsoidal)
    None,
    /// constant height anomaly, e.g. from a local Höhenanschluss
    Constant(f64),
    Grid(Grid),
    /// SAPOS-Transformationsnachrichten mit der Höhenanomalie des Empfängers zur Prüfung
    Sapos(Box<(crate::gnss::kern::transform::Transformation, Option<f64>)>),
}

impl HeightModel {
    /// height anomaly zeta at the point; H = h - zeta
    pub fn zeta(&self, lat: f64, lon: f64) -> Option<f64> {
        match self {
            HeightModel::None => None,
            HeightModel::Constant(z) => Some(*z),
            HeightModel::Grid(g) => g.interpolate(lat, lon),
            HeightModel::Sapos(t) => t.0.height(lat, lon, t.1).ok().map(|c| c.zeta),
        }
    }
}

pub struct Grid {
    lat0: f64,
    lon0: f64,
    dlat: f64,
    dlon: f64,
    nlat: usize,
    nlon: usize,
    z: Vec<f64>,
}

impl Grid {
    pub fn load(path: &Path) -> Result<Self> {
        let txt = std::fs::read_to_string(path)
            .with_context(|| format!("Quasigeoid {} nicht lesbar", path.display()))?;
        let mut pts = Vec::new();
        for l in txt.lines() {
            let l = l.trim();
            if l.is_empty() || l.starts_with('#') {
                continue;
            }
            let v: Vec<f64> = l
                .split_whitespace()
                .filter_map(|x| x.parse().ok())
                .collect();
            if v.len() >= 3 {
                pts.push((v[0], v[1], v[2]));
            }
        }
        Self::from_points(&pts)
    }

    pub fn from_points(pts: &[(f64, f64, f64)]) -> Result<Self> {
        if pts.len() < 4 {
            bail!("Quasigeoid: zu wenige Gitterpunkte");
        }
        let uniq = |sel: fn(&(f64, f64, f64)) -> f64| {
            let mut v: Vec<f64> = pts.iter().map(sel).collect();
            v.sort_by(|a, b| a.partial_cmp(b).unwrap());
            v.dedup_by(|a, b| (*a - *b).abs() < 1e-9);
            v
        };
        let lats = uniq(|p| p.0);
        let lons = uniq(|p| p.1);
        let (nlat, nlon) = (lats.len(), lons.len());
        if nlat < 2 || nlon < 2 || nlat * nlon != pts.len() {
            bail!(
                "Quasigeoid: kein vollständiges regelmäßiges Gitter ({nlat}x{nlon} vs {} Punkte)",
                pts.len()
            );
        }
        let (lat0, lon0) = (lats[0], lons[0]);
        let dlat = (lats[nlat - 1] - lat0) / (nlat - 1) as f64;
        let dlon = (lons[nlon - 1] - lon0) / (nlon - 1) as f64;
        let mut z = vec![f64::NAN; nlat * nlon];
        for &(la, lo, zz) in pts {
            let i = ((la - lat0) / dlat).round() as usize;
            let j = ((lo - lon0) / dlon).round() as usize;
            z[i * nlon + j] = zz;
        }
        Ok(Grid {
            lat0,
            lon0,
            dlat,
            dlon,
            nlat,
            nlon,
            z,
        })
    }

    /// bilinear interpolation
    pub fn interpolate(&self, lat: f64, lon: f64) -> Option<f64> {
        let y = (lat - self.lat0) / self.dlat;
        let x = (lon - self.lon0) / self.dlon;
        if y < 0.0 || x < 0.0 || y > (self.nlat - 1) as f64 || x > (self.nlon - 1) as f64 {
            return None;
        }
        let i = (y.floor() as usize).min(self.nlat - 2);
        let j = (x.floor() as usize).min(self.nlon - 2);
        let (fy, fx) = (y - i as f64, x - j as f64);
        let g = |a: usize, b: usize| self.z[a * self.nlon + b];
        let v = g(i, j) * (1.0 - fx) * (1.0 - fy)
            + g(i, j + 1) * fx * (1.0 - fy)
            + g(i + 1, j) * (1.0 - fx) * fy
            + g(i + 1, j + 1) * fx * fy;
        v.is_finite().then_some(v)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bilinear_plane() {
        // zeta = 47 + 0.5*dlat + 0.2*dlon -> bilinear must be exact
        let mut pts = vec![];
        for i in 0..3 {
            for j in 0..4 {
                let (la, lo) = (48.0 + i as f64 * 0.5, 12.0 + j as f64 * 0.5);
                pts.push((la, lo, 47.0 + 0.5 * (la - 48.0) + 0.2 * (lo - 12.0)));
            }
        }
        let g = Grid::from_points(&pts).unwrap();
        let z = g.interpolate(48.73, 13.1).unwrap();
        assert!((z - (47.0 + 0.5 * 0.73 + 0.2 * 1.1)).abs() < 1e-12);
        assert!(g.interpolate(50.0, 12.0).is_none());
    }
}
