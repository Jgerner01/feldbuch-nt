//! Zweck:    RTCM-3.1-Transformationsnachrichten 1021 (Helmert), 1023 (Residuengitter,
//!           ellipsoidisch) und 1025 (Abbildung) dekodieren und daraus die Höhenanomalie
//!           für DHHN2016 bestimmen (SAPOS: GCG2016 im Gitter 1023).
//! Hardware: keine
//! Autor:    Johann Gerner
//!
//! Übernommen aus gnss-vermessung 0.4.0 (src/transform.rs), eigenständig gepflegt in Feldbuch-NT.
//!
//! Feldlayout nach RTCM 10403.3 (Feldbreiten gegen pyrtcm und Node-NTRIP/rtcm abgeglichen).
//! Rechenvorschrift (Jäger/Kälber, „The new RTCM 3.1 Transformation Messages“, GEOS 2008):
//! Höhenindikator 1 oder 2: N_i = mittleres ΔH + δh_i, H = h − N (bilinear interpoliert).
//!
//! Nicht eindeutig belegt ist die Lage des Gitterursprungs zu den 16 Punkten. Verwendet wird
//! die Lesart „Ursprung in der Gittermitte, Δ = Punktabstand, Nummerierung zeilenweise ab
//! Nordwest“ (der Dienst legt das Gitter mittig um die Roverposition). Alle anderen Lesarten
//! werden mitgerechnet; ihre größte Abweichung wird als Unsicherheit ausgewiesen.

use std::time::Instant;

const ARCSEC: f64 = 1.0 / 3600.0;
const GRID_POINTS: usize = 16;
const GRID_SIDE: usize = 4;
/// Versätze der Ellipsoidhalbachsen in DF166–DF169 [m]
const AXIS_A_OFFSET_M: f64 = 6_370_000.0;
const AXIS_B_OFFSET_M: f64 = 6_350_000.0;
/// Größte zulässige Abweichung zur Höhenanomalie des Empfängers (globales Modell) [m]
pub const PLAUSIBILITY_M: f64 = 2.0;
/// Größte zulässige Unsicherheit aus den Gitter-Lesarten [m]
pub const READING_LIMIT_M: f64 = 0.005;

/// Bitweises Lesen, höchstwertiges Bit zuerst.
struct Bits<'a> {
    data: &'a [u8],
    pos: usize,
}

impl Bits<'_> {
    fn u(&mut self, len: usize) -> Option<u64> {
        let mut v = 0;
        for i in self.pos..self.pos + len {
            v = (v << 1) | u64::from((*self.data.get(i / 8)? >> (7 - i % 8)) & 1);
        }
        self.pos += len;
        Some(v)
    }

    fn i(&mut self, len: usize) -> Option<i64> {
        let raw = self.u(len)? as i64;
        Some((raw << (64 - len)) >> (64 - len))
    }

    fn chars(&mut self, n: usize) -> Option<String> {
        (0..n).map(|_| self.u(8).map(|c| c as u8 as char)).collect()
    }
}

/// Nachricht 1021: Helmert-/Molodenski-Parameter.
#[derive(Debug, Clone, PartialEq)]
pub struct Helmert {
    pub source: String,
    pub target: String,
    pub system_id: u8,
    pub plate: u8,
    /// DF150
    pub computation: u8,
    /// DF151: 0 geometrische Höhen, 1 und 2 physikalische Höhen
    pub height_indicator: u8,
    /// Gültigkeitsbereich: Ursprung und Ausdehnung [°]
    pub validity: [f64; 4],
    pub dx: f64,
    pub dy: f64,
    pub dz: f64,
    /// Rohwerte der Drehungen und der Maßstabskorrektur
    pub rotation_raw: [i64; 3],
    pub scale_raw: i64,
    /// Halbachsen Quell- und Zielellipsoid [m]
    pub axes: [f64; 4],
    pub quality: [u8; 2],
}

impl Helmert {
    /// Keine Datumsänderung (ETRS89 → ETRS89): alle Parameter null.
    pub fn is_identity(&self) -> bool {
        self.dx == 0.0 && self.dy == 0.0 && self.dz == 0.0 && self.rotation_raw == [0; 3] && self.scale_raw == 0
    }
}

/// Nachricht 1023: Residuen im ellipsoidischen Gitter.
#[derive(Debug, Clone, PartialEq)]
pub struct ResidualGrid {
    pub system_id: u8,
    pub horizontal_shift: bool,
    pub vertical_shift: bool,
    /// Ursprung [°]
    pub lat0: f64,
    pub lon0: f64,
    /// Δφ, Δλ [°]
    pub dlat: f64,
    pub dlon: f64,
    /// mittlere Versätze: Breite, Länge ["], Höhe [m]
    pub mean_dlat: f64,
    pub mean_dlon: f64,
    pub mean_dh: f64,
    /// je Punkt: δφ, δλ ["], δh [m]
    pub residuals: [(f64, f64, f64); GRID_POINTS],
    pub interpolation: [u8; 2],
    pub quality: [u8; 2],
    pub mjd: u16,
}

/// Nachricht 1025: Abbildungsparameter.
#[derive(Debug, Clone, PartialEq)]
pub struct Projection {
    pub system_id: u8,
    pub kind: u8,
    pub lat_origin: f64,
    pub lon_origin: f64,
    pub scale_raw: u64,
    pub false_easting: f64,
    pub false_northing: f64,
}

/// Eine Lesart der Gittergeometrie.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Reading {
    /// Lage des Ursprungs: 0 Mitte, 1 Südwest, 2 Nordwest
    origin: u8,
    /// Δ ist der Punktabstand (sonst die Gesamtausdehnung)
    spacing: bool,
    /// Nummerierung beginnt im Norden (sonst im Süden)
    north_first: bool,
}

const PRIMARY: Reading = Reading { origin: 0, spacing: true, north_first: true };
/// Randbereich des Gitters (Anteil der Ausdehnung), in dem der Dienst ein neues Gitter liefert
const BORDER: f64 = 0.1;

impl ResidualGrid {
    /// Lage des Punkts im Gitter in Punktabständen (0 bis 3) nach einer Lesart.
    fn locate(&self, r: Reading, lat: f64, lon: f64) -> Option<(f64, f64)> {
        let steps = (GRID_SIDE - 1) as f64;
        let (step_lat, step_lon) = if r.spacing { (self.dlat, self.dlon) } else { (self.dlat / steps, self.dlon / steps) };
        if step_lat <= 0.0 || step_lon <= 0.0 {
            return None;
        }
        let (south, west) = match r.origin {
            0 => (self.lat0 - 1.5 * step_lat, self.lon0 - 1.5 * step_lon),
            1 => (self.lat0, self.lon0),
            _ => (self.lat0 - steps * step_lat, self.lon0),
        };
        let y = (lat - south) / step_lat;
        let x = (lon - west) / step_lon;
        ((0.0..=steps).contains(&y) && (0.0..=steps).contains(&x)).then_some((x, y))
    }

    fn zeta_with(&self, r: Reading, lat: f64, lon: f64) -> Option<f64> {
        let (x, y) = self.locate(r, lat, lon)?;
        let node = |row_from_south: usize, col: usize| {
            let row = if r.north_first { GRID_SIDE - 1 - row_from_south } else { row_from_south };
            self.mean_dh + self.residuals[row * GRID_SIDE + col].2
        };
        let (i, j) = ((y.floor() as usize).min(GRID_SIDE - 2), (x.floor() as usize).min(GRID_SIDE - 2));
        let (fy, fx) = (y - i as f64, x - j as f64);
        Some(
            node(i, j) * (1.0 - fx) * (1.0 - fy)
                + node(i, j + 1) * fx * (1.0 - fy)
                + node(i + 1, j) * (1.0 - fx) * fy
                + node(i + 1, j + 1) * fx * fy,
        )
    }

    /// Höhenanomalie N = mittleres ΔH + interpoliertes δh [m] nach der Hauptlesart.
    pub fn zeta(&self, lat: f64, lon: f64) -> Option<f64> {
        self.zeta_with(PRIMARY, lat, lon)
    }

    /// Größte Abweichung anderer Gitter-Lesarten [m]. Berücksichtigt werden nur Lesarten, die
    /// den Punkt außerhalb des Randbereichs zeigen: Der Dienst legt das Gitter mittig um den
    /// Rover und liefert am Rand ein neues.
    pub fn reading_spread(&self, lat: f64, lon: f64) -> Option<f64> {
        let base = self.zeta(lat, lon)?;
        let steps = (GRID_SIDE - 1) as f64;
        let inner = BORDER * steps..=(1.0 - BORDER) * steps;
        let mut spread: f64 = 0.0;
        for origin in 0..3 {
            for spacing in [true, false] {
                for north_first in [true, false] {
                    let r = Reading { origin, spacing, north_first };
                    let inside = self.locate(r, lat, lon).is_some_and(|(x, y)| inner.contains(&x) && inner.contains(&y));
                    if let Some(z) = self.zeta_with(r, lat, lon).filter(|_| inside) {
                        spread = spread.max((z - base).abs());
                    }
                }
            }
        }
        Some(spread)
    }

    /// Spannweite der Höhenresiduen [m].
    pub fn residual_range(&self) -> (f64, f64) {
        self.residuals.iter().map(|r| r.2).fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), v| (lo.min(v), hi.max(v)))
    }

    pub fn has_horizontal_residuals(&self) -> bool {
        self.mean_dlat != 0.0 || self.mean_dlon != 0.0 || self.residuals.iter().any(|r| r.0 != 0.0 || r.1 != 0.0)
    }
}

/// Zuletzt empfangene Transformationsnachrichten.
#[derive(Default, Clone)]
pub struct Transformation {
    pub helmert: Option<(Helmert, Instant)>,
    pub grid: Option<(ResidualGrid, Instant)>,
    pub projection: Option<(Projection, Instant)>,
}

/// Ergebnis der Prüfung, ob die Höhe aus der Transformation verwendet werden darf.
#[derive(Debug)]
pub struct HeightCheck {
    pub zeta: f64,
    pub spread: f64,
}

impl Transformation {
    /// Dekodiert 1021, 1023 oder 1025 (Nutzdaten ab der Nachrichtennummer) und liefert Klartext.
    pub fn apply(&mut self, nr: u16, payload: &[u8]) -> Option<String> {
        let mut b = Bits { data: payload, pos: 12 };
        let now = Instant::now();
        match nr {
            1021 => {
                let h = decode_1021(&mut b)?;
                let text = format!(
                    "{} → {}, System {}, Höhenindikator {}, Helmert {}",
                    h.source.trim(),
                    h.target.trim(),
                    h.system_id,
                    h.height_indicator,
                    if h.is_identity() { "ohne Datumsänderung" } else { "mit Parametern" }
                );
                self.helmert = Some((h, now));
                Some(text)
            }
            1023 => {
                let g = decode_1023(&mut b)?;
                let (lo, hi) = g.residual_range();
                let text = format!(
                    "Gitter um {:.4}° {:.4}°, Abstand {:.1}\"×{:.1}\", mittleres ΔH {:.3} m, δh {:+.3}…{:+.3} m",
                    g.lat0,
                    g.lon0,
                    g.dlat * 3600.0,
                    g.dlon * 3600.0,
                    g.mean_dh,
                    lo,
                    hi
                );
                self.grid = Some((g, now));
                Some(text)
            }
            1025 => {
                let p = decode_1025(&mut b)?;
                let text = format!(
                    "Abbildung Typ {}, Ursprung {:.6}° {:.6}°, FE {:.3} m, FN {:.3} m",
                    p.kind, p.lat_origin, p.lon_origin, p.false_easting, p.false_northing
                );
                self.projection = Some((p, now));
                Some(text)
            }
            _ => None,
        }
    }

    /// Höhenanomalie am Punkt mit allen Sicherheitsprüfungen; `geoid_sep` aus der GGA-Meldung.
    pub fn height(&self, lat: f64, lon: f64, geoid_sep: Option<f64>) -> Result<HeightCheck, String> {
        let (grid, _) = self.grid.as_ref().ok_or("Kein Residuengitter (1023) empfangen")?;
        let (helmert, _) = self.helmert.as_ref().ok_or("Keine Helmert-Parameter (1021) empfangen")?;
        if helmert.system_id != grid.system_id {
            return Err(format!("1021 (System {}) und 1023 (System {}) gehören nicht zusammen", helmert.system_id, grid.system_id));
        }
        if !matches!(helmert.height_indicator, 1 | 2) {
            return Err(format!("Höhenindikator {}: keine physikalischen Höhen", helmert.height_indicator));
        }
        if !helmert.is_identity() {
            return Err("1021 enthält eine Datumsänderung; diese wird nicht angewendet".into());
        }
        let zeta = grid.zeta(lat, lon).ok_or("Position außerhalb des Gitters (neues Gitter wird angefordert)")?;
        let spread = grid.reading_spread(lat, lon).unwrap_or(f64::INFINITY);
        if spread > READING_LIMIT_M {
            return Err(format!("Gitterauslegung nicht eindeutig: Lesarten weichen um {:.1} mm ab", spread * 1000.0));
        }
        if let Some(n) = geoid_sep {
            if (zeta - n).abs() > PLAUSIBILITY_M {
                return Err(format!("Höhenanomalie {zeta:.3} m weicht um mehr als {PLAUSIBILITY_M} m vom Empfängermodell ({n:.3} m) ab"));
            }
        }
        Ok(HeightCheck { zeta, spread })
    }
}

fn decode_1021(b: &mut Bits) -> Option<Helmert> {
    let n = b.u(5)? as usize;
    let source = b.chars(n)?;
    let n = b.u(5)? as usize;
    let target = b.chars(n)?;
    let system_id = b.u(8)? as u8;
    b.u(10)?; // DF148
    let plate = b.u(5)? as u8;
    let computation = b.u(4)? as u8;
    let height_indicator = b.u(2)? as u8;
    let validity = [
        b.i(19)? as f64 * 2.0 * ARCSEC,
        b.i(20)? as f64 * 2.0 * ARCSEC,
        b.u(14)? as f64 * 2.0 * ARCSEC,
        b.u(14)? as f64 * 2.0 * ARCSEC,
    ];
    let (dx, dy, dz) = (b.i(23)? as f64 * 0.001, b.i(23)? as f64 * 0.001, b.i(23)? as f64 * 0.001);
    let rotation_raw = [b.i(32)?, b.i(32)?, b.i(32)?];
    let scale_raw = b.i(25)?;
    let axes = [
        AXIS_A_OFFSET_M + b.u(24)? as f64 * 0.001,
        AXIS_B_OFFSET_M + b.u(25)? as f64 * 0.001,
        AXIS_A_OFFSET_M + b.u(24)? as f64 * 0.001,
        AXIS_B_OFFSET_M + b.u(25)? as f64 * 0.001,
    ];
    let quality = [b.u(3)? as u8, b.u(3)? as u8];
    Some(Helmert { source, target, system_id, plate, computation, height_indicator, validity, dx, dy, dz, rotation_raw, scale_raw, axes, quality })
}

fn decode_1023(b: &mut Bits) -> Option<ResidualGrid> {
    let system_id = b.u(8)? as u8;
    let horizontal_shift = b.u(1)? == 1;
    let vertical_shift = b.u(1)? == 1;
    let lat0 = b.i(21)? as f64 * 0.5 * ARCSEC;
    let lon0 = b.i(22)? as f64 * 0.5 * ARCSEC;
    let dlat = b.u(12)? as f64 * 0.5 * ARCSEC;
    let dlon = b.u(12)? as f64 * 0.5 * ARCSEC;
    let mean_dlat = b.i(8)? as f64 * 0.001;
    let mean_dlon = b.i(8)? as f64 * 0.001;
    let mean_dh = b.i(15)? as f64 * 0.01;
    let mut residuals = [(0.0, 0.0, 0.0); GRID_POINTS];
    for r in &mut residuals {
        *r = (b.i(9)? as f64 * 0.00003, b.i(9)? as f64 * 0.00003, b.i(9)? as f64 * 0.001);
    }
    let interpolation = [b.u(2)? as u8, b.u(2)? as u8];
    let quality = [b.u(3)? as u8, b.u(3)? as u8];
    let mjd = b.u(16)? as u16;
    Some(ResidualGrid { system_id, horizontal_shift, vertical_shift, lat0, lon0, dlat, dlon, mean_dlat, mean_dlon, mean_dh, residuals, interpolation, quality, mjd })
}

fn decode_1025(b: &mut Bits) -> Option<Projection> {
    const ANGLE: f64 = 0.000_000_011;
    Some(Projection {
        system_id: b.u(8)? as u8,
        kind: b.u(6)? as u8,
        lat_origin: b.i(34)? as f64 * ANGLE,
        lon_origin: b.i(35)? as f64 * ANGLE,
        scale_raw: b.u(30)?,
        false_easting: b.u(36)? as f64 * 0.001,
        false_northing: b.i(35)? as f64 * 0.001,
    })
}

/// Bekannte Werte des Höhenindikators (DF151).
pub fn height_indicator_text(v: u8) -> &'static str {
    match v {
        0 => "geometrische Höhen (keine Umrechnung)",
        1 => "physikalische Höhen über Helmert und Residuen",
        2 => "physikalische Höhen über Höhenanomalie im Gitter",
        _ => "reserviert",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build(fields: &[(u64, usize)]) -> Vec<u8> {
        let mut out = Vec::new();
        let mut pos = 0;
        for &(value, len) in fields {
            for i in (0..len).rev() {
                if pos % 8 == 0 {
                    out.push(0);
                }
                if let Some(b) = out.last_mut() {
                    *b |= (((value >> i) & 1) as u8) << (7 - pos % 8);
                }
                pos += 1;
            }
        }
        out
    }

    fn signed(v: i64, len: usize) -> (u64, usize) {
        ((v as u64) & ((1 << len) - 1), len)
    }

    fn msg_1021(height_indicator: u64, dx: i64) -> Vec<u8> {
        let mut f = vec![(1021, 12), (6, 5)];
        f.extend(b"ETRS89".iter().map(|&c| (u64::from(c), 8)));
        f.push((8, 5));
        f.extend(b"DHHN2016".iter().map(|&c| (u64::from(c), 8)));
        f.extend([(5, 8), (0, 10), (0, 5), (0, 4), (height_indicator, 2), signed(87_840, 19), signed(21_600, 20), (1000, 14), (1000, 14)]);
        f.extend([signed(dx, 23), signed(0, 23), signed(0, 23), signed(0, 32), signed(0, 32), signed(0, 32), signed(0, 25)]);
        f.extend([(8_137_000, 24), (6_752_314, 25), (8_137_000, 24), (6_752_314, 25), (1, 3), (1, 3)]);
        build(&f)
    }

    /// Gitter mittig um 48.8°/12.95°, Abstand 60" × 90", δh steigt nach Osten an.
    fn msg_1023(mean_dh_cm: i64) -> Vec<u8> {
        let mut f = vec![(1023, 12), (5, 8), (0, 1), (1, 1)];
        f.extend([signed(351_360, 21), signed(93_240, 22), (120, 12), (180, 12), signed(0, 8), signed(0, 8), signed(mean_dh_cm, 15)]);
        for i in 0..GRID_POINTS {
            let (row, col) = ((i / GRID_SIDE) as i64, (i % GRID_SIDE) as i64);
            f.extend([signed(0, 9), signed(0, 9), signed(10 * col - 15 + 4 * row - 6, 9)]);
        }
        f.extend([(0, 2), (0, 2), (1, 3), (1, 3), (61_000, 16)]);
        build(&f)
    }

    #[test]
    fn decodes_and_interpolates() {
        let mut t = Transformation::default();
        let text = t.apply(1021, &msg_1021(2, 0)).unwrap();
        assert!(text.contains("ETRS89 → DHHN2016") && text.contains("ohne Datumsänderung"), "{text}");
        let (h, _) = t.helmert.as_ref().unwrap();
        assert!((h.axes[0] - 6_378_137.0).abs() < 1e-6 && (h.axes[1] - 6_356_752.314).abs() < 1e-6);
        assert!((h.validity[0] - 48.8).abs() < 1e-9);
        t.apply(1023, &msg_1023(4721)).unwrap();
        let (g, _) = t.grid.as_ref().unwrap();
        assert!((g.lat0 - 48.8).abs() < 1e-9 && (g.lon0 - 12.95).abs() < 1e-9);
        assert!((g.dlat - 60.0 * ARCSEC).abs() < 1e-12);
        // Mitte: Mittel der Spalten 1 und 2 → δh = 0
        let c = t.height(48.8, 12.95, Some(47.0)).unwrap();
        assert!((c.zeta - 47.21).abs() < 1e-9 && c.spread < 1e-9);
        // halber Abstand nach Osten: δh = +0,005 m; Lesarten weichen ab
        let z = g.zeta(48.8, 12.95 + 45.0 * ARCSEC).unwrap();
        assert!((z - 47.215).abs() < 1e-9);
        // halber Abstand nach Norden: Zeilenfolge Nord/Süd unterscheidet sich um 4 mm
        assert!((g.reading_spread(48.8 + 30.0 * ARCSEC, 12.95).unwrap() - 0.004).abs() < 1e-9);
        assert!(g.zeta(48.9, 12.95).is_none());
    }

    #[test]
    fn safety_checks() {
        let mut t = Transformation::default();
        t.apply(1023, &msg_1023(4721)).unwrap();
        assert!(t.height(48.8, 12.95, None).is_err(), "ohne 1021");
        t.apply(1021, &msg_1021(0, 0)).unwrap();
        assert!(t.height(48.8, 12.95, None).unwrap_err().contains("Höhenindikator"));
        t.apply(1021, &msg_1021(2, 1500)).unwrap();
        assert!(t.height(48.8, 12.95, None).unwrap_err().contains("Datumsänderung"));
        t.apply(1021, &msg_1021(2, 0)).unwrap();
        assert!(t.height(48.8, 12.95, Some(44.0)).unwrap_err().contains("weicht"));
        // Vorzeichenfehler würde ebenfalls auffallen
        t.apply(1023, &msg_1023(-4721)).unwrap();
        assert!(t.height(48.8, 12.95, Some(47.0)).is_err());
    }
}
