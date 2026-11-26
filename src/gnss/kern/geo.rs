//! Geodetic computations: GRS80 ellipsoid, ETRS89 geographic <-> ETRS89/UTM (EPSG:25832).
//!
//! UTM uses the Krüger n-series (6th order), accuracy well below 0.1 mm
//! inside the zone and its usual overlap.
//!
//! Hardware: simpleRTK2B (u-blox ZED-F9P)
//! Autor:    Johann Gerner
//!
//! Übernommen aus gnss-vermessung 0.4.0 (src/geo.rs), eigenständig gepflegt in Feldbuch-NT.

use std::f64::consts::PI;

pub const GRS80_A: f64 = 6_378_137.0;
pub const GRS80_F: f64 = 1.0 / 298.257_222_101;
pub const UTM_K0: f64 = 0.9996;
pub const UTM_FALSE_EASTING: f64 = 500_000.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Geographic {
    /// latitude in degrees
    pub lat: f64,
    /// longitude in degrees
    pub lon: f64,
    /// ellipsoidal height in metres (GRS80)
    pub h: f64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Utm {
    pub zone: u8,
    /// easting without zone prefix (500 000 m at central meridian)
    pub east: f64,
    pub north: f64,
    /// scale factor at the point
    pub k: f64,
    /// meridian convergence in degrees
    pub gamma: f64,
}

impl Utm {
    /// Easting with the zone number prefixed, as used in the Bavarian cadastre (32xxxxxx.xxx).
    pub fn east_prefixed(&self) -> f64 {
        self.zone as f64 * 1_000_000.0 + self.east
    }
}

struct Kruger {
    n: f64,
    a_hat: f64,
    alpha: [f64; 6],
    beta: [f64; 6],
}

fn kruger() -> Kruger {
    let f = GRS80_F;
    let n = f / (2.0 - f);
    let n2 = n * n;
    let n3 = n2 * n;
    let n4 = n3 * n;
    let n5 = n4 * n;
    let n6 = n5 * n;
    let a_hat = GRS80_A / (1.0 + n) * (1.0 + n2 / 4.0 + n4 / 64.0 + n6 / 256.0);
    let alpha = [
        n / 2.0 - 2.0 / 3.0 * n2 + 5.0 / 16.0 * n3 + 41.0 / 180.0 * n4 - 127.0 / 288.0 * n5
            + 7891.0 / 37800.0 * n6,
        13.0 / 48.0 * n2 - 3.0 / 5.0 * n3 + 557.0 / 1440.0 * n4 + 281.0 / 630.0 * n5
            - 1983433.0 / 1935360.0 * n6,
        61.0 / 240.0 * n3 - 103.0 / 140.0 * n4 + 15061.0 / 26880.0 * n5 + 167603.0 / 181440.0 * n6,
        49561.0 / 161280.0 * n4 - 179.0 / 168.0 * n5 + 6601661.0 / 7257600.0 * n6,
        34729.0 / 80640.0 * n5 - 3418889.0 / 1995840.0 * n6,
        212378941.0 / 319334400.0 * n6,
    ];
    let beta = [
        n / 2.0 - 2.0 / 3.0 * n2 + 37.0 / 96.0 * n3 - 1.0 / 360.0 * n4 - 81.0 / 512.0 * n5
            + 96199.0 / 604800.0 * n6,
        1.0 / 48.0 * n2 + 1.0 / 15.0 * n3 - 437.0 / 1440.0 * n4 + 46.0 / 105.0 * n5
            - 1118711.0 / 3870720.0 * n6,
        17.0 / 480.0 * n3 - 37.0 / 840.0 * n4 - 209.0 / 4480.0 * n5 + 5569.0 / 90720.0 * n6,
        4397.0 / 161280.0 * n4 - 11.0 / 504.0 * n5 - 830251.0 / 7257600.0 * n6,
        4583.0 / 161280.0 * n5 - 108847.0 / 3991680.0 * n6,
        20648693.0 / 638668800.0 * n6,
    ];
    Kruger {
        n,
        a_hat,
        alpha,
        beta,
    }
}

pub fn central_meridian(zone: u8) -> f64 {
    zone as f64 * 6.0 - 183.0
}

/// ETRS89 geographic -> ETRS89/UTM in the given zone (32 for Bavaria).
pub fn geo_to_utm(p: &Geographic, zone: u8) -> Utm {
    let k = kruger();
    let e = (GRS80_F * (2.0 - GRS80_F)).sqrt();
    let phi = p.lat.to_radians();
    let lam = (p.lon - central_meridian(zone)).to_radians();

    let t = (phi.sin().atanh() - e * (e * phi.sin()).atanh()).sinh();
    let xi_p = t.atan2(lam.cos());
    let eta_p = (lam.sin() / (1.0 + t * t).sqrt()).atanh();

    let mut xi = xi_p;
    let mut eta = eta_p;
    let mut sigma = 1.0;
    let mut tau = 0.0;
    for (j, a) in k.alpha.iter().enumerate() {
        let j2 = 2.0 * (j as f64 + 1.0);
        xi += a * (j2 * xi_p).sin() * (j2 * eta_p).cosh();
        eta += a * (j2 * xi_p).cos() * (j2 * eta_p).sinh();
        sigma += j2 * a * (j2 * xi_p).cos() * (j2 * eta_p).cosh();
        tau += j2 * a * (j2 * xi_p).sin() * (j2 * eta_p).sinh();
    }

    let east = UTM_FALSE_EASTING + UTM_K0 * k.a_hat * eta;
    let north = UTM_K0 * k.a_hat * xi; // northern hemisphere only

    // scale and convergence
    let gamma = (tau * (1.0 + t * t).sqrt() + sigma * t * lam.tan())
        .atan2(sigma * (1.0 + t * t).sqrt() - tau * t * lam.tan());
    let scale = UTM_K0 * k.a_hat / GRS80_A
        * (1.0 + (1.0 - e * e) * phi.tan().powi(2)).sqrt()
        * ((sigma * sigma + tau * tau) / (t * t + lam.cos().powi(2))).sqrt();
    let _ = k.n;
    Utm {
        zone,
        east,
        north,
        k: scale,
        gamma: gamma * 180.0 / PI,
    }
}

/// ETRS89/UTM -> ETRS89 geographic (height is passed through).
pub fn utm_to_geo(u: &Utm, h: f64) -> Geographic {
    let k = kruger();
    let e = (GRS80_F * (2.0 - GRS80_F)).sqrt();
    let xi = u.north / (UTM_K0 * k.a_hat);
    let eta = (u.east - UTM_FALSE_EASTING) / (UTM_K0 * k.a_hat);

    let mut xi_p = xi;
    let mut eta_p = eta;
    for (j, b) in k.beta.iter().enumerate() {
        let j2 = 2.0 * (j as f64 + 1.0);
        xi_p -= b * (j2 * xi).sin() * (j2 * eta).cosh();
        eta_p -= b * (j2 * xi).cos() * (j2 * eta).sinh();
    }
    let tau_p = xi_p.sin() / (eta_p.sinh().powi(2) + xi_p.cos().powi(2)).sqrt();
    let lam = eta_p.sinh().atan2(xi_p.cos());

    // Newton iteration for tau = tan(phi)
    let mut tau = tau_p;
    for _ in 0..10 {
        let s = (e * (e * tau / (1.0 + tau * tau).sqrt()).atanh()).sinh();
        let tau_i_p = tau * (1.0 + s * s).sqrt() - s * (1.0 + tau * tau).sqrt();
        let d = (tau_p - tau_i_p) / (1.0 + tau_i_p * tau_i_p).sqrt()
            * (1.0 + (1.0 - e * e) * tau * tau)
            / ((1.0 - e * e) * (1.0 + tau * tau).sqrt());
        tau += d;
        if d.abs() < 1e-14 {
            break;
        }
    }
    Geographic {
        lat: tau.atan().to_degrees(),
        lon: central_meridian(u.zone) + lam.to_degrees(),
        h,
    }
}

/// Geographische Koordinaten (GRS80) in kartesische Erdkoordinaten [m].
pub fn geo_to_ecef(g: &Geographic) -> [f64; 3] {
    let e2 = GRS80_F * (2.0 - GRS80_F);
    let (lat, lon) = (g.lat.to_radians(), g.lon.to_radians());
    let n = GRS80_A / (1.0 - e2 * lat.sin().powi(2)).sqrt();
    [
        (n + g.h) * lat.cos() * lon.cos(),
        (n + g.h) * lat.cos() * lon.sin(),
        (n * (1.0 - e2) + g.h) * lat.sin(),
    ]
}

/// Kartesische Erdkoordinaten [m] in geographische Koordinaten (GRS80), iterativ.
pub fn ecef_to_geo(x: [f64; 3]) -> Geographic {
    const ITERATIONS: usize = 6;
    let e2 = GRS80_F * (2.0 - GRS80_F);
    let p = x[0].hypot(x[1]);
    let mut lat = x[2].atan2(p * (1.0 - e2));
    let mut h = 0.0;
    for _ in 0..ITERATIONS {
        let n = GRS80_A / (1.0 - e2 * lat.sin().powi(2)).sqrt();
        h = p / lat.cos() - n;
        lat = x[2].atan2(p * (1.0 - e2 * n / (n + h)));
    }
    Geographic {
        lat: lat.to_degrees(),
        lon: x[1].atan2(x[0]).to_degrees(),
        h,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ecef_roundtrip() {
        let g = Geographic {
            lat: 48.831_234_5,
            lon: 12.955_432_1,
            h: 368.912,
        };
        let back = ecef_to_geo(geo_to_ecef(&g));
        assert!((back.lat - g.lat).abs() < 1e-10 && (back.lon - g.lon).abs() < 1e-10);
        assert!((back.h - g.h).abs() < 1e-5);
    }

    // reference values from PROJ (EPSG:4258 -> EPSG:25832)
    const REF: [(f64, f64, f64, f64); 4] = [
        (12.8, 48.8, 779025.8011, 5412189.8892),
        (9.0, 48.0, 500000.0000, 5316300.2243),
        (10.5, 49.5, 608609.4446, 5484122.0063),
        (13.5, 47.5, 838880.2158, 5270550.9749),
    ];

    #[test]
    fn forward_matches_proj() {
        for (lon, lat, e, n) in REF {
            let u = geo_to_utm(&Geographic { lat, lon, h: 0.0 }, 32);
            assert!((u.east - e).abs() < 1e-3, "E {} vs {}", u.east, e);
            assert!((u.north - n).abs() < 1e-3, "N {} vs {}", u.north, n);
        }
    }

    #[test]
    fn roundtrip() {
        for (lon, lat, _, _) in REF {
            let u = geo_to_utm(&Geographic { lat, lon, h: 0.0 }, 32);
            let g = utm_to_geo(&u, 0.0);
            assert!((g.lat - lat).abs() < 1e-10 && (g.lon - lon).abs() < 1e-10);
        }
    }

    #[test]
    fn scale_on_central_meridian() {
        let u = geo_to_utm(
            &Geographic {
                lat: 48.0,
                lon: 9.0,
                h: 0.0,
            },
            32,
        );
        assert!((u.k - 0.9996).abs() < 1e-9 && u.gamma.abs() < 1e-12);
    }
}
