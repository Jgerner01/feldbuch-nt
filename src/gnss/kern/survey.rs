//! Point measurement: epoch collection, quality filter, averaging, CSV export.
//!
//! Hardware: simpleRTK2B (u-blox ZED-F9P)
//! Autor:    Johann Gerner
//!
//! Übernommen aus gnss-vermessung 0.4.0 (src/survey.rs), eigenständig gepflegt in Feldbuch-NT.

use crate::gnss::kern::geo::{Geographic, geo_to_utm};
use crate::gnss::kern::height::HeightModel;
use crate::gnss::kern::nmea::{FixQuality, Gga, Gst};
use anyhow::Result;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::Path;

#[derive(Debug, Clone)]
pub struct Criteria {
    pub epochs: usize,
    pub max_sigma_2d: f64,
    pub max_sigma_h: f64,
    pub max_age: f64,
    /// pole height + ARP->L1 phase centre offset, subtracted from h
    pub antenna_height: f64,
    pub zone: u8,
}

#[derive(Debug, Clone)]
pub struct Epoch {
    pub east: f64,
    pub north: f64,
    pub h: f64,
    pub lat: f64,
    pub lon: f64,
}

#[derive(Debug, Clone)]
pub struct PointResult {
    pub pnr: String,
    pub east: f64,
    pub north: f64,
    pub h_ell: f64,
    pub h_dhhn: Option<f64>,
    pub sd_e: f64,
    pub sd_n: f64,
    pub sd_h: f64,
    pub n: usize,
    pub rejected: usize,
    pub timestamp: String,
}

/// Why an epoch was rejected (None = accepted)
pub fn check_epoch(gga: &Gga, gst: Option<&Gst>, c: &Criteria) -> Option<String> {
    if gga.quality != FixQuality::RtkFixed {
        return Some(format!("kein Fix ({})", gga.quality.label()));
    }
    if let Some(a) = gga.age_corr {
        if a > c.max_age {
            return Some(format!("Korrekturalter {a:.0} s"));
        }
    }
    // ohne zeitgleiche Genauigkeitsangabe wird die Epoche verworfen
    let Some(s) = gst.filter(|s| s.utc == gga.utc) else {
        return Some("keine Genauigkeitsangabe (GST)".to_string());
    };
    let s2d = s.sigma_lat.hypot(s.sigma_lon);
    if s2d > c.max_sigma_2d {
        return Some(format!("σ2D {s2d:.3} m"));
    }
    if s.sigma_alt > c.max_sigma_h {
        return Some(format!("σH {:.3} m", s.sigma_alt));
    }
    None
}

pub fn to_epoch(gga: &Gga, c: &Criteria) -> Epoch {
    let u = geo_to_utm(
        &Geographic {
            lat: gga.lat,
            lon: gga.lon,
            h: 0.0,
        },
        c.zone,
    );
    Epoch {
        east: u.east_prefixed(),
        north: u.north,
        h: gga.h_ell() - c.antenna_height,
        lat: gga.lat,
        lon: gga.lon,
    }
}

/// mean and empirical SD; values are reduced to the first one to keep precision
/// with 8-digit UTM eastings
fn mean_sd(v: impl Iterator<Item = f64> + Clone) -> (f64, f64) {
    let x0 = v.clone().next().unwrap_or(0.0);
    let v = v.map(move |x| x - x0);
    let n = v.clone().count() as f64;
    let m = v.clone().sum::<f64>() / n;
    let sd = if n > 1.0 {
        (v.map(|x| (x - m).powi(2)).sum::<f64>() / (n - 1.0)).sqrt()
    } else {
        f64::NAN
    };
    (x0 + m, sd)
}

pub fn evaluate(pnr: &str, ep: &[Epoch], rejected: usize, hm: &HeightModel) -> PointResult {
    let (e, sd_e) = mean_sd(ep.iter().map(|x| x.east));
    let (n, sd_n) = mean_sd(ep.iter().map(|x| x.north));
    let (h, sd_h) = mean_sd(ep.iter().map(|x| x.h));
    let lat = ep.iter().map(|x| x.lat).sum::<f64>() / ep.len() as f64;
    let lon = ep.iter().map(|x| x.lon).sum::<f64>() / ep.len() as f64;
    PointResult {
        pnr: pnr.to_string(),
        east: e,
        north: n,
        h_ell: h,
        h_dhhn: hm.zeta(lat, lon).map(|z| h - z),
        sd_e,
        sd_n,
        sd_h,
        n: ep.len(),
        rejected,
        timestamp: chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
    }
}

const HEADER: &str = "PNr;Ost;Nord;h_ell;H_DHHN2016;sE;sN;sH;Epochen;verworfen;Zeit";

pub fn append_csv(path: &Path, r: &PointResult) -> Result<()> {
    let new = !path.exists();
    let mut f = OpenOptions::new().create(true).append(true).open(path)?;
    if new {
        writeln!(f, "{HEADER}")?;
    }
    writeln!(
        f,
        "{};{:.3};{:.3};{:.3};{};{:.4};{:.4};{:.4};{};{};{}",
        r.pnr,
        r.east,
        r.north,
        r.h_ell,
        r.h_dhhn.map(|h| format!("{h:.3}")).unwrap_or_default(),
        r.sd_e,
        r.sd_n,
        r.sd_h,
        r.n,
        r.rejected,
        r.timestamp
    )?;
    Ok(())
}

/// increment trailing number of a point number: "A99" -> "A100", "17" -> "18"
pub fn next_pnr(p: &str) -> String {
    let split = p.trim_end_matches(|c: char| c.is_ascii_digit()).len();
    let (pre, num) = p.split_at(split);
    match num.parse::<u64>() {
        Ok(n) => format!("{pre}{:0width$}", n + 1, width = num.len()),
        Err(_) => format!("{p}1"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gnss::kern::nmea::{self, Sentence};

    /// Ohne GST mit gleicher Zeit darf keine Epoche angenommen werden.
    #[test]
    fn epoch_without_accuracy_is_rejected() {
        let body =
            "GNGGA,101530.00,4848.1234567,N,01248.7654321,E,4,24,0.55,320.123,M,47.456,M,1.0,0000";
        let line = format!("${body}*{:02X}", nmea::checksum(body));
        let Ok(Sentence::Gga(g)) = nmea::parse(&line) else {
            panic!()
        };
        let c = Criteria {
            epochs: 1,
            max_sigma_2d: 0.02,
            max_sigma_h: 0.04,
            max_age: 5.0,
            antenna_height: 0.0,
            zone: 32,
        };
        let mut gst = Gst {
            utc: "101529.00".into(),
            rms: 0.0,
            sigma_lat: 0.005,
            sigma_lon: 0.005,
            sigma_alt: 0.01,
        };
        assert!(check_epoch(&g, None, &c).is_some());
        assert!(check_epoch(&g, Some(&gst), &c).is_some());
        gst.utc = g.utc.clone();
        assert!(check_epoch(&g, Some(&gst), &c).is_none());
    }

    #[test]
    fn pnr_increment() {
        assert_eq!(next_pnr("17"), "18");
        assert_eq!(next_pnr("A099"), "A100");
        assert_eq!(next_pnr("GP"), "GP1");
    }

    #[test]
    fn stats() {
        let ep: Vec<Epoch> = [0.0, 0.01, -0.01]
            .iter()
            .map(|d| Epoch {
                east: 32_500_000.0 + d,
                north: 5_400_000.0,
                h: 400.0 + d,
                lat: 48.7,
                lon: 12.9,
            })
            .collect();
        let r = evaluate("1", &ep, 0, &HeightModel::Constant(47.0));
        assert!((r.east - 32_500_000.0).abs() < 1e-9);
        assert!((r.sd_e - 0.01).abs() < 1e-7); // input itself carries ~4e-9 m float error
        assert!((r.h_dhhn.unwrap() - 353.0).abs() < 1e-9);
    }
}
