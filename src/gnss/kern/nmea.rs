//! Minimal NMEA 0183 parser for GGA and GST (sufficient for RTK surveying).
//! Enable CFG-NMEA-HIGHPREC on the ZED-F9P, otherwise lat/lon are truncated to ~1 cm.
//!
//! Hardware: simpleRTK2B (u-blox ZED-F9P)
//! Autor:    Johann Gerner
//!
//! Übernommen aus gnss-vermessung 0.4.0 (src/nmea.rs), eigenständig gepflegt in Feldbuch-NT.

use anyhow::{Context, Result, bail};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FixQuality {
    Invalid,
    Autonomous,
    Dgnss,
    RtkFixed,
    RtkFloat,
    DeadReckoning,
    Other(u8),
}

impl FixQuality {
    fn from_u8(q: u8) -> Self {
        match q {
            0 => Self::Invalid,
            1 => Self::Autonomous,
            2 => Self::Dgnss,
            4 => Self::RtkFixed,
            5 => Self::RtkFloat,
            6 => Self::DeadReckoning,
            x => Self::Other(x),
        }
    }
    pub fn label(&self) -> &'static str {
        match self {
            Self::Invalid => "KEIN FIX",
            Self::Autonomous => "AUTONOM",
            Self::Dgnss => "DGNSS",
            Self::RtkFixed => "RTK FIX",
            Self::RtkFloat => "RTK FLOAT",
            Self::DeadReckoning => "DR",
            Self::Other(_) => "?",
        }
    }
}

#[derive(Debug, Clone)]
pub struct Gga {
    pub utc: String,
    pub lat: f64,
    pub lon: f64,
    pub quality: FixQuality,
    pub sats: u8,
    pub hdop: f64,
    /// height above geoid/MSL as reported by the receiver (NOT DHHN2016!)
    pub alt_msl: f64,
    /// geoid separation reported by the receiver (built-in global model)
    pub geoid_sep: f64,
    pub age_corr: Option<f64>,
    pub ref_station: Option<String>,
    /// the original sentence, used for upload to the VRS caster
    pub raw: String,
}

impl Gga {
    /// ellipsoidal height (GRS80) = MSL + geoid separation
    pub fn h_ell(&self) -> f64 {
        self.alt_msl + self.geoid_sep
    }
}

#[derive(Debug, Clone)]
pub struct Gst {
    pub utc: String,
    pub rms: f64,
    pub sigma_lat: f64,
    pub sigma_lon: f64,
    pub sigma_alt: f64,
}

#[derive(Debug, Clone)]
pub enum Sentence {
    Gga(Gga),
    Gst(Gst),
    Other,
}

pub fn checksum(body: &str) -> u8 {
    body.bytes().fold(0u8, |acc, b| acc ^ b)
}

fn verify(line: &str) -> Result<&str> {
    let line = line.trim();
    let s = line.strip_prefix('$').context("no '$'")?;
    let (body, cs) = s.rsplit_once('*').context("no checksum")?;
    let expected = u8::from_str_radix(cs, 16).context("bad checksum hex")?;
    if checksum(body) != expected {
        bail!("checksum mismatch");
    }
    Ok(body)
}

fn latlon(v: &str, hemi: &str, deg_digits: usize) -> Result<f64> {
    if v.len() < deg_digits + 2 {
        bail!("empty coordinate");
    }
    let deg: f64 = v[..deg_digits].parse()?;
    let min: f64 = v[deg_digits..].parse()?;
    let x = deg + min / 60.0;
    Ok(if hemi == "S" || hemi == "W" { -x } else { x })
}

fn f(v: &str) -> Result<f64> {
    Ok(v.parse::<f64>()?)
}

pub fn parse(line: &str) -> Result<Sentence> {
    let body = verify(line)?;
    let p: Vec<&str> = body.split(',').collect();
    let kind = p[0].get(2..).unwrap_or("");
    match kind {
        "GGA" if p.len() >= 15 => {
            let quality = FixQuality::from_u8(p[6].parse().unwrap_or(0));
            if quality == FixQuality::Invalid {
                bail!("no fix");
            }
            Ok(Sentence::Gga(Gga {
                utc: p[1].to_string(),
                lat: latlon(p[2], p[3], 2)?,
                lon: latlon(p[4], p[5], 3)?,
                quality,
                sats: p[7].parse().unwrap_or(0),
                hdop: p[8].parse().unwrap_or(f64::NAN),
                alt_msl: f(p[9])?,
                geoid_sep: f(p[11])?,
                age_corr: p[13].parse().ok(),
                ref_station: (!p[14].is_empty()).then(|| p[14].to_string()),
                raw: line.trim().to_string(),
            }))
        }
        "GST" if p.len() >= 9 => Ok(Sentence::Gst(Gst {
            utc: p[1].to_string(),
            rms: p[2].parse().unwrap_or(f64::NAN),
            sigma_lat: f(p[6])?,
            sigma_lon: f(p[7])?,
            sigma_alt: f(p[8])?,
        })),
        _ => Ok(Sentence::Other),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn with_cs(body: &str) -> String {
        format!("${}*{:02X}", body, checksum(body))
    }

    #[test]
    fn gga_highprec() {
        let s = with_cs(
            "GNGGA,101530.00,4848.1234567,N,01248.7654321,E,4,24,0.55,320.123,M,47.456,M,1.0,0000",
        );
        match parse(&s).unwrap() {
            Sentence::Gga(g) => {
                assert_eq!(g.quality, FixQuality::RtkFixed);
                assert!((g.lat - (48.0 + 48.1234567 / 60.0)).abs() < 1e-12);
                assert!((g.h_ell() - 367.579).abs() < 1e-9);
            }
            _ => panic!(),
        }
    }

    #[test]
    fn bad_checksum() {
        assert!(parse("$GNGGA,1,2,3*00").is_err());
    }
}
